//! MCP discovery: every MCP server configured on this machine that a Claude or Codex run could
//! load, whether Puppeteer manages it or not, so the MCP views can list it with its source and
//! offer to import it.
//!
//! Sources (ids as in the requirements): S2 user scope and S3 local scope of each Claude config
//! file (`~/.claude.json`, the global config dir's, every app account dir's), S4 a project's
//! `.mcp.json`, S6 Claude Code's managed MCP file ([`claude_sources`]); S7 the `config.toml` of
//! each Codex home (global and every app account), S8 a project's `.codex/config.toml`
//! ([`codex_sources`]); S9 the MCP config of other apps, through the import adapters
//! ([`other_sources`]). From the engine itself ([`probe`]): S1 the claude.ai connectors of each
//! Claude profile, S5 plugin servers, S6 managed / enterprise servers.
//!
//! Only reads, like an import (`import::claude_code`'s parsers are reused): a source file is never
//! written. A row carries key names only; env / header VALUEs stay in the scanned export, which is
//! never kept in a [`DiscoveryReport`]. Every path comes from [`DiscoveryInputs`], so tests read
//! temp dirs only. A file that cannot be read is one [`DiscoveryProblem`]; the other sources still
//! show.

pub mod claude_sources;
pub mod codex_sources;
pub mod commands;
pub mod connectors;
pub mod import_sync;
pub mod other_sources;
pub mod probe;
pub mod state;
pub mod value_mac;

use crate::commands::mcp::McpServerExport;
use crate::import::{Candidate, McpSummary, Payload, ScanContext, ScanReport};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where a server is configured (requirements section 4). Stored as `mcp_servers.source_kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum SourceId {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
}

impl SourceId {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceId::S1 => "S1",
            SourceId::S2 => "S2",
            SourceId::S3 => "S3",
            SourceId::S4 => "S4",
            SourceId::S5 => "S5",
            SourceId::S6 => "S6",
            SourceId::S7 => "S7",
            SourceId::S8 => "S8",
            SourceId::S9 => "S9",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryScope {
    /// Loaded by every run of the profile it belongs to.
    Global,
    /// Loaded by runs in one project folder.
    Project,
}

/// One server found in a source. Key names only: never an env / header VALUE.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServer {
    /// Stable across scans: `<source>|<source_ref>|<name>`.
    pub id: String,
    pub source: SourceId,
    pub scope: DiscoveryScope,
    /// The file it was read from (links resolved when the file could be resolved).
    pub origin: String,
    /// `origin` with HOME shown as `~`.
    pub origin_label: String,
    /// What an import stores in `mcp_servers.source_ref`: `origin`, and for local scope (S3)
    /// `origin#<projects key>`, since one file holds every project's local servers.
    pub source_ref: String,
    /// S3 only: the `projects.<key>` path of the local scope.
    pub local_key: Option<String>,
    /// The profile whose runs read this file. S2 / S3: a Claude profile (`claude_accounts.id` or
    /// `global`); S7: a Codex one (`codex_accounts.id` or `global`). `None` for a file no
    /// Puppeteer run reads (HOME's while `CLAUDE_CONFIG_DIR` / `CODEX_HOME` points elsewhere) or a
    /// source of every profile (`.mcp.json`, managed, project Codex config, other apps). A file
    /// shared by several profiles (a linked config) is listed once, under the first.
    pub profile: Option<String>,
    /// The Puppeteer project of a project-scope source; `None` for a local scope key that is no
    /// project's folder.
    pub project_id: Option<String>,
    /// S9: the import adapter of the app (`cursor`, `vscode`, …).
    pub app: Option<String>,
    /// The server's key in the source (`mcp_servers.source_name`), as the engine names its tools.
    pub name: String,
    /// The name an import proposes, valid for the MCP list.
    pub import_name: String,
    /// `stdio` | `http` | `sse`; `unknown` when the entry could not be read.
    pub transport: String,
    pub summary: Option<McpSummary>,
    /// sha256 of the entry's launch fields and key names (no VALUE): an import stores it, a later
    /// scan with another one means "changed since import". Backend only.
    #[serde(skip)]
    pub fingerprint: Option<String>,
    /// `false` for sources that are listed and blocked only (S5 plugin, S6 managed).
    pub importable: bool,
    /// What the engine probe reported (`connected`, `needs-auth`, `failed`, …); file rows: `None`.
    pub engine_status: Option<String>,
    /// Why the entry could not be read (the rest of its file still listed). Never quotes a value.
    pub problem: Option<String>,
}

/// A source file that could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryProblem {
    pub source: SourceId,
    pub origin: String,
    pub origin_label: String,
    pub profile: Option<String>,
    pub project_id: Option<String>,
    pub app: Option<String>,
    /// From the OS or the JSON parser (line / column only): never the file's content.
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DiscoveryReport {
    pub servers: Vec<DiscoveredServer>,
    pub problems: Vec<DiscoveryProblem>,
}

/// A row with what an import of it copies. Not `Debug`, not kept: the export holds secret VALUEs,
/// so it lives only as long as an import or sync needs it.
pub struct Found {
    pub row: DiscoveredServer,
    pub export: Option<McpServerExport>,
}

/// A scan: rows with their exports, and the files that could not be read.
#[derive(Default)]
pub struct Scan {
    pub found: Vec<Found>,
    pub problems: Vec<DiscoveryProblem>,
}

impl Scan {
    /// The rows only, for the cache and the webview.
    pub fn into_report(self) -> DiscoveryReport {
        DiscoveryReport { servers: self.found.into_iter().map(|f| f.row).collect(), problems: self.problems }
    }

    pub fn extend(&mut self, other: Scan) {
        self.found.extend(other.found);
        self.problems.extend(other.problems);
    }
}

/// An app account's config dir: a Claude account's `CLAUDE_CONFIG_DIR`, a Codex account's
/// `CODEX_HOME`.
#[derive(Debug, Clone)]
pub struct ProfileDir {
    /// `claude_accounts.id` / `codex_accounts.id`.
    pub profile: String,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProjectDir {
    pub id: String,
    pub path: PathBuf,
}

/// Every path discovery reads. Production fills it from the app ([`DiscoveryInputs::load`]);
/// tests from temp dirs.
pub struct DiscoveryInputs {
    pub home: PathBuf,
    /// `CLAUDE_CONFIG_DIR` as Puppeteer was started: the global profile reads its `.claude.json`
    /// instead of HOME's.
    pub claude_config_dir: Option<PathBuf>,
    /// App Claude accounts (never `global`).
    pub claude_accounts: Vec<ProfileDir>,
    /// `CODEX_HOME` as Puppeteer was started (else Codex reads `~/.codex`).
    pub codex_home: Option<PathBuf>,
    /// App Codex accounts, each its own `CODEX_HOME`.
    pub codex_accounts: Vec<ProfileDir>,
    pub projects: Vec<ProjectDir>,
    /// Folder of Claude Code's `managed-mcp.json`.
    pub claude_managed_dir: PathBuf,
    /// Puppeteer's app data: a link into it is not followed (same rule as an import).
    pub ctx: ScanContext,
    /// This install's key for the MAC of a row's values ([`value_mac`]); `None`: launch fields only.
    pub value_key: Option<value_mac::ValueKey>,
}

/// Folder of Claude Code's system-wide managed MCP file (Claude Code settings docs).
pub fn claude_managed_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/ClaudeCode")
    } else if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\ClaudeCode")
    } else {
        PathBuf::from("/etc/claude-code")
    }
}

impl DiscoveryInputs {
    /// The real machine: HOME, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, every app Claude and Codex
    /// account and every project not in the Trash.
    pub async fn load(app: &tauri::AppHandle, db: &crate::db::Db) -> Result<Self, String> {
        use tauri::Manager;
        let home = app.path().home_dir().map_err(|e| e.to_string())?;
        let claude_accounts: Vec<(String, String)> =
            sqlx::query_as("SELECT id, config_dir FROM claude_accounts ORDER BY created_at")
                .fetch_all(db)
                .await
                .map_err(|e| e.to_string())?;
        let codex_accounts: Vec<(String, String)> =
            sqlx::query_as("SELECT id, home_dir FROM codex_accounts ORDER BY created_at")
                .fetch_all(db)
                .await
                .map_err(|e| e.to_string())?;
        let projects: Vec<(String, String)> =
            sqlx::query_as("SELECT id, path FROM projects WHERE deleted_at IS NULL ORDER BY name")
                .fetch_all(db)
                .await
                .map_err(|e| e.to_string())?;
        let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        Ok(DiscoveryInputs {
            home,
            claude_config_dir: std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()).map(PathBuf::from),
            claude_accounts: claude_accounts
                .into_iter()
                .map(|(profile, dir)| ProfileDir { profile, config_dir: PathBuf::from(dir) })
                .collect(),
            codex_home: std::env::var_os("CODEX_HOME").filter(|d| !d.is_empty()).map(PathBuf::from),
            codex_accounts: codex_accounts
                .into_iter()
                .map(|(profile, dir)| ProfileDir { profile, config_dir: PathBuf::from(dir) })
                .collect(),
            projects: projects.into_iter().map(|(id, path)| ProjectDir { id, path: PathBuf::from(path) }).collect(),
            claude_managed_dir: claude_managed_dir(),
            value_key: value_mac::ValueKey::load_or_create(&app_data)
                .map_err(|e| tracing::warn!(event = "mcp_source_key_unavailable", error = %e))
                .ok(),
            ctx: ScanContext { library_root: app_data },
        })
    }

    /// `path` with HOME shown as `~`.
    pub fn label(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }
}

/// Every file source (no engine probe).
pub fn scan_files(inputs: &DiscoveryInputs) -> Scan {
    let mut scan = claude_sources::scan(inputs);
    scan.extend(codex_sources::scan(inputs));
    scan.extend(other_sources::scan(inputs));
    scan
}

/// Where a row comes from, before its servers are known.
pub(crate) struct Origin<'a> {
    pub source: SourceId,
    pub scope: DiscoveryScope,
    pub path: &'a Path,
    pub local_key: Option<&'a str>,
    pub profile: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub app: Option<&'a str>,
    pub importable: bool,
}

impl Origin<'_> {
    fn origin(&self) -> String {
        std::fs::canonicalize(self.path).unwrap_or_else(|_| self.path.to_path_buf()).display().to_string()
    }

    fn source_ref(&self) -> String {
        match self.local_key {
            Some(key) => format!("{}#{key}", self.origin()),
            None => self.origin(),
        }
    }

    /// A file that could not be read.
    pub fn problem(&self, inputs: &DiscoveryInputs, message: String) -> DiscoveryProblem {
        DiscoveryProblem {
            source: self.source,
            origin: self.origin(),
            origin_label: inputs.label(self.path),
            profile: self.profile.map(str::to_string),
            project_id: self.project_id.map(str::to_string),
            app: self.app.map(str::to_string),
            message,
        }
    }

    fn row(&self, inputs: &DiscoveryInputs, name: &str) -> DiscoveredServer {
        let source_ref = self.source_ref();
        DiscoveredServer {
            id: format!("{}|{source_ref}|{name}", self.source.as_str()),
            source: self.source,
            scope: self.scope,
            origin: self.origin(),
            origin_label: inputs.label(self.path),
            source_ref,
            local_key: self.local_key.map(str::to_string),
            profile: self.profile.map(str::to_string),
            project_id: self.project_id.map(str::to_string),
            app: self.app.map(str::to_string),
            name: name.to_string(),
            import_name: crate::import::library_name(name),
            transport: "unknown".to_string(),
            summary: None,
            fingerprint: None,
            importable: self.importable,
            engine_status: None,
            problem: None,
        }
    }

    /// The rows of an import scan of this source: one per server, one with `problem` per entry
    /// that could not be read.
    pub fn rows(&self, inputs: &DiscoveryInputs, report: ScanReport) -> Scan {
        let mut scan = Scan::default();
        for candidate in report.candidates {
            scan.found.push(self.found(inputs, candidate));
        }
        for problem in report.problems {
            let Some(name) = problem.item else {
                scan.problems.push(self.problem(inputs, problem.message));
                continue;
            };
            let mut row = self.row(inputs, &name);
            row.importable = false;
            row.problem = Some(problem.message);
            scan.found.push(Found { row, export: None });
        }
        scan
    }

    fn found(&self, inputs: &DiscoveryInputs, candidate: Candidate) -> Found {
        // An MCP candidate's id is `mcp:<key in the source>`.
        let name = candidate.id.strip_prefix("mcp:").unwrap_or(&candidate.id).to_string();
        let mut row = self.row(inputs, &name);
        row.import_name = candidate.name;
        row.transport = candidate.mcp.as_ref().map_or_else(|| "unknown".to_string(), |m| m.transport.clone());
        row.summary = candidate.mcp;
        let export = match candidate.payload {
            Payload::Mcp(export) => Some(export),
            _ => None,
        };
        row.fingerprint = Some(match (&inputs.value_key, &export) {
            (Some(key), Some(export)) => key.fingerprint(&candidate.hash, export),
            _ => candidate.hash,
        });
        Found { row, export }
    }
}
