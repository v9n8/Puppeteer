//! A worker whose turn is over while its process lives on (BG-04): background
//! tasks (a backgrounded Bash, a background subagent) keep its sidecar up, so
//! its row stays `running`. For the conductor that worker is idle: its tools
//! say so (listing the tasks), and the auto-resume wakes the conductor at the
//! turn's end instead of at the process exit. The live session decides
//! (`runs::bg_tasks::snapshot`); the DB status alone cannot tell.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde_json::{json, Value};
use sqlx::Row;

use super::worker_results::WorkerResult;
use crate::db::Db;
use crate::runs::bg_tasks::{snapshot, BgSnapshot};
use crate::runs::RunRegistry;

/// The reply each idle worker's turn was reported (or read) with. When the
/// worker later exits with that same reply, its exit has nothing to add.
static REPORTED: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The live run's snapshot when its turn is over; None when it is not live
/// here, or a turn is running (a follow-up, or a wake-up the CLI started).
pub async fn idle_turn(registry: &RunRegistry, run_id: &str) -> Option<BgSnapshot> {
    registry.lock().await.get(run_id).map(|h| snapshot(run_id, h)).filter(|s| s.turn_done)
}

/// Whether `run_id` is live here at all, and if so whether its turn is over.
pub async fn live_turn(registry: &RunRegistry, run_id: &str) -> Option<bool> {
    registry.lock().await.get(run_id).map(|h| snapshot(run_id, h).turn_done)
}

/// Every live run whose turn is over, with when it ended: what
/// `worker_results::collect` adds to the workers that exited.
pub async fn idle_turns(registry: &RunRegistry) -> HashMap<String, String> {
    registry
        .lock()
        .await
        .iter()
        .filter_map(|(id, h)| {
            let snap = snapshot(id, h);
            let done = snap.turn_done;
            snap.turn_ended_at.filter(|_| done).map(|at| (id.clone(), at))
        })
        .collect()
}

/// The conductor has an idle worker's reply (`session_read`, or a nudge).
pub fn note_reported(worker_id: &str, reply: &str) {
    if let Ok(mut m) = REPORTED.lock() {
        m.insert(worker_id.to_string(), reply.to_string());
    }
}

/// A nudge carrying these results went through: note the idle ones' replies.
pub async fn note_delivered(db: &Db, results: &[WorkerResult]) {
    for r in results.iter().filter(|r| r.status == "idle") {
        if let Some((log, _)) = worker_log(db, &r.id).await {
            note_reported(&r.id, &latest_reply(log).await);
        }
    }
}

/// A worker reported idle has exited. When its reply is still the one the
/// conductor has, this exit is marked read, so the wake for it is skipped
/// like any wake for workers already read (CO-10): no second model call to
/// repeat the same report.
pub async fn settle_exit(db: &Db, worker_id: &str) {
    let Some(reported) = REPORTED.lock().ok().and_then(|mut m| m.remove(worker_id)) else {
        return;
    };
    let Some((log, Some(finished_at))) = worker_log(db, worker_id).await else {
        return;
    };
    if latest_reply(log).await == reported {
        super::nudge_reports::mark_read(worker_id, &finished_at);
    }
}

/// The worker's log file and its row's `finished_at`.
async fn worker_log(db: &Db, worker_id: &str) -> Option<(PathBuf, Option<String>)> {
    let row = sqlx::query("SELECT r.finished_at, p.path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?")
        .bind(worker_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()?;
    let log = Path::new(&row.get::<String, _>("path")).join(".puppeteer").join("runs").join(format!("{worker_id}.log"));
    Some((log, row.get("finished_at")))
}

async fn latest_reply(log: PathBuf) -> String {
    // A worker's log runs to megabytes; parse it off the async runtime.
    tokio::task::spawn_blocking(move || super::tools::extract_latest_reply(&log)).await.unwrap_or_default()
}

/// The status the conductor's tools read: the row's, except `idle` for a
/// `running` row whose turn is over.
pub fn effective_status(db_status: String, idle: Option<&BgSnapshot>) -> String {
    if db_status == "running" && idle.is_some() {
        "idle".to_string()
    } else {
        db_status
    }
}

/// Name the background tasks an idle worker keeps alive, so the conductor
/// knows why it still holds a slot and that it may wake again.
pub fn with_background_tasks(mut entry: Value, idle: Option<&BgSnapshot>) -> Value {
    let Some(snap) = idle.filter(|s| !s.tasks.is_empty()) else {
        return entry;
    };
    if let Some(obj) = entry.as_object_mut() {
        let tasks: Vec<Value> = snap
            .tasks
            .iter()
            .map(|t| json!({ "task_id": t.task_id, "task_type": t.task_type, "description": t.description }))
            .collect();
        obj.insert("background_tasks".to_string(), Value::Array(tasks));
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::bg_tasks::BgTask;

    fn snap(tasks: &[&str]) -> BgSnapshot {
        BgSnapshot {
            run_id: "w".into(),
            turn_done: true,
            tasks: tasks
                .iter()
                .map(|d| BgTask { task_id: format!("t-{d}"), task_type: "local_bash".into(), description: d.to_string(), subagent_type: None })
                .collect(),
            turn_ended_at: Some("2026-11-24T10:59:33+00:00".into()),
        }
    }

    #[test]
    fn a_running_row_whose_turn_is_over_reads_idle() {
        let s = snap(&["grep"]);
        assert_eq!(effective_status("running".into(), Some(&s)), "idle");
        assert_eq!(effective_status("running".into(), None), "running");
        // A finished row keeps its own status.
        assert_eq!(effective_status("failed".into(), Some(&s)), "failed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn only_a_live_run_waiting_for_input_counts_as_idle() {
        use crate::runs::bg_tasks::stop_tests::insert_live;
        use crate::runs::TurnGate;
        let registry = crate::runs::new_registry();
        let at = Some("2026-11-24T10:59:33+00:00".to_string());
        let _idle = insert_live(&registry, "idle", TurnGate { turn_ended_at: at.clone(), ..Default::default() }).await;
        // The CLI woke it for a new turn (a task finished).
        let _woken = insert_live(&registry, "woken", TurnGate { busy: true, turn_ended_at: at.clone(), ..Default::default() }).await;
        // Started, its first turn not over yet.
        let _first = insert_live(&registry, "first", TurnGate { busy: true, ..Default::default() }).await;

        assert_eq!(idle_turns(&registry).await, HashMap::from([("idle".to_string(), at.clone().unwrap())]));
        assert_eq!(live_turn(&registry, "idle").await, Some(true));
        assert_eq!(live_turn(&registry, "woken").await, Some(false));
        assert_eq!(live_turn(&registry, "gone").await, None);
        assert!(idle_turn(&registry, "idle").await.is_some());
        assert!(idle_turn(&registry, "woken").await.is_none());
        // Its input closed: on its way out, the exit reports it.
        registry.lock().await.get_mut("idle").unwrap().stdin.take();
        assert_eq!(live_turn(&registry, "idle").await, Some(false));
        assert!(idle_turns(&registry).await.is_empty());
    }

    /// The wake path: a live worker is woken for at its turn's end, before
    /// (and instead of) the exit-time routing close and FAIL retry; the drain
    /// asks for it only when the turn really ended and nothing queued started.
    #[test]
    fn a_live_worker_wakes_its_conductor_at_its_turn_end() {
        let wake = include_str!("wake.rs");
        let finished = &wake[wake.find("pub async fn on_run_finished(").unwrap()..wake.find("async fn wake_conductor_for_worker(").unwrap()];
        let live = finished.find("super::idle::live_turn(&registry_from(app), run_id)").unwrap();
        assert!(live < finished.find("escalate::on_worker_finished(app, run_id)").unwrap());
        assert!(wake.contains("worker_results::collect(&db, conductor_run_id, &idle)"));
        let drain = include_str!("../runs/sidecar.rs");
        let gate = drain.find("let worker_idle = !next_turn && user_turn_ended_once && cst.is_tracked_worker(&run_id);").unwrap();
        let flush = drain[gate..].find("log_file.flush(&log_buf.lock().await);").unwrap();
        let notify = drain[gate..].find("cst.notify_run_finished(run_id.clone());").unwrap();
        assert!(flush < notify);
    }

    /// An idle worker's report is not sent again when it exits with the same
    /// reply; a new reply (a wake-up turn wrote more) is.
    #[tokio::test]
    async fn an_exit_with_the_reported_reply_reads_as_read() {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let dir = std::env::temp_dir().join(format!("puppeteer-idle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".puppeteer").join("runs")).unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'p', ?, 'c')")
            .bind(dir.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        let reply_line = |text: &str| format!("{}\n", json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } }));
        let exited = |id: &str| WorkerResult {
            id: id.into(),
            title: String::new(),
            status: "done".into(),
            finished_at: "2026-11-24T11:30:00+00:00".into(),
            duration_ms: None,
            total_tokens: None,
            retried: None,
        };
        let first = reply_line("Done: migration added.");
        // "more": a wake-up turn (its task notification, then new text) came after.
        let woken = format!("{first}{}\n{}", json!({ "type": "user" }), reply_line("Wake-up: the tests now pass."));
        for (id, log_text) in [("same", first.clone()), ("more", woken)] {
            sqlx::query("INSERT INTO runs (id, project_id, type, status, created_at, finished_at) VALUES (?, 'p', 'session', 'done', 'c', '2026-11-24T11:30:00+00:00')")
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
            note_reported(id, "Done: migration added.");
            std::fs::write(dir.join(".puppeteer").join("runs").join(format!("{id}.log")), log_text).unwrap();
            settle_exit(&db, id).await;
        }
        assert!(super::super::nudge_reports::all_read(&[exited("same")]));
        assert!(!super::super::nudge_reports::all_read(&[exited("more")]));
        // Never reported idle: left alone.
        settle_exit(&db, "never").await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_idle_worker_names_its_background_tasks() {
        let entry = with_background_tasks(json!({ "worker_id": "w" }), Some(&snap(&["npm run dev"])));
        assert_eq!(
            entry,
            json!({ "worker_id": "w", "background_tasks": [{ "task_id": "t-npm run dev", "task_type": "local_bash", "description": "npm run dev" }] })
        );
        // None to name, or not idle: the entry is left as it was.
        assert_eq!(with_background_tasks(json!({ "worker_id": "w" }), Some(&snap(&[]))), json!({ "worker_id": "w" }));
        assert_eq!(with_background_tasks(json!({ "worker_id": "w" }), None), json!({ "worker_id": "w" }));
    }
}
