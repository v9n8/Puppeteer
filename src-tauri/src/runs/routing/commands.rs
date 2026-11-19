//! Tauri commands of Settings › Routing. "Update now" and "Go back to this
//! version" are audited when a Remote Control controller runs them
//! (`remote::guard`); the reads are not.

use tauri::State;

use super::report::{self, Report};
use super::update::{self, Notice, Policies, Recomputed};
use crate::db::Db;

/// The routing report of the last 7 or 30 days.
#[tauri::command]
pub async fn get_routing_report(db: State<'_, Db>, days: u32) -> Result<Report, String> {
    report::load(db.inner(), days).await
}

/// "Update now": recompute the policy, skipping the once-a-day gates.
#[tauri::command]
pub async fn recompute_routing_policy(app: tauri::AppHandle) -> Result<Recomputed, String> {
    update::recompute_and_notify(&app, true).await
}

/// "Go back to this version": the new version's number.
#[tauri::command]
pub async fn revert_routing_policy(db: State<'_, Db>, version: i64) -> Result<i64, String> {
    update::revert(db.inner(), version).await
}

/// Every policy version, newest first, and the pending proposal.
#[tauri::command]
pub async fn list_routing_policies(db: State<'_, Db>) -> Result<Policies, String> {
    update::list(db.inner()).await
}

/// The automatic version not yet announced, once.
#[tauri::command]
pub async fn take_routing_notice(db: State<'_, Db>) -> Result<Option<Notice>, String> {
    update::take_notice(db.inner()).await
}
