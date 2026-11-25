//! Migration 0035: the MCP source columns arrive empty on a database that already holds
//! servers, and deleting a Claude account drops the connectors its last probe saw.

use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;

/// A database a user had right before 0035: every migration up to 0034 applied.
async fn db_before_0035() -> super::Db {
    let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let all = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in std::fs::read_dir(&all).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.as_str() < "0035" {
            std::fs::copy(all.join(&name), dir.join(&name)).unwrap();
        }
    }
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&pool).await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    pool
}

#[tokio::test]
async fn existing_servers_get_empty_source_columns() {
    let db = db_before_0035().await;
    sqlx::query(
        "INSERT INTO mcp_servers (id, name, transport, command, enabled, created_at) \
         VALUES ('m1', 'github', 'stdio', 'npx', 1, 'c')",
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::migrate!("./migrations").run(&db).await.unwrap();

    let row: (String, i64, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT command, enabled, source_kind, source_ref, source_name, source_fingerprint, connector_account \
             FROM mcp_servers WHERE id = 'm1'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row, ("npx".into(), 1, None, None, None, None, None));
    let connectors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM claude_connectors").fetch_one(&db).await.unwrap();
    assert_eq!(connectors, 0);
}

async fn connector(db: &super::Db, account: &str, name: &str) {
    sqlx::query(
        "INSERT INTO claude_connectors (account_ref, name, connector_id, url, status, seen_at) \
         VALUES (?, ?, 'id', 'https://example.invalid/mcp', 'connected', 'now')",
    )
    .bind(account)
    .bind(name)
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn deleting_a_claude_account_removes_only_its_connectors() {
    let db = db_before_0035().await;
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO claude_accounts (id, label, config_dir, created_at) \
         VALUES ('a1', 'Work', '/tmp/a1', 'c'), ('a2', 'Home', '/tmp/a2', 'c')",
    )
    .execute(&db)
    .await
    .unwrap();
    for account in ["a1", "a2", "global"] {
        connector(&db, account, "claude.ai Slack").await;
    }
    connector(&db, "a1", "claude.ai Figma").await;

    sqlx::query("DELETE FROM claude_accounts WHERE id = 'a1'").execute(&db).await.unwrap();

    let left: Vec<(String, String)> =
        sqlx::query_as("SELECT account_ref, name FROM claude_connectors ORDER BY account_ref")
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(
        left,
        vec![("a2".into(), "claude.ai Slack".into()), ("global".into(), "claude.ai Slack".into())]
    );
}

#[tokio::test]
async fn a_profile_lists_each_connector_name_once() {
    let db = db_before_0035().await;
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    connector(&db, "global", "claude.ai Slack").await;
    let again = sqlx::query(
        "INSERT INTO claude_connectors (account_ref, name, connector_id, url, status, seen_at) \
         VALUES ('global', 'claude.ai Slack', 'x', 'u', 's', 't')",
    )
    .execute(&db)
    .await;
    assert!(again.is_err());
}
