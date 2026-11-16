//! The `_puppeteer_turn_cut` log record (V3-29): one line at the end of a turn
//! that was cut short, so the conversation can say so for every such turn and
//! still does after the app restarts. The reason is one of `user` (Cancel),
//! `quota` (quota stop), `agent` (the Conductor stopped a worker) and
//! `app_closed` (the app exited mid-turn; found at the next start).

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use sqlx::Row;

use super::RunRegistry;
use crate::commands::runs::{fetch_run_log_row, resolve_log_source, LogSource};
use crate::db::Db;

pub const MARKER: &str = "_puppeteer_turn_cut";

/// The record as one log line (newline included).
pub fn line(reason: &str, at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "type": MARKER, "reason": reason, "at": at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true) })
    )
}

/// Whether a stop should leave a mark: a turn is running and this stop has not
/// marked it yet (the quota stop marks first, then cancels).
fn should_mark(busy: bool, pending: &str) -> bool {
    busy && !pending.contains(MARKER)
}

/// Mark the turn a live run is in the middle of, through the log buffer the
/// run's drain writes to the file. Nothing when the run is not live here or no
/// turn is running (a Cancel on a stopped run).
pub async fn push_live(registry: &RunRegistry, run_id: &str, reason: &str) {
    let log_buf = match registry.lock().await.get(run_id) {
        Some(handles) if handles.turn_gate.busy => handles.log_buf.clone(),
        _ => return,
    };
    let mut buf = log_buf.lock().await;
    if should_mark(true, &buf) {
        buf.push_str(&line(reason, chrono::Utc::now()));
    }
}

/// What `cancel_run_inner` / `cancel_run_detached` call first: the Conductor's
/// stop (`stamp_activity` false) is `agent`, everything else `user`.
pub async fn on_cancel(registry: &RunRegistry, run_id: &str, stamp_activity: bool) {
    push_live(registry, run_id, if stamp_activity { "user" } else { "agent" }).await;
}

/// Append one line to a log file. A last line the writer left unfinished gets
/// its newline first, so the record is not glued to it. Reads one byte, not
/// the file.
fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut open = std::fs::OpenOptions::new();
    open.read(true).append(true);
    let mut f = super::rewind::open_log(path, &mut open)?;
    let len = f.metadata()?.len();
    let mut out = String::new();
    if len > 0 {
        let mut last = [0u8; 1];
        f.seek(SeekFrom::Start(len - 1))?;
        f.read_exact(&mut last)?;
        if last[0] != b'\n' {
            out.push('\n');
        }
    }
    out.push_str(line);
    f.write_all(out.as_bytes())
}

/// At start-up, before the leftover `running` rows are reconciled: each of
/// those sessions lost its turn when the app exited, so mark it in its own log
/// file. Nothing drains these logs any more. Best effort: a log that cannot be
/// found or written just gets no mark.
pub async fn mark_app_closed(db: &Db) {
    let ids: Vec<String> = sqlx::query("SELECT id FROM runs WHERE status = 'running'")
        .fetch_all(db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| r.get("id"))
        .collect();
    let record = line("app_closed", chrono::Utc::now());
    for id in ids {
        let Ok(row) = fetch_run_log_row(db, &id).await else { continue };
        // Only the file our own drain writes; a CLI transcript is not ours to edit.
        let Some((LogSource::Conventional, path)) = resolve_log_source(&row, &id) else { continue };
        if let Err(e) = append_line(&path, &record) {
            tracing::warn!(event = "turn_cut_mark_failed", run_id = %id, error = %e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_is_one_json_record() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-16T10:52:03Z").unwrap().into();
        let v: serde_json::Value = serde_json::from_str(line("user", at).trim_end()).unwrap();
        assert_eq!(v["type"], "_puppeteer_turn_cut");
        assert_eq!(v["reason"], "user");
        assert_eq!(v["at"], "2026-11-16T10:52:03Z");
        assert!(line("user", at).ends_with('\n'));
    }

    #[test]
    fn marks_only_a_running_turn_once() {
        assert!(should_mark(true, ""));
        assert!(should_mark(true, "{\"type\":\"assistant\"}\n"));
        assert!(!should_mark(false, ""));
        let pending = format!("[log:warn] quota\n{}", line("quota", chrono::Utc::now()));
        assert!(!should_mark(true, &pending));
    }

    #[test]
    fn append_finishes_an_unfinished_last_line() {
        let dir = std::env::temp_dir().join(format!("puppeteer-cut-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.log");
        std::fs::write(&path, "{\"a\":1}\n{\"b\"").unwrap();
        append_line(&path, "{\"c\":3}\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}\n{\"b\"\n{\"c\":3}\n");
        std::fs::write(&path, "{\"a\":1}\n").unwrap();
        append_line(&path, "{\"c\":3}\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}\n{\"c\":3}\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn db_with_runs(project: &Path) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'proj', ?, '2026-01-01')")
            .bind(project.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        for (id, status) in [("live", "running"), ("done", "done")] {
            sqlx::query("INSERT INTO runs (id, project_id, type, engine, status, created_at) VALUES (?, 'p', 'session', 'claude', ?, '2026-01-01')")
                .bind(id)
                .bind(status)
                .execute(&db)
                .await
                .unwrap();
        }
        db
    }

    #[tokio::test]
    async fn app_closed_marks_only_running_runs_logs() {
        let project = std::env::temp_dir().join(format!("puppeteer-cut-{}", uuid::Uuid::new_v4()));
        let runs = project.join(".puppeteer").join("runs");
        std::fs::create_dir_all(&runs).unwrap();
        std::fs::write(runs.join("live.log"), "{\"type\":\"user\"}\n").unwrap();
        std::fs::write(runs.join("done.log"), "{\"type\":\"user\"}\n").unwrap();
        let db = db_with_runs(&project).await;
        mark_app_closed(&db).await;
        let live = std::fs::read_to_string(runs.join("live.log")).unwrap();
        assert_eq!(live.lines().count(), 2);
        assert!(live.lines().nth(1).unwrap().contains("\"reason\":\"app_closed\""));
        assert_eq!(std::fs::read_to_string(runs.join("done.log")).unwrap().lines().count(), 1);
        let _ = std::fs::remove_dir_all(&project);
    }
}
