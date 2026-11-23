//! Worktrees of finished workers (`<app data>/worktrees/<worker id>`). The
//! startup sweep (`conductor::worktree::sweep`) only takes cancelled and
//! deleted workers; this takes a worker idle for more than N days whose
//! worktree holds no unfinished work: nothing uncommitted, and its branch
//! already merged into the project's `HEAD`. It goes through
//! `worktree::remove`, which deletes the branch only when merged. A worker
//! resumed later gets its worktree back (`worktree::launch_dir`).
//!
//! A worktree with unfinished work, at any age, is counted as kept so the user
//! can see it; a git error counts as unfinished.

use super::{fs_safe, running_run_ids, Category, Ctx, Item, Target, CAT_WORKTREES};
use crate::conductor::worktree;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Worker {
    id: String,
    project: PathBuf,
    worktree: PathBuf,
    /// Last activity, else finish time.
    idle_since: Option<String>,
}

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT r.id, p.path, r.worktree_path, COALESCE(r.last_activity_at, r.finished_at) \
         FROM runs r JOIN projects p ON p.id = r.project_id \
         WHERE r.worktree_path IS NOT NULL AND r.worktree_path <> ''",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    let running = running_run_ids(ctx).await?;
    let root = ctx.app_data.join("worktrees");
    let workers: Vec<Worker> = rows
        .into_iter()
        .filter(|(id, ..)| !running.contains(id))
        .map(|(id, project, worktree, idle_since)| Worker { id, project: project.into(), worktree: worktree.into(), idle_since })
        .filter(|w| w.worktree.parent() == Some(root.as_path()))
        .collect();
    let cutoff = ctx.now - chrono::Duration::days(i64::from(ctx.days));
    tokio::task::spawn_blocking(move || pick(&root, &workers, cutoff)).await.map_err(|e| e.to_string())
}

fn pick(root: &Path, workers: &[Worker], cutoff: DateTime<Utc>) -> Category {
    let mut category = Category::new(CAT_WORKTREES);
    // Two runs can name one folder (a resumed copy); look at it once.
    let mut seen = HashSet::new();
    for worker in workers {
        if !worker.worktree.is_dir() || !seen.insert(&worker.worktree) {
            continue;
        }
        let size = fs_safe::size_of(&worker.worktree);
        if !finished_work(&worker.project, &worker.worktree, &worker.id) {
            category.kept_items += 1;
            category.kept_bytes += size.0;
            continue;
        }
        let old = worker
            .idle_since
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|at| at.with_timezone(&Utc) < cutoff);
        if old {
            let target = Target::Worktree { root: root.to_path_buf(), path: worker.worktree.clone() };
            category.items.push(Item { target, bytes: size.0, files: size.1 });
        }
    }
    category
}

/// Nothing uncommitted in `worktree`, and the worker's branch is in the
/// project's `HEAD`. Any git failure answers no.
fn finished_work(project: &Path, worktree: &Path, worker_id: &str) -> bool {
    let clean = git(worktree, &["status", "--porcelain"]).is_some_and(|out| out.trim().is_empty());
    clean && git(project, &["merge-base", "--is-ancestor", &worktree::branch_for(worker_id), "HEAD"]).is_some()
}

/// `git -C dir args`: its output on success.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Remove a picked worktree through the conductor's own removal.
pub(super) fn remove(root: &Path, path: &Path) -> Result<(), String> {
    worktree::remove(root, path);
    if path.exists() {
        Err("the worktree is still there".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::temp_dir;
    use crate::storage_cleanup::tests::test_db;
    use crate::storage_cleanup::{apply, plan as plan_all, Plan};

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn ago(days: i64) -> String {
        (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
    }

    #[tokio::test]
    async fn only_an_old_clean_merged_worktree_goes_and_a_resume_brings_it_back() {
        let base = temp_dir("worktrees");
        let project = base.join("repo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("a.txt"), "a").unwrap();
        sh(&project, &["init", "-q"]);
        sh(&project, &["add", "."]);
        sh(&project, &["commit", "-qm", "init"]);
        let root = base.join("worktrees");
        let mut paths = Vec::new();
        for id in ["merged", "dirty", "unmerged", "recent", "live"] {
            paths.push(worktree::create(&project, &root, id).unwrap().unwrap());
        }
        std::fs::write(root.join("dirty/new.txt"), "x").unwrap();
        std::fs::write(root.join("unmerged/b.txt"), "b").unwrap();
        sh(&root.join("unmerged"), &["add", "."]);
        sh(&root.join("unmerged"), &["commit", "-qm", "work"]);

        let db = test_db(&base).await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
            .bind(project.to_string_lossy().as_ref())
            .execute(&db)
            .await
            .unwrap();
        for (id, idle, status) in [("merged", 40, "done"), ("dirty", 40, "done"), ("unmerged", 40, "done"), ("recent", 2, "done"), ("live", 40, "running")] {
            sqlx::query("INSERT INTO runs (id, project_id, type, created_at, status, role, worktree_path, last_activity_at) VALUES (?, 'p1', 'session', 'c', ?, 'worker', ?, ?)")
                .bind(id)
                .bind(status)
                .bind(root.join(id).to_string_lossy().as_ref())
                .bind(ago(idle))
                .execute(&db)
                .await
                .unwrap();
        }
        let ctx = Ctx { home: None, temp: base.join("no-tmp"), ..Ctx::new(db.clone(), base.clone(), 30, crate::runs::new_registry(), None) };
        let category = plan_all(&ctx).await.categories.into_iter().find(|c| c.id == CAT_WORKTREES).unwrap();
        let picked: Vec<_> = category.items.iter().map(|i| i.target.describe()).collect();
        assert_eq!(picked, [root.join("merged").display().to_string()]);
        assert_eq!(category.kept_items, 2, "dirty and unmerged");

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert!(!root.join("merged").exists());
        assert!(sh(&project, &["branch", "--list", "puppeteer/worker-merged"]).is_empty(), "merged branch deleted");
        for id in ["dirty", "unmerged", "recent", "live"] {
            assert!(root.join(id).is_dir(), "{id}");
        }
        assert!(root.join("dirty/new.txt").exists());

        // A resumed worker gets its worktree back.
        let dir = worktree::launch_dir(&project, &root.join("merged"), "merged").unwrap();
        assert!(dir.join("a.txt").exists());
        db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }
}
