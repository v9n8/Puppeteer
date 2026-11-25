//! Migration 0033: a project default equal to the global setting of the same meaning becomes
//! NULL (follows the global setting); values that differ, aliases and unknown values stay.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::path::Path;
use std::time::{Duration, Instant};

const MIGRATION: &str = include_str!("../migrations/0033_project_defaults_follow_global.sql");

/// A database a user had right before 0033: every migration up to 0032 applied.
async fn db_before_0033() -> super::Db {
    let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let all = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in std::fs::read_dir(&all).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.as_str() < "0033" {
            std::fs::copy(all.join(&name), dir.join(&name)).unwrap();
        }
    }
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&pool).await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    pool
}

async fn set_setting(db: &super::Db, key: &str, value: &str) {
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)").bind(key).bind(value).execute(db).await.unwrap();
}

/// `key` = `value`, or no row at all when `None` (0001 seeds `default_engine`).
async fn global(db: &super::Db, key: &str, value: Option<&str>) {
    match value {
        Some(v) => set_setting(db, key, v).await,
        None => {
            sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(db).await.unwrap();
        }
    }
}

/// A project with `column` = `value` (NULL when `None`); the other defaults NULL.
async fn project(db: &super::Db, id: &str, column: &str, value: Option<&str>) {
    sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES (?, ?, ?, 'c')")
        .bind(id)
        .bind(id)
        .bind(format!("/{id}"))
        .execute(db)
        .await
        .unwrap();
    sqlx::query(&format!("UPDATE projects SET {column} = ? WHERE id = ?")).bind(value).bind(id).execute(db).await.unwrap();
}

async fn value(db: &super::Db, id: &str, column: &str) -> Option<String> {
    sqlx::query_scalar(&format!("SELECT {column} FROM projects WHERE id = ?")).bind(id).fetch_one(db).await.unwrap()
}

async fn migrate(db: &super::Db) {
    sqlx::migrate!("./migrations").run(db).await.unwrap();
}

#[tokio::test]
async fn a_model_equal_to_the_global_one_follows_it_and_an_alias_or_other_model_stays() {
    let db = db_before_0033().await;
    set_setting(&db, "claude_model", "claude-opus-5-5[1m]").await;
    set_setting(&db, "codex_model", "gpt-5.5").await;
    project(&db, "same", "default_model_claude", Some("claude-opus-5-5[1m]")).await;
    project(&db, "other", "default_model_claude", Some("claude-opus-5-5")).await;
    project(&db, "none", "default_model_claude", None).await;
    project(&db, "alias", "default_model_claude", Some("opus")).await;
    project(&db, "codex", "default_model_codex", Some("gpt-5.5")).await;
    migrate(&db).await;
    assert_eq!(value(&db, "same", "default_model_claude").await, None);
    assert_eq!(value(&db, "other", "default_model_claude").await.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(value(&db, "none", "default_model_claude").await, None);
    assert_eq!(value(&db, "alias", "default_model_claude").await.as_deref(), Some("opus"));
    assert_eq!(value(&db, "codex", "default_model_codex").await, None);
}

#[tokio::test]
async fn an_alias_of_the_global_model_is_not_the_same_model() {
    let db = db_before_0033().await;
    set_setting(&db, "claude_model", "claude-opus-5-5").await;
    project(&db, "alias", "default_model_claude", Some("opus")).await;
    migrate(&db).await;
    assert_eq!(value(&db, "alias", "default_model_claude").await.as_deref(), Some("opus"));
}

#[tokio::test]
async fn an_engine_follows_the_global_one_and_a_blank_or_missing_global_means_claude() {
    for (global_value, pinned, kept) in [
        (None, "claude", false),
        (Some(""), "claude", false),
        (Some("bogus"), "claude", false),
        (Some("codex"), "codex", false),
        (Some("codex"), "claude", true),
        (None, "codex", true),
    ] {
        let db = db_before_0033().await;
        global(&db, "default_engine", global_value).await;
        project(&db, "p", "default_engine_override", Some(pinned)).await;
        migrate(&db).await;
        let expected = kept.then(|| pinned.to_string());
        assert_eq!(value(&db, "p", "default_engine_override").await, expected, "global {global_value:?}, project {pinned}");
    }
}

#[tokio::test]
async fn a_permission_mode_follows_the_global_one_and_a_blank_or_invalid_global_means_default() {
    for (global_value, pinned, kept) in [
        (None, "default", false),
        (Some(""), "default", false),
        (Some("bogus"), "default", false),
        (Some("plan"), "plan", false),
        (Some("plan"), "auto", true),
        (Some("plan"), "default", true),
    ] {
        let db = db_before_0033().await;
        global(&db, "default_permission_mode", global_value).await;
        project(&db, "p", "default_permission_mode", Some(pinned)).await;
        migrate(&db).await;
        let expected = kept.then(|| pinned.to_string());
        assert_eq!(value(&db, "p", "default_permission_mode").await, expected, "global {global_value:?}, project {pinned}");
    }
}

#[tokio::test]
async fn an_effort_follows_only_a_valid_global_level() {
    for (global_value, pinned, kept) in [
        (Some("high"), "high", false),
        (Some(""), "high", true),
        (None, "high", true),
        (Some("bogus"), "bogus", true),
        (Some("high"), "low", true),
    ] {
        let db = db_before_0033().await;
        global(&db, "default_effort", global_value).await;
        project(&db, "p", "default_effort", Some(pinned)).await;
        migrate(&db).await;
        let expected = kept.then(|| pinned.to_string());
        assert_eq!(value(&db, "p", "default_effort").await, expected, "global {global_value:?}, project {pinned}");
    }
}

#[tokio::test]
async fn a_style_follows_the_global_one_and_a_blank_global_means_no_style() {
    for (global_value, pinned, kept) in [
        (Some(""), "default", false),
        (None, "default", false),
        (Some("style-x"), "style-x", false),
        (Some("style-x"), "default", true),
        (Some(""), "style-x", true),
    ] {
        let db = db_before_0033().await;
        global(&db, "default_style", global_value).await;
        project(&db, "p", "default_style", Some(pinned)).await;
        migrate(&db).await;
        let expected = kept.then(|| pinned.to_string());
        assert_eq!(value(&db, "p", "default_style").await, expected, "global {global_value:?}, project {pinned}");
    }
}

#[tokio::test]
async fn other_project_columns_stay_byte_identical_and_trashed_projects_are_converted() {
    let db = db_before_0033().await;
    set_setting(&db, "claude_model", "claude-opus-5-5").await;
    project(&db, "p", "default_model_claude", Some("claude-opus-5-5")).await;
    sqlx::query("INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('acct-1', 'a', '/a', 'c')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE projects SET enabled_models = '{\"claude\":[\"claude-opus-5-5\"],\"codex\":null}', \
         claude_account_id = 'acct-1', default_engine = 'claude', deleted_at = '2026-11-01T00:00:00Z' WHERE id = 'p'",
    )
    .execute(&db)
    .await
    .unwrap();
    let untouched = "SELECT enabled_models, claude_account_id, default_engine, deleted_at FROM projects WHERE id = 'p'";
    let row = |r: sqlx::sqlite::SqliteRow| -> Vec<Option<String>> { (0..4).map(|i| r.get(i)).collect() };
    let before = row(sqlx::query(untouched).fetch_one(&db).await.unwrap());
    migrate(&db).await;
    assert_eq!(row(sqlx::query(untouched).fetch_one(&db).await.unwrap()), before);
    assert_eq!(value(&db, "p", "default_model_claude").await, None);
}

#[tokio::test]
async fn running_the_migration_again_changes_nothing() {
    let db = db_before_0033().await;
    for (key, v) in [("claude_model", "m"), ("default_engine", "codex"), ("default_permission_mode", "plan"), ("default_effort", "high"), ("default_style", "")] {
        set_setting(&db, key, v).await;
    }
    for (id, column, v) in [
        ("a", "default_model_claude", "m"),
        ("b", "default_engine_override", "codex"),
        ("c", "default_permission_mode", "plan"),
        ("d", "default_effort", "high"),
        ("e", "default_style", "default"),
        ("f", "default_model_claude", "other"),
    ] {
        project(&db, id, column, Some(v)).await;
    }
    migrate(&db).await;
    let mut changed = 0;
    // The file's statements again, without its comments (they hold a `;`).
    let sql: Vec<&str> = MIGRATION.lines().filter(|l| !l.trim_start().starts_with("--")).collect();
    let statements: Vec<String> = sql.join("\n").split(';').filter(|s| s.contains("UPDATE")).map(str::to_string).collect();
    assert_eq!(statements.len(), 6);
    for statement in &statements {
        changed += sqlx::query(statement).execute(&db).await.unwrap().rows_affected();
    }
    assert_eq!(changed, 0);
    assert_eq!(value(&db, "f", "default_model_claude").await.as_deref(), Some("other"));
}

/// The quoted names in the `IN (…)` list that follows `after` in the migration.
fn sql_list(after: &str) -> Vec<String> {
    let start = MIGRATION.find(after).unwrap() + after.len();
    let rest = &MIGRATION[start..];
    let rest = &rest[rest.find("IN (").unwrap() + 4..];
    let list = &rest[..rest.find(')').unwrap()];
    list.split(',').map(|s| s.trim().trim_matches('\'').to_string()).collect()
}

#[test]
fn the_migrations_permission_and_effort_lists_match_what_a_run_accepts() {
    let efforts = sql_list("key = 'default_effort' AND TRIM(value)");
    assert_eq!(efforts, crate::commands::runs::EFFORT_LEVELS);
    let modes = sql_list("UPDATE projects SET default_permission_mode");
    // Every listed mode is valid, and every mode `is_valid_permission_mode` names is listed.
    assert!(modes.iter().all(|m| crate::commands::runs::is_valid_permission_mode(m)), "{modes:?}");
    let src = include_str!("commands/runs.rs");
    let body = &src[src.find("fn is_valid_permission_mode").unwrap()..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let valid: Vec<String> = body.split('"').skip(1).step_by(2).map(str::to_string).collect();
    assert_eq!(modes, valid);
}

#[tokio::test]
async fn a_thousand_projects_migrate_in_under_a_second() {
    let db = db_before_0033().await;
    set_setting(&db, "claude_model", "m").await;
    let mut tx = db.begin().await.unwrap();
    for i in 0..1000 {
        sqlx::query(
            "INSERT INTO projects (id, name, path, created_at, default_model_claude, default_engine_override, default_effort) \
             VALUES (?, ?, ?, 'c', 'm', 'claude', 'high')",
        )
        .bind(format!("p{i}"))
        .bind(format!("p{i}"))
        .bind(format!("/p{i}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let started = Instant::now();
    migrate(&db).await;
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE default_model_claude IS NOT NULL")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
