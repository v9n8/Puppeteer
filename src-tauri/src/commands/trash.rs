//! The Trash screen: what is in the Trash, and the event every window
//! refetches its lists on. Moving in, restoring and purging are the
//! `trash_*` / `restore_*` / `purge_*` commands of `runs.rs` and `projects.rs`.
//!
//! A run is in the Trash when it, or its project, is trashed. Trashing a
//! project does not stamp its runs, so a run of a trashed project has no
//! `deleted_at` of its own.

use crate::db::Db;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Something moved to the Trash, came back from it or was purged. Every window
/// refetches what it lists (History, Projects, Trash); a remote-desktop window
/// gets it through the bridge like any app event.
pub const TRASH_CHANGED_EVENT: &str = "trash:changed";

pub(crate) fn emit_changed(app: &AppHandle) {
    let _ = app.emit(TRASH_CHANGED_EVENT, ());
}

/// A project of the Trash screen: in the Trash itself, or live with sessions in it.
#[derive(Debug, Serialize, PartialEq)]
pub struct TrashProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    /// When the project went to the Trash; `None` for a live project listed
    /// for its trashed sessions.
    pub deleted_at: Option<String>,
}

/// A run in the Trash.
#[derive(Debug, Serialize, PartialEq)]
pub struct TrashRun {
    pub id: String,
    pub project_id: String,
    pub title: Option<String>,
    pub run_type: String,
    pub ref_number: Option<i64>,
    pub status: String,
    pub engine: String,
    pub model: Option<String>,
    pub created_at: String,
    /// The run's own stamp; `None` when it is in the Trash only through its project.
    pub deleted_at: Option<String>,
    pub conductor_run_id: Option<String>,
    /// Workers a purge of this run deletes with it: the ones it spawned that
    /// are in the Trash too (`purge_run_tree`; a live worker is detached).
    pub worker_count: i64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TrashListing {
    pub projects: Vec<TrashProject>,
    /// Every run in the Trash, a trashed project's runs included.
    pub runs: Vec<TrashRun>,
}

/// Everything in the Trash. Read-only, so a remote desktop may list it too.
#[tauri::command]
pub async fn list_trash(db: State<'_, Db>) -> Result<TrashListing, String> {
    list_trash_inner(db.inner()).await
}

pub(crate) async fn list_trash_inner(db: &Db) -> Result<TrashListing, String> {
    use sqlx::Row;
    let projects = sqlx::query(
        "SELECT p.id, p.name, p.path, p.created_at, p.deleted_at FROM projects p \
         WHERE p.deleted_at IS NOT NULL \
            OR EXISTS (SELECT 1 FROM runs r WHERE r.project_id = p.id AND r.deleted_at IS NOT NULL) \
         ORDER BY p.id",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?
    .iter()
    .map(|row| TrashProject {
        id: row.get("id"),
        name: row.get("name"),
        path: row.get("path"),
        created_at: row.get("created_at"),
        deleted_at: row.get("deleted_at"),
    })
    .collect();

    let runs = sqlx::query(
        "SELECT r.id, r.project_id, r.title, r.type, r.ref_number, r.status, r.engine, r.model, \
                r.created_at, r.deleted_at, r.conductor_run_id, \
                (SELECT COUNT(*) FROM runs w JOIN projects wp ON wp.id = w.project_id \
                 WHERE w.conductor_run_id = r.id AND w.id != r.id \
                   AND (w.deleted_at IS NOT NULL OR wp.deleted_at IS NOT NULL)) AS worker_count \
         FROM runs r JOIN projects p ON p.id = r.project_id \
         WHERE r.deleted_at IS NOT NULL OR p.deleted_at IS NOT NULL \
         ORDER BY r.id",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?
    .iter()
    .map(|row| TrashRun {
        id: row.get("id"),
        project_id: row.get("project_id"),
        title: row.get("title"),
        run_type: row.get("type"),
        ref_number: row.get("ref_number"),
        status: row.get("status"),
        engine: row.get("engine"),
        model: row.get("model"),
        created_at: row.get("created_at"),
        deleted_at: row.get("deleted_at"),
        conductor_run_id: row.get("conductor_run_id"),
        worker_count: row.get("worker_count"),
    })
    .collect();

    Ok(TrashListing { projects, runs })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db_with(rows: &[&str]) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in rows {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    /// `live` keeps r1, has r2 trashed on its own and conductor c1 trashed with
    /// worker w1 (w2 restored on its own); `gone` is trashed with r3 and r4,
    /// r4 trashed on its own before; `clean` has nothing in the Trash.
    const ROWS: &[&str] = &[
        "INSERT INTO projects (id, name, path, created_at, deleted_at) VALUES \
         ('live', 'Live', '/live', '2026-01-01', NULL), \
         ('gone', 'Gone', '/gone', '2026-01-02', '2026-03-01'), \
         ('clean', 'Clean', '/clean', '2026-01-03', NULL)",
        "INSERT INTO runs (id, project_id, type, ref_number, status, engine, model, created_at, title, conductor_run_id, deleted_at) VALUES \
         ('r1', 'live', 'session', NULL, 'done', 'claude', NULL, '2026-02-01', NULL, NULL, NULL), \
         ('r2', 'live', 'analyze_issue', 7, 'failed', 'codex', 'gpt-5', '2026-02-02', 'Fix it', NULL, '2026-03-02'), \
         ('c1', 'live', 'session', NULL, 'done', 'claude', NULL, '2026-02-03', NULL, 'c1', '2026-03-03'), \
         ('w1', 'live', 'session', NULL, 'done', 'claude', NULL, '2026-02-04', NULL, 'c1', '2026-03-03'), \
         ('w2', 'live', 'session', NULL, 'done', 'claude', NULL, '2026-02-05', NULL, 'c1', NULL), \
         ('r3', 'gone', 'session', NULL, 'done', 'claude', NULL, '2026-02-06', NULL, NULL, NULL), \
         ('r4', 'gone', 'session', NULL, 'done', 'claude', NULL, '2026-02-07', NULL, NULL, '2026-02-28'), \
         ('r5', 'clean', 'session', NULL, 'done', 'claude', NULL, '2026-02-08', NULL, NULL, NULL)",
    ];

    #[tokio::test]
    async fn lists_trashed_projects_with_every_run_and_trashed_runs_of_live_projects() {
        let db = db_with(ROWS).await;
        let listing = list_trash_inner(&db).await.unwrap();

        let projects: Vec<(&str, Option<&str>)> =
            listing.projects.iter().map(|p| (p.id.as_str(), p.deleted_at.as_deref())).collect();
        assert_eq!(projects, [("gone", Some("2026-03-01")), ("live", None)], "clean has nothing in the Trash");

        let runs: Vec<(&str, &str, Option<&str>)> = listing
            .runs
            .iter()
            .map(|r| (r.id.as_str(), r.project_id.as_str(), r.deleted_at.as_deref()))
            .collect();
        assert_eq!(
            runs,
            [
                ("c1", "live", Some("2026-03-03")),
                ("r2", "live", Some("2026-03-02")),
                // In the Trash through its project only: no stamp of its own.
                ("r3", "gone", None),
                ("r4", "gone", Some("2026-02-28")),
                ("w1", "live", Some("2026-03-03")),
            ],
            "r1 and w2 are live, r5 belongs to a live project"
        );

        let r2 = listing.runs.iter().find(|r| r.id == "r2").unwrap();
        assert_eq!(
            (r2.title.as_deref(), r2.run_type.as_str(), r2.ref_number, r2.engine.as_str(), r2.model.as_deref()),
            (Some("Fix it"), "analyze_issue", Some(7), "codex", Some("gpt-5"))
        );
        assert_eq!((r2.status.as_str(), r2.created_at.as_str()), ("failed", "2026-02-02"));
    }

    #[tokio::test]
    async fn worker_count_is_what_a_purge_of_the_run_deletes() {
        let db = db_with(ROWS).await;
        let listing = list_trash_inner(&db).await.unwrap();
        let count = |id: &str| listing.runs.iter().find(|r| r.id == id).unwrap().worker_count;
        // c1 points at itself (a conductor's own row): not one of its workers.
        // w2 is live again: a purge of c1 detaches it instead of deleting it.
        assert_eq!(count("c1"), 1);
        assert_eq!(count("w1"), 0);
        assert_eq!(count("r2"), 0);
        assert_eq!(listing.runs.iter().find(|r| r.id == "w1").unwrap().conductor_run_id.as_deref(), Some("c1"));
    }

    #[tokio::test]
    async fn an_empty_trash_lists_nothing() {
        let db = db_with(&[
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'P', '/p', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r', 'p', 'session', 'c')",
        ])
        .await;
        assert_eq!(list_trash_inner(&db).await.unwrap(), TrashListing { projects: vec![], runs: vec![] });
    }

    #[tokio::test]
    async fn a_restore_or_purge_leaves_the_listing() {
        let db = db_with(ROWS).await;
        crate::commands::projects::restore_project_inner(&db, "gone").await.unwrap();
        crate::commands::runs::restore_run_inner(&db, "r2").await.unwrap();
        sqlx::query("DELETE FROM runs WHERE id IN ('c1', 'w1', 'w2')").execute(&db).await.unwrap();
        let listing = list_trash_inner(&db).await.unwrap();
        assert_eq!(listing.projects.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["gone"]);
        // r4 was trashed on its own: it stays when its project comes back.
        assert_eq!(listing.runs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r4"]);
    }
}
