//! UX-58 (D1 step 2): turn the links Puppeteer made into its old library into files of their own,
//! so no Library item is "edit here, change everywhere" any more.
//!
//! Only an item link whose target is in a Puppeteer library ([`in_puppeteer_library`], the
//! `puppeteer_link` status of a scan) is touched, in HOME's and every live project's folders. It
//! is replaced by a copy of what it points at: a file for a rule / command / agent, the whole
//! folder for a skill. The rule blocks Puppeteer keeps in a project's `AGENTS.md` lose their
//! markers; their text stays where it is, now the user's own. The library itself is left as is.
//!
//! Nothing runs by itself: the preview lists every path, and the apply takes back the paths the
//! user confirmed. It plans again and does only what is both confirmed and still to do, file by
//! file, logged; the first failure stops it and what was done stays done. Both are local-only.

use super::{in_puppeteer_library, open_entry, open_folder, remove_link, ItemKind, LibraryScope, FOLDERS};
use crate::commands::agents_block;
use crate::commands::files::copy_recursive;
use crate::commands::memory::write_atomic;
use crate::commands::project_links::{create_symlink, git_tracked};
use crate::commands::skill_effective::tracked_names;
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

/// A link that turns into a file (or a skill's folder).
#[derive(Debug, Clone, Serialize)]
pub struct PlannedLink {
    pub kind: ItemKind,
    pub engine: &'static str,
    pub name: String,
    /// The link, as the Library lists it (its `entry`): the id the apply takes back.
    pub entry: String,
    /// What it points at in the library: copied in its place.
    pub target: String,
    /// Git tracks the link: the change ends up in a commit.
    pub git_tracked: bool,
    /// Why it can't be turned into a file (its target is gone, …): left as is.
    pub error: Option<String>,
    /// The link in its real folder.
    #[serde(skip)]
    at: PathBuf,
}

/// A project's `AGENTS.md` holding Puppeteer's blocks.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedBlocks {
    /// The id the apply takes back.
    pub path: String,
    /// `kind:name` of each block whose markers go.
    pub blocks: Vec<String>,
    pub git_tracked: bool,
    pub error: Option<String>,
}

/// What changes in HOME (`scope: global`) or one project.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedPlace {
    pub scope: LibraryScope,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub root: String,
    pub links: Vec<PlannedLink>,
    pub agents_md: Option<PlannedBlocks>,
}

#[derive(Debug, Serialize)]
pub struct MaterializeFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Default, Serialize)]
pub struct MaterializeOutcome {
    /// Changed, in order.
    pub done: Vec<String>,
    /// Where it stopped; what is in `done` stays done.
    pub failed: Option<MaterializeFailure>,
    /// Confirmed, but nothing to do any more (already a file, gone, can't be changed).
    pub skipped: Vec<String>,
}

/// A place to look in: HOME, or a live project.
pub(crate) struct Place {
    pub scope: LibraryScope,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub root: PathBuf,
}

/// Every link into a Puppeteer library and every `AGENTS.md` with blocks, by place; places with
/// nothing to do are left out. A link reached twice (`.agents/skills` linked to `.claude/skills`)
/// is listed once.
pub(crate) fn plan(places: &[Place], secret: &SecretPaths) -> Vec<PlannedPlace> {
    let mut seen = HashSet::new();
    places
        .iter()
        .map(|p| PlannedPlace {
            scope: p.scope,
            project_id: p.project_id.clone(),
            project_name: p.project_name.clone(),
            root: p.root.display().to_string(),
            links: own_links(&p.root, p.scope, secret, &mut seen),
            agents_md: match p.scope {
                LibraryScope::Project => agents_blocks(&p.root),
                LibraryScope::Global => None,
            },
        })
        .filter(|p| !p.links.is_empty() || p.agents_md.is_some())
        .collect()
}

fn own_links(base: &Path, scope: LibraryScope, secret: &SecretPaths, seen: &mut HashSet<PathBuf>) -> Vec<PlannedLink> {
    let mut out = Vec::new();
    for folder in FOLDERS.iter().filter(|f| f.in_scope(scope)) {
        let Ok((_, dir)) = open_folder(base, folder, secret) else { continue };
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut names: Vec<(String, String)> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_symlink()))
            .filter_map(|e| {
                let entry_name = e.file_name().to_str()?.to_string();
                if entry_name.starts_with('.') {
                    return None;
                }
                let name = match folder.kind {
                    ItemKind::Skill => entry_name.clone(),
                    _ => entry_name.strip_suffix(".md").filter(|n| !n.is_empty())?.to_string(),
                };
                Some((name, entry_name))
            })
            .collect();
        names.sort();
        let tracked = match scope {
            LibraryScope::Project => tracked_names(base, folder.rel),
            LibraryScope::Global => HashSet::new(),
        };
        for (name, entry_name) in names {
            let at = dir.join(&entry_name);
            let target = match fs::canonicalize(&at) {
                Ok(real) => real,
                // Dangling: still Puppeteer's when the path it names is in a library.
                Err(_) => match fs::read_link(&at) {
                    Ok(raw) => dir.join(raw),
                    Err(_) => continue,
                },
            };
            if !in_puppeteer_library(&target) || !seen.insert(at.clone()) {
                continue;
            }
            let error = match open_entry(base, folder, &name, secret) {
                Ok(entry) if entry.link && entry.at == at => None,
                Ok(_) => Some(format!("{} is no longer a link", at.display())),
                Err(e) => Some(e),
            };
            out.push(PlannedLink {
                kind: folder.kind,
                engine: folder.engine,
                name,
                entry: base.join(folder.rel).join(&entry_name).display().to_string(),
                target: target.display().to_string(),
                git_tracked: tracked.contains(&entry_name),
                error,
                at,
            });
        }
    }
    out
}

fn agents_blocks(root: &Path) -> Option<PlannedBlocks> {
    let path = root.join("AGENTS.md");
    let content = fs::read_to_string(&path).ok()?;
    let blocks = agents_block::block_names(&content);
    if blocks.is_empty() {
        return None;
    }
    // Puppeteer never writes through a linked AGENTS.md (agents_block); neither does this.
    let error = fs::symlink_metadata(&path)
        .is_ok_and(|m| m.file_type().is_symlink())
        .then(|| format!("{} is a link — Puppeteer won't write through it", path.display()));
    Some(PlannedBlocks { path: path.display().to_string(), blocks, git_tracked: git_tracked(root, &path), error })
}

/// Replace the link at `at` with a copy of `target`. The copy is made next to it first, so a
/// failed copy leaves the link; a skill folder that can't take the link's place gets it back.
fn materialize_link(at: &Path, target: &Path) -> Result<(), String> {
    let dir = at.parent().ok_or_else(|| format!("{}: no parent folder", at.display()))?;
    let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let raw = fs::read_link(at).map_err(|e| format!("{}: {}", at.display(), e))?;
    let tmp = dir.join(format!(".{name}.puppeteer-{}.tmp", uuid::Uuid::new_v4()));
    let discard = |tmp: &Path| {
        let _ = if tmp.is_dir() { fs::remove_dir_all(tmp) } else { fs::remove_file(tmp) };
    };
    if let Err(e) = copy_recursive(target, &tmp) {
        discard(&tmp);
        return Err(format!("{}: {}", at.display(), e));
    }
    if !tmp.is_dir() {
        // A file renamed over a link replaces the link, not what it points at.
        return fs::rename(&tmp, at).map_err(|e| {
            discard(&tmp);
            format!("{}: {}", at.display(), e)
        });
    }
    if let Err(e) = remove_link(at) {
        discard(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, at).map_err(|e| {
        let _ = create_symlink(&raw, at);
        discard(&tmp);
        format!("{}: {}", at.display(), e)
    })
}

fn unwrap_blocks(path: &Path) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    write_atomic(path, agents_block::unwrap_all(&content).as_bytes()).map_err(|e| format!("{}: {}", path.display(), e))
}

/// Do what the fresh plan and `confirmed` both hold, in plan order; stop at the first failure.
pub(crate) fn apply(places: &[Place], secret: &SecretPaths, confirmed: &[String]) -> MaterializeOutcome {
    let mut out = MaterializeOutcome::default();
    let mut todo: Vec<&str> = confirmed.iter().map(String::as_str).collect();
    let mut take = |path: &str| {
        let found = todo.iter().position(|p| *p == path);
        found.map(|i| todo.swap_remove(i)).is_some()
    };
    let steps = plan(places, secret).into_iter().flat_map(|place| {
        let links = place.links.into_iter().filter(|l| l.error.is_none()).map(|l| (l.entry, Some((l.at, l.target))));
        let blocks = place.agents_md.into_iter().filter(|b| b.error.is_none()).map(|b| (b.path, None));
        links.chain(blocks).collect::<Vec<_>>()
    });
    for (path, link) in steps {
        if !take(&path) {
            continue;
        }
        let result = match &link {
            Some((at, target)) => materialize_link(at, Path::new(target)),
            None => unwrap_blocks(Path::new(&path)),
        };
        match result {
            Ok(()) => {
                match &link {
                    Some((_, target)) => tracing::info!(path = %path, from = %target, "library: link turned into a file"),
                    None => tracing::info!(path = %path, "library: Puppeteer's AGENTS.md block markers removed"),
                }
                out.done.push(path);
            }
            Err(error) => {
                tracing::warn!(path = %path, error = %error, "library: turning links into files stopped");
                out.failed = Some(MaterializeFailure { path, error });
                break;
            }
        }
    }
    if out.failed.is_none() {
        out.skipped = todo.into_iter().map(str::to_string).collect();
    }
    out
}

async fn places(app: &AppHandle, db: &Db) -> Result<Vec<Place>, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let projects = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, name, path FROM projects WHERE deleted_at IS NULL ORDER BY position, name",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    let mut out = vec![Place { scope: LibraryScope::Global, project_id: None, project_name: None, root: home }];
    out.extend(projects.into_iter().map(|(id, name, path)| Place {
        scope: LibraryScope::Project,
        project_id: Some(id),
        project_name: Some(name),
        root: PathBuf::from(path),
    }));
    Ok(out)
}

/// Local-only, read-only: what `library_materialize` would change.
#[tauri::command]
pub async fn library_materialize_preview(app: AppHandle, db: State<'_, Db>) -> Result<Vec<PlannedPlace>, String> {
    let places = places(&app, db.inner()).await?;
    let secret = SecretPaths::for_app(&app);
    super::blocking(move || Ok(plan(&places, &secret))).await
}

/// Local-only: turn the confirmed links (their `entry`) into files and unwrap the confirmed
/// `AGENTS.md` files (their `path`), as the preview listed them.
#[tauri::command]
pub async fn library_materialize(
    app: AppHandle,
    db: State<'_, Db>,
    paths: Vec<String>,
) -> Result<MaterializeOutcome, String> {
    let places = places(&app, db.inner()).await?;
    let secret = SecretPaths::for_app(&app);
    super::blocking(move || Ok(apply(&places, &secret, &paths))).await
}

#[cfg(test)]
mod tests {
    use super::super::APP_ID_PREFIX;
    use super::*;
    use std::process::Command;

    /// A temp HOME with a Puppeteer library in its app data, and a project (a git repo).
    struct Fixture {
        dir: PathBuf,
        places: Vec<Place>,
        secret: SecretPaths,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-materialize-{}-{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            let dir = fs::canonicalize(dir).unwrap();
            assert!(Command::new("git").arg("-C").arg(dir.join("proj")).args(["init", "-q"]).status().unwrap().success());
            let home = dir.join("home");
            let secret = SecretPaths::new(Some(&home), Some(&home.join("Library/Application Support").join(format!("{APP_ID_PREFIX}.test"))));
            let places = vec![
                Place { scope: LibraryScope::Global, project_id: None, project_name: None, root: home },
                Place {
                    scope: LibraryScope::Project,
                    project_id: Some("p1".into()),
                    project_name: Some("Proj".into()),
                    root: dir.join("proj"),
                },
            ];
            Fixture { dir, places, secret }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.join(rel)
        }

        fn lib(&self, rel: &str) -> PathBuf {
            self.path("home/Library/Application Support").join(format!("{APP_ID_PREFIX}.test")).join(rel)
        }

        fn write(&self, path: &Path, content: &str) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        #[cfg(unix)]
        fn link(&self, target: &Path, rel: &str) {
            let at = self.path(rel);
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, at).unwrap();
        }

        fn plan(&self) -> Vec<PlannedPlace> {
            plan(&self.places, &self.secret)
        }

        /// Every path the preview offers, as the UI confirms them.
        fn confirm_all(&self) -> Vec<String> {
            self.plan()
                .into_iter()
                .flat_map(|p| {
                    let links = p.links.into_iter().filter(|l| l.error.is_none()).map(|l| l.entry);
                    links.chain(p.agents_md.into_iter().map(|b| b.path)).collect::<Vec<_>>()
                })
                .collect()
        }

        fn is_link(&self, rel: &str) -> bool {
            fs::symlink_metadata(self.path(rel)).is_ok_and(|m| m.file_type().is_symlink())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    const BLOCK: &str = "# Team\n\n<!-- puppeteer:rule:style START -->\nUse tabs.\n<!-- puppeteer:rule:style END -->\n";

    #[test]
    fn agents_md_blocks_lose_their_markers_and_keep_their_text() {
        let fx = Fixture::new("blocks");
        fx.write(&fx.path("proj/AGENTS.md"), BLOCK);
        let planned = fx.plan();
        assert_eq!(planned.len(), 1);
        let blocks = planned[0].agents_md.as_ref().unwrap();
        assert_eq!((blocks.blocks.clone(), blocks.git_tracked, blocks.error.is_none()), (vec!["rule:style".to_string()], false, true));

        let outcome = apply(&fx.places, &fx.secret, &fx.confirm_all());
        assert_eq!(outcome.done, vec![fx.path("proj/AGENTS.md").display().to_string()]);
        assert_eq!(fs::read_to_string(fx.path("proj/AGENTS.md")).unwrap(), "# Team\n\nUse tabs.\n");
        assert!(fx.plan().is_empty());
    }

    #[test]
    fn nothing_is_changed_without_being_confirmed() {
        let fx = Fixture::new("unconfirmed");
        fx.write(&fx.path("proj/AGENTS.md"), BLOCK);
        let outcome = apply(&fx.places, &fx.secret, &[]);
        assert!(outcome.done.is_empty() && outcome.failed.is_none());
        assert_eq!(fs::read_to_string(fx.path("proj/AGENTS.md")).unwrap(), BLOCK);
    }

    #[test]
    fn materialize_commands_are_local_only() {
        use crate::remote::guard::is_local_only_command;
        for name in ["library_materialize_preview", "library_materialize"] {
            assert!(is_local_only_command(name), "{name}");
        }
    }

    #[cfg(unix)]
    mod links {
        use super::*;

        /// A library with a skill, a rule, an agent and a command, linked into the project as
        /// Puppeteer does, a global skill link in HOME, plus links that aren't Puppeteer's.
        fn installed(label: &str) -> Fixture {
            let fx = Fixture::new(label);
            fx.write(&fx.lib("skills/pdf/SKILL.md"), "---\ndescription: PDFs\n---\nBody\n");
            fx.write(&fx.lib("skills/pdf/scripts/run.sh"), "echo\n");
            fx.write(&fx.lib("rules/style.md"), "Tabs\n");
            fx.write(&fx.lib("rules/.codex-skills/style/SKILL.md"), "Tabs for Codex\n");
            fx.write(&fx.lib("agents/tester.md"), "Tester\n");
            fx.write(&fx.lib("commands/fix.md"), "Fix\n");
            fx.link(&fx.lib("skills/pdf"), "proj/.claude/skills/pdf");
            fx.link(&fx.lib("rules/style.md"), "proj/.claude/rules/style.md");
            fx.link(&fx.lib("rules/.codex-skills/style"), "proj/.agents/skills/style");
            fx.link(&fx.lib("agents/tester.md"), "proj/.claude/agents/tester.md");
            fx.link(&fx.lib("commands/fix.md"), "proj/.claude/commands/fix.md");
            fx.link(&fx.lib("skills/pdf"), "home/.claude/skills/pdf");
            // Not Puppeteer's: a dotfiles link in HOME, a link inside the project.
            fx.write(&fx.path("home/dotfiles/mine.md"), "Mine\n");
            fx.link(&fx.path("home/dotfiles/mine.md"), "home/.claude/rules/mine.md");
            fx.write(&fx.path("proj/docs/notes.md"), "Notes\n");
            fx.link(&fx.path("proj/docs/notes.md"), "proj/.claude/commands/notes.md");
            fx
        }

        fn entries(planned: &[PlannedPlace]) -> Vec<(LibraryScope, ItemKind, &str, &str, bool)> {
            planned
                .iter()
                .flat_map(|p| p.links.iter().map(move |l| (p.scope, l.kind, l.engine, l.name.as_str(), l.error.is_some())))
                .collect()
        }

        #[test]
        fn preview_lists_only_puppeteers_links() {
            let fx = installed("preview");
            let planned = fx.plan();
            assert_eq!(
                entries(&planned),
                vec![
                    (LibraryScope::Global, ItemKind::Skill, "claude", "pdf", false),
                    (LibraryScope::Project, ItemKind::Skill, "claude", "pdf", false),
                    (LibraryScope::Project, ItemKind::Skill, "codex", "style", false),
                    (LibraryScope::Project, ItemKind::Rule, "claude", "style", false),
                    (LibraryScope::Project, ItemKind::Command, "claude", "fix", false),
                    (LibraryScope::Project, ItemKind::Agent, "claude", "tester", false),
                ]
            );
            let pdf = &planned[1].links[0];
            assert_eq!(pdf.entry, fx.path("proj/.claude/skills/pdf").display().to_string());
            assert_eq!(pdf.target, fx.lib("skills/pdf").display().to_string());
            assert_eq!((planned[1].project_id.as_deref(), planned[1].project_name.as_deref()), (Some("p1"), Some("Proj")));
            // The preview changed nothing.
            assert!(fx.is_link("proj/.claude/skills/pdf") && fx.is_link("home/.claude/skills/pdf"));
        }

        #[test]
        fn apply_turns_each_link_into_its_own_copy_and_a_second_run_does_nothing() {
            let fx = installed("apply");
            fx.write(&fx.path("proj/AGENTS.md"), BLOCK);
            let confirmed = fx.confirm_all();
            let outcome = apply(&fx.places, &fx.secret, &confirmed);
            assert!(outcome.failed.is_none() && outcome.skipped.is_empty(), "{outcome:?}");
            assert_eq!(outcome.done.len(), 7);

            for rel in [
                "home/.claude/skills/pdf",
                "proj/.claude/skills/pdf",
                "proj/.agents/skills/style",
                "proj/.claude/rules/style.md",
                "proj/.claude/agents/tester.md",
                "proj/.claude/commands/fix.md",
            ] {
                assert!(!fx.is_link(rel), "{rel}");
                assert!(fx.path(rel).exists(), "{rel}");
            }
            assert_eq!(fs::read_to_string(fx.path("proj/.claude/skills/pdf/scripts/run.sh")).unwrap(), "echo\n");
            assert_eq!(fs::read_to_string(fx.path("proj/.agents/skills/style/SKILL.md")).unwrap(), "Tabs for Codex\n");
            assert_eq!(fs::read_to_string(fx.path("proj/.claude/rules/style.md")).unwrap(), "Tabs\n");
            assert_eq!(fs::read_to_string(fx.path("proj/AGENTS.md")).unwrap(), "# Team\n\nUse tabs.\n");
            // Each copy is its own: editing one leaves the library and the others alone.
            fs::write(fx.path("proj/.claude/skills/pdf/SKILL.md"), "edited\n").unwrap();
            assert!(fs::read_to_string(fx.lib("skills/pdf/SKILL.md")).unwrap().contains("Body"));
            assert!(fs::read_to_string(fx.path("home/.claude/skills/pdf/SKILL.md")).unwrap().contains("Body"));
            // Links that aren't Puppeteer's stay links; the library stays; no temp copy is left.
            assert!(fx.is_link("home/.claude/rules/mine.md") && fx.is_link("proj/.claude/commands/notes.md"));
            assert!(fx.lib("agents/tester.md").is_file());
            let leftovers: Vec<_> = fs::read_dir(fx.path("proj/.claude/skills")).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(leftovers, vec!["pdf"]);

            // Second run: nothing left to plan, and the old confirmation does nothing.
            assert!(fx.plan().is_empty());
            let again = apply(&fx.places, &fx.secret, &confirmed);
            assert!(again.done.is_empty() && again.failed.is_none());
            assert_eq!(again.skipped.len(), confirmed.len());
        }

        #[test]
        fn git_tracked_links_are_flagged() {
            let fx = installed("git");
            let add = Command::new("git").arg("-C").arg(fx.path("proj")).args(["add", ".claude/rules/style.md"]).status();
            assert!(add.unwrap().success());
            fx.write(&fx.path("proj/AGENTS.md"), BLOCK);
            assert!(Command::new("git").arg("-C").arg(fx.path("proj")).args(["add", "AGENTS.md"]).status().unwrap().success());

            let planned = fx.plan();
            let project = planned.iter().find(|p| p.scope == LibraryScope::Project).unwrap();
            let tracked: Vec<_> = project.links.iter().filter(|l| l.git_tracked).map(|l| l.name.as_str()).collect();
            assert_eq!(tracked, vec!["style"]);
            assert_eq!(project.links.iter().find(|l| l.kind == ItemKind::Rule).map(|l| l.git_tracked), Some(true));
            assert!(project.agents_md.as_ref().unwrap().git_tracked);
        }

        #[test]
        fn a_link_whose_target_is_gone_is_listed_but_left_alone() {
            let fx = installed("dangling");
            fs::remove_file(fx.lib("agents/tester.md")).unwrap();
            let planned = fx.plan();
            let tester = planned.iter().flat_map(|p| &p.links).find(|l| l.name == "tester").unwrap();
            assert!(tester.error.is_some());

            let outcome = apply(&fx.places, &fx.secret, &[tester.entry.clone()]);
            assert!(outcome.done.is_empty() && outcome.failed.is_none());
            assert_eq!(outcome.skipped, vec![tester.entry.clone()]);
            assert!(fx.is_link("proj/.claude/agents/tester.md"));
        }

        #[test]
        fn a_link_reached_twice_is_listed_once() {
            let fx = Fixture::new("twice");
            fx.write(&fx.lib("skills/pdf/SKILL.md"), "Body\n");
            fx.link(&fx.lib("skills/pdf"), "proj/.claude/skills/pdf");
            // Codex reads Claude's skills folder through a folder link inside the project.
            fx.link(Path::new("../.claude/skills"), "proj/.agents/skills");
            assert_eq!(entries(&fx.plan()), vec![(LibraryScope::Project, ItemKind::Skill, "claude", "pdf", false)]);
            let outcome = apply(&fx.places, &fx.secret, &fx.confirm_all());
            assert_eq!(outcome.done.len(), 1);
            assert!(!fx.is_link("proj/.claude/skills/pdf") && fx.is_link("proj/.agents/skills"));
        }

        #[test]
        fn the_first_failure_stops_and_what_was_done_stays_done() {
            let fx = installed("stop");
            let confirmed = fx.confirm_all();
            // The project's rules folder can't take a new file: the rule fails, after the skills.
            let rules = fx.path("proj/.claude/rules");
            let mut perms = fs::metadata(&rules).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
            fs::set_permissions(&rules, perms.clone()).unwrap();

            let outcome = apply(&fx.places, &fx.secret, &confirmed);
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
            fs::set_permissions(&rules, perms).unwrap();

            let failed = outcome.failed.expect("stops at the rule");
            assert_eq!(failed.path, fx.path("proj/.claude/rules/style.md").display().to_string());
            assert_eq!(outcome.done.len(), 3, "{:?}", outcome.done);
            assert!(!fx.is_link("proj/.claude/skills/pdf"));
            assert!(fx.is_link("proj/.claude/rules/style.md"));
            assert!(fx.is_link("proj/.claude/agents/tester.md"), "nothing after the failure");
            // What remains is still offered.
            assert_eq!(fx.plan().iter().map(|p| p.links.len()).sum::<usize>(), 3);
        }
    }
}
