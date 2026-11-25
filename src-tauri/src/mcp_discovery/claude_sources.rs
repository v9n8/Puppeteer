//! Claude Code's file sources: S2 user scope and S3 local scope of every Claude config file, S4 a
//! project's `.mcp.json`, S6 the managed MCP file.
//!
//! A run reads `.claude.json` from its config dir: an app account's dir, else the global profile's
//! (`CLAUDE_CONFIG_DIR` as Puppeteer started, whose file replaces HOME's), else HOME. HOME's file is
//! listed even when no run reads it, so the user sees every server on the machine. A file reached
//! twice (a linked dir) is read once, under the first profile.

use super::{DiscoveryInputs, DiscoveryScope, Origin, Scan, SourceId};
use crate::import::claude_code::{scan_config_mcp, scan_mcp_servers_file, scan_project_mcp};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const GLOBAL_PROFILE: &str = "global";

/// `(config file, the dir it is read as part of, profile)` for every Claude config file.
fn config_files(inputs: &DiscoveryInputs) -> Vec<(PathBuf, PathBuf, Option<String>)> {
    let home = (inputs.home.join(".claude.json"), inputs.home.clone());
    let mut files = match &inputs.claude_config_dir {
        Some(dir) => vec![
            (dir.join(".claude.json"), dir.clone(), Some(GLOBAL_PROFILE.to_string())),
            (home.0, home.1, None),
        ],
        None => vec![(home.0, home.1, Some(GLOBAL_PROFILE.to_string()))],
    };
    for account in &inputs.claude_accounts {
        files.push((account.config_dir.join(".claude.json"), account.config_dir.clone(), Some(account.profile.clone())));
    }
    files
}

/// The project whose folder a local-scope key names.
fn project_of<'a>(inputs: &'a DiscoveryInputs, key: &str) -> Option<&'a str> {
    let key = Path::new(key);
    let real = std::fs::canonicalize(key).ok();
    inputs
        .projects
        .iter()
        .find(|p| p.path.as_path() == key || real.as_ref().is_some_and(|r| std::fs::canonicalize(&p.path).is_ok_and(|q| &q == r)))
        .map(|p| p.id.as_str())
}

pub fn scan(inputs: &DiscoveryInputs) -> Scan {
    let mut scan = Scan::default();
    let mut seen = HashSet::new();
    for (path, root, profile) in config_files(inputs) {
        if !path.is_file() || !seen.insert(std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone())) {
            continue;
        }
        let user = Origin {
            source: SourceId::S2,
            scope: DiscoveryScope::Global,
            path: &path,
            local_key: None,
            profile: profile.as_deref(),
            project_id: None,
            app: None,
            importable: true,
        };
        let config = match scan_config_mcp(&path, &root, &inputs.ctx) {
            Ok(config) => config,
            Err(e) => {
                scan.problems.push(user.problem(inputs, e));
                continue;
            }
        };
        scan.extend(user.rows(inputs, config.user));
        for (key, report) in config.local {
            let local = Origin {
                source: SourceId::S3,
                scope: DiscoveryScope::Project,
                local_key: Some(key.as_str()),
                project_id: project_of(inputs, &key),
                ..user
            };
            scan.extend(local.rows(inputs, report));
        }
    }

    for project in &inputs.projects {
        let path = project.path.join(".mcp.json");
        let origin = Origin {
            source: SourceId::S4,
            scope: DiscoveryScope::Project,
            path: &path,
            local_key: None,
            profile: None,
            project_id: Some(&project.id),
            app: None,
            importable: true,
        };
        match scan_project_mcp(&project.path, &inputs.ctx) {
            Ok(Some(report)) => scan.extend(origin.rows(inputs, report)),
            Ok(None) => {}
            Err(e) => scan.problems.push(origin.problem(inputs, e)),
        }
    }

    let managed = inputs.claude_managed_dir.join("managed-mcp.json");
    if managed.is_file() {
        let origin = Origin {
            source: SourceId::S6,
            scope: DiscoveryScope::Global,
            path: &managed,
            local_key: None,
            profile: None,
            project_id: None,
            app: None,
            importable: false,
        };
        match scan_mcp_servers_file(&managed, &inputs.claude_managed_dir, &inputs.ctx) {
            Ok(report) => scan.extend(origin.rows(inputs, report)),
            Err(e) => scan.problems.push(origin.problem(inputs, e)),
        }
    }
    scan
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{ProfileDir, DiscoveryReport, ProjectDir};
    use super::*;
    use crate::import::ScanContext;

    pub(crate) fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-mcp-disc-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Canonical, so paths compare equal to what discovery resolves (`/var` → `/private/var`).
        std::fs::canonicalize(&dir).unwrap()
    }

    pub(crate) fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    pub(crate) fn inputs(base: &Path) -> DiscoveryInputs {
        DiscoveryInputs {
            home: base.join("home"),
            claude_config_dir: None,
            claude_accounts: vec![],
            codex_home: None,
            codex_accounts: vec![],
            projects: vec![],
            claude_managed_dir: base.join("managed"),
            ctx: ScanContext { library_root: base.join("appdata") },
        }
    }

    /// `(source, profile, project, name, transport)` of each row, sorted.
    pub(crate) fn rows(report: &DiscoveryReport) -> Vec<(&'static str, Option<String>, Option<String>, String, String)> {
        let mut out: Vec<_> = report
            .servers
            .iter()
            .map(|r| (r.source.as_str(), r.profile.clone(), r.project_id.clone(), r.name.clone(), r.transport.clone()))
            .collect();
        out.sort();
        out
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    /// The machine of most tests: HOME (global profile) with user and local scope, one app account,
    /// one project with `.mcp.json`, a managed file. Every VALUE is `SECRET-…`.
    fn machine(base: &Path) -> DiscoveryInputs {
        let mut inputs = inputs(base);
        let project = base.join("work/app");
        std::fs::create_dir_all(&project).unwrap();
        write(
            &inputs.home.join(".claude.json"),
            &format!(
                r#"{{"oauthAccount": {{"accountUuid": "SECRET-UUID", "emailAddress": "SECRET-MAIL"}},
                    "mcpServers": {{"fs": {{"command": "npx", "args": ["fs"], "env": {{"FS_TOKEN": "SECRET-ENV"}}}}}},
                    "projects": {{
                      "{}": {{"allowedTools": ["SECRET-TOOL"], "mcpServers": {{"local.db": {{"type": "http", "url": "https://db", "headers": {{"Authorization": "SECRET-HEADER"}}}}}}}},
                      "/not/a/project": {{"mcpServers": {{"stray": {{"command": "s"}}}}}},
                      "/no/servers": {{"history": ["SECRET-HISTORY"]}}
                    }}}}"#,
                project.display()
            ),
        );
        let account = base.join("appdata/claude-accounts/acc-1");
        write(&account.join(".claude.json"), r#"{"mcpServers": {"acct": {"type": "sse", "url": "https://acct"}}}"#);
        inputs.claude_accounts.push(ProfileDir { profile: "acc-1".into(), config_dir: account });
        write(&project.join(".mcp.json"), r#"{"mcpServers": {"y": {"command": "y", "env": {"Y_KEY": "SECRET-PROJ"}}}}"#);
        inputs.projects.push(ProjectDir { id: "p1".into(), path: project });
        write(&inputs.claude_managed_dir.join("managed-mcp.json"), r#"{"mcpServers": {"corp": {"url": "https://corp"}}}"#);
        inputs
    }

    #[test]
    fn every_claude_file_source_is_listed_with_its_profile_and_project() {
        let base = tmp("all");
        let inputs = machine(&base);
        let report = scan(&inputs).into_report();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert_eq!(
            rows(&report),
            [
                ("S2", s("acc-1"), None, "acct".into(), "sse".into()),
                ("S2", s("global"), None, "fs".into(), "stdio".into()),
                ("S3", s("global"), None, "stray".into(), "stdio".into()),
                ("S3", s("global"), s("p1"), "local.db".into(), "http".into()),
                ("S4", None, s("p1"), "y".into(), "stdio".into()),
                ("S6", None, None, "corp".into(), "http".into()),
            ]
        );
        let by = |name: &str| report.servers.iter().find(|r| r.name == name).unwrap();
        assert_eq!(by("fs").origin_label, "~/.claude.json");
        assert_eq!(by("fs").summary.as_ref().unwrap().env_keys, ["FS_TOKEN"]);
        assert_eq!(by("local.db").summary.as_ref().unwrap().header_keys, ["Authorization"]);
        assert_eq!((by("local.db").scope, by("local.db").import_name.as_str()), (DiscoveryScope::Project, "local-db"));
        let local_ref = format!("{}#{}", inputs.home.join(".claude.json").display(), base.join("work/app").display());
        assert_eq!(by("local.db").source_ref, local_ref);
        assert_eq!(by("y").scope, DiscoveryScope::Project);
        assert!(by("fs").importable && by("y").importable && !by("corp").importable);
        assert!(report.servers.iter().all(|r| r.fingerprint.is_some() && r.problem.is_none()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rows_never_carry_a_value_but_the_export_does() {
        let base = tmp("secrets");
        let inputs = machine(&base);
        let scan = scan(&inputs);
        let fs_export = scan.found.iter().find(|f| f.row.name == "fs").and_then(|f| f.export.as_ref()).unwrap();
        assert_eq!(fs_export.env["FS_TOKEN"], "SECRET-ENV", "an import copies the VALUE from the export");
        let report = scan.into_report();
        let shown = format!("{}{report:?}", serde_json::to_string(&report).unwrap());
        assert!(!shown.contains("SECRET-"), "{shown}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn row_ids_stay_the_same_across_scans() {
        let base = tmp("ids");
        let inputs = machine(&base);
        let ids = |r: DiscoveryReport| r.servers.into_iter().map(|r| r.id).collect::<Vec<_>>();
        let first = ids(scan(&inputs).into_report());
        assert_eq!(first, ids(scan(&inputs).into_report()));
        let fs = format!("S2|{}|fs", inputs.home.join(".claude.json").display());
        assert!(first.contains(&fs), "{first:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn with_claude_config_dir_the_global_profile_reads_that_dir() {
        let base = tmp("config-dir");
        let mut inputs = inputs(&base);
        write(&inputs.home.join(".claude.json"), r#"{"mcpServers": {"home": {"command": "h"}}}"#);
        let dir = base.join("custom-claude");
        write(&dir.join(".claude.json"), r#"{"mcpServers": {"custom": {"command": "c"}}}"#);
        inputs.claude_config_dir = Some(dir.clone());
        // An app account that links to the same dir is read once, as global.
        let link = base.join("appdata/claude-accounts/acc-2");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        inputs.claude_accounts.push(ProfileDir { profile: "acc-2".into(), config_dir: link });
        let report = scan(&inputs).into_report();
        assert_eq!(
            rows(&report),
            [("S2", None, None, "home".into(), "stdio".into()), ("S2", s("global"), None, "custom".into(), "stdio".into())]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unreadable_file_is_one_problem_and_the_rest_still_shows() {
        let base = tmp("broken");
        let mut inputs = machine(&base);
        let account = inputs.claude_accounts[0].config_dir.clone();
        write(&account.join(".claude.json"), r#"{"oauthAccount": {"accountUuid": "SECRET-UUID"}, "mcpServers": "#);
        let project2 = base.join("work/other");
        write(&project2.join(".mcp.json"), r#"{"mcpServers": {"ok": {"command": "o"}, "bad": {"command": "b", "env": {"K": 1}}}}"#);
        inputs.projects.push(ProjectDir { id: "p2".into(), path: project2.clone() });
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let managed = inputs.claude_managed_dir.join("managed-mcp.json");
            std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        let report = scan(&inputs).into_report();
        let problems: Vec<_> = report.problems.iter().map(|p| (p.source.as_str(), p.profile.clone(), p.message.clone())).collect();
        assert_eq!(problems[0], ("S2", s("acc-1"), "invalid JSON at line 1 column 63".to_string()));
        #[cfg(unix)]
        {
            assert_eq!(problems.len(), 2, "{problems:?}");
            assert_eq!(problems[1].0, "S6");
            assert!(problems[1].2.contains("ermission denied"), "{problems:?}");
        }
        let names: Vec<_> = report.servers.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"fs") && names.contains(&"y") && names.contains(&"ok"), "{names:?}");
        assert!(!names.contains(&"acct"));
        // A bad entry is a row of its own, not importable, its problem naming only the field.
        let bad = report.servers.iter().find(|r| r.name == "bad").unwrap();
        assert_eq!((bad.importable, bad.transport.as_str()), (false, "unknown"));
        assert_eq!(bad.problem.as_deref(), Some("`env.K` is not a string"));
        assert_eq!(bad.project_id.as_deref(), Some("p2"));
        assert!(!format!("{report:?}").contains("SECRET-"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let managed = inputs.claude_managed_dir.join("managed-mcp.json");
            std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_files_are_not_problems() {
        let base = tmp("empty");
        let mut inputs = inputs(&base);
        inputs.projects.push(ProjectDir { id: "p1".into(), path: base.join("nowhere") });
        inputs.claude_accounts.push(ProfileDir { profile: "a".into(), config_dir: base.join("gone") });
        let report = scan(&inputs).into_report();
        assert!(report.servers.is_empty() && report.problems.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
