//! Where a managed MCP server came from (migration 0035): the source columns of
//! `mcp_servers`, and their part of a settings backup.
//!
//! A server created in Puppeteer has no source. An imported one remembers the
//! source id (`S2`..`S9`, or `claudeai` for a claude.ai connector), the source
//! file or project, its key there, a fingerprint of the entry at import / sync,
//! and for a connector the Claude profile it belongs to. Only names and paths:
//! never an env / header value.
//!
//! The backup carries the columns in their own section, matched by server name
//! like the servers themselves (`mcp::import_backup_mcp`). `claude_connectors`
//! is not backed up: each profile is probed again.

use serde::{Deserialize, Serialize};

use crate::commands::settings_backup::ImportReport;
use crate::db::Db;

/// The source columns of one `mcp_servers` row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSource {
    pub kind: String,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// `claudeai` rows only: a `claude_accounts.id`, or `global`.
    #[serde(default)]
    pub connector_account: Option<String>,
}

/// A server's source in a settings backup, keyed by the server's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSourceEntry {
    pub server: String,
    #[serde(flatten)]
    pub source: McpSource,
}

type SourceRow = (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);

fn from_row((kind, reference, name, fingerprint, connector_account): SourceRow) -> Option<McpSource> {
    Some(McpSource { kind: kind?, reference, name, fingerprint, connector_account })
}

/// The source of server `server_id`; `None` when it was created in Puppeteer (or does not exist).
#[allow(dead_code)] // read by discovery's row state and by Sync, which land after the columns
pub async fn source_of(db: &Db, server_id: &str) -> Result<Option<McpSource>, String> {
    let row: Option<SourceRow> = sqlx::query_as(
        "SELECT source_kind, source_ref, source_name, source_fingerprint, connector_account \
         FROM mcp_servers WHERE id = ?",
    )
    .bind(server_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.and_then(from_row))
}

/// Set (or with `None` clear) the source of server `server_id`. Returns whether a row was updated.
pub async fn set_source(db: &Db, server_id: &str, source: Option<&McpSource>) -> Result<bool, String> {
    let blank = McpSource::default();
    let s = source.unwrap_or(&blank);
    let kind = source.map(|s| s.kind.as_str());
    let done = sqlx::query(
        "UPDATE mcp_servers SET source_kind = ?, source_ref = ?, source_name = ?, \
         source_fingerprint = ?, connector_account = ? WHERE id = ?",
    )
    .bind(kind)
    .bind(&s.reference)
    .bind(&s.name)
    .bind(&s.fingerprint)
    .bind(&s.connector_account)
    .bind(server_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(done.rows_affected() > 0)
}

/// Every server with a source, for a settings backup.
pub async fn collect(db: &Db) -> Result<Vec<McpSourceEntry>, String> {
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT name, source_kind, source_ref, source_name, source_fingerprint, connector_account \
             FROM mcp_servers WHERE source_kind IS NOT NULL ORDER BY name",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|(server, kind, reference, name, fingerprint, account)| {
            Some(McpSourceEntry { server, source: from_row((kind, reference, name, fingerprint, account))? })
        })
        .collect())
}

/// Give the restored servers their sources. Runs after the servers are imported; a
/// server the backup lists without a source keeps the one it has here (an older
/// backup has no section at all). A name not on this machine is a warning.
pub async fn import(db: &Db, entries: &[McpSourceEntry], report: &mut ImportReport) {
    for entry in entries {
        let id: Result<Option<String>, String> = sqlx::query_scalar("SELECT id FROM mcp_servers WHERE name = ?")
            .bind(&entry.server)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string());
        let result = match id {
            Ok(Some(id)) => set_source(db, &id, Some(&entry.source)).await.map(|_| ()),
            Ok(None) => Err("server not on this machine".to_string()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            report.warnings.push(format!("mcpServers '{}': source not restored: {e}", entry.server));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::mcp::{import_backup_mcp, McpServer};

    async fn test_db() -> Db {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_server(db: &Db, id: &str, name: &str, transport: &str) {
        sqlx::query("INSERT INTO mcp_servers (id, name, transport, enabled, created_at) VALUES (?, ?, ?, 0, 'c')")
            .bind(id)
            .bind(name)
            .bind(transport)
            .execute(db)
            .await
            .unwrap();
    }

    fn file_source() -> McpSource {
        McpSource {
            kind: "S2".into(),
            reference: Some("/home/u/.claude.json".into()),
            name: Some("github".into()),
            fingerprint: Some("ab12".into()),
            connector_account: None,
        }
    }

    fn connector_source() -> McpSource {
        McpSource {
            kind: "claudeai".into(),
            reference: Some("acc-1".into()),
            name: Some("claude.ai Slack".into()),
            fingerprint: None,
            connector_account: Some("acc-1".into()),
        }
    }

    #[tokio::test]
    async fn a_server_made_in_puppeteer_has_no_source() {
        let db = test_db().await;
        add_server(&db, "m1", "github", "stdio").await;
        assert_eq!(source_of(&db, "m1").await.unwrap(), None);
        assert_eq!(source_of(&db, "missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_source_writes_and_clears_the_columns() {
        let db = test_db().await;
        add_server(&db, "m1", "github", "stdio").await;
        assert!(set_source(&db, "m1", Some(&file_source())).await.unwrap());
        assert_eq!(source_of(&db, "m1").await.unwrap(), Some(file_source()));
        assert!(set_source(&db, "m1", None).await.unwrap());
        assert_eq!(source_of(&db, "m1").await.unwrap(), None);
        assert!(!set_source(&db, "missing", Some(&file_source())).await.unwrap());
    }

    #[tokio::test]
    async fn a_backup_round_trip_keeps_the_source_columns() {
        let source = test_db().await;
        add_server(&source, "m1", "github", "stdio").await;
        add_server(&source, "m2", "claude_ai_Slack-work", "claudeai").await;
        add_server(&source, "m3", "local", "stdio").await;
        set_source(&source, "m1", Some(&file_source())).await.unwrap();
        set_source(&source, "m2", Some(&connector_source())).await.unwrap();

        let exported = collect(&source).await.unwrap();
        assert_eq!(exported.len(), 2, "a server made in Puppeteer has nothing to carry");
        let json = serde_json::to_string(&exported).unwrap();
        let back: Vec<McpSourceEntry> = serde_json::from_str(&json).unwrap();

        // The target restores the servers first (other ids), then their sources.
        let target = test_db().await;
        for (name, transport) in [("github", "stdio"), ("claude_ai_Slack-work", "claudeai")] {
            let server = McpServer {
                id: format!("backup-{name}"),
                name: name.into(),
                description: String::new(),
                transport: transport.into(),
                command: (transport == "stdio").then(|| "npx".to_string()),
                args: vec![],
                url: None,
                env_keys: vec![],
                header_keys: vec![],
                enabled: false,
                created_at: "c".into(),
            };
            import_backup_mcp(&target, &server.id, &server, None).await.unwrap();
        }
        let mut report = ImportReport::default();
        import(&target, &back, &mut report).await;
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(source_of(&target, "backup-github").await.unwrap(), Some(file_source()));
        assert_eq!(source_of(&target, "backup-claude_ai_Slack-work").await.unwrap(), Some(connector_source()));
    }

    #[tokio::test]
    async fn a_source_whose_server_is_missing_is_a_warning() {
        let db = test_db().await;
        let entries = vec![McpSourceEntry { server: "gone".into(), source: file_source() }];
        let mut report = ImportReport::default();
        import(&db, &entries, &mut report).await;
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("'gone'"));
    }

    #[test]
    fn the_backup_entry_is_flat_and_camel_case() {
        let entry = McpSourceEntry { server: "s".into(), source: connector_source() };
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["server"], "s");
        assert_eq!(v["kind"], "claudeai");
        assert_eq!(v["connectorAccount"], "acc-1");
        // Fields an older or trimmed backup leaves out default to none.
        let min: McpSourceEntry = serde_json::from_str(r#"{"server":"s","kind":"S4"}"#).unwrap();
        assert_eq!(min.source, McpSource { kind: "S4".into(), ..Default::default() });
    }
}
