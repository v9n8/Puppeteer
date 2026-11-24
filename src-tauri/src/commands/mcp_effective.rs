//! Whether an MCP server is on, across the levels that switch it: app-wide >
//! project > session. A level never rewrites the stored value of the one below:
//! the value that counts is computed (parent AND child), so switching a parent
//! back on brings the child's own choice back. A child may not be switched on
//! while its parent is off.
//!
//! The project setters (`mcp::set_project_*`), the session menu (`run_mcp`) and
//! the Slack token lookup read the rule here, so it lives in one place.

use crate::commands::mcp::builtin_mcp_globally_enabled;
use crate::db::Db;
use crate::features::ModuleState;
use std::collections::HashSet;

/// Whether built-in `name` is on app-wide, the way the run launch reads it
/// (`mcp::builtin_gates`): `puppeteer` is the `library.builtin_mcp_puppeteer`
/// feature; `google` needs both the `integrations.google` feature and its MCP switch.
pub async fn builtin_on_app_wide(db: &Db, tree: &[ModuleState], name: &str) -> bool {
    match name {
        "puppeteer" => crate::features::effective(tree, "library.builtin_mcp_puppeteer"),
        "google" => {
            crate::features::effective(tree, "integrations.google")
                && builtin_mcp_globally_enabled(db, name).await
        }
        _ => builtin_mcp_globally_enabled(db, name).await,
    }
}

/// [`builtin_on_app_wide`] on the feature switches stored now. A failed read
/// counts as off, so the caller refuses rather than enables.
pub async fn builtin_on_app_wide_now(db: &Db, name: &str) -> bool {
    match crate::features::commands::load(db).await {
        Ok(tree) => builtin_on_app_wide(db, &tree, name).await,
        Err(e) => {
            tracing::warn!(event = "mcp_effective_features_load_failed", name = %name, error = %e);
            false
        }
    }
}

/// Ids of the user servers effectively on: switched on app-wide and, for `Some(project)`,
/// switched on for that project.
pub async fn servers_on(db: &Db, project: Option<&str>) -> Result<HashSet<String>, sqlx::Error> {
    let ids: Vec<String> = match project {
        None => sqlx::query_scalar("SELECT id FROM mcp_servers WHERE enabled = 1").fetch_all(db).await?,
        Some(pid) => {
            sqlx::query_scalar(
                "SELECT s.id FROM mcp_servers s JOIN project_mcp_servers ps ON ps.server_id = s.id \
                 WHERE s.enabled = 1 AND ps.project_id = ?",
            )
            .bind(pid)
            .fetch_all(db)
            .await?
        }
    };
    Ok(ids.into_iter().collect())
}

/// Err unless user server `id` is switched on app-wide (`mcp_servers.enabled`).
pub async fn require_server_on_app_wide(db: &Db, id: &str) -> Result<(), String> {
    let row: Option<(String, bool)> = sqlx::query_as("SELECT name, enabled FROM mcp_servers WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    match row {
        None => Err(format!("unknown MCP server: {id}")),
        Some((name, false)) => Err(format!(
            "MCP server '{name}' is off app-wide — turn it on in Settings › Connections › MCP"
        )),
        Some((_, true)) => Ok(()),
    }
}

/// For a project's full list (`set_project_mcp_servers`): each id not yet stored
/// for the project must be on app-wide. Ids already stored are kept even while
/// off app-wide, so saving the list never drops a choice the parent only hides.
pub async fn require_added_servers_on_app_wide(db: &Db, project_id: &str, ids: &[String]) -> Result<(), String> {
    let stored: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT server_id FROM project_mcp_servers WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
    for id in ids.iter().filter(|id| !stored.contains(*id)) {
        require_server_on_app_wide(db, id).await?;
    }
    Ok(())
}

/// Err unless built-in `name` is on app-wide ([`builtin_on_app_wide_now`]).
pub async fn require_builtin_on_app_wide(db: &Db, name: &str) -> Result<(), String> {
    if builtin_on_app_wide_now(db, name).await {
        Ok(())
    } else {
        Err(format!("built-in MCP server '{name}' is off app-wide — turn it on in Settings first"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::mcp::{set_project_builtin_mcp, set_project_mcp_server_enabled};
    use crate::commands::settings::set_setting;

    async fn test_db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')",
            "INSERT INTO mcp_servers (id, name, transport, url, enabled, created_at) VALUES
                ('on', 'alpha', 'http', 'https://a', 1, 'c'),
                ('off', 'gamma', 'http', 'https://g', 0, 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    async fn stored(db: &Db) -> Vec<String> {
        sqlx::query_scalar("SELECT server_id FROM project_mcp_servers WHERE project_id = 'p1' ORDER BY server_id")
            .fetch_all(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_server_off_app_wide_cannot_be_switched_on_for_a_project() {
        let db = test_db().await;
        let err = set_project_mcp_server_enabled(&db, "p1", "off", true).await.unwrap_err();
        assert!(err.contains("'gamma' is off app-wide"), "{err}");
        let err = require_added_servers_on_app_wide(&db, "p1", &["on".into(), "off".into()]).await.unwrap_err();
        assert!(err.contains("off app-wide"), "{err}");
        assert!(stored(&db).await.is_empty());
        // A server on app-wide is fine, and switching off is never refused.
        set_project_mcp_server_enabled(&db, "p1", "on", true).await.unwrap();
        set_project_mcp_server_enabled(&db, "p1", "off", false).await.unwrap();
        assert_eq!(stored(&db).await, ["on"]);
        assert!(require_server_on_app_wide(&db, "nope").await.is_err());
    }

    #[tokio::test]
    async fn the_full_list_keeps_a_stored_server_that_is_off_app_wide() {
        let db = test_db().await;
        // Stored while it was on, then switched off app-wide.
        sqlx::query("INSERT INTO project_mcp_servers (project_id, server_id, enabled_at) VALUES ('p1', 'off', 'c')")
            .execute(&db)
            .await
            .unwrap();
        require_added_servers_on_app_wide(&db, "p1", &["off".into(), "on".into()]).await.unwrap();
    }

    #[tokio::test]
    async fn switching_the_app_wide_parent_back_on_brings_the_project_value_back() {
        let db = test_db().await;
        set_project_mcp_server_enabled(&db, "p1", "on", true).await.unwrap();
        sqlx::query("UPDATE mcp_servers SET enabled = 0 WHERE id = 'on'").execute(&db).await.unwrap();
        assert!(!crate::commands::run_mcp::run_uses_server(&db, "p1", "r1", "on").await.unwrap());
        assert_eq!(stored(&db).await, ["on"]);
        sqlx::query("UPDATE mcp_servers SET enabled = 1 WHERE id = 'on'").execute(&db).await.unwrap();
        assert!(crate::commands::run_mcp::run_uses_server(&db, "p1", "r1", "on").await.unwrap());
    }

    #[tokio::test]
    async fn the_google_built_in_needs_its_feature_as_well_as_its_switch() {
        let db = test_db().await;
        set_setting(&db, "mcp_builtin_google_enabled", "true").await.unwrap();
        // `integrations.google` is off by default.
        assert!(!builtin_on_app_wide_now(&db, "google").await);
        set_project_builtin_mcp(&db, "p1", "google", false).await.unwrap();
        let err = set_project_builtin_mcp(&db, "p1", "google", true).await.unwrap_err();
        assert!(err.contains("off app-wide"), "{err}");

        set_setting(&db, "feature.integrations.google", "true").await.unwrap();
        assert!(builtin_on_app_wide_now(&db, "google").await);
        set_project_builtin_mcp(&db, "p1", "google", true).await.unwrap();

        // The puppeteer built-in follows its feature prop.
        assert!(builtin_on_app_wide_now(&db, "puppeteer").await);
        set_setting(&db, "mcp_builtin_puppeteer_enabled", "false").await.unwrap();
        assert!(!builtin_on_app_wide_now(&db, "puppeteer").await);
        assert!(set_project_builtin_mcp(&db, "p1", "puppeteer", true).await.is_err());
    }
}
