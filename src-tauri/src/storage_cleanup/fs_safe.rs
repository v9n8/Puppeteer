//! The one way the cleanup deletes a file or folder. A path is removed only
//! when it sits under a declared root; a symlink is removed as a link and its
//! target is never touched; logins and `memory/` notes are refused.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// Login files of the Claude and Codex CLIs.
const PROTECTED_NAMES: &[&str] = &[".claude.json", ".credentials.json", "auth.json"];
/// The CLI's notes folder inside `projects/<slug>/`.
const MEMORY_DIR: &str = "memory";

pub fn is_protected_name(name: &str) -> bool {
    PROTECTED_NAMES.contains(&name)
}

/// Remove `path` (a file, a symlink or a whole folder) when it sits under
/// `root`, is not a login file, does not go through a `memory` folder, and,
/// for a folder, holds neither.
pub fn remove_under(root: &Path, path: &Path) -> Result<(), String> {
    let rel = checked_relative(root, path)?;
    if rel.iter().any(|c| c == MEMORY_DIR) {
        return Err("inside a memory folder".into());
    }
    if rel.file_name().and_then(|n| n.to_str()).is_some_and(is_protected_name) {
        return Err("a login file".into());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        if let Some(found) = protected_inside(path) {
            return Err(format!("holds {}", found.display()));
        }
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

/// Remove a deleted account's whole folder (its logins go with it: the account
/// is gone). Still under `root` only, and symlinks inside are removed as links.
pub fn remove_account_dir(root: &Path, dir: &Path) -> Result<(), String> {
    checked_relative(root, dir)?;
    let meta = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("not a folder".into());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

/// `path` relative to `root`, both with their parents' symlinks resolved, so a
/// `..` or a linked parent cannot lead outside. `path` itself is not resolved:
/// when it is a symlink, the link is what gets removed. `root` itself is refused.
fn checked_relative(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| format!("root {}: {e}", root.display()))?;
    let name = path.file_name().ok_or("no file name")?;
    let parent = path.parent().ok_or("no parent")?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    let full = parent.join(name);
    match full.strip_prefix(&root) {
        Ok(rel) if !rel.as_os_str().is_empty() => Ok(rel.to_path_buf()),
        _ => Err(format!("outside {}", root.display())),
    }
}

/// The first login file or `memory` folder inside `dir`, links not followed.
fn protected_inside(dir: &Path) -> Option<PathBuf> {
    WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| {
            let name = e.file_name().to_string_lossy();
            is_protected_name(&name) || (e.file_type().is_dir() && name == MEMORY_DIR)
        })
        .map(|e| e.into_path())
}

/// Newest modification time of `path` and, for a folder, of everything in it.
/// Symlinks count by their own time; their targets are never read.
pub fn newest_mtime(path: &Path) -> Option<SystemTime> {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Bytes and files under `path` (a file counts as one), links not followed.
pub fn size_of(path: &Path) -> (u64, u64) {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_type().is_dir())
        .fold((0, 0), |(bytes, files), e| {
            (bytes + e.metadata().map(|m| m.len()).unwrap_or(0), files + 1)
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-cleanup-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn write(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn removes_a_file_and_a_folder_under_the_root() {
        let root = temp_dir("fs-ok");
        write(&root.join("a.log"));
        write(&root.join("run-1/paste.txt"));
        remove_under(&root, &root.join("a.log")).unwrap();
        remove_under(&root, &root.join("run-1")).unwrap();
        assert!(!root.join("a.log").exists());
        assert!(!root.join("run-1").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refuses_the_root_and_anything_outside_it() {
        let base = temp_dir("fs-outside");
        let root = base.join("root");
        write(&root.join("in.txt"));
        write(&base.join("outside.txt"));
        assert!(remove_under(&root, &root).is_err());
        assert!(remove_under(&root, &base.join("outside.txt")).is_err());
        assert!(remove_under(&root, &root.join("../outside.txt")).is_err());
        assert!(base.join("outside.txt").exists());
        assert!(root.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn refuses_memory_folders_and_login_files() {
        let root = temp_dir("fs-protected");
        for rel in ["p/memory/MEMORY.md", "q/.credentials.json", "r/x.jsonl", "r/memory/a.md", "auth.json"] {
            write(&root.join(rel));
        }
        assert!(remove_under(&root, &root.join("p/memory/MEMORY.md")).is_err());
        assert!(remove_under(&root, &root.join("p/memory")).is_err());
        assert!(remove_under(&root, &root.join("q/.credentials.json")).is_err());
        assert!(remove_under(&root, &root.join("q")).is_err(), "a folder holding a login");
        assert!(remove_under(&root, &root.join("r")).is_err(), "a folder holding memory/");
        assert!(remove_under(&root, &root.join("auth.json")).is_err());
        for rel in ["p/memory/MEMORY.md", "q/.credentials.json", "r/x.jsonl", "auth.json"] {
            assert!(root.join(rel).exists(), "{rel}");
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_removed_as_a_link_never_followed() {
        let base = temp_dir("fs-link");
        let root = base.join("root");
        let outside = base.join("skills");
        write(&outside.join("keep.md"));
        write(&root.join("dir/file.txt"));
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("dir/nested-link")).unwrap();
        remove_under(&root, &root.join("link")).unwrap();
        remove_under(&root, &root.join("dir")).unwrap();
        assert!(!root.join("link").exists() && !root.join("dir").exists());
        assert!(outside.join("keep.md").exists());
        // A linked parent does not lead outside the root either.
        std::os::unix::fs::symlink(&outside, root.join("via")).unwrap();
        assert!(remove_under(&root, &root.join("via/keep.md")).is_err());
        assert!(outside.join("keep.md").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_account_folder_goes_whole_but_its_links_are_not_followed() {
        let base = temp_dir("fs-account");
        let root = base.join("claude-accounts");
        let outside = base.join("global-skills");
        write(&outside.join("s/SKILL.md"));
        write(&root.join("old/.credentials.json"));
        write(&root.join("old/projects/p/memory/MEMORY.md"));
        std::os::unix::fs::symlink(&outside, root.join("old/skills")).unwrap();
        assert!(remove_account_dir(&root, &root).is_err());
        remove_account_dir(&root, &root.join("old")).unwrap();
        assert!(!root.join("old").exists());
        assert!(outside.join("s/SKILL.md").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn newest_mtime_and_size_cover_the_whole_folder() {
        let root = temp_dir("fs-size");
        write(&root.join("a/1.txt"));
        write(&root.join("a/b/2.txt"));
        assert_eq!(size_of(&root.join("a")), (2, 2));
        assert_eq!(size_of(&root.join("a/1.txt")), (1, 1));
        let old = days_ago(90);
        for p in ["a/1.txt", "a/b/2.txt", "a/b", "a"] {
            backdate(&root.join(p), old);
        }
        assert!(newest_mtime(&root.join("a")).unwrap() <= old + std::time::Duration::from_secs(1));
        write(&root.join("a/b/new.txt"));
        assert!(newest_mtime(&root.join("a")).unwrap() > old + std::time::Duration::from_secs(86_400));
        fs::remove_dir_all(&root).unwrap();
    }

    /// Set the modification time of a file or a folder (Windows opens a folder
    /// only with backup semantics).
    pub(crate) fn backdate(path: &Path, when: SystemTime) {
        let mut options = fs::OpenOptions::new();
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.write(true).custom_flags(0x0200_0000);
        }
        #[cfg(not(windows))]
        options.read(true);
        options.open(path).unwrap().set_modified(when).unwrap();
    }

    pub(crate) fn days_ago(days: u64) -> SystemTime {
        SystemTime::now() - std::time::Duration::from_secs(days * 86_400)
    }
}
