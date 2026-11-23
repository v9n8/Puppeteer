//! The Stats page's cleanup: a preview (read-only, open to Remote Control) and
//! the cleanup itself (local-only, `remote::guard`). The background runner
//! (`runner`) uses the same [`run`].

use super::{apply, days, plan, CategorySummary, CleanupOutcome, Ctx, CAT_TRASH, DEFAULT_DAYS, LAST_SETTING, LOCK};
use crate::db::Db;
use crate::runs::RunRegistry;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Sent after every cleanup, manual or automatic: the Stats page rescans.
pub const CLEANED_EVENT: &str = "storage:cleaned";
/// What a second cleanup gets while one is running.
pub const BUSY: &str = "busy";

/// The stored record of the last cleanup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastCleanup {
    pub at: String,
    /// "auto" | "manual".
    pub trigger: String,
    pub files: u64,
    pub bytes: u64,
    pub errors: u64,
}

#[derive(Debug, Serialize)]
pub struct CleanupPreview {
    /// N as stored (0 = auto-clean off).
    pub days: u32,
    /// The age a cleanup started now uses: N, or the default when N is 0.
    pub effective_days: u32,
    /// A cleanup is running right now.
    pub busy: bool,
    pub categories: Vec<CategorySummary>,
    pub last: Option<LastCleanup>,
}

/// A manual cleanup with auto-clean off still needs an age.
pub fn effective_days(days: u32) -> u32 {
    if days == 0 {
        DEFAULT_DAYS
    } else {
        days
    }
}

pub async fn read_last(db: &Db) -> Option<LastCleanup> {
    let value: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(LAST_SETTING)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()?;
    serde_json::from_str(&value).ok()
}

fn context(app: &AppHandle, db: &Db, days: u32) -> Result<Ctx, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let registry = app.try_state::<RunRegistry>().ok_or("the run registry is not ready")?.inner().clone();
    let conductor = app.try_state::<crate::conductor::ConductorState>().map(|c| c.inner().clone());
    Ok(Ctx::new(db.clone(), app_data, days, registry, conductor))
}

#[tauri::command]
pub async fn preview_storage_cleanup(app: AppHandle, db: State<'_, Db>) -> Result<CleanupPreview, String> {
    let stored = days(db.inner()).await;
    let effective = effective_days(stored);
    let ctx = context(&app, db.inner(), effective)?;
    let planned = plan(&ctx).await;
    Ok(CleanupPreview {
        days: stored,
        effective_days: effective,
        busy: LOCK.try_lock().is_err(),
        categories: planned.categories.iter().map(|c| c.summary()).collect(),
        last: read_last(db.inner()).await,
    })
}

#[tauri::command]
pub async fn run_storage_cleanup(app: AppHandle, db: State<'_, Db>) -> Result<CleanupOutcome, String> {
    let days = effective_days(days(db.inner()).await);
    run(&app, db.inner(), days, "manual").await
}

/// One cleanup with age limit `days`; `Err(BUSY)` while another one runs.
/// The result is stored, logged and announced.
pub(crate) async fn run(app: &AppHandle, db: &Db, days: u32, trigger: &str) -> Result<CleanupOutcome, String> {
    let Ok(_held) = LOCK.try_lock() else {
        return Err(BUSY.into());
    };
    let ctx = context(app, db, days)?;
    let outcome = apply(&ctx, plan(&ctx).await).await;
    let last = LastCleanup {
        at: ctx.now.to_rfc3339(),
        trigger: trigger.into(),
        files: outcome.files,
        bytes: outcome.bytes,
        errors: outcome.errors,
    };
    let json = serde_json::to_string(&last).map_err(|e| e.to_string())?;
    if let Err(e) = crate::commands::settings::set_setting(db, LAST_SETTING, &json).await {
        tracing::warn!(event = "storage_cleanup_record_failed", error = %e);
    }
    tracing::info!(event = "storage_cleanup_done", trigger, days, files = outcome.files, bytes = outcome.bytes, errors = outcome.errors);
    // Runs purged from the Trash: the same notices a purge from the Trash gives.
    if outcome.per_category.iter().any(|c| c.id == CAT_TRASH && c.items > 0) {
        crate::slack_threads::emit_all_changed(app);
        crate::item_links::emit_changed(app, None);
        crate::commands::runs::notify_trash_changed(app);
    }
    let _ = app.emit(CLEANED_EVENT, &last);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::guard::{is_local_only_command, refuses_invoke};
    use crate::remote::ipc::BRIDGE_LABEL;

    #[test]
    fn a_cleanup_with_auto_clean_off_uses_the_default_age() {
        assert_eq!(effective_days(0), 30);
        assert_eq!(effective_days(90), 90);
    }

    #[tokio::test]
    async fn a_second_cleanup_is_busy_while_one_holds_the_lock() {
        let held = LOCK.try_lock().unwrap();
        assert!(LOCK.try_lock().is_err());
        drop(held);
        assert!(LOCK.try_lock().is_ok());
    }

    #[test]
    fn only_the_preview_is_open_to_a_remote_controller() {
        assert!(is_local_only_command("run_storage_cleanup"));
        assert!(refuses_invoke(BRIDGE_LABEL, "run_storage_cleanup"));
        assert!(!is_local_only_command("preview_storage_cleanup"));
        assert!(!refuses_invoke(BRIDGE_LABEL, "preview_storage_cleanup"));
    }

    #[test]
    fn the_last_cleanup_round_trips() {
        let last = LastCleanup { at: "2026-11-23T00:00:00+00:00".into(), trigger: "auto".into(), files: 3, bytes: 10, errors: 1 };
        let json = serde_json::to_string(&last).unwrap();
        assert_eq!(serde_json::from_str::<LastCleanup>(&json).unwrap(), last);
    }
}
