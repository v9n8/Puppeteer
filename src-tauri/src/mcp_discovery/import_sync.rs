//! Import a discovered server into the MCP list, and Sync a managed copy from its source.
//!
//! Both read a fresh scan (its exports hold the secret VALUEs, which go straight to the vault
//! sink) and never write a source file (FR-11). An import goes through the import pipeline
//! (`import::apply::import_server`: name clash → `<name>-2`, secrets to the vault), starts off
//! app-wide (Q8) and remembers its source (`commands::mcp_sources`). A claude.ai connector has no
//! launch fields: its import is a `claudeai` row of that one Claude profile, named
//! `<run key>-<profile>` (names are unique; the run key stays the connector's own, see
//! `connectors`). Sync is one-way, source → copy: launch fields, key names and VALUEs are
//! replaced; the name and every switch (app-wide, project, session) are kept.

use super::probe::connector_key;
use super::state::{managed_copy, managed_servers, CLAUDEAI_KIND};
use super::{claude_sources::GLOBAL_PROFILE, Found};
use crate::commands::mcp::{self, McpServer, McpServerExport, SecretEntry};
use crate::commands::mcp_sources::{set_source, source_of, McpSource};
use crate::db::Db;
use crate::import::apply::{import_server, OnConflict, Outcome, Pick, SecretSink, Status};
use crate::import::ImportKind;
use std::path::Path;

fn outcome(id: &str, result: Result<(Status, String), String>) -> Outcome {
    let (status, name, message) = match result {
        Ok((status, name)) => (status, Some(name), None),
        Err(e) => (Status::Failed, None, Some(e)),
    };
    Outcome { id: id.to_string(), kind: Some(ImportKind::Mcp), status, name, message }
}

const GONE: &str = "not found in the source any more";

/// Import each of `row_ids`: file rows from `found` (a fresh scan), connector rows (`S1|…`) from
/// the profile's last probe. One outcome per id; a failure does not stop the others.
pub async fn import_rows(db: &Db, found: &[Found], row_ids: &[String], store_secrets: SecretSink<'_>) -> Vec<Outcome> {
    let mut out = Vec::with_capacity(row_ids.len());
    for id in row_ids {
        let result = match id.strip_prefix("S1|").and_then(|rest| rest.split_once('|')) {
            Some((profile, name)) => import_connector(db, profile, name).await,
            None => match found.iter().find(|f| &f.row.id == id) {
                Some(f) => import_found(db, f, store_secrets).await,
                None => Err(GONE.to_string()),
            },
        };
        if let Err(e) = &result {
            tracing::warn!(event = "mcp_discovery_import_failed", error = %e);
        }
        out.push(outcome(id, result));
    }
    out
}

async fn import_found(db: &Db, found: &Found, store_secrets: SecretSink<'_>) -> Result<(Status, String), String> {
    let row = &found.row;
    if let Some(problem) = &row.problem {
        return Err(format!("the entry cannot be read: {problem}"));
    }
    if !row.importable {
        return Err("this server is listed and blocked only, it cannot be imported".to_string());
    }
    let managed = managed_servers(db).await?;
    if let Some(copy) = managed_copy(row, &managed) {
        return Ok((Status::Skipped, copy.name.clone()));
    }
    let export = found.export.as_ref().ok_or(GONE)?;
    let export = McpServerExport { enabled: false, ..export.clone() };
    let candidate = crate::import::mcp_candidate(Path::new(&row.origin), &row.name, export);
    let pick = Pick { id: candidate.id.clone(), name: None, on_conflict: OnConflict::Rename };
    let (status, name) = import_server(db, &candidate, &pick, store_secrets).await?;
    let id: String = sqlx::query_scalar("SELECT id FROM mcp_servers WHERE name = ?")
        .bind(&name)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    let source = McpSource {
        kind: row.source.as_str().to_string(),
        reference: Some(row.source_ref.clone()),
        name: Some(row.name.clone()),
        fingerprint: row.fingerprint.clone(),
        connector_account: None,
    };
    if let Err(e) = set_source(db, &id, Some(&source)).await {
        // A copy that forgot its source would show as a second, unmanaged-looking server.
        let _ = sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(&id).execute(db).await;
        return Err(format!("the source could not be recorded, server not imported: {e}"));
    }
    Ok((status, name))
}

/// The name part a profile gives its connector copies: `global`, or the account's label.
async fn profile_slug(db: &Db, profile: &str) -> Result<String, String> {
    if profile == GLOBAL_PROFILE {
        return Ok(GLOBAL_PROFILE.to_string());
    }
    let label: Option<String> = sqlx::query_scalar("SELECT label FROM claude_accounts WHERE id = ?")
        .bind(profile)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let label = label.ok_or("this Claude account no longer exists")?;
    Ok(crate::import::library_name(&label))
}

/// `base`, else `base-2`, `base-3`, …: the first name the MCP list does not have.
async fn free_server_name(db: &Db, base: &str) -> Result<String, String> {
    let mut name = base.to_string();
    for n in 2.. {
        if !crate::import::name_taken(db, ImportKind::Mcp, &name).await? {
            break;
        }
        name = format!("{base}-{n}");
    }
    Ok(name)
}

async fn import_connector(db: &Db, profile: &str, name: &str) -> Result<(Status, String), String> {
    let seen: Option<i64> = sqlx::query_scalar("SELECT 1 FROM claude_connectors WHERE account_ref = ? AND name = ?")
        .bind(profile)
        .bind(name)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    if seen.is_none() {
        return Err("this connector is not in the profile's last probe: reload and try again".to_string());
    }
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT name FROM mcp_servers WHERE source_kind = ? AND connector_account = ? AND source_name = ?",
    )
    .bind(CLAUDEAI_KIND)
    .bind(profile)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(existing) = existing {
        return Ok((Status::Skipped, existing));
    }
    let server = free_server_name(db, &format!("{}-{}", connector_key(name), profile_slug(db, profile).await?)).await?;
    sqlx::query(
        "INSERT INTO mcp_servers (id, name, description, transport, args, env_keys, header_keys, enabled, created_at, \
         source_kind, source_ref, source_name, connector_account) VALUES (?, ?, ?, ?, '[]', '[]', '[]', 0, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&server)
    .bind(name)
    .bind(CLAUDEAI_KIND)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(CLAUDEAI_KIND)
    .bind(profile)
    .bind(name)
    .bind(profile)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok((Status::Imported, server))
}

fn entries(map: &std::collections::HashMap<String, String>) -> Vec<SecretEntry> {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    keys.into_iter().map(|k| SecretEntry { key: k.clone(), value: None }).collect()
}

/// Make copy `server_id` equal its source entry in `found` (a fresh scan). Keeps the name and the
/// switches; returns the updated server.
pub async fn sync_server(db: &Db, found: &[Found], server_id: &str, store_secrets: SecretSink<'_>) -> Result<McpServer, String> {
    let source = source_of(db, server_id).await?.ok_or("this server was not imported from a source")?;
    if source.kind == CLAUDEAI_KIND {
        return Err("a claude.ai connector has nothing to sync".to_string());
    }
    let entry = found
        .iter()
        .find(|f| {
            f.row.source.as_str() == source.kind
                && Some(&f.row.source_ref) == source.reference.as_ref()
                && Some(&f.row.name) == source.name.as_ref()
        })
        .ok_or("its source no longer has this server")?;
    if let Some(problem) = &entry.row.problem {
        return Err(format!("the source entry cannot be read: {problem}"));
    }
    let export = entry.export.as_ref().ok_or("its source no longer has this server")?;
    let (env, headers) = (entries(&export.env), entries(&export.headers));
    mcp::validate_shape(&export.transport, &export.command, &export.url, &env, &headers)?;
    let (command, url) = mcp::normalized_fields(&export.transport, export.command.clone(), export.url.clone());
    let keys = |e: &[SecretEntry]| serde_json::to_string(&e.iter().map(|e| &e.key).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into());
    // VALUEs first: a failed vault write leaves the copy as it was.
    store_secrets(server_id, &export.env, &export.headers).map_err(|e| format!("secrets could not be stored, server not synced: {e}"))?;
    sqlx::query(
        "UPDATE mcp_servers SET transport = ?, command = ?, args = ?, url = ?, env_keys = ?, header_keys = ?, \
         source_fingerprint = ? WHERE id = ?",
    )
    .bind(&export.transport)
    .bind(&command)
    .bind(serde_json::to_string(&export.args).unwrap_or_else(|_| "[]".into()))
    .bind(&url)
    .bind(keys(&env))
    .bind(keys(&headers))
    .bind(&entry.row.fingerprint)
    .bind(server_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    let row = sqlx::query(&format!("SELECT {} FROM mcp_servers WHERE id = ?", mcp::SELECT_COLS))
        .bind(server_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(mcp::row_to_server(&row))
}

#[cfg(test)]
mod tests {
    use super::super::claude_sources::tests::{inputs, tmp, write};
    use super::super::probe::{merge_probes, store_connectors, ProbedServer};
    use super::super::state::{state_of, RowState};
    use super::super::{scan_files, DiscoveryInputs, ProjectDir};
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    async fn test_db() -> Db {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')").execute(&pool).await.unwrap();
        pool
    }

    /// Secrets the sink received, by server id.
    type Vault = Mutex<HashMap<String, (HashMap<String, String>, HashMap<String, String>)>>;

    fn sink(vault: &Vault) -> impl Fn(&str, &HashMap<String, String>, &HashMap<String, String>) -> Result<(), String> + Sync + '_ {
        move |id: &str, env: &HashMap<String, String>, headers: &HashMap<String, String>| {
            vault.lock().unwrap().insert(id.to_string(), (env.clone(), headers.clone()));
            Ok(())
        }
    }

    /// Every text the DB holds about MCP servers (to look for leaked VALUEs).
    async fn db_text(db: &Db) -> String {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT COALESCE(name,'') || COALESCE(description,'') || COALESCE(command,'') || COALESCE(args,'') || COALESCE(url,'') \
             || COALESCE(env_keys,'') || COALESCE(header_keys,'') || COALESCE(source_ref,'') || COALESCE(source_name,'') \
             || COALESCE(source_fingerprint,'') FROM mcp_servers",
        )
        .fetch_all(db)
        .await
        .unwrap();
        rows.into_iter().map(|r| r.0).collect()
    }

    /// HOME `.claude.json` (global user scope `x` with a secret env, `h` with a secret header),
    /// a project `.mcp.json` (`y`), a managed file (`corp`).
    fn machine(base: &Path) -> (DiscoveryInputs, Vec<std::path::PathBuf>) {
        let mut inputs = inputs(base);
        let project = base.join("work/app");
        write(
            &inputs.home.join(".claude.json"),
            r#"{"mcpServers": {"x": {"command": "x", "args": ["--a"], "env": {"X_KEY": "SECRET-X"}},
                               "h": {"type": "http", "url": "https://h", "headers": {"Authorization": "SECRET-H"}}}}"#,
        );
        write(&project.join(".mcp.json"), r#"{"mcpServers": {"y": {"command": "y"}}}"#);
        write(&inputs.claude_managed_dir.join("managed-mcp.json"), r#"{"mcpServers": {"corp": {"url": "https://corp"}}}"#);
        let files = vec![inputs.home.join(".claude.json"), project.join(".mcp.json"), inputs.claude_managed_dir.join("managed-mcp.json")];
        inputs.projects.push(ProjectDir { id: "p1".into(), path: project });
        (inputs, files)
    }

    fn bytes(files: &[std::path::PathBuf]) -> Vec<Vec<u8>> {
        files.iter().map(|f| std::fs::read(f).unwrap()).collect()
    }

    fn id_of(found: &[Found], name: &str) -> String {
        found.iter().find(|f| f.row.name == name).unwrap().row.id.clone()
    }

    #[tokio::test]
    async fn an_import_copies_off_app_wide_with_its_source_and_secrets_only_in_the_vault() {
        let base = tmp("import");
        let (inputs, files) = machine(&base);
        let before = bytes(&files);
        let db = test_db().await;
        // `x` is taken in the MCP list: the copy is renamed.
        sqlx::query("INSERT INTO mcp_servers (id, name, transport, command, enabled, created_at) VALUES ('own', 'x', 'stdio', 'mine', 1, 'c')")
            .execute(&db)
            .await
            .unwrap();
        let vault = Vault::default();
        let found = scan_files(&inputs).found;
        let ids = vec![id_of(&found, "x"), id_of(&found, "h"), id_of(&found, "y"), id_of(&found, "corp"), "S2|nowhere|z".to_string()];
        let out = import_rows(&db, &found, &ids, &sink(&vault)).await;
        let view: Vec<_> = out.iter().map(|o| (o.status, o.name.clone())).collect();
        assert_eq!(
            view,
            [
                (Status::Imported, Some("x-2".into())),
                (Status::Imported, Some("h".into())),
                (Status::Imported, Some("y".into())),
                (Status::Failed, None),
                (Status::Failed, None),
            ]
        );
        assert_eq!(out[4].message.as_deref(), Some(GONE));

        let managed = managed_servers(&db).await.unwrap();
        for name in ["x", "h", "y"] {
            let f = found.iter().find(|f| f.row.name == name).unwrap();
            let (state, copy) = state_of(&f.row, &managed);
            let copy = copy.unwrap();
            assert_eq!(state, RowState::Managed, "{name}");
            assert!(!copy.enabled, "{name} starts off app-wide");
            assert_eq!(copy.source_kind.as_deref(), Some(f.row.source.as_str()));
            assert_eq!(copy.source_fingerprint, f.row.fingerprint);
        }
        let x = managed.iter().find(|m| m.name == "x-2").unwrap();
        let h = managed.iter().find(|m| m.name == "h").unwrap();
        let stored = vault.lock().unwrap().clone();
        assert_eq!(stored[&x.id].0["X_KEY"], "SECRET-X");
        assert_eq!(stored[&h.id].1["Authorization"], "SECRET-H");
        assert!(!db_text(&db).await.contains("SECRET"));
        assert_eq!(bytes(&files), before, "source files are only read");

        // Again: already managed, skipped under the copy's name.
        let again = import_rows(&db, &scan_files(&inputs).found, &ids[..1], &sink(&vault)).await;
        assert_eq!((again[0].status, again[0].name.as_deref()), (Status::Skipped, Some("x-2")));
        assert_eq!(managed_servers(&db).await.unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn a_sync_replaces_launch_and_secrets_and_keeps_every_switch() {
        let base = tmp("sync");
        let (inputs, files) = machine(&base);
        let db = test_db().await;
        let vault = Vault::default();
        let found = scan_files(&inputs).found;
        import_rows(&db, &found, &[id_of(&found, "x")], &sink(&vault)).await;
        let id: String = sqlx::query_scalar("SELECT id FROM mcp_servers WHERE name = 'x'").fetch_one(&db).await.unwrap();
        // The user turned it on everywhere, then off for one session.
        for sql in [
            format!("UPDATE mcp_servers SET enabled = 1, description = 'kept' WHERE id = '{id}'"),
            format!("INSERT INTO project_mcp_servers (project_id, server_id, enabled_at) VALUES ('p1', '{id}', 'c')"),
            format!("INSERT INTO run_mcp_overrides (run_id, server_key, enabled, updated_at) VALUES ('r1', 'server:{id}', 0, 'c')"),
        ] {
            sqlx::query(&sql).execute(&db).await.unwrap();
        }

        write(
            &inputs.home.join(".claude.json"),
            r#"{"mcpServers": {"x": {"command": "x2", "args": ["--b"], "env": {"X_NEW": "SECRET-NEW"}}}}"#,
        );
        let before = bytes(&files);
        let found = scan_files(&inputs).found;
        let row = &found.iter().find(|f| f.row.name == "x").unwrap().row;
        assert_eq!(state_of(row, &managed_servers(&db).await.unwrap()).0, RowState::Changed);

        let server = sync_server(&db, &found, &id, &sink(&vault)).await.unwrap();
        assert_eq!((server.name.as_str(), server.command.as_deref(), server.args.clone()), ("x", Some("x2"), vec!["--b".to_string()]));
        assert_eq!((server.env_keys.clone(), server.enabled, server.description.as_str()), (vec!["X_NEW".to_string()], true, "kept"));
        assert_eq!(vault.lock().unwrap()[&id].0, HashMap::from([("X_NEW".to_string(), "SECRET-NEW".to_string())]));
        assert_eq!(state_of(row, &managed_servers(&db).await.unwrap()).0, RowState::Managed);
        let project: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_mcp_servers WHERE server_id = ?").bind(&id).fetch_one(&db).await.unwrap();
        let session: Option<bool> = sqlx::query_scalar("SELECT enabled FROM run_mcp_overrides WHERE server_key = ?")
            .bind(format!("server:{id}"))
            .fetch_optional(&db)
            .await
            .unwrap();
        assert_eq!((project, session), (1, Some(false)));
        assert!(!db_text(&db).await.contains("SECRET"));
        assert_eq!(bytes(&files), before, "a sync only reads the source");

        // The source dropped it; a server made in Puppeteer; an unknown id.
        write(&inputs.home.join(".claude.json"), r#"{"mcpServers": {}}"#);
        let gone = sync_server(&db, &scan_files(&inputs).found, &id, &sink(&vault)).await.unwrap_err();
        assert_eq!(gone, "its source no longer has this server");
        sqlx::query("INSERT INTO mcp_servers (id, name, transport, command, enabled, created_at) VALUES ('own', 'own', 'stdio', 'o', 1, 'c')")
            .execute(&db)
            .await
            .unwrap();
        assert!(sync_server(&db, &found, "own", &sink(&vault)).await.is_err());
        assert!(sync_server(&db, &found, "nope", &sink(&vault)).await.is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn connector(name: &str) -> ProbedServer {
        serde_json::from_value(serde_json::json!({
            "name": name, "source": "claudeai", "status": "connected", "transport": "claudeai-proxy",
            "connector": {"id": "conn", "url": "https://c"}
        }))
        .unwrap()
    }

    /// Account A's Slack becomes A's copy only: B's and global's Slack stay not managed.
    #[tokio::test]
    async fn a_connector_import_manages_that_profile_only() {
        let db = test_db().await;
        sqlx::query(
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('acc-a', 'Work A', '/a', 'c'), ('acc-b', 'Home', '/b', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        let ok: Result<Vec<ProbedServer>, String> = Ok(vec![connector("claude.ai Slack")]);
        for profile in ["acc-a", "acc-b", "global"] {
            store_connectors(&db, profile, &ok, "t").await.unwrap();
        }
        let vault = Vault::default();
        let ids = ["S1|acc-a|claude.ai Slack".to_string(), "S1|acc-a|claude.ai Drive".to_string()];
        let out = import_rows(&db, &[], &ids, &sink(&vault)).await;
        assert_eq!((out[0].status, out[0].name.as_deref()), (Status::Imported, Some("claude_ai_Slack-Work-A")));
        assert_eq!(out[1].status, Status::Failed, "not in the profile's probe");

        let managed = managed_servers(&db).await.unwrap();
        assert_eq!(managed.len(), 1);
        let copy = &managed[0];
        assert_eq!(
            (copy.source_kind.as_deref(), copy.source_ref.as_deref(), copy.source_name.as_deref(), copy.connector_account.as_deref(), copy.enabled),
            (Some("claudeai"), Some("acc-a"), Some("claude.ai Slack"), Some("acc-a"), false)
        );
        let transport: String = sqlx::query_scalar("SELECT transport FROM mcp_servers").fetch_one(&db).await.unwrap();
        assert_eq!(transport, "claudeai");
        let report = merge_probes([("acc-a", &ok), ("acc-b", &ok), ("global", &ok)]).into_report();
        let states: Vec<_> = report.servers.iter().map(|r| (r.id.as_str(), state_of(r, &managed).0)).collect();
        assert_eq!(
            states,
            [
                ("S1|acc-a|claude.ai Slack", RowState::Managed),
                ("S1|acc-b|claude.ai Slack", RowState::NotManaged),
                ("S1|global|claude.ai Slack", RowState::NotManaged),
            ]
        );
        assert!(vault.lock().unwrap().is_empty(), "a connector has no secret");

        // Global's own copy, then A's again (skipped).
        let out = import_rows(&db, &[], &["S1|global|claude.ai Slack".to_string(), ids[0].clone()], &sink(&vault)).await;
        assert_eq!((out[0].status, out[0].name.as_deref()), (Status::Imported, Some("claude_ai_Slack-global")));
        assert_eq!((out[1].status, out[1].name.as_deref()), (Status::Skipped, Some("claude_ai_Slack-Work-A")));
        // A connector has nothing to sync.
        let id: String = sqlx::query_scalar("SELECT id FROM mcp_servers WHERE connector_account = 'global'").fetch_one(&db).await.unwrap();
        assert_eq!(sync_server(&db, &[], &id, &sink(&vault)).await.unwrap_err(), "a claude.ai connector has nothing to sync");
    }
}
