//! V3-50: a new Library item from a template, in the folder the engine reads. Local-only
//! (remote/guard.rs): the file goes into every session's prompt, like `library_write`.
//!
//! Each part of the folder is checked before anything is created in it (as `claude_folder`), so a
//! `.claude` or `.agents` a cloned repo links out of the project gets nothing. An entry that
//! exists is never overwritten.

use super::{folder_allowed, item, open_folder, ItemKind, LibraryItem, LibraryScope, EXISTS, FOLDERS};
use crate::commands::memory::{project_root, write_atomic};
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

/// An item's name is a single path part that no scan skips.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\']) && !name.contains("..")
}

fn kind_of(kind: &str) -> Result<ItemKind, String> {
    match kind {
        "skill" => Ok(ItemKind::Skill),
        "rule" => Ok(ItemKind::Rule),
        "command" => Ok(ItemKind::Command),
        "agent" => Ok(ItemKind::Agent),
        other => Err(format!("Unknown item kind: {other}")),
    }
}

/// The file a new item starts as: a frontmatter `crate::import::frontmatter` reads, and a hint.
fn template(kind: ItemKind, name: &str) -> String {
    let hint = match kind {
        ItemKind::Skill => "Describe what this skill does and when to use it.",
        ItemKind::Rule => "Write the rule the agent must follow.",
        ItemKind::Command => "Write the prompt this command runs.",
        ItemKind::Agent => "Describe this agent's role and how it works.",
    };
    format!("---\nname: {name}\ndescription:\n---\n\n{hint}\n")
}

/// Make `rel` under `base` part by part, checking each part before going into it.
fn ensure_folder(base: &Path, rel: &str, secret: &SecretPaths) -> Result<(), String> {
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let mut done = PathBuf::new();
    for part in Path::new(rel).components() {
        done.push(part);
        let at = base.join(&done);
        if fs::symlink_metadata(&at).is_err() {
            fs::create_dir(&at).map_err(|e| format!("{}: {}", at.display(), e))?;
        }
        let real = fs::canonicalize(&at).map_err(|e| format!("{}: {}", at.display(), e))?;
        if !folder_allowed(&real, &real_base, &done, secret) {
            return Err(format!("{} leads outside {} — Puppeteer won't write there", at.display(), base.display()));
        }
    }
    Ok(())
}

fn create_at(
    base: &Path,
    scope: LibraryScope,
    kind: ItemKind,
    engine: &str,
    name: &str,
    secret: &SecretPaths,
) -> Result<LibraryItem, String> {
    if !valid_name(name) {
        return Err(format!("\"{name}\" is not a valid name"));
    }
    let folder = FOLDERS
        .iter()
        .find(|f| f.kind == kind && f.engine == engine && f.in_scope(scope))
        .ok_or_else(|| format!("No {engine} folder for {kind:?} here"))?;
    ensure_folder(base, folder.rel, secret)?;
    let (_, dir) = open_folder(base, folder, secret)?;
    let entry_name = folder.entry_name(name);
    let entry = dir.join(&entry_name);
    if fs::symlink_metadata(&entry).is_ok() {
        return Err(format!("{EXISTS}: {} already exists", base.join(folder.rel).join(&entry_name).display()));
    }
    let file = folder.file_of(&entry);
    if kind == ItemKind::Skill {
        fs::create_dir(&entry).map_err(|e| format!("{}: {}", entry.display(), e))?;
    }
    write_atomic(&file, template(kind, name).as_bytes()).map_err(|e| format!("{}: {}", file.display(), e))?;
    Ok(item(base, scope, folder, name, &HashSet::new(), secret))
}

/// Local-only. `kind` is `skill` / `rule` / `command` / `agent`; `engine` is `claude` or `codex`.
#[tauri::command]
pub async fn library_create(
    app: AppHandle,
    db: State<'_, Db>,
    scope: LibraryScope,
    project_id: Option<String>,
    kind: String,
    engine: String,
    name: String,
) -> Result<LibraryItem, String> {
    let kind = kind_of(&kind)?;
    let base = match scope {
        LibraryScope::Global => app.path().home_dir().map_err(|e| e.to_string())?,
        LibraryScope::Project => {
            project_root(db.inner(), project_id.as_deref().ok_or("A project is required")?).await?
        }
    };
    let secret = SecretPaths::for_app(&app);
    super::blocking(move || create_at(&base, scope, kind, &engine, &name, &secret)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library_disk::{parse, scan_at, Roots, APP_ID_PREFIX};

    struct Fixture {
        dir: PathBuf,
        roots: Roots,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-library-create-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            let dir = fs::canonicalize(dir).unwrap();
            let home = dir.join("home");
            let app_data = home.join("Library/Application Support").join(format!("{APP_ID_PREFIX}.test"));
            let secret = SecretPaths::new(Some(&home), Some(&app_data));
            let roots = Roots { home, projects: vec![dir.join("proj")], secret };
            Fixture { dir, roots }
        }

        fn create(&self, scope: LibraryScope, kind: ItemKind, engine: &str, name: &str) -> Result<LibraryItem, String> {
            let base = match scope {
                LibraryScope::Global => self.roots.home.clone(),
                LibraryScope::Project => self.dir.join("proj"),
            };
            create_at(&base, scope, kind, engine, name, &self.roots.secret)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn creates_each_kind_where_the_engine_reads_it() {
        let f = Fixture::new();
        let cases = [
            (ItemKind::Skill, "claude", ".claude/skills/a/SKILL.md"),
            (ItemKind::Skill, "codex", ".agents/skills/a/SKILL.md"),
            (ItemKind::Rule, "claude", ".claude/rules/a.md"),
            (ItemKind::Command, "claude", ".claude/commands/a.md"),
            (ItemKind::Agent, "claude", ".claude/agents/a.md"),
        ];
        for (kind, engine, rel) in cases {
            let it = f.create(LibraryScope::Project, kind, engine, "a").unwrap();
            assert_eq!(it.path, f.dir.join("proj").join(rel).display().to_string());
            assert!(Path::new(&it.path).is_file());
            assert!(it.error.is_none() && it.hash.is_some());
            // Resolves as an item of the project, so the editor can open it.
            assert!(parse(Path::new(&it.path), &f.roots).is_ok());
        }
        let it = f.create(LibraryScope::Global, ItemKind::Command, "codex", "a").unwrap();
        assert!(f.roots.home.join(".codex/prompts/a.md").is_file());
        assert_eq!(it.name, "a");
    }

    #[test]
    fn template_is_read_back_by_a_scan() {
        let f = Fixture::new();
        f.create(LibraryScope::Project, ItemKind::Skill, "claude", "tmp-skill").unwrap();
        f.create(LibraryScope::Project, ItemKind::Rule, "claude", "tmp-rule").unwrap();
        let items = scan_at(&f.dir.join("proj"), LibraryScope::Project, &f.roots.secret);
        let mut names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["tmp-rule", "tmp-skill"]);
        assert!(items.iter().all(|i| i.error.is_none()));
        let text = fs::read_to_string(&items[0].path).unwrap();
        assert!(text.starts_with("---\nname: "));
    }

    #[test]
    fn refuses_an_existing_entry_and_keeps_its_content() {
        let f = Fixture::new();
        let it = f.create(LibraryScope::Project, ItemKind::Rule, "claude", "dup").unwrap();
        fs::write(&it.path, "mine").unwrap();
        let err = f.create(LibraryScope::Project, ItemKind::Rule, "claude", "dup").unwrap_err();
        assert!(err.starts_with(EXISTS), "{err}");
        assert_eq!(fs::read_to_string(&it.path).unwrap(), "mine");
        f.create(LibraryScope::Project, ItemKind::Skill, "claude", "dup").unwrap();
        assert!(f.create(LibraryScope::Project, ItemKind::Skill, "claude", "dup").unwrap_err().starts_with(EXISTS));
    }

    #[test]
    fn refuses_bad_names() {
        let f = Fixture::new();
        for name in ["", ".hidden", "a/b", "a\\b", "..", "a..b"] {
            assert!(f.create(LibraryScope::Project, ItemKind::Rule, "claude", name).is_err(), "{name}");
        }
        assert!(!f.dir.join("proj/.claude").exists());
    }

    #[test]
    fn codex_command_has_no_project_folder() {
        let f = Fixture::new();
        assert!(f.create(LibraryScope::Project, ItemKind::Command, "codex", "a").is_err());
        assert!(f.create(LibraryScope::Project, ItemKind::Rule, "codex", "a").is_err());
        assert!(!f.dir.join("proj/.codex").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_folder_linked_out_of_the_project() {
        let f = Fixture::new();
        let outside = f.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        for dot in [".claude", ".agents"] {
            std::os::unix::fs::symlink(&outside, f.dir.join("proj").join(dot)).unwrap();
        }
        assert!(f.create(LibraryScope::Project, ItemKind::Rule, "claude", "x").is_err());
        assert!(f.create(LibraryScope::Project, ItemKind::Skill, "codex", "x").is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }

    #[test]
    fn kind_names_map() {
        assert_eq!(kind_of("skill").unwrap(), ItemKind::Skill);
        assert!(kind_of("memory").is_err());
    }
}
