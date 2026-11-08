//! Windsurf (the editor, now documented on docs.devin.ai). Paths and formats from
//! docs.devin.ai/windsurf/plugins/cascade/mcp, docs.devin.ai/desktop/cascade/memories and
//! docs.devin.ai/cli/reference/configuration/read-config-from (read 2026-11-08; docs.windsurf.com
//! redirects there):
//!
//! - Global (HOME): the stable build's `~/.codeium/windsurf/mcp_config.json` (`mcpServers`) and
//!   skills `~/.codeium/windsurf/skills`.
//! - Project: the rules `.windsurf/rules/*.md` and the legacy `.windsurfrules`, the skills
//!   `.windsurf/skills`.
//!
//! Not imported: the Next and Insiders builds' folders (`~/.codeium/windsurf-next`…), Devin's own
//! `~/.config/devin/mcp_config.json`, global rules (the docs name no Windsurf path for them) and
//! workflows.
//!
//! A server is `command`/`args`/`env`, or `serverUrl` (or `url`) with `headers`, over HTTP. One
//! taking a value from the environment (`${env:…}`, expanded in all those fields) or with tools
//! turned off (`disabledTools`) is left out and reported; a `disabled` one comes in disabled.
//! A rule's `trigger: glob` becomes `paths`; a rule Windsurf only loads when relevant or when
//! asked for comes in always on, as every library rule is.

use super::cursor::globs;
use super::*;
use serde::Deserialize;

pub struct Windsurf;

/// Windsurf's rules and skills are for no engine in particular: the library's default.
const TARGET: &str = "both";

struct Layout {
    /// The MCP config, global only.
    mcp: Option<PathBuf>,
    /// The rules folder and the legacy `.windsurfrules` with the name it is proposed under, project only.
    rules: Option<(PathBuf, PathBuf, String)>,
    skills: PathBuf,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    match scope {
        Scope::Global => {
            let dir = root.join(".codeium").join("windsurf");
            Layout { mcp: Some(dir.join("mcp_config.json")), rules: None, skills: dir.join("skills") }
        }
        Scope::Project => {
            let dot = root.join(".windsurf");
            let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            Layout {
                mcp: None,
                rules: Some((dot.join("rules"), root.join(".windsurfrules"), format!("{folder}-windsurfrules"))),
                skills: dot.join("skills"),
            }
        }
    }
}

/// Only `mcpServers` is read; serde skips every other key without keeping it.
#[derive(Deserialize)]
struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: Option<Value>,
}

impl Adapter for Windsurf {
    fn id(&self) -> &'static str {
        "windsurf"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The config may hold no server: telling would mean parsing it.
        if l.mcp.is_some_and(|p| p.is_file()) {
            kinds.push(ImportKind::Mcp);
        }
        if l.rules.is_some_and(|(dir, legacy, _)| (legacy.is_file() && !is_own_link(&legacy, ctx)) || !md_files(&dir, ctx).is_empty()) {
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
        if let Some(path) = l.mcp.as_ref().filter(|p| p.is_file()) {
            let config =
                read_text(path, MAX_CONFIG_BYTES).and_then(|raw| serde_json::from_str::<McpConfig>(&raw).map_err(|e| json_error(&e)));
            match config {
                Ok(McpConfig { mcp_servers: Some(servers) }) => {
                    let disabled = disabled_servers(&servers);
                    let servers = normalized_servers(&mut report, path, servers, server);
                    mcp_candidates(&mut report, path, &servers);
                    for c in &mut report.candidates {
                        if let Payload::Mcp(export) = &mut c.payload {
                            export.enabled = !disabled.iter().any(|name| c.id == format!("mcp:{name}"));
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => report.problem(path, None, e),
            }
        }
        let mut found = Vec::new();
        if let Some((dir, legacy, legacy_name)) = &l.rules {
            for path in md_files(dir, ctx) {
                if let Some(mut c) = text_candidate(&mut report, root, ImportKind::Rule, &path, None, Some(TARGET)) {
                    let Payload::Text(text) = &c.payload else { unreachable!("a text candidate") };
                    let rule = library_rule(text);
                    c.hash = crate::commands::rules::content_hash(&rule);
                    c.payload = Payload::Text(rule);
                    found.push(c);
                }
            }
            if legacy.is_file() && !is_own_link(legacy, ctx) {
                found.extend(text_candidate(&mut report, root, ImportKind::Rule, legacy, Some(legacy_name.clone()), Some(TARGET)));
            }
        }
        for dir in skill_dirs(&l.skills, ctx) {
            found.extend(skill_candidate(&mut report, root, &dir, TARGET));
        }
        report.candidates.extend(found);
        report
    }
}

/// The names of the servers with `disabled: true`.
fn disabled_servers(servers: &Value) -> Vec<String> {
    let Some(servers) = servers.as_object() else { return Vec::new() };
    servers.iter().filter(|(_, s)| s.get("disabled") == Some(&Value::Bool(true))).map(|(name, _)| name.clone()).collect()
}

/// One Windsurf server in the `mcpServers` shape, or why Puppeteer cannot run it as Windsurf
/// would. Errors name the field, never its value.
fn server(mut server: Value) -> Result<Value, String> {
    let Some(obj) = server.as_object_mut() else { return Ok(server) };
    if obj.get("disabledTools").and_then(Value::as_array).is_some_and(|tools| !tools.is_empty()) {
        return Err("`disabledTools` turns some of its tools off, which cannot be imported".to_string());
    }
    if reads_environment(&Value::Object(obj.clone()), false) {
        return Err("takes a value from the environment (`${env:…}`), which cannot be imported".to_string());
    }
    if let Some(url) = obj.remove("serverUrl").filter(|v| !v.is_null()) {
        obj.insert("url".into(), url);
    }
    Ok(server)
}

/// A Windsurf rule as a library rule: with `trigger: glob` its `globs` become `paths`,
/// `description` stays, the rest is dropped.
fn library_rule(content: &str) -> String {
    let Some((head, body)) = split_frontmatter(content) else { return content.to_string() };
    let paths = if frontmatter_field(head, "trigger") == Some("glob") {
        globs(head, frontmatter_field(head, "globs").unwrap_or_default())
    } else {
        Vec::new()
    };
    library_rule_text(&frontmatter(content).description, &paths, body)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-windsurf-{label}-{}", uuid::Uuid::new_v4()));
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

    fn text<'a>(report: &'a ScanReport, id: &str) -> &'a str {
        let c = report.candidates.iter().find(|c| c.id == id).unwrap();
        let Payload::Text(text) = &c.payload else { panic!("{c:?}") };
        text
    }

    #[test]
    fn home_yields_servers_and_skills() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("appdata") };
        write(
            &home.join(".codeium/windsurf/mcp_config.json"),
            r#"{"mcpServers": {
                "github": {"command": "npx", "args": ["-y", "gh"], "env": {"GITHUB_TOKEN": "SECRET-ENV"}},
                "remote": {"serverUrl": "https://mcp.example/mcp", "headers": {"API_KEY": "SECRET-HEADER"}},
                "plain": {"url": "https://mcp.example/x"},
                "off": {"command": "off-mcp", "disabled": true, "disabledTools": []},
                "env": {"serverUrl": "https://x", "headers": {"Authorization": "Bearer ${env:SECRET_NAME}"}},
                "picky": {"command": "x", "disabledTools": ["delete"]},
                "bad": {"command": "x", "args": "SECRET-ARGS"}
            }}"#,
        );
        write(&home.join(".codeium/windsurf/skills/deploy/SKILL.md"), "---\nname: deploy\n---\nDeploy.");
        // Not Windsurf's global rules.
        write(&home.join(".windsurf/rules/x.md"), "x");

        let report = Windsurf.scan(&home, Scope::Global, &ctx);
        let mut got = ids(&report);
        got.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(
            got,
            [
                (ImportKind::Mcp, "mcp:github".into(), "github".into()),
                (ImportKind::Mcp, "mcp:off".into(), "off".into()),
                (ImportKind::Mcp, "mcp:plain".into(), "plain".into()),
                (ImportKind::Mcp, "mcp:remote".into(), "remote".into()),
                (ImportKind::Skill, "skills:.codeium/windsurf/skills/deploy".into(), "deploy".into()),
            ]
        );
        let export = |id: &str| {
            let Payload::Mcp(e) = &report.candidates.iter().find(|c| c.id == id).unwrap().payload else { panic!() };
            e.clone()
        };
        let remote = export("mcp:remote");
        assert_eq!((remote.transport.as_str(), remote.url.as_deref()), ("http", Some("https://mcp.example/mcp")));
        assert!(!export("mcp:off").enabled && export("mcp:github").enabled);
        let mut problems: Vec<_> = report.problems.iter().map(|p| (p.item.clone().unwrap(), p.message.as_str())).collect();
        problems.sort();
        assert_eq!(
            problems,
            [
                ("bad".into(), "`args` is not a list of strings"),
                ("env".into(), "takes a value from the environment (`${env:…}`), which cannot be imported"),
                ("picky".into(), "`disabledTools` turns some of its tools off, which cannot be imported"),
            ]
        );
        assert_eq!(Windsurf.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp, ImportKind::Skill]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
    }

    #[test]
    fn a_project_yields_rules_in_the_librarys_form() {
        let base = tmp("project");
        let root = base.join("web app");
        let ctx = ScanContext { library_root: base.join("appdata") };
        write(&root.join(".windsurf/rules/tests.md"), "---\ntrigger: glob\nglobs: \"**/*.test.ts\"\n---\nMock the API.");
        write(&root.join(".windsurf/rules/always.md"), "---\ntrigger: always_on\nglobs: src/**\n---\nBe brief.");
        write(&root.join(".windsurf/rules/sql.md"), "---\ntrigger: model_decision\ndescription: When writing SQL\n---\nUse CTEs.");
        write(&root.join(".windsurf/rules/bare.md"), "No frontmatter.");
        write(&root.join(".windsurfrules"), "Legacy rules.");
        write(&root.join(".windsurf/skills/lint/SKILL.md"), "Lint.");
        // Only HOME has an MCP config.
        write(&root.join(".codeium/windsurf/mcp_config.json"), r#"{"mcpServers": {"x": {"command": "x"}}}"#);

        let report = Windsurf.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems, []);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Rule, "rules:.windsurf/rules/always.md".into(), "always".into()),
                (ImportKind::Rule, "rules:.windsurf/rules/bare.md".into(), "bare".into()),
                (ImportKind::Rule, "rules:.windsurf/rules/sql.md".into(), "sql".into()),
                (ImportKind::Rule, "rules:.windsurf/rules/tests.md".into(), "tests".into()),
                (ImportKind::Rule, "rules:.windsurfrules".into(), "web-app-windsurfrules".into()),
                (ImportKind::Skill, "skills:.windsurf/skills/lint".into(), "lint".into()),
            ]
        );
        assert_eq!(text(&report, "rules:.windsurf/rules/tests.md"), "---\npaths:\n  - \"**/*.test.ts\"\n---\nMock the API.");
        assert_eq!(text(&report, "rules:.windsurf/rules/always.md"), "Be brief.");
        assert_eq!(text(&report, "rules:.windsurf/rules/sql.md"), "---\ndescription: \"When writing SQL\"\n---\nUse CTEs.");
        assert_eq!(text(&report, "rules:.windsurf/rules/bare.md"), "No frontmatter.");
        assert_eq!(Windsurf.detect(&root, Scope::Project, &ctx), [ImportKind::Rule, ImportKind::Skill]);
    }

    #[test]
    fn a_broken_file_is_reported_without_quoting_it() {
        let home = tmp("broken");
        let ctx = ScanContext { library_root: home.join("appdata") };
        let path = home.join(".codeium/windsurf/mcp_config.json");
        write(&path, "{\"mcpServers\": {\"a\": {\"command\": \"SECRET-UNTERMINATED}}");
        let report = Windsurf.scan(&home, Scope::Global, &ctx);
        assert!(report.candidates.is_empty());
        assert!(report.problems[0].message.starts_with("invalid JSON at line"), "{:?}", report.problems);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
        write(&path, r#"{"mcpServers": ["SECRET-SHAPE"]}"#);
        let report = Windsurf.scan(&home, Scope::Global, &ctx);
        assert_eq!(report.problems[0].message, "`mcpServers` is not an object");
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(Windsurf.detect(&root, scope, &ctx), []);
            let report = Windsurf.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
    }
}
