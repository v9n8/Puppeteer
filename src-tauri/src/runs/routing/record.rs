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
mod tests {
    use super::*;
    use Verification as V;

    #[test]
    fn the_last_verification_line_decides() {
        for (text, want) in [
            ("Done.\n\nVerification: PASS — cargo test green", V::Pass),
            ("Done.\nVerification: FAIL — 2 tests red", V::Fail),
            ("Docs only.\nVerification: NONE — no code changed", V::None),
            ("Verification: pass - lint ok", V::Pass),
            ("  Verification:FAIL—build broke", V::Fail),
            ("**Verification:** PASS — all green", V::Pass),
            ("**Verification: FAIL** — 1 red", V::Fail),
            // An earlier line is overruled by the last one.
            ("Verification: FAIL — red\nfixed it\nVerification: PASS — green", V::Pass),
            // Text after the line does not hide it.
            ("Verification: NONE — docs\n\nThanks!", V::None),
        ] {
            assert_eq!(parse_verification(text), want, "{text}");
        }
    }

    #[test]
    fn no_line_or_the_old_free_form_wording_is_missing() {
        for text in [
            "",
            "All done, tests pass.",
            "Verification: ran cargo test, all green",
            "Verification: PASSED",
            "- Verification: PASS",
            "verification: PASS",
        ] {
            assert_eq!(parse_verification(text), V::Missing, "{text}");
        }
        assert_eq!(V::Missing.as_str(), "missing");
    }

    /// QO-AC48: a new worker is asked for the fixed form; one made before
    /// resumes with the wording stored on its row, not the new one.
    #[tokio::test]
    async fn a_worker_made_before_keeps_its_stored_wording() {
        let new = crate::conductor::worker_instruction(Some("You review."));
        assert!(new.ends_with(crate::conductor::WORKER_VERIFICATION));
        assert!(new.contains("`Verification: PASS — <what you ran>`"));

        let db = crate::commands::response_styles::tests::test_db().await;
        let old = "You review.\n\nBefore you finish a turn in which you changed files, run the project's tests or \
checks (test, lint, build) for the files you changed. End your reply with a `Verification:` line saying what you \
ran and the result. If no check applies, say why on that line.";
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, system_instruction, created_at) \
             VALUES ('w', 'p1', 'session', 'claude', 'worker', 'c', ?, 'c')",
        )
        .bind(old)
        .execute(&db)
        .await
        .unwrap();
        let resumed = crate::conductor::commands::resume_framing(&db, None, "w").await.unwrap();
        assert_eq!(resumed.framing.as_deref(), Some(old));
    }
    // ---- the stream recorder ----------------------------------------------

    const RUN4: &str = include_str!("fixtures/run4.jsonl");
    const RUN5: &str = include_str!("fixtures/run5.jsonl");

    #[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
    struct Row {
        kind: String,
        decided_by: String,
        agent_type: Option<String>,
        task_kind: String,
        requested_model: Option<String>,
        resolved_model: Option<String>,
        requested_effort: Option<String>,
        effort_source: String,
        policy_version: Option<i64>,
        description: Option<String>,
        status: Option<String>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
        escalated: i64,
    }

    async fn recorder_db() -> Db {
        let db = crate::commands::response_styles::tests::test_db().await;
        sqlx::query("INSERT INTO runs (id, project_id, type, engine, created_at, routing_policy_version) VALUES ('r1', 'p1', 'session', 'claude', 'c', 1)")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    async fn feed(recorder: &mut Recorder, lines: &str) {
        for line in lines.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            recorder.observe(Some(&v)).await;
        }
    }

    async fn rows(db: &Db) -> Vec<Row> {
        sqlx::query_as(
            "SELECT kind, decided_by, agent_type, task_kind, requested_model, resolved_model, requested_effort, effort_source, \
             policy_version, description, status, total_tokens, duration_ms, escalated FROM routing_decisions ORDER BY created_at, rowid",
        )
        .fetch_all(db)
        .await
        .unwrap()
    }

    fn agent_call(id: &str, input: Value) -> Value {
        serde_json::json!({
            "type": "assistant",
            "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Agent", "input": input}]},
            "parent_tool_use_id": null
        })
    }

    /// QO-AC42: the agent's own pick, read from a real stream (spike run 5).
    #[tokio::test]
    async fn a_call_with_model_and_effort_is_the_agents_decision() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        feed(&mut recorder, RUN5).await;
        let rows = rows(&db).await;
        assert_eq!(rows.len(), 3, "the sub-agents' own Bash / Read calls are not recorded");
        assert_eq!(
            rows[0],
            Row {
                kind: "subagent".into(),
                decided_by: "agent".into(),
                agent_type: Some("Explore".into()),
                task_kind: "search".into(),
                requested_model: Some("sonnet".into()),
                resolved_model: Some("claude-sonnet-5-5".into()),
                requested_effort: Some("low".into()),
                effort_source: "param".into(),
                policy_version: Some(1),
                description: Some("[search] find parseConfig definition".into()),
                status: Some("completed".into()),
                total_tokens: rows[0].total_tokens,
                duration_ms: rows[0].duration_ms,
                escalated: 0,
            }
        );
        let kinds: Vec<_> = rows.iter().map(|r| (r.task_kind.as_str(), r.requested_effort.as_deref())).collect();
        assert_eq!(kinds, vec![("search", Some("low")), ("docs", Some("medium")), ("review", Some("medium"))]);
        // Background agents: closed by their notification, with its usage.
        assert!(rows.iter().all(|r| r.status.as_deref() == Some("completed") && r.total_tokens.is_some_and(|t| t > 0)));
    }

    /// QO-AC43: no model, no effort → the default; the effort's source is the
    /// env while Settings › Sub-agent effort gives one, else unknown.
    #[tokio::test]
    async fn a_call_with_neither_is_the_default() {
        let db = recorder_db().await;
        let call = agent_call("t1", serde_json::json!({"description": "look around", "prompt": "p"}));
        let mut recorder = Recorder::new(&db, "r1", "claude");
        recorder.observe(Some(&call)).await;
        crate::commands::settings::set_setting(&db, crate::runs::subagent_env::SETTING_EFFORT, "inherit").await.unwrap();
        let mut recorder = Recorder::new(&db, "r1", "claude");
        recorder.observe(Some(&agent_call("t2", serde_json::json!({"description": "look again", "prompt": "p"})))).await;
        let rows = rows(&db).await;
        let got: Vec<_> = rows.iter().map(|r| (r.decided_by.as_str(), r.effort_source.as_str(), r.task_kind.as_str())).collect();
        assert_eq!(got, vec![("default", "env", "other"), ("default", "unknown", "other")]);
        assert_eq!(rows[0].agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(rows[0].status, None, "still open");
    }

    /// QO-AC44 (a call in the foreground, spike run 4) and the model a hook
    /// put in place of the one asked for.
    #[tokio::test]
    async fn a_completed_result_closes_the_call_with_the_model_that_ran() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        feed(&mut recorder, RUN4).await;
        let run4 = rows(&db).await;
        let a = &run4[0];
        assert_eq!((a.description.as_deref(), a.decided_by.as_str()), (Some("Spike A"), "agent"));
        assert_eq!((a.requested_model.as_deref(), a.resolved_model.as_deref()), (Some("sonnet"), Some("claude-haiku-5-5")));
        assert_eq!((a.status.as_deref(), a.total_tokens, a.duration_ms), (Some("completed"), Some(44666), Some(1035)));
        let b = &run4[1];
        assert_eq!((b.resolved_model.as_deref(), b.status.as_deref(), b.total_tokens), (Some("claude-haiku-5-5"), Some("completed"), Some(42874)));

        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        recorder.observe(Some(&agent_call("t1", serde_json::json!({"description": "[test] run it", "model": "sonnet"})))).await;
        let result = serde_json::json!({
            "type": "user", "parent_tool_use_id": null,
            "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]},
            "tool_use_result": {"status": "completed", "resolvedModel": "claude-sonnet-5-5", "totalTokens": 12000, "totalDurationMs": 900}
        });
        recorder.observe(Some(&result)).await;
        let row = &rows(&db).await[0];
        assert_eq!((row.status.as_deref(), row.resolved_model.as_deref(), row.total_tokens), (Some("completed"), Some("claude-sonnet-5-5"), Some(12000)));
    }

    /// QO-AC45: a background agent's notification says how it ended.
    #[tokio::test]
    async fn a_notification_closes_a_background_call_once() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        recorder.observe(Some(&agent_call("bg", serde_json::json!({"description": "[implement] x", "run_in_background": true})))).await;
        let note = |status: &str, tokens: i64| {
            serde_json::json!({"type": "system", "subtype": "task_notification", "tool_use_id": "bg", "status": status,
                "usage": {"total_tokens": tokens, "duration_ms": 50}})
        };
        recorder.observe(Some(&note("failed", 777))).await;
        recorder.observe(Some(&note("completed", 999))).await;
        let row = &rows(&db).await[0];
        assert_eq!((row.status.as_deref(), row.total_tokens, row.duration_ms), (Some("failed"), Some(777), Some(50)));
    }

    #[tokio::test]
    async fn an_error_result_fails_the_call_and_the_rest_stop_at_the_end() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        recorder.observe(Some(&agent_call("e", serde_json::json!({"description": "a"})))).await;
        recorder.observe(Some(&agent_call("left", serde_json::json!({"description": "b"})))).await;
        let error = serde_json::json!({
            "type": "user", "parent_tool_use_id": null,
            "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "e", "is_error": true, "content": "no"}]}
        });
        recorder.observe(Some(&error)).await;
        recorder.close().await;
        let got: Vec<_> = rows(&db).await.into_iter().map(|r| r.status).collect();
        assert_eq!(got, vec![Some("failed".into()), Some("stopped".into())]);
    }

    /// QO-AC46: the same work asked again one tier up is an escalation.
    #[tokio::test]
    async fn the_same_work_one_tier_up_escalates_the_earlier_call() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        let call = |id: &str, description: &str, model: &str, effort: &str| {
            agent_call(id, serde_json::json!({"description": description, "model": model, "effort": effort}))
        };
        recorder.observe(Some(&call("a", "[review] Check  auth diff", "sonnet", "medium"))).await;
        // Not higher: a plain second call.
        recorder.observe(Some(&call("b", "[review] check auth diff", "sonnet", "low"))).await;
        recorder.observe(Some(&call("c", "[REVIEW] check auth   diff ", "opus", "low"))).await;
        let got: Vec<_> = rows(&db).await.into_iter().map(|r| (r.decided_by, r.escalated)).collect();
        assert_eq!(got, vec![("agent".into(), 0), ("agent".into(), 1), ("escalation".into(), 0)]);
        // Another drain is another turn: not an escalation.
        let mut next = Recorder::new(&db, "r1", "claude");
        next.observe(Some(&call("d", "[review] check auth diff", "opus", "high"))).await;
        assert_eq!(rows(&db).await[3].decided_by, "agent");
    }

    /// The ceiling hook's report, read before or after the call it lowered.
    #[tokio::test]
    async fn a_clamp_is_stored_on_its_call_in_either_order() {
        let db = recorder_db().await;
        let mut recorder = Recorder::new(&db, "r1", "claude");
        let clamp = |id: &str| serde_json::json!({"type": "_puppeteer_routing_clamp", "tool_use_id": id, "from": "opus", "to": "sonnet"});
        recorder.observe(Some(&clamp("early"))).await;
        recorder.observe(Some(&agent_call("early", serde_json::json!({"description": "[review] a", "model": "opus"})))).await;
        recorder.observe(Some(&agent_call("late", serde_json::json!({"description": "[review] b", "model": "opus"})))).await;
        recorder.observe(Some(&agent_call("none", serde_json::json!({"description": "[review] c", "model": "haiku"})))).await;
        recorder.observe(Some(&clamp("late"))).await;
        // Half a report, or one for a call never recorded (a nested agent's).
        recorder.observe(Some(&serde_json::json!({"type": "_puppeteer_routing_clamp", "tool_use_id": "none", "from": "opus"}))).await;
        recorder.observe(Some(&clamp("nested"))).await;
        let got: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT tool_use_id, clamp FROM routing_decisions ORDER BY rowid").fetch_all(&db).await.unwrap();
        let want = [("early", Some("opus→sonnet")), ("late", Some("opus→sonnet")), ("none", None)];
        assert_eq!(got, want.map(|(id, c)| (id.to_string(), c.map(str::to_string))));
        assert_eq!(rows(&db).await[0].requested_model.as_deref(), Some("opus"), "the model asked for stays");
    }

    /// QO-AC50: no column can hold a prompt, and none of it is stored.
    #[tokio::test]
    async fn no_prompt_is_stored() {
        let db = recorder_db().await;
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('routing_decisions')").fetch_all(&db).await.unwrap();
        assert!(!columns.is_empty() && columns.iter().all(|c| !c.contains("prompt")), "{columns:?}");
        let mut recorder = Recorder::new(&db, "r1", "claude");
        let long = "x".repeat(200);
        recorder.observe(Some(&agent_call("t", serde_json::json!({"description": long, "prompt": "SECRET PROMPT"})))).await;
        let dump: String = sqlx::query_scalar("SELECT json_group_array(json_array(description, agent_type, requested_model)) FROM routing_decisions")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!dump.contains("SECRET"));
        assert_eq!(rows(&db).await[0].description.as_deref().map(|d| d.chars().count()), Some(80));
    }

    #[tokio::test]
    async fn codex_and_nested_calls_are_not_recorded() {
        let db = recorder_db().await;
        let mut codex = Recorder::new(&db, "r1", "codex");
        feed(&mut codex, RUN5).await;
        let mut nested = Recorder::new(&db, "r1", "claude");
        let mut call = agent_call("n", serde_json::json!({"description": "inner"}));
        call["parent_tool_use_id"] = Value::String("toolu_parent".into());
        nested.observe(Some(&call)).await;
        nested.observe(None).await;
        assert!(rows(&db).await.is_empty());
        let task = serde_json::json!({"type": "assistant", "parent_tool_use_id": null,
            "message": {"content": [{"type": "tool_use", "id": "old", "name": "Task", "input": {"description": "[plan] x"}}]}});
        assert_eq!(parse(&task).len(), 1, "the older tool name too");
    }

    /// The stream only passes through the drain, so its source is checked: one
    /// recorder per drain, fed every stdout line, closed when the loop ends,
    /// before the run's status is written (and the conductor told).
    #[test]
    fn the_drain_feeds_the_recorder() {
        let src = include_str!("../sidecar.rs").replace("\r\n", "\n");
        let drain = &src[src.find("pub async fn drain_sidecar(").unwrap()..src.find("async fn drain_trailing_lines<R>(").unwrap()];
        let new = drain.find("let mut routing = crate::runs::routing::record::Recorder::new(&db_pool, &run_id, &engine);").unwrap();
        let observe = drain.find("routing.observe(parsed.as_ref()).await;").unwrap();
        let close = drain.find("routing.close().await;").unwrap();
        let status = drain.find("UPDATE runs SET status = ?").unwrap();
        assert!(new < observe && observe < close && close < status);
        assert!(drain[..observe].ends_with("let parsed = serde_json::from_str::<Value>(&l).ok();\n                        "));
        assert_eq!(src.matches("routing.observe(").count(), 1);
    }
}
