//! Layer 3a for conductor workers: one `routing_decisions` row (kind
//! `worker`) per `session_spawn`, under the conductor's run. The worker's turn
//! result gives its verification ([`super::record::parse_verification`]); the
//! row closes when the worker's run finishes, with the tokens `run_usage`
//! recorded for it since the row was made.

use serde_json::Value;

use super::record::Verification;
use super::tier;
use crate::conductor::defaults::EffortSource;
use crate::db::Db;

fn effort_source(source: EffortSource) -> &'static str {
    match source {
        EffortSource::Param => "param",
        EffortSource::WorkerSetting => "worker_setting",
        EffortSource::Unknown => "unknown",
    }
}

fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Record a worker the conductor just started. The model and effort are the
/// ones the worker's run started with (resolved by `start_run_inner`); the
/// requested model is the conductor's `model`, if any. No prompt is kept.
pub async fn spawned(db: &Db, conductor_run_id: &str, worker_run_id: &str, args: &Value, source: EffortSource) {
    let (project_id, model, effort, policy_version): (Option<String>, Option<String>, Option<String>, Option<i64>) =
        match sqlx::query_as("SELECT project_id, model, effort, routing_policy_version FROM runs WHERE id = ?")
            .bind(worker_run_id)
            .fetch_optional(db)
            .await
        {
            Ok(Some(row)) => row,
            _ => return,
        };
    let named = arg(args, "model").is_some() || source == EffortSource::Param;
    let result = sqlx::query(
        "INSERT INTO routing_decisions (id, run_id, project_id, created_at, kind, decided_by, agent_type, task_kind, \
         requested_model, resolved_model, requested_effort, effort_source, policy_version, worker_run_id) \
         VALUES (?, ?, ?, ?, 'worker', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(conductor_run_id)
    .bind(project_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(if named { "agent" } else { "default" })
    .bind(arg(args, "role_label").unwrap_or("worker"))
    .bind(tier::kind_or_other(arg(args, "task_kind")))
    .bind(arg(args, "model"))
    .bind(model.filter(|m| !m.is_empty()))
    .bind(effort.filter(|e| !e.is_empty()))
    .bind(effort_source(source))
    .bind(policy_version)
    .bind(worker_run_id)
    .execute(db)
    .await;
    if let Err(e) = result {
        tracing::warn!(event = "routing_record_failed", run_id = %worker_run_id, error = %e);
    }
}

/// A worker's turn ended with this verification: kept on its open row (the
/// last turn's reply wins until the row closes).
pub async fn verified(db: &Db, worker_run_id: &str, verification: Verification) {
    let result = sqlx::query(
        "UPDATE routing_decisions SET verification = ? WHERE kind = 'worker' AND worker_run_id = ? AND status IS NULL",
    )
    .bind(verification.as_str())
    .bind(worker_run_id)
    .execute(db)
    .await;
    if let Err(e) = result {
        tracing::warn!(event = "routing_record_failed", run_id = %worker_run_id, error = %e);
    }
}

/// A run finished: close its open worker row, if it is a worker. The status
/// follows the run's (`cancelled` → `stopped`); tokens and time are the
/// worker's `run_usage` since the row was made; no verification read →
/// `missing` (a Codex reply, a turn that errored).
pub async fn finished(db: &Db, run_id: &str) {
    let result = sqlx::query(
        "UPDATE routing_decisions SET \
            status = CASE (SELECT status FROM runs WHERE id = ?1) \
                WHEN 'cancelled' THEN 'stopped' WHEN 'failed' THEN 'failed' ELSE 'completed' END, \
            total_tokens = (SELECT COALESCE(SUM(u.total_tokens), 0) FROM run_usage u \
                WHERE u.run_id = ?1 AND u.created_at >= routing_decisions.created_at), \
            duration_ms = (SELECT SUM(u.duration_ms) FROM run_usage u \
                WHERE u.run_id = ?1 AND u.created_at >= routing_decisions.created_at), \
            verification = COALESCE(verification, 'missing'), \
            finished_at = ?2 \
         WHERE kind = 'worker' AND worker_run_id = ?1 AND status IS NULL",
    )
    .bind(run_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(db)
    .await;
    if let Err(e) = result {
        tracing::warn!(event = "routing_record_failed", run_id = %run_id, error = %e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
    struct Row {
        run_id: Option<String>,
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
        verification: Option<String>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
    }

    async fn db() -> Db {
        let db = crate::commands::response_styles::tests::test_db().await;
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, model, effort, routing_policy_version, status, created_at) \
             VALUES ('c', 'p1', 'session', 'claude', 'conductor', NULL, 'claude-opus-5-5', 'high', 2, 'running', 'c'), \
                    ('w', 'p1', 'session', 'claude', 'worker', 'c', 'sonnet', 'low', 2, 'running', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        db
    }

    async fn rows(db: &Db) -> Vec<Row> {
        sqlx::query_as(
            "SELECT run_id, decided_by, agent_type, task_kind, requested_model, resolved_model, requested_effort, effort_source, \
             policy_version, description, status, verification, total_tokens, duration_ms FROM routing_decisions ORDER BY rowid",
        )
        .fetch_all(db)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_spawn_is_recorded_under_its_conductor_without_the_prompt() {
        let db = db().await;
        let args = serde_json::json!({"prompt": "SECRET", "role_label": "reviewer", "model": "sonnet", "effort": "low", "task_kind": "review"});
        spawned(&db, "c", "w", &args, EffortSource::Param).await;
        assert_eq!(
            rows(&db).await,
            vec![Row {
                run_id: Some("c".into()),
                decided_by: "agent".into(),
                agent_type: Some("reviewer".into()),
                task_kind: "review".into(),
                requested_model: Some("sonnet".into()),
                resolved_model: Some("sonnet".into()),
                requested_effort: Some("low".into()),
                effort_source: "param".into(),
                policy_version: Some(2),
                description: None,
                status: None,
                verification: None,
                total_tokens: None,
                duration_ms: None,
            }]
        );
        let dump: String = sqlx::query_scalar("SELECT json_group_array(json_array(agent_type, requested_model, task_kind)) FROM routing_decisions")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!dump.contains("SECRET"));
    }

    /// QO-AC40: an unknown kind is recorded as `other`; the effort's source is
    /// the one `conductor::defaults::worker_effort` reported.
    #[tokio::test]
    async fn an_unknown_kind_is_other_and_the_effort_source_is_kept() {
        let db = db().await;
        spawned(&db, "c", "w", &serde_json::json!({"prompt": "p", "task_kind": "xyz", "effort": "extreme"}), EffortSource::WorkerSetting).await;
        spawned(&db, "c", "w", &serde_json::json!({"prompt": "p"}), EffortSource::Unknown).await;
        let got: Vec<_> = rows(&db).await.into_iter().map(|r| (r.task_kind, r.effort_source, r.decided_by, r.agent_type)).collect();
        assert_eq!(
            got,
            vec![
                ("other".into(), "worker_setting".into(), "default".into(), Some("worker".into())),
                ("other".into(), "unknown".into(), "default".into(), Some("worker".into())),
            ]
        );
        // A run that is gone: nothing written, nothing fails.
        spawned(&db, "c", "gone", &serde_json::json!({}), EffortSource::Unknown).await;
        assert_eq!(rows(&db).await.len(), 2);
    }

    async fn usage(db: &Db, run_id: &str, tokens: i64, ms: i64, at: &str) {
        sqlx::query(
            "INSERT INTO run_usage (id, run_id, engine, total_tokens, duration_ms, created_at) VALUES (?, ?, 'claude', ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(tokens)
        .bind(ms)
        .bind(at)
        .execute(db)
        .await
        .unwrap();
    }

    async fn set_status(db: &Db, status: &str) {
        sqlx::query("UPDATE runs SET status = ? WHERE id = 'w'").bind(status).execute(db).await.unwrap();
    }

    /// QO-AC47 (the write): the turn's verification, then the close with the
    /// run's status and the usage of this piece of work only.
    #[tokio::test]
    async fn a_finished_worker_closes_its_row_once() {
        let db = db().await;
        usage(&db, "w", 5000, 100, "2000-01-01T00:00:00+00:00").await;
        spawned(&db, "c", "w", &serde_json::json!({"prompt": "p"}), EffortSource::WorkerSetting).await;
        usage(&db, "w", 1200, 300, "2999-01-01T00:00:00+00:00").await;
        usage(&db, "w", 800, 200, "2999-01-01T00:00:01+00:00").await;
        usage(&db, "c", 99999, 1, "2999-01-01T00:00:00+00:00").await;
        verified(&db, "w", Verification::Pass).await;
        verified(&db, "w", Verification::Fail).await;
        set_status(&db, "done").await;
        finished(&db, "w").await;
        let row = &rows(&db).await[0];
        assert_eq!(
            (row.status.as_deref(), row.verification.as_deref(), row.total_tokens, row.duration_ms),
            (Some("completed"), Some("fail"), Some(2000), Some(500))
        );
        // Closed: a later turn or finish leaves it as it is.
        verified(&db, "w", Verification::Pass).await;
        set_status(&db, "failed").await;
        finished(&db, "w").await;
        assert_eq!(rows(&db).await[0].verification.as_deref(), Some("fail"));
        assert_eq!(rows(&db).await[0].status.as_deref(), Some("completed"));
        // Not a worker's row: untouched.
        finished(&db, "c").await;
    }

    #[tokio::test]
    async fn a_stopped_or_failed_worker_with_no_line_is_missing() {
        for (run_status, want) in [("cancelled", "stopped"), ("failed", "failed")] {
            let db = db().await;
            spawned(&db, "c", "w", &serde_json::json!({"prompt": "p"}), EffortSource::Unknown).await;
            set_status(&db, run_status).await;
            finished(&db, "w").await;
            let row = &rows(&db).await[0];
            assert_eq!((row.status.as_deref(), row.verification.as_deref(), row.total_tokens), (Some(want), Some("missing"), Some(0)));
        }
    }

    /// The worker's own drain reads its turn's reply (`record::Recorder`).
    #[tokio::test]
    async fn the_workers_turn_result_gives_the_verification() {
        let db = db().await;
        spawned(&db, "c", "w", &serde_json::json!({"prompt": "p"}), EffortSource::Unknown).await;
        let mut recorder = super::super::record::Recorder::new(&db, "w", "claude");
        let result = |text: &str, origin: bool| {
            let mut v = serde_json::json!({"type": "result", "subtype": "success", "result": text});
            if origin {
                v["origin"] = serde_json::json!({"kind": "task-notification"});
            }
            v
        };
        recorder.observe(Some(&result("Done.\nVerification: FAIL — 2 tests red", false))).await;
        // A wake-up turn of a background task is not the user's turn: ignored.
        recorder.observe(Some(&result("Verification: PASS — x", true))).await;
        assert_eq!(rows(&db).await[0].verification.as_deref(), Some("fail"));
        // A top-level run's result writes nothing.
        let mut top = super::super::record::Recorder::new(&db, "c", "claude");
        top.observe(Some(&result("Verification: PASS — x", false))).await;
        assert_eq!(rows(&db).await[0].verification.as_deref(), Some("fail"));
    }

    fn section<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
        let body = &src[src.find(from).unwrap()..];
        &body[..body.find(to).unwrap()]
    }

    /// `session_spawn` and `on_run_finished` need a running app, so their
    /// source is checked: the row is written once the worker started, and
    /// closed before the conductor is woken to read the result.
    #[test]
    fn the_conductor_tools_write_and_close_the_row() {
        let tools = include_str!("../../conductor/tools.rs").replace("\r\n", "\n");
        let spawn = section(&tools, "async fn session_spawn(", "pub(super) async fn reserve_worker(");
        let start = spawn.find("start_run_inner(").unwrap();
        let record = spawn.find("crate::runs::routing::worker::spawned(&db, conductor_run_id, &worker_id, &args, effort_source).await;").unwrap();
        assert!(start < record);
        let finished = section(&tools, "pub async fn on_run_finished(", "\n}\n");
        let close = finished.find("crate::runs::routing::worker::finished(").unwrap();
        assert!(close < finished.find("wake_conductor_for_worker(app, &cst, run_id).await;").unwrap());
    }
}
