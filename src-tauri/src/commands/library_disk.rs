//! Library (D1): the skills, rules, commands and agents the CLIs read, taken straight from the
//! disk. The file is the item: no table, no copy in Puppeteer's app data. Global is HOME's
//! folders, Project is the project's (see [`FOLDERS`]). The old libraries are left untouched.
//!
//! The frontend passes back the `path` a scan returned. [`parse`] accepts only the shape of an
//! item in one of [`FOLDERS`] under HOME or a live project (`<folder>/<name>/SKILL.md`,
//! `<folder>/<name>.md`), never `..`. A link is followed only where it may lead: inside its
//! folder, into Puppeteer's own library (an item the app installed), or elsewhere in the same
//! HOME / project but a credential store. A project link out of the project is refused, whatever
//! a cloned repo planted.
//!
//! Writes are atomic and refused when the file changed since it was read, as Memory's. A delete
//! removes a link and never what it points at; a real file or skill folder goes to the OS trash.
//! `library_scan` and `library_read` may run from a remote desktop; the commands that change files
//! are local-only (remote/guard.rs): their content goes into every session's prompt.

use crate::commands::files::copy_recursive;
use crate::commands::memory::{hash, project_root, write_atomic};
use crate::commands::skill_effective::tracked_names;
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager, State};

pub mod materialize;

/// Error prefix of a write refused because the file changed on disk since it was read.
pub const CONFLICT: &str = "library_conflict";
/// Error prefix of a duplicate refused because Global already has that name; the UI asks to
/// overwrite.
pub const EXISTS: &str = "library_exists";

/// Same limit as Memory: larger files aren't loaded into the editor.
const MAX_BYTES: u64 = 1024 * 1024;

/// Puppeteer's app data folders (any build) are named after this; their library folders below.
const APP_ID_PREFIX: &str = "io.github.v9n8.puppeteer";
const LIBRARY_DIRS: &[&str] = &["skills", "rules", "agents", "commands"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Skill,
    Rule,
    Command,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    File,
    /// Committed to the project's repo: an edit ends up in a commit.
    GitTracked,
    /// A link Puppeteer made into its old library: an edit changes every project using it.
    PuppeteerLink,
    Symlink,
}

/// A folder the CLIs read items of one kind from, relative to HOME or a project.
#[derive(Debug)]
struct Folder {
    kind: ItemKind,
    engine: &'static str,
    rel: &'static str,
    /// Also read in a project, not only in HOME.
    project: bool,
}

/// The skill folders match `skills::SKILL_ENGINES`. Codex keeps custom prompts in HOME only;
/// its project rules are blocks in `AGENTS.md`, not files.
const FOLDERS: &[Folder] = &[
    Folder { kind: ItemKind::Skill, engine: "claude", rel: ".claude/skills", project: true },
    Folder { kind: ItemKind::Skill, engine: "codex", rel: ".agents/skills", project: true },
    Folder { kind: ItemKind::Rule, engine: "claude", rel: ".claude/rules", project: true },
    Folder { kind: ItemKind::Command, engine: "claude", rel: ".claude/commands", project: true },
    Folder { kind: ItemKind::Command, engine: "codex", rel: ".codex/prompts", project: false },
    Folder { kind: ItemKind::Agent, engine: "claude", rel: ".claude/agents", project: true },
];

impl Folder {
    fn in_scope(&self, scope: LibraryScope) -> bool {
        scope == LibraryScope::Global || self.project
    }

    /// The entry of item `name` in the folder: a skill's folder, or the markdown file.
    fn entry_name(&self, name: &str) -> String {
        match self.kind {
            ItemKind::Skill => name.to_string(),
            _ => format!("{name}.md"),
        }
    }

    /// The file an editor opens for the entry at `entry`.
    fn file_of(&self, entry: &Path) -> PathBuf {
        match self.kind {
            ItemKind::Skill => entry.join("SKILL.md"),
            _ => entry.to_path_buf(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryItem {
    pub kind: ItemKind,
    pub engine: &'static str,
    pub scope: LibraryScope,
    pub name: String,
    /// The file to open, and the id every other command takes (a skill's `SKILL.md`).
    pub path: String,
    /// What a delete removes: the skill's folder, or the file.
    pub entry: String,
    /// From the frontmatter.
    pub description: String,
    /// Milliseconds since the epoch.
    pub mtime: Option<i64>,
    /// SHA-256 of the file, as `library_write` expects it back.
    pub hash: Option<String>,
    pub status: ItemStatus,
    /// Where the entry really is, when it is a link.
    pub link_target: Option<String>,
    /// Why it can't be opened here (a link out of the project, not UTF-8, too large).
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LibraryFile {
    pub path: String,
    pub link_target: Option<String>,
    pub content: String,
    pub size: u64,
    pub hash: String,
}

/// Where items may be: HOME (Global) and every live project.
struct Roots {
    home: PathBuf,
    projects: Vec<PathBuf>,
    secret: SecretPaths,
}

/// An item path, checked for shape only.
#[derive(Debug)]
struct Located {
    folder: &'static Folder,
    scope: LibraryScope,
    base: PathBuf,
    name: String,
}

/// The item `path` names, when it has the shape of one under HOME or a live project.
fn parse(path: &Path, roots: &Roots) -> Result<Located, String> {
    let refused = || format!("{} is not a skill, rule, command or agent Puppeteer may open", path.display());
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err(refused());
    }
    let bases = std::iter::once((&roots.home, LibraryScope::Global))
        .chain(roots.projects.iter().map(|p| (p, LibraryScope::Project)));
    for (base, scope) in bases {
        let Ok(rel) = path.strip_prefix(base) else { continue };
        for folder in FOLDERS.iter().filter(|f| f.in_scope(scope)) {
            let Ok(rest) = rel.strip_prefix(folder.rel) else { continue };
            let parts: Vec<&OsStr> = rest.iter().collect();
            let name = match (folder.kind, parts.as_slice()) {
                (ItemKind::Skill, [name, file]) if *file == "SKILL.md" => name.to_str(),
                (ItemKind::Skill, _) => None,
                (_, [file]) => file.to_str().and_then(|f| f.strip_suffix(".md")),
                _ => None,
            };
            if let Some(name) = name.filter(|n| !n.is_empty() && !n.starts_with('.')) {
                return Ok(Located { folder, scope, base: base.clone(), name: name.to_string() });
            }
        }
    }
    Err(refused())
}

/// Whether `path` (canonical) is in a Puppeteer build's skill / rule / agent / command library.
fn in_puppeteer_library(path: &Path) -> bool {
    path.ancestors().any(|a| {
        a.file_name().is_some_and(|n| n.to_string_lossy().starts_with(APP_ID_PREFIX))
            && path
                .strip_prefix(a)
                .ok()
                .and_then(|r| r.iter().next())
                .is_some_and(|first| LIBRARY_DIRS.iter().any(|d| first == *d))
    })
}

/// Whether the folder `rel` of the base, really at `real`, lies where it may.
fn folder_allowed(real: &Path, real_base: &Path, rel: &Path, secret: &SecretPaths) -> bool {
    // `~/.codex/prompts` lies in a credential store (`~/.codex`): reached without a link, it is the
    // tool's own folder.
    real == real_base.join(rel) || (real.starts_with(real_base) && !secret.denies(real))
}

/// The real folder, with the real base, when it lies where it may.
fn open_folder(base: &Path, folder: &Folder, secret: &SecretPaths) -> Result<(PathBuf, PathBuf), String> {
    let at = base.join(folder.rel);
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let real = fs::canonicalize(&at).map_err(|_| format!("Folder not found: {}", at.display()))?;
    if folder_allowed(&real, &real_base, Path::new(folder.rel), secret) {
        Ok((real_base, real))
    } else {
        Err(format!("{} leads outside {} — Puppeteer won't open it", at.display(), base.display()))
    }
}

/// The real `.claude/` folder of `kind` in HOME or a project, where an import from another tool
/// writes (UX-56); with `create`, made when missing. Each part is checked before anything is
/// created in it, so a `.claude` a cloned repo links elsewhere gets nothing.
pub(crate) fn claude_folder(base: &Path, kind: ItemKind, secret: &SecretPaths, create: bool) -> Result<PathBuf, String> {
    let folder = FOLDERS
        .iter()
        .find(|f| f.kind == kind && f.engine == "claude")
        .ok_or_else(|| format!("No Claude folder for {kind:?}"))?;
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let mut rel = PathBuf::new();
    for part in Path::new(folder.rel).components() {
        rel.push(part);
        let at = base.join(&rel);
        if fs::symlink_metadata(&at).is_err() {
            if !create {
                return Err(format!("Folder not found: {}", at.display()));
            }
            fs::create_dir(&at).map_err(|e| format!("{}: {}", at.display(), e))?;
        }
        let real = fs::canonicalize(&at).map_err(|e| format!("{}: {}", at.display(), e))?;
        if !folder_allowed(&real, &real_base, &rel, secret) {
            return Err(format!("{} leads outside {} — Puppeteer won't write there", at.display(), base.display()));
        }
    }
    open_folder(base, folder, secret).map(|(_, dir)| dir)
}

/// An item's entry on disk.
#[derive(Debug)]
struct Entry {
    /// In the real folder: what a delete acts on.
    at: PathBuf,
    /// `at` with links followed.
    real: PathBuf,
    /// The file to read and write, links followed.
    file: PathBuf,
    link: bool,
}

fn open_entry(base: &Path, folder: &Folder, name: &str, secret: &SecretPaths) -> Result<Entry, String> {
    let (real_base, dir) = open_folder(base, folder, secret)?;
    let entry_name = folder.entry_name(name);
    let shown = base.join(folder.rel).join(&entry_name);
    let at = dir.join(&entry_name);
    let meta = fs::symlink_metadata(&at).map_err(|_| format!("Not found: {}", shown.display()))?;
    let real = fs::canonicalize(&at)
        .map_err(|_| format!("{} is a link to something that doesn't exist", shown.display()))?;
    let file = fs::canonicalize(folder.file_of(&real))
        .map_err(|_| format!("Not found: {}", folder.file_of(&shown).display()))?;
    for p in [&real, &file] {
        let allowed = p.starts_with(&dir)
            || in_puppeteer_library(p)
            || (p.starts_with(&real_base) && !secret.denies(p));
        if !allowed {
            return Err(format!("{} leads outside {} — Puppeteer won't open it", shown.display(), base.display()));
        }
    }
    if !fs::metadata(&file).is_ok_and(|m| m.is_file()) {
        return Err(format!("{} is not a file", folder.file_of(&shown).display()));
    }
    Ok(Entry { at, real, file, link: meta.file_type().is_symlink() })
}

/// The file's text, refused when too large or not UTF-8 (a save would rewrite it).
fn read_text(file: &Path) -> Result<String, String> {
    let meta = fs::metadata(file).map_err(|e| format!("{}: {}", file.display(), e))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("{} is too large to edit here ({} bytes, limit {MAX_BYTES})", file.display(), meta.len()));
    }
    let bytes = fs::read(file).map_err(|e| format!("{}: {}", file.display(), e))?;
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8 text", file.display()))
}

fn mtime_ms(file: &Path) -> Option<i64> {
    let modified = fs::metadata(file).and_then(|m| m.modified()).ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn item(
    base: &Path,
    scope: LibraryScope,
    folder: &'static Folder,
    name: &str,
    tracked: &HashSet<String>,
    secret: &SecretPaths,
) -> LibraryItem {
    let entry_name = folder.entry_name(name);
    let shown = base.join(folder.rel).join(&entry_name);
    let mut it = LibraryItem {
        kind: folder.kind,
        engine: folder.engine,
        scope,
        name: name.to_string(),
        path: folder.file_of(&shown).display().to_string(),
        entry: shown.display().to_string(),
        description: String::new(),
        mtime: None,
        hash: None,
        status: if tracked.contains(&entry_name) { ItemStatus::GitTracked } else { ItemStatus::File },
        link_target: None,
        error: None,
    };
    match open_entry(base, folder, name, secret) {
        Ok(entry) => {
            if entry.link {
                it.status =
                    if in_puppeteer_library(&entry.real) { ItemStatus::PuppeteerLink } else { ItemStatus::Symlink };
                it.link_target = Some(entry.real.display().to_string());
            }
            it.mtime = mtime_ms(&entry.file);
            match read_text(&entry.file) {
                Ok(content) => {
                    it.description = crate::import::frontmatter(&content).description;
                    it.hash = Some(hash(content.as_bytes()));
                }
                Err(e) => it.error = Some(e),
            }
        }
        Err(e) => {
            if fs::symlink_metadata(&shown).is_ok_and(|m| m.file_type().is_symlink()) {
                it.status = ItemStatus::Symlink;
            }
            it.error = Some(e);
        }
    }
    it
}

/// Every item of `scope`'s folders under `base`, by folder then name.
fn scan_at(base: &Path, scope: LibraryScope, secret: &SecretPaths) -> Vec<LibraryItem> {
    let mut out = Vec::new();
    for folder in FOLDERS.iter().filter(|f| f.in_scope(scope)) {
        let Ok((_, dir)) = open_folder(base, folder, secret) else { continue };
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let entry_name = e.file_name().to_str()?.to_string();
                if entry_name.starts_with('.') || !folder.file_of(&e.path()).is_file() {
                    return None;
                }
                match folder.kind {
                    ItemKind::Skill => Some(entry_name),
                    _ => entry_name.strip_suffix(".md").filter(|n| !n.is_empty()).map(str::to_string),
                }
            })
            .collect();
        names.sort();
        let tracked = match scope {
            LibraryScope::Project => tracked_names(base, folder.rel),
            LibraryScope::Global => HashSet::new(),
        };
        out.extend(names.iter().map(|name| item(base, scope, folder, name, &tracked, secret)));
    }
    out
}

fn read_at(loc: &Located, secret: &SecretPaths) -> Result<LibraryFile, String> {
    let entry = open_entry(&loc.base, loc.folder, &loc.name, secret)?;
    let content = read_text(&entry.file)?;
    Ok(LibraryFile {
        path: loc.folder.file_of(&loc.base.join(loc.folder.rel).join(loc.folder.entry_name(&loc.name))).display().to_string(),
        link_target: entry.link.then(|| entry.real.display().to_string()),
        size: content.len() as u64,
        hash: hash(content.as_bytes()),
        content,
    })
}

fn write_at(loc: &Located, content: &str, expected_hash: &str, secret: &SecretPaths) -> Result<LibraryFile, String> {
    if content.len() as u64 > MAX_BYTES {
        return Err(format!("Content is too large ({} bytes, limit {MAX_BYTES})", content.len()));
    }
    let entry = open_entry(&loc.base, loc.folder, &loc.name, secret)?;
    let current = fs::read(&entry.file).map_err(|e| format!("{}: {}", entry.file.display(), e))?;
    if hash(&current) != expected_hash {
        return Err(format!("{CONFLICT}: {} changed on disk since it was opened", entry.file.display()));
    }
    write_atomic(&entry.file, content.as_bytes()).map_err(|e| format!("{}: {}", entry.file.display(), e))?;
    read_at(loc, secret)
}

/// Remove the link at `at`, never what it points at (a link to a folder is a directory entry on
/// Windows).
pub(crate) fn remove_link(at: &Path) -> Result<(), String> {
    fs::remove_file(at).or_else(|_| fs::remove_dir(at)).map_err(|e| format!("{}: {}", at.display(), e))
}

pub(crate) type Trash<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

pub(crate) fn os_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| format!("Delete failed: {e}"))
}

/// A link: removed. A file or a skill's folder: to the trash.
fn delete_at(loc: &Located, secret: &SecretPaths, trash: Trash) -> Result<(), String> {
    let entry = open_entry(&loc.base, loc.folder, &loc.name, secret)?;
    if entry.link { remove_link(&entry.at) } else { trash(&entry.at) }
}

/// Copy a project's item into HOME's folder of the same kind. A link is copied as what it points
/// at, so Global gets its own file.
fn duplicate_at(loc: &Located, roots: &Roots, overwrite: bool, trash: Trash) -> Result<LibraryItem, String> {
    if loc.scope != LibraryScope::Project {
        return Err("Only a project's item can be duplicated to Global".to_string());
    }
    let source = open_entry(&loc.base, loc.folder, &loc.name, &roots.secret)?;
    let home_dir = roots.home.join(loc.folder.rel);
    fs::create_dir_all(&home_dir).map_err(|e| format!("{}: {}", home_dir.display(), e))?;
    let (_, dir) = open_folder(&roots.home, loc.folder, &roots.secret)?;
    let entry_name = loc.folder.entry_name(&loc.name);
    let dest = dir.join(&entry_name);
    let shown = home_dir.join(&entry_name);
    if let Ok(meta) = fs::symlink_metadata(&dest) {
        if fs::canonicalize(&dest).is_ok_and(|real| real == source.real) {
            return Err(format!("{} is already the global one", shown.display()));
        }
        if !overwrite {
            return Err(format!("{EXISTS}: {} already exists", shown.display()));
        }
        if meta.file_type().is_symlink() {
            remove_link(&dest)?;
        } else {
            trash(&dest)?;
        }
    }
    copy_recursive(&source.real, &dest)?;
    Ok(item(&roots.home, LibraryScope::Global, loc.folder, &loc.name, &HashSet::new(), &roots.secret))
}

async fn roots(app: &AppHandle, db: &Db) -> Result<Roots, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let projects = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE deleted_at IS NULL")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Roots { home, projects: projects.into_iter().map(PathBuf::from).collect(), secret: SecretPaths::for_app(app) })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Remote-callable, read-only: the items of HOME (`global`) or of a project.
#[tauri::command]
pub async fn library_scan(
    app: AppHandle,
    db: State<'_, Db>,
    scope: LibraryScope,
    project_id: Option<String>,
) -> Result<Vec<LibraryItem>, String> {
    let base = match scope {
        LibraryScope::Global => app.path().home_dir().map_err(|e| e.to_string())?,
        LibraryScope::Project => {
            project_root(db.inner(), project_id.as_deref().ok_or("A project is required")?).await?
        }
    };
    let secret = SecretPaths::for_app(&app);
    blocking(move || {
        if !base.is_dir() {
            return Err(format!("Folder not found: {}", base.display()));
        }
        Ok(scan_at(&base, scope, &secret))
    })
    .await
}

/// Remote-callable, read-only.
#[tauri::command]
pub async fn library_read(app: AppHandle, db: State<'_, Db>, path: String) -> Result<LibraryFile, String> {
    let roots = roots(&app, db.inner()).await?;
    blocking(move || read_at(&parse(Path::new(&path), &roots)?, &roots.secret)).await
}

#[tauri::command]
pub async fn library_write(
    app: AppHandle,
    db: State<'_, Db>,
    path: String,
    content: String,
    expected_hash: String,
) -> Result<LibraryFile, String> {
    let roots = roots(&app, db.inner()).await?;
    blocking(move || write_at(&parse(Path::new(&path), &roots)?, &content, &expected_hash, &roots.secret)).await
}

#[tauri::command]
pub async fn library_delete(app: AppHandle, db: State<'_, Db>, path: String) -> Result<(), String> {
    let roots = roots(&app, db.inner()).await?;
    blocking(move || delete_at(&parse(Path::new(&path), &roots)?, &roots.secret, &os_trash)).await
}

#[tauri::command]
pub async fn library_duplicate_to_global(
    app: AppHandle,
    db: State<'_, Db>,
    path: String,
    overwrite: bool,
) -> Result<LibraryItem, String> {
    let roots = roots(&app, db.inner()).await?;
    blocking(move || duplicate_at(&parse(Path::new(&path), &roots)?, &roots, overwrite, &os_trash)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A temp HOME and a project (a git repo) under one canonical folder.
    struct Fixture {
        dir: PathBuf,
        roots: Roots,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-library-{}-{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            let dir = fs::canonicalize(dir).unwrap();
            let git = Command::new("git").arg("-C").arg(dir.join("proj")).args(["init", "-q"]).status();
            assert!(git.unwrap().success());
            let home = dir.join("home");
            let app_data = home.join("Library/Application Support").join(format!("{APP_ID_PREFIX}.test"));
            let secret = SecretPaths::new(Some(&home), Some(&app_data));
            let roots = Roots { home, projects: vec![dir.join("proj")], secret };
            Fixture { dir, roots }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.join(rel)
        }

        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.path(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn locate(&self, rel: &str) -> Result<Located, String> {
            parse(&self.path(rel), &self.roots)
        }

        fn scan(&self, base: &str, scope: LibraryScope) -> Vec<LibraryItem> {
            scan_at(&self.path(base), scope, &self.roots.secret)
        }

        /// Moves into `<dir>/trash/` instead of the OS trash.
        fn trash(&self) -> impl Fn(&Path) -> Result<(), String> + '_ {
            move |p: &Path| {
                let bin = self.path("trash");
                fs::create_dir_all(&bin).unwrap();
                fs::rename(p, bin.join(p.file_name().unwrap())).map_err(|e| e.to_string())
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn skill(description: &str) -> String {
        format!("---\nname: x\ndescription: {description}\n---\nBody\n")
    }

    /// (kind, engine, name, status, has an error) of each item.
    fn summary(items: &[LibraryItem]) -> Vec<(ItemKind, &str, &str, ItemStatus, bool)> {
        items.iter().map(|i| (i.kind, i.engine, i.name.as_str(), i.status, i.error.is_some())).collect()
    }

    #[test]
    fn global_scan_reads_every_home_folder() {
        use ItemKind::*;
        use ItemStatus::File;
        let fx = Fixture::new("global");
        fx.write("home/.claude/skills/review/SKILL.md", &skill("Reviews a diff"));
        fx.write("home/.agents/skills/deploy/SKILL.md", &skill("Ships it"));
        fx.write("home/.claude/rules/style.md", "---\ndescription: Tabs\n---\n");
        fx.write("home/.claude/commands/fix.md", "Fix it\n");
        fx.write("home/.codex/prompts/plan.md", "Plan\n");
        fx.write("home/.claude/agents/tester.md", "---\nname: tester\ndescription: Runs tests\n---\n");
        // Not items: no SKILL.md, not markdown, hidden.
        fs::create_dir_all(fx.path("home/.claude/skills/empty")).unwrap();
        fx.write("home/.claude/rules/notes.txt", "x");
        fx.write("home/.claude/commands/.draft.md", "x");

        let items = fx.scan("home", LibraryScope::Global);
        assert_eq!(
            summary(&items),
            vec![
                (Skill, "claude", "review", File, false),
                (Skill, "codex", "deploy", File, false),
                (Rule, "claude", "style", File, false),
                (Command, "claude", "fix", File, false),
                (Command, "codex", "plan", File, false),
                (Agent, "claude", "tester", File, false),
            ]
        );
        let review = &items[0];
        assert_eq!(review.path, fx.path("home/.claude/skills/review/SKILL.md").display().to_string());
        assert_eq!(review.entry, fx.path("home/.claude/skills/review").display().to_string());
        assert_eq!(review.description, "Reviews a diff");
        assert_eq!(review.hash.as_deref(), Some(hash(skill("Reviews a diff").as_bytes()).as_str()));
        assert!(review.mtime.is_some());
        assert!(items.iter().all(|i| i.scope == LibraryScope::Global));
    }

    #[test]
    fn project_scan_marks_git_tracked_files_and_skips_home_only_folders() {
        let fx = Fixture::new("project");
        fx.write("proj/.claude/rules/team.md", "Team\n");
        fx.write("proj/.claude/agents/mine.md", "Mine\n");
        fx.write("proj/.codex/prompts/plan.md", "Codex reads prompts from HOME only\n");
        let add = Command::new("git").arg("-C").arg(fx.path("proj")).args(["add", ".claude/rules/team.md"]).status();
        assert!(add.unwrap().success());

        let items = fx.scan("proj", LibraryScope::Project);
        assert_eq!(
            summary(&items),
            vec![
                (ItemKind::Rule, "claude", "team", ItemStatus::GitTracked, false),
                (ItemKind::Agent, "claude", "mine", ItemStatus::File, false),
            ]
        );
    }

    #[test]
    fn write_is_refused_when_the_file_changed_since_it_was_read() {
        let fx = Fixture::new("write");
        let file = fx.write("proj/.claude/commands/fix.md", "v1\n");
        let loc = fx.locate("proj/.claude/commands/fix.md").unwrap();
        let opened = read_at(&loc, &fx.roots.secret).unwrap();
        assert_eq!((opened.content.as_str(), opened.size), ("v1\n", 3));

        let saved = write_at(&loc, "v2\n", &opened.hash, &fx.roots.secret).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2\n");
        // A save from the stale buffer: refused, the newer file kept.
        let err = write_at(&loc, "stale\n", &opened.hash, &fx.roots.secret).unwrap_err();
        assert!(err.starts_with(CONFLICT), "{err}");
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2\n");
        // Changed outside the app.
        fs::write(&file, "theirs\n").unwrap();
        assert!(write_at(&loc, "mine\n", &saved.hash, &fx.roots.secret).unwrap_err().starts_with(CONFLICT));
        assert_eq!(fs::read_to_string(&file).unwrap(), "theirs\n");
    }

    #[test]
    fn non_utf8_and_oversized_files_are_listed_but_not_opened() {
        let fx = Fixture::new("bad");
        fx.write("proj/.claude/rules/sjis.md", "");
        fs::write(fx.path("proj/.claude/rules/sjis.md"), b"\x93\xfa\x96\x7b\n").unwrap();
        fx.write("proj/.claude/rules/big.md", &"x".repeat(MAX_BYTES as usize + 1));
        let items = fx.scan("proj", LibraryScope::Project);
        assert!(items.iter().all(|i| i.error.is_some() && i.hash.is_none()), "{items:?}");
        assert!(read_at(&fx.locate("proj/.claude/rules/sjis.md").unwrap(), &fx.roots.secret).is_err());
        assert!(read_at(&fx.locate("proj/.claude/rules/big.md").unwrap(), &fx.roots.secret).is_err());
    }

    #[test]
    fn only_item_paths_under_home_or_a_live_project_are_accepted() {
        let fx = Fixture::new("paths");
        fx.write("home/.claude/.credentials.json", "SECRET");
        fx.write("home/.claude/settings.json", "{}");
        fx.write("home/.codex/auth.json", "SECRET");
        fx.write("elsewhere/.claude/rules/r.md", "x");
        for rel in [
            "home/.claude/.credentials.json",
            "home/.claude/settings.json",
            "home/.codex/auth.json",
            "home/.claude/skills/review",
            "home/.claude/skills/review/notes.md",
            "home/.claude/skills/review/SKILL.md/x",
            "home/.claude/rules/sub/r.md",
            "home/.claude/rules/.hidden.md",
            "home/.claude/rules/../../.codex/auth.json",
            "proj/.codex/prompts/plan.md",
            "elsewhere/.claude/rules/r.md",
        ] {
            assert!(fx.locate(rel).is_err(), "{rel}");
        }
        assert!(parse(Path::new(".claude/rules/r.md"), &fx.roots).is_err(), "relative");

        let rule = fx.locate("proj/.claude/rules/r.md").unwrap();
        assert_eq!((rule.folder.kind, rule.scope, rule.name.as_str()), (ItemKind::Rule, LibraryScope::Project, "r"));
        let prompt = fx.locate("home/.codex/prompts/plan.md").unwrap();
        assert_eq!((prompt.folder.engine, prompt.scope), ("codex", LibraryScope::Global));
        let skill = fx.locate("home/.agents/skills/deploy/SKILL.md").unwrap();
        assert_eq!((skill.folder.engine, skill.name.as_str()), ("codex", "deploy"));
    }

    #[test]
    fn codex_prompts_open_although_codex_home_is_a_credential_store() {
        let fx = Fixture::new("codex");
        fx.write("home/.codex/prompts/plan.md", "Plan\n");
        let loc = fx.locate("home/.codex/prompts/plan.md").unwrap();
        assert_eq!(read_at(&loc, &fx.roots.secret).unwrap().content, "Plan\n");
    }

    #[test]
    fn duplicate_to_global_asks_before_overwriting() {
        let fx = Fixture::new("dup");
        fx.write("proj/.claude/skills/review/SKILL.md", &skill("Project's"));
        fx.write("proj/.claude/skills/review/scripts/run.sh", "echo\n");
        let loc = fx.locate("proj/.claude/skills/review/SKILL.md").unwrap();
        let trash = fx.trash();

        let copied = duplicate_at(&loc, &fx.roots, false, &trash).unwrap();
        assert_eq!((copied.scope, copied.description.as_str()), (LibraryScope::Global, "Project's"));
        assert_eq!(fs::read_to_string(fx.path("home/.claude/skills/review/scripts/run.sh")).unwrap(), "echo\n");

        // Same name in Global: a distinct error the UI asks on, nothing changed.
        fx.write("home/.claude/skills/review/SKILL.md", &skill("Global's"));
        let err = duplicate_at(&loc, &fx.roots, false, &trash).unwrap_err();
        assert!(err.starts_with(EXISTS), "{err}");
        assert!(fs::read_to_string(fx.path("home/.claude/skills/review/SKILL.md")).unwrap().contains("Global's"));

        // Overwrite: the old global folder goes to the trash first.
        duplicate_at(&loc, &fx.roots, true, &trash).unwrap();
        assert!(fs::read_to_string(fx.path("home/.claude/skills/review/SKILL.md")).unwrap().contains("Project's"));
        assert!(fs::read_to_string(fx.path("trash/review/SKILL.md")).unwrap().contains("Global's"));

        // A markdown item, into HOME's folder of the same kind (created when missing).
        fx.write("proj/.claude/agents/tester.md", "Tester\n");
        duplicate_at(&fx.locate("proj/.claude/agents/tester.md").unwrap(), &fx.roots, false, &trash).unwrap();
        assert_eq!(fs::read_to_string(fx.path("home/.claude/agents/tester.md")).unwrap(), "Tester\n");

        // A global item has nowhere to go.
        let global = fx.locate("home/.claude/agents/tester.md").unwrap();
        assert!(duplicate_at(&global, &fx.roots, true, &trash).is_err());
    }

    #[test]
    fn deleting_a_file_or_a_skill_moves_it_to_the_trash() {
        let fx = Fixture::new("delete");
        fx.write("proj/.claude/skills/review/SKILL.md", &skill("d"));
        fx.write("proj/.claude/skills/review/notes.md", "n");
        fx.write("home/.claude/commands/fix.md", "Fix\n");
        let trash = fx.trash();

        delete_at(&fx.locate("proj/.claude/skills/review/SKILL.md").unwrap(), &fx.roots.secret, &trash).unwrap();
        assert!(!fx.path("proj/.claude/skills/review").exists());
        assert!(fx.path("trash/review/notes.md").is_file(), "the whole skill folder");

        delete_at(&fx.locate("home/.claude/commands/fix.md").unwrap(), &fx.roots.secret, &trash).unwrap();
        assert!(!fx.path("home/.claude/commands/fix.md").exists());
        assert!(fx.path("trash/fix.md").is_file());

        // Already gone.
        assert!(delete_at(&fx.locate("home/.claude/commands/fix.md").unwrap(), &fx.roots.secret, &trash).is_err());
    }

    #[test]
    fn library_commands_have_their_remote_rule() {
        use crate::remote::guard::is_local_only_command;
        for name in ["library_write", "library_delete", "library_duplicate_to_global"] {
            assert!(is_local_only_command(name), "{name}");
        }
        for name in ["library_scan", "library_read"] {
            assert!(!is_local_only_command(name), "{name}");
        }
    }

    #[cfg(unix)]
    mod links {
        use super::*;
        use std::os::unix::fs::symlink;

        fn library(fx: &Fixture) -> PathBuf {
            fx.path("home/Library/Application Support").join(format!("{APP_ID_PREFIX}.test"))
        }

        #[test]
        fn links_are_told_apart_and_followed_only_where_they_may_lead() {
            let fx = Fixture::new("links");
            let lib = library(&fx);
            fx.write(&format!("{}/skills/pdf/SKILL.md", lib.strip_prefix(&fx.dir).unwrap().display()), &skill("Library"));
            fs::create_dir_all(fx.path("proj/.claude/skills")).unwrap();
            fs::create_dir_all(fx.path("proj/.claude/commands")).unwrap();
            fs::create_dir_all(fx.path("proj/.claude/rules")).unwrap();
            // Puppeteer's own link into its library.
            symlink(lib.join("skills/pdf"), fx.path("proj/.claude/skills/pdf")).unwrap();
            // A link inside the project.
            fx.write("proj/docs/fix.md", "Fix\n");
            symlink(fx.path("proj/docs/fix.md"), fx.path("proj/.claude/commands/fix.md")).unwrap();
            // A link out of the project, and one into a credential store.
            fx.write("outside/leak.md", "SECRET");
            symlink(fx.path("outside/leak.md"), fx.path("proj/.claude/rules/leak.md")).unwrap();
            fx.write("home/.ssh/key.md", "SECRET");
            fs::create_dir_all(fx.path("home/.claude/rules")).unwrap();
            symlink(fx.path("home/.ssh/key.md"), fx.path("home/.claude/rules/key.md")).unwrap();
            // A dotfiles link in HOME is fine.
            fx.write("home/dotfiles/style.md", "Style\n");
            symlink(fx.path("home/dotfiles/style.md"), fx.path("home/.claude/rules/style.md")).unwrap();

            let project = fx.scan("proj", LibraryScope::Project);
            assert_eq!(
                summary(&project),
                vec![
                    (ItemKind::Skill, "claude", "pdf", ItemStatus::PuppeteerLink, false),
                    (ItemKind::Rule, "claude", "leak", ItemStatus::Symlink, true),
                    (ItemKind::Command, "claude", "fix", ItemStatus::Symlink, false),
                ]
            );
            assert_eq!(project[0].link_target.as_deref(), Some(lib.join("skills/pdf").display().to_string().as_str()));
            assert_eq!(project[0].description, "Library");
            assert!(project.iter().all(|i| !i.description.contains("SECRET")));

            let home = fx.scan("home", LibraryScope::Global);
            assert_eq!(
                summary(&home),
                vec![
                    (ItemKind::Rule, "claude", "key", ItemStatus::Symlink, true),
                    (ItemKind::Rule, "claude", "style", ItemStatus::Symlink, false),
                ]
            );

            for rel in ["proj/.claude/rules/leak.md", "home/.claude/rules/key.md"] {
                let loc = fx.locate(rel).unwrap();
                let err = read_at(&loc, &fx.roots.secret).unwrap_err();
                assert!(err.contains("outside") && !err.contains("SECRET"), "{err}");
                assert!(write_at(&loc, "x", &hash(b"SECRET"), &fx.roots.secret).is_err());
            }
            assert_eq!(fs::read_to_string(fx.path("outside/leak.md")).unwrap(), "SECRET");

            // Editing through Puppeteer's link edits the library file (the UI warns about it).
            let pdf = fx.locate("proj/.claude/skills/pdf/SKILL.md").unwrap();
            let opened = read_at(&pdf, &fx.roots.secret).unwrap();
            assert_eq!(opened.link_target.as_deref(), Some(lib.join("skills/pdf").display().to_string().as_str()));
            write_at(&pdf, "edited\n", &opened.hash, &fx.roots.secret).unwrap();
            assert_eq!(fs::read_to_string(lib.join("skills/pdf/SKILL.md")).unwrap(), "edited\n");
            assert!(fs::symlink_metadata(fx.path("proj/.claude/skills/pdf")).unwrap().file_type().is_symlink());
        }

        #[test]
        fn a_folder_linked_out_of_the_project_is_not_read() {
            let fx = Fixture::new("folder-out");
            fx.write("outside/rules/r.md", "SECRET");
            fs::create_dir_all(fx.path("proj/.claude")).unwrap();
            symlink(fx.path("outside/rules"), fx.path("proj/.claude/rules")).unwrap();
            assert!(fx.scan("proj", LibraryScope::Project).is_empty());
            let err = read_at(&fx.locate("proj/.claude/rules/r.md").unwrap(), &fx.roots.secret).unwrap_err();
            assert!(err.contains("outside"), "{err}");

            // A folder linked inside the project (Codex reading Claude's skills) is followed.
            fx.write("proj/.claude/skills/review/SKILL.md", &skill("d"));
            fs::create_dir_all(fx.path("proj/.agents")).unwrap();
            symlink("../.claude/skills", fx.path("proj/.agents/skills")).unwrap();
            let names: Vec<_> = fx.scan("proj", LibraryScope::Project).into_iter().map(|i| (i.engine, i.name)).collect();
            assert_eq!(names, vec![("claude", "review".to_string()), ("codex", "review".to_string())]);
        }

        #[test]
        fn deleting_a_link_never_touches_what_it_points_at() {
            let fx = Fixture::new("delete-link");
            let lib = library(&fx);
            fx.write(&format!("{}/skills/pdf/SKILL.md", lib.strip_prefix(&fx.dir).unwrap().display()), &skill("d"));
            fx.write(&format!("{}/agents/tester.md", lib.strip_prefix(&fx.dir).unwrap().display()), "Tester\n");
            fs::create_dir_all(fx.path("proj/.claude/skills")).unwrap();
            fs::create_dir_all(fx.path("proj/.claude/agents")).unwrap();
            symlink(lib.join("skills/pdf"), fx.path("proj/.claude/skills/pdf")).unwrap();
            symlink(lib.join("agents/tester.md"), fx.path("proj/.claude/agents/tester.md")).unwrap();
            let trash = |p: &Path| -> Result<(), String> { panic!("a link went to the trash: {}", p.display()) };

            delete_at(&fx.locate("proj/.claude/skills/pdf/SKILL.md").unwrap(), &fx.roots.secret, &trash).unwrap();
            delete_at(&fx.locate("proj/.claude/agents/tester.md").unwrap(), &fx.roots.secret, &trash).unwrap();
            assert!(fs::symlink_metadata(fx.path("proj/.claude/skills/pdf")).is_err());
            assert!(fs::symlink_metadata(fx.path("proj/.claude/agents/tester.md")).is_err());
            assert!(lib.join("skills/pdf/SKILL.md").is_file());
            assert!(lib.join("agents/tester.md").is_file());
        }

        #[test]
        fn duplicating_a_library_link_gives_global_its_own_copy() {
            let fx = Fixture::new("dup-link");
            let lib = library(&fx);
            fx.write(&format!("{}/skills/pdf/SKILL.md", lib.strip_prefix(&fx.dir).unwrap().display()), &skill("d"));
            fs::create_dir_all(fx.path("proj/.claude/skills")).unwrap();
            symlink(lib.join("skills/pdf"), fx.path("proj/.claude/skills/pdf")).unwrap();
            let trash = fx.trash();
            let loc = fx.locate("proj/.claude/skills/pdf/SKILL.md").unwrap();

            let copied = duplicate_at(&loc, &fx.roots, false, &trash).unwrap();
            assert_eq!(copied.status, ItemStatus::File);
            let dest = fx.path("home/.claude/skills/pdf");
            assert!(!fs::symlink_metadata(&dest).unwrap().file_type().is_symlink());
            assert!(dest.join("SKILL.md").is_file());

            // Global already links to the very same folder: nothing to copy, nothing trashed.
            fs::remove_dir_all(&dest).unwrap();
            symlink(lib.join("skills/pdf"), &dest).unwrap();
            assert!(duplicate_at(&loc, &fx.roots, true, &trash).unwrap_err().contains("already the global one"));
            assert!(lib.join("skills/pdf/SKILL.md").is_file());

            // Overwriting a global link to something else removes only the link.
            fx.write("home/dotfiles/pdf/SKILL.md", &skill("dotfiles"));
            fs::remove_file(&dest).unwrap();
            symlink(fx.path("home/dotfiles/pdf"), &dest).unwrap();
            duplicate_at(&loc, &fx.roots, true, &trash).unwrap();
            assert!(fx.path("home/dotfiles/pdf/SKILL.md").is_file());
            assert!(!fs::symlink_metadata(&dest).unwrap().file_type().is_symlink());
            assert!(!fx.path("trash").exists());
        }
    }
}
