//! AC-23: limits on a turn nobody is watching: the one a schedule fired
//! (`schedules::runner`) or Remote Control started (`remote::handler`). The
//! quota stop (`quota_stop`) caps a whole plan; these cap one turn, so a loop
//! (fix the test, still red, fix again…) cannot burn the quota all night.
//!
//! - Model calls: the sidecar gets [`MAX_TURNS_ENV`]. Claude hands it to the
//!   SDK as `maxTurns`; Codex counts the turn's model requests and interrupts
//!   it. Either reports `_puppeteer_limit_reached` when the limit stopped it.
//! - Time: the drain keeps the turn's clock and kills the sidecar when it runs
//!   out (no graceful Stop yet: AC-13).
//!
//! Either way the run ends `failed`, with the reason in its log and on its
//! `run:done` (the native notification), and nothing resumes it: a resumed
//! turn would get the whole limit again. Dollar budgets are left out: on a
//! subscription the cost is only an estimate, turns and minutes are not.
//!
//! Both settings are local-only for Remote Control (`remote::guard`), so a
//! controller cannot lift the limits of the runs it starts.

use crate::db::Db;
use crate::runs::RunRegistry;
use tokio::process::Command;

/// Model calls one unattended turn may make; "0" = no limit.
pub const MAX_TURNS_SETTING: &str = "unattended_max_turns";
/// Minutes one unattended turn may run; "0" = no limit.
pub const MAX_MINUTES_SETTING: &str = "unattended_max_minutes";
pub const DEFAULT_MAX_TURNS: u32 = 200;
pub const DEFAULT_MAX_MINUTES: u32 = 60;
/// How both sidecars receive the model-call limit.
pub const MAX_TURNS_ENV: &str = "PUPPETEER_MAX_TURNS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_turns: u32,
    pub max_minutes: u32,
}

/// A setting's value: unset, blank or not a whole number → `default`.
fn parse(value: Option<&str>, default: u32) -> u32 {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

pub async fn load(db: &Db) -> Limits {
    Limits {
        max_turns: parse(setting(db, MAX_TURNS_SETTING).await.as_deref(), DEFAULT_MAX_TURNS),
        max_minutes: parse(setting(db, MAX_MINUTES_SETTING).await.as_deref(), DEFAULT_MAX_MINUTES),
    }
}

/// The limits of a turn being started: none for one somebody watches.
pub async fn for_run(db: &Db, unattended: bool) -> Option<Limits> {
    if unattended {
        Some(load(db).await)
    } else {
        None
    }
}

impl Limits {
    /// Hand the sidecar the model-call limit (nothing when it is off).
    pub fn apply_env(&self, cmd: &mut Command) {
        if self.max_turns > 0 {
            cmd.env(MAX_TURNS_ENV, self.max_turns.to_string());
        }
    }

    /// The minutes the drain's clock gives the turn (None when it is off).
    pub fn time_limit(&self) -> Option<u32> {
        (self.max_minutes > 0).then_some(self.max_minutes)
    }
}

/// The log line (and notification body) of a turn the model-call limit stopped.
pub fn turns_reason(limit: u64) -> String {
    format!(
        "Stopped: an unattended turn may make at most {limit} model calls (Settings › AI › Sessions). It was not resumed."
    )
}

/// The log line (and notification body) of a turn the clock stopped.
pub fn time_reason(minutes: u32) -> String {
    format!(
        "Stopped: an unattended turn may run at most {minutes} minutes (Settings › AI › Sessions). It was not resumed."
    )
}

/// The turn's time ran out: kill its sidecar's process group, so the drain
/// sees the process go and ends the run. False when the run is not live here.
pub async fn stop_sidecar(registry: &RunRegistry, run_id: &str) -> bool {
    let pgid = registry.lock().await.get(run_id).and_then(|h| h.pgid);
    pgid.is_some_and(crate::runs::sidecar::kill_process_group)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    fn env_of(cmd: &Command) -> Option<String> {
        cmd.as_std()
            .get_envs()
            .find(|(k, _)| *k == MAX_TURNS_ENV)
            .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    #[tokio::test]
    async fn defaults_apply_until_set_and_zero_turns_a_limit_off() {
        let db = db().await;
        assert_eq!(load(&db).await, Limits { max_turns: 200, max_minutes: 60 });
        crate::commands::settings::set_setting(&db, MAX_TURNS_SETTING, " 50 ").await.unwrap();
        crate::commands::settings::set_setting(&db, MAX_MINUTES_SETTING, "0").await.unwrap();
        let limits = load(&db).await;
        assert_eq!(limits, Limits { max_turns: 50, max_minutes: 0 });
        assert_eq!(limits.time_limit(), None);
        // Blank or not a whole number: the default again.
        crate::commands::settings::set_setting(&db, MAX_TURNS_SETTING, "").await.unwrap();
        crate::commands::settings::set_setting(&db, MAX_MINUTES_SETTING, "-5").await.unwrap();
        assert_eq!(load(&db).await, Limits { max_turns: 200, max_minutes: 60 });
    }

    #[tokio::test]
    async fn only_an_unattended_turn_gets_limits() {
        let db = db().await;
        assert_eq!(for_run(&db, false).await, None);
        let limits = for_run(&db, true).await.unwrap();
        assert_eq!(limits.time_limit(), Some(60));

        let mut cmd = Command::new("true");
        limits.apply_env(&mut cmd);
        assert_eq!(env_of(&cmd).as_deref(), Some("200"));
        // Model calls not limited: no env, the SDK default applies.
        let mut cmd = Command::new("true");
        Limits { max_turns: 0, max_minutes: 60 }.apply_env(&mut cmd);
        assert_eq!(env_of(&cmd), None);
    }

    #[test]
    fn reasons_name_the_limit() {
        assert!(turns_reason(200).starts_with("Stopped: an unattended turn may make at most 200 model calls"));
        assert!(time_reason(60).starts_with("Stopped: an unattended turn may run at most 60 minutes"));
        for reason in [turns_reason(1), time_reason(1)] {
            assert!(reason.contains("Settings › AI › Sessions") && reason.ends_with("It was not resumed."), "{reason}");
        }
    }

    // The stand-in sidecar is `sleep`, which Windows does not ship.
    #[cfg(unix)]
    #[tokio::test]
    async fn out_of_time_kills_the_live_sidecar_only() {
        use crate::runs::{RunHandles, TurnGate};
        use std::sync::Arc;
        use tokio::sync::Mutex as TokioMutex;

        let registry = crate::runs::new_registry();
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        crate::runs::sidecar::detach_process_group(&mut cmd);
        let child = cmd.spawn().unwrap();
        registry.lock().await.insert(
            "r".to_string(),
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

        assert!(!stop_sidecar(&registry, "other").await);
        assert!(stop_sidecar(&registry, "r").await);
        let status = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            registry.lock().await.get_mut("r").unwrap().child.wait().await.unwrap()
        })
        .await
        .expect("the sidecar was killed");
        assert!(!status.success());
        // Left registered: the drain, not this, ends the run (`failed`, not `cancelled`).
        assert!(registry.lock().await.contains_key("r"));
    }
}
