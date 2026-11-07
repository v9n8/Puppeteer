//! Tauri commands of the feature registry. `get_features` is read-only, so a
//! remote desktop may call it (it hides the same entries as the local app).
//! `set_feature` and `reset_features` are local-only (remote/guard.rs): a
//! Controller must not turn the owner's features on or off.

use super::{apply, default_rows, entry, owner, resolve, turned_off, ModuleState};
use crate::commands::settings::set_setting;
use crate::db::Db;
use std::collections::HashMap;
use tauri::{AppHandle, State};

async fn load(db: &Db) -> Result<Vec<ModuleState>, String> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    let rows: HashMap<String, String> = rows.into_iter().filter(|(k, _)| owner(k).is_some()).collect();
    Ok(resolve(&rows))
}

/// Write one switch (its legacy key when it has one); returns the trees before
/// and after.
async fn set(db: &Db, id: &str, enabled: bool) -> Result<(Vec<ModuleState>, Vec<ModuleState>), String> {
    let (key, locked) = match entry(id) {
        Some((m, None)) => (m.key(), m.locked),
        Some((m, Some(p))) => (p.key(m), p.locked),
        None => return Err(format!("unknown feature: {id}")),
    };
    if locked {
        return Err(format!("feature is locked: {id}"));
    }
    let before = load(db).await?;
    set_setting(db, &key, &enabled.to_string()).await?;
    Ok((before, load(db).await?))
}

/// Write the fresh-install value of every switch; returns the trees before and
/// after.
async fn reset(db: &Db) -> Result<(Vec<ModuleState>, Vec<ModuleState>), String> {
    let before = load(db).await?;
    for (key, value) in default_rows() {
        set_setting(db, &key, &value).await?;
    }
    Ok((before, load(db).await?))
}

/// The resolved module → prop tree.
#[tauri::command]
pub async fn get_features(db: State<'_, Db>) -> Result<Vec<ModuleState>, String> {
    load(db.inner()).await
}

/// Turn a module or prop (full id) on or off; returns the new tree. Locked and
/// unknown ids are refused.
#[tauri::command]
pub async fn set_feature(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
    enabled: bool,
) -> Result<Vec<ModuleState>, String> {
    let (before, after) = set(db.inner(), &id, enabled).await?;
    apply::changed(&app, &before, &after);
    Ok(after)
}

/// Put every switch back to its fresh-install value. Returns the ids this
/// turns off; with `dry_run` nothing is written, so the UI can ask first.
#[tauri::command]
pub async fn reset_features(app: AppHandle, db: State<'_, Db>, dry_run: bool) -> Result<Vec<String>, String> {
    if dry_run {
        return Ok(turned_off(&load(db.inner()).await?, &resolve(&default_rows())));
    }
    let (before, after) = reset(db.inner()).await?;
    apply::changed(&app, &before, &after);
    Ok(turned_off(&before, &after))
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

    async fn value(db: &Db, key: &str) -> Option<String> {
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(db)
            .await
            .unwrap()
    }

    fn effective(tree: &[ModuleState], id: &str) -> bool {
        tree.iter().flat_map(|m| &m.props).find(|p| p.id == id).unwrap().effective
    }

    #[tokio::test]
    async fn locked_and_unknown_ids_are_refused() {
        let db = db().await;
        let count = || sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM settings").fetch_one(&db);
        let rows = count().await.unwrap();
        for id in ["sessions", "sessions.core", "library.skills", "code_hosting.issue_pr_runs"] {
            assert_eq!(set(&db, id, false).await.unwrap_err(), format!("feature is locked: {id}"));
        }
        for id in ["nope", "feature.remote", "text_tools.selection", "inbox_api_enabled"] {
            assert_eq!(set(&db, id, true).await.unwrap_err(), format!("unknown feature: {id}"));
        }
        assert_eq!(count().await.unwrap(), rows, "a refused write leaves no row");
    }

    #[tokio::test]
    async fn set_writes_the_feature_key_and_returns_both_trees() {
        let db = db().await;
        let (before, after) = set(&db, "text_tools.selection.translate", false).await.unwrap();
        assert_eq!(value(&db, "feature.text_tools.selection.translate").await.as_deref(), Some("false"));
        assert!(effective(&before, "text_tools.selection.translate"));
        assert!(!effective(&after, "text_tools.selection.translate"));

        set(&db, "remote", true).await.unwrap();
        assert_eq!(value(&db, "feature.remote").await.as_deref(), Some("true"));
    }

    #[tokio::test]
    async fn set_writes_a_legacy_key_instead_of_the_feature_key() {
        let db = db().await;
        let (_, after) = set(&db, "integrations.inbox_api", true).await.unwrap();
        assert_eq!(value(&db, "inbox_api_enabled").await.as_deref(), Some("true"));
        assert_eq!(value(&db, "feature.integrations.inbox_api").await, None);
        assert!(effective(&after, "integrations.inbox_api"));

        set(&db, "app.animated_background", false).await.unwrap();
        assert_eq!(value(&db, "animated_background").await.as_deref(), Some("false"));
    }

    #[tokio::test]
    async fn reset_writes_fresh_install_values_including_legacy_keys() {
        let db = db().await;
        for (key, value) in [
            ("feature.remote", "true"),
            ("feature.productivity.todos", "true"),
            ("feature.productivity", "false"),
            ("inbox_api_enabled", "true"),
            ("animated_background", "false"),
            ("theme", "dark"),
        ] {
            set_setting(&db, key, value).await.unwrap();
        }
        let before = load(&db).await.unwrap();
        // The dry run's answer, computed before anything is written.
        let preview = turned_off(&before, &resolve(&default_rows()));

        let (seen_before, after) = reset(&db).await.unwrap();
        assert_eq!(seen_before, before);
        let fresh = resolve(&HashMap::new());
        assert!(turned_off(&after, &fresh).is_empty() && turned_off(&fresh, &after).is_empty());
        assert_eq!(turned_off(&before, &after), preview);
        assert!(preview.contains(&"remote".to_string()));
        assert!(preview.contains(&"integrations.inbox_api".to_string()));
        assert!(!preview.contains(&"productivity.todos".to_string()), "already off: module was off");

        assert_eq!(value(&db, "feature.remote").await.as_deref(), Some("false"));
        assert_eq!(value(&db, "feature.productivity").await.as_deref(), Some("true"));
        assert_eq!(value(&db, "feature.productivity.todos").await.as_deref(), Some("false"));
        assert_eq!(value(&db, "inbox_api_enabled").await.as_deref(), Some("false"));
        assert_eq!(value(&db, "animated_background").await.as_deref(), Some("true"));
        assert_eq!(value(&db, "theme").await.as_deref(), Some("dark"), "other settings untouched");
        assert_eq!(value(&db, "feature.sessions").await, None, "locked entries are not written");
    }
}
