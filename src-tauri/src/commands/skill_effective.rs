//! "What the agent sees": for one project, the skills each engine in `SKILL_ENGINES` loads from
//! its HOME folder (global) and the project's folder, read from the disk rather than from
//! Puppeteer's switches. A skill the repo commits to git or the user put in HOME counts as much as
//! one linked from the library: the agent can't tell them apart.
//!
//! [`resolve`] is pure: what was found in the two folders in, the list with shadowing marked out.
//! [`effective_skills`] reads the folders and calls it. Read-only, so a remote desktop may call it.
//! `library.skills` is a locked prop, always on, so nothing is gated here; the Doctor badges come
//! from `diagnose_skills` on the screen's side.

use crate::commands::skills::{SkillEngineDecl, SKILL_ENGINES};
use crate::db::Db;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The engine's folder in HOME, used by every project.
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Puppeteer's link to a library skill.
    Library,
    /// Committed to the project's git repo: the team's own skill.
    GitManaged,
    /// Anything else: the user's own folder, a copy, a link elsewhere.
    Unmanaged,
}

/// A skill folder found in one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub scope: Scope,
    pub origin: Origin,
    /// The library skill a `Library` link points at.
    pub skill_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveSkill {
    pub name: String,
    pub scope: Scope,
    pub origin: Origin,
    pub skill_id: Option<String>,
    /// A global skill of the same name wins, so the agent never loads this one.
    pub shadowed: bool,
}

#[derive(Debug, Serialize)]
pub struct EngineSkills {
    pub engine: SkillEngineDecl,
    /// Whether a global skill hides the project's of the same name (Claude) or both load (Codex).
    pub home_wins: bool,
    /// By name, the global one first when both scopes hold a name.
    pub skills: Vec<EffectiveSkill>,
}

/// Mark the project skills a global one of the same name hides, when the engine lets HOME win.
/// The same library skill linked in both scopes hides nothing: it is one skill either way, as on
/// the Allocation matrix.
pub fn resolve(found: Vec<Found>, home_wins: bool) -> Vec<EffectiveSkill> {
    let global: HashMap<String, Option<String>> = found
        .iter()
        .filter(|f| f.scope == Scope::Global)
        .map(|f| (f.name.clone(), f.skill_id.clone()))
        .collect();
    let mut out: Vec<EffectiveSkill> = found
        .into_iter()
        .map(|f| {
            let shadowed = home_wins
                && f.scope == Scope::Project
                && global.get(&f.name).is_some_and(|id| id.is_none() || *id != f.skill_id);
            EffectiveSkill { name: f.name, scope: f.scope, origin: f.origin, skill_id: f.skill_id, shadowed }
        })
        .collect();
    out.sort_by(|a, b| (&a.name, a.scope).cmp(&(&b.name, b.scope)));
    out
}

/// Top-level names under `rel` that git tracks anything in, with one `git ls-files` for the folder.
/// Not a repo, or `rel` behind a symlink: none.
pub(crate) fn tracked_names(root: &Path, rel: &str) -> HashSet<String> {
    let Ok(out) = std::process::Command::new("git").arg("-C").arg(root).args(["ls-files", "-z", "--", rel]).output()
    else {
        return HashSet::new();
    };
    if !out.status.success() {
        return HashSet::new();
    }
    let prefix = Path::new(rel);
    out.stdout
        .split(|b| *b == 0)
        .filter_map(|p| {
            let path = PathBuf::from(String::from_utf8_lossy(p).into_owned());
            let first = path.strip_prefix(prefix).ok()?.components().next()?;
            Some(first.as_os_str().to_string_lossy().into_owned())
        })
        .collect()
}

/// The skill folders in `<base>/<rel>`: entries holding a `SKILL.md`, links followed as the agent
/// follows them. `library` maps a library skill's source folder to its id.
pub fn scan(base: &Path, rel: &str, scope: Scope, library: &HashMap<PathBuf, String>) -> Vec<Found> {
    let Ok(entries) = fs::read_dir(base.join(rel)) else { return Vec::new() };
    let tracked = match scope {
        Scope::Project => tracked_names(base, rel),
        Scope::Global => HashSet::new(),
    };
    entries
        .flatten()
        .filter(|e| e.path().join("SKILL.md").is_file())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let link = e.file_type().is_ok_and(|t| t.is_symlink()).then(|| fs::read_link(e.path()).ok()).flatten();
            let skill_id = link.and_then(|target| library.get(&target).cloned());
            let origin = if skill_id.is_some() {
                Origin::Library
            } else if tracked.contains(&name) {
                Origin::GitManaged
            } else {
                Origin::Unmanaged
            };
            Found { name, scope, origin, skill_id }
        })
        .collect()
}

pub async fn effective_skills(db: &Db, home: Option<PathBuf>, project_id: &str) -> Result<Vec<EngineSkills>, String> {
    use sqlx::Row;
    let path: String = sqlx::query("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Project not found: {}", project_id))?
        .get("path");
    let library: HashMap<PathBuf, String> = sqlx::query("SELECT id, source_path FROM skills")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| (PathBuf::from(r.get::<String, _>("source_path")), r.get("id")))
        .collect();

    // A read_dir per folder plus a git call per project folder: off the async runtime.
    tokio::task::spawn_blocking(move || {
        let root = PathBuf::from(path);
        if !root.is_dir() {
            return Err(format!("Project folder not found: {}", root.display()));
        }
        Ok(SKILL_ENGINES
            .iter()
            .map(|engine| {
                let mut found = scan(&root, engine.project_dir, Scope::Project, &library);
                if let (Some(home), Some(dir)) = (&home, engine.home_dir) {
                    found.extend(scan(home, dir, Scope::Global, &library));
                }
                EngineSkills { engine: *engine, home_wins: engine.home_wins, skills: resolve(found, engine.home_wins) }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read-only, remote-callable: per engine, the skills an agent run in this project loads.
#[tauri::command]
pub async fn list_effective_skills(
    app: AppHandle,
    db: State<'_, Db>,
    project_id: String,
) -> Result<Vec<EngineSkills>, String> {
    effective_skills(db.inner(), app.path().home_dir().ok(), &project_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(name: &str, scope: Scope, origin: Origin, skill_id: Option<&str>) -> Found {
        Found { name: name.into(), scope, origin, skill_id: skill_id.map(str::to_string) }
    }

    /// (name, scope, shadowed) of each resolved skill.
    fn shadows(list: &[EffectiveSkill]) -> Vec<(&str, Scope, bool)> {
        list.iter().map(|s| (s.name.as_str(), s.scope, s.shadowed)).collect()
    }

    #[test]
    fn a_winning_home_hides_the_projects_skill_of_that_name() {
        use Origin::*;
        use Scope::*;
        let input = vec![
            found("review", Project, Library, Some("r")),
            found("deploy", Project, GitManaged, None),
            found("only-here", Project, Unmanaged, None),
            found("review", Global, Unmanaged, None),
            found("deploy", Global, Library, Some("d")),
            found("mine", Global, Unmanaged, None),
        ];
        let want_claude = vec![
            ("deploy", Global, false),
            ("deploy", Project, true),
            ("mine", Global, false),
            ("only-here", Project, false),
            ("review", Global, false),
            ("review", Project, true),
        ];
        assert_eq!(shadows(&resolve(input.clone(), true)), want_claude);

        // Codex loads both of a pair: nothing is hidden.
        let codex = resolve(input, false);
        assert!(codex.iter().all(|s| !s.shadowed));
        assert_eq!(codex.len(), 6);
    }

    #[test]
    fn the_same_library_skill_in_both_scopes_hides_nothing() {
        use Origin::*;
        use Scope::*;
        let same = resolve(vec![found("a", Project, Library, Some("a")), found("a", Global, Library, Some("a"))], true);
        assert_eq!(shadows(&same), vec![("a", Global, false), ("a", Project, false)]);
        // Two library skills that happen to share the folder name are two skills.
        let other = resolve(vec![found("a", Project, Library, Some("a")), found("a", Global, Library, Some("b"))], true);
        assert_eq!(shadows(&other), vec![("a", Global, false), ("a", Project, true)]);
        // The project's own copy under a global link: the link wins.
        let copy = resolve(vec![found("a", Project, Unmanaged, None), found("a", Global, Library, Some("a"))], true);
        assert_eq!(shadows(&copy), vec![("a", Global, false), ("a", Project, true)]);
    }

    #[cfg(unix)]
    mod disk {
        use super::super::*;
        use sqlx::sqlite::SqlitePoolOptions;
        use std::os::unix::fs::symlink;
        use std::process::Command;

        struct Fixture {
            dir: PathBuf,
            db: Db,
        }

        impl Fixture {
            /// Library skills `alpha` and `beta`, project `p1` (a git repo) and an empty HOME, all
            /// under one temp folder.
            async fn new(label: &str) -> Self {
                let dir = std::env::temp_dir().join(format!("puppeteer-effective-{}-{}", label, uuid::Uuid::new_v4()));
                fs::create_dir_all(dir.join("p1")).unwrap();
                fs::create_dir_all(dir.join("home")).unwrap();
                assert!(Command::new("git").arg("-C").arg(dir.join("p1")).args(["init", "-q"]).status().unwrap().success());
                let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
                sqlx::migrate!("./migrations").run(&db).await.unwrap();
                sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'one', ?, '2026-01-01')")
                    .bind(dir.join("p1").to_string_lossy().to_string())
                    .execute(&db)
                    .await
                    .unwrap();
                for name in ["alpha", "beta"] {
                    let source = dir.join("library").join(name);
                    skill_md(&source);
                    sqlx::query(
                        "INSERT INTO skills (id, name, description, target, source_path, updated_at)
                         VALUES (?, ?, 'd', 'both', ?, '2026-01-01')",
                    )
                    .bind(format!("id-{name}"))
                    .bind(name)
                    .bind(source.to_string_lossy().to_string())
                    .execute(&db)
                    .await
                    .unwrap();
                }
                Fixture { dir, db }
            }

            fn path(&self, rel: &str) -> PathBuf {
                self.dir.join(rel)
            }

            fn link(&self, skill: &str, at: &str) {
                let dest = self.path(at);
                fs::create_dir_all(dest.parent().unwrap()).unwrap();
                symlink(self.path("library").join(skill), dest).unwrap();
            }

            /// Per engine id, (name, scope, origin, skill_id, shadowed).
            async fn seen(&self) -> Vec<(&'static str, Vec<(String, Scope, Origin, Option<String>, bool)>)> {
                effective_skills(&self.db, Some(self.path("home")), "p1")
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|e| {
                        let list = e.skills.into_iter().map(|s| (s.name, s.scope, s.origin, s.skill_id, s.shadowed));
                        (e.engine.id, list.collect())
                    })
                    .collect()
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.dir);
            }
        }

        fn skill_md(dir: &Path) {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("SKILL.md"), "---\nname: x\ndescription: d\n---\n").unwrap();
        }

        #[tokio::test]
        async fn reads_both_scopes_of_every_engine_from_the_disk() {
            use Origin::*;
            use Scope::*;
            let fx = Fixture::new("scan").await;
            // Claude: alpha linked in the project and the user's own alpha in HOME; the repo
            // commits `team`; an untracked copy `local`; a folder without SKILL.md is no skill.
            fx.link("alpha", "p1/.claude/skills/alpha");
            skill_md(&fx.path("home/.claude/skills/alpha"));
            skill_md(&fx.path("p1/.claude/skills/team"));
            let add = Command::new("git").arg("-C").arg(fx.path("p1")).args(["add", ".claude/skills/team"]).status();
            assert!(add.unwrap().success());
            skill_md(&fx.path("p1/.claude/skills/local"));
            fs::create_dir_all(fx.path("p1/.claude/skills/empty")).unwrap();
            // Codex: beta linked in both scopes, alpha in the project and the user's alpha in HOME.
            fx.link("beta", "p1/.agents/skills/beta");
            fx.link("beta", "home/.agents/skills/beta");
            fx.link("alpha", "p1/.agents/skills/alpha");
            skill_md(&fx.path("home/.agents/skills/alpha"));
            // A link to a library skill that's gone is no skill either.
            symlink(fx.path("library/gone"), fx.path("p1/.agents/skills/gone")).unwrap();

            let id = |s: &str| Some(format!("id-{s}"));
            assert_eq!(
                fx.seen().await,
                vec![
                    (
                        "claude",
                        vec![
                            ("alpha".into(), Global, Unmanaged, None, false),
                            ("alpha".into(), Project, Library, id("alpha"), true),
                            ("local".into(), Project, Unmanaged, None, false),
                            ("team".into(), Project, GitManaged, None, false),
                        ]
                    ),
                    (
                        "codex",
                        vec![
                            ("alpha".into(), Global, Unmanaged, None, false),
                            ("alpha".into(), Project, Library, id("alpha"), false),
                            ("beta".into(), Global, Library, id("beta"), false),
                            ("beta".into(), Project, Library, id("beta"), false),
                        ]
                    ),
                ]
            );
        }

        #[tokio::test]
        async fn a_symlinked_skills_folder_is_followed_like_the_agent_does() {
            use Origin::*;
            use Scope::*;
            let fx = Fixture::new("folder-link").await;
            // `.agents/skills -> ../.claude/skills`: Codex loads Claude's skills.
            fx.link("alpha", "p1/.claude/skills/alpha");
            fs::create_dir_all(fx.path("p1/.agents")).unwrap();
            symlink(Path::new("../.claude/skills"), fx.path("p1/.agents/skills")).unwrap();
            let seen = fx.seen().await;
            assert_eq!(seen[1].1, vec![("alpha".into(), Project, Library, Some("id-alpha".into()), false)]);
        }

        #[tokio::test]
        async fn nothing_installed_and_no_home_is_an_empty_list() {
            let fx = Fixture::new("empty").await;
            let seen = effective_skills(&fx.db, None, "p1").await.unwrap();
            assert_eq!(seen.iter().map(|e| (e.engine.id, e.skills.len())).collect::<Vec<_>>(), [("claude", 0), ("codex", 0)]);
            assert!(seen[0].home_wins && !seen[1].home_wins);
        }

        #[tokio::test]
        async fn an_unknown_project_or_a_missing_folder_is_an_error() {
            let fx = Fixture::new("missing").await;
            assert_eq!(
                effective_skills(&fx.db, None, "nope").await.unwrap_err(),
                "Project not found: nope"
            );
            fs::remove_dir_all(fx.path("p1")).unwrap();
            let err = effective_skills(&fx.db, None, "p1").await.unwrap_err();
            assert!(err.starts_with("Project folder not found"), "{err}");
        }
    }
}
