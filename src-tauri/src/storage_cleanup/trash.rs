//! The Trash: runs and projects trashed more than N days ago are purged exactly
//! like a permanent delete from the Trash screen (`purge_run_tree`,
//! `purge_project_inner`), so their logs, pastes, transcripts and rows go
//! together. A protected or running run stays, and so does a project or a
//! session holding one (or a worker trashed less than N days ago, or one in a
//! project that stays). The sizes are an estimate: a transcript's copies in
//! other accounts are not counted.

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
    // A session's worker can sit in one of these projects (see below).
    let mut kept_projects = HashSet::new();
    // Projects whose runs' sizes are already counted, picked or kept.
    let mut sized_projects = HashSet::new();
    for id in project_ids {
        let members = &projects[id];
        if !older(members[0].project_deleted_at.as_deref(), cutoff) {
            kept_projects.insert(id);
            continue;
        }
        let (bytes, files) = members.iter().map(|r| size(r)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        sized_projects.insert(id);
        if members.iter().any(|r| blocked(r)) {
            kept_projects.insert(id);
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

    // Trashed runs of live projects. `purge_run_tree` takes every trashed
    // worker of a session, so a session and its workers go together, all or
    // none: one worker that is blocked or trashed less than N days ago keeps
    // the whole session. So does a worker in a trashed project that stays: a
    // kept project stays whole. A worker in an old trashed project is counted
    // with that project, not again with its session.
    let mut workers: HashMap<&str, Vec<&TrashedRun>> = HashMap::new();
    for run in runs {
        if let Some(session) = run.conductor_run_id.as_deref().filter(|c| *c != run.id) {
            workers.entry(session).or_default().push(run);
        }
    }
    let old: Vec<&TrashedRun> = runs
        .iter()
        .filter(|r| r.project_deleted_at.is_none() && older(r.deleted_at.as_deref(), cutoff))
        .collect();
    let sessions: HashSet<&str> = old.iter().map(|r| r.id.as_str()).collect();
    for run in old {
        if run.conductor_run_id.as_deref().is_some_and(|c| c != run.id && sessions.contains(c)) {
            continue; // goes or stays with its session
        }
        let tree: Vec<&TrashedRun> = std::iter::once(run).chain(workers.get(run.id.as_str()).into_iter().flatten().copied()).collect();
        let (bytes, files) = tree
            .iter()
            .filter(|r| !sized_projects.contains(r.project_id.as_str()))
            .map(|r| size(r))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        // A worker may be in the Trash only with its project (no own stamp).
        let recent = |r: &TrashedRun| !older(r.deleted_at.as_deref().or(r.project_deleted_at.as_deref()), cutoff);
        if tree.iter().any(|r| blocked(r) || recent(r) || kept_projects.contains(r.project_id.as_str())) {
            category.kept_items += 1;
            category.kept_bytes += bytes;
        } else {
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
    async fn an_old_trashed_session_stays_while_one_of_its_workers_must_stay() {
        let base = temp_dir("trash-workers");
        let ctx = ctx_with(&base).await;
        let proj = base.join("proj");
        insert_project(&ctx, "p1", &proj, None).await;
        let worker = |id: &str, session: &str, extra: &str| {
            format!("UPDATE runs SET conductor_run_id = '{session}', role = 'worker'{extra} WHERE id = '{id}'")
        };
        for s in ["s", "s2", "s3", "s4"] {
            insert_run(&ctx, s, "p1", Some(ago(40)), "").await;
        }
        insert_run(&ctx, "w-protected", "p1", Some(ago(40)), &worker("w-protected", "s", ", protected = 1")).await;
        insert_run(&ctx, "w-recent", "p1", Some(ago(2)), &worker("w-recent", "s2", "")).await;
        insert_run(&ctx, "w-pending", "p1", Some(ago(40)), &worker("w-pending", "s3", ", interrupted_pending = 1")).await;
        insert_run(&ctx, "w-old", "p1", Some(ago(40)), &worker("w-old", "s4", "")).await;
        let logs = ["s", "s2", "s3", "s4", "w-protected", "w-recent", "w-pending", "w-old"];
        for id in logs {
            write(&proj.join(format!(".puppeteer/runs/{id}.log")));
        }

        let category = plan_all(&ctx).await.categories.into_iter().find(|c| c.id == CAT_TRASH).unwrap();
        let picked: Vec<_> = category.items.iter().map(|i| i.target.describe()).collect();
        assert_eq!(picked, ["run s4"], "a session goes only with all of its workers");
        assert_eq!((category.items[0].files, category.kept_items), (2, 3), "the worker's log is counted with its session");

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert_eq!(run_ids(&ctx).await, ["s", "s2", "s3", "w-pending", "w-protected", "w-recent"]);
        for id in logs {
            let gone = id == "s4" || id == "w-old";
            assert_eq!(proj.join(format!(".puppeteer/runs/{id}.log")).exists(), !gone, "{id}.log");
        }
        ctx.db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// A Session's worker can sit in another, trashed Project. A kept Project
    /// stays whole, so the Session stays too; a picked one counts the worker.
    #[tokio::test]
    async fn an_old_trashed_session_stays_while_a_worker_is_in_a_kept_project() {
        let base = temp_dir("trash-cross-project");
        let ctx = ctx_with(&base).await;
        let (proj, p3, p4) = (base.join("proj"), base.join("p3"), base.join("p4"));
        insert_project(&ctx, "p1", &proj, None).await;
        insert_project(&ctx, "p3", &p3, Some(ago(40))).await;
        insert_project(&ctx, "p4", &p4, Some(ago(40))).await;
        let worker = |id: &str, session: &str| format!("UPDATE runs SET conductor_run_id = '{session}', role = 'worker' WHERE id = '{id}'");
        insert_run(&ctx, "y", "p1", Some(ago(40)), "").await;
        insert_run(&ctx, "w-proj-old", "p3", Some(ago(40)), &worker("w-proj-old", "y")).await;
        insert_run(&ctx, "p3-guard", "p3", None, "UPDATE runs SET protected = 1 WHERE id = 'p3-guard'").await;
        insert_run(&ctx, "z", "p1", Some(ago(40)), "").await;
        insert_run(&ctx, "w-proj-gone", "p4", Some(ago(40)), &worker("w-proj-gone", "z")).await;
        for (dir, id) in [(&proj, "y"), (&p3, "w-proj-old"), (&proj, "z"), (&p4, "w-proj-gone")] {
            write(&dir.join(format!(".puppeteer/runs/{id}.log")));
        }

        let category = plan_all(&ctx).await.categories.into_iter().find(|c| c.id == CAT_TRASH).unwrap();
        let picked: Vec<_> = category.items.iter().map(|i| (i.target.describe(), i.files)).collect();
        assert_eq!(picked, [("project p4".to_string(), 1), ("run z".to_string(), 1)], "the worker's log is counted once, with its Project");
        assert_eq!(category.kept_items, 2, "p3 and y");
        let p3_log = crate::storage_cleanup::fs_safe::size_of(&p3.join(".puppeteer/runs/w-proj-old.log")).0;
        let y_log = crate::storage_cleanup::fs_safe::size_of(&proj.join(".puppeteer/runs/y.log")).0;
        assert_eq!(category.kept_bytes, p3_log + y_log, "the worker's log is kept once, with its Project");

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert_eq!(run_ids(&ctx).await, ["p3-guard", "w-proj-old", "y"]);
        let projects: Vec<String> = sqlx::query_scalar("SELECT id FROM projects ORDER BY id").fetch_all(&ctx.db).await.unwrap();
        assert_eq!(projects, ["p1", "p3"]);
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
