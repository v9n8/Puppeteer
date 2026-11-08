//! Craft Agents (Craft's desktop agent app). Paths from its repository, craft-ai-agents/craft-agents-oss:
//! the README's `~/.craft-agent/` layout, `apps/electron/resources/docs/skills.md` and
//! `packages/shared/src/skills/storage.ts` (read 2026-11-08; the agents.craft.do docs now redirect
//! to craft.do):
//!
//! - Global only: the skills of each workspace, `~/.craft-agent/workspaces/<id>/skills/<slug>/SKILL.md`,
//!   in the SKILL.md format of Claude Code.
//!
//! Craft also reads `~/.agents/skills` and a project's `.agents/skills`; the Codex adapter offers
//! those, so a workspace skill that is one of `~/.agents/skills` (by its SKILL.md but for the name)
//! is left out here, as is the same skill in a second workspace. Not imported: sources (MCP
//! servers; their credentials are in Craft's encrypted store), workspaces kept outside
//! `~/.craft-agent/workspaces`, and `CRAFT_CONFIG_DIR`.

use super::*;

pub struct Craft;

/// Craft runs Claude.
const TARGET: &str = "claude";

/// The skill folders of every workspace, by path.
fn workspace_skills(home: &Path, ctx: &ScanContext) -> Vec<PathBuf> {
    let mut workspaces: Vec<PathBuf> = fs::read_dir(home.join(".craft-agent").join("workspaces"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    workspaces.sort();
    workspaces.iter().flat_map(|w| skill_dirs(&w.join("skills"), ctx)).collect()
}

impl Adapter for Craft {
    fn id(&self) -> &'static str {
        "craft"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        if scope == Scope::Global && !workspace_skills(root, ctx).is_empty() { vec![ImportKind::Skill] } else { Vec::new() }
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let mut report = ScanReport::default();
        if scope != Scope::Global {
            return report;
        }
        let mut seen: Vec<String> =
            skill_dirs(&root.join(".agents").join("skills"), ctx).iter().filter_map(|d| skill_body_hash(d)).collect();
        for dir in workspace_skills(root, ctx) {
            let Some(c) = skill_candidate(&mut report, root, &dir, TARGET) else { continue };
            if let Some(body) = skill_body_hash(&dir) {
                if seen.contains(&body) {
                    continue;
                }
                seen.push(body);
            }
            report.candidates.push(c);
        }
        report
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-craft-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn ids(report: &ScanReport) -> Vec<(String, String)> {
        report.candidates.iter().map(|c| (c.id.clone(), c.name.clone())).collect()
    }

    #[test]
    fn home_yields_each_workspace_skill_once() {
        let home = tmp("home");
        let ctx = ScanContext { library_root: home.join("appdata") };
        let ws = home.join(".craft-agent/workspaces");
        write(&ws.join("a/skills/commit/SKILL.md"), "---\nname: \"Commit\"\ndescription: Commits\nalwaysAllow: [\"Bash\"]\n---\nCommit.");
        write(&ws.join("a/skills/commit/icon.svg"), "<svg/>");
        // The same skill under another name in a second workspace.
        write(&ws.join("b/skills/commit-2/SKILL.md"), "---\nname: \"Commit 2\"\ndescription: Commits\nalwaysAllow: [\"Bash\"]\n---\nCommit.");
        write(&ws.join("b/skills/review/SKILL.md"), "Review.");
        // Also in `~/.agents/skills`: the Codex adapter offers it.
        write(&ws.join("b/skills/shared/SKILL.md"), "---\nname: shared\n---\nShared.");
        write(&home.join(".agents/skills/shared/SKILL.md"), "---\nname: shared-skill\n---\nShared.");
        write(&ws.join("b/skills/broken/SKILL.md"), [0xff, 0xfe]);
        write(&ws.join("b/skills/.hidden/SKILL.md"), "x");
        write(&ws.join("b/config.json"), r#"{"token": "SECRET-CONFIG"}"#);
        write(&ws.join("b/sources/linear/config.json"), r#"{"token": "SECRET-SOURCE"}"#);

        let report = Craft.scan(&home, Scope::Global, &ctx);
        assert_eq!(
            ids(&report),
            [
                (".craft-agent/workspaces/a/skills/commit".into(), "commit".into()),
                (".craft-agent/workspaces/b/skills/review".into(), "review".into()),
            ]
            .map(|(path, name): (String, String)| (format!("skills:{path}"), name))
        );
        let commit = &report.candidates[0];
        assert_eq!((commit.description.as_str(), commit.target.as_deref()), ("Commits", Some("claude")));
        assert_eq!(report.problems.len(), 1);
        assert!(report.problems[0].path.ends_with("broken/SKILL.md"));
        assert_eq!(Craft.detect(&home, Scope::Global, &ctx), [ImportKind::Skill]);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
    }

    #[test]
    fn a_project_or_a_home_without_the_app_has_nothing_to_import() {
        let root = tmp("none");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".agents/skills/x/SKILL.md"), "x");
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(Craft.detect(&root, scope, &ctx), []);
            let report = Craft.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
        // A project folder laid out like HOME is still not read.
        write(&root.join(".craft-agent/workspaces/a/skills/x/SKILL.md"), "y");
        assert_eq!(Craft.detect(&root, Scope::Project, &ctx), []);
        assert!(Craft.scan(&root, Scope::Project, &ctx).candidates.is_empty());
    }
}
