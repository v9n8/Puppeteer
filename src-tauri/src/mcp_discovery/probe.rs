//! The engine probe: which MCP servers the Claude CLI loads for each Claude profile, as a normal
//! unlocked run would. Gives what no file shows: S1 claude.ai connectors, S5 plugin servers, S6
//! managed / enterprise servers.
//!
//! `sidecar/mcp-probe.mjs` is spawned once per profile — every app account and global — with the
//! run's env builder ([`apply_claude_config_dir`]): no model call, it prints one JSON line per
//! server (name / source / status / transport, and a connector's `{id, url}`) and a `done` line.
//! The connectors of a profile go into `claude_connectors` (a run passes them back to the engine);
//! a failed probe empties that profile's rows, so none of its connectors can enter a run (FR-5).
//!
//! Also here: whether two profiles are the same claude.ai login (conflicts C1 / C2) or a profile's
//! login cannot be read (C4). Only `oauthAccount.accountUuid` and `organizationUuid` are read,
//! hashed at once and compared; a value is never returned, logged or shown.

use super::claude_sources::GLOBAL_PROFILE;
use super::{DiscoveredServer, DiscoveryInputs, DiscoveryProblem, DiscoveryScope, Scan, SourceId, Found};
use crate::db::Db;
use crate::import::{read_config, Scope};
use crate::runs::sidecar::{apply_claude_config_dir, augment_command_path, detach_sidecar_process_group};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What the engine names a connector's source.
pub const CLAUDEAI_SOURCE: &str = "claudeai";
/// Origin shown for a connector row.
const CLAUDEAI_ORIGIN: &str = "claude.ai";
/// The probe settles 12 s and gives up after 45 s; this is the app's own ceiling on top.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const MESSAGE_CAP: usize = 200;

/// A claude.ai connector as the engine reports it; a run passes it back as
/// `{type: 'claudeai-proxy', url, id}`. Backend only, and kept out of `Debug`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ConnectorRef {
    pub id: String,
    pub url: String,
}

impl std::fmt::Debug for ConnectorRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConnectorRef { .. }")
    }
}

/// One server line of the probe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProbedServer {
    pub name: String,
    pub source: String,
    pub status: String,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub connector: Option<ConnectorRef>,
}

/// The servers of a probe's stdout. `Err` when the probe reported an error or never finished.
/// The error text is the probe's own one-line message, capped.
pub fn parse_output(stdout: &str) -> Result<Vec<ProbedServer>, String> {
    let mut servers = Vec::new();
    for line in stdout.lines().map(str::trim).filter(|l| l.starts_with('{')) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        match value.get("type").and_then(|t| t.as_str()) {
            Some("server") => {
                servers.push(serde_json::from_value(value).map_err(|_| "the probe printed an unreadable server line".to_string())?)
            }
            Some("done") => return Ok(servers),
            Some("error") => {
                let message = value.get("error").and_then(|e| e.as_str()).unwrap_or("probe failed");
                return Err(message.chars().take(MESSAGE_CAP).collect());
            }
            _ => {}
        }
    }
    Err("the probe ended without an answer".to_string())
}

/// How to start the probe: the run's node, the script beside the sidecar, the run's CLI.
#[derive(Debug, Clone)]
pub struct ProbeSpawn {
    pub node: String,
    pub script: PathBuf,
    /// A custom CLI (`claude_path` setting); `None`: the SDK's bundled one, like a run.
    pub claude_path: Option<String>,
    /// Working dir of the probe: project servers of this folder load too.
    pub cwd: PathBuf,
    pub timeout: Duration,
}

/// Probe one profile: `config_dir` of an app account, `None` for global (the env Puppeteer was
/// started with, like a run without an account).
pub async fn run_probe(spawn: &ProbeSpawn, config_dir: Option<&str>) -> Result<Vec<ProbedServer>, String> {
    let mut cmd = tokio::process::Command::new(&spawn.node);
    cmd.arg(&spawn.script).arg(&spawn.cwd).current_dir(&spawn.cwd);
    augment_command_path(&mut cmd);
    if let Some(path) = &spawn.claude_path {
        cmd.env("PUPPETEER_CLAUDE_PATH", path);
    }
    apply_claude_config_dir(&mut cmd, config_dir);
    // The CLI's stderr can echo config; the probe's answer is on stdout.
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    detach_sidecar_process_group(&mut cmd);
    cmd.kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| format!("could not start the MCP probe: {e}"))?;
    let _proc = crate::procs::track(crate::procs::ProcSpec::new(crate::procs::ProcKind::Probe, "MCP probe"), child.id());
    let output = tokio::time::timeout(spawn.timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("the MCP probe gave no answer within {} ms", spawn.timeout.as_millis()))?
        .map_err(|e| format!("the MCP probe failed: {e}"))?;
    parse_output(&String::from_utf8_lossy(&output.stdout))
}

/// Keep `profile`'s connectors as the probe saw them; after a failed probe, none.
pub async fn store_connectors(db: &Db, profile: &str, result: &Result<Vec<ProbedServer>, String>, seen_at: &str) -> Result<(), String> {
    let err = |e: sqlx::Error| e.to_string();
    let mut tx = db.begin().await.map_err(err)?;
    sqlx::query("DELETE FROM claude_connectors WHERE account_ref = ?").bind(profile).execute(&mut *tx).await.map_err(err)?;
    for server in result.iter().flatten().filter(|s| s.source == CLAUDEAI_SOURCE) {
        let Some(connector) = &server.connector else { continue };
        sqlx::query(
            "INSERT OR REPLACE INTO claude_connectors (account_ref, name, connector_id, url, status, seen_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(profile)
        .bind(&server.name)
        .bind(&connector.id)
        .bind(&connector.url)
        .bind(&server.status)
        .bind(seen_at)
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    }
    tx.commit().await.map_err(err)
}

/// The key a run gives a connector, as the CLI names its tools (`claude.ai Slack` →
/// `claude_ai_Slack`, tools `mcp__claude_ai_Slack__*`).
pub fn connector_key(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn engine_row(source: SourceId, id: String, source_ref: String, profile: Option<&str>, server: &ProbedServer) -> DiscoveredServer {
    let connector = source == SourceId::S1;
    DiscoveredServer {
        id,
        source,
        scope: DiscoveryScope::Global,
        origin: CLAUDEAI_ORIGIN.to_string(),
        origin_label: if connector { CLAUDEAI_ORIGIN.to_string() } else { format!("engine ({})", server.source) },
        source_ref,
        local_key: None,
        profile: profile.map(str::to_string),
        project_id: None,
        app: None,
        name: server.name.clone(),
        import_name: if connector { connector_key(&server.name) } else { crate::import::library_name(&server.name) },
        transport: if connector { "connector".to_string() } else { server.transport.clone().unwrap_or_else(|| "unknown".to_string()) },
        summary: None,
        fingerprint: None,
        importable: connector,
        engine_status: Some(server.status.clone()),
        problem: None,
    }
}

/// The rows a profile's probe adds: its connectors (S1, importable, under the profile), plugin
/// servers (S5) and managed / enterprise servers (S6), both listed and blocked. Servers from
/// files (user / project / local scope) are left to the file sources. A failed probe is one S1
/// problem of the profile.
pub fn probe_scan(profile: &str, result: &Result<Vec<ProbedServer>, String>) -> Scan {
    let mut scan = Scan::default();
    let servers = match result {
        Ok(servers) => servers,
        Err(message) => {
            scan.problems.push(DiscoveryProblem {
                source: SourceId::S1,
                origin: CLAUDEAI_ORIGIN.to_string(),
                origin_label: CLAUDEAI_ORIGIN.to_string(),
                profile: Some(profile.to_string()),
                project_id: None,
                app: None,
                message: message.clone(),
            });
            return scan;
        }
    };
    for server in servers {
        let row = match server.source.as_str() {
            CLAUDEAI_SOURCE => {
                engine_row(SourceId::S1, format!("S1|{profile}|{}", server.name), profile.to_string(), Some(profile), server)
            }
            "plugin" => engine_row(SourceId::S5, format!("S5|{profile}|{}", server.name), profile.to_string(), Some(profile), server),
            // One managed server reaches every profile: one row.
            "managed" | "enterprise" => {
                engine_row(SourceId::S6, format!("S6|engine|{}", server.name), "engine".to_string(), None, server)
            }
            _ => continue,
        };
        scan.found.push(Found { row, export: None });
    }
    scan
}

/// The probe rows of several profiles, a row reported by two (a managed server) kept once.
pub fn merge_probes<'a>(results: impl IntoIterator<Item = (&'a str, &'a Result<Vec<ProbedServer>, String>)>) -> Scan {
    let mut scan = Scan::default();
    let mut ids = HashSet::new();
    for (profile, result) in results {
        let mut one = probe_scan(profile, result);
        one.found.retain(|f| ids.insert(f.row.id.clone()));
        scan.extend(one);
    }
    scan
}

// ---- Profile identity (C1 / C2 / C4) ---------------------------------------------------------

#[derive(Deserialize)]
struct IdentityFile {
    #[serde(rename = "oauthAccount", default)]
    oauth_account: Option<OAuthAccount>,
}

#[derive(Deserialize)]
struct OAuthAccount {
    #[serde(rename = "accountUuid", default)]
    account_uuid: Option<String>,
    #[serde(rename = "organizationUuid", default)]
    organization_uuid: Option<String>,
}

/// A hash of a profile's login. No `Debug`, no `Serialize`: it is only ever compared.
#[derive(PartialEq, Eq)]
struct Login(String);

/// The login of the Claude config file `claude_json` (read as a file of `root`); `None` when the
/// file, its `oauthAccount` or its `accountUuid` is missing or unreadable.
fn login_of(claude_json: &Path, root: &Path, inputs: &DiscoveryInputs) -> Option<Login> {
    let raw = read_config(claude_json, root, Scope::Global, &inputs.ctx).ok()?;
    let account = serde_json::from_str::<IdentityFile>(&raw).ok()?.oauth_account?;
    let uuid = account.account_uuid.filter(|u| !u.is_empty())?;
    let org = account.organization_uuid.unwrap_or_default();
    Some(Login(crate::commands::rules::content_hash(&format!("{uuid}\n{org}"))))
}

/// Whether a profile's login could be read, and which other profiles share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileIdentity {
    pub profile: String,
    /// `false`: conflict C4, the login is not known.
    pub known: bool,
    /// Profiles with the same login (C1 when one side is `global`, C2 between app accounts).
    pub same_login_as: Vec<String>,
}

/// `(profile, .claude.json, dir it belongs to)` of global and every app account.
fn identity_files(inputs: &DiscoveryInputs) -> Vec<(String, PathBuf, PathBuf)> {
    let global_dir = inputs.claude_config_dir.clone().unwrap_or_else(|| inputs.home.clone());
    let mut files = vec![(GLOBAL_PROFILE.to_string(), global_dir.join(".claude.json"), global_dir)];
    files.extend(
        inputs
            .claude_accounts
            .iter()
            .map(|a| (a.profile.clone(), a.config_dir.join(".claude.json"), a.config_dir.clone())),
    );
    files
}

/// Each Claude profile's identity, compared with every other. Values never leave this function.
pub fn compare_identities(inputs: &DiscoveryInputs) -> Vec<ProfileIdentity> {
    let logins: Vec<(String, Option<Login>)> =
        identity_files(inputs).into_iter().map(|(profile, file, root)| (profile, login_of(&file, &root, inputs))).collect();
    logins
        .iter()
        .map(|(profile, login)| ProfileIdentity {
            profile: profile.clone(),
            known: login.is_some(),
            same_login_as: logins
                .iter()
                .filter(|(other, theirs)| other != profile && login.is_some() && theirs == login)
                .map(|(other, _)| other.clone())
                .collect(),
        })
        .collect()
}

// ---- The real machine -------------------------------------------------------------------------

/// The probe as a run would start it: the node / sidecar / CLI settings, the script beside the
/// sidecar, and a fixed folder of the app data as cwd (no project servers; the CLI keeps one
/// local-scope entry for it instead of one per probe).
pub async fn spawn_settings(app: &tauri::AppHandle, db: &Db) -> Result<ProbeSpawn, String> {
    use tauri::Manager;
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings WHERE key IN ('node_path', 'sidecar_path', 'claude_path')")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    let setting = |key: &str| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.trim().to_string()).filter(|v| !v.is_empty());
    let (node, sidecar) = crate::runs::sidecar::resolve_sidecar(
        app,
        &setting("node_path").unwrap_or_else(|| "node".to_string()),
        &setting("sidecar_path").unwrap_or_default(),
    )?;
    let cwd = app.path().app_data_dir().map_err(|e| e.to_string())?.join("mcp-probe");
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    Ok(ProbeSpawn {
        node,
        script: sidecar.with_file_name("mcp-probe.mjs"),
        claude_path: setting("claude_path").filter(|p| p != "claude"),
        cwd,
        timeout: PROBE_TIMEOUT,
    })
}

/// Probe global and every app account at once, keep each profile's connectors, and return the
/// rows the probes add. A profile whose probe fails gets a problem and no connector.
pub async fn probe_profiles(db: &Db, spawn: &ProbeSpawn, inputs: &DiscoveryInputs) -> Scan {
    let mut profiles: Vec<(String, Option<String>)> = vec![(GLOBAL_PROFILE.to_string(), None)];
    profiles.extend(inputs.claude_accounts.iter().map(|a| (a.profile.clone(), Some(a.config_dir.display().to_string()))));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, (_, config_dir)) in profiles.iter().enumerate() {
        let (spawn, config_dir) = (spawn.clone(), config_dir.clone());
        tasks.spawn(async move { (index, run_probe(&spawn, config_dir.as_deref()).await) });
    }
    let mut results: Vec<Result<Vec<ProbedServer>, String>> = vec![Err("the probe did not run".to_string()); profiles.len()];
    while let Some(joined) = tasks.join_next().await {
        if let Ok((index, result)) = joined {
            results[index] = result;
        }
    }
    let seen_at = chrono::Utc::now().to_rfc3339();
    for ((profile, _), result) in profiles.iter().zip(&results) {
        if let Err(e) = store_connectors(db, profile, result, &seen_at).await {
            tracing::warn!(event = "mcp_probe_store_failed", profile = %profile, error = %e);
        }
        let outcome = match result {
            Ok(servers) => format!("{} servers", servers.len()),
            Err(_) => "failed".to_string(),
        };
        tracing::info!(event = "mcp_probe", profile = %profile, outcome = %outcome);
    }
    merge_probes(profiles.iter().map(|(p, _)| p.as_str()).zip(&results))
}

#[cfg(test)]
mod tests {
    use super::super::claude_sources::tests::{inputs, tmp, write};
    use super::super::ProfileDir;
    use super::*;

    fn server(name: &str, source: &str, connector: Option<(&str, &str)>) -> ProbedServer {
        ProbedServer {
            name: name.into(),
            source: source.into(),
            status: "connected".into(),
            transport: Some(if connector.is_some() { "claudeai-proxy" } else { "stdio" }.into()),
            connector: connector.map(|(id, url)| ConnectorRef { id: id.into(), url: url.into() }),
        }
    }

    const OUTPUT: &str = concat!(
        "noise from a shell profile\n",
        r#"{"type":"server","name":"claude.ai Slack","source":"claudeai","status":"connected","transport":"claudeai-proxy","connector":{"id":"conn_1","url":"https://c/1"}}"#,
        "\n",
        r#"{"type":"server","name":"fs","source":"user","status":"connected","transport":"stdio"}"#,
        "\n",
        r#"{"type":"done","count":2}"#,
        "\n"
    );

    #[test]
    fn the_probe_output_is_read_line_by_line() {
        let servers = parse_output(OUTPUT).unwrap();
        assert_eq!(servers, [server("claude.ai Slack", "claudeai", Some(("conn_1", "https://c/1"))), server("fs", "user", None)]);
        assert_eq!(parse_output(r#"{"type":"error","error":"no login"}"#).unwrap_err(), "no login");
        assert_eq!(parse_output("{\"type\":\"server\",\"name\":\"x\",\"source\":\"user\",\"status\":\"s\"}\n").unwrap_err(), "the probe ended without an answer");
        assert_eq!(parse_output("").unwrap_err(), "the probe ended without an answer");
        assert!(parse_output("{\"type\":\"server\",\"name\":7}\n{\"type\":\"done\"}").is_err());
        let long = format!(r#"{{"type":"error","error":"{}"}}"#, "e".repeat(500));
        assert_eq!(parse_output(&long).unwrap_err().len(), MESSAGE_CAP);
    }

    #[test]
    fn a_connector_never_shows_its_id_or_url_in_debug() {
        let shown = format!("{:?}", parse_output(OUTPUT).unwrap());
        assert!(!shown.contains("conn_1") && !shown.contains("https://c/1"), "{shown}");
    }

    #[test]
    fn connector_keys_are_what_the_cli_names_tools_by() {
        assert_eq!(connector_key("claude.ai Slack"), "claude_ai_Slack");
        assert_eq!(connector_key("claude.ai Google Drive"), "claude_ai_Google_Drive");
        assert_eq!(connector_key("a-b_c"), "a-b_c");
    }

    #[test]
    fn probe_rows_are_connectors_plugins_and_managed_servers() {
        let ok: Result<Vec<ProbedServer>, String> = Ok(vec![
            server("claude.ai Slack", "claudeai", Some(("conn_1", "https://c/1"))),
            server("fs", "user", None),
            server("proj", "project", None),
            server("lint", "plugin", None),
            server("corp", "managed", None),
        ]);
        let again = ok.clone();
        let failed: Result<Vec<ProbedServer>, String> = Err("no login".into());
        let report = merge_probes([("global", &ok), ("acc-1", &again), ("acc-2", &failed)]).into_report();
        let rows: Vec<_> = report
            .servers
            .iter()
            .map(|r| (r.id.as_str(), r.source, r.profile.as_deref(), r.transport.as_str(), r.importable, r.import_name.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("S1|global|claude.ai Slack", SourceId::S1, Some("global"), "connector", true, "claude_ai_Slack"),
                ("S5|global|lint", SourceId::S5, Some("global"), "stdio", false, "lint"),
                ("S6|engine|corp", SourceId::S6, None, "stdio", false, "corp"),
                ("S1|acc-1|claude.ai Slack", SourceId::S1, Some("acc-1"), "connector", true, "claude_ai_Slack"),
                ("S5|acc-1|lint", SourceId::S5, Some("acc-1"), "stdio", false, "lint"),
            ]
        );
        assert_eq!(report.servers[0].source_ref, "global");
        assert_eq!(report.servers[0].engine_status.as_deref(), Some("connected"));
        assert_eq!(report.problems.len(), 1);
        let problem = &report.problems[0];
        assert_eq!((problem.source, problem.profile.as_deref(), problem.message.as_str()), (SourceId::S1, Some("acc-2"), "no login"));
        let shown = format!("{}{report:?}", serde_json::to_string(&report).unwrap());
        assert!(!shown.contains("conn_1") && !shown.contains("https://c/1"), "{shown}");
    }

    async fn test_db() -> Db {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn connectors(db: &Db) -> Vec<(String, String, String, String, String)> {
        sqlx::query_as("SELECT account_ref, name, connector_id, url, status FROM claude_connectors ORDER BY account_ref, name")
            .fetch_all(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_probe_replaces_its_profiles_connectors_and_a_failure_empties_them() {
        let db = test_db().await;
        let first: Result<Vec<ProbedServer>, String> = Ok(vec![
            server("claude.ai Slack", "claudeai", Some(("s1", "https://s"))),
            server("claude.ai Drive", "claudeai", Some(("d1", "https://d"))),
            server("fs", "user", None),
            // A connector line without its config is not usable by a run.
            server("claude.ai Odd", "claudeai", None),
        ]);
        store_connectors(&db, "global", &first, "t1").await.unwrap();
        store_connectors(&db, "acc-1", &first, "t1").await.unwrap();
        let second: Result<Vec<ProbedServer>, String> = Ok(vec![server("claude.ai Slack", "claudeai", Some(("s2", "https://s2")))]);
        store_connectors(&db, "global", &second, "t2").await.unwrap();
        assert_eq!(
            connectors(&db).await,
            [
                ("acc-1".into(), "claude.ai Drive".into(), "d1".into(), "https://d".into(), "connected".into()),
                ("acc-1".into(), "claude.ai Slack".into(), "s1".into(), "https://s".into(), "connected".into()),
                ("global".into(), "claude.ai Slack".into(), "s2".into(), "https://s2".into(), "connected".into()),
            ]
        );
        store_connectors(&db, "acc-1", &Err("no login".into()), "t3").await.unwrap();
        assert_eq!(connectors(&db).await.len(), 1, "a failed probe leaves the profile no connector");
    }

    fn claude_json(dir: &Path, uuid: Option<&str>, org: &str) {
        let body = match uuid {
            Some(u) => format!(r#"{{"oauthAccount": {{"accountUuid": "{u}", "organizationUuid": "{org}", "emailAddress": "x@y"}}, "mcpServers": {{}}}}"#),
            None => r#"{"mcpServers": {}}"#.to_string(),
        };
        write(&dir.join(".claude.json"), &body);
    }

    #[test]
    fn identities_are_compared_without_showing_them() {
        let base = tmp("identity");
        let mut inputs = inputs(&base);
        claude_json(&inputs.home, Some("UUID-SAME"), "ORG-1");
        let accounts = base.join("appdata/claude-accounts");
        for (id, uuid, org) in [("a1", Some("UUID-SAME"), "ORG-1"), ("a2", Some("UUID-OTHER"), "ORG-1"), ("a3", Some("UUID-OTHER"), "ORG-1"), ("a4", None, "")] {
            claude_json(&accounts.join(id), uuid, org);
            inputs.claude_accounts.push(ProfileDir { profile: id.into(), config_dir: accounts.join(id) });
        }
        // Same account in another organization is another login.
        claude_json(&accounts.join("a5"), Some("UUID-SAME"), "ORG-2");
        inputs.claude_accounts.push(ProfileDir { profile: "a5".into(), config_dir: accounts.join("a5") });
        // No file at all.
        inputs.claude_accounts.push(ProfileDir { profile: "a6".into(), config_dir: accounts.join("a6") });

        let got = compare_identities(&inputs);
        let view: Vec<_> = got.iter().map(|i| (i.profile.as_str(), i.known, i.same_login_as.clone())).collect();
        assert_eq!(
            view,
            [
                ("global", true, vec!["a1".to_string()]),
                ("a1", true, vec!["global".to_string()]),
                ("a2", true, vec!["a3".to_string()]),
                ("a3", true, vec!["a2".to_string()]),
                ("a4", false, vec![]),
                ("a5", true, vec![]),
                ("a6", false, vec![]),
            ]
        );
        assert!(!format!("{got:?}").contains("UUID") && !format!("{got:?}").contains("ORG"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn with_claude_config_dir_global_identity_is_read_there() {
        let base = tmp("identity-dir");
        let mut inputs = inputs(&base);
        claude_json(&inputs.home, Some("UUID-HOME"), "O");
        let dir = base.join("custom");
        claude_json(&dir, Some("UUID-CUSTOM"), "O");
        inputs.claude_config_dir = Some(dir);
        let account = base.join("appdata/claude-accounts/a1");
        claude_json(&account, Some("UUID-CUSTOM"), "O");
        inputs.claude_accounts.push(ProfileDir { profile: "a1".into(), config_dir: account });
        assert_eq!(compare_identities(&inputs)[0].same_login_as, ["a1"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The spawn wiring, with a shell script standing in for node + mcp-probe.mjs: the profile's
    /// config dir reaches the probe and its stdout is parsed.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_probe_runs_with_the_profiles_config_dir() {
        let base = tmp("spawn");
        let script = base.join("probe.sh");
        write(
            &script,
            "printf '{\"type\":\"server\",\"name\":\"%s\",\"source\":\"user\",\"status\":\"%s\"}\\n' \"${CLAUDE_CONFIG_DIR:-none}\" \"$1\"\n\
             printf '{\"type\":\"done\",\"count\":1}\\n'\n",
        );
        let spawn = ProbeSpawn { node: "/bin/sh".into(), script, claude_path: None, cwd: base.clone(), timeout: Duration::from_secs(10) };
        // A config dir that does not exist: nothing is linked into it.
        let dir = base.join("no-such-account").display().to_string();
        let servers = run_probe(&spawn, Some(&dir)).await.unwrap();
        assert_eq!((servers[0].name.as_str(), servers[0].status.as_str()), (dir.as_str(), base.display().to_string().as_str()));

        let slow = base.join("slow.sh");
        write(&slow, "sleep 5\n");
        let spawn = ProbeSpawn { script: slow, timeout: Duration::from_millis(200), ..spawn };
        assert_eq!(run_probe(&spawn, None).await.unwrap_err(), "the MCP probe gave no answer within 200 ms");
        let _ = std::fs::remove_dir_all(&base);
    }
}
