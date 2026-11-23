//! The Trash: runs and projects trashed more than N days ago are purged exactly
//! like a permanent delete from the Trash screen (`purge_run_tree`,
//! `purge_project_inner`), so their logs, pastes, transcripts and rows go
//! together. A protected or running run stays, and so does a project holding
//! one. The sizes are an estimate: a transcript's copies in other accounts are
//! not counted.

use super::{running_run_ids, Category, Ctx, Item, Target, CAT_TRASH};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

struct TrashedRun {
    id: String,
    project_id: String,
    conductor_run_id: Option<String>,
    deleted_at: Option<String>,
    project_deleted_at: Option<String>,
    protected: bool,
    files: Vec<PathBuf>,
}

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let rows = sqlx::query(
        "SELECT r.id, r.project_id, r.conductor_run_id, r.deleted_at, r.protected, r.transcript_path, \
                p.path AS project_path, p.deleted_at AS project_deleted_at \
         FROM runs r JOIN projects p ON p.id = r.project_id \
         WHERE r.deleted_at IS NOT NULL OR p.deleted_at IS NOT NULL",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    let mut runs = Vec::new();
    for row in rows {
        let id: String = row.try_get("id").map_err(|e| e.to_string())?;
        let project_path: String = row.try_get("project_path").map_err(|e| e.to_string())?;
        let transcript: Option<String> = row.try_get("transcript_path").map_err(|e| e.to_string())?;
        let files = run_files(&project_path, &id, transcript);
        runs.push(TrashedRun {
            project_id: row.try_get("project_id").map_err(|e| e.to_string())?,
            conductor_run_id: row.try_get("conductor_run_id").map_err(|e| e.to_string())?,
            deleted_at: row.try_get("deleted_at").map_err(|e| e.to_string())?,
            project_deleted_at: row.try_get("project_deleted_at").map_err(|e| e.to_string())?,
            protected: row.try_get::<i64, _>("protected").map_err(|e| e.to_string())? != 0,
            id,
            files,
        });
    }
    // Projects in the Trash with no run left still go.
    let empty: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT p.id, p.deleted_at FROM projects p \
         WHERE p.deleted_at IS NOT NULL AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.project_id = p.id)",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    let running = running_run_ids(ctx).await?;
    let cutoff = ctx.now - chrono::Duration::days(i64::from(ctx.days));
    tokio::task::spawn_blocking(move || pick(&runs, &empty, &running, cutoff)).await.map_err(|e| e.to_string())
}

/// A run's own files: its log, its run folder and its transcript.
pub(super) fn run_files(project_path: &str, id: &str, transcript: Option<String>) -> Vec<PathBuf> {
    let runs_dir = Path::new(project_path).join(".puppeteer").join("runs");
    let mut files = vec![runs_dir.join(format!("{id}.log")), runs_dir.join(id)];
    files.extend(transcript.filter(|t| !t.is_empty()).map(PathBuf::from));
    files
}

/// Trashed before `cutoff`; a stamp that does not parse is never old.
fn older(stamp: Option<&str>, cutoff: DateTime<Utc>) -> bool {
    stamp
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|at| at.with_timezone(&Utc) < cutoff)
}

fn size(run: &TrashedRun) -> (u64, u64) {
    run.files.iter().map(|f| super::fs_safe::size_of(f)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
}

fn pick(runs: &[TrashedRun], empty: &[(String, Option<String>)], running: &HashSet<String>, cutoff: DateTime<Utc>) -> Category {
    let mut category = Category::new(CAT_TRASH);
    let blocked = |r: &TrashedRun| r.protected || running.contains(&r.id);

    // Runs of a trashed project go with the project, all or none.
    let mut projects: HashMap<&str, Vec<&TrashedRun>> = HashMap::new();
    for run in runs.iter().filter(|r| r.project_deleted_at.is_some()) {
        projects.entry(&run.project_id).or_default().push(run);
    }
    let mut project_ids: Vec<&str> = projects.keys().copied().collect();
    project_ids.sort();
    for id in project_ids {
        let members = &projects[id];
        if !older(members[0].project_deleted_at.as_deref(), cutoff) {
            continue;
        }
        let (bytes, files) = members.iter().map(|r| size(r)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        if members.iter().any(|r| blocked(r)) {
            category.kept_items += 1;
            category.kept_bytes += bytes;
        } else {
            category.items.push(Item { target: Target::Project(id.to_string()), bytes, files });
        }
    }
    for (id, deleted_at) in empty {
        if older(deleted_at.as_deref(), cutoff) {
            category.items.push(Item { target: Target::Project(id.clone()), bytes: 0, files: 0 });
        }
    }

    // Trashed runs of live projects; a worker goes with its trashed session.
    let old: Vec<&TrashedRun> = runs
        .iter()
        .filter(|r| r.project_deleted_at.is_none() && older(r.deleted_at.as_deref(), cutoff))
        .collect();
    let picked: HashSet<&str> = old.iter().filter(|r| !blocked(r)).map(|r| r.id.as_str()).collect();
    for run in old {
        if blocked(run) {
            category.kept_items += 1;
            category.kept_bytes += size(run).0;
        } else if !run.conductor_run_id.as_deref().is_some_and(|c| picked.contains(c)) {
            let (bytes, files) = size(run);
            category.items.push(Item { target: Target::Run(run.id.clone()), bytes, files });
        }
    }
    category
}

/// Purge one picked run (with its trashed workers) or project, like the Trash
/// screen does, and drop the purged runs' Remote Control audit.
pub(super) async fn purge(ctx: &Ctx, target: &Target) -> Result<(), String> {
    let conductor = ctx.conductor.as_ref();
    let outcome = match target {
        Target::Run(id) => {
            if !crate::commands::runs::run_in_trash(&ctx.db, id).await? {
                return Err("no longer in the Trash".into());
            }
            crate::commands::runs::purge_run_tree(&ctx.db, &ctx.registry, conductor, id).await?
        }
        Target::Project(id) => {
            let outcome = crate::commands::projects::purge_project_inner(&ctx.db, &ctx.registry, conductor, id).await?;
            crate::aws_sandbox::remove_project_aws_config(&ctx.app_data, id);
            outcome
        }
        _ => return Err("not a Trash entry".into()),
    };
    for id in &outcome.deleted_ids {
        if let Err(e) = crate::remote::audit::delete_for_run(&ctx.db, id).await {
            tracing::warn!(event = "storage_cleanup_audit_prune_failed", run_id = %id, error = %e);
        }
    }
    for path in &outcome.leftover_files {
        tracing::warn!(event = "storage_cleanup_transcript_left", path = %path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::{temp_dir, write};
    use crate::storage_cleanup::tests::test_db;
    use crate::storage_cleanup::{apply, plan as plan_all, Plan};

    fn ago(days: i64) -> String {
        (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
    }

    async fn ctx_with(base: &Path) -> Ctx {
        let db = test_db(base).await;
        Ctx { home: None, temp: base.join("no-tmp"), ..Ctx::new(db, base.to_path_buf(), 30, crate::runs::new_registry(), None) }
    }

    async fn insert_project(ctx: &Ctx, id: &str, path: &Path, deleted_at: Option<String>) {
        sqlx::query("INSERT INTO projects (id, name, path, created_at, deleted_at) VALUES (?, ?, ?, 'c', ?)")
            .bind(id)
            .bind(id)
            .bind(path.to_string_lossy().as_ref())
            .bind(deleted_at)
            .execute(&ctx.db)
            .await
            .unwrap();
    }

    async fn insert_run(ctx: &Ctx, id: &str, project: &str, deleted_at: Option<String>, extra: &str) {
        sqlx::query(&format!(
            "INSERT INTO runs (id, project_id, type, created_at, status, deleted_at) VALUES (?, ?, 'session', 'c', 'done', ?); {extra}"
        ))
        .bind(id)
        .bind(project)
        .bind(deleted_at)
        .execute(&ctx.db)
        .await
        .unwrap();
    }

    async fn run_ids(ctx: &Ctx) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM runs ORDER BY id").fetch_all(&ctx.db).await.unwrap()
    }

    #[tokio::test]
    async fn an_old_trashed_run_is_purged_and_the_others_stay() {
        let base = temp_dir("trash-runs");
        let ctx = ctx_with(&base).await;
        let proj = base.join("proj");
        insert_project(&ctx, "p1", &proj, None).await;
        insert_run(&ctx, "old", "p1", Some(ago(40)), "").await;
        insert_run(&ctx, "old-worker", "p1", Some(ago(40)), "UPDATE runs SET conductor_run_id = 'old', role = 'worker' WHERE id = 'old-worker'").await;
        insert_run(&ctx, "recent", "p1", Some(ago(5)), "").await;
        insert_run(&ctx, "kept-protected", "p1", Some(ago(40)), "UPDATE runs SET protected = 1 WHERE id = 'kept-protected'").await;
        insert_run(&ctx, "kept-pending", "p1", Some(ago(40)), "UPDATE runs SET interrupted_pending = 1 WHERE id = 'kept-pending'").await;
        insert_run(&ctx, "kept-live", "p1", Some(ago(40)), "UPDATE runs SET status = 'running' WHERE id = 'kept-live'").await;
        insert_run(&ctx, "in-history", "p1", None, "").await;
        insert_run(&ctx, "bad-stamp", "p1", Some("d".into()), "").await;
        write(&proj.join(".puppeteer/runs/old.log"));
        write(&proj.join(".puppeteer/runs/old/turn.json"));
        write(&proj.join(".puppeteer/runs/recent.log"));

        let category = plan_all(&ctx).await.categories.into_iter().find(|c| c.id == CAT_TRASH).unwrap();
        let picked: Vec<_> = category.items.iter().map(|i| i.target.describe()).collect();
        assert_eq!(picked, ["run old"], "the worker goes with its session");
        assert_eq!((category.items[0].files, category.kept_items), (2, 3));

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert_eq!(run_ids(&ctx).await, ["bad-stamp", "in-history", "kept-live", "kept-pending", "kept-protected", "recent"]);
        assert!(!proj.join(".puppeteer/runs/old.log").exists() && !proj.join(".puppeteer/runs/old").exists());
        assert!(proj.join(".puppeteer/runs/recent.log").exists());
        ctx.db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn an_old_trashed_project_goes_with_its_runs_unless_one_is_protected() {
        let base = temp_dir("trash-projects");
        let ctx = ctx_with(&base).await;
        insert_project(&ctx, "gone", &base.join("gone"), Some(ago(40))).await;
        insert_run(&ctx, "g1", "gone", None, "").await;
        insert_run(&ctx, "g2", "gone", Some(ago(1)), "").await;
        insert_project(&ctx, "guarded", &base.join("guarded"), Some(ago(40))).await;
        insert_run(&ctx, "s1", "guarded", None, "UPDATE runs SET protected = 1 WHERE id = 's1'").await;
        insert_project(&ctx, "fresh", &base.join("fresh"), Some(ago(3))).await;
        insert_run(&ctx, "f1", "fresh", None, "").await;
        insert_project(&ctx, "empty", &base.join("empty"), Some(ago(40))).await;

        let category = plan_all(&ctx).await.categories.into_iter().find(|c| c.id == CAT_TRASH).unwrap();
        let mut picked: Vec<_> = category.items.iter().map(|i| i.target.describe()).collect();
        picked.sort();
        assert_eq!(picked, ["project empty", "project gone"]);
        assert_eq!(category.kept_items, 1);

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert_eq!(run_ids(&ctx).await, ["f1", "s1"]);
        let projects: Vec<String> = sqlx::query_scalar("SELECT id FROM projects ORDER BY id").fetch_all(&ctx.db).await.unwrap();
        assert_eq!(projects, ["fresh", "guarded"]);
        ctx.db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }
}
