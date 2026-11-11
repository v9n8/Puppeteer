//! Symlinks from a project into Puppeteer's skill/rule library.
//!
//! Skills and rules are installed into projects as symlinks pointing back at the library source,
//! so editing the source updates every project at once. There is deliberately no copy fallback:
//! if a link can't be created the caller gets an error.
//!
//! Projects applied before the switch hold plain copies. A copy that is provably unedited (`Known`
//! says so) is deleted when it is swapped for a link or the item is removed. Anything else is
//! never deleted: it is moved to `<app data>/legacy-backups/<project>/<timestamp>/` first, so the
//! switch-over never blocks and never loses a local edit.
//!
//! Paths git tracks are never touched: a team that committed its own `.claude/skills/x` keeps it,
//! and Puppeteer reports `Linked::TrackedByGit` instead of installing there. A project folder that no
//! longer exists is an error rather than being recreated by `create_dir_all`.
//!
//! Every operation first checks that the folder holding `dest` is a real directory inside the
//! project. If it is reached through a symlink (say `.agents/skills -> ../.claude/skills`), acting
//! on `dest` would actually touch another engine's install — or the library itself.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static BACKUP_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set once at startup to `<app data>/legacy-backups`.
pub fn set_backup_root(dir: PathBuf) {
    let _ = BACKUP_ROOT.set(dir);
}

/// Where legacy copies are moved before being replaced or removed.
pub fn backup_root() -> PathBuf {
    BACKUP_ROOT
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("puppeteer-legacy-backups"))
}

/// Backups for one project live under a folder named after its path.
pub fn project_backup_dir(project_root: &Path) -> PathBuf {
    let key: String = project_root
        .to_string_lossy()
        .trim_start_matches(['/', '\\'])
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    backup_root().join(key)
}

/// Decides whether a plain file/dir found at a destination is an unedited Puppeteer copy.
pub trait Known: Fn(&Path) -> bool {}
impl<F: Fn(&Path) -> bool> Known for F {}

/// Nothing at a destination is ever a known copy (used where Puppeteer never wrote copies).
pub fn no_copies(_: &Path) -> bool {
    false
}

/// Outcome of `ensure_symlink`.
#[derive(Debug, PartialEq, Eq)]
pub enum Linked {
    /// `dest` now links to the source.
    Yes,
    /// `dest` holds files git tracks; left as is, nothing installed there.
    TrackedByGit,
}

/// Whether `path` lies inside a Puppeteer app data dir (`io.github.v9n8.puppeteer`, `io.github.v9n8.puppeteer.dev`, …) —
/// i.e. a link to it was made by Puppeteer, possibly from another build or an older location.
fn in_puppeteer_library(path: &Path) -> bool {
    path.ancestors()
        .any(|a| a.file_name().is_some_and(|n| n.to_string_lossy().starts_with("io.github.v9n8.puppeteer")))
}

/// Whether git tracks anything at `dest` (a file, or files under a folder) in the project's repo.
pub(crate) fn git_tracked(root: &Path, dest: &Path) -> bool {
    let Ok(rel) = dest.strip_prefix(root) else { return false };
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--"])
        .arg(rel)
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

/// Whether `root` is the user's HOME, symlinks resolved. Its `.claude/skills` & co. are the global
/// folders every CLI reads, holding the user's own items: never managed as a project's.
pub fn is_home(root: &Path) -> bool {
    std::env::home_dir().is_some_and(|home| same_file(root, &home))
}

fn require_project_dir(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("Project folder not found: {}", root.display()));
    }
    if is_home(root) {
        return Err(format!(
            "{} is your home folder — Puppeteer won't link a project's items there; use a project folder inside it",
            root.display()
        ));
    }
    Ok(())
}

/// What currently sits at a link destination.
enum Existing {
    Nothing,
    Link(PathBuf),
    Dir,
    File,
}

fn inspect(dest: &Path) -> Result<Existing, String> {
    match fs::symlink_metadata(dest) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::read_link(dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
            Ok(Existing::Link(target))
        }
        Ok(meta) if meta.is_dir() => Ok(Existing::Dir),
        Ok(_) => Ok(Existing::File),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Existing::Nothing),
        Err(e) => Err(format!("{}: {}", dest.display(), e)),
    }
}

/// Whether every folder between `root` and `dest` is a real directory (no symlinked component).
/// Folders that don't exist yet count as real — `create_dir_all` will make them so.
pub fn parent_is_real(root: &Path, dest: &Path) -> Result<bool, String> {
    let Some(parent) = dest.parent() else { return Ok(false) };
    let Ok(rel) = parent.strip_prefix(root) else { return Ok(false) };
    let mut dir = root.to_path_buf();
    for part in rel.components() {
        dir.push(part);
        match fs::symlink_metadata(&dir) {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(false),
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(format!("{}: {}", dir.display(), e)),
        }
    }
    Ok(true)
}

fn require_real_parent(root: &Path, dest: &Path) -> Result<(), String> {
    if parent_is_real(root, dest)? {
        Ok(())
    } else {
        Err(format!(
            "{} is reached through a symlinked folder — Puppeteer won't manage links there",
            dest.display()
        ))
    }
}

/// Move a copy Puppeteer can't vouch for out of the project, keeping its path relative to `root`.
fn back_up(root: &Path, dest: &Path) -> Result<PathBuf, String> {
    let rel = dest.strip_prefix(root).unwrap_or(dest);
    move_to_backup(&project_backup_dir(root), dest, rel)
}

/// Move a library file/folder the DB doesn't know (left over from an old rename or delete) out of
/// the way, so a skill/rule can take its name. Kept under `legacy-backups/_library/`.
pub fn back_up_library_orphan(path: &Path, kind: &str) -> Result<PathBuf, String> {
    let name = path.file_name().map(PathBuf::from).unwrap_or_default();
    move_to_backup(&backup_root().join("_library"), path, &Path::new(kind).join(name))
}

/// Move `src` to `<base>/<timestamp>/<rel>`.
fn move_to_backup(base: &Path, src: &Path, rel: &Path) -> Result<PathBuf, String> {
    let dest = src;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut target = base.join(&stamp).join(rel);
    let mut n = 1;
    while target.exists() {
        target = base.join(format!("{}-{}", stamp, n)).join(rel);
        n += 1;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    if fs::rename(dest, &target).is_err() {
        // Different volume: copy, then delete the original.
        copy_tree(dest, &target)?;
        remove_copy(dest)?;
    }
    tracing::info!("moved legacy copy {} to {}", dest.display(), target.display());
    Ok(target)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_file() {
        return fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {}", to.display(), e));
    }
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let out = to.join(entry.path().strip_prefix(from).map_err(|e| e.to_string())?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&out).map_err(|e| format!("{}: {}", out.display(), e))?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &out).map_err(|e| format!("{}: {}", out.display(), e))?;
        }
    }
    Ok(())
}

/// Clear a legacy copy at `dest`: delete it when `known`, otherwise move it to the backups.
fn clear_copy(root: &Path, dest: &Path, known: impl Known) -> Result<(), String> {
    if known(dest) {
        remove_copy(dest)
    } else {
        back_up(root, dest).map(|_| ())
    }
}

/// Make `dest` (inside project `root`) a symlink to `target`.
///
/// * nothing there → create the link (parent dirs included);
/// * already a link to `target` → no-op; a dangling link or one into a Puppeteer library elsewhere →
///   repointed; a link to anything else → error (not ours to repoint);
/// * a real file/dir git tracks → left alone (`Linked::TrackedByGit`);
/// * any other real file/dir (legacy copy) → deleted if `known(dest)`, else moved to the backups.
pub fn ensure_symlink(root: &Path, dest: &Path, target: &Path, known: impl Known) -> Result<Linked, String> {
    require_project_dir(root)?;
    if !target.exists() {
        return Err(format!("Source not found: {}", target.display()));
    }
    require_real_parent(root, dest)?;
    match inspect(dest)? {
        Existing::Nothing => {}
        Existing::Link(current) => {
            if current == target {
                return Ok(Linked::Yes);
            }
            if dest.exists() && !in_puppeteer_library(&current) {
                return Err(format!(
                    "{} already links to {} — remove it, then apply again",
                    dest.display(),
                    current.display()
                ));
            }
            remove_link(dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
        }
        Existing::Dir | Existing::File => {
            if git_tracked(root, dest) {
                return Ok(Linked::TrackedByGit);
            }
            clear_copy(root, dest, known)?;
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    create_symlink(target, dest).map(|()| Linked::Yes)
}

/// Remove what Puppeteer installed at `dest`: a link to `target`, a dangling one or one into a Puppeteer
/// library, or a legacy copy (deleted if `known`, else moved to the backups). Other links and
/// anything git tracks are left alone. Missing is fine, and so is a symlinked parent folder (then
/// `dest` isn't ours — nothing is touched).
pub fn remove_installed(root: &Path, dest: &Path, target: &Path, known: impl Known) -> Result<(), String> {
    if !parent_is_real(root, dest)? {
        return Ok(());
    }
    match inspect(dest)? {
        Existing::Nothing => Ok(()),
        Existing::Link(current) => {
            if current == target || !dest.exists() || in_puppeteer_library(&current) {
                remove_link(dest).map_err(|e| format!("{}: {}", dest.display(), e))
            } else {
                Ok(())
            }
        }
        Existing::Dir | Existing::File if git_tracked(root, dest) => Ok(()),
        Existing::Dir | Existing::File => clear_copy(root, dest, known),
    }
}

/// Remove `dest` only if it is Puppeteer's link (see `remove_installed`); real files and folders are
/// left alone. For locations Puppeteer never wrote copies to, such as `.agents/skills`.
pub fn remove_own_link(root: &Path, dest: &Path, target: &Path) -> Result<(), String> {
    match inspect(dest)? {
        Existing::Link(_) => remove_installed(root, dest, target, no_copies),
        _ => Ok(()),
    }
}

/// Make `dest` (inside `root`, a folder shared with the user such as HOME) a link to `target`, only
/// where nothing is yet. Whatever already sits there — the user's own skill, another link, even a
/// dangling one — is reported and left exactly as it is: never moved, replaced or deleted.
pub fn link_if_free(root: &Path, dest: &Path, target: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("Folder not found: {}", root.display()));
    }
    if !target.exists() {
        return Err(format!("Source not found: {}", target.display()));
    }
    require_real_parent(root, dest)?;
    match inspect(dest)? {
        Existing::Nothing => {}
        Existing::Link(current) if current == target => return Ok(()),
        _ => {
            return Err(format!(
                "{} already exists and isn't Puppeteer's — rename or remove it (or rename the skill), then try again",
                dest.display()
            ))
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    create_symlink(target, dest)
}

/// Remove `dest` only when it is a link to `target` (what `link_if_free` made); anything else,
/// and anything reached through a symlinked folder, is left alone.
pub fn unlink_if_own(root: &Path, dest: &Path, target: &Path) -> Result<bool, String> {
    if !parent_is_real(root, dest)? {
        return Ok(false);
    }
    match inspect(dest)? {
        Existing::Link(current) if current == target => {
            remove_link(dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// What sits at a link destination, read without changing anything.
#[derive(Debug, PartialEq, Eq)]
pub enum AtDest {
    Nothing,
    /// A link to the source.
    OwnLink,
    /// Files git tracks: the repo's own, never touched.
    GitTracked,
    /// Anything else: a link elsewhere (or dangling), a copy, or a folder reached through a
    /// symlinked parent that Puppeteer won't manage.
    Other,
}

/// Classify `dest` (inside project `root`) against `target`. git is asked only when a real
/// file or folder is there, so an empty destination costs one `lstat`.
pub fn at_dest(root: &Path, dest: &Path, target: &Path) -> AtDest {
    if !matches!(parent_is_real(root, dest), Ok(true)) {
        return AtDest::Other;
    }
    match inspect(dest) {
        Ok(Existing::Nothing) => AtDest::Nothing,
        Ok(Existing::Link(current)) if current == target => AtDest::OwnLink,
        Ok(Existing::Dir | Existing::File) if git_tracked(root, dest) => AtDest::GitTracked,
        _ => AtDest::Other,
    }
}

/// True when `dest` is a symlink (dangling or not).
#[cfg(test)]
pub fn is_symlink(dest: &Path) -> bool {
    fs::symlink_metadata(dest).is_ok_and(|m| m.file_type().is_symlink())
}

/// Finder metadata files, never part of a skill — ignored when comparing copies.
fn is_os_junk(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == ".DS_Store")
}

/// True when the legacy copy `copy` holds exactly the files of `source` with the same bytes —
/// nothing edited, added or removed in the project — leaving out the file at relative path `skip`
/// (checked separately by the caller). Used for skill folders, where a SKILL.md hash alone would
/// miss edits to references/ or scripts/.
pub fn same_tree_except(copy: &Path, source: &Path, skip: Option<&Path>) -> bool {
    fn files(dir: &Path, skip: Option<&Path>) -> Option<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(dir).follow_links(false) {
            let entry = entry.ok()?;
            if entry.file_type().is_symlink() {
                return None;
            }
            if entry.file_type().is_file() {
                let rel = entry.path().strip_prefix(dir).ok()?.to_path_buf();
                if Some(rel.as_path()) != skip && !is_os_junk(&rel) {
                    out.push(rel);
                }
            }
        }
        out.sort();
        Some(out)
    }
    let (Some(a), Some(b)) = (files(copy, skip), files(source, skip)) else { return false };
    a == b && a.iter().all(|rel| match (fs::read(copy.join(rel)), fs::read(source.join(rel))) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    })
}

/// Slack for files written in the same sync pass as the copy's SKILL.md.
const SYNC_PASS_SLACK: std::time::Duration = std::time::Duration::from_secs(2);

/// True when no file in `dir` (other than `reference` itself) was written after `reference`.
///
/// Copy-era syncs rewrote a skill's whole folder in one pass, SKILL.md included, so every file
/// Puppeteer wrote has a change time (ctime) at or before that pass — leftovers from older syncs are
/// earlier still. A file edited or added in the project afterwards has a later ctime. ctime is
/// used, not mtime: `fs::copy` keeps the source's mtime, which says nothing about when the copy
/// was made. (No ctime on Windows; mtime is the best available there.)
pub fn untouched_since(dir: &Path, reference: &Path) -> bool {
    untouched_since_within(dir, reference, SYNC_PASS_SLACK)
}

fn untouched_since_within(dir: &Path, reference: &Path, slack: std::time::Duration) -> bool {
    let Some(limit) = fs::metadata(reference).ok().and_then(|m| written_at(&m)) else { return false };
    let limit = limit + slack;
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let Ok(entry) = entry else { return false };
        if entry.file_type().is_symlink() {
            return false;
        }
        if !entry.file_type().is_file() || entry.path() == reference || is_os_junk(entry.path()) {
            continue;
        }
        match entry.metadata().ok().and_then(|m| written_at(&m)) {
            Some(t) if t <= limit => {}
            _ => return false,
        }
    }
    true
}

#[cfg(unix)]
fn written_at(meta: &fs::Metadata) -> Option<std::time::SystemTime> {
    use std::os::unix::fs::MetadataExt;
    let secs = u64::try_from(meta.ctime()).ok()?;
    let nanos = u32::try_from(meta.ctime_nsec()).ok()?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::new(secs, nanos))
}

#[cfg(not(unix))]
fn written_at(meta: &fs::Metadata) -> Option<std::time::SystemTime> {
    meta.modified().ok()
}

/// True when `a` and `b` resolve to the same existing file — e.g. a rename that only changes
/// letter case on a case-insensitive file system.
pub fn same_file(a: &Path, b: &Path) -> bool {
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

fn remove_copy(dest: &Path) -> Result<(), String> {
    let result = if dest.is_dir() { fs::remove_dir_all(dest) } else { fs::remove_file(dest) };
    result.map_err(|e| format!("{}: {}", dest.display(), e))
}

#[cfg(unix)]
pub(crate) fn create_symlink(target: &Path, dest: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, dest)
        .map_err(|e| format!("Cannot create symlink {} → {}: {}", dest.display(), target.display(), e))
}

#[cfg(windows)]
pub(crate) fn create_symlink(target: &Path, dest: &Path) -> Result<(), String> {
    let result = if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, dest)
    } else {
        std::os::windows::fs::symlink_file(target, dest)
    };
    result.map_err(|e| {
        format!(
            "Cannot create symlink {} → {}: {} (Windows needs Developer Mode enabled to create symlinks)",
            dest.display(),
            target.display(),
            e
        )
    })
}

#[cfg(unix)]
pub(crate) fn remove_link(dest: &Path) -> std::io::Result<()> {
    fs::remove_file(dest)
}

#[cfg(windows)]
pub(crate) fn remove_link(dest: &Path) -> std::io::Result<()> {
    // Directory symlinks are directories to the Windows API.
    fs::remove_file(dest).or_else(|_| fs::remove_dir(dest))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-links-{}-{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn content_is(expected: &'static str) -> impl Fn(&Path) -> bool {
        move |p| fs::read_to_string(p).is_ok_and(|c| c == expected)
    }

    #[test]
    fn creates_link_and_is_idempotent() {
        let root = tmp("create");
        let src = root.join("src.md");
        fs::write(&src, "a").unwrap();
        let dest = root.join("proj/.claude/rules/x.md");

        ensure_symlink(&root, &dest, &src, no_copies).unwrap();
        ensure_symlink(&root, &dest, &src, no_copies).unwrap();
        assert_eq!(fs::read_link(&dest).unwrap(), src);
        assert_eq!(fs::read_to_string(&dest).unwrap(), "a");
    }

    #[test]
    fn replaces_known_copy_but_keeps_edited_copy() {
        let root = tmp("copy");
        let src = root.join("src.md");
        fs::write(&src, "source").unwrap();

        let known = root.join("known.md");
        fs::write(&known, "synced").unwrap();
        ensure_symlink(&root, &known, &src, content_is("synced")).unwrap();
        assert!(is_symlink(&known));

        // An unrecognised copy is moved to the backups, then linked.
        let edited = root.join("edited.md");
        fs::write(&edited, "local edit").unwrap();
        ensure_symlink(&root, &edited, &src, content_is("synced")).unwrap();
        assert!(is_symlink(&edited));
        assert_eq!(backed_up(&root, "edited.md"), vec!["local edit".to_string()]);

        // Removal follows the same rule.
        let removed = root.join("removed.md");
        fs::write(&removed, "another edit").unwrap();
        remove_installed(&root, &removed, &src, content_is("synced")).unwrap();
        assert!(!removed.exists());
        assert_eq!(backed_up(&root, "removed.md"), vec!["another edit".to_string()]);
    }

    /// Contents of every backup of `rel` taken for project `root`.
    fn backed_up(root: &Path, rel: &str) -> Vec<String> {
        let Ok(stamps) = fs::read_dir(project_backup_dir(root)) else { return Vec::new() };
        stamps
            .flatten()
            .filter_map(|e| fs::read_to_string(e.path().join(rel)).ok())
            .collect()
    }

    #[test]
    fn refuses_foreign_link_but_replaces_dangling_one() {
        let root = tmp("foreign");
        let src = root.join("src.md");
        let other = root.join("other.md");
        fs::write(&src, "a").unwrap();
        fs::write(&other, "b").unwrap();

        let dest = root.join("dest.md");
        std::os::unix::fs::symlink(&other, &dest).unwrap();
        assert!(ensure_symlink(&root, &dest, &src, no_copies).is_err());
        // Removal leaves a link it doesn't own.
        remove_installed(&root, &dest, &src, no_copies).unwrap();
        assert!(is_symlink(&dest));

        fs::remove_file(&other).unwrap();
        ensure_symlink(&root, &dest, &src, no_copies).unwrap();
        assert_eq!(fs::read_link(&dest).unwrap(), src);
    }

    #[test]
    fn removing_a_dir_link_keeps_the_source() {
        let root = tmp("remove");
        let src = root.join("skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "x").unwrap();
        let dest = root.join("proj/.claude/skills/skill");
        ensure_symlink(&root, &dest, &src, no_copies).unwrap();

        remove_installed(&root, &dest, &src, no_copies).unwrap();
        assert!(!is_symlink(&dest));
        assert!(src.join("SKILL.md").exists());
    }

    #[test]
    fn symlinked_parent_folder_is_never_touched() {
        let root = tmp("shared-parent");
        let src = root.join("lib/skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "x").unwrap();
        let claude = root.join(".claude/skills/skill");
        ensure_symlink(&root, &claude, &src, no_copies).unwrap();
        // `.agents/skills` is the same folder as `.claude/skills`.
        fs::create_dir_all(root.join(".agents")).unwrap();
        std::os::unix::fs::symlink(root.join(".claude/skills"), root.join(".agents/skills")).unwrap();
        let codex = root.join(".agents/skills/skill");

        assert!(ensure_symlink(&root, &codex, &src, no_copies).is_err());
        remove_installed(&root, &codex, &src, no_copies).unwrap();
        assert!(is_symlink(&claude), "Claude's link survives");
    }

    #[test]
    fn same_tree_spots_edits_outside_skill_md() {
        let root = tmp("tree");
        let src = root.join("src");
        let copy = root.join("copy");
        for d in [&src, &copy] {
            fs::create_dir_all(d.join("references")).unwrap();
            fs::write(d.join("SKILL.md"), "s").unwrap();
            fs::write(d.join("references/a.md"), "ref").unwrap();
        }
        assert!(same_tree_except(&copy, &src, None));
        fs::write(copy.join("references/a.md"), "edited").unwrap();
        assert!(!same_tree_except(&copy, &src, None));
        fs::write(copy.join("references/a.md"), "ref").unwrap();
        fs::write(copy.join("notes.md"), "added").unwrap();
        assert!(!same_tree_except(&copy, &src, None));
    }

    #[test]
    fn untouched_since_catches_later_writes_only() {
        let root = tmp("ctime");
        fs::create_dir_all(root.join("references")).unwrap();
        let md = root.join("SKILL.md");
        let reference = root.join("references/a.md");
        // A leftover from an older sync, then the pass that wrote SKILL.md.
        fs::write(&reference, "old ref").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&md, "s").unwrap();
        let none = std::time::Duration::ZERO;
        assert!(untouched_since_within(&root, &md, none), "older files are fine");

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&reference, "edited ref").unwrap();
        assert!(!untouched_since_within(&root, &md, none), "a later write is caught");
        assert!(untouched_since(&root, &md), "within the sync-pass slack it still counts as one pass");
    }

    #[test]
    fn git_tracked_paths_are_left_alone() {
        let root = tmp("tracked");
        let src = root.join("lib/skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "lib").unwrap();
        let dest = root.join(".claude/skills/skill");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "team version").unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git").arg("-C").arg(&root).args(args).output().unwrap().status.success());
        };
        git(&["init", "-q"]);
        git(&["add", ".claude"]);

        assert_eq!(ensure_symlink(&root, &dest, &src, no_copies).unwrap(), Linked::TrackedByGit);
        remove_installed(&root, &dest, &src, |_: &Path| true).unwrap();
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "team version");
        assert!(!is_symlink(&dest));
    }

    #[test]
    fn missing_project_folder_is_not_recreated() {
        let root = tmp("gone").join("project");
        let src = tmp("gone-lib").join("x.md");
        fs::write(&src, "x").unwrap();
        let err = ensure_symlink(&root, &root.join(".claude/rules/x.md"), &src, no_copies).unwrap_err();
        assert!(err.contains("Project folder not found"), "{err}");
        assert!(!root.exists());
    }

    #[test]
    fn links_into_another_puppeteer_library_are_repointed() {
        let root = tmp("relink");
        let old_lib = root.join("Application Support/io.github.v9n8.puppeteer.dev/skills/skill");
        let src = root.join("Application Support/io.github.v9n8.puppeteer/skills/skill");
        for d in [&old_lib, &src] {
            fs::create_dir_all(d).unwrap();
            fs::write(d.join("SKILL.md"), "x").unwrap();
        }
        let dest = root.join("proj/.claude/skills/skill");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&old_lib, &dest).unwrap();

        assert_eq!(ensure_symlink(&root, &dest, &src, no_copies).unwrap(), Linked::Yes);
        assert_eq!(fs::read_link(&dest).unwrap(), src);
    }

    #[test]
    fn remove_own_link_keeps_real_folders() {
        let root = tmp("own-link");
        let src = root.join("lib/skill");
        fs::create_dir_all(&src).unwrap();
        let dest = root.join(".agents/skills/skill");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "someone else's").unwrap();
        remove_own_link(&root, &dest, &src).unwrap();
        assert!(dest.join("SKILL.md").exists());
    }

    #[test]
    fn link_if_free_never_touches_what_is_there() {
        let home = tmp("free");
        let src = home.join("lib/skill");
        fs::create_dir_all(&src).unwrap();
        let dir = home.join(".claude/skills");

        let dest = dir.join("skill");
        link_if_free(&home, &dest, &src).unwrap();
        link_if_free(&home, &dest, &src).unwrap();
        assert_eq!(fs::read_link(&dest).unwrap(), src);

        // The user's own folder, a foreign link and a dangling one all stay as they are.
        let own = dir.join("own");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join("SKILL.md"), "mine").unwrap();
        let foreign = dir.join("foreign");
        std::os::unix::fs::symlink(home.join("lib"), &foreign).unwrap();
        let dangling = dir.join("dangling");
        std::os::unix::fs::symlink(home.join("gone"), &dangling).unwrap();
        for d in [&own, &foreign, &dangling] {
            let err = link_if_free(&home, d, &src).unwrap_err();
            assert!(err.contains("isn't Puppeteer's"), "{err}");
            assert!(!unlink_if_own(&home, d, &src).unwrap());
        }
        assert_eq!(fs::read_to_string(own.join("SKILL.md")).unwrap(), "mine");
        assert_eq!(fs::read_link(&foreign).unwrap(), home.join("lib"));
        assert_eq!(fs::read_link(&dangling).unwrap(), home.join("gone"));

        assert!(unlink_if_own(&home, &dest, &src).unwrap());
        assert!(!is_symlink(&dest));
        assert!(src.exists());
        assert!(!unlink_if_own(&home, &dest, &src).unwrap(), "already gone");
    }

    #[test]
    fn link_if_free_refuses_a_symlinked_folder() {
        let home = tmp("free-parent");
        let src = home.join("lib/skill");
        fs::create_dir_all(&src).unwrap();
        // `~/.claude/skills` kept in a dotfiles repo, say.
        fs::create_dir_all(home.join("dotfiles/skills")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        std::os::unix::fs::symlink(home.join("dotfiles/skills"), home.join(".claude/skills")).unwrap();
        let dest = home.join(".claude/skills/skill");

        let err = link_if_free(&home, &dest, &src).unwrap_err();
        assert!(err.contains("symlinked folder"), "{err}");
        assert!(!home.join("dotfiles/skills/skill").exists());
        std::os::unix::fs::symlink(&src, home.join("dotfiles/skills/skill")).unwrap();
        assert!(!unlink_if_own(&home, &dest, &src).unwrap());
        assert!(is_symlink(&home.join("dotfiles/skills/skill")));
    }

    #[test]
    fn home_is_never_a_project_root() {
        let Some(home) = std::env::home_dir() else { return };
        let err = require_project_dir(&home).unwrap_err();
        assert!(err.contains("home folder"), "{err}");
        // Reached through a symlink it is still HOME; any other folder is a project.
        let root = tmp("home");
        std::os::unix::fs::symlink(&home, root.join("home")).unwrap();
        assert!(require_project_dir(&root.join("home")).is_err());
        assert!(require_project_dir(&root).is_ok());
    }

    #[test]
    fn missing_source_is_an_error() {
        let root = tmp("missing");
        let err = ensure_symlink(&root, &root.join("d"), &root.join("nope"), no_copies).unwrap_err();
        assert!(err.contains("Source not found"), "{err}");
    }
}
