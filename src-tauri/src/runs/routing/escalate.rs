//! Layer 3b: a conductor worker whose turn ended with `Verification: FAIL` is
//! resumed once more, one tier up ([`super::tier::next_up`]), before its
//! conductor is woken (QO-FR38 … QO-FR42). The retry gets a row of its own
//! (`decided_by = 'escalation'`); the failed row is marked `escalated`. When
//! the retry ends, its row is already an escalation, so the conductor is woken
//! whatever it says: one retry per piece of work at most.
//!
//! Whenever a FAIL is not retried, the reason goes to the conductor's
//! `conductor_events` (kind [`SKIPPED_EVENT`]): the conductor may have no live
//! sidecar to write a log line to.

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::tier::{self, Effort, Model};
use crate::db::Db;

/// Setting: whether a failed worker is retried. Unset = on; `"false"` = off.
/// Portable, and a Remote Control controller may set it (audited with
/// `update_setting`), like `routing.policy_enabled`.
pub const ENABLED_SETTING: &str = "routing.escalation_enabled";
/// The message the retried worker gets; fixed, English, never translated.
pub const RETRY_MESSAGE: &str = "Your previous attempt ended with Verification: FAIL. Fix the failure, then verify again.";
/// `conductor_events` kind of a FAIL that was not retried.
pub const SKIPPED_EVENT: &str = "escalation_skipped";

/// What the decision needs to know about a worker whose row just closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub verification: String,
    pub run_status: String,
    pub engine: String,
    pub enabled: bool,
    /// The closed row is itself a retry.
    pub retried: bool,
    pub model: Option<Model>,
    pub effort: Option<Effort>,
    /// The conductor's model family.
    pub ceiling: Option<Model>,
    pub running: usize,
    pub max_workers: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    NotFailed,
    Cancelled,
    Codex,
    Disabled,
    AlreadyRetried,
    TopTier,
    CapReached,
}

impl Skip {
    pub fn as_str(self) -> &'static str {
        match self {
            Skip::NotFailed => "not_failed",
            Skip::Cancelled => "cancelled",
            Skip::Codex => "codex",
            Skip::Disabled => "disabled",
            Skip::AlreadyRetried => "already_retried",
            Skip::TopTier => "top_tier",
            Skip::CapReached => "cap_reached",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Retry on this tier (a part that stays the same is the worker's own).
    Escalate(Option<Model>, Option<Effort>),
    Skip(Skip),
}

/// Pure: retry the worker, or why not. A slot is needed like any resume
/// (`session_send`): the finished worker no longer holds one.
pub fn decide_escalation(f: &Facts) -> Decision {
    let skip = if f.verification != "fail" {
        Skip::NotFailed
    } else if f.run_status == "cancelled" {
        Skip::Cancelled
    } else if f.engine != "claude" {
        Skip::Codex
    } else if !f.enabled {
        Skip::Disabled
    } else if f.retried {
        Skip::AlreadyRetried
    } else if let Some((model, effort)) = tier::next_up(f.model, f.effort, f.ceiling) {
        if f.running >= f.max_workers {
            Skip::CapReached
        } else {
            return Decision::Escalate(model, effort);
        }
    } else {
        Skip::TopTier
    };
    Decision::Skip(skip)
}

/// The worker row that just closed, with the runs it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Closed {
    pub row_id: String,
    pub conductor_run_id: Option<String>,
    pub decided_by: String,
    pub verification: Option<String>,
    pub run_status: Option<String>,
    pub engine: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub conductor_model: Option<String>,
}

/// Close the worker's open row ([`super::worker::finished`]) and read it back.
/// `None`: not a worker, or no open row (a later turn the conductor sent).
pub async fn close_and_read(db: &Db, worker_run_id: &str) -> Option<Closed> {
    let open: Option<String> = sqlx::query_scalar(
        "SELECT id FROM routing_decisions WHERE kind = 'worker' AND worker_run_id = ? AND status IS NULL ORDER BY rowid DESC LIMIT 1",
    )
    .bind(worker_run_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let row_id = open?;
    super::worker::finished(db, worker_run_id).await;
    sqlx::query_as(
        "SELECT d.id AS row_id, d.run_id AS conductor_run_id, d.decided_by, d.verification, \
                w.status AS run_status, w.engine, w.model, w.effort, c.model AS conductor_model \
         FROM routing_decisions d LEFT JOIN runs w ON w.id = d.worker_run_id LEFT JOIN runs c ON c.id = d.run_id \
         WHERE d.id = ?",
    )
    .bind(&row_id)
    .fetch_optional(db)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(event = "routing_record_failed", run_id = %worker_run_id, error = %e);
        None
    })
}

async fn enabled(db: &Db) -> bool {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(ENABLED_SETTING)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    value.as_deref().map(str::trim) != Some("false")
}

fn family(model: Option<&str>) -> Option<Model> {
    model.and_then(Model::family)
}

fn effort_of(effort: Option<&str>) -> Option<Effort> {
    effort.and_then(Effort::parse)
}

/// Write the retry's row (same work, `decided_by = 'escalation'`, open) and
/// mark the failed one. The model and effort are the run's own after the
/// resume; `model` is the alias asked for, when the family went up. Written
/// before the retry's message goes out, so its turn's verification lands on
/// the new row.
pub async fn record_retry(db: &Db, failed_row_id: &str, model: Option<&str>) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO routing_decisions (id, run_id, project_id, created_at, kind, decided_by, agent_type, task_kind, \
         requested_model, resolved_model, requested_effort, effort_source, policy_version, worker_run_id) \
         SELECT ?, d.run_id, d.project_id, ?, 'worker', 'escalation', d.agent_type, d.task_kind, ?, \
                NULLIF(w.model, ''), NULLIF(w.effort, ''), d.effort_source, d.policy_version, d.worker_run_id \
         FROM routing_decisions d LEFT JOIN runs w ON w.id = d.worker_run_id WHERE d.id = ?",
    )
    .bind(&id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(model)
    .bind(failed_row_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE routing_decisions SET escalated = 1 WHERE id = ?")
        .bind(failed_row_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// Take a retry back when its message could not be sent.
async fn undo_retry(db: &Db, failed_row_id: &str, retry_row_id: &str) {
    let _ = sqlx::query("DELETE FROM routing_decisions WHERE id = ?").bind(retry_row_id).execute(db).await;
    let _ = sqlx::query("UPDATE routing_decisions SET escalated = 0 WHERE id = ?").bind(failed_row_id).execute(db).await;
}

/// One `conductor_events` row for a FAIL that was not retried.
pub async fn log_skipped(db: &Db, conductor_run_id: &str, worker_run_id: &str, reason: &str) {
    let payload = serde_json::json!({ "worker_id": worker_run_id, "reason": reason });
    let result = sqlx::query("INSERT INTO conductor_events (session_id, ts, kind, payload_json) VALUES (?, ?, ?, ?)")
        .bind(conductor_run_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(SKIPPED_EVENT)
        .bind(payload.to_string())
        .execute(db)
        .await;
    if let Err(e) = result {
        tracing::warn!(event = "routing_record_failed", run_id = %worker_run_id, error = %e);
    }
}

/// Called by `conductor::wake::on_run_finished` for every finished run that
/// is not a conductor, before its conductor would be woken. Closes the
/// worker's row; `true` when the worker was resumed one tier up, so the
/// conductor must not be woken yet. Either way the hold its drain put on a
/// failed worker ends here, once it is running again or will not be.
pub async fn on_worker_finished(app: &tauri::AppHandle, run_id: &str) -> bool {
    let retried = retry_if_failed(app, run_id).await;
    if let Some(cst) = app.try_state::<crate::conductor::ConductorState>() {
        cst.release_retry(run_id);
    }
    retried
}

async fn retry_if_failed(app: &tauri::AppHandle, run_id: &str) -> bool {
    let db = app.state::<Db>().inner().clone();
    let Some(closed) = close_and_read(&db, run_id).await else {
        return false;
    };
    if closed.verification.as_deref() != Some("fail") {
        return false;
    }
    let Some(conductor_run_id) = closed.conductor_run_id.clone() else {
        return false;
    };
    let skipped = |reason: String| {
        let (db, app, conductor_run_id) = (db.clone(), app.clone(), conductor_run_id.clone());
        async move {
            tracing::info!(event = "worker_escalation_skipped", run_id = %run_id, reason = %reason);
            log_skipped(&db, &conductor_run_id, run_id, &reason).await;
            let _ = app.emit(&format!("conductor:changed:{conductor_run_id}"), serde_json::json!({ "kind": SKIPPED_EVENT }));
            false
        }
    };
    let Some(cst) = app.try_state::<crate::conductor::ConductorState>() else {
        return skipped("conductor_not_live".into()).await;
    };
    let cst = cst.inner().clone();
    let Some(max_workers) = cst.session(&conductor_run_id).map(|s| s.max_workers) else {
        return skipped("conductor_not_live".into()).await;
    };

    // Held through the resume, like `session_send`, so a parallel spawn cannot
    // take the slot between the count and the start.
    let _start_guard = cst.lock_worker_start().await;
    let running = match crate::conductor::tools::count_running_workers(&db, &conductor_run_id).await {
        Ok(n) => n,
        Err(e) => return skipped(format!("resume_failed: {e}")).await,
    };
    let engine = closed.engine.clone().unwrap_or_default();
    let facts = Facts {
        verification: "fail".into(),
        run_status: closed.run_status.clone().unwrap_or_default(),
        engine: engine.clone(),
        enabled: enabled(&db).await,
        retried: closed.decided_by == "escalation",
        model: family(closed.model.as_deref()),
        effort: effort_of(closed.effort.as_deref()),
        ceiling: family(closed.conductor_model.as_deref()),
        running,
        max_workers,
    };
    let (model, effort) = match decide_escalation(&facts) {
        Decision::Escalate(model, effort) => (model, effort),
        Decision::Skip(skip) => return skipped(skip.as_str().into()).await,
    };
    // Only a part that goes up is named; the rest stays the run's own.
    let model_override = model.filter(|m| Some(*m) != facts.model).map(|m| m.as_str().to_string());
    let effort_override = effort.map(|e| e.as_str().to_string());

    // The retry keeps the mode the worker ran with (QO-07).
    let mode = match crate::conductor::defaults::worker_resume_mode(&db, &cst, &conductor_run_id, run_id, &engine).await {
        Ok(mode) => mode,
        Err(e) => return skipped(format!("resume_failed: {e}")).await,
    };
    let resumed = crate::commands::runs::resume_run_with(
        app.clone(),
        app.state::<Db>(),
        app.state::<crate::runs::RunRegistry>(),
        run_id.to_string(),
        Some(mode),
        model_override.clone(),
        effort_override.clone(),
        None,
        Some(false),
        false,
    )
    .await;
    if let Err(e) = resumed {
        return skipped(format!("resume_failed: {e}")).await;
    }
    let retry_row = match record_retry(&db, &closed.row_id, model_override.as_deref()).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(event = "routing_record_failed", run_id = %run_id, error = %e);
            String::new()
        }
    };
    let registry = app.state::<crate::runs::RunRegistry>().inner().clone();
    let sent = crate::commands::runs::send_user_message_inner(
        &db,
        &registry,
        crate::commands::runs::SendUserMessagePayload {
            run_id: run_id.to_string(),
            content: RETRY_MESSAGE.to_string(),
            images: Vec::new(),
            override_budget: false,
            quotes: Vec::new(),
            quote_set: None,
        },
        false,
    )
    .await;
    if let Err(e) = sent {
        undo_retry(&db, &closed.row_id, &retry_row).await;
        return skipped(format!("resume_failed: {e}")).await;
    }
    tracing::info!(event = "worker_escalated", run_id = %run_id, model = ?model_override, effort = ?effort_override);
    true
}

/// "Retried on {to} after a failed check on {from}" on the worker's result
/// card (QO-AC52): the tiers of its newest retry and of the row it retried.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Retried {
    pub from: String,
    pub to: String,
}

/// `sonnet/medium`: the model's family (else its id as stored), then the
/// effort; `default` when the engine picked both.
pub fn tier_label(model: Option<&str>, effort: Option<&str>) -> String {
    let model = model.map(str::trim).filter(|m| !m.is_empty()).map(|m| Model::family(m).map(Model::as_str).unwrap_or(m));
    let effort = effort.map(str::trim).filter(|e| !e.is_empty());
    let parts: Vec<&str> = model.into_iter().chain(effort).collect();
    if parts.is_empty() {
        "default".to_string()
    } else {
        parts.join("/")
    }
}

pub async fn retried(db: &Db, worker_run_id: &str) -> Option<Retried> {
    let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT o.resolved_model, o.requested_effort, e.resolved_model, e.requested_effort \
         FROM routing_decisions e JOIN routing_decisions o \
           ON o.worker_run_id = e.worker_run_id AND o.kind = 'worker' AND o.escalated = 1 \
         WHERE e.kind = 'worker' AND e.worker_run_id = ? AND e.decided_by = 'escalation' \
         ORDER BY e.rowid DESC, o.rowid DESC LIMIT 1",
    )
    .bind(worker_run_id)
    .fetch_optional(db)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(event = "routing_record_failed", run_id = %worker_run_id, error = %e);
        None
    });
    let (from_model, from_effort, to_model, to_effort) = row?;
    Some(Retried {
        from: tier_label(from_model.as_deref(), from_effort.as_deref()),
        to: tier_label(to_model.as_deref(), to_effort.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::defaults::EffortSource;
    use crate::runs::routing::record::Verification;
    use Effort::*;
    use Model::*;

    fn facts() -> Facts {
        Facts {
            verification: "fail".into(),
            run_status: "done".into(),
            engine: "claude".into(),
            enabled: true,
            retried: false,
            model: Some(Sonnet),
            effort: Some(Medium),
            ceiling: Some(Opus),
            running: 0,
            max_workers: 3,
        }
    }

    /// QO-AC51, QO-AC52 (no second retry), QO-AC53 – QO-AC55.
    #[test]
    fn a_failed_worker_goes_one_tier_up_once() {
        assert_eq!(decide_escalation(&facts()), Decision::Escalate(Some(Opus), Some(Medium)));
        let skip = |change: fn(&mut Facts)| {
            let mut f = facts();
            change(&mut f);
            decide_escalation(&f)
        };
        assert_eq!(skip(|f| f.verification = "pass".into()), Decision::Skip(Skip::NotFailed));
        assert_eq!(skip(|f| f.verification = "missing".into()), Decision::Skip(Skip::NotFailed));
        assert_eq!(skip(|f| f.retried = true), Decision::Skip(Skip::AlreadyRetried));
        assert_eq!(skip(|f| (f.model, f.effort) = (Some(Opus), Some(High))), Decision::Skip(Skip::TopTier));
        assert_eq!(skip(|f| f.running = 3), Decision::Skip(Skip::CapReached));
        assert_eq!(skip(|f| f.enabled = false), Decision::Skip(Skip::Disabled));
        assert_eq!(skip(|f| f.engine = "codex".into()), Decision::Skip(Skip::Codex));
        assert_eq!(skip(|f| f.run_status = "cancelled".into()), Decision::Skip(Skip::Cancelled));
        // A failed run (an error ended the turn) that still said FAIL is retried.
        assert_eq!(skip(|f| f.run_status = "failed".into()), Decision::Escalate(Some(Opus), Some(Medium)));
        // At the ceiling the effort goes up; a ceiling no one knows: effort only.
        assert_eq!(skip(|f| f.model = Some(Opus)), Decision::Escalate(Some(Opus), Some(High)));
        assert_eq!(skip(|f| f.ceiling = None), Decision::Escalate(Some(Sonnet), Some(High)));
    }

    async fn db() -> Db {
        let db = crate::commands::response_styles::tests::test_db().await;
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, model, effort, status, created_at) \
             VALUES ('c', 'p1', 'session', 'claude', 'conductor', NULL, 'claude-opus-5-5', 'high', 'running', 'c'), \
                    ('w', 'p1', 'session', 'claude', 'worker', 'c', 'claude-sonnet-5-5', 'medium', 'done', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO conductor_sessions (id, project_id, goal, status, max_workers, created_at) VALUES ('c', 'p1', '', 'running', 3, 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    #[tokio::test]
    async fn the_row_closes_once_and_a_retry_has_a_row_of_its_own() {
        let db = db().await;
        super::super::worker::spawned(&db, "c", "w", &serde_json::json!({"task_kind": "review"}), EffortSource::WorkerSetting).await;
        super::super::worker::verified(&db, "w", Verification::Fail).await;
        let closed = close_and_read(&db, "w").await.unwrap();
        assert_eq!(
            (closed.conductor_run_id.as_deref(), closed.decided_by.as_str(), closed.verification.as_deref(), closed.run_status.as_deref()),
            (Some("c"), "default", Some("fail"), Some("done"))
        );
        assert_eq!((closed.model.as_deref(), closed.conductor_model.as_deref()), (Some("claude-sonnet-5-5"), Some("claude-opus-5-5")));
        // Nothing open: a later turn the conductor sent decides nothing.
        assert_eq!(close_and_read(&db, "w").await, None);
        assert_eq!(close_and_read(&db, "c").await, None);

        // The resume set the run's model; then the retry's row is written.
        sqlx::query("UPDATE runs SET model = 'opus', status = 'running' WHERE id = 'w'").execute(&db).await.unwrap();
        record_retry(&db, &closed.row_id, Some("opus")).await.unwrap();
        let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, i64, Option<String>)> = sqlx::query_as(
            "SELECT decided_by, task_kind, requested_model, resolved_model, requested_effort, escalated, status FROM routing_decisions ORDER BY rowid",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(rows[0].5, 1, "the failed row is marked");
        assert_eq!(
            rows[1],
            ("escalation".into(), "review".into(), Some("opus".into()), Some("opus".into()), Some("medium".into()), 0, None)
        );
        assert_eq!(
            retried(&db, "w").await,
            Some(Retried { from: "sonnet/medium".into(), to: "opus/medium".into() })
        );

        // The retry's turn writes to its own row; once it ends, it is not retried again.
        super::super::worker::verified(&db, "w", Verification::Fail).await;
        sqlx::query("UPDATE runs SET status = 'done' WHERE id = 'w'").execute(&db).await.unwrap();
        let again = close_and_read(&db, "w").await.unwrap();
        assert_eq!((again.decided_by.as_str(), again.verification.as_deref()), ("escalation", Some("fail")));
        assert_eq!(rows[0].6.as_deref(), Some("completed"));
    }

    #[tokio::test]
    async fn the_switch_is_on_until_set_to_false_and_a_skip_is_logged() {
        let db = db().await;
        assert!(enabled(&db).await);
        crate::commands::settings::set_setting(&db, ENABLED_SETTING, "false").await.unwrap();
        assert!(!enabled(&db).await);
        crate::commands::settings::set_setting(&db, ENABLED_SETTING, "true").await.unwrap();
        assert!(enabled(&db).await);

        // QO-AC54: the reason is in the conductor's own events.
        log_skipped(&db, "c", "w", Skip::CapReached.as_str()).await;
        let (kind, payload): (String, String) = sqlx::query_as("SELECT kind, payload_json FROM conductor_events WHERE session_id = 'c'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(kind, "escalation_skipped");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap(), serde_json::json!({"worker_id": "w", "reason": "cap_reached"}));
    }

    #[test]
    fn tiers_are_labelled_by_family_then_effort() {
        assert_eq!(tier_label(Some("claude-sonnet-5-5"), Some("medium")), "sonnet/medium");
        assert_eq!(tier_label(Some("opus"), None), "opus");
        assert_eq!(tier_label(Some(" gpt-5 "), Some("high")), "gpt-5/high");
        assert_eq!(tier_label(None, Some("high")), "high");
        assert_eq!(tier_label(Some(""), None), "default");
    }

    fn section<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
        let body = &src[src.find(from).unwrap()..];
        &body[..body.find(to).unwrap()]
    }

    fn order(src: &str, steps: &[&str]) {
        let at: Vec<usize> = steps.iter().map(|s| src.find(s).unwrap_or_else(|| panic!("missing {s}"))).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{steps:?} at {at:?}");
    }

    /// The hook needs a running app, so the source is checked: the worker's
    /// row is closed (with its verification) before anything is decided, a
    /// retry returns before the conductor is woken (QO-AC51), and the retry's
    /// row exists before its message starts the turn.
    #[test]
    fn the_row_is_closed_before_anything_is_decided() {
        let wake = include_str!("../../conductor/wake.rs").replace("\r\n", "\n");
        let finished = section(&wake, "pub async fn on_run_finished(", "\n}\n");
        order(
            finished,
            &[
                "if cst.session(run_id).is_some() {",
                // A live worker's turn end (BG-04) wakes and returns before any of it.
                "super::idle::live_turn(&registry_from(app), run_id)",
                "if crate::runs::routing::escalate::on_worker_finished(app, run_id).await {\n        return;\n    }",
            ],
        );
        let exit_wake = finished.rfind("wake_conductor_for_worker(app, &cst, run_id).await;").unwrap();
        assert!(finished.find("escalate::on_worker_finished(app, run_id)").unwrap() < exit_wake);
        assert!(!finished.contains("routing::worker::finished("), "closed once, by the escalation");
        let own = include_str!("escalate.rs").replace("\r\n", "\n");
        let wrapper = section(&own, "pub async fn on_worker_finished(", "\n}\n");
        order(wrapper, &["retry_if_failed(app, run_id).await", "cst.release_retry(run_id);"]);
        let hook = section(&own, "async fn retry_if_failed(", "\n}\n");
        order(
            hook,
            &[
                "close_and_read(&db, run_id).await",
                "lock_worker_start().await",
                "decide_escalation(&facts)",
                "resume_run_with(",
                "record_retry(&db,",
                "send_user_message_inner(",
                "undo_retry(",
                "event = \"worker_escalated\"",
            ],
        );
        let close = section(&own, "pub async fn close_and_read(", "\n}\n");
        order(close, &["status IS NULL", "super::worker::finished(db, worker_run_id).await;", "WHERE d.id = ?"]);
    }
}
