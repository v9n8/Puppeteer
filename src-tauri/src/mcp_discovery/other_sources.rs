//! S9: the MCP servers of other apps (Claude Desktop, Cursor, VS Code, Windsurf, Gemini, …), in HOME
//! and in each project, through the import adapters. Neither engine loads them; they are listed so
//! the user can import one.
//!
//! Only adapters whose cheap check finds an MCP config are scanned, and only their MCP findings
//! are kept: a server, a bad server entry, or a JSON config file that could not be read (an
//! adapter's other files are Markdown rules and skills).

use super::{DiscoveryInputs, DiscoveryScope, Origin, Scan, SourceId};
use crate::import::{ImportKind, ScanReport, Scope, ADAPTERS};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// The engines' own configs, read by `claude_sources` / `codex_sources`.
const ENGINE_ADAPTERS: &[&str] = &["claude_code", "codex"];

/// The MCP part of `report`, by the file each finding comes from.
fn by_file(report: ScanReport) -> BTreeMap<PathBuf, ScanReport> {
    let mut out: BTreeMap<PathBuf, ScanReport> = BTreeMap::new();
    for candidate in report.candidates.into_iter().filter(|c| c.kind == ImportKind::Mcp) {
        out.entry(PathBuf::from(&candidate.origin)).or_default().candidates.push(candidate);
    }
    for problem in report.problems {
        let config_file = Path::new(&problem.path).extension().is_some_and(|x| x == "json");
        if problem.item.is_some() || config_file {
            out.entry(PathBuf::from(&problem.path)).or_default().problems.push(problem);
        }
    }
    out
}

pub fn scan(inputs: &DiscoveryInputs) -> Scan {
    let roots = std::iter::once((inputs.home.as_path(), Scope::Global, None))
        .chain(inputs.projects.iter().map(|p| (p.path.as_path(), Scope::Project, Some(p.id.as_str()))));
    let mut scan = Scan::default();
    let mut ids = HashSet::new();
    for (root, scope, project_id) in roots {
        for adapter in ADAPTERS.iter().filter(|a| !ENGINE_ADAPTERS.contains(&a.id())) {
            if !adapter.detect(root, scope, &inputs.ctx).contains(&ImportKind::Mcp) {
                continue;
            }
            for (path, report) in by_file(adapter.scan(root, scope, &inputs.ctx)) {
                let origin = Origin {
                    source: SourceId::S9,
                    scope: if scope == Scope::Global { DiscoveryScope::Global } else { DiscoveryScope::Project },
                    path: &path,
                    local_key: None,
                    profile: None,
                    project_id,
                    app: Some(adapter.id()),
                    importable: true,
                };
                let mut found = origin.rows(inputs, report);
                // Two adapters reading one file list its servers once.
                found.found.retain(|f| ids.insert(f.row.id.clone()));
                scan.extend(found);
            }
        }
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::super::claude_sources::tests::{inputs, rows, tmp, write};
    use super::super::ProjectDir;
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn other_apps_servers_are_listed_with_their_app() {
        let base = tmp("other");
        let mut inputs = inputs(&base);
        write(
            &inputs.home.join(".cursor/mcp.json"),
            r#"{"mcpServers": {"cur": {"command": "c", "env": {"K": "SECRET-CURSOR"}}, "bad": {"command": 1}}}"#,
        );
        write(&inputs.home.join(".cursor/rules/style.mdc"), "Use tabs.");
        // Claude Code and Codex configs are not S9.
        write(&inputs.home.join(".claude.json"), r#"{"mcpServers": {"engine": {"command": "e"}}}"#);
        let project = base.join("work/site");
        write(&project.join(".cursor/mcp.json"), r#"{"mcpServers": {"proj": {"url": "https://p", "headers": {"A": "SECRET-H"}}}}"#);
        inputs.projects.push(ProjectDir { id: "p1".into(), path: project });

        let report = scan(&inputs).into_report();
        assert_eq!(
            rows(&report),
            [
                ("S9", None, None, "bad".into(), "unknown".into()),
                ("S9", None, None, "cur".into(), "stdio".into()),
                ("S9", None, s("p1"), "proj".into(), "http".into()),
            ]
        );
        assert!(report.servers.iter().all(|r| r.app.as_deref() == Some("cursor")));
        let proj = report.servers.iter().find(|r| r.name == "proj").unwrap();
        assert_eq!(proj.scope, DiscoveryScope::Project);
        assert_eq!(proj.summary.as_ref().unwrap().header_keys, ["A"]);
        assert_eq!(report.servers.iter().find(|r| r.name == "cur").unwrap().origin_label, "~/.cursor/mcp.json");
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        let shown = format!("{}{report:?}", serde_json::to_string(&report).unwrap());
        assert!(!shown.contains("SECRET-"), "{shown}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unreadable_app_config_is_a_problem_but_a_bad_rule_file_is_not() {
        let base = tmp("other-broken");
        let inputs = inputs(&base);
        write(&inputs.home.join(".cursor/mcp.json"), r#"{"mcpServers": {"SECRET-NAME": "#);
        std::fs::create_dir_all(inputs.home.join(".cursor/rules")).unwrap();
        std::fs::write(inputs.home.join(".cursor/rules/latin1.mdc"), [0x66, 0xe9]).unwrap();
        let report = scan(&inputs).into_report();
        assert!(report.servers.is_empty());
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        let problem = &report.problems[0];
        assert_eq!((problem.source, problem.app.as_deref()), (SourceId::S9, Some("cursor")));
        assert!(problem.message.starts_with("invalid JSON"), "{problem:?}");
        assert!(!format!("{report:?}").contains("SECRET-"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn every_file_source_comes_together() {
        let base = tmp("all-files");
        let mut inputs = inputs(&base);
        write(&inputs.home.join(".claude.json"), r#"{"mcpServers": {"a": {"command": "a"}}}"#);
        write(&inputs.home.join(".codex/config.toml"), "[mcp_servers.b]\ncommand = \"b\"\n");
        write(&inputs.home.join(".cursor/mcp.json"), r#"{"mcpServers": {"c": {"command": "c"}}}"#);
        let project = base.join("work/x");
        write(&project.join(".mcp.json"), r#"{"mcpServers": {"d": {"command": "d"}}}"#);
        write(&project.join(".codex/config.toml"), "[mcp_servers.e]\ncommand = \"e\"\n");
        inputs.projects.push(ProjectDir { id: "p1".into(), path: project });
        let report = super::super::scan_files(&inputs).into_report();
        let got: Vec<_> = rows(&report).into_iter().map(|(source, _, project, name, _)| (source, project, name)).collect();
        assert_eq!(
            got,
            [
                ("S2", None, "a".into()),
                ("S4", s("p1"), "d".into()),
                ("S7", None, "b".into()),
                ("S8", s("p1"), "e".into()),
                ("S9", None, "c".into()),
            ]
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
