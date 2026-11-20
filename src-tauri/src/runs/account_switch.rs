//! RT-04: a Claude account switch takes effect at the next turn, even when the
//! run's sidecar is still alive.
//!
//! A sidecar runs under the login it was spawned with (`CLAUDE_CONFIG_DIR`) for
//! its whole life. Normally it exits when a turn ends and the next message goes
//! through `resume_run`, which spawns one on the run's current account. But a
//! session with a background task (a backgrounded Bash, a Monitor, an agent)
//! keeps its sidecar alive between turns, and a follow-up then goes to that old
//! process — still on the old, possibly exhausted, account (#23).
//!
//! So when the account changes while the process is alive:
//! - no turn in flight: the process is stopped now (its background tasks with
//!   it) and the next message resumes on the new account;
//! - a turn in flight: it is left to finish; when its `result` comes and the
//!   sidecar would otherwise stay alive, the drain stops it
//!   ([`stop_if_switched`]).

use serde::Serialize;

use crate::db::Db;
use crate::runs::RunRegistry;

/// `runs.end_reason` of a session stopped to switch accounts: its `run:done`
/// then tells it apart from one the user stopped.
pub const END_REASON: &str = "account_switch";

/// The log line of a session stopped here.
const NOTICE: &str =
    "[stderr] --- session stopped to switch the Claude account (background tasks included); the next message resumes on it ---\n";

/// What a switch did to the run's live process (`set_run_claude_account`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchOutcome {
    /// No live process here, or it already runs on that account.
    Applied,
    /// A turn is in flight: its process is stopped once that turn ends.
    AfterTurn,
    /// The idle process (and any background task it ran) was stopped; the
    /// next message resumes on the new account.
    Restarted,
}

/// A process spawned on `spawned_with` must be replaced to run on `account`
/// (resolved account ids; `None` = the global `~/.claude` login).
pub fn needs_respawn(spawned_with: Option<&str>, account: Option<&str>) -> bool {
    fn norm(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    norm(spawned_with) != norm(account)
}

/// Stop the run's live process when it runs on another account than
/// `account` and no turn is in flight. Never waits for the run to end.
pub async fn stop_idle(db: &Db, registry: &RunRegistry, run_id: &str, account: Option<&str>) -> SwitchOutcome {
    let mut reg = registry.lock().await;
    let Some(handles) = reg.get(run_id) else {
        return SwitchOutcome::Applied;
    };
    if !needs_respawn(handles.claude_account_id.as_deref(), account) {
        return SwitchOutcome::Applied;
    }
    if handles.turn_gate.busy || handles.turn_gate.stopping || !handles.turn_gate.queued.is_empty() {
        return SwitchOutcome::AfterTurn;
    }
    // Written before the entry goes: the drain reads it once it sees the run
    // removed (as a cancel) and reports it on `run:done`.
    if let Err(e) = sqlx::query("UPDATE runs SET end_reason = ? WHERE id = ?")
        .bind(END_REASON)
        .bind(run_id)
        .execute(db)
        .await
    {
        tracing::warn!(event = "account_switch_end_reason_failed", run_id = %run_id, error = %e);
    }
    let Some(mut handles) = reg.remove(run_id) else {
        return SwitchOutcome::Applied;
    };
    drop(reg);
    handles.log_buf.lock().await.push_str(NOTICE);
    tracing::info!(event = "account_switch_stopped_sidecar", run_id = %run_id);
    if let Some(pgid) = handles.pgid {
        crate::runs::sidecar::kill_process_group(pgid);
    }
    let _ = handles.child.kill().await;
    SwitchOutcome::Restarted
}

/// [`stop_idle`], then wait (up to `settle`) until the drain has ended the run
/// and emitted `run:done`, so the next message sees it stopped and resumes.
pub async fn apply(
    db: &Db,
    registry: &RunRegistry,
    run_id: &str,
    account: Option<&str>,
    settle: std::time::Duration,
) -> SwitchOutcome {
    let outcome = stop_idle(db, registry, run_id, account).await;
    if outcome == SwitchOutcome::Restarted {
        let deadline = tokio::time::Instant::now() + settle;
        while tokio::time::Instant::now() < deadline {
            let status: Option<String> = sqlx::query_scalar("SELECT status FROM runs WHERE id = ?")
                .bind(run_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten();
            if status.as_deref() != Some("running") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        // The drain holds the registry until its `run:done` is out.
        drop(registry.lock().await);
    }
    outcome
}

/// The drain, at the end of a turn whose sidecar stays alive: stop it when the
/// run was switched to another Claude account meanwhile.
pub async fn stop_if_switched(db: &Db, registry: &RunRegistry, run_id: &str) -> SwitchOutcome {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT project_id, claude_account_id FROM runs WHERE id = ? AND engine = 'claude'")
            .bind(run_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    let Some((project_id, explicit)) = row else {
        return SwitchOutcome::Applied;
    };
    // An account that cannot be resolved (deleted) is not switched to: the
    // resume would refuse it anyway, so keep the session as it is.
    let Ok(account) =
        crate::commands::claude_accounts::resolve_runtime_account(db, project_id.as_deref(), explicit.as_deref()).await
    else {
        return SwitchOutcome::Applied;
    };
    stop_idle(db, registry, run_id, account.as_ref().map(|a| a.id.as_str())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::stdin_writer::SidecarInput;
    use crate::runs::{RunHandles, TurnGate};
    use std::sync::Arc;
    use tokio::process::Command;
    use tokio::sync::Mutex;

    #[test]
    fn only_another_account_needs_a_new_process() {
        assert!(!needs_respawn(Some("a1"), Some("a1")));
        assert!(!needs_respawn(None, Some(" ")));
        assert!(needs_respawn(Some("a1"), Some("a2")));
        assert!(needs_respawn(Some("a1"), None));
        assert!(needs_respawn(None, Some("a2")));
    }

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/tmp/p1', 'c')",
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES
                ('a1', 'One', '/tmp/a1', 'c'), ('a2', 'Two', '/tmp/a2', 'c')",
            "INSERT INTO runs (id, project_id, type, status, engine, created_at, claude_account_id)
                VALUES ('r1', 'p1', 'session', 'running', 'claude', 'c', 'a1')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    /// A live run `r1` spawned on `a1` whose process (`sleep`, standing in for
    /// a sidecar kept alive by a background task) is idle unless `busy`.
    async fn live(busy: bool) -> RunRegistry {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take().map(|w| SidecarInput::spawn(w, Box::new(|_| {})));
        let registry = crate::runs::new_registry();
        registry.lock().await.insert(
            "r1".to_string(),
            RunHandles {
                proc: None,
                pgid: None,
                child,
                stdin: input,
                session_id: Arc::new(Mutex::new(None)),
                log_buf: Arc::new(Mutex::new(String::new())),
                broker_run: None,
                ssh_access: None,
                claude_account_id: Some("a1".into()),
                turn_gate: TurnGate { busy, ..Default::default() },
            },
        );
        registry
    }

    async fn end_reason(db: &Db) -> Option<String> {
        sqlx::query_scalar("SELECT end_reason FROM runs WHERE id = 'r1'").fetch_one(db).await.unwrap()
    }

    #[tokio::test]
    async fn an_idle_process_kept_alive_by_a_background_task_is_stopped_on_a_switch() {
        let db = db().await;
        let registry = live(false).await;
        let log = registry.lock().await.get("r1").map(|h| h.log_buf.clone()).unwrap();
        // Same account: nothing happens.
        assert_eq!(stop_idle(&db, &registry, "r1", Some("a1")).await, SwitchOutcome::Applied);
        assert!(registry.lock().await.contains_key("r1"));

        assert_eq!(stop_idle(&db, &registry, "r1", Some("a2")).await, SwitchOutcome::Restarted);
        assert!(!registry.lock().await.contains_key("r1"), "the next message must resume");
        assert_eq!(end_reason(&db).await.as_deref(), Some(END_REASON));
        assert!(log.lock().await.contains("switch the Claude account"));
        // Not live any more: a second switch has nothing to stop.
        assert_eq!(stop_idle(&db, &registry, "r1", Some("a1")).await, SwitchOutcome::Applied);
    }

    #[tokio::test]
    async fn a_turn_in_flight_finishes_on_the_old_account_first() {
        let db = db().await;
        let registry = live(true).await;
        assert_eq!(stop_idle(&db, &registry, "r1", Some("a2")).await, SwitchOutcome::AfterTurn);
        assert!(registry.lock().await.contains_key("r1"));
        assert_eq!(end_reason(&db).await, None);

        // The turn ends with the sidecar still alive: the drain stops it, as
        // the run row now names the other account.
        registry.lock().await.get_mut("r1").unwrap().turn_gate.busy = false;
        sqlx::query("UPDATE runs SET claude_account_id = 'a2' WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(stop_if_switched(&db, &registry, "r1").await, SwitchOutcome::Restarted);
        assert!(!registry.lock().await.contains_key("r1"));
    }

    #[tokio::test]
    async fn the_drain_leaves_a_run_on_its_own_account_alone() {
        let db = db().await;
        let registry = live(false).await;
        assert_eq!(stop_if_switched(&db, &registry, "r1").await, SwitchOutcome::Applied);
        // A Codex run has no Claude account to follow.
        sqlx::query("UPDATE runs SET engine = 'codex', claude_account_id = 'a2' WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(stop_if_switched(&db, &registry, "r1").await, SwitchOutcome::Applied);
        assert!(registry.lock().await.contains_key("r1"));
    }

    #[tokio::test]
    async fn apply_returns_once_the_run_has_ended() {
        let db = db().await;
        let registry = live(false).await;
        // No drain here: the row stands for one that already wrote the end.
        sqlx::query("UPDATE runs SET status = 'cancelled' WHERE id = 'r1'").execute(&db).await.unwrap();
        let started = std::time::Instant::now();
        let out = apply(&db, &registry, "r1", Some("a2"), std::time::Duration::from_secs(5)).await;
        assert_eq!(out, SwitchOutcome::Restarted);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
