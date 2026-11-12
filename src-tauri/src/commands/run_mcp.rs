//! MCP per session (UX-27).
//!
//! A session gets the MCP servers its project switches on (`project_mcp_servers`, and
//! the built-ins not in `project_builtin_mcp_disabled`), and can switch a server on or
//! off for itself. Only that difference is stored (`run_mcp_overrides`), so a project
//! change still reaches every session that has not overridden that server.
//!
//! [`run_mcp_selection`] is the one place that merges the two: the run launch (start and
//! resume, `runs::assemble_run_mcp`) and the credential broker both read it. App-wide
//! switches (a server's `enabled`, the built-in switches and features) still apply on
//! top, at launch. The engines load MCP servers when their process starts, so a change
//! reaches a session at its next start or resume, not the turn in flight.
//!
//! A Conductor worker is a run of its own: it gets its project's set and its own
//! overrides, never the conductor's.

use crate::commands::mcp::{builtin_mcp_globally_enabled, BUILTIN_MCP_NAMES};
use crate::db::Db;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

/// `server_key` of a user server: this prefix and its `mcp_servers.id`.
const SERVER_KEY: &str = "server:";
/// `server_key` of a built-in server: this prefix and its name.
const BUILTIN_KEY: &str = "builtin:";

/// The servers one run is set to get: its project's set with the run's overrides.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunMcpSelection {
    /// `mcp_servers.id` of the user servers.
    pub servers: HashSet<String>,
    /// Names of the built-in servers.
    pub builtins: HashSet<String>,
}

/// Project set ± overrides: an override decides its server, the project the rest.
fn effective_selection(
    project_servers: &[String],
    project_builtins_off: &[String],
    overrides: &[(String, bool)],
) -> RunMcpSelection {
    let mut servers: HashSet<String> = project_servers.iter().cloned().collect();
    let mut builtins: HashSet<String> = BUILTIN_MCP_NAMES
        .iter()
        .filter(|n| !project_builtins_off.iter().any(|off| off == *n))
        .map(|n| n.to_string())
        .collect();
    for (key, on) in overrides {
        let (set, name) = if let Some(id) = key.strip_prefix(SERVER_KEY) {
            (&mut servers, id)
        } else if let Some(name) = key.strip_prefix(BUILTIN_KEY) {
            (&mut builtins, name)
        } else {
            continue;
        };
        if *on {
            set.insert(name.to_string());
        } else {
            set.remove(name);
        }
    }
    RunMcpSelection { servers, builtins }
}

async fn project_servers(db: &Db, project_id: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar("SELECT server_id FROM project_mcp_servers WHERE project_id = ?")
        .bind(project_id)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())
}

async fn project_builtins_off(db: &Db, project_id: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar("SELECT name FROM project_builtin_mcp_disabled WHERE project_id = ?")
        .bind(project_id)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())
}

async fn overrides(db: &Db, run_id: &str) -> Result<Vec<(String, bool)>, String> {
    sqlx::query_as::<_, (String, bool)>(
        "SELECT server_key, enabled FROM run_mcp_overrides WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

/// What run `run_id` of `project_id` is set to get (see the module docs).
pub async fn run_mcp_selection(db: &Db, project_id: &str, run_id: &str) -> Result<RunMcpSelection, String> {
    Ok(effective_selection(
        &project_servers(db, project_id).await?,
        &project_builtins_off(db, project_id).await?,
        &overrides(db, run_id).await?,
    ))
}

/// Whether the run may use user server `id` now: in its set and switched on app-wide.
pub async fn run_uses_server(db: &Db, project_id: &str, run_id: &str, id: &str) -> Result<bool, String> {
    let on: Option<i64> = sqlx::query_scalar("SELECT 1 FROM mcp_servers WHERE id = ? AND enabled = 1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(on.is_some() && run_mcp_selection(db, project_id, run_id).await?.servers.contains(id))
}

/// Whether the run may use built-in `name` now: in its set and on app-wide.
pub async fn run_uses_builtin(db: &Db, project_id: &str, run_id: &str, name: &str) -> Result<bool, String> {
    Ok(builtin_mcp_globally_enabled(db, name).await
        && run_mcp_selection(db, project_id, run_id).await?.builtins.contains(name))
}

/// One server in the session's MCP menu.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct RunMcpEntry {
    /// What [`set_run_mcp_override`] takes: `server:<id>` or `builtin:<name>`.
    pub key: String,
    pub name: String,
    pub builtin: bool,
    /// `stdio` | `http` | `sse`; `None` for a built-in.
    pub transport: Option<String>,
    pub description: String,
    /// Switched on app-wide (a built-in: its switch and feature). Off, the server is
    /// not injected whatever the session says.
    pub available: bool,
    pub enabled_for_project: bool,
    /// What the session is set to: the override, else the project.
    pub enabled: bool,
    /// `override` when the session decides, `project` when it follows the project.
    pub source: &'static str,
}

async fn run_project(db: &Db, run_id: &str) -> Result<String, String> {
    sqlx::query_scalar("SELECT project_id FROM runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("unknown run: {run_id}"))
}

/// Whether built-in `name` is on app-wide, the way the run launch reads it.
async fn builtin_available(db: &Db, tree: &[crate::features::ModuleState], name: &str) -> bool {
    match name {
        "puppeteer" => crate::features::effective(tree, "library.builtin_mcp_puppeteer"),
        "google" => {
            crate::features::effective(tree, "integrations.google")
                && builtin_mcp_globally_enabled(db, name).await
        }
        _ => builtin_mcp_globally_enabled(db, name).await,
    }
}

/// The session's MCP menu: the built-ins, then every user server by name.
pub async fn list_run_mcp(db: &Db, run_id: &str) -> Result<Vec<RunMcpEntry>, String> {
    use sqlx::Row;
    let project_id = run_project(db, run_id).await?;
    let in_project: HashSet<String> = project_servers(db, &project_id).await?.into_iter().collect();
    let builtins_off = project_builtins_off(db, &project_id).await?;
    let overridden: HashMap<String, bool> = overrides(db, run_id).await?.into_iter().collect();
    let tree = crate::features::commands::load(db).await.unwrap_or_default();

    let entry = |key: String, name: String, builtin, transport, description, available, project: bool| {
        let over = overridden.get(&key).copied();
        RunMcpEntry {
            enabled: over.unwrap_or(project),
            source: if over.is_some() { "override" } else { "project" },
            key,
            name,
            builtin,
            transport,
            description,
            available,
            enabled_for_project: project,
        }
    };

    let mut out = Vec::new();
    for name in BUILTIN_MCP_NAMES {
        out.push(entry(
            format!("{BUILTIN_KEY}{name}"),
            name.to_string(),
            true,
            None,
            String::new(),
            builtin_available(db, &tree, name).await,
            !builtins_off.iter().any(|off| off == name),
        ));
    }
    let rows = sqlx::query("SELECT id, name, description, transport, enabled FROM mcp_servers ORDER BY name")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    for row in rows {
        let id: String = row.get("id");
        let project = in_project.contains(&id);
        out.push(entry(
            format!("{SERVER_KEY}{id}"),
            row.get("name"),
            false,
            Some(row.get("transport")),
            row.try_get::<Option<String>, _>("description").ok().flatten().unwrap_or_default(),
            row.get::<i64, _>("enabled") != 0,
            project,
        ));
    }
    Ok(out)
}

/// Switch server `key` on or off for one session, or follow the project again
/// (`enabled` = `None`). Setting what the project already says clears the
/// override, so only a real difference is stored. Returns the updated menu.
pub async fn set_override(
    db: &Db,
    run_id: &str,
    key: &str,
    enabled: Option<bool>,
) -> Result<Vec<RunMcpEntry>, String> {
    let project_id = run_project(db, run_id).await?;
    let project_value = if let Some(id) = key.strip_prefix(SERVER_KEY) {
        let known: Option<i64> = sqlx::query_scalar("SELECT 1 FROM mcp_servers WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
        if known.is_none() {
            return Err(format!("unknown MCP server: {id}"));
        }
        project_servers(db, &project_id).await?.iter().any(|s| s == id)
    } else if let Some(name) = key.strip_prefix(BUILTIN_KEY).filter(|n| BUILTIN_MCP_NAMES.contains(n)) {
        !project_builtins_off(db, &project_id).await?.iter().any(|off| off == name)
    } else {
        return Err(format!("unknown MCP server key: {key}"));
    };

    match enabled {
        Some(on) if on != project_value => {
            sqlx::query(
                "INSERT INTO run_mcp_overrides (run_id, server_key, enabled, updated_at) VALUES (?, ?, ?, ?) \
                 ON CONFLICT (run_id, server_key) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
            )
            .bind(run_id)
            .bind(key)
            .bind(on)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        _ => {
            sqlx::query("DELETE FROM run_mcp_overrides WHERE run_id = ? AND server_key = ?")
                .bind(run_id)
                .bind(key)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    list_run_mcp(db, run_id).await
}

#[tauri::command]
pub async fn get_run_mcp(db: State<'_, Db>, run_id: String) -> Result<Vec<RunMcpEntry>, String> {
    list_run_mcp(db.inner(), &run_id).await
}

#[tauri::command]
pub async fn set_run_mcp_override(
    db: State<'_, Db>,
    run_id: String,
    key: String,
    enabled: Option<bool>,
) -> Result<Vec<RunMcpEntry>, String> {
    set_override(db.inner(), &run_id, &key, enabled).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c'), ('r2', 'p1', 'session', 'c')",
            "INSERT INTO mcp_servers (id, name, transport, url, enabled, created_at) VALUES
                ('m1', 'alpha', 'http', 'https://a', 1, 'c'),
                ('m2', 'beta', 'http', 'https://b', 1, 'c'),
                ('m3', 'gamma', 'http', 'https://g', 0, 'c')",
            "INSERT INTO project_mcp_servers (project_id, server_id, enabled_at) VALUES ('p1', 'm1', 'c'), ('p1', 'm3', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_effective_set_is_the_project_set_plus_minus_overrides() {
        let project = vec!["m1".to_string(), "m2".to_string()];
        let off = vec!["google".to_string()];
        // No override: the project set.
        let s = effective_selection(&project, &off, &[]);
        assert_eq!((s.servers, s.builtins), (set(&["m1", "m2"]), set(&["puppeteer"])));
        // Overrides win both ways; unknown prefixes are ignored.
        let s = effective_selection(
            &project,
            &off,
            &[
                ("server:m2".into(), false),
                ("server:m9".into(), true),
                ("builtin:google".into(), true),
                ("builtin:puppeteer".into(), false),
                ("m1".into(), false),
            ],
        );
        assert_eq!((s.servers, s.builtins), (set(&["m1", "m9"]), set(&["google"])));
    }

    #[tokio::test]
    async fn a_session_without_overrides_follows_the_project() {
        let db = test_db().await;
        let s = run_mcp_selection(&db, "p1", "r1").await.unwrap();
        assert_eq!(s.servers, set(&["m1", "m3"]));
        assert_eq!(s.builtins, set(&["puppeteer", "google"]));

        // The project changes: the session follows.
        crate::commands::mcp::set_project_mcp_server_enabled(&db, "p1", "m2", true).await.unwrap();
        crate::commands::mcp::set_project_builtin_mcp(&db, "p1", "google", false).await.unwrap();
        let s = run_mcp_selection(&db, "p1", "r1").await.unwrap();
        assert_eq!(s.servers, set(&["m1", "m2", "m3"]));
        assert_eq!(s.builtins, set(&["puppeteer"]));
    }

    #[tokio::test]
    async fn an_override_wins_over_the_project_and_reset_clears_it() {
        let db = test_db().await;
        set_override(&db, "r1", "server:m1", Some(false)).await.unwrap();
        set_override(&db, "r1", "server:m2", Some(true)).await.unwrap();
        let menu = set_override(&db, "r1", "builtin:puppeteer", Some(false)).await.unwrap();
        let s = run_mcp_selection(&db, "p1", "r1").await.unwrap();
        assert_eq!(s.servers, set(&["m2", "m3"]));
        assert_eq!(s.builtins, set(&["google"]));
        // Only r1: r2 keeps the project set.
        assert_eq!(run_mcp_selection(&db, "p1", "r2").await.unwrap().servers, set(&["m1", "m3"]));

        let alpha = menu.iter().find(|e| e.key == "server:m1").unwrap();
        assert_eq!((alpha.enabled, alpha.enabled_for_project, alpha.source), (false, true, "override"));
        let gamma = menu.iter().find(|e| e.key == "server:m3").unwrap();
        assert_eq!((gamma.enabled, gamma.available, gamma.source), (true, false, "project"));
        // Built-ins first, then servers by name.
        let keys: Vec<_> = menu.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["builtin:puppeteer", "builtin:google", "server:m1", "server:m2", "server:m3"]);

        // The project switching m2 on and off again does not undo the session's choice.
        crate::commands::mcp::set_project_mcp_server_enabled(&db, "p1", "m2", true).await.unwrap();
        crate::commands::mcp::set_project_mcp_server_enabled(&db, "p1", "m2", false).await.unwrap();
        assert!(run_mcp_selection(&db, "p1", "r1").await.unwrap().servers.contains("m2"));

        // Reset: back to the project.
        for key in ["server:m1", "server:m2", "builtin:puppeteer"] {
            set_override(&db, "r1", key, None).await.unwrap();
        }
        let s = run_mcp_selection(&db, "p1", "r1").await.unwrap();
        assert_eq!((s.servers, s.builtins), (set(&["m1", "m3"]), set(&["puppeteer", "google"])));
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_mcp_overrides").fetch_one(&db).await.unwrap();
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn only_a_difference_from_the_project_is_stored() {
        let db = test_db().await;
        // m1 is already on for the project.
        let menu = set_override(&db, "r1", "server:m1", Some(true)).await.unwrap();
        assert_eq!(menu.iter().find(|e| e.key == "server:m1").unwrap().source, "project");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_mcp_overrides").fetch_one(&db).await.unwrap();
        assert_eq!(n, 0);
        // Switching back to the project's value drops the override.
        set_override(&db, "r1", "server:m2", Some(true)).await.unwrap();
        set_override(&db, "r1", "server:m2", Some(false)).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_mcp_overrides").fetch_one(&db).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn bad_keys_and_unknown_runs_are_refused() {
        let db = test_db().await;
        for key in ["server:nope", "builtin:github", "m1", ""] {
            assert!(set_override(&db, "r1", key, Some(true)).await.is_err(), "{key}");
        }
        assert!(set_override(&db, "r9", "server:m1", Some(false)).await.is_err());
        assert!(list_run_mcp(&db, "r9").await.is_err());
    }

    #[tokio::test]
    async fn the_app_wide_switch_still_applies_on_top() {
        let db = test_db().await;
        // m3 is in the project set but switched off app-wide.
        assert!(!run_uses_server(&db, "p1", "r1", "m3").await.unwrap());
        assert!(run_uses_server(&db, "p1", "r1", "m1").await.unwrap());
        assert!(!run_uses_server(&db, "p1", "r1", "m2").await.unwrap());
        set_override(&db, "r1", "server:m2", Some(true)).await.unwrap();
        assert!(run_uses_server(&db, "p1", "r1", "m2").await.unwrap());

        crate::commands::settings::set_setting(&db, "mcp_builtin_google_enabled", "true").await.unwrap();
        crate::commands::mcp::set_project_builtin_mcp(&db, "p1", "google", false).await.unwrap();
        assert!(!run_uses_builtin(&db, "p1", "r1", "google").await.unwrap());
        set_override(&db, "r1", "builtin:google", Some(true)).await.unwrap();
        assert!(run_uses_builtin(&db, "p1", "r1", "google").await.unwrap());
        crate::commands::settings::set_setting(&db, "mcp_builtin_google_enabled", "false").await.unwrap();
        assert!(!run_uses_builtin(&db, "p1", "r1", "google").await.unwrap());
    }

    #[tokio::test]
    async fn overrides_go_with_their_run_and_their_server() {
        let db = test_db().await;
        set_override(&db, "r1", "server:m2", Some(true)).await.unwrap();
        set_override(&db, "r2", "server:m2", Some(true)).await.unwrap();
        set_override(&db, "r2", "builtin:google", Some(false)).await.unwrap();
        let count = |sql: &'static str| {
            let db = db.clone();
            async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&db).await.unwrap() }
        };
        // Purging a session drops its overrides.
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_mcp_overrides WHERE run_id = 'r1'").await, 0);
        assert_eq!(count("SELECT COUNT(*) FROM run_mcp_overrides WHERE run_id = 'r2'").await, 2);
        // Deleting a server drops the overrides on it.
        sqlx::query("DELETE FROM mcp_servers WHERE id = 'm2'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_mcp_overrides").await, 1);
        // Purging the project drops the rest.
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_mcp_overrides").await, 0);
    }
}
