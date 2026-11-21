//! Sessions cut off by the app (HD-04 marks them `end_reason = 'app_closed'`,
//! `interrupted_pending = 1`) and what the user does about them from the
//! banner: list them, dismiss them, or resume them.
//!
//! Nothing here runs on its own. A run may have stopped half-way through
//! writing a file, so only the user's click resumes it: no startup, timer or
//! Conductor path calls [`resume_interrupted_runs`].
//!
//! A Conductor's workers never get a row of their own: they count on their
//! conductor's row. Resuming that row tells the conductor which workers were
//! cut off, and resuming or dismissing it clears them too (their worktrees are
//! kept until then, `conductor::worktree::sweep`). A conductor that was idle
//! when the app closed, with workers mid-turn, gets a row for those workers.
//!
//! A conductor resumes in the mode it last launched with, stored since CO-05
//! (`conductor_sessions.permission_mode`). Any other run, or a conductor not
//! launched since then, resumes in the project's default permission mode, else
//! the global one: its last mode lived only in the webview. The list names the
//! mode so the banner can say it.

use std::future::Future;

use serde::Serialize;
use sqlx::Row;
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::runs::RunRegistry;

/// One banner row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterruptedRun {
    pub run_id: String,
    pub project_id: String,
    pub project_name: String,
    pub title: Option<String>,
    pub engine: String,
    /// Workers of this run cut off by the app, resumed or dismissed with it.
    pub worker_count: usize,
    /// The mode a resume from the banner runs in.
    pub permission_mode: String,
}

/// What resuming one row did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumeOutcome {
    pub run_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A run still waiting for the user; `worker_of` names a worker's conductor.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    id: String,
    worker_of: Option<String>,
}

/// A banner row before its details are read: the run, whether the app cut it
/// off itself (false: only its workers were), and its cut-off workers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    run_id: String,
    cut_off: bool,
    worker_ids: Vec<String>,
}

/// Fold workers into their conductor's row, in the order the runs come.
fn group(pending: &[Pending]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for p in pending {
        let head = p.worker_of.as_deref().unwrap_or(&p.id);
        let at = match groups.iter().position(|g| g.run_id == head) {
            Some(at) => at,
            None => {
                groups.push(Group { run_id: head.to_string(), cut_off: false, worker_ids: Vec::new() });
                groups.len() - 1
            }
        };
        match p.worker_of {
            Some(_) => groups[at].worker_ids.push(p.id.clone()),
            None => groups[at].cut_off = true,
        }
    }
    groups
}

const CUT_OFF: &str = "[session] The app closed in the middle of your last turn, so it was cut off. \
Check the files you were editing (they may be half-written), then continue the task.";

const WORKERS_NEXT: &str = "Call session_list() and session_read() before deciding; \
resend work with session_send() only if it is still needed.";

/// The message a resumed run gets: fixed English, like the conductor's wake,
/// and shown in its log like any prompt.
fn resume_message(cut_off: bool, worker_ids: &[String]) -> String {
    let workers = worker_ids.join(", ");
    match (cut_off, worker_ids.is_empty()) {
        (true, true) => CUT_OFF.to_string(),
        (true, false) => format!("{CUT_OFF} Your workers {workers} were cut off too and are now cancelled. {WORKERS_NEXT}"),
        (false, _) => format!(
            "[session] The app closed while your workers {workers} were running, so they were cut off \
             and are now cancelled. {WORKERS_NEXT}"
        ),
    }
}

/// Runs waiting for the user, outside the Trash. A worker without a conductor
/// stands as its own row rather than never being shown.
async fn load_pending(db: &Db) -> Result<Vec<Pending>, String> {
    let rows = sqlx::query(
        "SELECT id, CASE WHEN role = 'worker' THEN conductor_run_id END AS worker_of FROM runs \
         WHERE interrupted_pending = 1 AND deleted_at IS NULL \
         ORDER BY COALESCE(last_activity_at, started_at, created_at) DESC, id",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| Pending { id: r.get("id"), worker_of: r.get("worker_of") }).collect())
}

/// The mode a conductor last launched with, when stored (CO-09).
async fn last_mode(db: &Db, run_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT permission_mode FROM conductor_sessions WHERE id = ?")
        .bind(run_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
}

async fn list(db: &Db) -> Result<Vec<InterruptedRun>, String> {
    let global_mode: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'default_permission_mode'")
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let mut out = Vec::new();
    for g in group(&load_pending(db).await?) {
        let row = sqlx::query(
            "SELECT r.project_id, p.name, r.title, r.engine, p.default_permission_mode \
             FROM runs r JOIN projects p ON p.id = r.project_id \
             WHERE r.id = ? AND r.deleted_at IS NULL AND p.deleted_at IS NULL",
        )
        .bind(&g.run_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
        // A conductor in the Trash: its row comes back with it.
        let Some(row) = row else { continue };
        let project_mode: Option<String> = row.get("default_permission_mode");
        let permission_mode = match last_mode(db, &g.run_id).await {
            Some(mode) => mode,
            None => crate::commands::session_defaults::fallback_permission_mode(project_mode.as_deref(), &global_mode),
        };
        out.push(InterruptedRun {
            run_id: g.run_id,
            project_id: row.get("project_id"),
            project_name: row.get("name"),
            title: row.get("title"),
            engine: row.get("engine"),
            worker_count: g.worker_ids.len(),
            permission_mode,
        });
    }
    Ok(out)
}

/// The run and its workers stop waiting. `end_reason` stays for History.
async fn clear(db: &Db, run_id: &str) -> Result<(), String> {
    sqlx::query("UPDATE runs SET interrupted_pending = 0 WHERE id = ? OR (role = 'worker' AND conductor_run_id = ?)")
        .bind(run_id)
        .bind(run_id)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn dismiss(db: &Db, run_ids: &[String]) -> Result<(), String> {
    for id in run_ids {
        clear(db, id).await?;
    }
    Ok(())
}

/// Resume each row in turn, one at a time, through `resume(run_id, message)`
/// (resume, then send `message`). A failed row stays waiting and does not stop
/// the others.
async fn resume_each<F, Fut>(db: &Db, run_ids: &[String], mut resume: F) -> Result<Vec<ResumeOutcome>, String>
where
    F: FnMut(String, String) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let groups = group(&load_pending(db).await?);
    let mut out = Vec::with_capacity(run_ids.len());
    for id in run_ids {
        // Resumed or dismissed since the banner was drawn (another window, a
        // phone): resuming it now would be a resume nobody asked for.
        let Some(g) = groups.iter().find(|g| &g.run_id == id) else {
            out.push(ResumeOutcome {
                run_id: id.clone(),
                ok: false,
                error: Some("this session is no longer waiting to be resumed".to_string()),
            });
            continue;
        };
        let result = resume(id.clone(), resume_message(g.cut_off, &g.worker_ids)).await;
        match &result {
            Ok(()) => {
                if let Err(e) = clear(db, id).await {
                    tracing::warn!(event = "interrupted_clear_failed", run_id = %id, error = %e);
                }
            }
            // `resume_run_with` clears the mark once it marks the run running;
            // a resume that failed after that, with the run no longer live,
            // waits again. A failure before it never touched the row.
            Err(_) if g.cut_off => {
                if let Err(e) = sqlx::query(
                    "UPDATE runs SET end_reason = 'app_closed', interrupted_pending = 1 WHERE id = ? AND status != 'running'",
                )
                .bind(id)
                .execute(db)
                .await
                {
                    tracing::warn!(event = "interrupted_keep_failed", run_id = %id, error = %e);
                }
            }
            Err(_) => {}
        }
        out.push(ResumeOutcome { run_id: id.clone(), ok: result.is_ok(), error: result.err() });
    }
    Ok(out)
}

/// Resume like the composer does: `resume_run` in the mode the list named
/// (the conductor's stored one, else the default), then the message.
async fn resume_and_send(app: AppHandle, run_id: String, message: String) -> Result<(), String> {
    let mode = last_mode(app.state::<Db>().inner(), &run_id).await;
    crate::commands::runs::resume_run_with(
        app.clone(),
        app.state::<Db>(),
        app.state::<RunRegistry>(),
        run_id.clone(),
        mode,
        None,
        None,
        None,
        None,
        true,
    )
    .await?;
    crate::commands::runs::send_user_message_inner(
        app.state::<Db>().inner(),
        app.state::<RunRegistry>().inner(),
        crate::commands::runs::SendUserMessagePayload {
            run_id,
            content: message,
            images: Vec::new(),
            override_budget: false,
            quotes: Vec::new(),
            quote_set: None,
        },
        true,
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn list_interrupted_runs(app: AppHandle) -> Result<Vec<InterruptedRun>, String> {
    list(app.state::<Db>().inner()).await
}

#[tauri::command]
pub async fn dismiss_interrupted_runs(app: AppHandle, run_ids: Vec<String>) -> Result<(), String> {
    dismiss(app.state::<Db>().inner(), &run_ids).await
}

#[tauri::command]
pub async fn resume_interrupted_runs(app: AppHandle, run_ids: Vec<String>) -> Result<Vec<ResumeOutcome>, String> {
    let db = app.state::<Db>().inner().clone();
    resume_each(&db, &run_ids, |run_id, message| resume_and_send(app.clone(), run_id, message)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn pending(id: &str, worker_of: Option<&str>) -> Pending {
        Pending { id: id.into(), worker_of: worker_of.map(Into::into) }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn workers_fold_into_their_conductor() {
        let groups = group(&[
            pending("w1", Some("c")),
            pending("r", None),
            pending("c", None),
            pending("w2", Some("c")),
            // Its conductor was idle when the app closed.
            pending("iw", Some("idle")),
        ]);
        assert_eq!(
            groups,
            vec![
                Group { run_id: "c".into(), cut_off: true, worker_ids: ids(&["w1", "w2"]) },
                Group { run_id: "r".into(), cut_off: true, worker_ids: vec![] },
                Group { run_id: "idle".into(), cut_off: false, worker_ids: ids(&["iw"]) },
            ]
        );
        assert!(group(&[]).is_empty());
    }

    #[test]
    fn the_message_names_every_cut_off_worker() {
        assert_eq!(
            resume_message(true, &[]),
            "[session] The app closed in the middle of your last turn, so it was cut off. Check the files you were editing (they may be half-written), then continue the task."
        );
        assert_eq!(
            resume_message(true, &ids(&["w-1", "w-2"])),
            "[session] The app closed in the middle of your last turn, so it was cut off. Check the files you were editing (they may be half-written), then continue the task. \
             Your workers w-1, w-2 were cut off too and are now cancelled. Call session_list() and session_read() before deciding; resend work with session_send() only if it is still needed."
        );
        let idle = resume_message(false, &ids(&["w-1"]));
        assert!(idle.starts_with("[session] The app closed while your workers w-1 were running"), "{idle}");
        assert!(!idle.contains("your last turn"), "an idle conductor's own turn was not cut off: {idle}");
    }

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at, default_permission_mode) VALUES ('p1', 'one', '/a', 'c', 'plan')",
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p2', 'two', '/b', 'c')",
            "INSERT INTO settings (key, value) VALUES ('default_permission_mode', 'acceptEdits')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    /// `(id, project, engine, status, role, conductor, cut off)`; a cut-off run
    /// is what `db::reconcile_interrupted_runs` leaves.
    type TestRun<'a> = (&'a str, &'a str, &'a str, &'a str, Option<&'a str>, Option<&'a str>, bool);

    async fn add(db: &Db, runs: &[TestRun<'_>]) {
        for &(id, project, engine, status, role, conductor, cut_off) in runs {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs").fetch_one(db).await.unwrap();
            sqlx::query(
                "INSERT INTO runs (id, project_id, type, status, engine, created_at, title, role, conductor_run_id, end_reason, interrupted_pending) \
                 VALUES (?, ?, 'session', ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(project)
            .bind(status)
            .bind(engine)
            // Later rows first in the list.
            .bind(format!("2026-11-12T00:00:{n:02}Z"))
            .bind(format!("title {id}"))
            .bind(role)
            .bind(conductor)
            .bind(cut_off.then_some("app_closed"))
            .bind(cut_off as i64)
            .execute(db)
            .await
            .unwrap();
        }
    }

    async fn marks(db: &Db) -> Vec<(String, Option<String>, i64)> {
        sqlx::query_as("SELECT id, end_reason, interrupted_pending FROM runs ORDER BY id").fetch_all(db).await.unwrap()
    }

    const W: Option<&str> = Some("worker");
    const C: Option<&str> = Some("conductor");

    /// 1 conductor + 2 workers + 1 plain run cut off: 2 rows.
    async fn scenario() -> Db {
        let db = db().await;
        add(
            &db,
            &[
                ("c", "p1", "claude", "cancelled", C, None, true),
                ("w1", "p1", "claude", "cancelled", W, Some("c"), true),
                ("w2", "p1", "codex", "cancelled", W, Some("c"), true),
                // Finished before the app closed: not counted.
                ("w3", "p1", "claude", "done", W, Some("c"), false),
                ("r", "p2", "codex", "cancelled", C, None, true),
                // Stopped by the user.
                ("s", "p1", "claude", "cancelled", C, None, false),
            ],
        )
        .await;
        db
    }

    #[tokio::test]
    async fn nothing_cut_off_lists_nothing() {
        let db = db().await;
        add(&db, &[("s", "p1", "claude", "cancelled", C, None, false), ("d", "p1", "claude", "done", None, None, false)])
            .await;
        assert_eq!(list(&db).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn the_list_has_one_row_per_session_with_its_workers_counted() {
        let db = scenario().await;
        add(
            &db,
            &[
                // An idle conductor whose worker was mid-turn.
                ("i", "p2", "claude", "done", C, None, false),
                ("iw", "p2", "claude", "cancelled", W, Some("i"), true),
                // Cut off, then put in the Trash.
                ("t", "p1", "claude", "cancelled", C, None, true),
            ],
        )
        .await;
        sqlx::query("UPDATE runs SET deleted_at = 'x' WHERE id = 't'").execute(&db).await.unwrap();

        let row = |id: &str, project: &str, name: &str, engine: &str, workers: usize, mode: &str| InterruptedRun {
            run_id: id.into(),
            project_id: project.into(),
            project_name: name.into(),
            title: Some(format!("title {id}")),
            engine: engine.into(),
            worker_count: workers,
            permission_mode: mode.into(),
        };
        assert_eq!(
            list(&db).await.unwrap(),
            vec![
                row("i", "p2", "two", "claude", 1, "acceptEdits"),
                row("r", "p2", "two", "codex", 0, "acceptEdits"),
                row("c", "p1", "one", "claude", 2, "plan"),
            ]
        );

        // No global default either: the built-in one.
        sqlx::query("DELETE FROM settings").execute(&db).await.unwrap();
        let modes: Vec<String> = list(&db).await.unwrap().into_iter().map(|r| r.permission_mode).collect();
        assert_eq!(modes, ["default", "default", "plan"]);
    }

    /// CO-09: a conductor with a stored launch mode is listed (and resumed) in
    /// it; one without keeps the project's default.
    #[tokio::test]
    async fn a_conductor_is_listed_in_the_mode_it_last_ran_with() {
        let db = scenario().await;
        sqlx::query(
            "INSERT INTO conductor_sessions (id, project_id, goal, status, max_workers, created_at, permission_mode) VALUES
             ('c', 'p1', '', 'running', 3, 'x', 'default'),
             ('r', 'p2', '', 'running', 3, 'x', NULL)",
        )
        .execute(&db)
        .await
        .unwrap();
        let modes: Vec<_> = list(&db).await.unwrap().into_iter().map(|r| (r.run_id, r.permission_mode)).collect();
        assert_eq!(modes, [("r".to_string(), "acceptEdits".to_string()), ("c".to_string(), "default".to_string())]);
        assert_eq!(last_mode(&db, "c").await.as_deref(), Some("default"));
        assert_eq!(last_mode(&db, "r").await, None);
    }

    #[tokio::test]
    async fn dismissing_a_conductor_clears_its_workers_and_keeps_the_reason() {
        let db = scenario().await;
        dismiss(&db, &ids(&["c"])).await.unwrap();
        let app_closed = Some("app_closed".to_string());
        assert_eq!(
            marks(&db).await,
            vec![
                ("c".into(), app_closed.clone(), 0),
                ("r".into(), app_closed.clone(), 1),
                ("s".into(), None, 0),
                ("w1".into(), app_closed.clone(), 0),
                ("w2".into(), app_closed.clone(), 0),
                ("w3".into(), None, 0),
            ]
        );
        assert_eq!(list(&db).await.unwrap().iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(), ["r"]);
    }

    #[tokio::test]
    async fn a_failed_resume_stays_waiting_and_does_not_stop_the_others() {
        let db = scenario().await;
        add(&db, &[("q", "p2", "claude", "cancelled", C, None, true)]).await;
        let mut calls: Vec<(String, String)> = Vec::new();
        let outcomes = resume_each(&db, &ids(&["q", "c", "s"]), |id, message| {
            calls.push((id.clone(), message));
            async move {
                if id == "q" {
                    Err("Usage is over the quota block threshold".to_string())
                } else {
                    Ok(())
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(
            outcomes,
            vec![
                ResumeOutcome { run_id: "q".into(), ok: false, error: Some("Usage is over the quota block threshold".into()) },
                ResumeOutcome { run_id: "c".into(), ok: true, error: None },
                ResumeOutcome {
                    run_id: "s".into(),
                    ok: false,
                    error: Some("this session is no longer waiting to be resumed".into()),
                },
            ]
        );
        // Only the rows asked for, the stopped one not at all; the conductor
        // hears about both workers.
        assert_eq!(calls.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["q", "c"]);
        assert_eq!(calls[0].1, resume_message(true, &[]));
        assert_eq!(calls[1].1, resume_message(true, &ids(&["w2", "w1"])));

        let pending: Vec<String> =
            sqlx::query_scalar("SELECT id FROM runs WHERE interrupted_pending = 1 ORDER BY id").fetch_all(&db).await.unwrap();
        assert_eq!(pending, ["q", "r"]);
    }

    #[tokio::test]
    async fn a_resume_that_failed_after_marking_the_run_waits_again_unless_it_is_live() {
        let db = scenario().await;
        // What `resume_run_with` leaves when it fails after marking the run
        // running: `r` died at once, `c` is live but the message failed.
        let outcomes = resume_each(&db, &ids(&["r", "c"]), |id, _| {
            let db = db.clone();
            async move {
                let status = if id == "r" { "failed" } else { "running" };
                sqlx::query("UPDATE runs SET status = ?, end_reason = NULL, interrupted_pending = 0 WHERE id = ?")
                    .bind(status)
                    .bind(&id)
                    .execute(&db)
                    .await
                    .unwrap();
                Err("run not active".to_string())
            }
        })
        .await
        .unwrap();
        assert!(outcomes.iter().all(|o| !o.ok));
        let m = marks(&db).await;
        let mark = |id: &str| m.iter().find(|r| r.0 == id).map(|r| (r.1.clone(), r.2)).unwrap();
        assert_eq!(mark("r"), (Some("app_closed".into()), 1));
        assert_eq!(mark("c"), (None, 0));
        // Its workers were not resumed: they still wait.
        assert_eq!((mark("w1").1, mark("w2").1), (1, 1));
    }

    #[test]
    fn the_commands_are_registered_and_remote_may_call_them() {
        let lib = include_str!("../lib.rs");
        for name in ["list_interrupted_runs", "dismiss_interrupted_runs", "resume_interrupted_runs"] {
            assert!(lib.contains(&format!("runs::interrupted::{name},")), "{name} is not in generate_handler!");
            assert!(!crate::remote::guard::is_local_only_command(name), "{name}");
            assert!(!crate::remote::guard::audited_command(name), "{name}");
        }
    }

    /// HD-AC-06: nothing resumes a cut-off run without the user. The command is
    /// the only way in, and nothing but the invoke handler names it.
    #[test]
    fn only_the_user_can_resume_a_cut_off_run() {
        fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let mut files = Vec::new();
        sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut uses = Vec::new();
        for (file, src) in files.iter().filter(|(f, _)| !f.ends_with("interrupted.rs")) {
            for line in src.lines() {
                if ["resume_interrupted_runs", "resume_each", "resume_and_send"].iter().any(|n| line.contains(n)) {
                    uses.push(format!("{file}: {}", line.trim()));
                }
            }
        }
        assert_eq!(uses.len(), 1, "{uses:#?}");
        assert!(uses[0].ends_with("runs::interrupted::resume_interrupted_runs,"), "{uses:#?}");

        // Inside this file, the command is the only caller.
        let own = include_str!("interrupted.rs");
        let code = &own[..own.find("#[cfg(test)]").unwrap()];
        assert_eq!(code.matches("resume_each(&").count(), 1);
        assert_eq!(code.matches("resume_and_send(app.clone()").count(), 1);
    }
}
