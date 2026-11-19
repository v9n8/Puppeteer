//! What a conductor falls back to when nothing more specific was chosen, read
//! from Settings › New sessions each time it is needed (never cached), so a
//! change applies to the next conductor without a restart.
//!
//! - [`max_workers`]: the worker cap a conductor is created with. Only a new
//!   `conductor_sessions` row takes it; an existing conductor keeps its cap and
//!   so its framing byte for byte (the cap is part of it).
//! - [`worker_effort`]: the effort of a worker `session_spawn` creates. It goes
//!   in as the run's own pick, so it is stored on the run and a resume
//!   (`session_send`) keeps it; top-level sessions never read it.
//!
//! The keys are portable (`commands::settings::PORTABLE_SETTING_KEYS`) and a
//! Remote Control controller may set them, like `default_effort`: they lower
//! cost, they are not a safety limit of an unattended turn.

use crate::commands::runs::EFFORT_LEVELS;
use crate::db::Db;

use super::DEFAULT_MAX_WORKERS;

/// Setting: the worker cap of a new conductor, a whole number 1–100.
pub const MAX_WORKERS_SETTING: &str = "default_max_workers";
/// Bounds for the worker cap, mirrored by the composer / sidebar / Settings
/// number inputs (`src/lib/workerCap.ts`).
pub const MIN_MAX_WORKERS: u32 = 1;
pub const MAX_MAX_WORKERS: u32 = 100;

/// Setting: the effort of a new worker when the conductor names none — a level
/// from [`EFFORT_LEVELS`], or [`INHERIT`]. Unset = [`DEFAULT_WORKER_EFFORT`].
pub const WORKER_EFFORT_SETTING: &str = "worker_effort";
pub const DEFAULT_WORKER_EFFORT: &str = "medium";
/// "Same as new sessions": the worker takes the project's, else the global,
/// default effort, as before the setting existed. An empty value already means
/// "unset", so this one needs a word of its own (the CLI's own "not set" word).
pub const INHERIT: &str = "inherit";
/// Levels a conductor may name with `session_spawn`'s `effort`.
pub const SPAWN_EFFORT_LEVELS: &[&str] = &["low", "medium", "high"];
/// Kinds of work a conductor may name with `session_spawn`'s `task_kind`; the
/// routing records count usage by them (QO-06).
pub const TASK_KINDS: &[&str] = &["search", "plan", "implement", "review", "test", "docs", "other"];

/// Where a worker's effort came from, for the routing records (QO-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortSource {
    /// The conductor named it (`session_spawn` `effort`).
    Param,
    /// Settings › Worker effort (or its default).
    WorkerSetting,
    /// Neither: the new-session default applies, decided at start.
    Unknown,
}

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// A cap the user asked for, kept within [`MIN_MAX_WORKERS`]–[`MAX_MAX_WORKERS`].
pub fn clamp_max_workers(value: i64) -> usize {
    value.clamp(MIN_MAX_WORKERS as i64, MAX_MAX_WORKERS as i64) as usize
}

/// The setting's value: unset, blank or not a whole number → [`DEFAULT_MAX_WORKERS`].
fn parse_max_workers(value: Option<&str>) -> usize {
    value
        .map(str::trim)
        .and_then(|v| v.parse::<i64>().ok())
        .map(clamp_max_workers)
        .unwrap_or(DEFAULT_MAX_WORKERS)
}

/// The worker cap a new conductor gets when its launch names none.
pub async fn max_workers(db: &Db) -> usize {
    parse_max_workers(setting(db, MAX_WORKERS_SETTING).await.as_deref())
}

/// A worker's effort: a valid `session_spawn` level wins; else the setting (unset
/// → medium). `inherit` or an unknown value → `None`, so the run falls back to
/// the new-session default. An unknown parameter is ignored, never an error: the
/// spawn goes on with the setting.
fn decide_worker_effort(param: Option<&str>, setting: Option<&str>) -> (Option<String>, EffortSource) {
    if let Some(level) = param.map(str::trim).filter(|p| SPAWN_EFFORT_LEVELS.contains(p)) {
        return (Some(level.to_string()), EffortSource::Param);
    }
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        None => (Some(DEFAULT_WORKER_EFFORT.to_string()), EffortSource::WorkerSetting),
        Some(INHERIT) => (None, EffortSource::Unknown),
        Some(level) if EFFORT_LEVELS.contains(&level) => (Some(level.to_string()), EffortSource::WorkerSetting),
        Some(_) => (None, EffortSource::Unknown),
    }
}

/// The effort a new worker starts with (`StartRunPayload::effort_override`).
pub async fn worker_effort(db: &Db, param: Option<&str>) -> (Option<String>, EffortSource) {
    decide_worker_effort(param, setting(db, WORKER_EFFORT_SETTING).await.as_deref())
}

/// The permission mode a worker resumes with (`session_send`, an escalation):
/// the one it last ran with while its conductor's live mode still allows it
/// (QO-07: a worker spawned stricter stays stricter), else what a new worker
/// would get. Remembered as the worker's mode. A conductor whose mode is
/// unknown gets the error a spawn gets.
pub async fn worker_resume_mode(
    db: &Db,
    conductor: &super::ConductorState,
    conductor_run_id: &str,
    worker_id: &str,
    engine: &str,
) -> Result<String, String> {
    use super::tools::worker_permission_mode;
    let kept = match conductor.worker_mode(worker_id) {
        Some(mode) => worker_permission_mode(db, conductor, conductor_run_id, Some(&mode), engine).await.ok(),
        None => None,
    };
    let mode = match kept {
        Some(mode) => mode,
        None => worker_permission_mode(db, conductor, conductor_run_id, None, engine).await?,
    };
    conductor.set_worker_mode(worker_id, &mode);
    Ok(mode)
}

/// The cap a launch starts with: the one it names (clamped), else the setting.
pub async fn initial_max_workers(db: &Db, requested: Option<u32>) -> usize {
    match requested {
        Some(m) => clamp_max_workers(m as i64),
        None => max_workers(db).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::runs::{fallback_effort, resolve_effort};
    use crate::commands::settings::set_setting;
    use EffortSource::*;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    #[test]
    fn the_cap_setting_is_clamped_and_falls_back_to_three() {
        assert_eq!(DEFAULT_MAX_WORKERS, 3);
        assert_eq!(parse_max_workers(None), 3);
        assert_eq!(parse_max_workers(Some("")), 3);
        assert_eq!(parse_max_workers(Some("many")), 3);
        assert_eq!(parse_max_workers(Some("2.5")), 3);
        assert_eq!(parse_max_workers(Some(" 5 ")), 5);
        assert_eq!(parse_max_workers(Some("0")), 1);
        assert_eq!(parse_max_workers(Some("-4")), 1);
        assert_eq!(parse_max_workers(Some("500")), 100);
    }

    #[tokio::test]
    async fn the_cap_is_read_on_every_launch() {
        let db = db().await;
        assert_eq!(max_workers(&db).await, 3);
        assert_eq!(initial_max_workers(&db, None).await, 3);
        set_setting(&db, MAX_WORKERS_SETTING, "5").await.unwrap();
        assert_eq!(initial_max_workers(&db, None).await, 5);
        // A launch that names its cap keeps it, within bounds.
        assert_eq!(initial_max_workers(&db, Some(8)).await, 8);
        assert_eq!(initial_max_workers(&db, Some(0)).await, 1);
        assert_eq!(initial_max_workers(&db, Some(500)).await, 100);
    }

    #[test]
    fn the_keys_are_portable_and_remote_may_set_them() {
        for key in [MAX_WORKERS_SETTING, WORKER_EFFORT_SETTING] {
            assert!(crate::commands::settings::is_portable_setting(key), "{key}");
            assert!(crate::remote::guard::remote_may_set_setting(key), "{key}");
        }
    }

    /// The effort a fresh worker runs with: [`worker_effort`] goes in as the
    /// run's pick, then the run's own resolution (no stored effort yet).
    fn started(pick: Option<String>, project: Option<&str>, global: &str) -> Option<String> {
        resolve_effort(pick, fallback_effort(None, project, global))
    }

    #[tokio::test]
    async fn a_worker_runs_medium_until_the_setting_says_otherwise() {
        let db = db().await;
        // Unset, with `default_effort = high`: medium, on either engine (the
        // level is resolved before the engine branch).
        let (pick, source) = worker_effort(&db, None).await;
        assert_eq!((pick.as_deref(), source), (Some("medium"), WorkerSetting));
        assert_eq!(started(pick, None, "high").as_deref(), Some("medium"));
        // "Same as new sessions": the project's, else the global default, as before.
        set_setting(&db, WORKER_EFFORT_SETTING, INHERIT).await.unwrap();
        let (pick, source) = worker_effort(&db, None).await;
        assert_eq!((pick.clone(), source), (None, Unknown));
        assert_eq!(started(pick.clone(), None, "high").as_deref(), Some("high"));
        assert_eq!(started(pick, Some("low"), "high").as_deref(), Some("low"));
        // Any level of the shared list is taken, read again on every spawn.
        set_setting(&db, WORKER_EFFORT_SETTING, "xhigh").await.unwrap();
        assert_eq!(worker_effort(&db, None).await, (Some("xhigh".into()), WorkerSetting));
        set_setting(&db, WORKER_EFFORT_SETTING, "turbo").await.unwrap();
        assert_eq!(worker_effort(&db, None).await, (None, Unknown));
    }

    #[tokio::test]
    async fn the_conductors_pick_wins_and_a_bad_one_is_ignored() {
        let db = db().await;
        set_setting(&db, WORKER_EFFORT_SETTING, "medium").await.unwrap();
        assert_eq!(worker_effort(&db, Some("low")).await, (Some("low".into()), Param));
        assert_eq!(worker_effort(&db, Some(" high ")).await, (Some("high".into()), Param));
        // Not a spawn level: the setting decides, the spawn goes on.
        for bad in ["extreme", "max", "", "LOW"] {
            assert_eq!(worker_effort(&db, Some(bad)).await, (Some("medium".into()), WorkerSetting), "{bad}");
        }
    }

    #[test]
    fn a_resume_keeps_the_workers_stored_effort() {
        // `session_send` resumes with no pick: the stored level wins over any
        // default, so changing the setting never touches a running worker.
        assert_eq!(resolve_effort(None, fallback_effort(Some("high"), Some("low"), "low")).as_deref(), Some("high"));
    }

    /// Only `session_spawn` reads the setting: a resume and a top-level session
    /// never do (their effort stays the run's / project's / global).
    #[test]
    fn only_a_spawn_reads_the_worker_effort() {
        let tools = include_str!("tools.rs");
        assert_eq!(tools.matches("defaults::worker_effort(").count(), 1);
        let spawn = &tools[tools.find("async fn session_spawn(").unwrap()..tools.find("pub(super) async fn reserve_worker(").unwrap()];
        assert!(spawn.contains("defaults::worker_effort(") && spawn.contains("effort_override: effort,"));
        for src in [include_str!("../commands/runs.rs"), include_str!("commands.rs")] {
            assert!(!src.contains("worker_effort("));
        }
    }

    /// A conductor `c` in `mode`, with one project.
    async fn conductor(mode: &str) -> (Db, super::super::ConductorState) {
        let db = db().await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', '2026-01-01')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, status, engine, created_at, role) VALUES ('c', 'p1', 'session', 'running', 'claude', 'c', 'conductor')")
            .execute(&db)
            .await
            .unwrap();
        let cst = super::super::ConductorState::new();
        cst.register_session("c", "p1", 5);
        cst.set_permission_mode("c", mode);
        (db, cst)
    }

    /// QO-AC72: a resume (an escalation's retry, a `session_send`) keeps the
    /// mode the worker ran with, never looser than the conductor now allows.
    #[tokio::test]
    async fn a_resumed_worker_keeps_the_mode_it_ran_with() {
        let (db, cst) = conductor("bypassPermissions").await;
        let (strict, mode) = super::super::tools::reserve_worker(&db, &cst, "c", "claude", "t", None, Some("plan")).await.unwrap();
        assert_eq!(mode, "plan");
        let (loose, _) = super::super::tools::reserve_worker(&db, &cst, "c", "claude", "t", None, None).await.unwrap();
        assert_eq!(worker_resume_mode(&db, &cst, "c", &strict, "claude").await.as_deref(), Ok("plan"));
        assert_eq!(worker_resume_mode(&db, &cst, "c", &loose, "claude").await.as_deref(), Ok("bypassPermissions"));
        // The conductor went stricter: the looser worker comes down to it.
        cst.set_permission_mode("c", "default");
        assert_eq!(worker_resume_mode(&db, &cst, "c", &loose, "claude").await.as_deref(), Ok("default"));
        assert_eq!(cst.worker_mode(&loose).as_deref(), Some("default"));
        // Not known (an app restart): the conductor's own; conductor not known: refused.
        assert_eq!(worker_resume_mode(&db, &cst, "c", "unknown", "claude").await.as_deref(), Ok("default"));
        cst.reregister_session("c", "p1", 5, vec![strict.clone()]);
        assert!(worker_resume_mode(&db, &cst, "c", &strict, "claude").await.is_err());
        // Both resume paths use it.
        let tools = include_str!("tools.rs");
        let send = &tools[tools.find("async fn session_send(").unwrap()..tools.find("async fn session_cancel(").unwrap()];
        assert!(send.contains("super::defaults::worker_resume_mode(&db, &state.conductor, conductor_run_id, &worker_id, &engine)"));
        assert!(include_str!("../runs/routing/escalate.rs").contains("crate::conductor::defaults::worker_resume_mode(&db, &cst, &conductor_run_id, run_id, &engine)"));
    }

    /// QO-AC73: both descriptions say what an omitted mode means.
    #[test]
    fn the_spawn_tool_says_an_omitted_mode_is_the_sessions_own() {
        assert!(super::super::CONDUCTOR_SYSTEM_PROMPT.contains("Omit `permission_mode` to give the worker your own mode."));
        assert!(include_str!("mcp_http.rs").contains("Omit it to give the worker your own mode (or the nearest stricter one its engine has)"));
    }
}
