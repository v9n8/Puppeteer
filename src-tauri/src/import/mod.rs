//! Import from other AI tools (report §5.1.9): one adapter per tool finds its config (detect),
//! reads it into tool-neutral candidates (scan), and the picked ones are copied into Puppeteer's
//! libraries ([`apply`]).
//!
//! The flow is three commands ([`commands`]), all local-only and gated on [`FEATURE`]:
//! 1. `list_import_sources`: each (adapter, scope) holding something to import — HOME (`global`)
//!    and every project.
//! 2. `scan_import_source`: that source's candidates, plus one problem per item that could not be
//!    read. A broken item never fails the scan.
//! 3. `import_candidates`: the picked candidates, by id, one outcome each. The source is scanned
//!    again, so file contents and MCP secret VALUEs never travel through the webview.
//!
//! Only copies: a source file is read, never written, moved or deleted. Nothing read from a file
//! is logged; JSON errors are reduced to line / column, since serde's message can quote a value.
//!
//! Another tool is one more [`Adapter`] in [`ADAPTERS`], built from the helpers below.

pub mod apply;
pub mod claude_code;
pub mod claude_desktop;
pub mod codex;
pub mod commands;
pub mod craft;
pub mod cursor;
pub mod gemini;
pub mod vscode;
pub mod windsurf;

use crate::commands::library_items::LibraryKind;
use crate::commands::mcp::McpServerExport;
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const FEATURE: &str = "library.import_external";

/// Markdown read for a rule, agent, command or SKILL.md.
const MAX_TEXT_BYTES: u64 = 1024 * 1024;
/// A JSON config (`~/.claude.json` also keeps the CLI's history and caches).
const MAX_CONFIG_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The tool's config in HOME.
    Global,
    /// The tool's config in one project folder.
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Mcp,
    Rule,
    Skill,
    Agent,
    Command,
}

impl ImportKind {
    /// The library table, where a name is looked up.
    pub fn table(self) -> &'static str {
        match self {
            ImportKind::Mcp => "mcp_servers",
            ImportKind::Rule => "rules",
            ImportKind::Skill => "skills",
            ImportKind::Agent => "agents",
            ImportKind::Command => "commands",
        }
    }

    /// The prop this kind's library also needs (skills, rules and MCP are core).
    pub fn feature(self) -> Option<&'static str> {
        match self {
            ImportKind::Agent => Some(LibraryKind::Agent.feature()),
            ImportKind::Command => Some(LibraryKind::Command.feature()),
            _ => None,
        }
    }
}

/// What a candidate brings into the library. Never serialized: file contents and secret VALUEs
/// stay in the backend.
#[derive(Debug, Clone)]
pub enum Payload {
    Mcp(McpServerExport),
    /// A rule, agent or command file.
    Text(String),
    /// A skill folder (SKILL.md and its files), read when imported.
    SkillDir(PathBuf),
}

/// An MCP server as previewed: KEY names only, never a VALUE.
#[derive(Debug, Clone, Serialize)]
pub struct McpSummary {
    pub transport: String,
    pub command: Option<String>,
    /// Shown with the command: `npx -y some-pkg` must not read as just `npx`.
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
}

/// One importable item, in the same shape whatever tool it comes from.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// Stable within its source: `<kind>:<path from the source root>`, `mcp:<server name>`.
    pub id: String,
    pub kind: ImportKind,
    /// Proposed library name, already valid for every library.
    pub name: String,
    pub description: String,
    /// The file or folder it comes from (an MCP server: its config file).
    pub origin: String,
    /// Skills and rules: engines it is meant for (`claude`, `codex`, `both`).
    pub target: Option<String>,
    /// sha256 of the content (an MCP server: of its fields but the secret VALUEs), to spot one
    /// item found by two adapters.
    pub hash: String,
    /// A library item of this kind already has `name`.
    pub exists: bool,
    /// Skills: the library skill with the same SKILL.md (but for its `name:`). Importing it again
    /// is skipped, whatever the pick says.
    pub duplicate_of: Option<String>,
    pub mcp: Option<McpSummary>,
    #[serde(skip)]
    pub payload: Payload,
}

/// An item that was skipped because it could not be read. `message` never quotes the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub path: String,
    /// The entry inside the file (an MCP server name), when the file itself was readable.
    pub item: Option<String>,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ScanReport {
    pub candidates: Vec<Candidate>,
    pub problems: Vec<Problem>,
}

impl ScanReport {
    pub(crate) fn problem(&mut self, path: &Path, item: Option<&str>, message: impl Into<String>) {
        self.problems.push(Problem {
            path: path.display().to_string(),
            item: item.map(str::to_string),
            message: message.into(),
        });
    }
}

pub struct ScanContext {
    /// Puppeteer's app data: a link into it is the app's own install, not something to import.
    pub library_root: PathBuf,
}

/// One tool. `root` is HOME for [`Scope::Global`], the project folder for [`Scope::Project`].
/// Both calls only read.
pub trait Adapter: Sync {
    fn id(&self) -> &'static str;
    /// Kinds the source has something of, from cheap checks (no file is parsed).
    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind>;
    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport;
}

pub static ADAPTERS: &[&dyn Adapter] = &[
    &claude_code::ClaudeCode,
    &codex::Codex,
    &cursor::Cursor,
    &claude_desktop::ClaudeDesktop,
    &gemini::Gemini,
    &vscode::VsCode,
    &windsurf::Windsurf,
    &craft::Craft,
];

pub fn adapter(id: &str) -> Result<&'static dyn Adapter, String> {
    ADAPTERS.iter().copied().find(|a| a.id() == id).ok_or_else(|| format!("unknown import adapter '{id}'"))
}

/// Scan a source, keep the kinds whose library is on, and flag names the library already has.
pub async fn scan(db: &Db, adapter: &dyn Adapter, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
    let mut report = adapter.scan(root, scope, ctx);
    let mut off = Vec::new();
    for kind in [ImportKind::Agent, ImportKind::Command] {
        if !kind_on(db, kind).await {
            off.push(kind);
        }
    }
    report.candidates.retain(|c| !off.contains(&c.kind));
    for c in &mut report.candidates {
        c.exists = name_taken(db, c.kind, &c.name).await.unwrap_or(false);
        if let Payload::SkillDir(dir) = &c.payload {
            c.duplicate_of = same_library_skill(db, dir).await;
        }
    }
    report
}

pub(crate) async fn kind_on(db: &Db, kind: ImportKind) -> bool {
    match kind.feature() {
        Some(id) => crate::features::is_on(db, id).await,
        None => true,
    }
}

/// Case-insensitive like the file system (APFS) the library files live on.
pub(crate) async fn name_taken(db: &Db, kind: ImportKind, name: &str) -> Result<bool, String> {
    let sql = format!("SELECT 1 FROM {} WHERE name = ? COLLATE NOCASE", kind.table());
    let hit: Option<i64> = sqlx::query_scalar(&sql).bind(name).fetch_optional(db).await.map_err(|e| e.to_string())?;
    Ok(hit.is_some())
}

/// The hash of a skill folder's SKILL.md with its frontmatter `name:` left out, so a copy imported
/// under another name (which rewrites that line) still matches.
pub(crate) fn skill_body_hash(dir: &Path) -> Option<String> {
    let content = read_text(&dir.join("SKILL.md"), MAX_TEXT_BYTES).ok()?;
    Some(crate::commands::skills::skill_hash(&with_frontmatter_name(&content, "")))
}

/// The library skill whose SKILL.md is the one in `dir` but for the name. The same folder can be
/// found by two adapters (Codex and Craft both read `.agents/skills`, a skill is often kept for
/// both Claude and Codex), and must come in once.
pub(crate) async fn same_library_skill(db: &Db, dir: &Path) -> Option<String> {
    let wanted = skill_body_hash(dir)?;
    let library: Vec<(String, String)> =
        sqlx::query_as("SELECT name, source_path FROM skills ORDER BY name").fetch_all(db).await.ok()?;
    library.into_iter().find(|(_, source)| skill_body_hash(Path::new(source)).as_ref() == Some(&wanted)).map(|(name, _)| name)
}

// ---- Helpers for adapters ---------------------------------------------------------------------

/// A JSON error without serde's message, which may quote a value of the file.
pub(crate) fn json_error(e: &serde_json::Error) -> String {
    format!("invalid JSON at line {} column {}", e.line(), e.column())
}

/// A text file up to `limit` bytes.
pub(crate) fn read_text(path: &Path, limit: u64) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > limit {
        return Err(format!("file too large ({} bytes, limit {limit})", meta.len()));
    }
    String::from_utf8(fs::read(path).map_err(|e| e.to_string())?).map_err(|_| "not UTF-8 text".to_string())
}

/// A tool's config file, read where [`readable_path`] lets its source reach: a project's
/// `.mcp.json` linked to `~/.claude.json` would bring the user's own servers (and secrets) along.
pub(crate) fn read_config(path: &Path, root: &Path, scope: Scope, ctx: &ScanContext) -> Result<String, String> {
    readable_path(path, root, scope, ctx).and_then(|real| read_text(&real, MAX_CONFIG_BYTES))
}

/// Where `path` really is (links followed), when the source at `root` may read it from there. A
/// project's file must lie in the project, whatever a cloned repo links to. A HOME file may link
/// into a dotfiles folder, but not into a credential store or the app's data; one reached without
/// a link is the tool's own, even in a store (Codex keeps its prompts in `~/.codex`). Read the
/// returned path, so a link swapped in after the check is not followed. Errors name no content.
pub(crate) fn readable_path(path: &Path, root: &Path, scope: Scope, ctx: &ScanContext) -> Result<PathBuf, String> {
    let real = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let base = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    match scope {
        Scope::Project if !real.starts_with(&base) => Err("links outside the project, not imported".to_string()),
        Scope::Global
            if !path.strip_prefix(root).is_ok_and(|rel| base.join(rel) == real)
                && SecretPaths::new(Some(root), Some(&ctx.library_root)).denies(&real) =>
        {
            Err("links into a credential store or Puppeteer's data, not imported".to_string())
        }
        _ => Ok(real),
    }
}

/// A name every library accepts: ASCII letters, digits, `-`, `_` (anything else becomes `-`).
pub(crate) fn library_name(raw: &str) -> String {
    let name: String =
        raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let name = name.trim_matches('-');
    if name.is_empty() { "imported".to_string() } else { name.to_string() }
}

/// Whether `path` is a link into Puppeteer's app data (a skill / agent / rule the app installed).
pub(crate) fn is_own_link(path: &Path, ctx: &ScanContext) -> bool {
    if !fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return false;
    }
    let root = fs::canonicalize(&ctx.library_root).unwrap_or_else(|_| ctx.library_root.clone());
    fs::canonicalize(path).is_ok_and(|real| real.starts_with(root))
}

/// The `*.md` files directly in `dir` (links followed), but the app's own, by name.
pub(crate) fn md_files(dir: &Path, ctx: &ScanContext) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file() && !is_own_link(p, ctx))
        .collect();
    out.sort();
    out
}

/// The folders directly in `dir` holding a `SKILL.md`, but the app's own, by name.
pub(crate) fn skill_dirs(dir: &Path, ctx: &ScanContext) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
                && p.join("SKILL.md").is_file()
                && !is_own_link(p, ctx)
        })
        .collect();
    out.sort();
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub name: String,
    pub description: String,
    pub target: String,
}

/// The few top-level YAML frontmatter keys an import needs. A block scalar (`description: |`)
/// is joined into one line.
pub(crate) fn frontmatter(content: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let Some(rest) = content.strip_prefix("---") else { return fm };
    let Some(end) = rest.find("\n---") else { return fm };
    let lines: Vec<&str> = rest[..end].lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let slot = match key {
            "name" => &mut fm.name,
            "description" => &mut fm.description,
            "target" => &mut fm.target,
            _ => continue,
        };
        let value = value.trim();
        *slot = if matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
            lines[i + 1..]
                .iter()
                .take_while(|l| l.is_empty() || l.starts_with(char::is_whitespace))
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            value.trim_matches('"').trim_matches('\'').to_string()
        };
    }
    fm
}

/// `content` with its frontmatter `name:` set to `name` (unchanged without one). A renamed copy
/// of a skill or agent must not keep the name the tool knows the original by.
pub(crate) fn with_frontmatter_name(content: &str, name: &str) -> String {
    let Some(rest) = content.strip_prefix("---") else { return content.to_string() };
    let Some(end) = rest.find("\n---") else { return content.to_string() };
    let mut out = String::from("---");
    let mut done = false;
    for line in rest[..end].split_inclusive('\n') {
        if !done && line.starts_with("name:") {
            out.push_str(&format!("name: {name}{}", if line.ends_with('\n') { "\n" } else { "" }));
            done = true;
        } else {
            out.push_str(line);
        }
    }
    out.push_str(&rest[end..]);
    out
}

/// A rule, agent or command file as a candidate; `None` (with a problem) when unreadable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_candidate(
    report: &mut ScanReport,
    root: &Path,
    scope: Scope,
    ctx: &ScanContext,
    kind: ImportKind,
    path: &Path,
    name: Option<String>,
    target: Option<&str>,
) -> Option<Candidate> {
    let content = match readable_path(path, root, scope, ctx).and_then(|real| read_text(&real, MAX_TEXT_BYTES)) {
        Ok(c) => c,
        Err(e) => {
            report.problem(path, None, e);
            return None;
        }
    };
    let fm = frontmatter(&content);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // An agent is known by its frontmatter name; the rest by file name.
    let raw = name.unwrap_or_else(|| if kind == ImportKind::Agent && !fm.name.is_empty() { fm.name.clone() } else { stem });
    let name = library_name(&raw);
    let description = if fm.description.is_empty() { name.clone() } else { fm.description };
    Some(Candidate {
        id: candidate_id(kind, root, path),
        kind,
        name,
        description,
        origin: path.display().to_string(),
        target: target.map(str::to_string),
        hash: crate::commands::rules::content_hash(&content),
        exists: false,
        duplicate_of: None,
        mcp: None,
        payload: Payload::Text(content),
    })
}

/// A skill folder as a candidate; `None` (with a problem) when its SKILL.md is unreadable.
pub(crate) fn skill_candidate(
    report: &mut ScanReport,
    root: &Path,
    scope: Scope,
    ctx: &ScanContext,
    dir: &Path,
    default_target: &str,
) -> Option<Candidate> {
    let real_dir = match readable_path(dir, root, scope, ctx) {
        Ok(d) => d,
        Err(e) => {
            report.problem(dir, None, e);
            return None;
        }
    };
    let skill_md = dir.join("SKILL.md");
    let content = match readable_path(&skill_md, root, scope, ctx).and_then(|real| read_text(&real, MAX_TEXT_BYTES)) {
        Ok(c) => c,
        Err(e) => {
            report.problem(&skill_md, None, e);
            return None;
        }
    };
    let fm = frontmatter(&content);
    let folder = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = library_name(&folder);
    let target = if matches!(fm.target.as_str(), "claude" | "codex" | "both") { fm.target } else { default_target.to_string() };
    Some(Candidate {
        id: candidate_id(ImportKind::Skill, root, dir),
        kind: ImportKind::Skill,
        description: if fm.description.is_empty() { name.clone() } else { fm.description },
        name,
        origin: dir.display().to_string(),
        target: Some(target),
        hash: crate::commands::skills::skill_hash(&content),
        exists: false,
        duplicate_of: None,
        mcp: None,
        payload: Payload::SkillDir(real_dir),
    })
}

fn candidate_id(kind: ImportKind, root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    format!("{}:{}", kind.table(), rel.display())
}

/// The candidates of a `mcpServers` object (Claude Code, Claude Desktop, Cursor share it): one
/// per valid server, a problem per invalid one.
pub(crate) fn mcp_candidates(report: &mut ScanReport, path: &Path, servers: &Value) {
    let Some(servers) = servers.as_object() else {
        report.problem(path, None, "`mcpServers` is not an object");
        return;
    };
    for (raw_name, value) in servers {
        match mcp_export(&library_name(raw_name), value) {
            Ok(export) => report.candidates.push(mcp_candidate(path, raw_name, export)),
            Err(e) => report.problem(path, Some(raw_name), e),
        }
    }
}

/// `servers` with each server put in the `mcpServers` shape by `normalize`, but those it cannot
/// import (one problem each, by name). Not an object: left as is for [`mcp_candidates`] to report.
pub(crate) fn normalized_servers(
    report: &mut ScanReport,
    path: &Path,
    servers: Value,
    normalize: impl Fn(Value) -> Result<Value, String>,
) -> Value {
    let Value::Object(servers) = servers else { return servers };
    let mut kept = serde_json::Map::new();
    for (name, server) in servers {
        match normalize(server) {
            Ok(server) => {
                kept.insert(name, server);
            }
            Err(e) => report.problem(path, Some(&name), e),
        }
    }
    Value::Object(kept)
}

/// Whether a string of `value` names a variable of the environment the tool runs in: `${…}`, and
/// with `bare` also `$NAME`. Its value is not in the file, so an import cannot copy it.
pub(crate) fn reads_environment(value: &Value, bare: bool) -> bool {
    match value {
        Value::String(s) => s.match_indices('$').any(|(i, _)| {
            s[i + 1..].chars().next().is_some_and(|c| c == '{' || (bare && (c == '_' || c.is_ascii_alphabetic())))
        }),
        Value::Array(items) => items.iter().any(|v| reads_environment(v, bare)),
        Value::Object(map) => map.values().any(|v| reads_environment(v, bare)),
        _ => false,
    }
}

/// JSON with comments and trailing commas (VS Code's and Gemini CLI's settings files) as plain
/// JSON. Both are blanked out rather than removed, so an error keeps its line and column.
pub(crate) fn strip_jsonc(raw: &str) -> String {
    let mut out: Vec<char> = raw.chars().collect();
    let mut in_string = false;
    // A comma that is trailing if the next significant character closes its object or array.
    let mut comma = None;
    let mut i = 0;
    while i < out.len() {
        let c = out[i];
        if in_string {
            match c {
                '\\' => i += 1,
                '"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        let comment_end = match (c, out.get(i + 1)) {
            ('/', Some('/')) => Some((i..out.len()).find(|&j| out[j] == '\n').unwrap_or(out.len())),
            ('/', Some('*')) => Some((i + 2..out.len()).find(|&j| out[j] == '*' && out.get(j + 1) == Some(&'/')).map_or(out.len(), |j| j + 2)),
            _ => None,
        };
        if let Some(end) = comment_end {
            out[i..end].iter_mut().filter(|c| **c != '\n').for_each(|c| *c = ' ');
            i = end;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                comma = None;
            }
            ',' => comma = Some(i),
            '}' | ']' => {
                if let Some(at) = comma.take() {
                    out[at] = ' ';
                }
            }
            c if c.is_whitespace() => {}
            _ => comma = None,
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// The frontmatter (without its `---` lines) and the body of a Markdown file that has one.
pub(crate) fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    Some((&rest[..end], rest[end + 4..].split_once('\n').map_or("", |(_, body)| body)))
}

/// A top-level frontmatter value as written (quotes kept), `""` when the key has none.
pub(crate) fn frontmatter_field<'a>(head: &'a str, key: &str) -> Option<&'a str> {
    head.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(':').map(str::trim))
}

/// Another tool's rule as a library rule: `body` under the frontmatter Claude Code reads, where
/// `paths` limits the rule to matching files (none: always on).
pub(crate) fn library_rule_text(description: &str, paths: &[String], body: &str) -> String {
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut head = String::new();
    if !description.is_empty() {
        head += &format!("description: {}\n", quoted(description));
    }
    if !paths.is_empty() {
        head += "paths:\n";
        for path in paths {
            head += &format!("  - {}\n", quoted(path));
        }
    }
    if head.is_empty() { body.to_string() } else { format!("---\n{head}---\n{body}") }
}

/// The candidate of one MCP server read from `path`, whatever the config format. The same server
/// gets the same hash from every tool.
pub(crate) fn mcp_candidate(path: &Path, raw_name: &str, export: McpServerExport) -> Candidate {
    let mut env_keys: Vec<String> = export.env.keys().cloned().collect();
    let mut header_keys: Vec<String> = export.headers.keys().cloned().collect();
    env_keys.sort();
    header_keys.sort();
    let summary = McpSummary {
        transport: export.transport.clone(),
        command: export.command.clone(),
        args: export.args.clone(),
        url: export.url.clone(),
        env_keys,
        header_keys,
    };
    let fingerprint = serde_json::json!([summary.transport, summary.command, summary.args, summary.url, summary.env_keys, summary.header_keys]);
    Candidate {
        id: format!("mcp:{raw_name}"),
        kind: ImportKind::Mcp,
        name: export.name.clone(),
        description: String::new(),
        origin: path.display().to_string(),
        target: None,
        hash: crate::commands::rules::content_hash(&fingerprint.to_string()),
        exists: false,
        duplicate_of: None,
        mcp: Some(summary),
        payload: Payload::Mcp(export),
    }
}

/// One `mcpServers` entry. Errors name the field, never its value.
fn mcp_export(name: &str, value: &Value) -> Result<McpServerExport, String> {
    let obj = value.as_object().ok_or("not an object")?;
    let text = |key: &str| match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{key}` is not a string")),
    };
    let map = |key: &str| -> Result<HashMap<String, String>, String> {
        match obj.get(key) {
            None | Some(Value::Null) => Ok(HashMap::new()),
            Some(Value::Object(m)) => m
                .iter()
                .map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())).ok_or_else(|| format!("`{key}.{k}` is not a string")))
                .collect(),
            Some(_) => Err(format!("`{key}` is not an object")),
        }
    };
    let command = text("command")?;
    let url = text("url")?;
    let transport = match text("type")?.as_deref() {
        Some("stdio") => "stdio",
        Some("http") => "http",
        Some("sse") => "sse",
        Some(_) => return Err("unsupported `type` (stdio, http and sse only)".to_string()),
        None if command.is_some() => "stdio",
        None if url.is_some() => "http",
        None => return Err("needs a `command` or a `url`".to_string()),
    };
    let args = match obj.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or("`args` is not a list of strings")?,
        Some(_) => return Err("`args` is not a list of strings".to_string()),
    };
    Ok(McpServerExport {
        name: name.to_string(),
        description: String::new(),
        transport: transport.to_string(),
        command,
        args,
        url,
        env: map("env")?,
        headers: map("headers")?,
        enabled: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_reads_flat_and_block_values() {
        let fm = frontmatter("---\nname: \"rev\"\ndescription: |\n  Reviews code.\n  Twice.\ntarget: both\nmetadata:\n  name: inner\n---\nbody");
        assert_eq!(fm, Frontmatter { name: "rev".into(), description: "Reviews code. Twice.".into(), target: "both".into() });
        assert_eq!(frontmatter("no frontmatter"), Frontmatter::default());
        assert_eq!(frontmatter("---\nname: x\nunterminated"), Frontmatter::default());
    }

    #[test]
    fn a_renamed_copy_gets_its_own_frontmatter_name() {
        assert_eq!(with_frontmatter_name("---\nname: rev\ndescription: d\n---\nname: body", "rev-2"), "---\nname: rev-2\ndescription: d\n---\nname: body");
        assert_eq!(with_frontmatter_name("plain", "x"), "plain");
        assert_eq!(with_frontmatter_name("---\ndescription: d\n---\n", "x"), "---\ndescription: d\n---\n");
    }

    #[test]
    fn library_names_are_valid_everywhere() {
        assert_eq!(library_name("my.server name"), "my-server-name");
        assert_eq!(library_name("ok_name-1"), "ok_name-1");
        assert_eq!(library_name("..."), "imported");
        assert_eq!(library_name("đồ"), "imported");
    }

    #[test]
    fn mcp_entries_keep_values_out_of_errors() {
        let ok = mcp_export("s", &serde_json::json!({"command": "npx", "args": ["-y", "pkg"], "env": {"K": "v"}})).unwrap();
        assert_eq!((ok.transport.as_str(), ok.args.len(), ok.env["K"].as_str()), ("stdio", 2, "v"));
        let http = mcp_export("h", &serde_json::json!({"url": "https://x", "headers": {"Authorization": "Bearer t"}})).unwrap();
        assert_eq!(http.transport, "http");
        assert_eq!(mcp_export("s", &serde_json::json!({"type": "sse", "url": "https://x"})).unwrap().transport, "sse");
        for (bad, want) in [
            (serde_json::json!("SECRET-1"), "not an object"),
            (serde_json::json!({"command": "x", "env": "SECRET-2"}), "`env` is not an object"),
            (serde_json::json!({"command": "x", "env": {"TOKEN": 7}}), "`env.TOKEN` is not a string"),
            (serde_json::json!({"command": "x", "args": ["SECRET-3", 1]}), "`args` is not a list of strings"),
            (serde_json::json!({"type": "SECRET-4", "url": "u"}), "unsupported `type` (stdio, http and sse only)"),
            (serde_json::json!({"env": {}}), "needs a `command` or a `url`"),
        ] {
            assert_eq!(mcp_export("s", &bad).unwrap_err(), want);
        }
    }

    #[test]
    fn jsonc_becomes_json_without_moving_an_error() {
        let raw = "{\n  // a comment\n  \"a\": \"x // not a comment\", /* block\n */ \"b\": [1, 2,],\n  \"c\": \"q\\\"/*\",\n}";
        let json: Value = serde_json::from_str(&strip_jsonc(raw)).unwrap();
        assert_eq!(json, serde_json::json!({"a": "x // not a comment", "b": [1, 2], "c": "q\"/*"}));
        let error = |raw: &str| json_error(&serde_json::from_str::<Value>(raw).unwrap_err());
        assert_eq!(error(&strip_jsonc("{\n  /* x */ \"a\": nope\n}")), error("{\n          \"a\": nope\n}"));
        assert_eq!(strip_jsonc("{\"a\": 1} // end"), "{\"a\": 1}       ");
    }

    #[test]
    fn environment_references_are_spotted() {
        let server = serde_json::json!({"command": "x", "env": {"A": "plain $ 5", "B": "pa$sword"}});
        assert!(!reads_environment(&server, false));
        assert!(reads_environment(&server, true));
        assert!(reads_environment(&serde_json::json!({"args": ["${env:HOME}"]}), false));
        assert!(!reads_environment(&serde_json::json!({"args": ["$", "a$1", 7]}), true));
    }

    #[test]
    fn foreign_rules_get_the_librarys_frontmatter() {
        assert_eq!(split_frontmatter("---\napplyTo: '**'\n---\nbody\n"), Some(("\napplyTo: '**'", "body\n")));
        assert_eq!(split_frontmatter("no head"), None);
        assert_eq!(frontmatter_field("\ntrigger: glob\nglobs:", "globs"), Some(""));
        assert_eq!(frontmatter_field("\ntrigger: glob", "trig"), None);
        assert_eq!(library_rule_text("", &[], "b"), "b");
        assert_eq!(library_rule_text("Tests", &["*.ts".into()], "b"), "---\ndescription: \"Tests\"\npaths:\n  - \"*.ts\"\n---\nb");
    }

    #[cfg(unix)]
    #[test]
    fn links_are_read_only_where_their_source_may_reach() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("puppeteer-import-links-{}", uuid::Uuid::new_v4()));
        let write = |path: &Path, content: &str| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        let link = |target: &Path, path: &Path| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(target, path).unwrap();
        };
        let (home, proj) = (base.join("home"), base.join("proj"));
        let ctx = ScanContext { library_root: home.join("Library/Application Support/io.github.v9n8.puppeteer") };
        write(&home.join(".ssh/id_ed25519"), "SECRET-PRIVATE-KEY");
        write(&home.join(".claude.json"), r#"{"mcpServers": {"mine": {"command": "m", "env": {"K": "SECRET-MCP-ENV"}}}}"#);
        write(&home.join(".aws/mcp.json"), r#"{"mcpServers": {"aws": {"command": "a", "env": {"K": "SECRET-AWS-ENV"}}}}"#);
        write(&home.join(".aws/kit/SKILL.md"), "SECRET-AWS-SKILL");
        write(&home.join("dotfiles/agents/rev.md"), "Review.");
        write(&base.join("elsewhere/notes.md"), "SECRET-OUTSIDE");
        write(&base.join("elsewhere/kit/SKILL.md"), "SECRET-OUTSIDE-SKILL");
        write(&proj.join("docs/style.md"), "Use tabs.");
        write(&proj.join(".claude/skills/inner/ref.md"), "ref");

        link(&proj.join("docs/style.md"), &proj.join(".claude/rules/style.md"));
        link(&base.join("elsewhere/notes.md"), &proj.join(".claude/rules/notes.md"));
        link(&home.join(".ssh/id_ed25519"), &proj.join(".claude/agents/key.md"));
        link(&base.join("elsewhere/kit"), &proj.join(".claude/skills/kit"));
        link(&base.join("elsewhere/kit/SKILL.md"), &proj.join(".claude/skills/inner/SKILL.md"));
        link(Path::new("../../../home/.ssh/id_ed25519"), &proj.join(".cursor/rules/style.mdc"));
        link(Path::new("../home/.claude.json"), &proj.join(".mcp.json"));
        link(&home.join(".claude.json"), &proj.join(".cursor/mcp.json"));
        let mut report = claude_code::ClaudeCode.scan(&proj, Scope::Project, &ctx);
        let cursor = cursor::Cursor.scan(&proj, Scope::Project, &ctx);
        report.candidates.extend(cursor.candidates);
        report.problems.extend(cursor.problems);
        let names = |r: &ScanReport| r.candidates.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
        let refused = |r: &ScanReport, root: &Path| {
            let mut p: Vec<String> =
                r.problems.iter().map(|p| Path::new(&p.path).strip_prefix(root).unwrap().display().to_string()).collect();
            p.sort();
            p
        };
        assert_eq!(names(&report), ["style"]);
        assert_eq!(
            refused(&report, &proj),
            [
                ".claude/agents/key.md",
                ".claude/rules/notes.md",
                ".claude/skills/inner/SKILL.md",
                ".claude/skills/kit",
                ".cursor/mcp.json",
                ".cursor/rules/style.mdc",
                ".mcp.json"
            ]
        );

        link(&home.join(".ssh/id_ed25519"), &home.join(".claude/agents/key.md"));
        link(&home.join("dotfiles/agents/rev.md"), &home.join(".claude/agents/rev.md"));
        link(&home.join(".aws/kit"), &home.join(".claude/skills/aws"));
        link(&base.join("elsewhere/notes.md"), &home.join(".claude/rules/notes.md"));
        link(&home.join(".aws/mcp.json"), &home.join(".cursor/mcp.json"));
        let mut global = claude_code::ClaudeCode.scan(&home, Scope::Global, &ctx);
        let cursor = cursor::Cursor.scan(&home, Scope::Global, &ctx);
        global.candidates.extend(cursor.candidates);
        global.problems.extend(cursor.problems);
        // `~/.claude.json` itself is the tool's own config, read without a link.
        assert_eq!(names(&global), ["mine", "notes", "rev"]);
        assert_eq!(refused(&global, &home), [".claude/agents/key.md", ".claude/skills/aws", ".cursor/mcp.json"]);

        // HOME may link outside itself (`notes`), but never into `~/.ssh` or `~/.aws`.
        for (r, leak) in [(&report, "SECRET-"), (&global, "SECRET-PRIVATE-KEY"), (&global, "SECRET-AWS")] {
            let shown = format!("{:?}{:?}", r.problems, r.candidates.iter().map(|c| (&c.description, &c.payload)).collect::<Vec<_>>());
            assert!(!shown.contains(leak), "{shown}");
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn json_errors_do_not_quote_the_file() {
        let e = serde_json::from_str::<HashMap<String, String>>("{\"k\": 5}").unwrap_err();
        assert_eq!(json_error(&e), "invalid JSON at line 1 column 7");
    }
}
