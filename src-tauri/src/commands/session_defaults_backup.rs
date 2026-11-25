//! A project's list of models (Project › Model, V3-64) in a settings backup. Kept out of
//! `settings_backup.rs`, which only carries the field and calls [`merged_list`]; declared
//! there as a child module so the round-trip test can use its private collect / import.
//!
//! The list travels with the project's other defaults for new sessions. A project
//! following Settings is written as an explicit `null`, so importing it puts the target
//! project back on Settings too. A backup without the field (an older one) keeps the
//! local list, like every other default left unset there.

use super::SessionDefaultsEntry;
use crate::commands::session_defaults::{self as defaults, check_model, GlobalDefaults, ProjectSessionDefaults};
use crate::db::Db;
use crate::model_policy::EnabledModels;

/// The field in a backup entry: outer `None` = absent (older backup), `Some(None)` = `null`
/// (follows Settings), `Some(Some(list))` = the project's own list.
pub(super) type List = Option<Option<EnabledModels>>;

/// serde hook that tells `null` from an absent field (a plain `Option` reads both as `None`).
pub(super) fn de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<List, D::Error> {
    serde::Deserialize::deserialize(d).map(Some)
}

/// The list a project ends up with on import: the backup's (trimmed, ids of the wrong
/// engine left out with a warning), Settings for `null`, else the local one.
pub(super) fn merged_list(
    backup: Option<&Option<EnabledModels>>,
    current: &ProjectSessionDefaults,
    warnings: &mut Vec<String>,
) -> Option<EnabledModels> {
    let Some(backup) = backup else { return current.enabled_models.clone() };
    let Some(backup) = backup else { return None };
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

/// After the import (settings first, then projects and styles): a project default the
/// backup itself set that now equals the global setting in force becomes NULL, so the
/// project follows the global setting instead of a pinned copy. A local value the backup
/// did not set stays as it is.
pub(super) async fn follow_globals(db: &Db, project_id: &str, entry: &SessionDefaultsEntry) -> Result<(), String> {
    let current = defaults::load(db, project_id).await;
    // Set by the backup: the stored value is the backup's (a value left out keeps the local one).
    let from_backup = |backup: &Option<String>, stored: &Option<String>| {
        backup.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_some_and(|v| stored.as_deref() == Some(v))
    };
    let set_by_backup = [
        ("default_engine_override", from_backup(&entry.engine, &current.engine)),
        ("default_model_claude", from_backup(&entry.model_claude, &current.model_claude)),
        ("default_model_codex", from_backup(&entry.model_codex, &current.model_codex)),
        ("default_permission_mode", from_backup(&entry.permission_mode, &current.permission_mode)),
        ("default_effort", from_backup(&entry.effort, &current.effort)),
        // The style travels by id or name and is stored under the local id.
        ("default_style", entry.style.is_some()),
    ];
    let stored = [&current.engine, &current.model_claude, &current.model_codex, &current.permission_mode, &current.effort, &current.style];
    let global = GlobalDefaults::load(db).await;
    for ((column, set), value) in set_by_backup.into_iter().zip(stored) {
        let Some(value) = value.as_deref().filter(|_| set) else { continue };
        if global.follows(column, value) {
            // `column` is one of the fixed names above.
            sqlx::query(&format!("UPDATE projects SET {column} = NULL WHERE id = ?"))
                .bind(project_id)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
        assert_eq!(merged_list(Some(&None), &local, &mut warnings), None);
        let got = merged_list(Some(&list(r#"{"claude":["claude-haiku-4-5","gpt-5.5"]}"#)), &local, &mut warnings);
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
        // `api` follows Settings: its entry carries an explicit null.
        let projects: Vec<ProjectEntry> =
            serde_json::from_value(serde_json::to_value(collect_projects(&source).await.unwrap()).unwrap()).unwrap();
        let by_name = |n: &str| projects.iter().find(|p| p.name == n).unwrap();
        assert_eq!(serde_json::to_value(by_name("api")).unwrap()["session_defaults"], serde_json::json!({ "enabled_models": null }));
        let json = serde_json::to_value(by_name("web")).unwrap();
        assert_eq!(
            json["session_defaults"]["enabled_models"],
            serde_json::json!({ "claude": ["claude-haiku-4-5", "claude-sonnet-4-6"], "codex": [] })
        );

        // Another machine: `api` has its own list, which the backup of a project on Settings resets.
        let target = db_with_projects("('w', 'web', '/other/web', 'c'), ('a', 'api', '/other/api', 'c')").await;
        let api_own = ProjectSessionDefaults { enabled_models: list(r#"{"codex":["gpt-5.5"]}"#), ..Default::default() };
        defaults::set(&target, "a", api_own.clone()).await.unwrap();
        for name in ["web", "api"] {
            let (outcome, _, warnings) = import_project(&target, by_name(name), false, None).await.unwrap();
            assert!(!warnings.iter().any(|w| w.contains("model")), "{warnings:?}");
            assert_ne!(outcome, Upsert::Created, "{name}");
        }
        assert_eq!(defaults::load(&target, "w").await, web);
        assert_eq!(defaults::load(&target, "a").await.enabled_models, None);
        defaults::set(&target, "a", api_own.clone()).await.unwrap();

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

    async fn set_setting(db: &Db, key: &str, value: &str) {
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)").bind(key).bind(value).execute(db).await.unwrap();
    }

    /// Import `entry` into project `id` of `db` the way `import_bundle` does: the project, then the follow pass.
    async fn import_and_follow(db: &Db, id: &str, entry: &ProjectEntry) -> ProjectSessionDefaults {
        import_project(db, entry, false, None).await.unwrap();
        follow_globals(db, id, entry.session_defaults.as_ref().unwrap()).await.unwrap();
        defaults::load(db, id).await
    }

    fn entry(defaults: serde_json::Value) -> ProjectEntry {
        serde_json::from_value(serde_json::json!({
            "id": "x", "name": "web", "path": "/other/web", "default_engine": "claude", "created_at": "c",
            "session_defaults": defaults,
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn an_imported_default_equal_to_the_global_one_follows_it_and_another_stays() {
        let db = db_with_projects("('w', 'web', '/other/web', 'c')").await;
        set_setting(&db, "claude_model", "claude-opus-5-5[1m]").await;
        set_setting(&db, "default_effort", "high").await;
        let got = import_and_follow(&db, "w", &entry(serde_json::json!({
            "model_claude": "claude-opus-5-5[1m]", "engine": "claude", "effort": "low",
        })))
        .await;
        // Same as the global model, and the engine a blank global engine means: both follow.
        assert_eq!((got.model_claude, got.engine), (None, None));
        assert_eq!(got.effort.as_deref(), Some("low"));

        let got = import_and_follow(&db, "w", &entry(serde_json::json!({ "model_claude": "claude-sonnet-4-6" }))).await;
        assert_eq!(got.model_claude.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[tokio::test]
    async fn a_local_pinned_default_the_backup_does_not_set_stays_pinned() {
        let db = db_with_projects("('w', 'web', '/other/web', 'c')").await;
        set_setting(&db, "codex_model", "gpt-5.5").await;
        let local = ProjectSessionDefaults { model_codex: Some("gpt-5.5".into()), ..Default::default() };
        defaults::set(&db, "w", local).await.unwrap();
        let got = import_and_follow(&db, "w", &entry(serde_json::json!({ "effort": "low" }))).await;
        assert_eq!((got.model_codex.as_deref(), got.effort.as_deref()), (Some("gpt-5.5"), Some("low")));
    }

    #[tokio::test]
    async fn a_project_following_every_default_exports_none_and_still_follows_after_import() {
        let source = db_with_projects("('p1', 'web', '/src/web', 'c')").await;
        let projects: Vec<ProjectEntry> =
            serde_json::from_value(serde_json::to_value(collect_projects(&source).await.unwrap()).unwrap()).unwrap();
        // Only the list's explicit null (follows Settings); none of the six defaults.
        assert_eq!(serde_json::to_value(&projects[0]).unwrap()["session_defaults"], serde_json::json!({ "enabled_models": null }));
        let target = db_with_projects("('w', 'web', '/other/web', 'c')").await;
        set_setting(&target, "claude_model", "claude-opus-5-5").await;
        assert_eq!(import_and_follow(&target, "w", &projects[0]).await, ProjectSessionDefaults::default());
    }
}
