//! Tauri commands of the feature registry. `get_features` is read-only, so a
//! remote desktop may call it (it hides the same entries as the local app).

use super::{owner, resolve, ModuleState};
use crate::db::Db;
use std::collections::HashMap;
use tauri::State;

/// The resolved module → prop tree.
#[tauri::command]
pub async fn get_features(db: State<'_, Db>) -> Result<Vec<ModuleState>, String> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    let rows: HashMap<String, String> = rows.into_iter().filter(|(k, _)| owner(k).is_some()).collect();
    Ok(resolve(&rows))
}
