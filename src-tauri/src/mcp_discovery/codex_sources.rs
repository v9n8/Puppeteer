//! Codex's file sources: S7 the `config.toml` of each Codex home, S8 a project's
//! `.codex/config.toml`.
//!
//! A Codex run reads `config.toml` from its `CODEX_HOME`: an app account's home, else the global one
//! (`CODEX_HOME` as Puppeteer started, else `~/.codex`). `~/.codex` is listed even when no run reads
//! it. An account home links `config.toml` to the global one (`codex_accounts` shared entries), so a
//! file is read once, by its real path, under the first profile. Codex loads a project's
//! `.codex/config.toml` only for a trusted project; it is listed either way, since trust can change
//! outside Puppeteer.

use super::claude_sources::GLOBAL_PROFILE;
use super::{DiscoveryInputs, DiscoveryScope, Origin, Scan, SourceId};
use crate::import::codex::scan_config_mcp;
use crate::import::Scope;
use std::collections::HashSet;
use std::path::PathBuf;

/// `(Codex home, profile)` for every Codex home.
fn homes(inputs: &DiscoveryInputs) -> Vec<(PathBuf, Option<String>)> {
    let default = inputs.home.join(".codex");
    let mut homes = match &inputs.codex_home {
        Some(dir) if *dir != default => vec![(dir.clone(), Some(GLOBAL_PROFILE.to_string())), (default, None)],
        _ => vec![(default, Some(GLOBAL_PROFILE.to_string()))],
    };
    homes.extend(inputs.codex_accounts.iter().map(|a| (a.config_dir.clone(), Some(a.profile.clone()))));
    homes
}

pub fn scan(inputs: &DiscoveryInputs) -> Scan {
    let mut scan = Scan::default();
    let mut seen = HashSet::new();
    for (home, profile) in homes(inputs) {
        let path = home.join("config.toml");
        if !path.is_file() || !seen.insert(std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone())) {
            continue;
        }
        let origin = Origin {
            source: SourceId::S7,
            scope: DiscoveryScope::Global,
            path: &path,
            local_key: None,
            profile: profile.as_deref(),
            project_id: None,
            app: None,
            importable: true,
        };
        match scan_config_mcp(&path, &home, Scope::Global, &inputs.ctx) {
            Ok(report) => scan.extend(origin.rows(inputs, report)),
            Err(e) => scan.problems.push(origin.problem(inputs, e)),
        }
    }

    for project in &inputs.projects {
        let path = project.path.join(".codex").join("config.toml");
        if !path.is_file() {
            continue;
        }
        let origin = Origin {
            source: SourceId::S8,
            scope: DiscoveryScope::Project,
            path: &path,
            local_key: None,
            profile: None,
            project_id: Some(&project.id),
            app: None,
            importable: true,
        };
        match scan_config_mcp(&path, &project.path, Scope::Project, &inputs.ctx) {
            Ok(report) => scan.extend(origin.rows(inputs, report)),
            Err(e) => scan.problems.push(origin.problem(inputs, e)),
        }
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::super::claude_sources::tests::{inputs, rows, tmp, write};
    use super::super::{ProfileDir, ProjectDir};
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    const CONFIG: &str = "model = \"gpt-5\"\n[model_providers.corp]\nenv_key = \"SECRET-PROVIDER\"\n\
        [mcp_servers.docs]\ncommand = \"npx\"\nenv = { API_KEY = \"SECRET-ENV\" }\n\
        [mcp_servers.remote]\nurl = \"https://r\"\nhttp_headers = { Authorization = \"SECRET-HEADER\" }\n";

    #[cfg(unix)]
    #[test]
    fn an_account_home_linked_to_the_global_config_is_read_once() {
        let base = tmp("codex-link");
        let mut inputs = inputs(&base);
        let global = inputs.home.join(".codex");
        write(&global.join("config.toml"), CONFIG);
        // Like `codex_accounts`: the account home links the shared `config.toml`.
        let linked = base.join("appdata/codex-accounts/c1");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(global.join("config.toml"), linked.join("config.toml")).unwrap();
        // An account with a config of its own.
        let own = base.join("appdata/codex-accounts/c2");
        write(&own.join("config.toml"), "[mcp_servers.mine]\ncommand = \"m\"\n");
        inputs.codex_accounts = vec![
            ProfileDir { profile: "c1".into(), config_dir: linked },
            ProfileDir { profile: "c2".into(), config_dir: own },
        ];

        let report = scan(&inputs).into_report();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert_eq!(
            rows(&report),
            [
                ("S7", s("c2"), None, "mine".into(), "stdio".into()),
                ("S7", s("global"), None, "docs".into(), "stdio".into()),
                ("S7", s("global"), None, "remote".into(), "http".into()),
            ]
        );
        let docs = report.servers.iter().find(|r| r.name == "docs").unwrap();
        assert_eq!(docs.origin_label, "~/.codex/config.toml");
        assert_eq!(docs.summary.as_ref().unwrap().env_keys, ["API_KEY"]);
        let shown = format!("{}{report:?}", serde_json::to_string(&report).unwrap());
        assert!(!shown.contains("SECRET-"), "{shown}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn codex_home_replaces_the_default_for_the_global_profile() {
        let base = tmp("codex-home");
        let mut inputs = inputs(&base);
        write(&inputs.home.join(".codex/config.toml"), "[mcp_servers.default_home]\ncommand = \"d\"\n");
        let custom = base.join("custom-codex");
        write(&custom.join("config.toml"), "[mcp_servers.custom]\nurl = \"https://c\"\n");
        inputs.codex_home = Some(custom);
        let report = scan(&inputs).into_report();
        assert_eq!(
            rows(&report),
            [
                ("S7", None, None, "default_home".into(), "stdio".into()),
                ("S7", s("global"), None, "custom".into(), "http".into()),
            ]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_project_codex_config_is_listed_under_its_project() {
        let base = tmp("codex-project");
        let mut inputs = inputs(&base);
        let project = base.join("work/api");
        write(&project.join(".codex/config.toml"), "[mcp_servers.projy]\ncommand = \"p\"\nenv = { K = \"SECRET-P\" }\n");
        let broken = base.join("work/broken");
        write(&broken.join(".codex/config.toml"), "[mcp_servers.a]\ncommand = \"SECRET-UNTERMINATED\n");
        inputs.projects = vec![
            ProjectDir { id: "p1".into(), path: project },
            ProjectDir { id: "p2".into(), path: broken },
            ProjectDir { id: "p3".into(), path: base.join("work/none") },
        ];
        let report = scan(&inputs).into_report();
        assert_eq!(rows(&report), [("S8", None, s("p1"), "projy".into(), "stdio".into())]);
        assert_eq!(report.servers[0].scope, DiscoveryScope::Project);
        assert_eq!(report.problems.len(), 1);
        let problem = &report.problems[0];
        assert_eq!((problem.source, problem.project_id.as_deref()), (SourceId::S8, Some("p2")));
        assert!(problem.message.starts_with("invalid TOML at line 2"), "{problem:?}");
        assert!(!format!("{report:?}").contains("SECRET-"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
