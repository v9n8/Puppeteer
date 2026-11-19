//! Layer 3a: record every delegated piece of work and how it ended, from what
//! the app already receives (no model call). A row of `routing_decisions` never
//! holds a prompt or a sub-agent's output.
//!
//! Sub-agents: a [`Recorder`] per drain of a Claude sidecar (`runs::sidecar`)
//! sees every stream message. Reading a message is the pure [`parse`]; only
//! the `Agent` calls of the main thread are recorded, since the policy only
//! reaches the main agent. The `tool_use` carries the input the agent asked
//! for; a hook may have changed it since, so the model that really ran comes
//! from the result's `resolvedModel`. The sidecar's ceiling hook says when it
//! lowered one (`_puppeteer_routing_clamp`, stored as `clamp`).

use std::collections::HashMap;

use serde_json::Value;

use super::tier::{self, Effort, Model};
use crate::db::Db;

/// Longest description kept, in characters.
const DESCRIPTION_CHARS: usize = 80;

/// What a worker's last `Verification:` line says (QO-FR35).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Pass,
    Fail,
    None,
    /// No line, or one that does not say PASS / FAIL / NONE (a worker asked
    /// with the older free-form wording).
    Missing,
}

impl Verification {
    pub fn as_str(self) -> &'static str {
        match self {
            Verification::Pass => "pass",
            Verification::Fail => "fail",
            Verification::None => "none",
            Verification::Missing => "missing",
        }
    }
}

/// Read the outcome from a worker's final reply: the LAST line that starts with
/// `Verification:` (after leading spaces and markdown bold), then its first
/// word, any case.
pub fn parse_verification(text: &str) -> Verification {
    let Some(rest) = text.lines().rev().find_map(|line| {
        line.trim_start().trim_start_matches("**").strip_prefix("Verification:")
    }) else {
        return Verification::Missing;
    };
    // `**Verification:** PASS` closes the bold after the colon.
    let word: String = rest
        .trim_start_matches("**")
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match word.to_ascii_uppercase().as_str() {
        "PASS" => Verification::Pass,
        "FAIL" => Verification::Fail,
        "NONE" => Verification::None,
        _ => Verification::Missing,
    }
}

/// An `Agent` call of the main thread, as the agent asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    pub tool_use_id: String,
    pub agent_type: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub description: String,
}

/// What one stream message says about delegated work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Spawn(Spawn),
    /// The call's `tool_use_result`: `completed` closes it, `async_launched`
    /// (`status: None`) only names the model a background agent runs on.
    Result {
        tool_use_id: String,
        status: Option<String>,
        resolved_model: Option<String>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
    },
    /// The call's `tool_result` came back as an error.
    Failed { tool_use_id: String },
    /// `system/task_notification`: how a (background or not) agent ended.
    Notified { tool_use_id: String, status: String, total_tokens: Option<i64>, duration_ms: Option<i64> },
    /// A `result` with no `origin`: the user's turn ended with this reply (a
    /// background task's wake-up turn has an origin).
    TurnEnded { text: Option<String> },
    /// The sidecar's ceiling hook lowered the call's model: `"from→to"`.
    Clamped { tool_use_id: String, clamp: String },
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn main_thread(v: &Value) -> bool {
    v.get("parent_tool_use_id").is_none_or(Value::is_null)
}

fn blocks<'a>(v: &'a Value, kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    v.pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(move |b| b.get("type").and_then(Value::as_str) == Some(kind))
}

/// Read one stream message. Tool names: `Agent`, and `Task` for a CLI that
/// still calls it that (U12).
pub fn parse(v: &Value) -> Vec<Event> {
    match v.get("type").and_then(Value::as_str) {
        Some("assistant") if main_thread(v) => blocks(v, "tool_use")
            .filter(|b| matches!(b.get("name").and_then(Value::as_str), Some("Agent" | "Task")))
            .filter_map(|b| {
                let input = b.get("input")?;
                Some(Event::Spawn(Spawn {
                    tool_use_id: text(b, "id")?,
                    agent_type: text(input, "subagent_type").unwrap_or_else(|| "general-purpose".to_string()),
                    model: text(input, "model"),
                    effort: text(input, "effort"),
                    description: text(input, "description").unwrap_or_default(),
                }))
            })
            .collect(),
        Some("user") if main_thread(v) => {
            let mut events = Vec::new();
            for block in blocks(v, "tool_result") {
                let Some(id) = text(block, "tool_use_id") else { continue };
                if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                    events.push(Event::Failed { tool_use_id: id });
                    continue;
                }
                let Some(result) = v.get("tool_use_result").filter(|r| r.is_object()) else { continue };
                let status = text(result, "status");
                if !matches!(status.as_deref(), Some("completed" | "async_launched")) {
                    continue;
                }
                events.push(Event::Result {
                    tool_use_id: id,
                    status: status.filter(|s| s == "completed"),
                    resolved_model: text(result, "resolvedModel"),
                    total_tokens: result.get("totalTokens").and_then(Value::as_i64),
                    duration_ms: result.get("totalDurationMs").and_then(Value::as_i64),
                });
            }
            events
        }
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("task_notification") => {
            let (Some(id), Some(status)) = (text(v, "tool_use_id"), text(v, "status")) else { return Vec::new() };
            vec![Event::Notified {
                tool_use_id: id,
                status,
                total_tokens: v.pointer("/usage/total_tokens").and_then(Value::as_i64),
                duration_ms: v.pointer("/usage/duration_ms").and_then(Value::as_i64),
            }]
        }
        Some("result") if v.get("origin").is_none_or(Value::is_null) => vec![Event::TurnEnded { text: text(v, "result") }],
        Some("_puppeteer_routing_clamp") => {
            let (Some(id), Some(from), Some(to)) = (text(v, "tool_use_id"), text(v, "from"), text(v, "to")) else { return Vec::new() };
            vec![Event::Clamped { tool_use_id: id, clamp: format!("{from}→{to}") }]
        }
        _ => Vec::new(),
    }
}

/// A notification's status as a row's: anything but completed / failed
/// (killed, stopped…) did not run to an end.
fn closed_status(status: &str) -> &'static str {
    match status {
        "completed" => "completed",
        "failed" => "failed",
        _ => "stopped",
    }
}

/// The same piece of work, asked again: case and spacing do not count.
fn work_key(description: &str) -> String {
    description.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn requested_tier(model: Option<&str>, effort: Option<&str>) -> (Option<Model>, Option<Effort>) {
    (model.and_then(Model::family), effort.and_then(Effort::parse))
}

/// The run a drain belongs to, read once on the first event that needs it.
struct RunMeta {
    project_id: Option<String>,
    policy_version: Option<i64>,
    /// Settings › Sub-agent effort gave the sidecar a level (`runs::subagent_env`).
    env_effort: bool,
    is_worker: bool,
}

/// Records one sidecar drain's `Agent` calls. A drain is one process, so "the
/// same turn" (QO-FR33) is the same recorder. Writes never fail the drain:
/// they are logged and skipped.
pub struct Recorder {
    db: Db,
    run_id: String,
    claude: bool,
    meta: Option<RunMeta>,
    /// tool_use_id → row id, for every call recorded by this drain.
    rows: HashMap<String, String>,
    /// tool_use_ids whose row is still open.
    open: Vec<String>,
    /// [`work_key`] → (row id, tier asked for) of the latest call.
    work: HashMap<String, (String, (Option<Model>, Option<Effort>))>,
    /// tool_use_id → clamp reported before the call's `tool_use` was read: the
    /// SDK runs the hook as soon as the CLI asks, maybe ahead of the stream.
    clamps: HashMap<String, String>,
    /// This worker's last turn ended `Verification: FAIL`.
    failed: bool,
}

impl Recorder {
    pub fn new(db: &Db, run_id: &str, engine: &str) -> Self {
        Recorder {
            db: db.clone(),
            run_id: run_id.to_string(),
            claude: engine == "claude",
            meta: None,
            rows: HashMap::new(),
            open: Vec::new(),
            work: HashMap::new(),
            clamps: HashMap::new(),
            failed: false,
        }
    }

    /// Take one stream message (`None`: not JSON).
    pub async fn observe(&mut self, message: Option<&Value>) {
        let Some(v) = message.filter(|_| self.claude) else { return };
        for event in parse(v) {
            if let Err(e) = self.apply(event).await {
                tracing::warn!(event = "routing_record_failed", run_id = %self.run_id, error = %e);
            }
        }
    }

    /// The drain ended: a call still open never reported back (the user
    /// stopped the turn, or the app quit) and ends `stopped` (U13).
    pub async fn close(&mut self) {
        let now = chrono::Utc::now().to_rfc3339();
        for tool_use_id in std::mem::take(&mut self.open) {
            let Some(id) = self.rows.get(&tool_use_id) else { continue };
            let _ = sqlx::query("UPDATE routing_decisions SET status = 'stopped', finished_at = ? WHERE id = ? AND status IS NULL")
                .bind(&now)
                .bind(id)
                .execute(&self.db)
                .await
                .map_err(|e| tracing::warn!(event = "routing_record_failed", run_id = %self.run_id, error = %e));
        }
    }

    /// Called before the drain writes the run's final status: a worker that
    /// failed its check may be retried (`escalate`), so its conductor's tools
    /// keep seeing it running until the wake consumer decides.
    pub fn hold_if_failed(&self, app: &tauri::AppHandle) {
        use tauri::Manager;
        if let Some(cst) = app.try_state::<crate::conductor::ConductorState>().filter(|_| self.failed) {
            cst.hold_for_retry(&self.run_id);
        }
    }

    async fn meta(&mut self) -> &RunMeta {
        if self.meta.is_none() {
            let (project_id, policy_version, role): (Option<String>, Option<i64>, Option<String>) =
                sqlx::query_as("SELECT project_id, routing_policy_version, role FROM runs WHERE id = ?")
                    .bind(&self.run_id)
                    .fetch_optional(&self.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
            let env_effort = match &project_id {
                Some(project_id) => crate::runs::subagent_env::load(&self.db, project_id).await.effort.is_some(),
                None => false,
            };
            let is_worker = role.as_deref() == Some("worker");
            self.meta = Some(RunMeta { project_id, policy_version, env_effort, is_worker });
        }
        self.meta.as_ref().expect("set above")
    }

    /// The open row of a call this drain recorded; closing it once.
    fn take_open(&mut self, tool_use_id: &str) -> Option<String> {
        let id = self.rows.get(tool_use_id)?.clone();
        self.open.retain(|open| open != tool_use_id);
        Some(id)
    }

    async fn apply(&mut self, event: Event) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        match event {
            Event::Spawn(spawn) => self.spawned(spawn, &now).await?,
            Event::Clamped { tool_use_id, clamp } => {
                let Some(id) = self.rows.get(&tool_use_id) else {
                    self.clamps.insert(tool_use_id, clamp);
                    return Ok(());
                };
                sqlx::query("UPDATE routing_decisions SET clamp = ? WHERE id = ?")
                    .bind(clamp)
                    .bind(id)
                    .execute(&self.db)
                    .await?;
            }
            // A worker's reply says how its checks went (QO-FR35).
            Event::TurnEnded { text } => {
                if self.meta().await.is_worker {
                    let verification = parse_verification(text.as_deref().unwrap_or_default());
                    self.failed = verification == Verification::Fail;
                    super::worker::verified(&self.db, &self.run_id, verification).await;
                }
            }
            Event::Result { tool_use_id, status: None, resolved_model, .. } => {
                let Some(id) = self.rows.get(&tool_use_id) else { return Ok(()) };
                sqlx::query("UPDATE routing_decisions SET resolved_model = COALESCE(?, resolved_model) WHERE id = ?")
                    .bind(resolved_model)
                    .bind(id)
                    .execute(&self.db)
                    .await?;
            }
            Event::Result { tool_use_id, status: Some(status), resolved_model, total_tokens, duration_ms } => {
                let Some(id) = self.take_open(&tool_use_id) else { return Ok(()) };
                sqlx::query(
                    "UPDATE routing_decisions SET status = ?, resolved_model = COALESCE(?, resolved_model), \
                     total_tokens = COALESCE(?, total_tokens), duration_ms = COALESCE(?, duration_ms), \
                     finished_at = COALESCE(finished_at, ?) WHERE id = ?",
                )
                .bind(closed_status(&status))
                .bind(resolved_model)
                .bind(total_tokens)
                .bind(duration_ms)
                .bind(&now)
                .bind(id)
                .execute(&self.db)
                .await?;
            }
            Event::Failed { tool_use_id } => {
                let Some(id) = self.take_open(&tool_use_id) else { return Ok(()) };
                sqlx::query("UPDATE routing_decisions SET status = 'failed', finished_at = ? WHERE id = ? AND status IS NULL")
                    .bind(&now)
                    .bind(id)
                    .execute(&self.db)
                    .await?;
            }
            Event::Notified { tool_use_id, status, total_tokens, duration_ms } => {
                let Some(id) = self.take_open(&tool_use_id) else { return Ok(()) };
                sqlx::query(
                    "UPDATE routing_decisions SET status = ?, total_tokens = COALESCE(?, total_tokens), \
                     duration_ms = COALESCE(?, duration_ms), finished_at = ? WHERE id = ? AND status IS NULL",
                )
                .bind(closed_status(&status))
                .bind(total_tokens)
                .bind(duration_ms)
                .bind(&now)
                .bind(id)
                .execute(&self.db)
                .await?;
            }
        }
        Ok(())
    }

    /// Insert the call's row. The same work asked again one tier up within the
    /// drain is an escalation: the earlier call counts as failed (QO-FR33).
    async fn spawned(&mut self, spawn: Spawn, now: &str) -> Result<(), sqlx::Error> {
        if self.rows.contains_key(&spawn.tool_use_id) {
            return Ok(());
        }
        let tier = requested_tier(spawn.model.as_deref(), spawn.effort.as_deref());
        let key = work_key(&spawn.description);
        let escalated_from = self
            .work
            .get(&key)
            .filter(|(_, earlier)| tier::is_higher(tier, *earlier))
            .map(|(row, _)| row.clone());
        let decided_by = match (&escalated_from, spawn.model.is_some() || spawn.effort.is_some()) {
            (Some(_), _) => "escalation",
            (None, true) => "agent",
            (None, false) => "default",
        };
        let (project_id, policy_version, env_effort) = {
            let meta = self.meta().await;
            (meta.project_id.clone(), meta.policy_version, meta.env_effort)
        };
        let effort_source = match (&spawn.effort, env_effort) {
            (Some(_), _) => "param",
            (None, true) => "env",
            (None, false) => "unknown",
        };
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO routing_decisions (id, run_id, project_id, created_at, kind, decided_by, agent_type, task_kind, \
             requested_model, requested_effort, effort_source, policy_version, description, tool_use_id, clamp) \
             VALUES (?, ?, ?, ?, 'subagent', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&self.run_id)
        .bind(project_id)
        .bind(now)
        .bind(decided_by)
        .bind(&spawn.agent_type)
        .bind(tier::read_kind(&spawn.description))
        .bind(&spawn.model)
        .bind(&spawn.effort)
        .bind(effort_source)
        .bind(policy_version)
        .bind(spawn.description.chars().take(DESCRIPTION_CHARS).collect::<String>())
        .bind(&spawn.tool_use_id)
        .bind(self.clamps.remove(&spawn.tool_use_id))
        .execute(&self.db)
        .await?;
        if let Some(earlier) = escalated_from {
            sqlx::query("UPDATE routing_decisions SET escalated = 1 WHERE id = ?")
                .bind(earlier)
                .execute(&self.db)
                .await?;
        }
        self.work.insert(key, (id.clone(), tier));
        self.rows.insert(spawn.tool_use_id.clone(), id);
        self.open.push(spawn.tool_use_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
