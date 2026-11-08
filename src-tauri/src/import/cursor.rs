//! Cursor (the editor). Paths and formats from Cursor's docs, cursor.com/docs/context/mcp and
//! cursor.com/docs/context/rules (read 2026-11-08):
//!
//! - Global (HOME): `~/.cursor/mcp.json`. User Rules live in Cursor's settings, not in a file.
//! - Project: `.cursor/mcp.json`, the rules `.cursor/rules/**/*.mdc` (Cursor ignores a `.md`
//!   there) and the legacy `.cursorrules` file. `AGENTS.md`, which Cursor also reads, is left to
//!   the Codex adapter.
//!
//! An `.mdc` rule's frontmatter becomes the one Claude Code reads: `description` stays, `globs`
//! becomes `paths` (but with `alwaysApply: true`, where Cursor ignores them), the rest is dropped.
//! A rule Cursor only pulls in when relevant or when `@`-mentioned has no counterpart and comes in
//! always on, as every library rule is.

use super::*;
use serde::Deserialize;

pub struct Cursor;

/// Cursor's rules are for no engine in particular: the library's default.
const TARGET: &str = "both";

struct Layout {
    mcp: PathBuf,
    /// The rules folder and the legacy `.cursorrules` with the name it is proposed under, project only.
    rules: Option<(PathBuf, PathBuf, String)>,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    let dot = root.join(".cursor");
    let rules = (scope == Scope::Project).then(|| {
        let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        (dot.join("rules"), root.join(".cursorrules"), format!("{folder}-cursorrules"))
    });
    Layout { mcp: dot.join("mcp.json"), rules }
}

/// Only `mcpServers` is read; serde skips every other key without keeping it.
#[derive(Deserialize)]
struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: Option<Value>,
}

impl Adapter for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The config may hold no server: telling would mean parsing it.
        if l.mcp.is_file() {
            kinds.push(ImportKind::Mcp);
        }
        if l.rules.is_some_and(|(dir, legacy, _)| legacy.is_file() || !mdc_files(&dir, ctx).is_empty()) {
            kinds.push(ImportKind::Rule);
        }
        kinds
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let l = layout(root, scope);
        let mut report = ScanReport::default();
        if l.mcp.is_file() {
            let config = read_text(&l.mcp, MAX_CONFIG_BYTES)
                .and_then(|raw| serde_json::from_str::<McpConfig>(&raw).map_err(|e| json_error(&e)));
            match config {
                Ok(McpConfig { mcp_servers: Some(servers) }) => {
                    let servers = importable_servers(&mut report, &l.mcp, root, scope, servers);
                    mcp_candidates(&mut report, &l.mcp, &servers);
                }
                Ok(_) => {}
                Err(e) => report.problem(&l.mcp, None, e),
            }
        }
        let Some((dir, legacy, legacy_name)) = l.rules else { return report };
        let mut found = Vec::new();
        for path in mdc_files(&dir, ctx) {
            let rel = path.strip_prefix(&dir).unwrap_or(&path).with_extension("");
            let name = library_name(&rel.to_string_lossy());
            if let Some(mut c) = text_candidate(&mut report, root, ImportKind::Rule, &path, Some(name), Some(TARGET)) {
                let Payload::Text(text) = &c.payload else { unreachable!("a text candidate") };
                let rule = library_rule(text);
                c.hash = crate::commands::rules::content_hash(&rule);
                c.payload = Payload::Text(rule);
                found.push(c);
            }
        }
        if legacy.is_file() && !is_own_link(&legacy, ctx) {
            found.extend(text_candidate(&mut report, root, ImportKind::Rule, &legacy, Some(legacy_name), Some(TARGET)));
        }
        report.candidates.extend(found);
        report
    }
}

/// The `*.mdc` files under `dir`, subfolders included (Cursor groups rules in folders), but the
/// app's own, by path.
fn mdc_files(dir: &Path, ctx: &ScanContext) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .map(walkdir::DirEntry::into_path)
        .filter(|p| p.extension().is_some_and(|x| x == "mdc") && p.is_file() && !is_own_link(p, ctx))
        .collect();
    out.sort();
    out
}

/// `servers` without those Puppeteer cannot run as Cursor would (one problem each), and with
/// Cursor's folder variables expanded. Errors name the field, never its value.
fn importable_servers(report: &mut ScanReport, path: &Path, root: &Path, scope: Scope, servers: Value) -> Value {
    let Value::Object(servers) = servers else { return servers };
    let vars = folder_vars(root, scope);
    let mut kept = serde_json::Map::new();
    for (name, mut server) in servers {
        let unsupported = ["envFile", "auth"].into_iter().find(|key| server.get(key).is_some());
        let result = match unsupported {
            Some("envFile") => Err("`envFile` takes its values from a file, which cannot be imported".to_string()),
            Some(_) => Err("`auth` (OAuth sign-in) cannot be imported".to_string()),
            None => expand(&mut server, &vars),
        };
        match result {
            Ok(()) => {
                kept.insert(name, server);
            }
            Err(e) => report.problem(path, Some(&name), e),
        }
    }
    Value::Object(kept)
}

/// The folder variables Cursor expands in `mcp.json`: VS Code's, so the VS Code adapter uses them too.
pub(super) fn folder_vars(root: &Path, scope: Scope) -> Vec<(&'static str, String)> {
    let sep = std::path::MAIN_SEPARATOR_STR.to_string();
    let mut vars = vec![("${pathSeparator}", sep.clone()), ("${/}", sep)];
    let home = if scope == Scope::Global { Some(root.to_path_buf()) } else { std::env::var_os("HOME").map(PathBuf::from) };
    if let Some(home) = home {
        vars.push(("${userHome}", home.display().to_string()));
    }
    // In the global config the workspace is whatever project Cursor has open.
    if scope == Scope::Project {
        vars.push(("${workspaceFolder}", root.display().to_string()));
        vars.push(("${workspaceFolderBasename}", root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    }
    vars
}

/// Every string of `value` with `vars` replaced. Any other `${…}` (`${env:NAME}` reads the shell
/// Cursor runs in) would reach the server as is.
pub(super) fn expand(value: &mut Value, vars: &[(&str, String)]) -> Result<(), String> {
    match value {
        Value::String(s) => {
            for (var, with) in vars {
                if s.contains(var) {
                    *s = s.replace(var, with);
                }
            }
            if s.contains("${") {
                return Err("uses a `${…}` variable that cannot be expanded on import (`${env:…}` or one tied to the open project)".to_string());
            }
            Ok(())
        }
        Value::Array(items) => items.iter_mut().try_for_each(|v| expand(v, vars)),
        Value::Object(map) => map.values_mut().try_for_each(|v| expand(v, vars)),
        _ => Ok(()),
    }
}

/// An `.mdc` file as a library rule: its body under the frontmatter Claude Code reads.
fn library_rule(mdc: &str) -> String {
    let Some(rest) = mdc.strip_prefix("---") else { return mdc.to_string() };
    let Some(end) = rest.find("\n---") else { return mdc.to_string() };
    let head = &rest[..end];
    let body = rest[end + 4..].split_once('\n').map_or("", |(_, body)| body);
    let field = |key: &str| head.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(':').map(str::trim));
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut out = String::new();
    let description = frontmatter(mdc).description;
    if !description.is_empty() {
        out += &format!("description: {}\n", quoted(&description));
    }
    if field("alwaysApply") != Some("true") {
        let globs = globs(head, field("globs").unwrap_or_default());
        if !globs.is_empty() {
            out += "paths:\n";
            for glob in globs {
                out += &format!("  - {}\n", quoted(&glob));
            }
        }
    }
    if out.is_empty() { body.to_string() } else { format!("---\n{out}---\n{body}") }
}

/// Cursor's `globs`: a comma-separated string (as its docs write it), `[a, b]`, or a YAML list.
/// Read by hand: a glob starting with `*` is a YAML alias, so such frontmatter is not valid YAML.
pub(super) fn globs(head: &str, value: &str) -> Vec<String> {
    let items: Vec<&str> = if value.is_empty() {
        let lines = head.lines().skip_while(|l| !l.starts_with("globs:")).skip(1);
        lines.map(str::trim).map_while(|l| l.strip_prefix("- ")).collect()
    } else {
        value.trim_start_matches('[').trim_end_matches(']').split(',').collect()
    };
    items.iter().map(|g| g.trim().trim_matches('"').trim_matches('\'').to_string()).filter(|g| !g.is_empty()).collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-cursor-{label}-{}", uuid::Uuid::new_v4()));
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

    fn problems(report: &ScanReport) -> Vec<(String, Option<String>, String)> {
        report
            .problems
            .iter()
            .map(|p| (p.path.rsplit('/').next().unwrap().to_string(), p.item.clone(), p.message.clone()))
            .collect()
    }

    fn text(c: &Candidate) -> &str {
        let Payload::Text(text) = &c.payload else { panic!("{c:?}") };
        text
    }

    #[test]
    fn a_project_yields_servers_and_rules_in_the_librarys_form() {
        let base = tmp("project");
        let root = base.join("web app");
        let ctx = ScanContext { library_root: base.join("appdata") };
        write(
            &root.join(".cursor/mcp.json"),
            r#"{"mcpServers": {
                "fs": {"type": "stdio", "command": "npx", "args": ["fs", "${workspaceFolder}${/}src", "${workspaceFolderBasename}"], "env": {"API_KEY": "SECRET-ENV"}},
                "remote": {"url": "https://mcp.example/x", "headers": {"Authorization": "Bearer SECRET-HEADER"}}
            }}"#,
        );
        write(&root.join(".cursor/rules/always.mdc"), "---\ndescription: Always on\nglobs: src/**\nalwaysApply: true\n---\nBe brief.\n");
        write(&root.join(".cursor/rules/tsx.mdc"), "---\nglobs: *.tsx, src/**/*.ts\nalwaysApply: false\n---\n\nUse hooks.");
        write(&root.join(".cursor/rules/frontend/api.mdc"), "---\ndescription: \"API style\"\nglobs:\n  - \"api/**\"\n  - lib/*.ts\n---\nValidate input.");
        write(&root.join(".cursor/rules/smart.mdc"), "---\ndescription: When writing SQL\nalwaysApply: false\n---\nUse CTEs.");
        write(&root.join(".cursor/rules/manual.mdc"), "---\nalwaysApply: false\n---\nOnly on request.");
        write(&root.join(".cursor/rules/bare.mdc"), "No frontmatter.");
        write(&root.join(".cursor/rules/notes.md"), "Cursor ignores this.");
        write(&root.join(".cursorrules"), "Legacy rules.");

        let report = Cursor.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems, []);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:fs".into(), "fs".into()),
                (ImportKind::Mcp, "mcp:remote".into(), "remote".into()),
                (ImportKind::Rule, "rules:.cursor/rules/always.mdc".into(), "always".into()),
                (ImportKind::Rule, "rules:.cursor/rules/bare.mdc".into(), "bare".into()),
                (ImportKind::Rule, "rules:.cursor/rules/frontend/api.mdc".into(), "frontend-api".into()),
                (ImportKind::Rule, "rules:.cursor/rules/manual.mdc".into(), "manual".into()),
                (ImportKind::Rule, "rules:.cursor/rules/smart.mdc".into(), "smart".into()),
                (ImportKind::Rule, "rules:.cursor/rules/tsx.mdc".into(), "tsx".into()),
                (ImportKind::Rule, "rules:.cursorrules".into(), "web-app-cursorrules".into()),
            ]
        );
        let by_id = |id: &str| report.candidates.iter().find(|c| c.id == id).unwrap();
        let Payload::Mcp(fs) = &by_id("mcp:fs").payload else { panic!() };
        let src = format!("{}/src", root.display());
        assert_eq!(fs.args, ["fs", src.as_str(), "web app"]);
        assert_eq!(by_id("mcp:remote").mcp.as_ref().unwrap().header_keys, ["Authorization"]);

        assert_eq!(text(by_id("rules:.cursor/rules/always.mdc")), "---\ndescription: \"Always on\"\n---\nBe brief.\n");
        assert_eq!(text(by_id("rules:.cursor/rules/tsx.mdc")), "---\npaths:\n  - \"*.tsx\"\n  - \"src/**/*.ts\"\n---\n\nUse hooks.");
        assert_eq!(
            text(by_id("rules:.cursor/rules/frontend/api.mdc")),
            "---\ndescription: \"API style\"\npaths:\n  - \"api/**\"\n  - \"lib/*.ts\"\n---\nValidate input."
        );
        assert_eq!(text(by_id("rules:.cursor/rules/smart.mdc")), "---\ndescription: \"When writing SQL\"\n---\nUse CTEs.");
        assert_eq!(text(by_id("rules:.cursor/rules/manual.mdc")), "Only on request.");
        assert_eq!(text(by_id("rules:.cursor/rules/bare.mdc")), "No frontmatter.");
        assert_eq!(text(by_id("rules:.cursorrules")), "Legacy rules.");
        let smart = by_id("rules:.cursor/rules/smart.mdc");
        assert_eq!((smart.description.as_str(), smart.target.as_deref()), ("When writing SQL", Some("both")));
        assert_eq!(smart.hash, crate::commands::rules::content_hash(text(smart)));
        assert_eq!(Cursor.detect(&root, Scope::Project, &ctx), [ImportKind::Mcp, ImportKind::Rule]);

        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("SECRET-"), "{json}");
    }

    #[test]
    fn home_reads_only_its_mcp_json() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("Library/Application Support/io.github.v9n8.puppeteer") };
        write(
            &home.join(".cursor/mcp.json"),
            r#"{"mcpServers": {
                "local": {"command": "${userHome}/bin/mcp", "env": {"TOKEN": "SECRET-ENV"}},
                "env": {"command": "x", "env": {"TOKEN": "${env:SECRET_NAME}"}},
                "file": {"command": "x", "envFile": "SECRET-FILE"},
                "oauth": {"url": "https://x", "auth": {"CLIENT_ID": "SECRET-ID", "CLIENT_SECRET": "SECRET-CLIENT"}},
                "workspace": {"command": "x", "args": ["${workspaceFolder}"]},
                "bad": {"command": "x", "args": "SECRET-ARGS"}
            }}"#,
        );
        // Neither rules nor a legacy file at HOME: Cursor keeps User Rules in its settings.
        write(&home.join(".cursor/rules/x.mdc"), "x");
        write(&home.join(".cursorrules"), "x");

        let report = Cursor.scan(&home, Scope::Global, &ctx);
        assert_eq!(ids(&report), [(ImportKind::Mcp, "mcp:local".into(), "local".into())]);
        let Payload::Mcp(local) = &report.candidates[0].payload else { panic!() };
        assert_eq!(local.command.as_deref(), Some(format!("{}/bin/mcp", home.display()).as_str()));
        let variable = "uses a `${…}` variable that cannot be expanded on import (`${env:…}` or one tied to the open project)";
        let mut got = problems(&report);
        got.sort();
        assert_eq!(
            got,
            [
                ("mcp.json".into(), Some("bad".into()), "`args` is not a list of strings".into()),
                ("mcp.json".into(), Some("env".into()), variable.into()),
                ("mcp.json".into(), Some("file".into()), "`envFile` takes its values from a file, which cannot be imported".into()),
                ("mcp.json".into(), Some("oauth".into()), "`auth` (OAuth sign-in) cannot be imported".into()),
                ("mcp.json".into(), Some("workspace".into()), variable.into()),
            ]
        );
        assert_eq!(Cursor.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
    }

    #[test]
    fn a_broken_file_is_reported_without_quoting_it_and_the_rest_still_scanned() {
        let root = tmp("broken");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".cursor/mcp.json"), "{\"mcpServers\": {\"a\": {\"command\": \"SECRET-UNTERMINATED}}");
        write(&root.join(".cursor/rules/latin1.mdc"), [0x66, 0xe9]);
        write(&root.join(".cursor/rules/big.mdc"), vec![b'a'; MAX_TEXT_BYTES as usize + 1]);
        write(&root.join(".cursor/rules/ok.mdc"), "---\nglobs:\n---\nok");
        write(&root.join(".cursorrules"), [0xff, 0xfe]);

        let report = Cursor.scan(&root, Scope::Project, &ctx);
        assert_eq!(ids(&report), [(ImportKind::Rule, "rules:.cursor/rules/ok.mdc".into(), "ok".into())]);
        assert_eq!(text(&report.candidates[0]), "ok");
        let got: Vec<_> = problems(&report).into_iter().map(|(file, _, msg)| (file, msg.split(' ').take(2).collect::<Vec<_>>().join(" "))).collect();
        assert_eq!(
            got,
            [
                ("mcp.json".to_string(), "invalid JSON".to_string()),
                ("big.mdc".to_string(), "file too".to_string()),
                ("latin1.mdc".to_string(), "not UTF-8".to_string()),
                (".cursorrules".to_string(), "not UTF-8".to_string()),
            ]
        );
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));

        // `mcpServers` of the wrong shape, and a config without it.
        write(&root.join(".cursor/mcp.json"), r#"{"mcpServers": ["SECRET-SHAPE"]}"#);
        let report = Cursor.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems[0].message, "`mcpServers` is not an object");
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
        write(&root.join(".cursor/mcp.json"), r#"{"other": 1}"#);
        assert!(Cursor.scan(&root, Scope::Project, &ctx).problems.iter().all(|p| !p.path.ends_with("mcp.json")));
    }

    #[test]
    fn same_servers_hash_alike_from_claude_and_cursor() {
        let root = tmp("hash");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".cursor/mcp.json"), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["fs"], "env": {"K": "cursor value"}}}}"#);
        write(&root.join(".mcp.json"), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["fs"], "env": {"K": "claude value"}}}}"#);
        let cursor = Cursor.scan(&root, Scope::Project, &ctx);
        let claude = super::super::claude_code::ClaudeCode.scan(&root, Scope::Project, &ctx);
        assert_eq!(cursor.candidates[0].hash, claude.candidates[0].hash);
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(Cursor.detect(&root, scope, &ctx), []);
            let report = Cursor.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
    }
}
