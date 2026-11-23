//! "Compact database": SQLite keeps the pages of deleted rows for reuse, so the
//! file never shrinks on its own (`auto_vacuum` is off). VACUUM rewrites it; it
//! needs the database to itself and free disk space the size of the file, so
//! it only runs by hand, never while a run is running, and never next to a
//! cleanup. A lock `sidecar-mcp` holds past the busy timeout is reported, not
//! retried; a VACUUM SQLite cannot finish leaves the file as it was.

use super::LOCK;
use crate::db::Db;
use crate::runs::RunRegistry;
use serde::Serialize;
use std::path::Path;
use tauri::{Manager, State};

pub const RUNS_ACTIVE: &str = "runs_active";
pub const DB_BUSY: &str = "db_busy";

#[derive(Debug, Serialize)]
pub struct Compacted {
    pub before_bytes: u64,
    pub after_bytes: u64,
}

#[tauri::command]
pub async fn compact_database(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    registry: State<'_, RunRegistry>,
) -> Result<Compacted, String> {
    let Ok(_held) = LOCK.try_lock() else {
        return Err(super::commands::BUSY.into());
    };
    if !registry.lock().await.is_empty() || any_running(db.inner()).await? {
        return Err(RUNS_ACTIVE.into());
    }
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("data.db");
    let done = compact_at(db.inner(), &path).await?;
    tracing::info!(event = "database_compacted", before = done.before_bytes, after = done.after_bytes);
    Ok(done)
}

async fn any_running(db: &Db) -> Result<bool, String> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE status = 'running')")
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())
}

/// `data.db` with its `-wal` and `-shm`, which only ever move together.
fn size_on_disk(path: &Path) -> u64 {
    ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(format!("{}{suffix}", path.display())).ok())
        .map(|m| m.len())
        .sum()
}

async fn compact_at(db: &Db, path: &Path) -> Result<Compacted, String> {
    let before_bytes = size_on_disk(path);
    let mut conn = db.acquire().await.map_err(|e| e.to_string())?;
    // VACUUM writes the new file through the WAL: checkpoint before, so it
    // starts from a short log, and after, so the log shrinks back.
    for sql in ["PRAGMA wal_checkpoint(TRUNCATE)", "VACUUM", "PRAGMA wal_checkpoint(TRUNCATE)"] {
        sqlx::query(sql).execute(&mut *conn).await.map_err(busy_or_message)?;
    }
    Ok(Compacted { before_bytes, after_bytes: size_on_disk(path) })
}

/// SQLITE_BUSY (with any extended code) → [`DB_BUSY`]; anything else as SQLite says it.
fn busy_or_message(e: sqlx::Error) -> String {
    let busy = e
        .as_database_error()
        .and_then(|d| d.code())
        .and_then(|c| c.parse::<i32>().ok())
        .is_some_and(|c| c & 0xff == 5);
    if busy {
        DB_BUSY.into()
    } else {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::temp_dir;
    use crate::storage_cleanup::tests::test_db;

    #[tokio::test]
    async fn vacuum_gives_back_the_space_of_deleted_rows() {
        let dir = temp_dir("compact");
        let db = test_db(&dir).await;
        let big = "x".repeat(64 * 1024);
        for i in 0..200 {
            crate::commands::settings::set_setting(&db, &format!("junk.{i}"), &big).await.unwrap();
        }
        sqlx::query("DELETE FROM settings WHERE key LIKE 'junk.%'").execute(&db).await.unwrap();
        let done = compact_at(&db, &dir.join("data.db")).await.unwrap();
        assert!(done.after_bytes + 5 * 1024 * 1024 < done.before_bytes, "{done:?}");
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key LIKE 'junk.%'").fetch_one(&db).await.unwrap();
        assert_eq!(left, 0);
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_running_run_is_seen() {
        let dir = temp_dir("compact-running");
        let db = test_db(&dir).await;
        assert!(!any_running(&db).await.unwrap());
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/p', 'c')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at, status) VALUES ('r1', 'p1', 'session', 'c', 'running')").execute(&db).await.unwrap();
        assert!(any_running(&db).await.unwrap());
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compacting_is_local_only() {
        assert!(crate::remote::guard::is_local_only_command("compact_database"));
    }
}
