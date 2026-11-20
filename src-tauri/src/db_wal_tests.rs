//! `init_db` opens `data.db` in WAL mode: a fresh database and one a user already had (rollback
//! journal, with data) both come out as `wal`, and a reader no longer waits on an open write
//! transaction.

use super::init_db;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::Row;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn temp_db() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("puppeteer-wal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.db");
    (dir, path)
}

async fn journal_mode(db: &super::Db) -> String {
    sqlx::query("PRAGMA journal_mode").fetch_one(db).await.unwrap().get::<String, _>(0)
}

#[tokio::test]
async fn a_new_database_is_created_in_wal_mode() {
    let (dir, path) = temp_db();
    let db = init_db(&path).await.unwrap();
    assert_eq!(journal_mode(&db).await, "wal");
    let busy: i64 = sqlx::query("PRAGMA busy_timeout").fetch_one(&db).await.unwrap().get(0);
    assert_eq!(busy, super::BUSY_TIMEOUT.as_millis() as i64);
    db.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_existing_rollback_journal_database_switches_to_wal_and_keeps_its_data() {
    let (dir, path) = temp_db();
    {
        // What every install before this change has: sqlx defaults, journal_mode = delete.
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete);
        let old = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        sqlx::migrate!("./migrations").run(&old).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'kept', '/kept', 'c')")
            .execute(&old)
            .await
            .unwrap();
        assert_eq!(journal_mode(&old).await, "delete");
        old.close().await;
    }
    let db = init_db(&path).await.unwrap();
    assert_eq!(journal_mode(&db).await, "wal");
    let name: String = sqlx::query("SELECT name FROM projects WHERE id = 'p1'").fetch_one(&db).await.unwrap().get(0);
    assert_eq!(name, "kept");
    db.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_reader_does_not_wait_for_an_open_write_transaction() {
    let (dir, path) = temp_db();
    let db = init_db(&path).await.unwrap();
    sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'before', '/p', 'c')")
        .execute(&db)
        .await
        .unwrap();

    let mut writer = db.acquire().await.unwrap();
    // EXCLUSIVE: under a rollback journal this shuts every reader out until COMMIT.
    sqlx::query("BEGIN EXCLUSIVE").execute(&mut *writer).await.unwrap();
    sqlx::query("UPDATE projects SET name = 'during' WHERE id = 'p1'").execute(&mut *writer).await.unwrap();

    let started = Instant::now();
    let read = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("SELECT name FROM projects WHERE id = 'p1'").fetch_one(&db),
    )
    .await
    .expect("the read waited on the writer")
    .unwrap();
    let waited = started.elapsed();
    // The writer still holds its transaction: the reader sees the last committed value.
    assert_eq!(read.get::<String, _>(0), "before");
    assert!(waited < Duration::from_secs(1), "read took {waited:?}");

    sqlx::query("COMMIT").execute(&mut *writer).await.unwrap();
    drop(writer);
    let after: String = sqlx::query("SELECT name FROM projects WHERE id = 'p1'").fetch_one(&db).await.unwrap().get(0);
    assert_eq!(after, "during");
    db.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
