//! Saving the list of models that are on (V3-62-AC-12/13). A default that names
//! a model the new list turns off moves to the first model still on for its
//! engine, in the same transaction as the list, so a default is never left off.

use serde::Serialize;
use sqlx::{Row, SqliteConnection};
use tauri::State;

use super::{first_enabled, intersect, is_disabled, EnabledModels, SETTING};
use crate::commands::settings::DEFAULT_CLAUDE_FALLBACK_MODEL;
use crate::db::Db;

/// A project's default model of each engine (`None` = follows the global one).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectModels {
    pub id: String,
    pub name: String,
    pub claude: Option<String>,
    pub codex: Option<String>,
    /// The project's own list (Project › Model, V3-64); `None` = follows Settings.
    pub enabled: Option<EnabledModels>,
}

/// Every place that names a default model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefaultSlots {
    pub claude_model: String,
    pub codex_model: String,
    /// `None` while unset ([`DEFAULT_CLAUDE_FALLBACK_MODEL`] applies); `""` = off.
    pub claude_fallback_model: Option<String>,
    pub translate_engine: String,
    pub translate_model: String,
    pub focus_ai_engine: String,
    pub focus_ai_model: String,
    /// Projects not in the Trash.
    pub projects: Vec<ProjectModels>,
}

/// One default moved off a model that is now off. `place` is the setting key, or
/// `project.<column>` with the project named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultChange {
    pub place: String,
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

impl DefaultChange {
    fn setting(place: &str, from: &str, to: String) -> Self {
        Self { place: place.to_string(), from: from.trim().to_string(), to, project_id: None, project_name: None }
    }
}

/// The engine a side task runs on when its setting is blank: the translate
/// engine (`translate::submit_prompt_as`), else Claude.
fn side_engine(own: &str, fallback: &str) -> String {
    [own, fallback, "claude"]
        .into_iter()
        .map(str::trim)
        .find(|e| !e.is_empty())
        .unwrap_or("claude")
        .to_ascii_lowercase()
}

/// What `value` becomes on `engine`: `None` when it stays (empty, an alias, an
/// id that is on), else the first id still on, or `""` (the engine picks).
fn moved(engine: &str, value: &str, enabled: &EnabledModels) -> Option<String> {
    is_disabled(engine, value, enabled).then(|| first_enabled(engine, enabled))
}

/// The fallback setting may list several models (tried in order): the ones now
/// off are dropped, and with none left it goes back to the `sonnet` alias.
fn moved_fallback(value: &str, enabled: &EnabledModels) -> Option<String> {
    let items: Vec<&str> = value.split(',').map(str::trim).filter(|m| !m.is_empty()).collect();
    if !items.iter().any(|m| is_disabled("claude", m, enabled)) {
        return None;
    }
    let kept: Vec<&str> = items.into_iter().filter(|m| !is_disabled("claude", m, enabled)).collect();
    Some(if kept.is_empty() { DEFAULT_CLAUDE_FALLBACK_MODEL.to_string() } else { kept.join(",") })
}

/// The defaults `enabled` moves, in a stable order (global ones, then projects).
pub fn reassign_defaults(current: &DefaultSlots, enabled: &EnabledModels) -> Vec<DefaultChange> {
    let mut out = Vec::new();
    let translate_engine = side_engine(&current.translate_engine, "");
    let focus_engine = side_engine(&current.focus_ai_engine, &current.translate_engine);
    for (place, engine, value) in [
        ("claude_model", "claude", &current.claude_model),
        ("codex_model", "codex", &current.codex_model),
        ("translate_model", translate_engine.as_str(), &current.translate_model),
        ("focus.ai_model", focus_engine.as_str(), &current.focus_ai_model),
    ] {
        if let Some(to) = moved(engine, value, enabled) {
            out.push(DefaultChange::setting(place, value, to));
        }
    }
    if let Some(value) = &current.claude_fallback_model {
        if let Some(to) = moved_fallback(value, enabled) {
            out.push(DefaultChange::setting("claude_fallback_model", value, to));
        }
    }
    for p in &current.projects {
        // A project with its own list (V3-64) keeps its defaults within both lists.
        let enabled = &intersect(enabled, p.enabled.as_ref());
        for (column, engine, value) in [
            ("default_model_claude", "claude", &p.claude),
            ("default_model_codex", "codex", &p.codex),
        ] {
            let Some(value) = value else { continue };
            if let Some(to) = moved(engine, value, enabled) {
                out.push(DefaultChange {
                    place: format!("project.{column}"),
                    from: value.trim().to_string(),
                    to,
                    project_id: Some(p.id.clone()),
                    project_name: Some(p.name.clone()),
                });
            }
        }
    }
    out
}

async fn read_slots(conn: &mut SqliteConnection) -> Result<DefaultSlots, String> {
    let rows = sqlx::query(
        "SELECT key, value FROM settings WHERE key IN \
         ('claude_model', 'codex_model', 'claude_fallback_model', 'translate_engine', 'translate_model', \
          'focus.ai_engine', 'focus.ai_model')",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let mut slots = DefaultSlots::default();
    for row in rows {
        let value: String = row.get("value");
        match row.get::<String, _>("key").as_str() {
            "claude_model" => slots.claude_model = value,
            "codex_model" => slots.codex_model = value,
            "claude_fallback_model" => slots.claude_fallback_model = Some(value),
            "translate_engine" => slots.translate_engine = value,
            "translate_model" => slots.translate_model = value,
            "focus.ai_engine" => slots.focus_ai_engine = value,
            "focus.ai_model" => slots.focus_ai_model = value,
            _ => {}
        }
    }
    let projects = sqlx::query(
        "SELECT id, name, default_model_claude, default_model_codex, enabled_models FROM projects \
         WHERE deleted_at IS NULL ORDER BY name COLLATE NOCASE, id",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    slots.projects = projects
        .iter()
        .map(|r| ProjectModels {
            id: r.get("id"),
            name: r.get("name"),
            claude: r.get("default_model_claude"),
            codex: r.get("default_model_codex"),
            enabled: r.get::<Option<String>, _>("enabled_models").map(|raw| EnabledModels::parse(Some(&raw))),
        })
        .collect();
    Ok(slots)
}

async fn write_change(conn: &mut SqliteConnection, change: &DefaultChange) -> Result<(), sqlx::Error> {
    match (&change.project_id, change.place.strip_prefix("project.")) {
        (Some(id), Some(column)) => {
            // `column` is one of the two names `reassign_defaults` writes.
            let sql = match column {
                "default_model_claude" => "UPDATE projects SET default_model_claude = ? WHERE id = ?",
                _ => "UPDATE projects SET default_model_codex = ? WHERE id = ?",
            };
            // A project with no model left follows the global default (NULL).
            let to = Some(change.to.as_str()).filter(|v| !v.is_empty());
            sqlx::query(sql).bind(to).bind(id).execute(&mut *conn).await?;
        }
        _ => {
            sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
                .bind(&change.place)
                .bind(&change.to)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Save `value` as the list of models that are on and move the defaults it
/// turns off. `dry_run` only reports the moves. Nothing is written when any
/// write fails.
pub async fn save(db: &Db, value: &str, dry_run: bool) -> Result<Vec<DefaultChange>, String> {
    if !serde_json::from_str::<serde_json::Value>(value).is_ok_and(|v| v.is_object()) {
        return Err(format!("{SETTING} must be a JSON object"));
    }
    let enabled = EnabledModels::parse(Some(value));
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let changes = reassign_defaults(&read_slots(&mut tx).await?, &enabled);
    if dry_run {
        return Ok(changes);
    }
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(SETTING)
        .bind(enabled.to_json())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for change in &changes {
        write_change(&mut tx, change).await.map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    tracing::info!(event = "enabled_models_saved", moved = changes.len());
    Ok(changes)
}

/// Settings › Model "Save" (V3-63). Remote: allowed + audited (`remote::guard`).
#[tauri::command]
pub async fn save_enabled_models(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    value: String,
    dry_run: bool,
) -> Result<Vec<DefaultChange>, String> {
    let changes = save(db.inner(), &value, dry_run).await?;
    if !dry_run {
        super::notify_changed(&app);
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(raw: &str) -> EnabledModels {
        EnabledModels::parse(Some(raw))
    }

    fn change(place: &str, from: &str, to: &str) -> DefaultChange {
        DefaultChange::setting(place, from, to.to_string())
    }

    const SONNET_HAIKU: &str = r#"{"claude":["claude-sonnet-4-6","claude-haiku-4-5"],"codex":null}"#;

    #[test]
    fn each_global_default_moves_to_the_first_model_on() {
        let slots = DefaultSlots {
            claude_model: "claude-opus-4-8".into(),
            codex_model: "gpt-5.4".into(),
            translate_engine: "claude".into(),
            translate_model: "claude-opus-4-8".into(),
            focus_ai_model: "claude-opus-5-5".into(),
            ..Default::default()
        };
        assert_eq!(
            reassign_defaults(&slots, &enabled(r#"{"claude":["claude-sonnet-4-6","claude-haiku-4-5"],"codex":["gpt-5.5"]}"#)),
            vec![
                change("claude_model", "claude-opus-4-8", "claude-sonnet-4-6"),
                change("codex_model", "gpt-5.4", "gpt-5.5"),
                change("translate_model", "claude-opus-4-8", "claude-sonnet-4-6"),
                // A blank Focus engine runs on the translate engine.
                change("focus.ai_model", "claude-opus-5-5", "claude-sonnet-4-6"),
            ]
        );
    }

    #[test]
    fn side_tasks_follow_their_own_engine() {
        let slots = DefaultSlots {
            translate_engine: "codex".into(),
            translate_model: "gpt-5.4".into(),
            focus_ai_engine: "codex".into(),
            focus_ai_model: "gpt-5.5".into(),
            ..Default::default()
        };
        let e = enabled(r#"{"claude":[],"codex":["gpt-5.5"]}"#);
        assert_eq!(reassign_defaults(&slots, &e), vec![change("translate_model", "gpt-5.4", "gpt-5.5")]);
    }

    #[test]
    fn with_no_model_left_the_engine_picks() {
        let slots = DefaultSlots { claude_model: "claude-opus-4-8".into(), ..Default::default() };
        assert_eq!(
            reassign_defaults(&slots, &enabled(r#"{"claude":[]}"#)),
            vec![change("claude_model", "claude-opus-4-8", "")]
        );
    }

    #[test]
    fn empty_alias_and_models_on_stay() {
        let slots = DefaultSlots {
            claude_model: "sonnet".into(),
            codex_model: String::new(),
            translate_model: "claude-sonnet-4-6".into(),
            claude_fallback_model: Some("sonnet".into()),
            ..Default::default()
        };
        assert!(reassign_defaults(&slots, &enabled(SONNET_HAIKU)).is_empty());
        // `null` = every model of that engine is on.
        let slots = DefaultSlots { codex_model: "gpt-5.1-codex-mini".into(), ..Default::default() };
        assert!(reassign_defaults(&slots, &enabled(SONNET_HAIKU)).is_empty());
        assert!(reassign_defaults(&slots, &EnabledModels::default()).is_empty());
    }

    #[test]
    fn the_fallback_goes_back_to_the_sonnet_alias() {
        let e = enabled(SONNET_HAIKU);
        let one = DefaultSlots { claude_fallback_model: Some("claude-opus-4-8".into()), ..Default::default() };
        assert_eq!(reassign_defaults(&one, &e), vec![change("claude_fallback_model", "claude-opus-4-8", "sonnet")]);
        let list = DefaultSlots {
            claude_fallback_model: Some("claude-opus-4-8, claude-haiku-4-5".into()),
            ..Default::default()
        };
        assert_eq!(
            reassign_defaults(&list, &e),
            vec![change("claude_fallback_model", "claude-opus-4-8, claude-haiku-4-5", "claude-haiku-4-5")]
        );
        // Unset or off ("") stays.
        for v in [None, Some(String::new())] {
            assert!(reassign_defaults(&DefaultSlots { claude_fallback_model: v, ..Default::default() }, &e).is_empty());
        }
    }

    #[test]
    fn project_defaults_move_too() {
        let slots = DefaultSlots {
            projects: vec![
                ProjectModels { id: "p1".into(), name: "A".into(), claude: Some("claude-opus-4-8".into()), codex: Some("gpt-5.4".into()), enabled: None },
                ProjectModels { id: "p2".into(), name: "B".into(), claude: None, codex: None, enabled: None },
            ],
            ..Default::default()
        };
        let got = reassign_defaults(&slots, &enabled(r#"{"claude":["claude-haiku-4-5"],"codex":[]}"#));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].place, "project.default_model_claude");
        assert_eq!((got[0].from.as_str(), got[0].to.as_str()), ("claude-opus-4-8", "claude-haiku-4-5"));
        assert_eq!(got[0].project_id.as_deref(), Some("p1"));
        assert_eq!(got[1].place, "project.default_model_codex");
        assert_eq!(got[1].to, "");
    }

    /// V3-64-AC-14: a project with its own list moves to the first model on in both lists.
    #[test]
    fn a_project_with_its_own_list_moves_within_both_lists() {
        let own = enabled(r#"{"claude":["claude-opus-5-5","claude-haiku-4-5","claude-sonnet-4-6"],"codex":["gpt-5.4"]}"#);
        let slots = DefaultSlots {
            projects: vec![ProjectModels {
                id: "p1".into(),
                name: "A".into(),
                claude: Some("claude-opus-5-5".into()),
                codex: Some("gpt-5.4".into()),
                enabled: Some(own),
            }],
            ..Default::default()
        };
        let got = reassign_defaults(&slots, &enabled(r#"{"claude":["claude-sonnet-4-6","claude-haiku-4-5"],"codex":["gpt-5.5"]}"#));
        // Not the global first (sonnet): the project's first that is still on in both.
        assert_eq!((got[0].from.as_str(), got[0].to.as_str()), ("claude-opus-5-5", "claude-haiku-4-5"));
        // Nothing left in both: the project follows Settings again ('' is written as NULL).
        assert_eq!((got[1].from.as_str(), got[1].to.as_str()), ("gpt-5.4", ""));
        assert_eq!(got.len(), 2);
    }

    async fn test_db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO settings (key, value) VALUES ('translate_engine', 'claude'), ('translate_model', 'claude-opus-4-8')",
            "INSERT INTO projects (id, name, path, created_at, default_model_claude) VALUES \
             ('p1', 'a', '/a', 'c', 'claude-opus-4-8'), ('p2', 'b', '/b', 'c', 'claude-opus-4-8')",
            "UPDATE projects SET deleted_at = 'c' WHERE id = 'p2'",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    async fn setting(db: &Db, key: &str) -> Option<String> {
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(db).await.unwrap()
    }

    async fn project_model(db: &Db, id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT default_model_claude FROM projects WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    #[tokio::test]
    async fn dry_run_reports_and_writes_nothing() {
        let db = test_db().await;
        let got = save(&db, SONNET_HAIKU, true).await.unwrap();
        assert_eq!(
            serde_json::to_value(&got[0]).unwrap(),
            serde_json::json!({ "place": "translate_model", "from": "claude-opus-4-8", "to": "claude-sonnet-4-6" })
        );
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(setting(&db, SETTING).await, None);
        assert_eq!(setting(&db, "translate_model").await.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(project_model(&db, "p1").await.as_deref(), Some("claude-opus-4-8"));
    }

    #[tokio::test]
    async fn save_writes_the_list_and_the_moves_together() {
        let db = test_db().await;
        let dry = save(&db, SONNET_HAIKU, true).await.unwrap();
        assert_eq!(save(&db, SONNET_HAIKU, false).await.unwrap(), dry);
        assert_eq!(setting(&db, SETTING).await.as_deref(), Some(EnabledModels::parse(Some(SONNET_HAIKU)).to_json().as_str()));
        assert_eq!(setting(&db, "translate_model").await.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(project_model(&db, "p1").await.as_deref(), Some("claude-sonnet-4-6"));
        // A project in the Trash is left as it is.
        assert_eq!(project_model(&db, "p2").await.as_deref(), Some("claude-opus-4-8"));
        // Nothing left to move the second time.
        assert!(save(&db, SONNET_HAIKU, false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_write_leaves_everything_as_it_was() {
        let db = test_db().await;
        // Make the project write fail after the settings writes went in.
        sqlx::query(
            "CREATE TRIGGER no_model_change BEFORE UPDATE OF default_model_claude ON projects \
             BEGIN SELECT RAISE(ABORT, 'blocked'); END",
        )
        .execute(&db)
        .await
        .unwrap();
        assert!(save(&db, SONNET_HAIKU, false).await.is_err());
        assert_eq!(setting(&db, SETTING).await, None);
        assert_eq!(setting(&db, "translate_model").await.as_deref(), Some("claude-opus-4-8"));
    }

    #[tokio::test]
    async fn a_value_that_is_not_an_object_is_refused() {
        let db = test_db().await;
        for bad in ["", "null", "[]", "nope"] {
            assert!(save(&db, bad, false).await.is_err(), "{bad}");
        }
        assert_eq!(setting(&db, SETTING).await, None);
    }
}
