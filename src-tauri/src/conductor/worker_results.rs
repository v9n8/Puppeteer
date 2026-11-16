//! What a woken conductor is told about the workers that just finished (V3-30).
//!
//! `wake_conductor` collects the workers whose turn ended since the last nudge,
//! writes one `_puppeteer_worker_results` record into the conductor's log (the
//! session screen draws it as one result card per worker) and names the same
//! workers in the nudge text, so the agent need not call `session_list` to guess.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use sqlx::Row;

use crate::db::Db;
use crate::runs::RunRegistry;

/// Log record type read by `src/lib/streamEvents.ts`.
const RECORD_TYPE: &str = "_puppeteer_worker_results";

/// Newest `finished_at` already reported, per conductor. Kept in memory: after a
/// restart the conductor's own `last_activity_at` stands in for it.
static REPORTED: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkerResult {
    pub id: String,
    pub title: String,
    pub status: String,
    pub finished_at: String,
    /// The worker's latest turn (its newest `run_usage` row); absent when none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
}

/// Workers of this conductor (not trashed, not running) that finished after the
/// last report, oldest first.
pub async fn collect(db: &Db, conductor_run_id: &str) -> Vec<WorkerResult> {
    let known = REPORTED.lock().ok().and_then(|m| m.get(conductor_run_id).cloned());
    let since = match known {
        Some(s) => s,
        None => sqlx::query_scalar::<_, Option<String>>("SELECT last_activity_at FROM runs WHERE id = ?")
            .bind(conductor_run_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .flatten()
            .unwrap_or_default(),
    };
    let rows = sqlx::query(
        "SELECT r.id, COALESCE(r.title, '') AS title, r.status, r.finished_at,
                (SELECT u.duration_ms FROM run_usage u WHERE u.run_id = r.id ORDER BY u.created_at DESC LIMIT 1) AS duration_ms,
                (SELECT u.total_tokens FROM run_usage u WHERE u.run_id = r.id ORDER BY u.created_at DESC LIMIT 1) AS total_tokens
         FROM runs r
         WHERE r.conductor_run_id = ? AND r.deleted_at IS NULL AND r.status != 'running'
           AND r.finished_at IS NOT NULL AND r.finished_at > ?
         ORDER BY r.finished_at ASC",
    )
    .bind(conductor_run_id)
    .bind(since)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    rows.iter()
        .map(|r| WorkerResult {
            id: r.get("id"),
            title: r.get("title"),
            status: r.get("status"),
            finished_at: r.get("finished_at"),
            duration_ms: r.get("duration_ms"),
            total_tokens: r.get("total_tokens"),
        })
        .collect()
}

/// The nudge: the `[session] ` prefix the log reader keys on, the finished
/// workers, then the standing instructions.
pub fn nudge_text(results: &[WorkerResult]) -> String {
    let mut out = String::from("[session] A worker you spawned has finished its turn.");
    if !results.is_empty() {
        out.push_str(" Finished now:\n");
        for r in results {
            let id: String = r.id.chars().take(8).collect();
            out.push_str(&format!("- worker {} ({}): {}\n", id, r.status, r.title));
        }
    }
    out.push_str(
        " Call session_poll() or session_list() to see which of your workers are now idle, \
session_read() each finished worker's result, then continue toward the goal \
(send follow-ups, spawn a specialist, or conclude). If the goal is now met, give \
the human your final summary.",
    );
    out
}

/// Write the cards' record into the conductor's live log and move the
/// "already reported" mark past these workers. Called once the conductor is
/// able to take the nudge, so a failed wake reports them again next time.
pub async fn push(registry: &RunRegistry, conductor_run_id: &str, results: &[WorkerResult]) {
    let Some(last) = results.last() else { return };
    let log_buf = match registry.lock().await.get(conductor_run_id) {
        Some(handles) => handles.log_buf.clone(),
        None => return,
    };
    let record = serde_json::json!({ "type": RECORD_TYPE, "workers": results });
    log_buf.lock().await.push_str(&format!("{record}\n"));
    if let Ok(mut m) = REPORTED.lock() {
        m.insert(conductor_run_id.to_string(), last.finished_at.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wr(id: &str, status: &str, title: &str) -> WorkerResult {
        WorkerResult {
            id: id.into(),
            title: title.into(),
            status: status.into(),
            finished_at: "2026-01-01T00:00:00Z".into(),
            duration_ms: None,
            total_tokens: None,
        }
    }

    #[test]
    fn nudge_lists_workers_before_the_instructions() {
        let t = nudge_text(&[wr("91c0abcdef", "done", "Write the outbox migration"), wr("22ff0000", "failed", "Lint")]);
        assert!(t.starts_with("[session] "));
        let list = t.find("- worker 91c0abcd (done): Write the outbox migration").unwrap();
        assert!(t.contains("- worker 22ff0000 (failed): Lint"));
        assert!(list < t.find("Call session_poll()").unwrap());
    }

    #[test]
    fn nudge_without_results_keeps_the_old_text() {
        let t = nudge_text(&[]);
        assert!(t.starts_with("[session] A worker you spawned has finished its turn. Call session_poll()"));
        assert!(!t.contains("Finished now"));
    }

    #[test]
    fn record_skips_missing_numbers() {
        let v = serde_json::to_value(wr("a", "done", "t")).unwrap();
        assert!(v.get("duration_ms").is_none() && v.get("total_tokens").is_none());
    }

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'proj', '/tmp/p', '2026-01-01')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    async fn run(db: &Db, id: &str, conductor: Option<&str>, status: &str, finished: Option<&str>, deleted: Option<&str>) {
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, engine, status, created_at, title, conductor_run_id, finished_at, deleted_at, last_activity_at)
             VALUES (?, 'p', 'session', 'claude', ?, '2026-01-01', ?, ?, ?, ?, '2026-01-02T00:00:00Z')",
        )
        .bind(id)
        .bind(status)
        .bind(format!("title {id}"))
        .bind(conductor)
        .bind(finished)
        .bind(deleted)
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn collect_takes_finished_live_workers_of_this_conductor_after_its_last_activity() {
        let db = db().await;
        run(&db, "c1", None, "running", None, None).await;
        run(&db, "w-late", Some("c1"), "done", Some("2026-01-03T00:00:00Z"), None).await;
        run(&db, "w-later", Some("c1"), "failed", Some("2026-01-04T00:00:00Z"), None).await;
        run(&db, "w-old", Some("c1"), "done", Some("2026-01-01T00:00:00Z"), None).await;
        run(&db, "w-busy", Some("c1"), "running", Some("2026-01-05T00:00:00Z"), None).await;
        run(&db, "w-trash", Some("c1"), "done", Some("2026-01-05T00:00:00Z"), Some("2026-01-06")).await;
        run(&db, "w-other", Some("c2"), "done", Some("2026-01-05T00:00:00Z"), None).await;
        for (uid, ms, tok, at) in [("u1", 5, 50, "2026-01-03T00:00:00Z"), ("u2", 7, 70, "2026-01-03T00:00:01Z")] {
            sqlx::query("INSERT INTO run_usage (id, run_id, engine, duration_ms, total_tokens, created_at) VALUES (?, 'w-late', 'claude', ?, ?, ?)")
                .bind(uid)
                .bind(ms)
                .bind(tok)
                .bind(at)
                .execute(&db)
                .await
                .unwrap();
        }
        let got = collect(&db, "c1").await;
        let ids: Vec<_> = got.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["w-late", "w-later"]);
        assert_eq!((got[0].duration_ms, got[0].total_tokens), (Some(7), Some(70)));
        assert_eq!((got[1].duration_ms, got[1].total_tokens), (None, None));
    }
}
