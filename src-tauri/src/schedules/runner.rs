//! The schedule loop: every 30 s, while [`FEATURE`] is on, fire each enabled
//! schedule that came due. Like the Focus reminders it reads the switch on
//! every tick, so turning it off stops the firing within one tick; the
//! [`on_features_changed`] hook only moves the slots on when it comes back.
//!
//! Per due schedule, in this order:
//! 1. claim it: move `next_run_at` on from now (switch a `once` off) before
//!    anything runs, so a slow start cannot make the next tick fire it again,
//!    and however many slots were missed (app closed, machine asleep) it runs
//!    once — the "wake-up" catch-up of AWOG `schedules/cron.ts`;
//! 2. skip it (`last_status = 'skipped'`) while the session it opened last
//!    time is still running;
//! 3. fire: open a new session in its project and start it with the prompt
//!    through `start_run_inner`, as the Start button would.

use super::{next_run, parse_stamp, stamp, CHANGED_EVENT, FEATURE};
use crate::db::Db;
use crate::features::{self, apply::Change};
use chrono::{DateTime, Local, Utc};
use std::future::Future;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const TICK: Duration = Duration::from_secs(30);
const FIRST_TICK: Duration = Duration::from_secs(20);
/// `last_message` is shown in a list row.
const MAX_MESSAGE_CHARS: usize = 500;

/// What firing one schedule needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Firing {
    pub schedule_id: String,
    pub project_id: String,
    pub name: String,
    pub prompt: String,
    pub engine: Option<String>,
    pub permission_mode: String,
}

/// Schedule ids by outcome, for tests and the log.
#[derive(Debug, Default, PartialEq)]
pub struct TickResult {
    pub fired: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
}

impl TickResult {
    fn is_empty(&self) -> bool {
        self.fired.is_empty() && self.skipped.is_empty() && self.failed.is_empty()
    }
}

/// One pass at `now`. `fire` opens and starts the session and returns its
/// run id. Nothing is read beyond the switch while [`FEATURE`] is off.
pub async fn tick<F, Fut>(db: &Db, now: DateTime<Local>, mut fire: F) -> Result<TickResult, String>
where
    F: FnMut(Firing) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut out = TickResult::default();
    if !features::is_on(db, FEATURE).await {
        return Ok(out);
    }
    let now_utc = now.with_timezone(&Utc);
    for s in super::list(db).await? {
        if !s.enabled || s.proposed {
            continue;
        }
        let Some(due) = s.next_run_at.as_deref().and_then(parse_stamp) else { continue };
        if due > now_utc {
            continue;
        }
        let next = s.timing.as_ref().and_then(|t| next_run(t, &now));
        // Conditional on the row read above: a switch-off or an edit in between wins.
        let claimed = sqlx::query(
            "UPDATE schedules SET next_run_at = ?, enabled = ?, updated_at = ?
             WHERE id = ? AND enabled = 1 AND proposed = 0 AND next_run_at = ?",
        )
        .bind(next.map(stamp))
        .bind(next.is_some())
        .bind(now_utc.to_rfc3339())
        .bind(&s.id)
        .bind(&s.next_run_at)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected()
            == 1;
        if !claimed {
            continue;
        }
        if let Some(run_id) = still_running(db, s.last_run_id.as_deref()).await? {
            record(db, &s.id, now_utc, "skipped", None, Some(format!("session {run_id} is still running"))).await?;
            out.skipped.push(s.id);
            continue;
        }
        let firing = Firing {
            schedule_id: s.id.clone(),
            project_id: s.project_id,
            name: s.name,
            prompt: s.prompt,
            engine: s.engine,
            permission_mode: s.permission_mode,
        };
        match fire(firing).await {
            Ok(run_id) => {
                record(db, &s.id, now_utc, "started", Some(&run_id), None).await?;
                out.fired.push(s.id);
            }
            Err(e) => {
                tracing::warn!(event = "schedule_fire_failed", schedule_id = %s.id, error = %e);
                record(db, &s.id, now_utc, "error", None, Some(e)).await?;
                out.failed.push(s.id);
            }
        }
    }
    Ok(out)
}

/// `run_id` when that run is still going.
async fn still_running(db: &Db, run_id: Option<&str>) -> Result<Option<String>, String> {
    let Some(run_id) = run_id else { return Ok(None) };
    let running: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE id = ? AND status = 'running')")
        .bind(run_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(running.then(|| run_id.to_string()))
}

async fn record(
    db: &Db,
    id: &str,
    at: DateTime<Utc>,
    status: &str,
    run_id: Option<&str>,
    message: Option<String>,
) -> Result<(), String> {
    let message = message.map(|m| m.chars().take(MAX_MESSAGE_CHARS).collect::<String>());
    sqlx::query(
        "UPDATE schedules SET last_run_at = ?, last_status = ?, last_message = ?,
            last_run_id = COALESCE(?, last_run_id)
         WHERE id = ?",
    )
    .bind(stamp(at))
    .bind(status)
    .bind(message)
    .bind(run_id)
    .bind(id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Start every enabled schedule afresh from `now`: what came due while the
/// switch was off is dropped, not fired at once on the next tick (a `once`
/// that passed meanwhile is switched off).
pub async fn reanchor(db: &Db, now: DateTime<Local>) -> Result<(), String> {
    for s in super::list(db).await? {
        if !s.enabled || s.proposed {
            continue;
        }
        let next = s.timing.as_ref().and_then(|t| next_run(t, &now));
        sqlx::query("UPDATE schedules SET next_run_at = ?, enabled = ? WHERE id = ? AND enabled = 1")
            .bind(next.map(stamp))
            .bind(next.is_some())
            .bind(&s.id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// `features::apply` hook: turning [`FEATURE`] back on re-anchors the slots.
pub fn on_features_changed(app: &AppHandle, change: &Change<'_>) {
    if features::effective(change.before, FEATURE) || !features::effective(change.after, FEATURE) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let db = app.state::<Db>().inner().clone();
        match reanchor(&db, Local::now()).await {
            Ok(()) => {
                let _ = app.emit(CHANGED_EVENT, ());
            }
            Err(e) => tracing::warn!(event = "schedule_reanchor_failed", error = %e),
        }
    });
}

/// Open a session in the schedule's project and start it with the prompt —
/// the same steps as a session started from the app (`create_session_run`,
/// then `start_run_inner`). A start that fails leaves no empty session.
async fn fire(app: &AppHandle, db: &Db, f: Firing) -> Result<String, String> {
    let registry = app
        .try_state::<crate::runs::RunRegistry>()
        .ok_or("the run registry is not ready")?
        .inner()
        .clone();
    let engine = match f.engine {
        Some(e) => e,
        None => crate::commands::settings::resolve_default_engine(db).await,
    };
    let run_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO runs (id, project_id, type, status, engine, created_at, title)
         VALUES (?, ?, 'session', 'fetched', ?, ?, ?)",
    )
    .bind(&run_id)
    .bind(&f.project_id)
    .bind(&engine)
    .bind(Utc::now().to_rfc3339())
    .bind(&f.name)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    let payload = crate::commands::runs::StartRunPayload {
        run_id: run_id.clone(),
        engine_override: None,
        permission_mode_override: Some(f.permission_mode),
        prompt_override: Some(f.prompt),
        model_override: None,
        images: Vec::new(),
        // A scheduled run never goes past the budget guard: nobody is there to confirm.
        override_budget: false,
        append_system_prompt: None,
        conductor_token: None,
    };
    if let Err(e) = crate::commands::runs::start_run_inner(app.clone(), db.clone(), registry, payload).await {
        for sql in ["DELETE FROM conductor_sessions WHERE id = ?", "DELETE FROM runs WHERE id = ?"] {
            let _ = sqlx::query(sql).bind(&run_id).execute(db).await;
        }
        return Err(e);
    }
    Ok(run_id)
}

/// Start the loop (app setup). Runs while the app runs.
pub fn start(app: AppHandle, db: Db) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_TICK).await;
        loop {
            match tick(&db, Local::now(), |f| fire(&app, &db, f)).await {
                Ok(r) if !r.is_empty() => {
                    tracing::info!(event = "schedules_tick", fired = ?r.fired, skipped = ?r.skipped, failed = ?r.failed);
                    let _ = app.emit(CHANGED_EVENT, ());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(event = "schedules_tick_failed", error = %e),
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::tests::{db, input};
    use super::super::{get, save, set_enabled, Timing};
    use super::*;
    use std::sync::{Arc, Mutex};

    async fn switch(db: &Db, on: bool) {
        crate::commands::settings::set_setting(db, "feature.orchestration.schedules", if on { "true" } else { "false" })
            .await
            .unwrap();
    }

    /// An enabled every-30-minutes schedule whose slot is `due`.
    async fn due_schedule(db: &Db, due: &str) -> String {
        let id = save(db, input(None, Timing::Interval { every_minutes: 30 }), false, &Local::now())
            .await
            .unwrap()
            .id;
        set_enabled(db, &id, true, &Local::now()).await.unwrap();
        sqlx::query("UPDATE schedules SET next_run_at = ? WHERE id = ?").bind(due).bind(&id).execute(db).await.unwrap();
        id
    }

    fn recorder() -> (Arc<Mutex<Vec<Firing>>>, impl FnMut(Firing) -> std::future::Ready<Result<String, String>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let inner = seen.clone();
        (seen, move |f: Firing| {
            let mut v = inner.lock().unwrap();
            v.push(f);
            std::future::ready(Ok(format!("run-{}", v.len())))
        })
    }

    fn now() -> DateTime<Local> {
        Local::now()
    }

    #[tokio::test]
    async fn nothing_fires_while_the_switch_is_off() {
        let db = db().await;
        let id = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        let before = get(&db, &id).await.unwrap();
        let (seen, fire) = recorder();
        // Fresh install: the prop is off by default.
        assert_eq!(tick(&db, now(), fire).await.unwrap(), TickResult::default());
        switch(&db, false).await;
        let (_, fire) = recorder();
        assert_eq!(tick(&db, now(), fire).await.unwrap(), TickResult::default());
        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(get(&db, &id).await.unwrap(), before, "the row is left as it was");
    }

    #[tokio::test]
    async fn a_due_schedule_fires_once_and_moves_its_slot_on() {
        let db = db().await;
        switch(&db, true).await;
        let id = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        let (seen, mut fire) = recorder();
        let t = now();
        let r = tick(&db, t, &mut fire).await.unwrap();
        assert_eq!(r.fired, [id.clone()]);
        let f = seen.lock().unwrap()[0].clone();
        assert_eq!((f.project_id.as_str(), f.prompt.as_str(), f.permission_mode.as_str()), ("p1", "Review the open PRs", "plan"));

        let s = get(&db, &id).await.unwrap();
        assert_eq!(s.last_status.as_deref(), Some("started"));
        assert_eq!(s.last_run_id.as_deref(), Some("run-1"));
        let next = parse_stamp(s.next_run_at.as_deref().unwrap()).unwrap();
        assert!(next > t.with_timezone(&Utc), "years of missed slots run once, then the next is ahead");

        // The same moment again: nothing is due.
        assert!(tick(&db, t, &mut fire).await.unwrap().is_empty());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn off_proposed_and_future_schedules_do_not_fire() {
        let db = db().await;
        switch(&db, true).await;
        let off = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        sqlx::query("UPDATE schedules SET enabled = 0 WHERE id = ?").bind(&off).execute(&db).await.unwrap();
        let proposed = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        sqlx::query("UPDATE schedules SET proposed = 1 WHERE id = ?").bind(&proposed).execute(&db).await.unwrap();
        due_schedule(&db, "2999-01-01T00:00:00Z").await;
        let (seen, fire) = recorder();
        assert!(tick(&db, now(), fire).await.unwrap().is_empty());
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_session_still_running_skips_the_slot() {
        let db = db().await;
        switch(&db, true).await;
        let id = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        sqlx::query("INSERT INTO runs (id, project_id, type, status, created_at) VALUES ('r1', 'p1', 'session', 'running', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE schedules SET last_run_id = 'r1' WHERE id = ?").bind(&id).execute(&db).await.unwrap();
        let (seen, fire) = recorder();
        assert_eq!(tick(&db, now(), fire).await.unwrap().skipped, [id.clone()]);
        assert!(seen.lock().unwrap().is_empty());
        let s = get(&db, &id).await.unwrap();
        assert_eq!(s.last_status.as_deref(), Some("skipped"));
        assert_eq!(s.last_run_id.as_deref(), Some("r1"));
        assert!(parse_stamp(s.next_run_at.as_deref().unwrap()).unwrap() > Utc::now(), "the slot is used up");
    }

    #[tokio::test]
    async fn a_failed_start_is_recorded_and_a_once_switches_off() {
        let db = db().await;
        switch(&db, true).await;
        let id = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        sqlx::query("UPDATE schedules SET timing = '{\"kind\":\"once\",\"at\":\"2020-01-01T07:00\"}' WHERE id = ?")
            .bind(&id)
            .execute(&db)
            .await
            .unwrap();
        let r = tick(&db, now(), |_| std::future::ready(Err::<String, _>("over budget".to_string()))).await.unwrap();
        assert_eq!(r.failed, [id.clone()]);
        let s = get(&db, &id).await.unwrap();
        assert_eq!((s.last_status.as_deref(), s.last_message.as_deref()), (Some("error"), Some("over budget")));
        assert!(!s.enabled && s.next_run_at.is_none());
    }

    #[tokio::test]
    async fn turning_the_switch_back_on_drops_what_came_due_meanwhile() {
        let db = db().await;
        let id = due_schedule(&db, "2020-01-01T00:00:00Z").await;
        reanchor(&db, now()).await.unwrap();
        switch(&db, true).await;
        let (seen, fire) = recorder();
        assert!(tick(&db, now(), fire).await.unwrap().is_empty());
        assert!(seen.lock().unwrap().is_empty());
        assert!(get(&db, &id).await.unwrap().enabled);
    }
}
