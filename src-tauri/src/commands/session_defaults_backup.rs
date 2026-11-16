//! A project's list of models (Project › Model, V3-64) in a settings backup. Kept out of
//! `settings_backup.rs`, which only carries the field and calls [`merged_list`]; declared
//! there as a child module so the round-trip test can use its private collect / import.
//!
//! The list travels with the project's other defaults for new sessions. A backup without
//! it (older, or a project following Settings) keeps the local list, like every other
//! default left unset there.

use crate::commands::session_defaults::{check_model, ProjectSessionDefaults};
use crate::model_policy::EnabledModels;

/// The list a project ends up with on import: the backup's (trimmed, ids of the wrong
/// engine left out with a warning), else the local one.
pub(super) fn merged_list(
    backup: Option<&EnabledModels>,
    current: &ProjectSessionDefaults,
    warnings: &mut Vec<String>,
) -> Option<EnabledModels> {
    let Some(backup) = backup else { return current.enabled_models.clone() };
    let backup = EnabledModels::parse(Some(&backup.to_json()));
    let mut keep = |engine: &str| -> Option<Vec<String>> {
        let ids = backup.list(engine)?;
        let ok = |id: &&String| match check_model(engine, Some(id.to_string())) {
            Ok(_) => true,
            Err(e) => {
                warnings.push(format!("model list: {e}, left out"));
                false
            }
        };
        Some(ids.iter().filter(ok).cloned().collect())
    };
    Some(EnabledModels { claude: keep("claude"), codex: keep("codex") })
}

#[cfg(test)]
mod tests {
    use super::super::{collect_projects, import_project, ProjectEntry, Upsert};
    use super::*;
    use crate::commands::session_defaults::{self as defaults};
    use crate::db::Db;

    async fn db_with_projects(rows: &str) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query(&format!("INSERT INTO projects (id, name, path, created_at) VALUES {rows}")).execute(&db).await.unwrap();
        db
    }

    fn list(raw: &str) -> Option<EnabledModels> {
        Some(EnabledModels::parse(Some(raw)))
    }

    #[test]
    fn a_backup_without_a_list_keeps_the_local_one() {
        let local = ProjectSessionDefaults { enabled_models: list(r#"{"codex":["gpt-5.5"]}"#), ..Default::default() };
        let mut warnings = Vec::new();
        assert_eq!(merged_list(None, &local, &mut warnings), local.enabled_models);
        let got = merged_list(list(r#"{"claude":["claude-haiku-4-5","gpt-5.5"]}"#).as_ref(), &local, &mut warnings);
        assert_eq!(got, list(r#"{"claude":["claude-haiku-4-5"]}"#));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[tokio::test]
    async fn the_list_of_each_project_round_trips() {
        let source = db_with_projects("('p1', 'web', '/src/web', 'c'), ('p2', 'api', '/src/api', 'c')").await;
        let web = ProjectSessionDefaults {
            model_claude: Some("claude-haiku-4-5".into()),
            enabled_models: list(r#"{"claude":["claude-haiku-4-5","claude-sonnet-4-6"],"codex":[]}"#),
            ..Default::default()
        };
        defaults::set(&source, "p1", web.clone()).await.unwrap();
        // `api` follows Settings: its entry carries no list.
        let projects: Vec<ProjectEntry> =
            serde_json::from_value(serde_json::to_value(collect_projects(&source).await.unwrap()).unwrap()).unwrap();
        let by_name = |n: &str| projects.iter().find(|p| p.name == n).unwrap();
        assert!(by_name("api").session_defaults.is_none());
        let json = serde_json::to_value(by_name("web")).unwrap();
        assert_eq!(
            json["session_defaults"]["enabled_models"],
            serde_json::json!({ "claude": ["claude-haiku-4-5", "claude-sonnet-4-6"], "codex": [] })
        );

        // Another machine: `api` has its own list, which the backup does not touch.
        let target = db_with_projects("('w', 'web', '/other/web', 'c'), ('a', 'api', '/other/api', 'c')").await;
        let api_own = ProjectSessionDefaults { enabled_models: list(r#"{"codex":["gpt-5.5"]}"#), ..Default::default() };
        defaults::set(&target, "a", api_own.clone()).await.unwrap();
        for name in ["web", "api"] {
            let (outcome, _, warnings) = import_project(&target, by_name(name), false, None).await.unwrap();
            assert!(!warnings.iter().any(|w| w.contains("model")), "{warnings:?}");
            assert_ne!(outcome, Upsert::Created, "{name}");
        }
        assert_eq!(defaults::load(&target, "w").await, web);
        assert_eq!(defaults::load(&target, "a").await, api_own);

        // A backup from before V3-64 (defaults, no list) keeps the local list too.
        let old: ProjectEntry = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "api", "path": "/other/api", "default_engine": "claude", "created_at": "c",
            "session_defaults": { "effort": "low" },
        }))
        .unwrap();
        import_project(&target, &old, false, None).await.unwrap();
        let after = defaults::load(&target, "a").await;
        assert_eq!((after.effort.as_deref(), after.enabled_models), (Some("low"), api_own.enabled_models));
    }
}
