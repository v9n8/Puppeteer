//! UX-14: stop the running sessions of an engine / account once its
//! quota block tier (`budget_5h_percent` / `budget_week_percent`) is reached.
//!
//! Opt-in (`quota_stop_running`, off by default): a session stopped mid-turn
//! can leave a file half-edited. Off, nothing here runs and only the existing
//! composer lock (`run:budget_exceeded`) applies.
//!
//! It acts on the crossing only (under → over, per engine / account, as seen by
//! this process), like AWOG's `useQuotaGuard`: a session the user started with
//! a per-turn override while already over must not be stopped right away. The
//! first check after launch only records the state.

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};

use sqlx::Row;

use crate::commands::stats::{claude_plan_usage_key, codex_plan_usage_key, guard_status, BudgetStatus};
use crate::db::Db;
use crate::runs::RunRegistry;

/// The opt-in switch. Local-only for Remote Control (`remote::guard`).
pub const SETTING: &str = "quota_stop_running";

/// Last guard verdict (`is_over`) per quota key.
type Latch = StdMutex<HashMap<String, bool>>;

fn latch() -> &'static Latch {
    static LATCH: OnceLock<Latch> = OnceLock::new();
    LATCH.get_or_init(Default::default)
}

/// The plan snapshot the guard reads for a run: per account of its engine
/// (`None` → the global `~/.claude` / shared `~/.codex` key, as
/// `budget_status_for` does).
fn quota_key(engine: &str, account_id: Option<&str>) -> String {
    if engine == "codex" {
        codex_plan_usage_key(account_id)
    } else {
        format!("{engine}:{}", claude_plan_usage_key(account_id))
    }
}

/// Records `is_over` for `key`; true only on an under → over crossing. The
/// first observation of a key only seeds it.
fn crossed(latch: &Latch, key: &str, is_over: bool) -> bool {
    let prev = latch.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string(), is_over);
    is_over && prev == Some(false)
}

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("value"))
}

pub async fn is_enabled(db: &Db) -> bool {
    setting(db, SETTING).await.is_some_and(|v| v.trim() == "true")
}

/// The line written into each stopped session's log, e.g.
/// "Stopped: quota limit reached (5h 92% ≥ 90%)".
async fn stop_reason(db: &Db, status: &BudgetStatus) -> String {
    let (label, key) = if status.period == "week" {
        ("weekly", "budget_week_percent")
    } else {
        (status.period.as_str(), "budget_5h_percent")
    };
    let block = setting(db, key)
        .await
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(0);
    if block > 0 && status.percent >= block {
        format!("Stopped: quota limit reached ({label} {}% ≥ {block}%)", status.percent)
    } else {
        // Over because the plan itself reports the window blocked.
        format!("Stopped: quota limit reached ({label} {}%, blocked by the plan)", status.percent)
    }
}

/// Feed one guard verdict for `engine` / `account_id` (that engine's account).
/// When the setting
/// is on and this is the crossing, every live, running session on the same
/// quota (except `exclude`) gets the reason in its log, then is cancelled the
/// way the UI's Cancel does. `notify(run_id, line)` runs right before each
/// cancel, so the caller can show the line live. Returns the stopped run ids.
pub async fn on_guard_status(
    registry: &RunRegistry,
    db: &Db,
    engine: &str,
    account_id: Option<&str>,
    status: &BudgetStatus,
    exclude: Option<&str>,
    notify: impl FnMut(&str, &str),
) -> Vec<String> {
    on_guard_status_with(latch(), registry, db, engine, account_id, status, exclude, notify).await
}

/// [`on_guard_status`] after a fresh plan snapshot: reads the guard itself,
/// and nothing at all while the setting is off.
pub async fn on_plan_snapshot(
    registry: &RunRegistry,
    db: &Db,
    engine: &str,
    account_id: Option<&str>,
    notify: impl FnMut(&str, &str),
) -> Vec<String> {
    if !is_enabled(db).await {
        return Vec::new();
    }
    match guard_status(db, engine, account_id).await {
        Ok(status) => on_guard_status(registry, db, engine, account_id, &status, None, notify).await,
        Err(_) => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn on_guard_status_with(
    latch: &Latch,
    registry: &RunRegistry,
    db: &Db,
    engine: &str,
    account_id: Option<&str>,
    status: &BudgetStatus,
    exclude: Option<&str>,
    mut notify: impl FnMut(&str, &str),
) -> Vec<String> {
    if engine.is_empty() || !is_enabled(db).await {
        return Vec::new();
    }
    let key = quota_key(engine, account_id);
    if !crossed(latch, &key, status.is_over) {
        return Vec::new();
    }
    let reason = stop_reason(db, status).await;
    let line = format!("[log:warn] {reason}\n");

    let rows = sqlx::query("SELECT id, engine, claude_account_id, codex_account_id FROM runs WHERE status = 'running'")
        .fetch_all(db)
        .await
        .unwrap_or_default();
    let mut stopped = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let run_engine: String = row.get("engine");
        let run_account: Option<String> =
            row.get(if run_engine == "codex" { "codex_account_id" } else { "claude_account_id" });
        if Some(id.as_str()) == exclude || quota_key(&run_engine, run_account.as_deref()) != key {
            continue;
        }
        // Only sessions live in this process: a stale "running" row has no
        // sidecar to stop.
        let log_buf = match registry.lock().await.get(&id) {
            Some(handles) => handles.log_buf.clone(),
            None => continue,
        };
        // The drain still holds this buffer and persists it once the sidecar
        // is gone, so the reason lands in the session's log file.
        log_buf.lock().await.push_str(&line);
        notify(&id, &reason);
        // Not waiting for the turn to end: this may run in that session's own
        // drain, which must keep reading to see it.
        if crate::commands::runs::cancel_run_detached(registry, db, &id, false).await.is_ok() {
            stopped.push(id);
        }
    }
    stopped
}

// The stand-in sidecars are `sleep`, which Windows does not ship.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::runs::{RunHandles, TurnGate};
    use std::sync::Arc;
    use tokio::process::Command;
    use tokio::sync::Mutex as TokioMutex;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for q in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'proj', '/tmp/p', '2026-01-01')",
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('a1', 'Work', '/tmp/a1', '2026-01-01')",
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('a2', 'Personal', '/tmp/a2', '2026-01-01')",
            // a1 running ×2, a2 running, a1 finished, codex running.
            "INSERT INTO runs (id, project_id, type, engine, claude_account_id, status, created_at) VALUES ('a1-run', 'p', 'session', 'claude', 'a1', 'running', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, claude_account_id, status, created_at) VALUES ('a1-run2', 'p', 'session', 'claude', 'a1', 'running', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, claude_account_id, status, created_at) VALUES ('a2-run', 'p', 'session', 'claude', 'a2', 'running', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, claude_account_id, status, created_at) VALUES ('a1-done', 'p', 'session', 'claude', 'a1', 'done', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, claude_account_id, status, created_at) VALUES ('codex-run', 'p', 'session', 'codex', NULL, 'running', '2026-01-01')",
        ] {
            sqlx::query(q).execute(&db).await.unwrap();
        }
        crate::commands::settings::set_setting(&db, "budget_5h_percent", "90").await.unwrap();
        db
    }

    /// A live run: a `sleep` stand-in sidecar in its own process group.
    async fn registry(ids: &[&str]) -> RunRegistry {
        let registry = crate::runs::new_registry();
        for id in ids {
            let mut cmd = Command::new("sleep");
            cmd.arg("30");
            crate::runs::sidecar::detach_process_group(&mut cmd);
            let child = cmd.spawn().unwrap();
            registry.lock().await.insert(
                id.to_string(),
                RunHandles {
                    proc: None,
                    pgid: child.id(),
                    child,
                    stdin: None,
                    session_id: Arc::new(TokioMutex::new(None)),
                    log_buf: Arc::new(TokioMutex::new(String::new())),
                    broker_run: None,
                    ssh_access: None,
                    turn_gate: TurnGate::default(),
                },
            );
        }
        registry
    }

    fn status(percent: i64) -> BudgetStatus {
        BudgetStatus {
            source: "plan".into(),
            period: "5h".into(),
            percent,
            is_warning: percent >= 81,
            is_over: percent >= 90,
            reset: None,
            captured_at: None,
            is_stale: false,
            status: None,
            rolled_over: false,
            is_quota_warn: false,
            quota_warn_windows: Vec::new(),
        }
    }

    async fn run_status(db: &Db, id: &str) -> String {
        sqlx::query("SELECT status FROM runs WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
            .get("status")
    }

    /// Under, then over: the crossing stops a1's running sessions only.
    async fn cross(latch: &Latch, registry: &RunRegistry, db: &Db, notes: &mut Vec<(String, String)>) -> Vec<String> {
        let mut notify = |id: &str, line: &str| notes.push((id.to_string(), line.to_string()));
        let under = on_guard_status_with(latch, registry, db, "claude", Some("a1"), &status(85), None, &mut notify).await;
        assert!(under.is_empty());
        on_guard_status_with(latch, registry, db, "claude", Some("a1"), &status(92), None, &mut notify).await
    }

    #[tokio::test]
    async fn on_stops_the_running_sessions_of_that_account_only() {
        let db = db().await;
        crate::commands::settings::set_setting(&db, SETTING, "true").await.unwrap();
        let registry = registry(&["a1-run", "a1-run2", "a2-run", "codex-run"]).await;
        let a1_log = registry.lock().await["a1-run"].log_buf.clone();
        let mut notes = Vec::new();

        let mut stopped = cross(&Latch::default(), &registry, &db, &mut notes).await;
        stopped.sort();
        assert_eq!(stopped, vec!["a1-run", "a1-run2"]);
        let reason = "Stopped: quota limit reached (5h 92% ≥ 90%)";
        assert!(notes.iter().all(|(_, line)| line == reason), "{notes:?}");
        assert_eq!(a1_log.lock().await.as_str(), format!("[log:warn] {reason}\n"));
        for id in ["a1-run", "a1-run2"] {
            assert_eq!(run_status(&db, id).await, "cancelled");
            assert!(!registry.lock().await.contains_key(id));
        }
        // Other account, other engine, finished run: untouched.
        for (id, want) in [("a2-run", "running"), ("codex-run", "running"), ("a1-done", "done")] {
            assert_eq!(run_status(&db, id).await, want, "{id}");
        }
        assert!(registry.lock().await.contains_key("a2-run"));
        assert!(registry.lock().await["a2-run"].log_buf.lock().await.is_empty());
    }

    #[tokio::test]
    async fn a_codex_crossing_stops_that_codex_account_only() {
        let db = db().await;
        crate::commands::settings::set_setting(&db, SETTING, "true").await.unwrap();
        for q in [
            "INSERT INTO codex_accounts (id, label, home_dir, created_at) VALUES ('x1', 'Work', '/tmp/x1', '2026-01-01')",
            "INSERT INTO codex_accounts (id, label, home_dir, created_at) VALUES ('x2', 'Home', '/tmp/x2', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, codex_account_id, status, created_at) VALUES ('x1-run', 'p', 'session', 'codex', 'x1', 'running', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, codex_account_id, status, created_at) VALUES ('x2-run', 'p', 'session', 'codex', 'x2', 'running', '2026-01-01')",
        ] {
            sqlx::query(q).execute(&db).await.unwrap();
        }
        let registry = registry(&["x1-run", "x2-run", "codex-run", "a1-run"]).await;
        let latch = Latch::default();
        let check = |percent| {
            let (latch, registry, db) = (&latch, &registry, &db);
            async move { on_guard_status_with(latch, registry, db, "codex", Some("x1"), &status(percent), None, |_, _| {}).await }
        };
        assert!(check(85).await.is_empty());
        assert_eq!(check(92).await, vec!["x1-run"]);
        // The other Codex account, the shared login and Claude: untouched.
        for (id, want) in [("x1-run", "cancelled"), ("x2-run", "running"), ("codex-run", "running"), ("a1-run", "running")] {
            assert_eq!(run_status(&db, id).await, want, "{id}");
        }
    }

    #[tokio::test]
    async fn off_stops_nothing() {
        let db = db().await;
        let registry = registry(&["a1-run", "a1-run2"]).await;
        let latch = Latch::default();
        let mut notes = Vec::new();
        // Never set (default) and explicitly off.
        assert!(cross(&latch, &registry, &db, &mut notes).await.is_empty());
        crate::commands::settings::set_setting(&db, SETTING, "false").await.unwrap();
        assert!(cross(&latch, &registry, &db, &mut notes).await.is_empty());
        assert!(notes.is_empty());
        assert_eq!(run_status(&db, "a1-run").await, "running");
        assert_eq!(registry.lock().await.len(), 2);
        assert!(on_plan_snapshot(&registry, &db, "claude", Some("a1"), |_, _| {}).await.is_empty());
    }

    #[tokio::test]
    async fn acts_on_the_crossing_only() {
        let db = db().await;
        crate::commands::settings::set_setting(&db, SETTING, "true").await.unwrap();
        let registry = registry(&["a1-run", "a1-run2"]).await;
        let latch = Latch::default();
        let check = |percent, exclude| {
            let (latch, registry, db) = (&latch, &registry, &db);
            async move {
                on_guard_status_with(latch, registry, db, "claude", Some("a1"), &status(percent), exclude, |_, _| {}).await
            }
        };
        // First check after launch already over (e.g. an overridden turn): seed only.
        assert!(check(95, None).await.is_empty());
        assert!(check(96, None).await.is_empty(), "still over");
        assert!(check(50, None).await.is_empty(), "back under");
        // The crossing stops them; the run whose turn just ended is spared.
        assert_eq!(check(91, Some("a1-run")).await, vec!["a1-run2"]);
        assert_eq!(run_status(&db, "a1-run").await, "running");
    }

    #[test]
    fn the_quota_is_per_account_for_both_engines() {
        assert_ne!(quota_key("codex", Some("x1")), quota_key("codex", None));
        assert_ne!(quota_key("codex", Some("x1")), quota_key("codex", Some("x2")));
        assert_eq!(quota_key("codex", Some(" ")), quota_key("codex", None));
        assert_ne!(quota_key("codex", Some("a1")), quota_key("claude", Some("a1")));
        assert_ne!(quota_key("claude", Some("a1")), quota_key("claude", Some("a2")));
        assert_ne!(quota_key("claude", None), quota_key("claude", Some("a1")));
        assert_eq!(quota_key("claude", Some(" ")), quota_key("claude", None));
    }

    #[tokio::test]
    async fn reason_names_the_window_and_threshold() {
        let db = db().await;
        crate::commands::settings::set_setting(&db, "budget_week_percent", "80").await.unwrap();
        let mut week = status(85);
        week.period = "week".into();
        assert_eq!(stop_reason(&db, &week).await, "Stopped: quota limit reached (weekly 85% ≥ 80%)");
        assert_eq!(
            stop_reason(&db, &status(70)).await,
            "Stopped: quota limit reached (5h 70%, blocked by the plan)"
        );
    }
}
