//! One git worktree per Conductor worker (`orchestration.conductor_worktree`).
//!
//! A worker spawned in a git project gets its own worktree on its own branch
//! and runs there, so parallel workers never edit the same files. Worktrees
//! live under `<app data>/worktrees/<worker id>`, outside the project, so the
//! project's own build, watch and search tools never see the copies. The path
//! is stored on the worker's run row (`runs.worktree_path`); start and resume
//! run the worker in it.
//!
//! Cleanup happens at the next startup ([`sweep`]): a cancelled worker
//! (stopped, or cut off by the app quitting, see
//! `db::reconcile_interrupted_runs`) and a deleted one lose their worktree.
//! Its branch is deleted only when merged, so committed work is never lost.
//! A cancelled worker that is resumed later gets its worktree back
//! ([`launch_dir`]).
//!
//! The human reviews a worker's changes ([`diff`]) and merges its branch into
//! the project's current branch ([`merge`]). A merge that conflicts is aborted
//! and reported: nothing is resolved for the user.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;

use crate::db::Db;

pub const FEATURE: &str = "orchestration.conductor_worktree";

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set once at startup to `<app data>/worktrees`.
pub fn set_root(dir: PathBuf) {
    let _ = ROOT.set(dir);
}

pub fn root() -> PathBuf {
    ROOT.get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("puppeteer-worktrees"))
}

pub fn branch_for(worker_id: &str) -> String {
    format!("puppeteer/worker-{worker_id}")
}

fn git<I, S>(dir: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git_with_index(dir, None, args)
}

/// [`git`] with `GIT_INDEX_FILE` set to `index` when given.
fn git_with_index<I, S>(dir: &Path, index: Option<&Path>, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new("git");
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", index);
    }
    let out = cmd
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The repository top level and the project's folder inside it (`""` at the
/// top), or `None` when the project is not in a git work tree.
fn repo_of(project: &Path) -> Option<(PathBuf, PathBuf)> {
    let out = git(project, ["rev-parse", "--show-toplevel", "--show-prefix"]).ok()?;
    let mut lines = out.lines();
    let top = PathBuf::from(lines.next()?);
    Some((top, PathBuf::from(lines.next().unwrap_or(""))))
}

/// Give `worker_id` a new worktree of the project's repository on a new branch
/// from the current `HEAD`. `Ok(None)` when the project is not a git project:
/// the worker then runs in the project folder.
pub fn create(project: &Path, root: &Path, worker_id: &str) -> Result<Option<PathBuf>, String> {
    let Some((top, _)) = repo_of(project) else {
        return Ok(None);
    };
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join(worker_id);
    let branch = branch_for(worker_id);
    let args: [&OsStr; 6] =
        ["worktree".as_ref(), "add".as_ref(), "-b".as_ref(), branch.as_ref(), path.as_ref(), "HEAD".as_ref()];
    git(&top, args).map_err(|e| format!("Could not create the worker's worktree: {e}"))?;
    Ok(Some(path))
}

/// The folder a worker with worktree `worktree` runs in: the project's folder
/// inside it. A worktree the sweep removed is added back first, on its branch
/// when that still exists.
pub fn launch_dir(project: &Path, worktree: &Path, worker_id: &str) -> Result<PathBuf, String> {
    let (top, prefix) =
        repo_of(project).ok_or_else(|| "The project is no longer a git repository".to_string())?;
    if !worktree.is_dir() {
        // A folder deleted by hand is still registered and would block `add`.
        let _ = git(&top, ["worktree", "prune"]);
        let branch = branch_for(worker_id);
        let has_branch = git(&top, ["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok();
        let added = if has_branch {
            let args: [&OsStr; 4] = ["worktree".as_ref(), "add".as_ref(), worktree.as_ref(), branch.as_ref()];
            git(&top, args)
        } else {
            let args: [&OsStr; 6] =
                ["worktree".as_ref(), "add".as_ref(), "-b".as_ref(), branch.as_ref(), worktree.as_ref(), "HEAD".as_ref()];
            git(&top, args)
        };
        added.map_err(|e| format!("Could not restore the worker's worktree: {e}"))?;
    }
    // `join("")` would add a trailing separator.
    Ok(if prefix.as_os_str().is_empty() { worktree.to_path_buf() } else { worktree.join(prefix) })
}

/// Give a new worker its own worktree when [`FEATURE`] is on, and record it on
/// the worker's run row. `Ok(None)`: the worker runs in the project folder.
pub async fn assign(db: &Db, root: &Path, worker_id: &str) -> Result<Option<PathBuf>, String> {
    if !crate::features::is_on(db, FEATURE).await {
        return Ok(None);
    }
    let project: String = sqlx::query_scalar(
        "SELECT p.path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
    )
    .bind(worker_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    let (root, id) = (root.to_path_buf(), worker_id.to_string());
    let path = tokio::task::spawn_blocking(move || create(Path::new(&project), &root, &id))
        .await
        .map_err(|e| e.to_string())??;
    if let Some(path) = &path {
        sqlx::query("UPDATE runs SET worktree_path = ? WHERE id = ?")
            .bind(path.to_string_lossy().to_string())
            .bind(worker_id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// The folder a run works in: the project folder, or for a worker with a
/// worktree its [`launch_dir`].
pub async fn run_dir(project: &str, worktree: Option<String>, run_id: &str) -> Result<String, String> {
    let Some(worktree) = worktree else {
        return Ok(project.to_string());
    };
    let (project, run_id) = (PathBuf::from(project), run_id.to_string());
    tokio::task::spawn_blocking(move || launch_dir(&project, Path::new(&worktree), &run_id))
        .await
        .map_err(|e| e.to_string())?
        .map(|p| p.to_string_lossy().into_owned())
}

/// Remove a worktree folder under `root` and delete its branch when merged.
/// Anything outside `root` is left alone.
fn remove(root: &Path, worktree: &Path) {
    if worktree.parent() != Some(root) {
        return;
    }
    let common = git(worktree, ["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok();
    let args: [&OsStr; 4] = ["worktree".as_ref(), "remove".as_ref(), "--force".as_ref(), worktree.as_ref()];
    if git(worktree, args).is_err() {
        // Not a worktree any more (repository moved or deleted): git cannot
        // tell us anything, the folder is still ours.
        let _ = std::fs::remove_dir_all(worktree);
    }
    if let (Some(common), Some(name)) = (common, worktree.file_name()) {
        // `-d`: a branch with commits not merged into HEAD is kept.
        let _ = Command::new("git")
            .arg("--git-dir")
            .arg(common)
            .args(["branch", "-d", &branch_for(&name.to_string_lossy())])
            .output();
    }
}

/// Startup cleanup: remove every worktree under `root` that no live worker
/// owns. Runs after `db::reconcile_interrupted_runs`, so a worker cut off by
/// the app quitting is already `cancelled`. Returns how many were removed.
pub async fn sweep(db: &Db, root: &Path) -> usize {
    let kept: HashSet<PathBuf> = match sqlx::query_scalar::<_, String>(
        "SELECT worktree_path FROM runs WHERE worktree_path IS NOT NULL AND status != 'cancelled'",
    )
    .fetch_all(db)
    .await
    {
        Ok(paths) => paths.into_iter().map(PathBuf::from).collect(),
        // Without the list every worktree would look orphaned.
        Err(_) => return 0,
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !kept.contains(&path) {
            remove(root, &path);
            removed += 1;
        }
    }
    removed
}

/// Largest patch [`diff`] returns; its file list is always complete.
const MAX_PATCH_BYTES: usize = 1 << 20;

/// `git diff` options shared by [`diff`]: plain paths, and an output the
/// user's diff config (external tools, text conversion) cannot change.
const DIFF: [&str; 7] =
    ["-c", "core.quotepath=false", "diff", "--no-color", "--no-ext-diff", "--no-renames", "--no-textconv"];

#[derive(Debug, Serialize)]
pub struct DiffFile {
    pub path: String,
    /// `None` for a binary file.
    pub added: Option<u64>,
    pub deleted: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct WorkerDiff {
    pub branch: String,
    /// The commit the worker's branch left the project's current branch at.
    pub base: String,
    pub files: Vec<DiffFile>,
    pub patch: String,
    /// `patch` was cut at [`MAX_PATCH_BYTES`].
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MergeOutcome {
    /// `commit`: the project's `HEAD` after the merge. `rescued`: what the
    /// worker had not committed was first committed on its branch.
    Merged { commit: String, rescued: bool },
    /// Nothing merged: the merge was aborted, the project is as it was and
    /// the branch keeps the work. `files`: those in conflict.
    Conflict { files: Vec<String> },
}

fn not_git() -> String {
    "The project is no longer a git repository".to_string()
}

/// A worktree the worker switched to another branch would show, and commit,
/// changes that are not on the branch being merged.
fn check_on_branch(worktree: &Path, branch: &str) -> Result<(), String> {
    match git(worktree, ["branch", "--show-current"]) {
        Ok(current) if current == branch => Ok(()),
        Ok(_) => Err(format!("The worker's worktree is no longer on its branch {branch}")),
        Err(e) => Err(format!("Could not read the worker's worktree: {e}")),
    }
}

fn numstat(out: &str) -> Vec<DiffFile> {
    out.lines()
        .filter_map(|line| {
            let mut cols = line.splitn(3, '\t');
            let (added, deleted, path) = (cols.next()?, cols.next()?, cols.next()?);
            Some(DiffFile { path: path.to_string(), added: added.parse().ok(), deleted: deleted.parse().ok() })
        })
        .collect()
}

/// What merging `worker_id` would bring into the project: the commits on its
/// branch plus what it has not committed yet (new files included, ignored
/// ones not), against the commit its branch left the project's `HEAD` at.
/// Neither the worktree nor its index changes: uncommitted files are staged
/// into a throwaway copy of the index.
pub fn diff(project: &Path, worktree: &Path, worker_id: &str) -> Result<WorkerDiff, String> {
    let (top, _) = repo_of(project).ok_or_else(not_git)?;
    let branch = branch_for(worker_id);
    let base = git(&top, ["merge-base", "HEAD", &branch])
        .map_err(|e| format!("Could not compare the worker's branch with the project: {e}"))?;
    let (stat, mut patch) = if worktree.is_dir() {
        check_on_branch(worktree, &branch)?;
        let index = std::env::temp_dir().join(format!("puppeteer-diff-{}.index", uuid::Uuid::new_v4()));
        let staged = (|| {
            // A copy keeps the stat cache, so `add` only hashes changed files.
            let real = git(worktree, ["rev-parse", "--path-format=absolute", "--git-path", "index"])?;
            std::fs::copy(real, &index).map_err(|e| e.to_string())?;
            git_with_index(worktree, Some(&index), ["add", "-A"])?;
            let cached = |extra: &str| {
                git_with_index(worktree, Some(&index), DIFF.iter().copied().chain(["--cached", extra, base.as_str()]))
            };
            Ok::<_, String>((cached("--numstat")?, cached("--patch")?))
        })();
        let _ = std::fs::remove_file(&index);
        staged.map_err(|e| format!("Could not read the worker's changes: {e}"))?
    } else {
        // The sweep removed a cancelled worker's worktree; its commits are
        // still on the branch.
        let committed = |extra: &str| git(&top, DIFF.iter().copied().chain([extra, base.as_str(), branch.as_str()]));
        (committed("--numstat")?, committed("--patch")?)
    };
    let truncated = patch.len() > MAX_PATCH_BYTES;
    if truncated {
        let cut = patch.as_bytes()[..MAX_PATCH_BYTES].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        patch.truncate(cut);
    }
    Ok(WorkerDiff { branch, base, files: numstat(&stat), patch, truncated })
}

/// Commit what the worker left uncommitted in its worktree onto its branch,
/// so the merge brings in what [`diff`] showed. `Ok(false)`: nothing to commit.
fn commit_pending(worktree: &Path, worker_id: &str) -> Result<bool, String> {
    if git(worktree, ["status", "--porcelain"])?.is_empty() {
        return Ok(false);
    }
    git(worktree, ["add", "-A"])?;
    // The worker's branch, not the user's own commit: a hook or a signing
    // prompt must not block it. The merge commit follows the user's config.
    let message = format!("Uncommitted work of worker {worker_id}");
    git(worktree, ["commit", "--no-verify", "--no-gpg-sign", "-q", "-m", &message])?;
    Ok(true)
}

/// Merge `worker_id`'s branch into the project's current branch, after
/// committing what the worker left uncommitted. A conflict is aborted and
/// reported ([`MergeOutcome::Conflict`]); the branch and worktree stay for
/// another try. Refused while the project is not on a branch, is in the middle
/// of a merge, or has uncommitted changes to tracked files (an aborted merge
/// cannot always give those back).
pub fn merge(project: &Path, worktree: &Path, worker_id: &str) -> Result<MergeOutcome, String> {
    let (top, _) = repo_of(project).ok_or_else(not_git)?;
    let branch = branch_for(worker_id);
    if git(&top, ["symbolic-ref", "--quiet", "HEAD"]).is_err() {
        return Err("The project is not on a branch (detached HEAD): check out the branch to merge into".into());
    }
    let merging = |top: &Path| git(top, ["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_ok();
    if merging(&top) {
        return Err("The project is in the middle of a merge: finish or abort it first".into());
    }
    if !git(&top, ["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err("The project has uncommitted changes: commit or stash them first".into());
    }
    let rescued = if worktree.is_dir() {
        check_on_branch(worktree, &branch)?;
        commit_pending(worktree, worker_id)
            .map_err(|e| format!("Could not commit the worker's uncommitted changes: {e}"))?
    } else {
        false
    };

    let message = format!("Merge worker branch {branch}");
    let out = Command::new("git")
        .arg("-C")
        .arg(&top)
        .args(["merge", "--no-edit", "-m", &message, &branch])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        let commit = git(&top, ["rev-parse", "HEAD"])?;
        return Ok(MergeOutcome::Merged { commit, rescued });
    }
    if !merging(&top) {
        // Refused before it started (it would overwrite untracked files, the
        // branch is gone, …): nothing to undo.
        let said = [&out.stderr[..], &out.stdout[..]].map(String::from_utf8_lossy).join("\n");
        return Err(format!("Could not merge the worker's branch: {}", said.trim()));
    }
    let files = git(&top, ["-c", "core.quotepath=false", "diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    git(&top, ["merge", "--abort"])
        .map_err(|e| format!("The merge conflicted and could not be aborted, finish it by hand: {e}"))?;
    Ok(MergeOutcome::Conflict { files })
}

/// A worker's project folder, its worktree, and whether a turn is running.
async fn locate(db: &Db, worker_id: &str) -> Result<(PathBuf, PathBuf, bool), String> {
    crate::features::require(db, FEATURE).await?;
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT p.path, r.worktree_path, r.status FROM runs r JOIN projects p ON p.id = r.project_id
         WHERE r.id = ? AND r.role = 'worker'",
    )
    .bind(worker_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    let (project, worktree, status) = row.ok_or_else(|| "worker not found".to_string())?;
    let worktree =
        worktree.ok_or_else(|| "This worker has no worktree: it worked in the project folder".to_string())?;
    Ok((PathBuf::from(project), PathBuf::from(worktree), status == "running"))
}

/// [`diff`] of a worker, gated by [`FEATURE`].
pub async fn worker_diff(db: &Db, worker_id: &str) -> Result<WorkerDiff, String> {
    let (project, worktree, _) = locate(db, worker_id).await?;
    let id = worker_id.to_string();
    tokio::task::spawn_blocking(move || diff(&project, &worktree, &id)).await.map_err(|e| e.to_string())?
}

/// [`merge`] of a worker, gated by [`FEATURE`]. Refused mid-turn: the worker
/// could still be writing the files being committed.
pub async fn worker_merge(db: &Db, worker_id: &str) -> Result<MergeOutcome, String> {
    let (project, worktree, running) = locate(db, worker_id).await?;
    if running {
        return Err("The worker is still running: merge once its turn has ended".into());
    }
    let id = worker_id.to_string();
    tokio::task::spawn_blocking(move || merge(&project, &worktree, &id)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: PathBuf,
        project: PathBuf,
        root: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    /// A git repo with one commit and an empty worktree root.
    fn fixture() -> Fixture {
        let dir = std::env::temp_dir().join(format!("puppeteer-wt-{}", uuid::Uuid::new_v4()));
        let project = dir.join("repo");
        std::fs::create_dir_all(project.join("app")).unwrap();
        std::fs::write(project.join("app").join("a.txt"), "a").unwrap();
        sh(&project, &["init", "-q"]);
        // For the commits `merge` makes itself (CI has no global identity).
        sh(&project, &["config", "user.name", "t"]);
        sh(&project, &["config", "user.email", "t@t"]);
        sh(&project, &["add", "."]);
        sh(&project, &["commit", "-qm", "init"]);
        Fixture { root: dir.join("worktrees"), project, dir }
    }

    fn branches(project: &Path) -> String {
        git(project, ["branch", "--list", "puppeteer/*"]).unwrap()
    }

    async fn db_with_workers(rows: &[(&str, &str, &Path)]) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/p', 'c')")
            .execute(&db)
            .await
            .unwrap();
        for (id, status, path) in rows {
            sqlx::query(
                "INSERT INTO runs (id, project_id, type, status, created_at, role, worktree_path)
                 VALUES (?, 'p1', 'session', ?, 'c', 'worker', ?)",
            )
            .bind(id)
            .bind(status)
            .bind(path.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        }
        db
    }

    #[test]
    fn each_worker_gets_its_own_worktree_and_branch() {
        let f = fixture();
        let w1 = create(&f.project, &f.root, "w1").unwrap().unwrap();
        let w2 = create(&f.project, &f.root, "w2").unwrap().unwrap();
        assert_eq!(w1, f.root.join("w1"));
        assert!(w1.join("app/a.txt").is_file() && w2.join("app/a.txt").is_file());
        assert_eq!(git(&w1, ["branch", "--show-current"]).unwrap(), "puppeteer/worker-w1");
        assert_eq!(git(&w2, ["branch", "--show-current"]).unwrap(), "puppeteer/worker-w2");
        // A change in one worktree is not seen by the other or the project.
        std::fs::write(w1.join("app/a.txt"), "changed").unwrap();
        assert_eq!(std::fs::read_to_string(w2.join("app/a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(f.project.join("app/a.txt")).unwrap(), "a");
    }

    #[test]
    fn a_project_outside_git_runs_in_place() {
        let dir = std::env::temp_dir().join(format!("puppeteer-wt-nogit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(create(&dir, &dir.join("worktrees"), "w1").unwrap(), None);
        assert!(!dir.join("worktrees").join("w1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_dir_follows_the_project_folder_and_restores_a_removed_worktree() {
        let f = fixture();
        // A project that is a sub-folder of its repository.
        let sub = f.project.join("app");
        let wt = create(&sub, &f.root, "w1").unwrap().unwrap();
        assert_eq!(launch_dir(&sub, &wt, "w1").unwrap(), wt.join("app"));

        // Removed with its (merged) branch: comes back on a new branch.
        remove(&f.root, &wt);
        assert!(!wt.exists() && branches(&f.project).is_empty());
        assert_eq!(launch_dir(&sub, &wt, "w1").unwrap(), wt.join("app"));
        assert!(wt.join("app/a.txt").is_file());

        // Removed with a commit on its branch: the branch is kept and reused.
        std::fs::write(wt.join("app/b.txt"), "b").unwrap();
        sh(&wt, &["add", "."]);
        sh(&wt, &["commit", "-qm", "work"]);
        remove(&f.root, &wt);
        assert!(!wt.exists());
        assert!(branches(&f.project).contains("puppeteer/worker-w1"), "unmerged work is kept");
        launch_dir(&sub, &wt, "w1").unwrap();
        assert!(wt.join("app/b.txt").is_file());

        // Deleted by hand: still registered in git, restored anyway.
        std::fs::remove_dir_all(&wt).unwrap();
        launch_dir(&sub, &wt, "w1").unwrap();
        assert!(wt.join("app/b.txt").is_file());
    }

    #[tokio::test]
    async fn assign_follows_the_switch_and_records_the_worktree() {
        let f = fixture();
        let db = db_with_workers(&[]).await;
        sqlx::query("UPDATE projects SET path = ? WHERE id = 'p1'")
            .bind(f.project.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        for id in ["w1", "w2"] {
            sqlx::query("INSERT INTO runs (id, project_id, type, created_at, role) VALUES (?, 'p1', 'session', 'c', 'worker')")
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
        }
        let stored = |id: &'static str| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, Option<String>>("SELECT worktree_path FROM runs WHERE id = ?")
                    .bind(id)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };

        // On by default.
        let wt = assign(&db, &f.root, "w1").await.unwrap().unwrap();
        assert_eq!(stored("w1").await, Some(wt.to_string_lossy().to_string()));
        let project = f.project.to_string_lossy().to_string();
        assert_eq!(run_dir(&project, stored("w1").await, "w1").await.unwrap(), wt.to_string_lossy());
        assert_eq!(run_dir(&project, None, "x").await.unwrap(), project);

        sqlx::query("INSERT INTO settings (key, value) VALUES ('feature.orchestration.conductor_worktree', 'false')")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(assign(&db, &f.root, "w2").await.unwrap(), None);
        assert_eq!(stored("w2").await, None);
        assert!(!f.root.join("w2").exists());
    }

    #[tokio::test]
    async fn sweep_removes_worktrees_of_cancelled_and_deleted_workers_only() {
        let f = fixture();
        let done = create(&f.project, &f.root, "done").unwrap().unwrap();
        let idle = create(&f.project, &f.root, "idle").unwrap().unwrap();
        let cancelled = create(&f.project, &f.root, "cancelled").unwrap().unwrap();
        let deleted = create(&f.project, &f.root, "deleted").unwrap().unwrap();
        std::fs::write(cancelled.join("dirty.txt"), "x").unwrap();
        let outside = f.dir.join("not-ours");
        std::fs::create_dir_all(&outside).unwrap();
        let db = db_with_workers(&[
            ("done", "done", &done),
            ("idle", "error", &idle),
            ("cancelled", "cancelled", &cancelled),
            // A row pointing outside the root is never touched.
            ("odd", "cancelled", &outside),
        ])
        .await;

        assert_eq!(sweep(&db, &f.root).await, 2);
        assert!(done.is_dir() && idle.is_dir() && outside.is_dir());
        assert!(!cancelled.exists() && !deleted.exists());
        let left = git(&f.project, ["worktree", "list"]).unwrap();
        assert!(!left.contains("cancelled") && !left.contains("deleted"), "{left}");
        let b = branches(&f.project);
        assert!(b.contains("worker-done") && b.contains("worker-idle"));
        assert!(!b.contains("worker-cancelled") && !b.contains("worker-deleted"), "{b}");

        // Nothing left to do on the next start; a missing root is fine.
        assert_eq!(sweep(&db, &f.root).await, 0);
        assert_eq!(sweep(&db, &f.dir.join("none")).await, 0);
    }

    #[tokio::test]
    async fn sweep_falls_back_to_deleting_a_folder_git_no_longer_knows() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        // The repository is gone: the worktree's `.git` link points nowhere.
        std::fs::remove_dir_all(&f.project).unwrap();
        let db = db_with_workers(&[]).await;
        assert_eq!(sweep(&db, &f.root).await, 1);
        assert!(!wt.exists());
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn commit_all(dir: &Path, message: &str) {
        sh(dir, &["add", "-A"]);
        sh(dir, &["commit", "-qm", message]);
    }

    fn head(dir: &Path) -> String {
        git(dir, ["rev-parse", "HEAD"]).unwrap()
    }

    fn paths(d: &WorkerDiff) -> Vec<&str> {
        let mut p: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
        p.sort();
        p
    }

    #[test]
    fn diff_shows_committed_and_uncommitted_work_without_touching_the_worktree() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("app/a.txt"), "a\ncommitted\n").unwrap();
        commit_all(&wt, "work");
        std::fs::write(wt.join("app/a.txt"), "a\ncommitted\nlater\n").unwrap();
        std::fs::write(wt.join("new.txt"), "new\n").unwrap();
        std::fs::write(wt.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(wt.join("ignored.txt"), "x").unwrap();
        // The project moves on meanwhile: not part of the worker's diff.
        std::fs::write(f.project.join("other.txt"), "o").unwrap();
        commit_all(&f.project, "project work");
        let status = git(&wt, ["status", "--porcelain"]).unwrap();

        let d = diff(&f.project, &wt, "w1").unwrap();
        assert_eq!(d.branch, "puppeteer/worker-w1");
        assert_eq!(d.base, git(&f.project, ["rev-parse", "HEAD~1"]).unwrap());
        assert_eq!(paths(&d), [".gitignore", "app/a.txt", "new.txt"]);
        let a = d.files.iter().find(|f| f.path == "app/a.txt").unwrap();
        assert_eq!((a.added, a.deleted), (Some(3), Some(1)));
        assert!(d.patch.contains("+committed") && d.patch.contains("+later") && d.patch.contains("+new"));
        assert!(!d.truncated);
        // Nothing staged in the worker's own index.
        assert_eq!(git(&wt, ["status", "--porcelain"]).unwrap(), status);
        assert!(status.contains("?? new.txt"), "{status}");
    }

    #[test]
    fn diff_reads_the_branch_once_the_worktree_is_gone() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("b.bin"), [0u8, 1, 2]).unwrap();
        commit_all(&wt, "work");
        remove(&f.root, &wt);
        let d = diff(&f.project, &wt, "w1").unwrap();
        assert_eq!(paths(&d), ["b.bin"]);
        assert_eq!((d.files[0].added, d.files[0].deleted), (None, None), "binary");

        // A branch that is gone cannot be compared.
        assert!(diff(&f.project, &wt, "nobody").is_err());
    }

    #[test]
    fn merge_brings_in_committed_and_uncommitted_work() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("app/a.txt"), "worker").unwrap();
        commit_all(&wt, "work");
        std::fs::write(wt.join("new.txt"), "new").unwrap();
        // The project moved on too: a real merge commit.
        std::fs::write(f.project.join("other.txt"), "o").unwrap();
        commit_all(&f.project, "project work");
        // Untracked files of the user's that the merge does not touch are fine.
        std::fs::write(f.project.join("notes.txt"), "mine").unwrap();

        let MergeOutcome::Merged { commit, rescued } = merge(&f.project, &wt, "w1").unwrap() else {
            panic!("expected a merge")
        };
        assert!(rescued);
        assert_eq!(commit, head(&f.project));
        assert_eq!(git(&f.project, ["log", "-1", "--format=%s"]).unwrap(), "Merge worker branch puppeteer/worker-w1");
        assert_eq!(read(&f.project.join("app/a.txt")), "worker");
        assert_eq!(read(&f.project.join("new.txt")), "new");
        assert_eq!(git(&wt, ["status", "--porcelain"]).unwrap(), "");
        // The worker keeps its worktree and branch, and can be merged again.
        assert!(branches(&f.project).contains("puppeteer/worker-w1"));
        assert_eq!(
            merge(&f.project, &wt, "w1").unwrap(),
            MergeOutcome::Merged { commit: head(&f.project), rescued: false }
        );
        assert!(diff(&f.project, &wt, "w1").unwrap().files.is_empty());
    }

    #[test]
    fn a_conflict_is_aborted_and_reported() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("app/a.txt"), "worker").unwrap();
        std::fs::write(wt.join("app/w.txt"), "w").unwrap();
        commit_all(&wt, "work");
        std::fs::write(f.project.join("app/a.txt"), "user").unwrap();
        commit_all(&f.project, "user work");
        let before = head(&f.project);
        let worker_head = head(&wt);

        assert_eq!(
            merge(&f.project, &wt, "w1").unwrap(),
            MergeOutcome::Conflict { files: vec!["app/a.txt".to_string()] }
        );
        // The project is exactly as it was: no merge in progress, nothing changed.
        assert_eq!(head(&f.project), before);
        assert!(git(&f.project, ["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_err());
        assert_eq!(git(&f.project, ["status", "--porcelain"]).unwrap(), "");
        assert_eq!(read(&f.project.join("app/a.txt")), "user");
        assert!(!f.project.join("app/w.txt").exists());
        // The worker's work is untouched.
        assert_eq!(head(&wt), worker_head);
        assert_eq!(read(&wt.join("app/a.txt")), "worker");
    }

    #[test]
    fn merge_refuses_a_project_it_could_leave_in_a_mess() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("app/a.txt"), "worker").unwrap();
        std::fs::write(wt.join("new.txt"), "worker").unwrap();

        // Uncommitted changes of the user's: refused before anything is committed.
        std::fs::write(f.project.join("app/a.txt"), "user, not committed").unwrap();
        assert!(merge(&f.project, &wt, "w1").unwrap_err().contains("uncommitted changes"));
        assert_ne!(git(&wt, ["status", "--porcelain"]).unwrap(), "", "worker's changes not committed");
        sh(&f.project, &["checkout", "-q", "--", "app/a.txt"]);

        // Detached HEAD.
        sh(&f.project, &["checkout", "-q", "--detach"]);
        assert!(merge(&f.project, &wt, "w1").unwrap_err().contains("detached HEAD"));
        sh(&f.project, &["checkout", "-q", "-"]);

        // The user's own merge in progress is left alone.
        sh(&f.project, &["checkout", "-q", "-b", "side"]);
        std::fs::write(f.project.join("app/a.txt"), "side").unwrap();
        commit_all(&f.project, "side");
        sh(&f.project, &["checkout", "-q", "-"]);
        std::fs::write(f.project.join("app/a.txt"), "main").unwrap();
        commit_all(&f.project, "main");
        let ok = Command::new("git").arg("-C").arg(&f.project).args(["merge", "-q", "side"]).output().unwrap();
        assert!(!ok.status.success(), "the user's merge conflicts");
        assert!(merge(&f.project, &wt, "w1").unwrap_err().contains("middle of a merge"));
        assert!(git(&f.project, ["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_ok());
        sh(&f.project, &["merge", "--abort"]);

        // An untracked file of the user's in the way: git refuses before starting.
        std::fs::write(f.project.join("new.txt"), "user").unwrap();
        assert!(merge(&f.project, &wt, "w1").unwrap_err().contains("Could not merge"));
        assert!(git(&f.project, ["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_err());
        assert_eq!(read(&f.project.join("new.txt")), "user");
    }

    #[test]
    fn a_worktree_moved_off_its_branch_is_refused() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        sh(&wt, &["checkout", "-q", "-b", "elsewhere"]);
        assert!(diff(&f.project, &wt, "w1").unwrap_err().contains("no longer on its branch"));
        assert!(merge(&f.project, &wt, "w1").unwrap_err().contains("no longer on its branch"));
    }

    #[test]
    fn a_long_patch_is_cut_on_a_line() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        let line = "é".repeat(100) + "\n";
        std::fs::write(wt.join("big.txt"), line.repeat(MAX_PATCH_BYTES / line.len() + 10)).unwrap();
        let d = diff(&f.project, &wt, "w1").unwrap();
        assert!(d.truncated && d.patch.len() <= MAX_PATCH_BYTES && d.patch.ends_with('\n'));
        assert_eq!(paths(&d), ["big.txt"]);
    }

    #[tokio::test]
    async fn review_and_merge_go_through_the_switch_and_the_worker_row() {
        let f = fixture();
        let wt = create(&f.project, &f.root, "w1").unwrap().unwrap();
        std::fs::write(wt.join("new.txt"), "new").unwrap();
        let db = db_with_workers(&[("w1", "running", &wt)]).await;
        sqlx::query("UPDATE projects SET path = ? WHERE id = 'p1'")
            .bind(f.project.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at, role) VALUES ('w2', 'p1', 'session', 'c', 'worker')")
            .execute(&db)
            .await
            .unwrap();

        assert_eq!(paths(&worker_diff(&db, "w1").await.unwrap()), ["new.txt"]);
        assert!(worker_merge(&db, "w1").await.unwrap_err().contains("still running"));
        assert!(worker_diff(&db, "w2").await.unwrap_err().contains("no worktree"));
        assert_eq!(worker_diff(&db, "nope").await.unwrap_err(), "worker not found");

        sqlx::query("UPDATE runs SET status = 'done' WHERE id = 'w1'").execute(&db).await.unwrap();
        assert!(matches!(worker_merge(&db, "w1").await.unwrap(), MergeOutcome::Merged { rescued: true, .. }));
        assert_eq!(read(&f.project.join("new.txt")), "new");

        sqlx::query("INSERT INTO settings (key, value) VALUES ('feature.orchestration.conductor_worktree', 'false')")
            .execute(&db)
            .await
            .unwrap();
        let off = format!("feature_disabled: {FEATURE}");
        assert_eq!(worker_diff(&db, "w1").await.unwrap_err(), off);
        assert_eq!(worker_merge(&db, "w1").await.unwrap_err(), off);
    }
}
