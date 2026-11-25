//! The working trees of the git repository a viewer root belongs to: the main
//! one and every linked `git worktree`. An agent often runs
//! `git worktree add ../repo-123` and then links files in it; that folder sits
//! beside the project (often outside HOME) yet holds the same repository's
//! files, so the local file viewer may open them. A remote desktop never gets
//! here: it keeps its own allowlist.
//!
//! Read from git's own metadata files rather than a `git` process: `.git` (a
//! folder, or a `gitdir:` file in a linked tree), `commondir` and
//! `worktrees/<name>/gitdir`. A linked tree counts only while its `.git` file
//! points back at the entry that lists it — the link `git worktree prune`
//! checks — so a stale entry or a folder recreated in its place adds nothing.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

/// The working trees of `root`'s repository, listed on first use and then kept:
/// one command call reads git's metadata at most once, however many paths it
/// checks.
pub(super) struct WorkTrees<'a> {
    /// Canonical, as every caller in `files.rs` resolves it first.
    root: &'a Path,
    trees: OnceCell<Vec<PathBuf>>,
}

impl<'a> WorkTrees<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self { root, trees: OnceCell::new() }
    }

    /// True when the canonical `path` lies inside one of the working trees.
    pub(super) fn allows(&self, path: &Path) -> bool {
        self.trees.get_or_init(|| list(self.root)).iter().any(|t| path.starts_with(t))
    }
}

/// Canonical working trees of the repository holding `root`; empty when there
/// is none or its metadata can't be read.
fn list(root: &Path) -> Vec<PathBuf> {
    let Some(tree) = root.ancestors().find(|dir| dir.join(".git").exists()) else {
        return Vec::new();
    };
    let Some(common) = common_dir(&tree.join(".git")) else {
        return Vec::new();
    };
    let mut trees = vec![tree.to_path_buf()];
    // The main working tree holds the common `.git`; a bare repository has none.
    if common.file_name() == Some(".git".as_ref()) {
        trees.extend(common.parent().map(Path::to_path_buf));
    }
    if let Ok(entries) = std::fs::read_dir(common.join("worktrees")) {
        trees.extend(entries.flatten().filter_map(|e| linked_tree(&e.path())));
    }
    // Never the whole disk, whatever a `.git` at `/` says.
    trees.retain(|t| t.parent().is_some());
    trees.sort();
    trees.dedup();
    trees
}

/// The repository's common git dir seen from a working tree's `.git`: the
/// folder itself, or, for a `.git` file, the entry it names and that entry's
/// `commondir`. A submodule's entry has no `commondir`: it is its own repository.
fn common_dir(dot_git: &Path) -> Option<PathBuf> {
    if dot_git.is_dir() {
        return dot_git.canonicalize().ok();
    }
    let entry = gitdir_of(dot_git)?;
    match std::fs::read_to_string(entry.join("commondir")) {
        Ok(rel) => entry.join(rel.trim()).canonicalize().ok(),
        Err(_) => Some(entry),
    }
}

/// The canonical folder a `.git` file's `gitdir: <path>` line names (a
/// relative path starts at the file's folder).
fn gitdir_of(git_file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git_file).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    git_file.parent()?.join(target).canonicalize().ok()
}

/// The working tree a `worktrees/<name>` entry lists, while that tree's `.git`
/// still points back at the entry. Its `gitdir` holds the tree's `.git` path,
/// absolute or relative to the entry (`worktree.useRelativePaths`).
fn linked_tree(entry: &Path) -> Option<PathBuf> {
    let entry = entry.canonicalize().ok()?;
    let text = std::fs::read_to_string(entry.join("gitdir")).ok()?;
    let tree = entry.join(text.trim()).parent()?.canonicalize().ok()?;
    (gitdir_of(&tree.join(".git"))? == entry).then_some(tree)
}

#[cfg(test)]
mod tests {
    use super::super::{read_within, stat_within, write_within};
    use super::*;
    use crate::remote::fs_policy::{RemoteFs, Roots};
    use std::process::Command;

    fn sh(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// `<tmp>/puppeteer-wt-<uuid>/` with `repo/` (one commit, `a.txt`), its
    /// linked worktree `wt/` holding `wt/report.md`, and an unrelated repo `other/`.
    fn fixture() -> PathBuf {
        let base = std::env::temp_dir().canonicalize().unwrap().join(format!("puppeteer-wt-{}", uuid::Uuid::new_v4()));
        for name in ["repo", "other"] {
            let dir = base.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("a.txt"), name).unwrap();
            sh(&dir, &["init", "-q"]);
            sh(&dir, &["add", "."]);
            sh(&dir, &["commit", "-qm", "init"]);
        }
        sh(&base.join("repo"), &["worktree", "add", "-q", "-b", "side", "../wt"]);
        std::fs::write(base.join("wt/report.md"), "report").unwrap();
        base
    }

    #[test]
    fn a_linked_worktree_and_the_main_tree_reach_each_other() {
        let base = fixture();
        let (repo, wt) = (base.join("repo"), base.join("wt"));
        assert!(WorkTrees::new(&repo).allows(&wt.join("report.md")));
        assert!(WorkTrees::new(&wt).allows(&repo.join("a.txt")));
        // A project in a subfolder of the repository reaches the same trees.
        std::fs::create_dir(repo.join("sub")).unwrap();
        assert!(WorkTrees::new(&repo.join("sub")).allows(&wt.join("report.md")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn another_repository_or_no_repository_allows_nothing() {
        let base = fixture();
        let (repo, plain) = (base.join("repo"), base.join("plain"));
        std::fs::create_dir(&plain).unwrap();
        let trees = WorkTrees::new(&repo);
        assert!(!trees.allows(&base.join("other/a.txt")));
        assert!(!trees.allows(&base.join("plain")));
        assert!(!trees.allows(&base));
        assert!(!WorkTrees::new(&plain).allows(&repo.join("a.txt")));
        assert!(list(&plain).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A worktree deleted without `git worktree prune`, then a plain folder
    /// recreated at its place, and an entry forged to list a folder whose `.git`
    /// doesn't point back: none of them is a tree.
    #[test]
    fn a_stale_or_forged_entry_is_not_a_tree() {
        let base = fixture();
        let repo = base.join("repo");
        sh(&repo, &["worktree", "add", "-q", "-b", "gone", "../gone"]);
        std::fs::remove_dir_all(base.join("gone")).unwrap();
        assert_eq!(list(&repo), [repo.clone(), base.join("wt")]);
        std::fs::create_dir(base.join("gone")).unwrap();
        std::fs::write(base.join("gone/x.txt"), "x").unwrap();
        assert!(!WorkTrees::new(&repo).allows(&base.join("gone/x.txt")));

        let forged = repo.join(".git/worktrees/forged");
        std::fs::create_dir_all(&forged).unwrap();
        std::fs::write(forged.join("gitdir"), format!("{}\n", base.join("other/.git").display())).unwrap();
        assert!(!WorkTrees::new(&repo).allows(&base.join("other/a.txt")));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The trees are listed once per call: a worktree removed after the first
    /// check still counts for the rest of that call.
    #[test]
    fn the_trees_are_listed_once() {
        let base = fixture();
        let repo = base.join("repo");
        let trees = WorkTrees::new(&repo);
        assert!(trees.allows(&base.join("wt/report.md")));
        sh(&repo, &["worktree", "remove", "--force", "../wt"]);
        assert!(trees.allows(&base.join("wt/report.md")));
        assert!(!WorkTrees::new(&repo).allows(&base.join("wt/report.md")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn read_stat_and_write_reach_a_file_in_the_worktree() {
        let base = fixture();
        let repo = base.join("repo");
        let repo_s = repo.to_str().unwrap();
        let file = base.join("wt/report.md").to_string_lossy().into_owned();
        assert_eq!(read_within(repo_s, &file, None).unwrap().content, "report");
        assert_eq!(stat_within(repo_s, &[file.clone()], None).unwrap(), [true]);
        write_within(repo_s, &file, "edited", None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "edited");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A remote desktop allowed only the repository can't reach its worktree.
    #[test]
    fn a_remote_caller_does_not_reach_the_worktree() {
        let base = fixture();
        let repo = base.join("repo");
        let repo_s = repo.to_str().unwrap();
        let file = base.join("wt/report.md").to_string_lossy().into_owned();
        let roots = Roots { writable: vec![repo.clone()], ..Roots::default() };
        let policy = RemoteFs::new(roots, None, crate::remote::guard::SecretPaths::new(None, None));
        assert!(read_within(repo_s, &file, Some(&policy)).unwrap_err().contains("remote desktop"));
        assert_eq!(stat_within(repo_s, &[file.clone()], Some(&policy)).unwrap(), [false]);
        assert!(write_within(repo_s, &file, "PWNED", Some(&policy)).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "report");
        let _ = std::fs::remove_dir_all(&base);
    }
}
