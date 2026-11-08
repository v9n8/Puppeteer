//! Bookmarks on the turns of a run: reading anchors the user puts on a
//! question or an answer from the row under it, to jump back to later. They
//! never reach the engine.
//!
//! A turn is anchored by its prompt record in the run log, not by anything in
//! the DOM (the viewer only holds a window of the log): the byte offset the
//! record starts at — the same cursor the prompt list and ⌘F jump to — plus the
//! prompt's one-line label. The snapshot `turn` a prompt carries is not used:
//! only prompts sent while Rewind was on have one. The viewer matches a
//! bookmark against the session's prompts by offset and label, and by label
//! alone when the log was rebuilt from the engine transcript. A Rewind that
//! cuts the log drops the bookmarks of the cut part ([`drop_from`]).

use serde::Serialize;
use tauri::State;

use crate::db::Db;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RunBookmark {
    pub id: String,
    pub run_id: String,
    /// Byte offset of the turn's prompt record in the run log.
    pub log_offset: i64,
    /// 'question' | 'answer'.
    pub side: String,
    pub label: String,
    pub created_at: String,
}

/// Longest label kept: the viewer's preview is 160 chars and an ellipsis.
const LABEL_MAX_CHARS: usize = 200;

const COLUMNS: &str = "id, run_id, log_offset, side, label, created_at";

pub async fn of_run(db: &Db, run_id: &str) -> Result<Vec<RunBookmark>, String> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM run_bookmarks WHERE run_id = ? ORDER BY log_offset, side DESC"))
        .bind(run_id)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())
}

/// Bookmark one side of the turn whose prompt starts at `log_offset`. Again on
/// the same turn and side only refreshes the label.
pub async fn add(db: &Db, run_id: &str, log_offset: i64, side: &str, label: &str) -> Result<RunBookmark, String> {
    if !matches!(side, "question" | "answer") {
        return Err(format!("invalid side: {side}"));
    }
    if log_offset < 0 {
        return Err("invalid offset".into());
    }
    let label: String = label.chars().take(LABEL_MAX_CHARS).collect();
    sqlx::query(
        "INSERT INTO run_bookmarks (id, run_id, log_offset, side, label, created_at)
         SELECT ?, id, ?, ?, ?, ? FROM runs WHERE id = ?
         ON CONFLICT (run_id, log_offset, side) DO UPDATE SET label = excluded.label",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(log_offset)
    .bind(side)
    .bind(&label)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(run_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM run_bookmarks WHERE run_id = ? AND log_offset = ? AND side = ?"))
        .bind(run_id)
        .bind(log_offset)
        .bind(side)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "run not found".to_string())
}

/// Drop the bookmarks of the turns at or after `log_offset`: a Rewind cut the
/// log there, so what they pointed at is gone.
pub async fn drop_from(db: &Db, run_id: &str, log_offset: u64) -> Result<u64, String> {
    sqlx::query("DELETE FROM run_bookmarks WHERE run_id = ? AND log_offset >= ?")
        .bind(run_id)
        .bind(log_offset as i64)
        .execute(db)
        .await
        .map(|r| r.rows_affected())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_run_bookmarks(db: State<'_, Db>, run_id: String) -> Result<Vec<RunBookmark>, String> {
    of_run(db.inner(), &run_id).await
}

#[tauri::command]
pub async fn add_run_bookmark(
    db: State<'_, Db>,
    run_id: String,
    log_offset: i64,
    side: String,
    label: String,
) -> Result<RunBookmark, String> {
    add(db.inner(), &run_id, log_offset, &side, &label).await
}

#[tauri::command]
pub async fn delete_run_bookmark(db: State<'_, Db>, id: String) -> Result<(), String> {
    sqlx::query("DELETE FROM run_bookmarks WHERE id = ?")
        .bind(&id)
        .execute(db.inner())
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub async fn db_with_run(run_id: &str) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at, session_id) VALUES (?, 'p1', 'session', 'c', 's-1')")
            .bind(run_id)
            .execute(&db)
            .await
            .unwrap();
        db
    }

    #[tokio::test]
    async fn a_bookmark_is_kept_per_turn_and_side() {
        let db = db_with_run("r1").await;
        let q = add(&db, "r1", 120, "question", "fix the login").await.unwrap();
        add(&db, "r1", 120, "answer", "fix the login").await.unwrap();
        // The same turn and side again: one row, the label refreshed.
        let again = add(&db, "r1", 120, "question", "fix the login page").await.unwrap();
        assert_eq!(again.id, q.id);
        let all = of_run(&db, "r1").await.unwrap();
        assert_eq!(all.iter().map(|b| (b.log_offset, b.side.as_str(), b.label.as_str())).collect::<Vec<_>>(), [
            (120, "question", "fix the login page"),
            (120, "answer", "fix the login"),
        ]);
    }

    #[tokio::test]
    async fn a_bookmark_needs_a_known_run_and_side() {
        let db = db_with_run("r1").await;
        assert!(add(&db, "nope", 0, "question", "x").await.is_err());
        assert!(add(&db, "r1", 0, "tool", "x").await.is_err());
        assert!(add(&db, "r1", -1, "question", "x").await.is_err());
        assert!(of_run(&db, "r1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_cut_drops_the_bookmarks_after_it_and_a_deleted_run_all_of_them() {
        let db = db_with_run("r1").await;
        for offset in [0, 500, 900] {
            add(&db, "r1", offset, "answer", "x").await.unwrap();
        }
        assert_eq!(drop_from(&db, "r1", 500).await.unwrap(), 2);
        assert_eq!(of_run(&db, "r1").await.unwrap().iter().map(|b| b.log_offset).collect::<Vec<_>>(), [0]);
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_bookmarks").fetch_one(&db).await.unwrap();
        assert_eq!(left, 0);
    }
}
