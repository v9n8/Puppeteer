//! Tauri commands of Settings › Routing.

use tauri::State;

use super::report::{self, Report};
use crate::db::Db;

/// The routing report of the last 7 or 30 days.
#[tauri::command]
pub async fn get_routing_report(db: State<'_, Db>, days: u32) -> Result<Report, String> {
    report::load(db.inner(), days).await
}
