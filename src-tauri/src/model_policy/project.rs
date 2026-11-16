//! V3-64: a project's own list of models (Project › Model), on top of Settings › Model.
//!
//! `projects.enabled_models` (migration 0024) holds the same JSON as the global setting;
//! NULL = the project follows Settings › Model. A run of a project that has a list may use
//! the models on in BOTH lists ([`intersect`]): a model turned off in Settings is off in
//! every project without touching their lists, and is back once it is turned on again.

use std::collections::BTreeMap;

use sqlx::Row;

use super::{EnabledModels, ModelBlocked};
use crate::db::Db;
use crate::remote::protocol::EnabledModelsWire;

/// The models on for a run of a project: the global list narrowed by the project's.
/// Per engine, `None` (every model) on one side gives the other side's list; with two
/// lists the project's order is kept, so its first id is where a moved default goes.
pub fn intersect(global: &EnabledModels, project: Option<&EnabledModels>) -> EnabledModels {
    let Some(project) = project else { return global.clone() };
    let one = |g: &Option<Vec<String>>, p: &Option<Vec<String>>| match (g, p) {
        (None, p) => p.clone(),
        (g, None) => g.clone(),
        (Some(g), Some(p)) => Some(p.iter().filter(|id| g.contains(id)).cloned().collect()),
    };
    EnabledModels { claude: one(&global.claude, &project.claude), codex: one(&global.codex, &project.codex) }
}

/// A project's name and own list; `None` when it follows Settings › Model, is gone,
/// or cannot be read (a start never fails over reading it).
pub async fn load(db: &Db, project_id: &str) -> Option<(String, EnabledModels)> {
    let row = sqlx::query("SELECT name, enabled_models FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(event = "project_enabled_models_read_failed", project_id, error = %e);
            None
        })?;
    let raw: Option<String> = row.get("enabled_models");
    raw.map(|raw| (row.get("name"), EnabledModels::parse(Some(&raw))))
}

/// The error of a start refused by the project's list: the same codes as
/// [`super::blocked_message`], the English naming the project (the UI reads it
/// from `for project "<name>"`).
pub fn blocked_message(blocked: &ModelBlocked, from_default: bool, project: &str) -> String {
    let m = &blocked.model;
    if from_default {
        format!(
            "model_default_disabled:{m}: Model {m} is the default model for new sessions but is turned off for project \"{project}\" (Project › Model). \
             Change it in the project's defaults for new sessions."
        )
    } else {
        format!(
            "model_disabled:{m}: Model {m} is turned off for project \"{project}\" (Project › Model). Turn it on there or pick another model."
        )
    }
}

/// The lists of the projects that have one, for a phone's `engine_model_options`
/// frame (`project_enabled_models`); projects following Settings are left out.
pub async fn wire_projects(db: &Db) -> BTreeMap<String, EnabledModelsWire> {
    let rows = sqlx::query("SELECT id, enabled_models FROM projects WHERE deleted_at IS NULL AND enabled_models IS NOT NULL")
        .fetch_all(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(event = "project_enabled_models_read_failed", error = %e);
            Vec::new()
        });
    rows.iter()
        .map(|r| {
            let e = EnabledModels::parse(r.get::<Option<String>, _>("enabled_models").as_deref());
            (r.get("id"), EnabledModelsWire { claude: e.claude, codex: e.codex })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{check_model, ensure_allowed, first_enabled};
    use super::*;

    fn only(claude: Option<&[&str]>, codex: Option<&[&str]>) -> EnabledModels {
        let v = |l: Option<&[&str]>| l.map(|l| l.iter().map(|s| s.to_string()).collect());
        EnabledModels { claude: v(claude), codex: v(codex) }
    }

    #[test]
    fn intersect_narrows_the_global_list() {
        let global = only(Some(&["claude-opus-5-5", "claude-sonnet-4-6", "claude-haiku-4-5"]), None);
        // Following Settings: the global list as is.
        assert_eq!(intersect(&global, None), global);
        // An engine the project leaves at null follows the global list of that engine.
        let p = only(Some(&["claude-haiku-4-5", "claude-opus-4-8", "claude-sonnet-4-6"]), Some(&["gpt-5.5"]));
        // Only ids on in both; the project's order; Codex all on globally gives the project's.
        assert_eq!(intersect(&global, Some(&p)), only(Some(&["claude-haiku-4-5", "claude-sonnet-4-6"]), Some(&["gpt-5.5"])));
        assert_eq!(
            intersect(&global, Some(&only(None, Some(&[])))),
            only(Some(&["claude-opus-5-5", "claude-sonnet-4-6", "claude-haiku-4-5"]), Some(&[]))
        );
        // A project with every model off leaves only the engine's own pick.
        let none = intersect(&global, Some(&only(Some(&[]), Some(&[]))));
        assert!(check_model("claude", Some("claude-sonnet-4-6"), None, &none).is_err());
        assert!(check_model("claude", Some("sonnet"), None, &none).is_ok());
        assert_eq!(first_enabled("claude", &none), "");
    }

    #[test]
    fn a_model_off_in_settings_is_off_in_the_project_until_it_is_back() {
        let p = only(Some(&["claude-opus-5-5", "claude-haiku-4-5"]), None);
        let off = intersect(&only(Some(&["claude-haiku-4-5"]), None), Some(&p));
        assert!(check_model("claude", Some("claude-opus-5-5"), None, &off).is_err());
        assert_eq!(first_enabled("claude", &off), "claude-haiku-4-5");
        let back = intersect(&EnabledModels::default(), Some(&p));
        assert!(check_model("claude", Some("claude-opus-5-5"), None, &back).is_ok());
        assert_eq!(first_enabled("claude", &back), "claude-opus-5-5");
    }

    async fn db_with(global: Option<&str>, project: Option<&str>) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        if let Some(v) = global {
            crate::commands::settings::set_setting(&db, super::super::SETTING, v).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO projects (id, name, path, created_at, enabled_models) VALUES ('p1', 'Client', '/a', 'c', ?), ('p2', 'Mine', '/b', 'c', NULL)",
        )
        .bind(project)
        .execute(&db)
        .await
        .unwrap();
        db
    }

    #[tokio::test]
    async fn a_start_in_the_project_is_refused_a_model_off_there() {
        let db = db_with(None, Some(r#"{"claude":["claude-sonnet-4-6"],"codex":null}"#)).await;
        let picked = ensure_allowed(&db, "p1", "claude", Some("claude-opus-4-8"), None, Some("claude-opus-4-8")).await.unwrap_err();
        assert!(picked.starts_with("model_disabled:claude-opus-4-8: "), "{picked}");
        assert!(picked.contains("for project \"Client\""), "{picked}");
        let default = ensure_allowed(&db, "p1", "claude", Some("claude-opus-4-8"), None, None).await.unwrap_err();
        assert!(default.starts_with("model_default_disabled:claude-opus-4-8: "), "{default}");
        // On in the project, another project, a resume keeping its model, the other engine.
        assert!(ensure_allowed(&db, "p1", "claude", Some("claude-sonnet-4-6"), None, None).await.is_ok());
        assert!(ensure_allowed(&db, "p2", "claude", Some("claude-opus-4-8"), None, None).await.is_ok());
        assert!(ensure_allowed(&db, "p1", "claude", Some("claude-opus-4-8"), Some("claude-opus-4-8"), None).await.is_ok());
        assert!(ensure_allowed(&db, "p1", "codex", Some("gpt-5.4"), None, None).await.is_ok());
    }

    #[tokio::test]
    async fn the_global_refusal_comes_first() {
        let db = db_with(Some(r#"{"claude":["claude-haiku-4-5"]}"#), Some(r#"{"claude":["claude-sonnet-4-6"]}"#)).await;
        let e = ensure_allowed(&db, "p1", "claude", Some("claude-sonnet-4-6"), None, Some("claude-sonnet-4-6")).await.unwrap_err();
        assert!(e.contains("in Settings › Model"), "{e}");
        // On globally but not in the project's list.
        let e = ensure_allowed(&db, "p1", "claude", Some("claude-haiku-4-5"), None, Some("claude-haiku-4-5")).await.unwrap_err();
        assert!(e.contains("for project \"Client\""), "{e}");
    }

    #[tokio::test]
    async fn the_phone_gets_only_the_projects_with_a_list() {
        assert!(wire_projects(&db_with(None, None).await).await.is_empty());
        let w = wire_projects(&db_with(None, Some(r#"{"codex":["gpt-5.5"]}"#)).await).await;
        assert_eq!(w.len(), 1);
        assert_eq!(w["p1"], EnabledModelsWire { claude: None, codex: Some(vec!["gpt-5.5".into()]) });
        let (name, own) = load(&db_with(None, Some("{}")).await, "p1").await.unwrap();
        assert_eq!((name.as_str(), own), ("Client", EnabledModels::default()));
    }
}
