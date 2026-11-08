//! Gemini CLI (the `gemini` CLI). Paths and formats from its docs in the google-gemini/gemini-cli
//! repository: docs/tools/mcp-server.md, docs/reference/configuration.md, docs/cli/gemini-md.md,
//! docs/cli/skills.md and docs/cli/custom-commands.md (read 2026-11-08):
//!
//! - Global (HOME): `~/.gemini/settings.json` — only its `mcpServers`; no other key is even
//!   deserialized — the context file `~/.gemini/GEMINI.md` and the skills `~/.gemini/skills`.
//! - Project: `.gemini/settings.json`, `GEMINI.md` and `.gemini/skills`.
//!
//! `.agents/skills`, Gemini CLI's other skills folder, is left to the Codex adapter. Not imported:
//! custom commands (`.gemini/commands/*.toml`, a TOML prompt with `{{args}}`, `!{…}` and `@{…}`
//! that no library format has), the GEMINI.md files of subfolders, a context file renamed by
//! `context.fileName`, and the system settings file.
//!
//! Gemini CLI resolves `$NAME` and `${NAME}` in every string of settings.json, so a server using
//! one is left out and reported, as is one with a working folder (`cwd`), a tool filter or a
//! sign-in (`oauth`, `authProviderType`). `httpUrl` is streamable HTTP and `url` SSE. `trust`
//! (skip tool confirmations) and `timeout` are dropped.

use super::*;
use serde::Deserialize;

pub struct Gemini;

/// Gemini's rules and skills are for no engine in particular: the library's default.
const TARGET: &str = "both";

struct Layout {
    settings: PathBuf,
    /// GEMINI.md with the rule name it is proposed under.
    context: (PathBuf, String),
    skills: PathBuf,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    let dot = root.join(".gemini");
    let context = match scope {
        Scope::Global => (dot.join("GEMINI.md"), "global-gemini-md".to_string()),
        Scope::Project => {
            let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            (root.join("GEMINI.md"), format!("{folder}-gemini-md"))
        }
    };
    Layout { settings: dot.join("settings.json"), context, skills: dot.join("skills") }
}

/// Only `mcpServers` is read; serde skips every other key without keeping it.
#[derive(Deserialize)]
struct Settings {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: Option<Value>,
}

impl Adapter for Gemini {
    fn id(&self) -> &'static str {
        "gemini_cli"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The settings may hold no server: telling would mean parsing them.
        if l.settings.is_file() {
            kinds.push(ImportKind::Mcp);
        }
        if l.context.0.is_file() && !is_own_link(&l.context.0, ctx) {
            kinds.push(ImportKind::Rule);
        }
        if !skill_dirs(&l.skills, ctx).is_empty() {
            kinds.push(ImportKind::Skill);
        }
        kinds
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let l = layout(root, scope);
        let mut report = ScanReport::default();
        if l.settings.is_file() {
            let settings = read_text(&l.settings, MAX_CONFIG_BYTES)
                .and_then(|raw| serde_json::from_str::<Settings>(&strip_jsonc(&raw)).map_err(|e| json_error(&e)));
            match settings {
                Ok(Settings { mcp_servers: Some(servers) }) => {
                    let servers = normalized_servers(&mut report, &l.settings, servers, server);
                    mcp_candidates(&mut report, &l.settings, &servers);
                }
                Ok(_) => {}
                Err(e) => report.problem(&l.settings, None, e),
            }
        }
        let mut found = Vec::new();
        let (path, name) = &l.context;
        if path.is_file() && !is_own_link(path, ctx) {
            found.extend(text_candidate(&mut report, root, ImportKind::Rule, path, Some(name.clone()), Some(TARGET)));
        }
        for dir in skill_dirs(&l.skills, ctx) {
            found.extend(skill_candidate(&mut report, root, &dir, TARGET));
        }
        report.candidates.extend(found);
        report
    }
}

/// One Gemini server in the `mcpServers` shape, or why Puppeteer cannot run it as Gemini CLI
/// would. Errors name the field, never its value.
fn server(mut server: Value) -> Result<Value, String> {
    let Some(obj) = server.as_object_mut() else { return Ok(server) };
    let set = |obj: &serde_json::Map<String, Value>, key: &str| obj.get(key).is_some_and(|v| !v.is_null());
    if set(obj, "cwd") {
        return Err("`cwd` (the folder it runs in) cannot be imported".to_string());
    }
    if let Some(key) = ["includeTools", "excludeTools"].into_iter().find(|k| set(obj, k)) {
        return Err(format!("`{key}` limits its tools, which cannot be imported"));
    }
    let discovery = obj.get("authProviderType").is_none_or(|v| v.is_null() || v == "dynamic_discovery");
    if set(obj, "oauth") || !discovery {
        return Err("a sign-in (`oauth`, `authProviderType`) cannot be imported".to_string());
    }
    if reads_environment(&Value::Object(obj.clone()), true) {
        return Err("takes a value from the environment (`$NAME`, `${NAME}`), which cannot be imported".to_string());
    }
    if let Some(url) = obj.remove("httpUrl").filter(|v| !v.is_null()) {
        obj.insert("url".into(), url);
        obj.insert("type".into(), "http".into());
    } else if set(obj, "url") && !set(obj, "type") {
        obj.insert("type".into(), "sse".into());
    }
    Ok(server)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-gemini-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn ids(report: &ScanReport) -> Vec<(ImportKind, String, String)> {
        report.candidates.iter().map(|c| (c.kind, c.id.clone(), c.name.clone())).collect()
    }

    fn export<'a>(report: &'a ScanReport, id: &str) -> &'a McpServerExport {
        let c = report.candidates.iter().find(|c| c.id == id).unwrap();
        let Payload::Mcp(export) = &c.payload else { panic!("{c:?}") };
        export
    }

    #[test]
    fn home_yields_servers_its_context_file_and_skills() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("appdata") };
        write(
            &home.join(".gemini/settings.json"),
            r#"{
                // Gemini CLI writes and reads comments here.
                "security": {"auth": {"selectedType": "ACCOUNT-SECRET"}},
                "mcpServers": {
                    "fs": {"command": "npx", "args": ["-y", "fs"], "env": {"API_KEY": "SECRET-ENV"}, "trust": true, "timeout": 5000},
                    "stream": {"httpUrl": "https://mcp.example/mcp", "headers": {"Authorization": "Bearer SECRET-HEADER"}},
                    "events": {"url": "https://mcp.example/sse"},
                    "typed": {"url": "https://mcp.example/x", "type": "http"},
                    "shell": {"command": "x", "env": {"TOKEN": "$SECRET_NAME"}},
                    "braced": {"command": "x", "args": ["${HOME}/x"]},
                    "here": {"command": "python", "cwd": "./servers"},
                    "picky": {"command": "x", "excludeTools": ["rm"]},
                    "google": {"httpUrl": "https://x", "authProviderType": "google_credentials"},
                    "oauth": {"url": "https://x", "oauth": {"clientSecret": "SECRET-CLIENT"}},
                    "bad": {"command": "x", "args": "SECRET-ARGS"},
                },
            }"#,
        );
        write(&home.join(".gemini/GEMINI.md"), "Answer briefly.");
        write(&home.join(".gemini/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs.");
        write(&home.join(".gemini/commands/ship.toml"), "prompt = \"Ship {{args}}\"");

        let report = Gemini.scan(&home, Scope::Global, &ctx);
        let mut got = ids(&report);
        got.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(
            got,
            [
                (ImportKind::Mcp, "mcp:events".into(), "events".into()),
                (ImportKind::Mcp, "mcp:fs".into(), "fs".into()),
                (ImportKind::Mcp, "mcp:stream".into(), "stream".into()),
                (ImportKind::Mcp, "mcp:typed".into(), "typed".into()),
                (ImportKind::Rule, "rules:.gemini/GEMINI.md".into(), "global-gemini-md".into()),
                (ImportKind::Skill, "skills:.gemini/skills/pdf".into(), "pdf".into()),
            ]
        );
        assert_eq!(export(&report, "mcp:fs").transport, "stdio");
        let stream = export(&report, "mcp:stream");
        assert_eq!((stream.transport.as_str(), stream.url.as_deref()), ("http", Some("https://mcp.example/mcp")));
        assert_eq!(export(&report, "mcp:events").transport, "sse");
        assert_eq!(export(&report, "mcp:typed").transport, "http");

        let mut problems: Vec<_> = report.problems.iter().map(|p| (p.item.clone().unwrap(), p.message.as_str())).collect();
        problems.sort();
        let env = "takes a value from the environment (`$NAME`, `${NAME}`), which cannot be imported";
        let sign_in = "a sign-in (`oauth`, `authProviderType`) cannot be imported";
        assert_eq!(
            problems,
            [
                ("bad".into(), "`args` is not a list of strings"),
                ("braced".into(), env),
                ("google".into(), sign_in),
                ("here".into(), "`cwd` (the folder it runs in) cannot be imported"),
                ("oauth".into(), sign_in),
                ("picky".into(), "`excludeTools` limits its tools, which cannot be imported"),
                ("shell".into(), env),
            ]
        );
        assert_eq!(Gemini.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp, ImportKind::Rule, ImportKind::Skill]);
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("SECRET") && !json.contains("ACCOUNT-SECRET"), "{json}");
    }

    #[test]
    fn a_project_reads_its_own_files() {
        let base = tmp("project");
        let root = base.join("web app");
        let ctx = ScanContext { library_root: base.join("appdata") };
        write(&root.join(".gemini/settings.json"), r#"{"mcpServers": {"db": {"command": "db-mcp"}}}"#);
        write(&root.join("GEMINI.md"), "Use pnpm.");
        // HOME's place of the context file means nothing in a project.
        write(&root.join(".gemini/GEMINI.md"), "x");
        // Left to the Codex adapter.
        write(&root.join(".agents/skills/x/SKILL.md"), "x");

        let report = Gemini.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems, []);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:db".into(), "db".into()),
                (ImportKind::Rule, "rules:GEMINI.md".into(), "web-app-gemini-md".into()),
            ]
        );
        assert_eq!(report.candidates[1].target.as_deref(), Some("both"));
        assert_eq!(Gemini.detect(&root, Scope::Project, &ctx), [ImportKind::Mcp, ImportKind::Rule]);
    }

    #[test]
    fn a_broken_file_is_reported_without_quoting_it() {
        let root = tmp("broken");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".gemini/settings.json"), "{\"mcpServers\": {\"a\": {\"command\": \"SECRET-UNTERMINATED}}");
        write(&root.join("GEMINI.md"), [0xff, 0xfe]);
        write(&root.join(".gemini/skills/ok/SKILL.md"), "ok");
        let report = Gemini.scan(&root, Scope::Project, &ctx);
        assert_eq!(ids(&report), [(ImportKind::Skill, "skills:.gemini/skills/ok".into(), "ok".into())]);
        let messages: Vec<_> = report.problems.iter().map(|p| p.message.split(" at ").next().unwrap()).collect();
        assert_eq!(messages, ["invalid JSON", "not UTF-8 text"]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));

        write(&root.join(".gemini/settings.json"), r#"{"mcpServers": ["SECRET-SHAPE"]}"#);
        let report = Gemini.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems[0].message, "`mcpServers` is not an object");
        write(&root.join(".gemini/settings.json"), r#"{"model": {"name": "x"}}"#);
        assert!(Gemini.scan(&root, Scope::Project, &ctx).problems.iter().all(|p| !p.path.ends_with("settings.json")));
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(Gemini.detect(&root, scope, &ctx), []);
            let report = Gemini.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
    }
}
