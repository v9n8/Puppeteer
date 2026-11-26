//! A managed claude.ai connector at run time (FR-5, AC-014).
//!
//! A run has one Claude profile (its app account, else `global`) and only that profile's
//! connector rows enter it, as the engine's own entry `{type: 'claudeai-proxy', url, id}` from the
//! profile's last probe (`claude_connectors`), under the connector's run key (`claude.ai Slack` →
//! `claude_ai_Slack`): the tools keep the names they have in the user's terminal and the Claude
//! guard (MG-01) lets them through, since the CLI reports the entry as `dynamic`. A row of
//! another profile is left out silently; one that cannot be passed is skipped with a note: a
//! Codex run, a run key another managed server already has (C3), no probe entry (FR-5: a failed
//! probe leaves none). The app-wide / project / session switches are those of any server.

use super::claude_sources::GLOBAL_PROFILE;
use super::probe::connector_key;
use super::state::CLAUDEAI_KIND;
use crate::commands::mcp::McpServer;
use crate::commands::mcp_sources::source_of;
use crate::db::Db;
use tauri::State;

/// The Claude profile whose connectors a run on `engine` takes: its account (`claude_account`,
/// `None` = global); `None` for Codex, which takes none.
pub fn run_profile<'a>(engine: &str, claude_account: Option<&'a str>) -> Option<&'a str> {
    (engine != "codex").then(|| claude_account.unwrap_or(GLOBAL_PROFILE))
}

/// What a connector row gives a run.
#[derive(Debug, PartialEq)]
pub enum RunEntry {
    Include(String, serde_json::Value),
    /// Left out with this note.
    Skip(String),
    /// A row of another profile: not this run's business.
    OtherProfile,
}

/// The run entry of connector row `server` for a run of `profile` (see [`run_profile`]).
pub async fn run_entry(db: &Db, profile: Option<&str>, server: &McpServer) -> RunEntry {
    let Some(profile) = profile else {
        return RunEntry::Skip("Codex runs do not take claude.ai connectors".to_string());
    };
    let source = match source_of(db, &server.id).await {
        Ok(Some(source)) if source.kind == CLAUDEAI_KIND => source,
        Ok(_) => return RunEntry::Skip("not linked to a claude.ai connector".to_string()),
        Err(e) => return RunEntry::Skip(format!("its connector could not be read: {e}")),
    };
    if source.connector_account.as_deref() != Some(profile) {
        return RunEntry::OtherProfile;
    }
    let Some(name) = source.name else {
        return RunEntry::Skip("not linked to a claude.ai connector".to_string());
    };
    let key = connector_key(&name);
    let clash: Result<Option<i64>, _> = sqlx::query_scalar("SELECT 1 FROM mcp_servers WHERE name = ? COLLATE NOCASE AND id <> ?")
        .bind(&key)
        .bind(&server.id)
        .fetch_optional(db)
        .await;
    match clash {
        Ok(None) => {}
        Ok(Some(_)) => return RunEntry::Skip(format!("another MCP server is named {key}")),
        Err(e) => return RunEntry::Skip(format!("its connector could not be read: {e}")),
    }
    let cached: Result<Option<(String, String)>, _> =
        sqlx::query_as("SELECT connector_id, url FROM claude_connectors WHERE account_ref = ? AND name = ?")
            .bind(profile)
            .bind(&name)
            .fetch_optional(db)
            .await;
    match cached {
        Ok(Some((id, url))) => RunEntry::Include(key, serde_json::json!({ "type": "claudeai-proxy", "url": url, "id": id })),
        Ok(None) => RunEntry::Skip("the profile's last probe did not report this connector".to_string()),
        Err(e) => RunEntry::Skip(format!("its connector could not be read: {e}")),
    }
}

/// `resolve_run_mcp_servers`' `claudeai` arm: put the row into the run's `map`, or into
/// `skipped` with a log line (the note; no id / url in it).
pub async fn add_to_run(
    db: &Db,
    profile: Option<&str>,
    server: &McpServer,
    map: &mut serde_json::Map<String, serde_json::Value>,
    skipped: &mut Vec<String>,
) {
    let note = match run_entry(db, profile, server).await {
        RunEntry::Include(key, _) if map.contains_key(&key) => format!("another MCP server of the run is named {key}"),
        RunEntry::Include(key, entry) => {
            map.insert(key, entry);
            return;
        }
        RunEntry::OtherProfile => return,
        RunEntry::Skip(note) => note,
    };
    tracing::warn!(event = "mcp_connector_skipped", server = %server.name, note = %note);
    skipped.push(server.name.clone());
}

/// The Claude profile of run `run_id` as its next start would resolve it
/// (`claude_accounts::resolve_runtime_account`'s order: its account, else the project's, else the
/// default one, else global), read only — that function also creates the account dir. `None`
/// for a Codex run. A run's own account that no longer exists is an error: the caller then
/// shows no connector, as the run would not start.
pub async fn profile_of_run(db: &Db, run_id: &str) -> Result<Option<String>, String> {
    let err = |e: sqlx::Error| e.to_string();
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT r.engine, r.claude_account_id, p.claude_account_id FROM runs r LEFT JOIN projects p ON p.id = r.project_id \
         WHERE r.id = ?",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .map_err(err)?;
    let (engine, own, project) = row.ok_or_else(|| format!("unknown run: {run_id}"))?;
    if engine == "codex" {
        return Ok(None);
    }
    let set = |v: Option<String>| v.filter(|v| !v.trim().is_empty());
    let own = set(own);
    let wanted = match own.clone().or_else(|| set(project)) {
        Some(id) => Some(id),
        None => sqlx::query_scalar("SELECT id FROM claude_accounts WHERE is_default = 1 ORDER BY created_at LIMIT 1")
            .fetch_optional(db)
            .await
            .map_err(err)?,
    };
    let Some(wanted) = wanted else { return Ok(Some(GLOBAL_PROFILE.to_string())) };
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM claude_accounts WHERE id = ?")
        .bind(wanted.trim())
        .fetch_optional(db)
        .await
        .map_err(err)?;
    match exists {
        Some(id) => Ok(Some(id)),
        None if own.is_some() => Err("the run's Claude account no longer exists".to_string()),
        None => Ok(Some(GLOBAL_PROFILE.to_string())),
    }
}

/// Switch a connector row on or off app-wide. (`update_mcp_server` takes launch fields, which a
/// connector has none of.) Turning it off keeps the project and session choices below it.
pub async fn set_connector_enabled(db: &Db, server_id: &str, enabled: bool) -> Result<(), String> {
    let done = sqlx::query("UPDATE mcp_servers SET enabled = ? WHERE id = ? AND transport = ?")
        .bind(enabled)
        .bind(server_id)
        .bind(CLAUDEAI_KIND)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    if done.rows_affected() == 0 {
        return Err(format!("not a claude.ai connector: {server_id}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn set_mcp_connector_enabled(db: State<'_, Db>, server_id: String, enabled: bool) -> Result<(), String> {
    set_connector_enabled(db.inner(), &server_id, enabled).await
}

#[cfg(test)]
mod tests {
    use super::super::import_sync::import_rows;
    use super::super::probe::{store_connectors, ProbedServer};
    use super::*;
    use crate::commands::mcp::{resolve_run_mcp_servers, set_project_mcp_server_enabled, McpLaunch};
    use crate::commands::run_mcp::{list_run_mcp, run_mcp_selection, set_override};
    use std::collections::HashSet;

    fn launch() -> McpLaunch {
        McpLaunch {
            node_bin: "/opt/node".into(),
            launcher: "/l.mjs".into(),
            broker_sock: "/tmp/b.sock".into(),
            project_id: "p1".into(),
            run_id: "r1".into(),
        }
    }

    fn connector(name: &str, id: &str) -> ProbedServer {
        serde_json::from_value(serde_json::json!({
            "name": name, "source": "claudeai", "status": "connected", "transport": "claudeai-proxy",
            "connector": {"id": id, "url": format!("https://mcp/{id}")}
        }))
        .unwrap()
    }

    /// Accounts A and B (neither default), Slack probed for A, B and global and imported for
    /// each, all on app-wide and for project p1. Runs: r1 and r2 on A, rb on B, rg on none (so
    /// global), rc a Codex run.
    async fn setup() -> (Db, String, String, String) {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('acc-a', 'A', '/ca', 'c'), ('acc-b', 'B', '/cb', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at, claude_account_id, engine) VALUES
                ('r1', 'p1', 'session', 'c', 'acc-a', 'claude'), ('r2', 'p1', 'session', 'c', 'acc-a', 'claude'),
                ('rb', 'p1', 'session', 'c', 'acc-b', 'claude'), ('rg', 'p1', 'session', 'c', NULL, 'claude'),
                ('rc', 'p1', 'session', 'c', NULL, 'codex')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        for (profile, id) in [("acc-a", "ca"), ("acc-b", "cb"), ("global", "cg")] {
            store_connectors(&db, profile, &Ok(vec![connector("claude.ai Slack", id)]), "t").await.unwrap();
        }
        let ids = ["S1|acc-a|claude.ai Slack", "S1|acc-b|claude.ai Slack", "S1|global|claude.ai Slack"].map(String::from);
        let no_secret = |_: &str,
                         _: &std::collections::HashMap<String, String>,
                         _: &std::collections::HashMap<String, String>|
         -> Result<(), String> { Ok(()) };
        import_rows(&db, &[], &ids, &no_secret).await;
        let id = |account: &'static str| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, String>("SELECT id FROM mcp_servers WHERE connector_account = ?")
                    .bind(account)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };
        let (a, b, g) = (id("acc-a").await, id("acc-b").await, id("global").await);
        for server in [&a, &b, &g] {
            set_connector_enabled(&db, server, true).await.unwrap();
            set_project_mcp_server_enabled(&db, "p1", server, true).await.unwrap();
        }
        (db, a, b, g)
    }

    /// The run's MCP map, as the launch builds it.
    async fn run_map(db: &Db, run_id: &str, engine: &str, account: Option<&str>) -> (serde_json::Value, Vec<String>) {
        let selection = run_mcp_selection(db, "p1", run_id).await.unwrap();
        resolve_run_mcp_servers(db, &selection.servers, engine, &launch(), run_profile(engine, account)).await
    }

    #[tokio::test]
    async fn a_run_gets_its_profiles_connector_under_the_run_key_only() {
        let (db, ..) = setup().await;
        let (map, skipped) = run_map(&db, "r1", "claude", Some("acc-a")).await;
        assert_eq!(map, serde_json::json!({ "claude_ai_Slack": { "type": "claudeai-proxy", "url": "https://mcp/ca", "id": "ca" } }));
        assert!(skipped.is_empty(), "other profiles' rows are left out silently: {skipped:?}");
        let (map, _) = run_map(&db, "rb", "claude", Some("acc-b")).await;
        assert_eq!(map["claude_ai_Slack"]["id"], "cb");
        let (map, _) = run_map(&db, "rg", "claude", None).await;
        assert_eq!(map["claude_ai_Slack"]["id"], "cg");
        // Codex takes none, with a note.
        let (map, skipped) = run_map(&db, "rc", "codex", None).await;
        assert!(map.is_null());
        assert_eq!(skipped.len(), 3);
    }

    #[tokio::test]
    async fn a_clashing_key_or_a_missing_probe_entry_skips_the_connector() {
        let (db, a, ..) = setup().await;
        sqlx::query("INSERT INTO mcp_servers (id, name, transport, command, enabled, created_at) VALUES ('u1', 'claude_ai_Slack', 'stdio', 'x', 1, 'c')")
            .execute(&db)
            .await
            .unwrap();
        set_project_mcp_server_enabled(&db, "p1", "u1", true).await.unwrap();
        let (map, skipped) = run_map(&db, "r1", "claude", Some("acc-a")).await;
        assert_eq!(map["claude_ai_Slack"]["type"], "stdio", "the user's own server keeps its name");
        assert_eq!(skipped, ["claude_ai_Slack-A"]);
        sqlx::query("DELETE FROM mcp_servers WHERE id = 'u1'").execute(&db).await.unwrap();

        // A failed probe empties the profile's connectors (MG-08): fail closed.
        store_connectors(&db, "acc-a", &Err("no login".into()), "t2").await.unwrap();
        let (map, skipped) = run_map(&db, "r1", "claude", Some("acc-a")).await;
        assert!(map.is_null());
        assert_eq!(skipped, ["claude_ai_Slack-A"]);
        let server = McpServer {
            id: a,
            name: "claude_ai_Slack-A".into(),
            description: String::new(),
            transport: "claudeai".into(),
            command: None,
            args: vec![],
            url: None,
            env_keys: vec![],
            header_keys: vec![],
            enabled: true,
            created_at: "c".into(),
        };
        assert_eq!(
            run_entry(&db, Some("acc-a"), &server).await,
            RunEntry::Skip("the profile's last probe did not report this connector".into())
        );
        assert_eq!(run_entry(&db, Some("acc-b"), &server).await, RunEntry::OtherProfile);
    }

    /// FR-12 on connector rows: off at any level → not in the run; a child cannot be switched on
    /// while its parent is off; switching a child off leaves the parent and the siblings on.
    #[tokio::test]
    async fn every_level_switches_a_connector_and_a_parent_wins() {
        let (db, a, ..) = setup().await;
        let key = format!("server:{a}");
        let in_run = |run: &'static str| {
            let db = db.clone();
            async move { !run_map(&db, run, "claude", Some("acc-a")).await.0.is_null() }
        };
        assert!(in_run("r1").await);

        // Session off: r1 loses it, r2 and the project and app-wide switches keep it.
        set_override(&db, "r1", &key, Some(false)).await.unwrap();
        assert!(!in_run("r1").await && in_run("r2").await);
        let enabled: bool = sqlx::query_scalar("SELECT enabled FROM mcp_servers WHERE id = ?").bind(&a).fetch_one(&db).await.unwrap();
        assert!(enabled);
        assert!(run_mcp_selection(&db, "p1", "r2").await.unwrap().servers.contains(&a));
        set_override(&db, "r1", &key, None).await.unwrap();

        // Project off: no session of it gets it, and a session cannot switch it back on.
        set_project_mcp_server_enabled(&db, "p1", &a, false).await.unwrap();
        assert!(!in_run("r1").await && !in_run("r2").await);
        assert!(set_override(&db, "r1", &key, Some(true)).await.is_err());
        let menu = list_run_mcp(&db, "r1").await.unwrap();
        let entry = menu.iter().find(|e| e.key == key).unwrap();
        assert_eq!((entry.enabled, entry.locked_by), (false, Some("project")));
        set_project_mcp_server_enabled(&db, "p1", &a, true).await.unwrap();

        // App-wide off: nothing gets it, the project and session cannot switch it on.
        set_connector_enabled(&db, &a, false).await.unwrap();
        assert!(!in_run("r1").await);
        assert!(set_project_mcp_server_enabled(&db, "p1", &a, true).await.is_err());
        assert!(set_override(&db, "r1", &key, Some(true)).await.is_err());
        let menu = list_run_mcp(&db, "r1").await.unwrap();
        assert_eq!(menu.iter().find(|e| e.key == key).unwrap().locked_by, Some("app"));
        // Only connector rows take this switch.
        sqlx::query("INSERT INTO mcp_servers (id, name, transport, command, enabled, created_at) VALUES ('u1', 'u', 'stdio', 'x', 1, 'c')")
            .execute(&db)
            .await
            .unwrap();
        assert!(set_connector_enabled(&db, "u1", false).await.is_err());
    }

    /// AC-022 on a connector row: switched off for one project (or one session), it stays on app-wide
    /// and in the other project, and that project's sessions still get it.
    #[tokio::test]
    async fn a_connector_switched_off_for_one_project_stays_on_in_another() {
        let (db, a, ..) = setup().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p2', 'b', '/b', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at, claude_account_id, engine) VALUES ('q1', 'p2', 'session', 'c', 'acc-a', 'claude')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        set_project_mcp_server_enabled(&db, "p2", &a, true).await.unwrap();
        let in_run = |project: &'static str, run: &'static str| {
            let db = db.clone();
            async move {
                let selection = run_mcp_selection(&db, project, run).await.unwrap();
                let (map, _) = resolve_run_mcp_servers(&db, &selection.servers, "claude", &launch(), run_profile("claude", Some("acc-a"))).await;
                map.get("claude_ai_Slack").and_then(|c| c["id"].as_str()).map(str::to_string)
            }
        };
        assert_eq!((in_run("p1", "r1").await, in_run("p2", "q1").await), (Some("ca".into()), Some("ca".into())));

        // Project p1 off: its sessions lose it; p2's switch, its session and the app-wide switch keep it.
        set_project_mcp_server_enabled(&db, "p1", &a, false).await.unwrap();
        assert_eq!((in_run("p1", "r1").await, in_run("p1", "r2").await), (None, None));
        assert_eq!(in_run("p2", "q1").await, Some("ca".into()));
        let on_for = |project: &'static str| {
            let db = db.clone();
            let a = a.clone();
            async move {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM project_mcp_servers WHERE project_id = ? AND server_id = ?")
                    .bind(project)
                    .bind(&a)
                    .fetch_one(&db)
                    .await
                    .unwrap()
                    == 1
            }
        };
        assert!(!on_for("p1").await && on_for("p2").await);
        let enabled: bool = sqlx::query_scalar("SELECT enabled FROM mcp_servers WHERE id = ?").bind(&a).fetch_one(&db).await.unwrap();
        assert!(enabled, "the app-wide switch is the parent: a project never turns it off");
        let menu = list_run_mcp(&db, "q1").await.unwrap();
        let entry = menu.iter().find(|e| e.key == format!("server:{a}")).unwrap();
        assert_eq!((entry.enabled, entry.locked_by), (true, None));

        // A session of p2 off: the other project is not touched either way.
        set_project_mcp_server_enabled(&db, "p1", &a, true).await.unwrap();
        set_override(&db, "q1", &format!("server:{a}"), Some(false)).await.unwrap();
        assert_eq!((in_run("p2", "q1").await, in_run("p1", "r1").await), (None, Some("ca".into())));
        assert!(on_for("p1").await && on_for("p2").await);
    }

    #[test]
    fn the_connector_switch_is_audited_over_remote() {
        use crate::remote::{guard::{audited_command, refuses_invoke}, ipc::BRIDGE_LABEL};
        assert!(audited_command("set_mcp_connector_enabled"));
        assert!(!refuses_invoke(BRIDGE_LABEL, "set_mcp_connector_enabled"));
    }

    /// The session menu lists only the run's profile's connector rows.
    #[tokio::test]
    async fn the_session_menu_shows_the_runs_profile_only() {
        let (db, a, b, g) = setup().await;
        let keys = |run: &'static str| {
            let db = db.clone();
            async move {
                list_run_mcp(&db, run).await.unwrap().into_iter().filter(|e| !e.builtin).map(|e| e.key).collect::<HashSet<_>>()
            }
        };
        assert_eq!(keys("r1").await, HashSet::from([format!("server:{a}")]));
        assert_eq!(keys("rb").await, HashSet::from([format!("server:{b}")]));
        assert_eq!(keys("rg").await, HashSet::from([format!("server:{g}")]));
        assert!(keys("rc").await.is_empty());
        assert_eq!(profile_of_run(&db, "rg").await.unwrap().as_deref(), Some("global"));
        assert_eq!(profile_of_run(&db, "rc").await.unwrap(), None);
        // The project's account, then the default one, apply to a run with none.
        sqlx::query("UPDATE claude_accounts SET is_default = 1 WHERE id = 'acc-b'").execute(&db).await.unwrap();
        assert_eq!(profile_of_run(&db, "rg").await.unwrap().as_deref(), Some("acc-b"));
        sqlx::query("UPDATE projects SET claude_account_id = 'acc-a'").execute(&db).await.unwrap();
        assert_eq!(profile_of_run(&db, "rg").await.unwrap().as_deref(), Some("acc-a"));
        // A run whose own account is gone (the foreign key normally clears it): no connector at all.
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&db).await.unwrap();
        sqlx::query("UPDATE runs SET claude_account_id = 'gone' WHERE id = 'rb'").execute(&db).await.unwrap();
        assert!(profile_of_run(&db, "rb").await.is_err());
        assert!(keys("rb").await.is_empty());
    }
}
