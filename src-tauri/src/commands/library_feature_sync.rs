//! A library kind switched off stops reaching the CLI. Applied agents and commands are
//! symlinks in each project's `.claude/agents|commands`, which Claude Code loads whatever
//! Puppeteer's switches say, so turning `library.agents` / `library.commands` off removes
//! Puppeteer's own links from every project, and turning it back on links them again.
//! The `project_agents` / `project_commands` rows are kept: they are the project's choice,
//! which comes back with the switch. Only Puppeteer's links are touched: a user's file of
//! the same name, or one git tracks, is left alone (`library_items::link_into` / `unlink_from`).

use crate::commands::library_items::{link_into, project_dest, unlink_from, LibraryKind};
use crate::db::Db;
use crate::features::{self, apply::Change};
use std::path::Path;
use tauri::{AppHandle, Manager};

const KINDS: [LibraryKind; 2] = [LibraryKind::Agent, LibraryKind::Command];

/// One sync at a time: a quick off → on runs its two syncs in order, each on the
/// switch as stored when it starts, so the last one leaves the links matching it.
static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Feature hook: sync each kind whose switch moved.
pub fn on_features_changed(app: &AppHandle, change: &Change<'_>) {
    for kind in KINDS {
        if features::effective(change.before, kind.feature()) == features::effective(change.after, kind.feature()) {
            continue;
        }
        let db = app.state::<Db>().inner().clone();
        tauri::async_runtime::spawn(async move { sync_now(&db, kind).await });
    }
}

/// At startup: remove the links of a kind that is off, which a switch turned off
/// before the links were synced (an older version) left behind.
pub async fn clean_on_start(db: &Db) {
    for kind in KINDS {
        if !features::is_on(db, kind.feature()).await {
            sync_now(db, kind).await;
        }
    }
}

/// [`sync_kind`] to the switch as stored now, after any sync already running.
async fn sync_now(db: &Db, kind: LibraryKind) {
    let _guard = SYNC.lock().await;
    let on = features::is_on(db, kind.feature()).await;
    if let Err(e) = sync_kind(db, kind, on).await {
        tracing::warn!(event = "library_feature_sync_failed", kind = kind.table(), on, error = %e);
    }
}

/// Link (`on`) or unlink every applied item of `kind` in every project. A failing item is
/// logged and skipped, so one project cannot stop the others; returns those failures.
pub async fn sync_kind(db: &Db, kind: LibraryKind, on: bool) -> Result<Vec<String>, String> {
    let sql = format!(
        "SELECT p.path, i.name, i.source_path FROM {links} l \
         JOIN projects p ON p.id = l.project_id JOIN {items} i ON i.id = l.{col}",
        links = kind.link_table(),
        items = kind.table(),
        col = kind.id_column()
    );
    let rows: Vec<(String, String, String)> = sqlx::query_as(&sql).fetch_all(db).await.map_err(|e| e.to_string())?;
    let mut failures = Vec::new();
    for (path, name, source) in rows {
        let dest = project_dest(kind, &path, &name);
        let done = if on {
            link_into(&path, &dest, Path::new(&source)).map(|_| ())
        } else {
            unlink_from(&path, &dest, Path::new(&source))
        };
        if let Err(e) = done {
            tracing::warn!(event = "library_feature_sync_item_failed", kind = kind.table(), on, dest = %dest.display(), error = %e);
            failures.push(format!("{}: {e}", dest.display()));
        }
    }
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library_items::{apply_item, create_item, CreateItemPayload};
    use crate::commands::project_links::is_symlink;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    /// DB with project `p1` (a git repo) at `<tmp>/proj`, and the libraries under `<tmp>/lib`.
    async fn fixture(label: &str) -> (Db, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("puppeteer-libsync-{label}-{}", uuid::Uuid::new_v4()));
        let project = dir.join("proj");
        fs::create_dir_all(&project).unwrap();
        assert!(Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap().success());
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', ?, 'c')")
            .bind(project.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        (db, project, dir.join("lib"))
    }

    async fn applied(db: &Db, kind: LibraryKind, lib: &Path, name: &str) {
        let payload = CreateItemPayload { name: name.into(), description: "d".into(), content: "body".into() };
        let item = create_item(db, kind, &lib.join(kind.table()), payload).await.unwrap();
        apply_item(db, kind, "p1", &item.id).await.unwrap();
    }

    async fn rows(db: &Db, kind: LibraryKind) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", kind.link_table())).fetch_one(db).await.unwrap()
    }

    #[tokio::test]
    async fn off_removes_puppeteers_links_and_keeps_the_choice_on_links_them_again() {
        let (db, project, lib) = fixture("cycle").await;
        for (kind, folder) in [(LibraryKind::Agent, ".claude/agents"), (LibraryKind::Command, ".claude/commands")] {
            applied(&db, kind, &lib, "fix").await;
            let dest = project.join(folder).join("fix.md");
            assert!(is_symlink(&dest));

            assert!(sync_kind(&db, kind, false).await.unwrap().is_empty());
            assert!(fs::symlink_metadata(&dest).is_err(), "the link is gone");
            assert_eq!(rows(&db, kind).await, 1, "the project still lists it as applied");

            assert!(sync_kind(&db, kind, true).await.unwrap().is_empty());
            assert!(is_symlink(&dest));
            assert_eq!(fs::read_link(&dest).unwrap(), lib.join(kind.table()).join("fix.md"));
        }
    }

    #[tokio::test]
    async fn a_sync_follows_the_switch_as_stored_when_it_runs() {
        let (db, project, lib) = fixture("now").await;
        applied(&db, LibraryKind::Agent, &lib, "fix").await;
        let dest = project.join(".claude/agents/fix.md");
        crate::commands::settings::set_setting(&db, "feature.library.agents", "false").await.unwrap();
        sync_now(&db, LibraryKind::Agent).await;
        assert!(fs::symlink_metadata(&dest).is_err());
        // Turned back on before a late sync runs: that sync links, whatever it was started for.
        crate::commands::settings::set_setting(&db, "feature.library.agents", "true").await.unwrap();
        sync_now(&db, LibraryKind::Agent).await;
        assert!(is_symlink(&dest));
        // At startup only a kind that is off is cleaned.
        clean_on_start(&db).await;
        assert!(is_symlink(&dest));
        crate::commands::settings::set_setting(&db, "feature.library.agents", "false").await.unwrap();
        clean_on_start(&db).await;
        assert!(fs::symlink_metadata(&dest).is_err());
        assert_eq!(rows(&db, LibraryKind::Agent).await, 1);
    }

    #[tokio::test]
    async fn a_users_own_file_of_the_same_name_is_never_touched() {
        let (db, project, lib) = fixture("own").await;
        let kind = LibraryKind::Agent;
        applied(&db, kind, &lib, "fix").await;
        sync_kind(&db, kind, false).await.unwrap();
        // While it was off, the user wrote their own file there.
        let dest = project.join(".claude/agents/fix.md");
        fs::write(&dest, "mine").unwrap();

        sync_kind(&db, kind, false).await.unwrap();
        let failures = sync_kind(&db, kind, true).await.unwrap();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(!is_symlink(&dest));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "mine");
    }
}
