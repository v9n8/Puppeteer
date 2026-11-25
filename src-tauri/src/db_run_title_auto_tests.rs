//! Migration 0034: `runs.title_auto` exists on a database that already holds runs, and only
//! "Imported session" runs start out as app-named.

use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;

/// A database a user had right before 0034: every migration up to 0033 applied.
async fn db_before_0034() -> super::Db {
    let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let all = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in std::fs::read_dir(&all).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.as_str() < "0034" {
            std::fs::copy(all.join(&name), dir.join(&name)).unwrap();
        }
    }
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&pool).await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    pool
}

#[tokio::test]
async fn only_imported_session_runs_start_out_app_named() {
    let db = db_before_0034().await;
    sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO runs (id, project_id, type, created_at, title) VALUES
         ('imported', 'p1', 'session', 'c', 'Imported session'),
         ('prompt', 'p1', 'session', 'c', 'Fix the login bug'),
         ('untitled', 'p1', 'session', 'c', NULL),
         ('review', 'p1', 'review_pr', 'c', 'PR #1')",
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::migrate!("./migrations").run(&db).await.unwrap();

    let rows: Vec<(String, Option<String>, i64)> =
        sqlx::query_as("SELECT id, title, title_auto FROM runs ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("imported".into(), Some("Imported session".into()), 1),
            ("prompt".into(), Some("Fix the login bug".into()), 0),
            ("review".into(), Some("PR #1".into()), 0),
            ("untitled".into(), None, 0),
        ]
    );

    let (ty, notnull, default): (String, i64, Option<String>) = sqlx::query_as(
        "SELECT type, \"notnull\", dflt_value FROM pragma_table_info('runs') WHERE name = 'title_auto'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!((ty.as_str(), notnull, default.as_deref()), ("INTEGER", 1, Some("0")));

    // A run inserted without the column is not app-named.
    sqlx::query("INSERT INTO runs (id, project_id, type, created_at) VALUES ('new', 'p1', 'session', 'c')")
        .execute(&db)
        .await
        .unwrap();
    let flag: i64 = sqlx::query_scalar("SELECT title_auto FROM runs WHERE id = 'new'").fetch_one(&db).await.unwrap();
    assert_eq!(flag, 0);
}
