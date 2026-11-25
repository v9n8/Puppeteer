//! Per-project defaults for new sessions (UX-19): the engine, the model of each
//! engine, the permission mode, the reasoning effort and the response style a
//! session of the project starts with (migration 0013). The project's Claude
//! account has its own column and command (`set_project_claude_account`).
//!
//! Every field is optional: unset follows the global setting of the same meaning
//! (Settings › Defaults for new sessions). A run resolves each field as: its own
//! pick > what the run already has (a resume keeps it) > the project's > the
//! global setting — see `runs::start_run_inner` / `resume_run_with` and
//! `response_styles::resolve_run_style`. So changing a project's defaults never
//! switches the engine, model, effort or style of a session that already ran.
//!
//! The project's own list of models (Project › Model, V3-64, migration 0024) is
//! written with the defaults, so one call saves a new list and the default model it
//! moves to; [`validate`] refuses a default model that list (or Settings) turns off.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

use crate::commands::projects::Project;
use crate::db::Db;
use crate::model_policy::{self, EnabledModels};

/// Engines a project may default to.
pub const ENGINES: &[&str] = &["claude", "codex"];

/// Longest model id accepted (ids are short; this only stops junk).
const MODEL_MAX_CHARS: usize = 200;

/// The project columns, in the order [`ProjectSessionDefaults::from_row`] reads
/// them by name. Prefix each with `p.` where the query aliases `projects`.
pub const COLUMNS: [&str; 7] = [
    "default_engine_override",
    "default_model_claude",
    "default_model_codex",
    "default_permission_mode",
    "default_effort",
    "default_style",
    "enabled_models",
];

/// A project's defaults for new sessions; `None` = follow the global setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSessionDefaults {
    #[serde(default)]
    pub engine: Option<String>,
    /// Default model of a Claude session (like the global `claude_model`).
    #[serde(default)]
    pub model_claude: Option<String>,
    /// Default model of a Codex session (like the global `codex_model`).
    #[serde(default)]
    pub model_codex: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// A built-in style key or a style id (global, or this project's own).
    #[serde(default)]
    pub style: Option<String>,
    /// The models the project's sessions may use (V3-64); `None` = Settings › Model's.
    #[serde(default)]
    pub enabled_models: Option<EnabledModels>,
}

impl ProjectSessionDefaults {
    /// The default model for a session on `engine`.
    pub fn model_for(&self, engine: &str) -> Option<&str> {
        if engine == "codex" {
            self.model_codex.as_deref()
        } else {
            self.model_claude.as_deref()
        }
    }

    /// Read from a row selecting [`COLUMNS`] (by those names).
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Self {
        Self {
            engine: row.get("default_engine_override"),
            model_claude: row.get("default_model_claude"),
            model_codex: row.get("default_model_codex"),
            permission_mode: row.get("default_permission_mode"),
            effort: row.get("default_effort"),
            style: row.get("default_style"),
            enabled_models: row
                .get::<Option<String>, _>("enabled_models")
                .map(|raw| EnabledModels::parse(Some(&raw))),
        }
    }
}

/// `COLUMNS` as a select list, each prefixed with `prefix` (`"p."` or `""`).
pub fn select_list(prefix: &str) -> String {
    COLUMNS.map(|c| format!("{prefix}{c}")).join(", ")
}

/// The defaults of `project_id`. A missing project or a read error gives none,
/// so the global defaults apply: a start never fails over them.
pub async fn load(db: &Db, project_id: &str) -> ProjectSessionDefaults {
    let sql = format!("SELECT {} FROM projects WHERE id = ?", select_list(""));
    match sqlx::query(&sql).bind(project_id).fetch_optional(db).await {
        Ok(Some(row)) => ProjectSessionDefaults::from_row(&row),
        Ok(None) => ProjectSessionDefaults::default(),
        Err(e) => {
            tracing::warn!(event = "project_session_defaults_read_failed", project_id, error = %e);
            ProjectSessionDefaults::default()
        }
    }
}

/// The engine of a new session in `project_id` when the caller names none:
/// the project's default, else the global `default_engine`.
pub async fn default_engine(db: &Db, project_id: &str) -> String {
    match load(db, project_id).await.engine.filter(|e| ENGINES.contains(&e.as_str())) {
        Some(engine) => engine,
        None => crate::commands::settings::resolve_default_engine(db).await,
    }
}

/// The permission mode a run takes without a pick of its own: the project's
/// default, else the global `default_permission_mode`, else `default`. Unknown
/// values count as unset.
pub fn fallback_permission_mode(project: Option<&str>, global: &str) -> String {
    [project, Some(global)]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|m| crate::commands::runs::is_valid_permission_mode(m))
        .unwrap_or("default")
        .to_string()
}

/// The global defaults for new sessions as a project default compares with them:
/// the value the project page stars (blank or unknown engine = `claude`, blank or
/// unknown permission mode = `default`, blank style = no style). A project value
/// that [`GlobalDefaults::follows`] means "follow the global setting" (NULL).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalDefaults {
    pub engine: String,
    pub model_claude: String,
    pub model_codex: String,
    pub permission_mode: String,
    /// `None` while the global level is blank or unknown: no project level matches.
    pub effort: Option<String>,
    pub style: String,
}

impl GlobalDefaults {
    /// From the settings `get` returns ('' when unset).
    pub fn from_settings(get: impl Fn(&str) -> String) -> Self {
        let engine = get("default_engine").trim().to_string();
        let effort = get("default_effort").trim().to_string();
        let style = get("default_style").trim().to_string();
        Self {
            engine: if ENGINES.contains(&engine.as_str()) { engine } else { "claude".into() },
            model_claude: get("claude_model").trim().to_string(),
            model_codex: get("codex_model").trim().to_string(),
            permission_mode: fallback_permission_mode(None, &get("default_permission_mode")),
            effort: crate::commands::runs::EFFORT_LEVELS.contains(&effort.as_str()).then_some(effort),
            style: if style.is_empty() { crate::commands::response_styles::DEFAULT_STYLE_KEY.into() } else { style },
        }
    }

    /// A read error is returned, not taken as "nothing set": blank globals match
    /// `claude` / `default`, so guessing them could clear a real choice.
    pub async fn load(db: &Db) -> Result<Self, String> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT key, value FROM settings WHERE key IN \
             ('default_engine', 'claude_model', 'codex_model', 'default_permission_mode', 'default_effort', 'default_style')",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
        Ok(Self::from_settings(|key| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()))
    }

    /// Whether a project's stored `value` for `column` (one of [`COLUMNS`]) means
    /// the same as the global setting. Models compare exactly: an alias is not the id.
    pub fn follows(&self, column: &str, value: &str) -> bool {
        let value = value.trim();
        match column {
            "default_engine_override" => value == self.engine,
            "default_model_claude" => value == self.model_claude,
            "default_model_codex" => value == self.model_codex,
            "default_permission_mode" => value == self.permission_mode,
            "default_effort" => self.effort.as_deref() == Some(value),
            "default_style" => value == self.style,
            _ => false,
        }
    }
}

/// The permission defaults for a phone's `engine_model_options` frame, so it
/// stars the mode a run without a pick takes (UR-40): the global one resolved
/// like a run's, and per project id the projects that set a valid one.
pub async fn wire_permission_modes(db: &Db) -> (String, std::collections::BTreeMap<String, String>) {
    let global = crate::remote::get_setting(db, "default_permission_mode").await.unwrap_or_default();
    let rows = sqlx::query(
        "SELECT id, default_permission_mode FROM projects WHERE deleted_at IS NULL AND default_permission_mode IS NOT NULL",
    )
    .fetch_all(db)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(event = "project_permission_modes_read_failed", error = %e);
        Vec::new()
    });
    let projects = rows
        .iter()
        .filter_map(|r| {
            let mode = r.get::<String, _>("default_permission_mode").trim().to_string();
            crate::commands::runs::is_valid_permission_mode(&mode).then(|| (r.get("id"), mode))
        })
        .collect();
    (fallback_permission_mode(None, &global), projects)
}

fn clean(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// A model id for `engine`: no blanks or control characters, not too long, and
/// not an id known to belong to the other engine.
pub(crate) fn check_model(engine: &str, model: Option<String>) -> Result<Option<String>, String> {
    let Some(model) = clean(model) else { return Ok(None) };
    if model.chars().count() > MODEL_MAX_CHARS || model.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("'{model}' is not a model id"));
    }
    match crate::commands::runs::engine_of_model(&model) {
        Some(other) if other != engine => Err(format!("'{model}' is not a {engine} model")),
        _ => Ok(Some(model)),
    }
}

/// Trim `defaults` ('' = unset) and check each value against what a run
/// accepts: a known engine, permission mode and effort level, a model of the
/// right engine, a style usable in `project_id` (built-in, global, or its own).
pub async fn validate(
    db: &Db,
    project_id: &str,
    defaults: ProjectSessionDefaults,
) -> Result<ProjectSessionDefaults, String> {
    let engine = clean(defaults.engine);
    if let Some(e) = &engine {
        if !ENGINES.contains(&e.as_str()) {
            return Err(format!("Unknown engine '{e}'"));
        }
    }
    let permission_mode = clean(defaults.permission_mode);
    if let Some(m) = &permission_mode {
        if !crate::commands::runs::is_valid_permission_mode(m) {
            return Err(format!("Unknown permission mode '{m}'"));
        }
    }
    let effort = clean(defaults.effort);
    if let Some(e) = &effort {
        if !crate::commands::runs::EFFORT_LEVELS.contains(&e.as_str()) {
            return Err(format!("Unknown effort level '{e}'"));
        }
    }
    let style = clean(defaults.style);
    if let Some(s) = &style {
        if !crate::commands::response_styles::usable(db, project_id, s).await {
            return Err("This response style does not exist in the project".into());
        }
    }
    let enabled_models = defaults.enabled_models.map(check_enabled_models).transpose()?;
    let checked = ProjectSessionDefaults {
        engine,
        model_claude: check_model("claude", defaults.model_claude)?,
        model_codex: check_model("codex", defaults.model_codex)?,
        permission_mode,
        effort,
        style,
        enabled_models,
    };
    // V3-64-AC-14: a default model must be on in Settings and in the project's list.
    let on = model_policy::intersect(&model_policy::load_enabled(db).await, checked.enabled_models.as_ref());
    for engine in ENGINES {
        if let Some(m) = checked.model_for(engine).filter(|m| model_policy::is_disabled(engine, m, &on)) {
            return Err(format!(
                "Model {m} is turned off for this project (Project › Model) or in Settings › Model, so it cannot be the default {engine} model"
            ));
        }
    }
    Ok(checked)
}

/// A project's list of models, trimmed and deduplicated like the global one, each id
/// a model of its own engine.
fn check_enabled_models(list: EnabledModels) -> Result<EnabledModels, String> {
    let list = EnabledModels::parse(Some(&list.to_json()));
    for engine in ENGINES {
        for id in list.list(engine).unwrap_or_default() {
            check_model(engine, Some(id.clone()))?;
        }
    }
    Ok(list)
}

/// Write `defaults` (checked first) as the project's; every field is replaced.
pub async fn set(db: &Db, project_id: &str, defaults: ProjectSessionDefaults) -> Result<(), String> {
    let d = validate(db, project_id, defaults).await?;
    write(db, project_id, &d).await
}

/// Store already-checked defaults (backup import checks its own way).
pub(crate) async fn write(db: &Db, project_id: &str, d: &ProjectSessionDefaults) -> Result<(), String> {
    let assignments = COLUMNS.map(|c| format!("{c} = ?")).join(", ");
    let done = sqlx::query(&format!("UPDATE projects SET {assignments} WHERE id = ?"))
        .bind(&d.engine)
        .bind(&d.model_claude)
        .bind(&d.model_codex)
        .bind(&d.permission_mode)
        .bind(&d.effort)
        .bind(&d.style)
        .bind(d.enabled_models.as_ref().map(EnabledModels::to_json))
        .bind(project_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    if done.rows_affected() == 0 {
        return Err("Project not found".into());
    }
    Ok(())
}

/// Replace a project's defaults for new sessions. Sessions that already ran
/// keep their own engine, model, effort and style. Remote: allowed + audited.
#[tauri::command]
pub async fn set_project_session_defaults(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    project_id: String,
    defaults: ProjectSessionDefaults,
) -> Result<Project, String> {
    set(db.inner(), &project_id, defaults).await?;
    // The project's list of models may have changed: a paired phone gets it again.
    model_policy::notify_changed(&app);
    crate::commands::projects::project_by_id(db.inner(), &project_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::response_styles::tests::test_db;

    fn d(engine: &str, claude: &str, codex: &str, mode: &str, effort: &str, style: &str) -> ProjectSessionDefaults {
        let o = |s: &str| Some(s.to_string());
        ProjectSessionDefaults {
            engine: o(engine),
            model_claude: o(claude),
            model_codex: o(codex),
            permission_mode: o(mode),
            effort: o(effort),
            style: o(style),
            enabled_models: None,
        }
    }

    #[test]
    fn a_project_value_follows_the_global_one_of_the_same_meaning() {
        let settings = |pairs: &'static [(&'static str, &'static str)]| {
            GlobalDefaults::from_settings(move |k| pairs.iter().find(|(key, _)| *key == k).map(|(_, v)| v.to_string()).unwrap_or_default())
        };
        // Nothing set: Claude, the "default" mode, no effort, no style.
        let blank = settings(&[]);
        for (column, value, follows) in [
            ("default_engine_override", "claude", true),
            ("default_engine_override", "codex", false),
            ("default_permission_mode", "default", true),
            ("default_permission_mode", "plan", false),
            ("default_effort", "high", false),
            ("default_style", "default", true),
            ("default_style", "style-x", false),
            ("default_model_claude", "opus", false),
        ] {
            assert_eq!(blank.follows(column, value), follows, "{column} {value}");
        }
        let set = settings(&[
            ("default_engine", " codex "),
            ("claude_model", "claude-opus-5-5"),
            ("codex_model", "gpt-5.5"),
            ("default_permission_mode", "plan"),
            ("default_effort", "high"),
            ("default_style", "style-x"),
        ]);
        for (column, value, follows) in [
            ("default_engine_override", "codex", true),
            ("default_engine_override", "claude", false),
            ("default_model_claude", " claude-opus-5-5 ", true),
            // An alias is not the full id.
            ("default_model_claude", "opus", false),
            ("default_model_codex", "gpt-5.5", true),
            ("default_permission_mode", "plan", true),
            ("default_permission_mode", "default", false),
            ("default_effort", "high", true),
            ("default_effort", "low", false),
            ("default_style", "style-x", true),
            ("default_style", "default", false),
            ("enabled_models", "x", false),
        ] {
            assert_eq!(set.follows(column, value), follows, "{column} {value}");
        }
        // Unknown global engine / mode / effort: as the star shows them.
        let odd = settings(&[("default_engine", "gemini"), ("default_permission_mode", "yolo"), ("default_effort", "turbo")]);
        assert!(odd.follows("default_engine_override", "claude"));
        assert!(odd.follows("default_permission_mode", "default"));
        assert!(!odd.follows("default_effort", "turbo"));
    }

    #[tokio::test]
    async fn a_new_project_follows_every_global_default() {
        // Both places that create a project (the app, and the MCP sidecar) insert no default
        // column, so they stay NULL and follow Settings.
        let app = include_str!("projects.rs");
        let app = &app[app.find("pub async fn add_project").unwrap()..];
        let app = &app[..app.find("\n}\n").unwrap()];
        let sidecar = include_str!("../../../sidecar-mcp/lib/projects-store.mjs");
        for src in [app, sidecar] {
            let insert = &src[src.find("INSERT INTO projects").unwrap()..];
            let insert = &insert[..insert.find(')').unwrap()];
            assert!(COLUMNS.iter().all(|c| !insert.contains(c)), "{insert}");
        }
        let db = test_db().await;
        assert_eq!(load(&db, "p1").await, ProjectSessionDefaults::default());
    }

    #[tokio::test]
    async fn a_project_back_on_the_global_permission_mode_leaves_the_phone_list() {
        let db = test_db().await;
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('default_permission_mode', 'default')")
            .execute(&db)
            .await
            .unwrap();
        set(&db, "p1", ProjectSessionDefaults { permission_mode: Some("plan".into()), ..Default::default() }).await.unwrap();
        let (global, projects) = wire_permission_modes(&db).await;
        assert_eq!((global.as_str(), projects.get("p1").map(String::as_str)), ("default", Some("plan")));
        // Picking the starred mode on the project page sends the field as null.
        set(&db, "p1", ProjectSessionDefaults::default()).await.unwrap();
        let (global, projects) = wire_permission_modes(&db).await;
        assert_eq!(global, "default");
        assert!(!projects.contains_key("p1"), "{projects:?}");
    }

    #[tokio::test]
    async fn set_validates_and_blank_means_follow_global() {
        let db = test_db().await;
        let own = crate::commands::response_styles::create(&db, Some("p1"), "Team", "", "Team text").await.unwrap();
        let other = crate::commands::response_styles::create(&db, Some("p2"), "Theirs", "", "x").await.unwrap();

        set(&db, "p1", d(" codex ", "claude-opus-4-8", "gpt-5.5", "plan", "high", &own.id)).await.unwrap();
        let got = load(&db, "p1").await;
        assert_eq!(got, d("codex", "claude-opus-4-8", "gpt-5.5", "plan", "high", &own.id));
        assert_eq!(got.model_for("codex"), Some("gpt-5.5"));
        assert_eq!(got.model_for("claude"), Some("claude-opus-4-8"));

        // Refused, nothing written.
        for bad in [
            d("gemini", "", "", "", "", ""),
            d("", "gpt-5.5", "", "", "", ""),
            d("", "", "claude-opus-4-8", "", "", ""),
            d("", "claude opus", "", "", "", ""),
            d("", "", "", "yolo", "", ""),
            d("", "", "", "", "turbo", ""),
            d("", "", "", "", "", "gone"),
            d("", "", "", "", "", &other.id),
        ] {
            assert!(set(&db, "p1", bad.clone()).await.is_err(), "{bad:?}");
        }
        assert_eq!(load(&db, "p1").await.engine.as_deref(), Some("codex"));
        assert!(set(&db, "missing", ProjectSessionDefaults::default()).await.is_err());

        // A built-in or global style is fine; an unknown model id is trusted.
        set(&db, "p1", d("", "my-proxy-model", "", "", "", "concise")).await.unwrap();
        // Blank everywhere: back to following the global settings.
        set(&db, "p1", d(" ", "", "", "", "", "")).await.unwrap();
        assert_eq!(load(&db, "p1").await, ProjectSessionDefaults::default());
    }

    /// V3-64: the project's list is stored normalized; NULL follows Settings.
    #[tokio::test]
    async fn the_project_list_is_checked_and_stored() {
        let db = test_db().await;
        let list = |raw: &str| Some(EnabledModels::parse(Some(raw)));
        let mut own = ProjectSessionDefaults {
            enabled_models: Some(EnabledModels {
                claude: Some(vec![" claude-haiku-4-5 ".into(), "claude-haiku-4-5".into(), "".into()]),
                codex: None,
            }),
            ..Default::default()
        };
        set(&db, "p1", own.clone()).await.unwrap();
        assert_eq!(load(&db, "p1").await.enabled_models, list(r#"{"claude":["claude-haiku-4-5"]}"#));
        let raw: Option<String> = sqlx::query_scalar("SELECT enabled_models FROM projects WHERE id = 'p1'").fetch_one(&db).await.unwrap();
        assert_eq!(raw.as_deref(), Some(r#"{"claude":["claude-haiku-4-5"],"codex":null}"#));
        // Switching to "own" without turning anything off is still a list of its own.
        set(&db, "p2", ProjectSessionDefaults { enabled_models: Some(EnabledModels::default()), ..Default::default() }).await.unwrap();
        assert_eq!(load(&db, "p2").await.enabled_models, Some(EnabledModels::default()));
        // A model of the other engine in a list is refused.
        own.enabled_models = list(r#"{"claude":["gpt-5.5"]}"#);
        assert!(set(&db, "p1", own).await.is_err());
        assert_eq!(load(&db, "p1").await.enabled_models, list(r#"{"claude":["claude-haiku-4-5"]}"#));
        // Following Settings again.
        set(&db, "p1", ProjectSessionDefaults::default()).await.unwrap();
        assert_eq!(load(&db, "p1").await.enabled_models, None);
    }

    /// V3-64-AC-14: a default model off in the project's list or in Settings is refused, nothing written.
    #[tokio::test]
    async fn a_default_model_must_be_on_in_both_lists() {
        let db = test_db().await;
        let own = |claude: &str, list: &str| ProjectSessionDefaults {
            model_claude: Some(claude.to_string()),
            enabled_models: Some(EnabledModels::parse(Some(list))),
            ..Default::default()
        };
        set(&db, "p1", own("claude-haiku-4-5", r#"{"claude":["claude-haiku-4-5"]}"#)).await.unwrap();
        let e = set(&db, "p1", own("claude-opus-4-8", r#"{"claude":["claude-haiku-4-5"]}"#)).await.unwrap_err();
        assert!(e.contains("claude-opus-4-8"), "{e}");
        assert_eq!(load(&db, "p1").await.model_claude.as_deref(), Some("claude-haiku-4-5"));
        // The new list and the default it moves to, in one call.
        set(&db, "p1", own("claude-sonnet-4-6", r#"{"claude":["claude-sonnet-4-6"]}"#)).await.unwrap();
        // An alias or "Default" is never refused.
        set(&db, "p1", own("sonnet", r#"{"claude":[]}"#)).await.unwrap();
        // Off in Settings, the project following it.
        crate::commands::settings::set_setting(&db, model_policy::SETTING, r#"{"codex":["gpt-5.5"]}"#).await.unwrap();
        let follow = ProjectSessionDefaults { model_codex: Some("gpt-5.4".into()), ..Default::default() };
        assert!(set(&db, "p2", follow).await.is_err());
        // Intersection: on in the project, off in Settings.
        let both = ProjectSessionDefaults {
            model_codex: Some("gpt-5.4".into()),
            enabled_models: Some(EnabledModels::parse(Some(r#"{"codex":["gpt-5.4","gpt-5.5"]}"#))),
            ..Default::default()
        };
        assert!(set(&db, "p2", both).await.is_err());
    }

    #[tokio::test]
    async fn default_engine_project_then_global() {
        let db = test_db().await;
        assert_eq!(default_engine(&db, "p1").await, "claude");
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('default_engine', 'codex')").execute(&db).await.unwrap();
        assert_eq!(default_engine(&db, "p1").await, "codex");
        set(&db, "p1", d("claude", "", "", "", "", "")).await.unwrap();
        assert_eq!(default_engine(&db, "p1").await, "claude");
        // Another project, or one that is gone, follows the global setting.
        assert_eq!(default_engine(&db, "p2").await, "codex");
        assert_eq!(default_engine(&db, "missing").await, "codex");
        // The legacy `projects.default_engine` column ('claude' everywhere) is not read.
        sqlx::query("UPDATE projects SET default_engine = 'claude'").execute(&db).await.unwrap();
        assert_eq!(default_engine(&db, "p2").await, "codex");
    }

    #[test]
    fn permission_mode_project_then_global() {
        assert_eq!(fallback_permission_mode(Some("plan"), "acceptEdits"), "plan");
        assert_eq!(fallback_permission_mode(None, "acceptEdits"), "acceptEdits");
        assert_eq!(fallback_permission_mode(Some(" "), "acceptEdits"), "acceptEdits");
        assert_eq!(fallback_permission_mode(Some("yolo"), "bogus"), "default");
        assert_eq!(fallback_permission_mode(None, ""), "default");
    }
}
