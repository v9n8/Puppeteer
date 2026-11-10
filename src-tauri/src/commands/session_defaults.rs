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

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

use crate::commands::projects::Project;
use crate::db::Db;

/// Engines a project may default to.
pub const ENGINES: &[&str] = &["claude", "codex"];

/// Longest model id accepted (ids are short; this only stops junk).
const MODEL_MAX_CHARS: usize = 200;

/// The project columns, in the order [`ProjectSessionDefaults::from_row`] reads
/// them by name. Prefix each with `p.` where the query aliases `projects`.
pub const COLUMNS: [&str; 6] = [
    "default_engine_override",
    "default_model_claude",
    "default_model_codex",
    "default_permission_mode",
    "default_effort",
    "default_style",
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
    Ok(ProjectSessionDefaults {
        engine,
        model_claude: check_model("claude", defaults.model_claude)?,
        model_codex: check_model("codex", defaults.model_codex)?,
        permission_mode,
        effort,
        style,
    })
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
/// keep their own engine, model, effort and style.
#[tauri::command]
pub async fn set_project_session_defaults(
    db: State<'_, Db>,
    project_id: String,
    defaults: ProjectSessionDefaults,
) -> Result<Project, String> {
    set(db.inner(), &project_id, defaults).await?;
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
        }
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
