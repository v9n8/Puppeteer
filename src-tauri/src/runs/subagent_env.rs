//! QO-01 / QO-02: the model and effort Claude Code gives a sub-agent (a call of
//! its `Agent` tool) when nothing more specific names one. Sub-agents spend most
//! of a session's quota, and by default they run on the session's own model and
//! effort.
//!
//! The CLI reads [`MODEL_ENV`] / [`EFFORT_ENV`] from its environment, which it
//! inherits from the sidecar (the sidecar passes no `options.env`). So every
//! Claude turn with tools, started or resumed, gets them set or removed here
//! from Settings › New sessions, read at spawn time: a change applies from the
//! session's next turn. The rest of the order is the CLI's own: a `model` /
//! `effort` the agent passes to the tool, then the agent file's `model:` /
//! `effort:`, then these, then the session's.
//!
//! Not for Codex, side tasks (`commands::translate`), the model check
//! (`commands::models`) or the `/usage` probe (`commands::stats`): those spawn
//! on their own and never call [`apply`].
//!
//! A chosen model that is turned off in Settings › Model (or the project's own
//! list) is skipped, not enforced: the turn still runs, its sub-agents use the
//! session's model, and the run log says why ([`SubagentEnv::note`]).
//!
//! Both keys are portable and a Remote Control controller may set them, like
//! `claude_model` / `default_effort`: they lower cost, they are no safety limit.
//!
//! The same turns get [`CEILING_ENV`] while the routing policy is on (QO-06):
//! the sidecar then lowers an `Agent` call that asks for a model family above
//! the session's (`sidecar/routing-hook.mjs`).

use tokio::process::Command;

use crate::db::Db;
use crate::model_policy::{self, EnabledModels};

/// Setting: the sub-agent model, an alias (`sonnet`) or a Claude id; "" = the
/// session's model (the env is removed).
pub const SETTING_MODEL: &str = "claude_subagent_model";
/// Setting: the sub-agent effort, `low` / `medium` / `high`, or [`INHERIT`].
/// Unset = [`DEFAULT_EFFORT`].
pub const SETTING_EFFORT: &str = "claude_subagent_effort";
pub const MODEL_ENV: &str = "CLAUDE_CODE_SUBAGENT_MODEL";
pub const EFFORT_ENV: &str = "CLAUDE_CODE_SUBAGENT_EFFORT";
pub const DEFAULT_EFFORT: &str = "medium";
/// Turns on the sidecar's sub-agent ceiling hook.
pub const CEILING_ENV: &str = "PUPPETEER_ROUTING_CEILING";
/// "Same as the session" for the effort: "" already means "unset → medium".
/// It is also the word the CLI reads as "not set", but the env is removed anyway.
pub const INHERIT: &str = "inherit";
/// The levels offered: the aim is to lower cost, never to go above the session.
const LEVELS: &[&str] = &["low", "medium", "high"];

/// What a turn's sidecar gets for its sub-agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentEnv {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The chosen model, turned off in Settings › Model: left out of the env.
    pub skipped_model: Option<String>,
}

/// Decide from the two settings and the models on for the run's project.
fn decide(model: Option<&str>, effort: Option<&str>, enabled: &EnabledModels) -> SubagentEnv {
    // `default` / `inherit` are not models: the CLI would read `default` as an
    // unknown id, not as "the session's".
    let model = model.map(str::trim).filter(|m| !m.is_empty() && *m != "default" && *m != INHERIT);
    let (model, skipped_model) = match model {
        Some(m) if model_policy::is_disabled("claude", m, enabled) => (None, Some(m.to_string())),
        m => (m.map(str::to_string), None),
    };
    let effort = match effort.map(str::trim).filter(|e| !e.is_empty()) {
        None => Some(DEFAULT_EFFORT.to_string()),
        Some(level) if LEVELS.contains(&level) => Some(level.to_string()),
        Some(_) => None,
    };
    SubagentEnv { model, effort, skipped_model }
}

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Read the settings and the models on for `project_id` (never cached).
pub async fn load(db: &Db, project_id: &str) -> SubagentEnv {
    let model = setting(db, SETTING_MODEL).await;
    let effort = setting(db, SETTING_EFFORT).await;
    // The lists only matter for a chosen model; skip the reads otherwise.
    let enabled = if model.as_deref().is_some_and(|m| !m.trim().is_empty()) {
        let global = model_policy::load_enabled(db).await;
        let project = model_policy::project::load(db, project_id).await;
        model_policy::project::intersect(&global, project.as_ref().map(|(_, own)| own))
    } else {
        EnabledModels::default()
    };
    decide(model.as_deref(), effort.as_deref(), &enabled)
}

impl SubagentEnv {
    /// Set each variable, or remove it so a value in the app's own environment
    /// never reaches the CLI when the setting says "same as the session".
    pub fn apply_to(&self, cmd: &mut Command) {
        for (key, value) in [(MODEL_ENV, &self.model), (EFFORT_ENV, &self.effort)] {
            match value {
                Some(v) => cmd.env(key, v),
                None => cmd.env_remove(key),
            };
        }
    }

    /// The run-log line for a skipped model (English, like the broker's other
    /// `[log:*]` lines).
    pub fn note(&self) -> Option<String> {
        self.skipped_model.as_ref().map(|m| {
            format!("[log:info] Sub-agent model {m} is turned off, so sub-agents use the session's model.\n")
        })
    }
}

/// Give a Claude turn's sidecar the sub-agent env; returns the run-log line to
/// add when the chosen model was skipped.
pub async fn apply(db: &Db, project_id: &str, cmd: &mut Command) -> Option<String> {
    let env = load(db, project_id).await;
    env.apply_to(cmd);
    // One switch for routing: the ceiling goes with the policy's (read per turn).
    if crate::runs::routing::policy::enabled(db).await {
        cmd.env(CEILING_ENV, "1");
    } else {
        cmd.env_remove(CEILING_ENV);
    }
    env.note()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::set_setting;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', '2026-01-01')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    /// The variable as the spawn would see it: `Some(None)` = removed.
    fn env(cmd: &Command, key: &str) -> Option<Option<String>> {
        cmd.as_std()
            .get_envs()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    async fn spawn_env(db: &Db) -> (Command, Option<String>) {
        let mut cmd = Command::new("x");
        let note = apply(db, "p1", &mut cmd).await;
        (cmd, note)
    }

    #[tokio::test]
    async fn unset_keys_keep_the_sessions_model_and_give_medium_effort() {
        let db = db().await;
        let (cmd, note) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(None), "removed, so an inherited value never leaks");
        assert_eq!(env(&cmd, EFFORT_ENV), Some(Some("medium".into())));
        assert_eq!(note, None);
    }

    #[tokio::test]
    async fn a_chosen_model_and_effort_reach_the_env_and_are_read_on_every_turn() {
        let db = db().await;
        set_setting(&db, SETTING_MODEL, "sonnet").await.unwrap();
        set_setting(&db, SETTING_EFFORT, "low").await.unwrap();
        let (cmd, _) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(Some("sonnet".into())));
        assert_eq!(env(&cmd, EFFORT_ENV), Some(Some("low".into())));
        // Changed while the session runs: its next turn (a new sidecar) gets it.
        set_setting(&db, SETTING_MODEL, "").await.unwrap();
        set_setting(&db, SETTING_EFFORT, INHERIT).await.unwrap();
        let (cmd, _) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(None));
        assert_eq!(env(&cmd, EFFORT_ENV), Some(None), "same as the session");
    }

    #[tokio::test]
    async fn the_ceiling_follows_the_routing_policy_switch() {
        let db = db().await;
        assert_eq!(env(&spawn_env(&db).await.0, CEILING_ENV), Some(Some("1".into())), "unset = on");
        set_setting(&db, crate::runs::routing::policy::ENABLED_SETTING, "false").await.unwrap();
        assert_eq!(env(&spawn_env(&db).await.0, CEILING_ENV), Some(None), "removed, so an inherited value never leaks");
    }

    #[test]
    fn unknown_values_set_nothing() {
        let all_on = EnabledModels::default();
        for model in ["default", "inherit", "  "] {
            assert_eq!(decide(Some(model), Some("high"), &all_on).model, None, "{model}");
        }
        for effort in ["max", "xhigh", "HIGH", "turbo"] {
            assert_eq!(decide(None, Some(effort), &all_on).effort, None, "{effort}");
        }
        assert_eq!(decide(Some(" claude-opus-5-5 "), Some(" high "), &all_on).model.as_deref(), Some("claude-opus-5-5"));
    }

    #[tokio::test]
    async fn a_model_turned_off_is_skipped_with_a_log_line() {
        let db = db().await;
        set_setting(&db, SETTING_MODEL, "claude-sonnet-5-5").await.unwrap();
        // Off in Settings › Model.
        set_setting(&db, model_policy::SETTING, r#"{"claude":["claude-opus-5-5"],"codex":null}"#).await.unwrap();
        let (cmd, note) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(None));
        assert_eq!(
            note.as_deref(),
            Some("[log:info] Sub-agent model claude-sonnet-5-5 is turned off, so sub-agents use the session's model.\n")
        );
        assert_eq!(env(&cmd, EFFORT_ENV), Some(Some("medium".into())), "the effort still applies");
        // On globally, off in the project's own list.
        set_setting(&db, model_policy::SETTING, r#"{"claude":null,"codex":null}"#).await.unwrap();
        assert!(spawn_env(&db).await.1.is_none());
        sqlx::query(r#"UPDATE projects SET enabled_models = '{"claude":["claude-opus-5-5"],"codex":null}' WHERE id = 'p1'"#)
            .execute(&db)
            .await
            .unwrap();
        let (cmd, note) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(None));
        assert!(note.is_some());
        // An alias is no id a list can name, so it is never turned off.
        set_setting(&db, SETTING_MODEL, "sonnet").await.unwrap();
        let (cmd, note) = spawn_env(&db).await;
        assert_eq!(env(&cmd, MODEL_ENV), Some(Some("sonnet".into())));
        assert!(note.is_none());
    }

    #[test]
    fn the_keys_are_portable_and_remote_may_set_them() {
        for key in [SETTING_MODEL, SETTING_EFFORT] {
            assert!(crate::commands::settings::is_portable_setting(key), "{key}");
            assert!(crate::remote::guard::remote_may_set_setting(key), "{key}");
        }
    }

    fn section<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
        let body = &src[src.find(from).unwrap()..];
        &body[..body.find(to).unwrap()]
    }

    /// The spawns need an AppHandle, so the source is checked: start and resume
    /// each apply the env once, on the Claude path only (between two lines only
    /// that path has), and the start / resume log gets the note.
    #[test]
    fn start_and_resume_apply_it_on_the_claude_path_only() {
        let src = include_str!("../commands/runs.rs").replace("\r\n", "\n");
        let call = "crate::runs::subagent_env::apply(";
        for (from, to, logged) in [
            ("pub(crate) async fn start_run_inner(", "#[tauri::command]", "log_buf.lock().await.push_str(&note);"),
            // Its log buffer is made after the spawn: the note waits for it.
            ("pub(crate) async fn resume_run_with(", "\n}\n", "buf.push_str(subagent_note.as_deref().unwrap_or_default());"),
        ] {
            let body = section(&src, from, to);
            assert_eq!(body.matches(call).count(), 1, "{from}");
            let claude = section(body, "apply_claude_fallback_model(&mut cmd,", "apply_verify_on_stop(");
            assert!(claude.contains(call), "{from}: not on the Claude path");
            assert!(body.contains(logged), "{from}");
        }
    }

    /// Side tasks, the model check and the `/usage` probe spawn on their own and
    /// never get the env.
    #[test]
    fn side_spawns_do_not_get_it() {
        for src in [
            include_str!("../commands/translate.rs"),
            include_str!("../commands/models.rs"),
            include_str!("../commands/stats.rs"),
        ] {
            assert!(!src.contains("subagent_env") && !src.contains("CLAUDE_CODE_SUBAGENT"));
        }
    }
}
