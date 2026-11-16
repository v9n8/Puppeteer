//! V3-62: the models a person allows (Settings › Model), per engine.
//!
//! The `enabled_models` setting holds `{"claude": [id…] | null, "codex": [id…] | null}`.
//! `null`, a missing engine, a missing key or broken JSON all mean "every model of
//! that engine is on", so a machine that never saved the list behaves as before.
//! `[]` leaves only "Default (engine picks)".
//!
//! A new session, a Conductor worker and a schedule are refused a model that is off
//! ([`ensure_allowed`], called by `runs::start_run_inner` and `resume_run_with`).
//! A run keeps the model it already ran with, so an old session still resumes.
//! Saving the list through [`save::save_enabled_models`] also moves every default
//! that names a model now off, so the refusal is only a safety net for the paths
//! that write the setting directly (`update_setting` from Remote, a restored backup).

pub mod save;

use serde::{Deserialize, Serialize};

use crate::db::Db;

/// The setting row (portable, Remote: allowed + audited via `update_setting`).
pub const SETTING: &str = "enabled_models";

/// Engines the list covers.
const ENGINES: &[&str] = &["claude", "codex"];

/// Which models of each engine are on; `None` = every model of that engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnabledModels {
    pub claude: Option<Vec<String>>,
    pub codex: Option<Vec<String>>,
}

impl EnabledModels {
    /// Tolerant read of the stored value: anything unreadable turns that engine
    /// (or both) fully on. Ids are trimmed; blanks and repeats are dropped, the
    /// order is kept (the first one is what `save` moves a default to).
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(value) = raw.and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok()) else {
            return Self::default();
        };
        let list = |engine: &str| -> Option<Vec<String>> {
            let items = value.get(engine)?.as_array()?;
            let mut out: Vec<String> = Vec::new();
            for id in items.iter().filter_map(|v| v.as_str()).map(str::trim) {
                if !id.is_empty() && !out.iter().any(|o| o == id) {
                    out.push(id.to_string());
                }
            }
            Some(out)
        };
        Self { claude: list("claude"), codex: list("codex") }
    }

    /// The normalized JSON stored and handed to the UI.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"claude":null,"codex":null}"#.to_string())
    }

    /// The ids of `engine` that are on, or `None` when all of them are.
    pub fn list(&self, engine: &str) -> Option<&[String]> {
        match engine {
            "claude" => self.claude.as_deref(),
            "codex" => self.codex.as_deref(),
            _ => None,
        }
    }

    pub fn all_on(&self) -> bool {
        ENGINES.iter().all(|e| self.list(e).is_none())
    }
}

/// Whether the list decides about `model` on `engine`. Empty is the engine's own
/// pick, a model known to be the other engine's is dropped before a run uses it
/// (`runs::resolve_model_for_engine`), and a Claude family alias (`sonnet`,
/// `opus[1m]`…) is not an id the list can name. Codex has no aliases.
fn governed(engine: &str, model: &str) -> bool {
    if model.is_empty() {
        return false;
    }
    match crate::commands::runs::engine_of_model(model) {
        Some(other) if other != engine => false,
        _ => match engine {
            "claude" => model.to_ascii_lowercase().starts_with("claude-"),
            "codex" => true,
            _ => false,
        },
    }
}

/// Whether `model` is a concrete id of `engine` that the list turns off.
pub fn is_disabled(engine: &str, model: &str, enabled: &EnabledModels) -> bool {
    let model = model.trim();
    governed(engine, model) && enabled.list(engine).is_some_and(|ids| !ids.iter().any(|id| id == model))
}

/// A refused model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelBlocked {
    pub model: String,
}

/// Whether a run of `engine` may use `model`. `previous` is the model the run
/// already ran with: keeping it is always allowed, so an old session resumes.
pub fn check_model(
    engine: &str,
    model: Option<&str>,
    previous: Option<&str>,
    enabled: &EnabledModels,
) -> Result<(), ModelBlocked> {
    let model = model.map(str::trim).unwrap_or("");
    if model.is_empty() || previous.map(str::trim) == Some(model) || !is_disabled(engine, model, enabled) {
        return Ok(());
    }
    Err(ModelBlocked { model: model.to_string() })
}

/// The error a refused start returns: `"<code>:<id>: <English>"`. The UI
/// translates by the code (`run.errors.*`); an agent (Conductor) or a schedule
/// failure notice reads the English.
pub fn blocked_message(blocked: &ModelBlocked, from_default: bool) -> String {
    let m = &blocked.model;
    if from_default {
        format!(
            "model_default_disabled:{m}: Model {m} is the default model for new sessions but is turned off in Settings › Model. \
             Change it in Settings › Defaults for new sessions."
        )
    } else {
        format!("model_disabled:{m}: Model {m} is turned off in Settings › Model. Turn it on or pick another model.")
    }
}

pub async fn load_enabled(db: &Db) -> EnabledModels {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(SETTING)
        .fetch_optional(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(event = "enabled_models_read_failed", error = %e);
            None
        });
    EnabledModels::parse(raw.as_deref())
}

/// Refuse a run start whose resolved `model` is off. `picked` is the caller's
/// own model choice: when the refused model is not it, the model came from a
/// default (project or global) and the message points there.
/// `_project_id` is kept for a per-project list (V3-64).
pub async fn ensure_allowed(
    db: &Db,
    _project_id: &str,
    engine: &str,
    model: Option<&str>,
    previous: Option<&str>,
    picked: Option<&str>,
) -> Result<(), String> {
    let enabled = load_enabled(db).await;
    check_model(engine, model, previous, &enabled).map_err(|blocked| {
        let from_default = picked.map(str::trim) != Some(blocked.model.as_str());
        tracing::info!(event = "model_blocked", engine, model = %blocked.model, from_default);
        blocked_message(&blocked, from_default)
    })
}

/// Tell a paired Controller the list changed, so it gets a fresh
/// `engine_model_options` frame (no-op with Remote Control off).
pub fn notify_changed(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(bus) = app.try_state::<crate::remote::RemoteBus>() {
        bus.publish(crate::remote::RemoteRunEvent::ModelOptionsChanged);
    }
}

/// The list for a phone's `engine_model_options` frame; `None` when every
/// model is on (the field is then left out).
pub async fn wire(db: &Db) -> Option<crate::remote::protocol::EnabledModelsWire> {
    let enabled = load_enabled(db).await;
    (!enabled.all_on()).then(|| crate::remote::protocol::EnabledModelsWire {
        claude: enabled.claude,
        codex: enabled.codex,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(claude: Option<&[&str]>, codex: Option<&[&str]>) -> EnabledModels {
        let v = |l: Option<&[&str]>| l.map(|l| l.iter().map(|s| s.to_string()).collect());
        EnabledModels { claude: v(claude), codex: v(codex) }
    }

    #[test]
    fn parse_is_tolerant_and_normalizes() {
        for raw in [None, Some(""), Some("not json"), Some("[]"), Some("{}"), Some(r#"{"claude":null,"codex":null}"#)] {
            assert_eq!(EnabledModels::parse(raw), EnabledModels::default(), "{raw:?}");
            assert!(EnabledModels::parse(raw).all_on());
        }
        let e = EnabledModels::parse(Some(r#"{"claude":[" claude-sonnet-4-6 ","",3,"claude-sonnet-4-6","claude-haiku-4-5"],"codex":"x"}"#));
        assert_eq!(e, only(Some(&["claude-sonnet-4-6", "claude-haiku-4-5"]), None));
        assert_eq!(e.to_json(), r#"{"claude":["claude-sonnet-4-6","claude-haiku-4-5"],"codex":null}"#);
        assert_eq!(EnabledModels::parse(Some(r#"{"codex":[]}"#)), only(None, Some(&[])));
        assert_eq!(EnabledModels::default().to_json(), r#"{"claude":null,"codex":null}"#);
    }

    #[test]
    fn everything_on_allows_any_model() {
        let all = EnabledModels::default();
        assert!(check_model("claude", Some("claude-opus-4-8"), None, &all).is_ok());
        assert!(check_model("codex", Some("gpt-5.5"), None, &all).is_ok());
    }

    #[test]
    fn a_list_allows_only_its_ids() {
        let e = only(Some(&["claude-sonnet-4-6"]), Some(&["gpt-5.5"]));
        assert!(check_model("claude", Some("claude-sonnet-4-6"), None, &e).is_ok());
        assert_eq!(
            check_model("claude", Some(" claude-opus-4-8 "), None, &e),
            Err(ModelBlocked { model: "claude-opus-4-8".into() })
        );
        // The 1M variant is its own entry.
        assert!(check_model("claude", Some("claude-sonnet-4-6[1m]"), None, &e).is_err());
        assert!(check_model("codex", Some("gpt-5.5"), None, &e).is_ok());
        assert!(check_model("codex", Some("gpt-5.4"), None, &e).is_err());
        assert!(check_model("codex", Some("o4-mini"), None, &e).is_err());
        // An empty list leaves only the engine's own pick.
        let none = only(Some(&[]), None);
        assert!(check_model("claude", Some("claude-sonnet-4-6"), None, &none).is_err());
        // The other engine stays fully on.
        assert!(check_model("codex", Some("gpt-5.4"), None, &none).is_ok());
    }

    #[test]
    fn empty_model_and_aliases_are_always_allowed() {
        let none = only(Some(&[]), Some(&[]));
        for m in [None, Some(""), Some("  ")] {
            assert!(check_model("claude", m, None, &none).is_ok());
            assert!(check_model("codex", m, None, &none).is_ok());
        }
        for alias in ["sonnet", "opus[1m]", "haiku", "default"] {
            assert!(check_model("claude", Some(alias), None, &none).is_ok(), "{alias}");
        }
    }

    #[test]
    fn the_runs_own_model_is_kept() {
        let e = only(Some(&["claude-sonnet-4-6"]), None);
        assert!(check_model("claude", Some("claude-opus-4-8"), Some("claude-opus-4-8"), &e).is_ok());
        // Another model that is off is still refused on resume.
        assert!(check_model("claude", Some("claude-opus-5-5"), Some("claude-opus-4-8"), &e).is_err());
    }

    #[test]
    fn the_other_engines_model_is_not_checked() {
        let none = only(Some(&[]), Some(&[]));
        assert!(check_model("codex", Some("claude-opus-4-8"), None, &none).is_ok());
        assert!(check_model("claude", Some("gpt-5.5"), None, &none).is_ok());
    }

    #[test]
    fn messages_carry_a_code_and_say_where_to_fix() {
        let b = ModelBlocked { model: "claude-opus-4-8".into() };
        assert_eq!(
            blocked_message(&b, false),
            "model_disabled:claude-opus-4-8: Model claude-opus-4-8 is turned off in Settings › Model. Turn it on or pick another model."
        );
        let d = blocked_message(&b, true);
        assert!(d.starts_with("model_default_disabled:claude-opus-4-8: "), "{d}");
        assert!(d.contains("Settings › Defaults for new sessions"), "{d}");
    }

    async fn db_with(value: Option<&str>) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        if let Some(v) = value {
            crate::commands::settings::set_setting(&db, SETTING, v).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn ensure_allowed_names_the_default_when_nothing_was_picked() {
        let db = db_with(Some(r#"{"claude":["claude-sonnet-4-6"],"codex":null}"#)).await;
        let picked = ensure_allowed(&db, "p", "claude", Some("claude-opus-4-8"), None, Some("claude-opus-4-8")).await;
        assert!(picked.unwrap_err().starts_with("model_disabled:claude-opus-4-8: "));
        let default = ensure_allowed(&db, "p", "claude", Some("claude-opus-4-8"), None, None).await;
        assert!(default.unwrap_err().starts_with("model_default_disabled:claude-opus-4-8: "));
        // A cross-engine pick was dropped, so the model came from the default.
        let dropped = ensure_allowed(&db, "p", "claude", Some("claude-opus-4-8"), None, Some("gpt-5.5")).await;
        assert!(dropped.unwrap_err().starts_with("model_default_disabled:"));
        assert!(ensure_allowed(&db, "p", "claude", Some("claude-opus-4-8"), Some("claude-opus-4-8"), None).await.is_ok());
        assert!(ensure_allowed(&db, "p", "codex", Some("gpt-5.4"), None, None).await.is_ok());
    }

    #[tokio::test]
    async fn wire_is_left_out_while_everything_is_on() {
        assert!(wire(&db_with(None).await).await.is_none());
        assert!(wire(&db_with(Some("broken")).await).await.is_none());
        let w = wire(&db_with(Some(r#"{"codex":["gpt-5.5"]}"#)).await).await.unwrap();
        assert_eq!(w.claude, None);
        assert_eq!(w.codex, Some(vec!["gpt-5.5".to_string()]));
    }
}
