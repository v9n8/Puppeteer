//! Claude Code (the `claude` CLI).
//!
//! - Global (HOME): `~/.claude.json` — only its top-level `mcpServers` (user scope); the file also
//!   holds the signed-in account, so no other key is even deserialized — then `~/.claude/CLAUDE.md`
//!   and `~/.claude/{rules,skills,agents,commands}`.
//! - Project: `.mcp.json`, `CLAUDE.md` and `.claude/CLAUDE.md`, `.claude/{rules,skills,agents,commands}`.
//!
//! CLAUDE.md files and `.claude/rules/*.md` become rules. Only files directly in each folder are
//! read; links into Puppeteer's app data (what the app installed) are left out.

use super::*;
use serde::Deserialize;

pub struct ClaudeCode;

/// Skills and rules from Claude Code are meant for Claude.
const TARGET: &str = "claude";

struct Layout {
    mcp: PathBuf,
    /// CLAUDE.md files with the rule name each one is proposed under.
    claude_mds: Vec<(PathBuf, String)>,
    dot: PathBuf,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    match scope {
        Scope::Global => Layout {
            mcp: root.join(".claude.json"),
            claude_mds: vec![(root.join(".claude").join("CLAUDE.md"), "global-claude-md".to_string())],
            dot: root.join(".claude"),
        },
        Scope::Project => {
            let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            Layout {
                mcp: root.join(".mcp.json"),
                claude_mds: vec![
                    (root.join("CLAUDE.md"), format!("{folder}-claude-md")),
                    (root.join(".claude").join("CLAUDE.md"), format!("{folder}-dot-claude-md")),
                ],
                dot: root.join(".claude"),
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

impl Adapter for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude_code"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The config file may hold no server: telling would mean parsing it.
        if l.mcp.is_file() {
            kinds.push(ImportKind::Mcp);
        }
        if l.claude_mds.iter().any(|(p, _)| p.is_file() && !is_own_link(p, ctx)) || !md_files(&l.dot.join("rules"), ctx).is_empty() {
            kinds.push(ImportKind::Rule);
        }
        if !skill_dirs(&l.dot.join("skills"), ctx).is_empty() {
            kinds.push(ImportKind::Skill);
        }
        if !md_files(&l.dot.join("agents"), ctx).is_empty() {
            kinds.push(ImportKind::Agent);
        }
        if !md_files(&l.dot.join("commands"), ctx).is_empty() {
            kinds.push(ImportKind::Command);
        }
        kinds
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let l = layout(root, scope);
        let mut report = ScanReport::default();
        if l.mcp.is_file() {
            let config = read_config(&l.mcp, root, scope, ctx)
                .and_then(|raw| serde_json::from_str::<McpConfig>(&raw).map_err(|e| json_error(&e)));
            match config {
                Ok(McpConfig { mcp_servers: Some(servers) }) => mcp_candidates(&mut report, &l.mcp, &servers),
                Ok(_) => {}
                Err(e) => report.problem(&l.mcp, None, e),
            }
        }
        let mut found = Vec::new();
        for (path, name) in &l.claude_mds {
            if path.is_file() && !is_own_link(path, ctx) {
                found.extend(text_candidate(&mut report, root, scope, ctx, ImportKind::Rule, path, Some(name.clone()), Some(TARGET)));
            }
        }
        for path in md_files(&l.dot.join("rules"), ctx) {
            found.extend(text_candidate(&mut report, root, scope, ctx, ImportKind::Rule, &path, None, Some(TARGET)));
        }
        for dir in skill_dirs(&l.dot.join("skills"), ctx) {
            found.extend(skill_candidate(&mut report, root, scope, ctx, &dir, TARGET));
        }
        for (folder, kind) in [("agents", ImportKind::Agent), ("commands", ImportKind::Command)] {
            for path in md_files(&l.dot.join(folder), ctx) {
                found.extend(text_candidate(&mut report, root, scope, ctx, kind, &path, None, None));
            }
        }
        report.candidates.extend(found);
        report
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-cc-{label}-{}", uuid::Uuid::new_v4()));
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

    #[test]
    fn a_project_yields_one_candidate_per_item_and_skips_the_apps_own_links() {
        let base = tmp("project");
        let root = base.join("my app");
        let lib = base.join("appdata");
        write(&root.join(".mcp.json"), r#"{"mcpServers": {"gh.tools": {"command": "npx", "args": ["gh"], "env": {"GH_TOKEN": "v"}}}}"#);
        write(&root.join("CLAUDE.md"), "Be brief.");
        write(&root.join(".claude/rules/style.md"), "---\ndescription: Style\npaths: src/**\n---\nUse tabs.");
        write(&root.join(".claude/rules/notes.txt"), "not markdown");
        write(&root.join(".claude/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\ntarget: both\n---\nbody");
        write(&root.join(".claude/skills/pdf/ref/forms.md"), "forms");
        write(&root.join(".claude/skills/no-skill-md/README.md"), "x");
        write(&root.join(".claude/agents/reviewer.md"), "---\nname: code-reviewer\ndescription: Reviews\n---\nReview.");
        write(&root.join(".claude/commands/deploy.md"), "Deploy $ARGUMENTS");
        // What Puppeteer installed: links into its app data.
        write(&lib.join("skills/own/SKILL.md"), "---\nname: own\n---\n");
        write(&lib.join("agents/mine.md"), "---\nname: mine\n---\n");
        std::os::unix::fs::symlink(lib.join("skills/own"), root.join(".claude/skills/own")).unwrap();
        std::os::unix::fs::symlink(lib.join("agents/mine.md"), root.join(".claude/agents/mine.md")).unwrap();

        let ctx = ScanContext { library_root: lib };
        let report = ClaudeCode.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems, []);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:gh.tools".into(), "gh-tools".into()),
                (ImportKind::Rule, "rules:CLAUDE.md".into(), "my-app-claude-md".into()),
                (ImportKind::Rule, "rules:.claude/rules/style.md".into(), "style".into()),
                (ImportKind::Skill, "skills:.claude/skills/pdf".into(), "pdf".into()),
                (ImportKind::Agent, "agents:.claude/agents/reviewer.md".into(), "code-reviewer".into()),
                (ImportKind::Command, "commands:.claude/commands/deploy.md".into(), "deploy".into()),
            ]
        );
        let by_id = |id: &str| report.candidates.iter().find(|c| c.id == id).unwrap();
        let mcp = by_id("mcp:gh.tools").mcp.as_ref().unwrap();
        assert_eq!((mcp.transport.as_str(), mcp.env_keys.clone()), ("stdio", vec!["GH_TOKEN".to_string()]));
        assert_eq!((mcp.command.as_deref(), mcp.args.clone()), (Some("npx"), vec!["gh".to_string()]));
        assert_eq!(by_id("rules:.claude/rules/style.md").description, "Style");
        assert_eq!(by_id("rules:CLAUDE.md").target.as_deref(), Some("claude"));
        assert_eq!(by_id("skills:.claude/skills/pdf").target.as_deref(), Some("both"));
        assert_eq!(by_id("commands:.claude/commands/deploy.md").description, "deploy");
        assert_eq!(
            ClaudeCode.detect(&root, Scope::Project, &ctx),
            [ImportKind::Mcp, ImportKind::Rule, ImportKind::Skill, ImportKind::Agent, ImportKind::Command]
        );
    }

    #[test]
    fn a_broken_item_is_reported_and_the_rest_still_scanned() {
        let root = tmp("broken");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".mcp.json"), "{\"mcpServers\": {\"ok\": {\"url\": \"https://x\"}, ");
        write(&root.join(".claude/rules/latin1.md"), [0x66, 0xe9, 0x65]);
        write(&root.join(".claude/rules/big.md"), vec![b'a'; MAX_TEXT_BYTES as usize + 1]);
        write(&root.join(".claude/rules/fine.md"), "fine");
        write(&root.join(".claude/skills/bad/SKILL.md"), [0xff, 0xfe]);
        write(&root.join(".claude/agents/a.md"), "---\ndescription: A\n---\n");

        let report = ClaudeCode.scan(&root, Scope::Project, &ctx);
        let names: Vec<_> = report.candidates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["fine", "a"]);
        let problems: Vec<_> = report
            .problems
            .iter()
            .map(|p| (p.path.rsplit('/').next().unwrap().to_string(), p.message.split(' ').take(2).collect::<Vec<_>>().join(" ")))
            .collect();
        assert_eq!(
            problems,
            [
                (".mcp.json".to_string(), "invalid JSON".to_string()),
                ("big.md".to_string(), "file too".to_string()),
                ("latin1.md".to_string(), "not UTF-8".to_string()),
                ("SKILL.md".to_string(), "not UTF-8".to_string()),
            ]
        );
    }

    #[test]
    fn one_bad_mcp_server_does_not_drop_the_others() {
        let root = tmp("mcp");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(
            &root.join(".mcp.json"),
            r#"{"mcpServers": {"good": {"type": "http", "url": "https://x", "headers": {"Authorization": "Bearer SECRET-H"}},
                "bad": {"command": "x", "env": {"TOKEN": 1}}, "odd": {"type": "ws", "url": "wss://x"}}}"#,
        );
        let report = ClaudeCode.scan(&root, Scope::Project, &ctx);
        assert_eq!(ids(&report), [(ImportKind::Mcp, "mcp:good".into(), "good".into())]);
        assert_eq!(report.candidates[0].mcp.as_ref().unwrap().header_keys, ["Authorization"]);
        let problems: Vec<_> = report.problems.iter().map(|p| (p.item.clone().unwrap(), p.message.clone())).collect();
        assert_eq!(
            problems,
            [
                ("bad".to_string(), "`env.TOKEN` is not a string".to_string()),
                ("odd".to_string(), "unsupported `type` (stdio, http and sse only)".to_string()),
            ]
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("SECRET-H"), "{json}");
    }

    #[test]
    fn home_reads_only_mcp_servers_of_claude_json() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("Library/Application Support/io.github.v9n8.puppeteer") };
        write(
            &home.join(".claude.json"),
            r#"{"oauthAccount": {"emailAddress": "ACCOUNT-EMAIL", "accountUuid": "ACCOUNT-UUID"}, "primaryApiKey": "ACCOUNT-KEY",
                "projects": {"/p": {"mcpServers": {"local": {"command": "l"}}}},
                "mcpServers": {"fs": {"command": "npx", "args": ["fs"]}}}"#,
        );
        write(&home.join(".claude/CLAUDE.md"), "Global.");
        write(&home.join(".claude/commands/hi.md"), "---\ndescription: Say hi\n---\nhi");

        let report = ClaudeCode.scan(&home, Scope::Global, &ctx);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:fs".into(), "fs".into()),
                (ImportKind::Rule, "rules:.claude/CLAUDE.md".into(), "global-claude-md".into()),
                (ImportKind::Command, "commands:.claude/commands/hi.md".into(), "hi".into()),
            ]
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("ACCOUNT-"), "{json}");

        // Not an object at the top: the error does not quote the file either.
        write(&home.join(".claude.json"), r#""ACCOUNT-TOP""#);
        let report = ClaudeCode.scan(&home, Scope::Global, &ctx);
        assert!(report.problems[0].message.starts_with("invalid JSON at line 1"), "{:?}", report.problems);
        assert!(!serde_json::to_string(&report).unwrap().contains("ACCOUNT-"));
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        assert_eq!(ClaudeCode.detect(&root, Scope::Project, &ctx), []);
        let report = ClaudeCode.scan(&root, Scope::Project, &ctx);
        assert!(report.candidates.is_empty() && report.problems.is_empty());
    }
}
