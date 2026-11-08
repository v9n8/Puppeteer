//! One-time seed of the feature switches, run at startup right after the
//! migrations and before anything else reads or writes settings.
//!
//! - Fresh install (no project yet): the registry defaults apply as they are.
//!   Only `inbox_api_enabled` is written (`"false"`), because builds before
//!   the registry read a missing key as on.
//! - Upgrade: every switch that can be turned off and has no stored value is
//!   written `"true"`, so the user keeps everything they had. Stored values,
//!   such as a legacy key set to `"false"`, are kept.
//!
//! Props registered later have no row, so they get the registry default.

use super::default_rows;
use crate::db::Db;

/// Set once the seed has run; it never runs again.
pub const SEEDED_KEY: &str = "feature._seeded";

pub async fn seed(db: &Db) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    let seeded: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(SEEDED_KEY)
        .fetch_optional(&mut *tx)
        .await?;
    if seeded.is_some() {
        return Ok(());
    }
    // Migration 0001 already inserts settings rows, so a fresh install is told
    // apart by its projects.
    let projects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects").fetch_one(&mut *tx).await?;
    let (keys, value): (Vec<String>, &str) = if projects == 0 {
        (vec![crate::inbox_api::ENABLED_SETTING_KEY.to_string()], "false")
    } else {
        (default_rows().into_keys().collect(), "true")
    };
    for key in keys {
        sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, '1')")
        .bind(SEEDED_KEY)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::set_setting;
    use crate::features::{resolve, turned_off, ModuleState};
    use std::collections::HashMap;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    async fn add_project(db: &Db) {
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', '2026-01-01')")
            .execute(db)
            .await
            .unwrap();
    }

    async fn rows(db: &Db) -> HashMap<String, String> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM settings").fetch_all(db).await.unwrap();
        rows.into_iter().collect()
    }

    fn same(a: &[ModuleState], b: &[ModuleState]) -> bool {
        turned_off(a, b).is_empty() && turned_off(b, a).is_empty()
    }

    #[tokio::test]
    async fn fresh_install_keeps_the_defaults_and_closes_the_inbox_api() {
        let db = db().await;
        let before = rows(&db).await;
        seed(&db).await.unwrap();
        let after = rows(&db).await;

        assert!(same(&resolve(&after), &resolve(&HashMap::new())));
        assert!(!crate::features::is_on(&db, crate::inbox_api::server::FEATURE).await);
        assert_eq!(after.get(SEEDED_KEY).map(String::as_str), Some("1"));
        // Only the Inbox API switch and the flag were added.
        let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
        assert_eq!(added.len(), 2, "{added:?}");
    }

    #[tokio::test]
    async fn upgrade_turns_everything_on_and_keeps_stored_values() {
        let db = db().await;
        add_project(&db).await;
        set_setting(&db, "mcp_builtin_puppeteer_enabled", "false").await.unwrap();
        set_setting(&db, "mcp_builtin_google_enabled", "false").await.unwrap();
        seed(&db).await.unwrap();
        let r = rows(&db).await;
        let tree = resolve(&r);

        assert!(tree.iter().all(|m| m.enabled), "every module on, remote too");
        for p in tree.iter().flat_map(|m| &m.props) {
            assert_eq!(p.effective, p.id != "library.builtin_mcp_puppeteer", "{}", p.id);
        }
        assert_eq!(r.get("mcp_builtin_puppeteer_enabled").map(String::as_str), Some("false"));
        // Not a registry key: the MCP page switch stays off.
        assert_eq!(r.get("mcp_builtin_google_enabled").map(String::as_str), Some("false"));
        assert!(crate::features::is_on(&db, crate::inbox_api::server::FEATURE).await);
        // Remote is on as a feature, but the relay itself stays off.
        assert_eq!(r.get(crate::remote::KEY_ENABLED), None);
        // Only switches the registry has now get a row; later props keep their default.
        let feature_rows: Vec<&String> = r.keys().filter(|k| k.starts_with(crate::features::PREFIX)).collect();
        assert!(feature_rows.iter().all(|k| *k == SEEDED_KEY || default_rows().contains_key(*k)));
        assert!(!r.contains_key("feature.sessions"), "locked entries are not written");
    }

    #[tokio::test]
    async fn upgrade_keeps_a_legacy_key_set_to_false() {
        let db = db().await;
        add_project(&db).await;
        set_setting(&db, crate::inbox_api::ENABLED_SETTING_KEY, "false").await.unwrap();
        set_setting(&db, "animated_background", "false").await.unwrap();
        set_setting(&db, "feature.productivity.todos", "false").await.unwrap();
        seed(&db).await.unwrap();
        let r = rows(&db).await;

        assert_eq!(r.get(crate::inbox_api::ENABLED_SETTING_KEY).map(String::as_str), Some("false"));
        assert_eq!(r.get("animated_background").map(String::as_str), Some("false"));
        assert_eq!(r.get("feature.productivity.todos").map(String::as_str), Some("false"));
        assert_eq!(r.get("feature.productivity.focus").map(String::as_str), Some("true"));
    }

    #[tokio::test]
    async fn running_twice_changes_nothing() {
        for with_project in [false, true] {
            let db = db().await;
            if with_project {
                add_project(&db).await;
            }
            seed(&db).await.unwrap();
            let first = rows(&db).await;
            // A project added after the first run must not turn it into an upgrade.
            if !with_project {
                add_project(&db).await;
            }
            seed(&db).await.unwrap();
            assert_eq!(rows(&db).await, first, "with_project = {with_project}");
        }
    }

    /// A machine seeded before a prop existed has no row for it: the seed does
    /// not run again, and the prop takes its registry default.
    #[tokio::test]
    async fn a_prop_registered_after_the_seed_gets_its_default() {
        let db = db().await;
        add_project(&db).await;
        set_setting(&db, SEEDED_KEY, "1").await.unwrap();
        set_setting(&db, "feature.text_tools", "true").await.unwrap();
        set_setting(&db, "feature.text_tools.selection.translate", "true").await.unwrap();
        seed(&db).await.unwrap();
        let r = rows(&db).await;

        for id in [
            "text_tools.find_in_page",
            "text_tools.selection.quote",
            "text_tools.selection.copy_md",
            "sessions.cross_session_search",
            "sessions.rewind",
            "library.agents",
            "library.commands",
            "library.skill_doctor",
            "library.import_external",
            "library.code_index",
            "app.auto_update",
            "app.whats_new",
            "app.command_palette",
        ] {
            let (m, p) = crate::features::entry(id).unwrap();
            assert!(!r.contains_key(&p.unwrap().key(m)), "{id}");
            assert!(crate::features::is_on(&db, id).await, "{id}");
        }
    }

    /// An opt-in prop registered after the seed stays off on a machine
    /// already seeded: an upgrade does not start scheduled runs or hand an
    /// agent the dev server tools.
    #[tokio::test]
    async fn an_off_prop_registered_after_the_seed_stays_off() {
        let db = db().await;
        add_project(&db).await;
        set_setting(&db, SEEDED_KEY, "1").await.unwrap();
        set_setting(&db, "feature.orchestration", "true").await.unwrap();
        seed(&db).await.unwrap();
        let r = rows(&db).await;
        for id in [crate::schedules::FEATURE, "orchestration.dev_servers"] {
            assert!(!r.contains_key(&format!("feature.{id}")), "{id}");
            assert!(!crate::features::is_on(&db, id).await, "{id}");
        }
    }

    #[tokio::test]
    async fn an_existing_flag_skips_the_seed() {
        let db = db().await;
        add_project(&db).await;
        set_setting(&db, SEEDED_KEY, "1").await.unwrap();
        let before = rows(&db).await;
        seed(&db).await.unwrap();
        assert_eq!(rows(&db).await, before);
    }
}
