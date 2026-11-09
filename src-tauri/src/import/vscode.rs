//! VS Code with GitHub Copilot. Paths and formats from code.visualstudio.com/docs/agents/reference/mcp-configuration,
//! /docs/copilot/customization/custom-instructions and /docs/copilot/customization/agent-skills, and
//! for Copilot's own MCP file docs.github.com (Copilot CLI, "Adding MCP servers") (read 2026-11-08):
//!
//! - Global (HOME): Copilot's `~/.copilot/mcp-config.json` (`mcpServers`, the "portable" format
//!   VS Code now saves user servers in) and the skills `~/.copilot/skills`.
//! - Project: `.vscode/mcp.json` (`servers`, the VS Code format), `.github/copilot-instructions.md`,
//!   the rules `.github/instructions/*.instructions.md` and the skills `.github/skills`.
//!
//! Left to the other adapters: `.mcp.json` and `CLAUDE.md`, `.claude/skills` (Claude Code),
//! `AGENTS.md` and `.agents/skills` (Codex). Not imported: the user-profile `mcp.json` (its folder
//! depends on the VS Code build and profile, and the docs give no path), `COPILOT_HOME`, prompt
//! files and custom agents.
//!
//! A server Puppeteer cannot run as VS Code would is left out and reported: one taking a value
//! VS Code prompts for (`${input:…}`) or from the environment (`${env:…}`, `${NAME}`), from an
//! `envFile`, with a working folder (`cwd`), a sign-in (`oauth`), VS Code's sandbox or a tool
//! filter (`tools`). In `.vscode/mcp.json` the folder variables are expanded, as Cursor's are.

use super::cursor::{expand, folder_vars, globs};
use super::*;
use serde::Deserialize;

pub struct VsCode;

/// Copilot's rules and skills are for no engine in particular: the library's default.
const TARGET: &str = "both";

struct Layout {
    /// The MCP file with the key holding its servers.
    mcp: (PathBuf, &'static str),
    /// Project only: `copilot-instructions.md` with the name it is proposed under, and the
    /// `.instructions.md` folder.
    rules: Option<(PathBuf, String, PathBuf)>,
    skills: PathBuf,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    match scope {
        Scope::Global => {
            let copilot = root.join(".copilot");
            Layout { mcp: (copilot.join("mcp-config.json"), "mcpServers"), rules: None, skills: copilot.join("skills") }
        }
        Scope::Project => {
            let github = root.join(".github");
            let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            Layout {
                mcp: (root.join(".vscode").join("mcp.json"), "servers"),
                rules: Some((github.join("copilot-instructions.md"), format!("{folder}-copilot-instructions"), github.join("instructions"))),
                skills: github.join("skills"),
            }
        }
    }
}

/// Only the servers are read; serde skips every other key (`inputs` among them) without keeping it.
#[derive(Deserialize)]
struct McpFile {
    #[serde(default)]
    servers: Option<Value>,
    #[serde(rename = "mcpServers", default)]
    mcp_servers: Option<Value>,
}

impl Adapter for VsCode {
    fn id(&self) -> &'static str {
        "vscode"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The file may hold no server: telling would mean parsing it.
        if l.mcp.0.is_file() {
            kinds.push(ImportKind::Mcp);
        }
        if l.rules.is_some_and(|(file, _, dir)| (file.is_file() && !is_own_link(&file, ctx)) || !instruction_files(&dir, ctx).is_empty()) {
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
        let (path, key) = &l.mcp;
        if path.is_file() {
            let file = read_config(path, root, scope, ctx)
                .and_then(|raw| serde_json::from_str::<McpFile>(&strip_jsonc(&raw)).map_err(|e| json_error(&e)));
            let servers = match file {
                Ok(McpFile { servers, mcp_servers }) => if *key == "servers" { servers } else { mcp_servers },
                Err(e) => {
                    report.problem(path, None, e);
                    None
                }
            };
            match servers {
                Some(servers) if !servers.is_object() => report.problem(path, None, format!("`{key}` is not an object")),
                Some(servers) => {
                    let servers = match scope {
                        Scope::Global => normalized_servers(&mut report, path, servers, copilot_server),
                        Scope::Project => {
                            let vars = folder_vars(root, scope);
                            normalized_servers(&mut report, path, servers, |s| vscode_server(s, &vars))
                        }
                    };
                    mcp_candidates(&mut report, path, &servers);
                }
                None => {}
            }
        }
        let mut found = Vec::new();
        if let Some((file, name, dir)) = &l.rules {
            if file.is_file() && !is_own_link(file, ctx) {
                found.extend(text_candidate(&mut report, root, scope, ctx, ImportKind::Rule, file, Some(name.clone()), Some(TARGET)));
            }
            for path in instruction_files(dir, ctx) {
                let stem = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let name = library_name(stem.trim_end_matches(".instructions.md"));
                if let Some(mut c) = text_candidate(&mut report, root, scope, ctx, ImportKind::Rule, &path, Some(name), Some(TARGET)) {
                    let Payload::Text(text) = &c.payload else { unreachable!("a text candidate") };
                    let rule = library_rule(text);
                    c.hash = crate::commands::rules::content_hash(&rule);
                    c.payload = Payload::Text(rule);
                    found.push(c);
                }
            }
        }
        for dir in skill_dirs(&l.skills, ctx) {
            found.extend(skill_candidate(&mut report, root, scope, ctx, &dir, TARGET));
        }
        report.candidates.extend(found);
        report
    }
}

/// The `*.instructions.md` files directly in `dir`, but the app's own, by name.
fn instruction_files(dir: &Path, ctx: &ScanContext) -> Vec<PathBuf> {
    md_files(dir, ctx).into_iter().filter(|p| p.to_string_lossy().ends_with(".instructions.md")).collect()
}

/// An `.instructions.md` file as a library rule: `applyTo` becomes `paths` (but `**`, which is
/// every file), `description` stays, the rest is dropped.
fn library_rule(content: &str) -> String {
    let Some((head, body)) = split_frontmatter(content) else { return content.to_string() };
    let paths: Vec<String> =
        globs(head, frontmatter_field(head, "applyTo").unwrap_or_default()).into_iter().filter(|g| g != "**" && g != "**/*").collect();
    library_rule_text(&frontmatter(content).description, &paths, body)
}

fn set(obj: &serde_json::Map<String, Value>, key: &str) -> bool {
    obj.get(key).is_some_and(|v| !v.is_null() && v != false)
}

/// The fields VS Code and Copilot have that Puppeteer cannot run a server with.
fn unsupported(obj: &serde_json::Map<String, Value>) -> Result<(), String> {
    if set(obj, "envFile") {
        return Err("`envFile` takes its values from a file, which cannot be imported".to_string());
    }
    if set(obj, "cwd") {
        return Err("`cwd` (the folder it runs in) cannot be imported".to_string());
    }
    if set(obj, "oauth") {
        return Err("`oauth` (sign-in) cannot be imported".to_string());
    }
    if set(obj, "sandboxEnabled") {
        return Err("`sandboxEnabled` (VS Code's sandbox) cannot be imported".to_string());
    }
    Ok(())
}

/// One `.vscode/mcp.json` server in the `mcpServers` shape, folder variables expanded.
fn vscode_server(mut server: Value, vars: &[(&str, String)]) -> Result<Value, String> {
    let Some(obj) = server.as_object_mut() else { return Ok(server) };
    unsupported(obj)?;
    if serde_json::to_string(obj).is_ok_and(|s| s.contains("${input:")) {
        return Err("asks for a value when it starts (`${input:…}`), which cannot be imported".to_string());
    }
    // VS Code takes numbers too; null leaves a variable unset.
    if let Some(Value::Object(env)) = obj.get_mut("env") {
        env.retain(|_, v| !v.is_null());
        for v in env.values_mut() {
            if let Value::Number(n) = v {
                *v = Value::String(n.to_string());
            }
        }
    }
    expand(&mut server, vars)?;
    Ok(server)
}

/// One `~/.copilot/mcp-config.json` server in the `mcpServers` shape.
fn copilot_server(mut server: Value) -> Result<Value, String> {
    let Some(obj) = server.as_object_mut() else { return Ok(server) };
    unsupported(obj)?;
    if obj.get("tools").is_some_and(|t| t != &serde_json::json!(["*"])) {
        return Err("`tools` limits its tools, which cannot be imported".to_string());
    }
    if reads_environment(&Value::Object(obj.clone()), false) {
        return Err("takes a value from the environment (`${NAME}`), which cannot be imported".to_string());
    }
    if obj.get("type").is_some_and(|t| t == "local") {
        obj.insert("type".into(), "stdio".into());
    }
    Ok(server)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-vscode-{label}-{}", uuid::Uuid::new_v4()));
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

    fn problems(report: &ScanReport) -> Vec<(String, String)> {
        let mut out: Vec<_> = report.problems.iter().map(|p| (p.item.clone().unwrap_or_default(), p.message.clone())).collect();
        out.sort();
        out
    }

    fn text<'a>(report: &'a ScanReport, id: &str) -> &'a str {
        let c = report.candidates.iter().find(|c| c.id == id).unwrap();
        let Payload::Text(text) = &c.payload else { panic!("{c:?}") };
        text
    }

    fn export<'a>(report: &'a ScanReport, id: &str) -> &'a McpServerExport {
        let c = report.candidates.iter().find(|c| c.id == id).unwrap();
        let Payload::Mcp(export) = &c.payload else { panic!("{c:?}") };
        export
    }

    #[test]
    fn a_project_yields_servers_instructions_and_skills() {
        let base = tmp("project");
        let root = base.join("web app");
        let ctx = ScanContext { library_root: base.join("appdata") };
        write(
            &root.join(".vscode/mcp.json"),
            r#"{
                // VS Code keeps comments in this file.
                "inputs": [{"type": "promptString", "id": "key", "password": true}],
                "servers": {
                    "fs": {"type": "stdio", "command": "npx", "args": ["fs", "${workspaceFolder}${/}src"], "env": {"API_KEY": "SECRET-ENV", "PORT": 8080, "UNSET": null}},
                    "web": {"type": "http", "url": "https://mcp.example", "headers": {"Authorization": "Bearer SECRET-HEADER"}},
                    "prompted": {"type": "stdio", "command": "x", "env": {"KEY": "${input:key}"}},
                    "shell": {"type": "stdio", "command": "x", "env": {"KEY": "${env:SECRET_NAME}"}},
                    "file": {"type": "stdio", "command": "x", "envFile": "${workspaceFolder}/.env"},
                    "here": {"type": "stdio", "command": "x", "cwd": "${workspaceFolder}/server"},
                    "boxed": {"type": "stdio", "command": "x", "sandboxEnabled": true},
                    "unboxed": {"type": "stdio", "command": "y", "sandboxEnabled": false},
                    "signin": {"type": "http", "url": "https://x", "oauth": {"clientId": "id"}},
                },
            }"#,
        );
        write(&root.join(".github/copilot-instructions.md"), "Use pnpm.");
        write(&root.join(".github/instructions/tests.instructions.md"), "---\napplyTo: \"**/*.test.ts,e2e/**\"\ndescription: Tests\n---\nMock the API.\n");
        write(&root.join(".github/instructions/all.instructions.md"), "---\napplyTo: '**'\n---\nBe brief.");
        write(&root.join(".github/instructions/notes.md"), "Not an instructions file.");
        write(&root.join(".github/skills/pdf/SKILL.md"), "---\nname: pdf\n---\nRead PDFs.");
        // Left to the Claude Code adapter.
        write(&root.join(".mcp.json"), r#"{"mcpServers": {"other": {"command": "x"}}}"#);

        let report = VsCode.scan(&root, Scope::Project, &ctx);
        let mut got = ids(&report);
        got.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(
            got,
            [
                (ImportKind::Mcp, "mcp:fs".into(), "fs".into()),
                (ImportKind::Mcp, "mcp:unboxed".into(), "unboxed".into()),
                (ImportKind::Mcp, "mcp:web".into(), "web".into()),
                (ImportKind::Rule, "rules:.github/copilot-instructions.md".into(), "web-app-copilot-instructions".into()),
                (ImportKind::Rule, "rules:.github/instructions/all.instructions.md".into(), "all".into()),
                (ImportKind::Rule, "rules:.github/instructions/tests.instructions.md".into(), "tests".into()),
                (ImportKind::Skill, "skills:.github/skills/pdf".into(), "pdf".into()),
            ]
        );
        let fs = export(&report, "mcp:fs");
        assert_eq!(fs.args, ["fs".to_string(), format!("{}/src", root.display())]);
        assert_eq!((fs.env["PORT"].as_str(), fs.env.contains_key("UNSET")), ("8080", false));
        assert_eq!(export(&report, "mcp:web").transport, "http");
        assert_eq!(
            problems(&report),
            [
                ("boxed".into(), "`sandboxEnabled` (VS Code's sandbox) cannot be imported".into()),
                ("file".into(), "`envFile` takes its values from a file, which cannot be imported".into()),
                ("here".into(), "`cwd` (the folder it runs in) cannot be imported".into()),
                ("prompted".into(), "asks for a value when it starts (`${input:…}`), which cannot be imported".into()),
                (
                    "shell".into(),
                    "uses a `${…}` variable that cannot be expanded on import (`${env:…}` or one tied to the open project)".into()
                ),
                ("signin".into(), "`oauth` (sign-in) cannot be imported".into()),
            ]
        );
        assert_eq!(text(&report, "rules:.github/copilot-instructions.md"), "Use pnpm.");
        assert_eq!(
            text(&report, "rules:.github/instructions/tests.instructions.md"),
            "---\ndescription: \"Tests\"\npaths:\n  - \"**/*.test.ts\"\n  - \"e2e/**\"\n---\nMock the API.\n"
        );
        assert_eq!(text(&report, "rules:.github/instructions/all.instructions.md"), "Be brief.");
        assert_eq!(VsCode.detect(&root, Scope::Project, &ctx), [ImportKind::Mcp, ImportKind::Rule, ImportKind::Skill]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
    }

    #[test]
    fn home_reads_copilots_own_files() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("appdata") };
        write(
            &home.join(".copilot/mcp-config.json"),
            r#"{"mcpServers": {
                "gh": {"type": "local", "command": "gh-mcp", "tools": ["*"], "env": {"GH_TOKEN": "SECRET-ENV"}},
                "remote": {"type": "http", "url": "https://x", "headers": {"X-Key": "SECRET-HEADER"}},
                "some": {"command": "x", "tools": ["read"]},
                "shell": {"command": "x", "env": {"T": "${SECRET_NAME:-dev}"}}
            }}"#,
        );
        write(&home.join(".copilot/skills/notes/SKILL.md"), "Take notes.");
        // Neither VS Code's project files nor instructions at HOME.
        write(&home.join(".vscode/mcp.json"), r#"{"servers": {"x": {"command": "x"}}}"#);
        write(&home.join(".github/copilot-instructions.md"), "x");

        let report = VsCode.scan(&home, Scope::Global, &ctx);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:gh".into(), "gh".into()),
                (ImportKind::Mcp, "mcp:remote".into(), "remote".into()),
                (ImportKind::Skill, "skills:.copilot/skills/notes".into(), "notes".into()),
            ]
        );
        assert_eq!(export(&report, "mcp:gh").transport, "stdio");
        assert_eq!(
            problems(&report),
            [
                ("shell".into(), "takes a value from the environment (`${NAME}`), which cannot be imported".into()),
                ("some".into(), "`tools` limits its tools, which cannot be imported".into()),
            ]
        );
        assert_eq!(VsCode.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp, ImportKind::Skill]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
    }

    #[test]
    fn a_broken_file_is_reported_without_quoting_it() {
        let root = tmp("broken");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".vscode/mcp.json"), "{\"servers\": {\"a\": {\"command\": \"SECRET-UNTERMINATED}}");
        write(&root.join(".github/instructions/bad.instructions.md"), [0xff, 0xfe]);
        write(&root.join(".github/copilot-instructions.md"), "ok");
        let report = VsCode.scan(&root, Scope::Project, &ctx);
        assert_eq!(ids(&report).len(), 1);
        let messages: Vec<_> = report.problems.iter().map(|p| p.message.split(" at ").next().unwrap()).collect();
        assert_eq!(messages, ["invalid JSON", "not UTF-8 text"]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));

        write(&root.join(".vscode/mcp.json"), r#"{"servers": ["SECRET-SHAPE"]}"#);
        assert_eq!(VsCode.scan(&root, Scope::Project, &ctx).problems[0].message, "`servers` is not an object");
        // Only the VS Code format's key is read there.
        write(&root.join(".vscode/mcp.json"), r#"{"mcpServers": {"x": {"command": "x"}}}"#);
        let report = VsCode.scan(&root, Scope::Project, &ctx);
        assert!(report.candidates.iter().all(|c| c.kind != ImportKind::Mcp) && report.problems.len() == 1);
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(VsCode.detect(&root, scope, &ctx), []);
            let report = VsCode.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
    }
}
