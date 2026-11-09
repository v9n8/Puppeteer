//! Claude Desktop (the chat app). Path and format from the MCP docs Anthropic points to,
//! modelcontextprotocol.io/docs/develop/connect-local-servers (read 2026-11-08):
//!
//! - Global only: `claude_desktop_config.json` in the app's folder — `~/Library/Application
//!   Support/Claude` on macOS, `%APPDATA%\Claude` on Windows, `~/.config/Claude` on Linux. The app
//!   has no project config.
//! - Only its top-level `mcpServers` is read; the file also keeps the app's preferences, so no
//!   other key is even deserialized. Values are used as written: the app expands no variable.
//!
//! Remote servers are added as connectors in the app's settings (the docs' connect-remote-servers
//! page), not in this file; Desktop Extensions are installed there too. Neither is read here.

use super::*;
use serde::Deserialize;

pub struct ClaudeDesktop;

/// The config file under HOME.
pub(crate) fn config_path(home: &Path) -> PathBuf {
    let folder = if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        home.join("AppData").join("Roaming")
    } else {
        home.join(".config")
    };
    folder.join("Claude").join("claude_desktop_config.json")
}

/// Only `mcpServers` is read; serde skips every other key without keeping it.
#[derive(Deserialize)]
struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: Option<Value>,
}

impl Adapter for ClaudeDesktop {
    fn id(&self) -> &'static str {
        "claude_desktop"
    }

    fn detect(&self, root: &Path, scope: Scope, _ctx: &ScanContext) -> Vec<ImportKind> {
        // The config may hold no server: telling would mean parsing it.
        if scope == Scope::Global && config_path(root).is_file() { vec![ImportKind::Mcp] } else { Vec::new() }
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let mut report = ScanReport::default();
        let path = config_path(root);
        if scope != Scope::Global || !path.is_file() {
            return report;
        }
        let config =
            read_config(&path, root, scope, ctx).and_then(|raw| serde_json::from_str::<McpConfig>(&raw).map_err(|e| json_error(&e)));
        match config {
            Ok(McpConfig { mcp_servers: Some(servers) }) => mcp_candidates(&mut report, &path, &servers),
            Ok(_) => {}
            Err(e) => report.problem(&path, None, e),
        }
        report
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-claude-desktop-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn ctx(home: &Path) -> ScanContext {
        ScanContext { library_root: home.join("Library/Application Support/io.github.v9n8.puppeteer") }
    }

    #[test]
    fn the_config_lives_in_the_apps_folder() {
        let want = if cfg!(target_os = "macos") {
            "/h/Library/Application Support/Claude/claude_desktop_config.json"
        } else {
            "/h/.config/Claude/claude_desktop_config.json"
        };
        assert_eq!(config_path(Path::new("/h")), PathBuf::from(want));
    }

    #[test]
    fn home_yields_its_servers_and_nothing_else_of_the_file() {
        let home = tmp("home");
        let ctx = ctx(&home);
        write(
            &config_path(&home),
            r#"{
                "globalShortcut": "SECRET-PREFERENCE",
                "preferences": {"menuBarEnabled": true, "token": "SECRET-PREFERENCE"},
                "mcpServers": {
                    "filesystem": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/me/Desktop"]},
                    "brave search": {"command": "npx", "args": ["-y", "brave"], "env": {"APPDATA": "C:\\x", "BRAVE_API_KEY": "SECRET-ENV"}},
                    "bad": {"command": "x", "env": {"TOKEN": ["SECRET-BAD"]}},
                    "empty": {}
                }
            }"#,
        );

        let report = ClaudeDesktop.scan(&home, Scope::Global, &ctx);
        assert!(report.candidates.iter().all(|c| c.kind == ImportKind::Mcp));
        let mut got: Vec<_> = report.candidates.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
        got.sort();
        assert_eq!(got, [("mcp:brave search", "brave-search"), ("mcp:filesystem", "filesystem")]);
        let by_id = |id: &str| report.candidates.iter().find(|c| c.id == id).unwrap();
        let brave = by_id("mcp:brave search");
        assert_eq!(brave.mcp.as_ref().unwrap().env_keys, ["APPDATA", "BRAVE_API_KEY"]);
        assert!(brave.origin.ends_with("Claude/claude_desktop_config.json"), "{}", brave.origin);
        let Payload::Mcp(fs) = &by_id("mcp:filesystem").payload else { panic!() };
        assert_eq!((fs.transport.as_str(), fs.args.len()), ("stdio", 3));

        let mut problems: Vec<_> = report.problems.iter().map(|p| (p.item.as_deref(), p.message.as_str())).collect();
        problems.sort();
        assert_eq!(problems, [(Some("bad"), "`env.TOKEN` is not a string"), (Some("empty"), "needs a `command` or a `url`")]);
        assert_eq!(ClaudeDesktop.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
    }

    #[test]
    fn a_broken_or_serverless_config_is_reported_without_quoting_it() {
        let home = tmp("broken");
        let ctx = ctx(&home);
        let path = config_path(&home);
        write(&path, "{\"mcpServers\": {\"a\": {\"command\": \"SECRET-UNTERMINATED}}");
        let report = ClaudeDesktop.scan(&home, Scope::Global, &ctx);
        assert!(report.candidates.is_empty());
        assert!(report.problems[0].message.starts_with("invalid JSON at line"), "{:?}", report.problems);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));

        write(&path, [0xff, 0xfe]);
        assert_eq!(ClaudeDesktop.scan(&home, Scope::Global, &ctx).problems[0].message, "not UTF-8 text");
        write(&path, r#"{"mcpServers": ["SECRET-SHAPE"]}"#);
        let report = ClaudeDesktop.scan(&home, Scope::Global, &ctx);
        assert_eq!(report.problems[0].message, "`mcpServers` is not an object");
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
        // The app writes the file with only its preferences until a server is added.
        write(&path, r#"{"preferences": {"x": 1}}"#);
        let report = ClaudeDesktop.scan(&home, Scope::Global, &ctx);
        assert!(report.candidates.is_empty() && report.problems.is_empty());
    }

    #[test]
    fn a_project_or_a_home_without_the_app_has_nothing_to_import() {
        let root = tmp("none");
        let ctx = ctx(&root);
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(ClaudeDesktop.detect(&root, scope, &ctx), []);
            let report = ClaudeDesktop.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
        // A project folder laid out like HOME is still not read.
        write(&config_path(&root), r#"{"mcpServers": {"a": {"command": "a"}}}"#);
        assert_eq!(ClaudeDesktop.detect(&root, Scope::Project, &ctx), []);
        assert!(ClaudeDesktop.scan(&root, Scope::Project, &ctx).candidates.is_empty());
    }

    #[test]
    fn same_servers_hash_alike_from_claude_code_and_claude_desktop() {
        let home = tmp("hash");
        let ctx = ctx(&home);
        write(&config_path(&home), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["fs"], "env": {"K": "desktop value"}}}}"#);
        write(&home.join(".claude.json"), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["fs"], "env": {"K": "code value"}}}}"#);
        let desktop = ClaudeDesktop.scan(&home, Scope::Global, &ctx);
        let code = super::super::claude_code::ClaudeCode.scan(&home, Scope::Global, &ctx);
        assert_eq!(desktop.candidates[0].hash, code.candidates[0].hash);
    }
}
