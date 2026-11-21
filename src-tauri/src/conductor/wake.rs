//! Waking a conductor when one of its workers finishes (auto-resume). Moved
//! out of `tools.rs` as is (CO-07): that file had reached the size the agents'
//! file reader stops at.

use sqlx::Row;
use tauri::Manager;

use super::ConductorState;
use crate::db::Db;
use crate::runs::RunRegistry;

/// Called (via the wake channel) when ANY run's sidecar exits, and when a
/// conductor's turn ends while its sidecar stays alive. Closes the conductor
/// control loop so the human never has to prod it:
///   • if the run is a conductor, its turn is over: release its auto-wake slot
///     and deliver any wake a worker deferred while it was busy;
///   • if the run is a worker, wake its conductor.
/// A no-op for ordinary (non-conductor, non-worker) runs.
pub async fn on_run_finished(app: &tauri::AppHandle, run_id: &str) {
    let Some(cst) = app.try_state::<crate::conductor::ConductorState>() else {
        return;
    };
    let cst = cst.inner().clone();

    // A finishing conductor frees its slot BEFORE we consider any worker wake, so
    // a worker that finished during the conductor's turn can wake it next.
    if cst.session(run_id).is_some() {
        if cst.turn_ended(run_id) {
            wake_conductor(app, &cst, run_id).await;
        }
        return;
    }

    // Closes the worker's routing row; a FAIL retried one tier up wakes no one yet.
    if crate::runs::routing::escalate::on_worker_finished(app, run_id).await {
        return;
    }
    wake_conductor_for_worker(app, &cst, run_id).await;
}

/// If `worker_run_id` is a worker of a live (tracked) conductor, wake that
/// conductor. No-op when the run isn't a worker or its conductor is a
/// finished/forgotten session.
async fn wake_conductor_for_worker(
    app: &tauri::AppHandle,
    cst: &crate::conductor::ConductorState,
    worker_run_id: &str,
) {
    let db = db_from(app);

    // DB is the source of truth for the role + conductor link.
    let row = match sqlx::query("SELECT role, conductor_run_id FROM runs WHERE id = ?")
        .bind(worker_run_id)
        .fetch_optional(&db)
        .await
    {
        Ok(Some(r)) => r,
        _ => return,
    };
    let role: Option<String> = row.get("role");
    if role.as_deref() != Some("worker") {
        return;
    }
    let Some(conductor_run_id) = row.get::<Option<String>, _>("conductor_run_id") else {
        return;
    };

    // Only wake a conductor we still track in memory — it has a live token so the
    // resumed run keeps its `session_*` tools. A finished/forgotten conductor is
    // left alone.
    if cst.session(&conductor_run_id).is_none() {
        return;
    }

    wake_conductor(app, cst, &conductor_run_id).await;
}

/// Nudge a conductor to read its finished workers. Three cases:
///   • mid-turn → defer; the nudge is delivered when that turn ends (injecting
///     now would collide with the in-flight turn);
///   • turn over but sidecar still alive (DB `running`, e.g. kept up by a
///     backgrounded Bash) → send the nudge straight to the live sidecar;
///   • sidecar exited → resume the conductor, then send the nudge.
/// A burst of workers finishing produces one nudge naming (and carrying the
/// reports of) every worker that finished since the last one.
async fn wake_conductor(
    app: &tauri::AppHandle,
    cst: &crate::conductor::ConductorState,
    conductor_run_id: &str,
) {
    // Server down (the switch went off and nothing was left running): a resume
    // would be a plain run that cannot follow the nudge.
    if cst.port() == 0 {
        return;
    }
    let db = db_from(app);

    if cst.defer_wake_if_busy(conductor_run_id) {
        return;
    }
    let alive = match super::tools::worker_status(&db, conductor_run_id).await {
        Ok(s) => s == "running",
        Err(_) => return,
    };
    if !cst.begin_wake_turn(conductor_run_id) {
        return;
    }

    let results = crate::conductor::worker_results::collect(&db, conductor_run_id).await;
    let nudge = crate::conductor::worker_results::nudge(&db, conductor_run_id, &results).await;

    let outcome = async {
        if !alive {
            let permission_mode = wake_permission_mode(cst, conductor_run_id)?;
            crate::commands::runs::resume_run_with(
                app.clone(),
                app.state::<Db>(),
                app.state::<RunRegistry>(),
                conductor_run_id.to_string(),
                Some(permission_mode),
                None,
                None,
                None,
                Some(false),
                false,
            )
            .await?;
        }
        let registry = registry_from(app);
        let send = crate::commands::runs::send_user_message_inner(
            &db,
            &registry,
            crate::commands::runs::SendUserMessagePayload {
                run_id: conductor_run_id.to_string(),
                content: nudge.clone(),
                images: Vec::new(),
                override_budget: false,
                quotes: Vec::new(),
                quote_set: None,
            },
            false,
        );
        crate::conductor::worker_results::deliver(&db, &registry, conductor_run_id, &results, send).await
    }
    .await;

    if let Err(e) = outcome {
        // Couldn't wake now (budget gate, lost race, sidecar shutting down…).
        // Release the slot and keep the wake pending: if the live sidecar was on
        // its way out, its exit (`run:done`) delivers it via a resume instead.
        eprintln!("conductor auto-wake failed for {}: {}", conductor_run_id, e);
        cst.turn_ended(conductor_run_id);
        if alive {
            cst.defer_wake(conductor_run_id);
        }
    }
}

/// The mode a woken conductor resumes with: the one it last ran with. Without
/// it the resume would fall back to `default_permission_mode`, which may be
/// looser (a `plan` session woken in bypass, then free to spawn bypass workers).
/// Unknown (never stored, `launch_mode`) means no wake: guessing could loosen it.
pub(super) fn wake_permission_mode(cst: &ConductorState, conductor_run_id: &str) -> Result<String, String> {
    cst.session(conductor_run_id)
        .and_then(|s| s.permission_mode)
        .ok_or_else(|| "its permission mode is unknown; resume it by hand".to_string())
}

fn db_from(app: &tauri::AppHandle) -> Db {
    app.state::<Db>().inner().clone()
}

fn registry_from(app: &tauri::AppHandle) -> RunRegistry {
    app.state::<RunRegistry>().inner().clone()
}
