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

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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
    let out = Command::new("git")
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
}
