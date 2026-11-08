//! Tauri commands of the Schedules screen, behind [`FEATURE`].
//!
//! Remote (`remote/guard.rs`): a remote desktop may list, save and delete
//! schedules (save and delete are audited), but switching one on is
//! local-only and its saves switch the schedule off: a schedule runs while
//! nobody is at this machine, so only the person at it starts one.

use super::{Schedule, ScheduleInput, CHANGED_EVENT, FEATURE};
use crate::db::Db;
use chrono::Local;
use tauri::{AppHandle, Emitter, State};

fn changed(app: &AppHandle) {
    let _ = app.emit(CHANGED_EVENT, ());
}

#[tauri::command]
pub async fn list_schedules(db: State<'_, Db>) -> Result<Vec<Schedule>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    super::list(db.inner()).await
}

#[tauri::command]
pub async fn save_schedule(
    app: AppHandle,
    webview: tauri::Webview,
    db: State<'_, Db>,
    input: ScheduleInput,
) -> Result<Schedule, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let from_remote = crate::remote::guard::from_remote_bridge(&webview);
    let saved = super::save(db.inner(), input, from_remote, &Local::now()).await?;
    changed(&app);
    Ok(saved)
}

#[tauri::command]
pub async fn delete_schedule(app: AppHandle, db: State<'_, Db>, id: String) -> Result<(), String> {
    crate::features::require(db.inner(), FEATURE).await?;
    super::delete(db.inner(), &id).await?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_schedule_enabled(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
    enabled: bool,
) -> Result<Schedule, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let saved = super::set_enabled(db.inner(), &id, enabled, &Local::now()).await?;
    changed(&app);
    Ok(saved)
}
