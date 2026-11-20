//! Reading Slack direct-message threads (#9, RT-27). Off unless the user turns it on:
//! the `slack_dm_read` setting switches it for every project, and a project can only
//! narrow that (`projects.slack_dm_read`, migration 0029: 1 = follow the setting,
//! 0 = never). A thread without a project follows the setting alone.
//!
//! Gates: adding a thread whose link is a DM (`D…` channel), and every fetch of a DM
//! thread (by its link, or when `conversations.info` says `is_im` / `is_mpim`), so
//! turning it off stops a thread added while it was on. The token needs
//! `im:history` / `mpim:history`; a token known to lack it fails with `missing_scope`.
//!
//! Remote: the setting and both commands are local-only (a controller must not open
//! this machine's DMs to itself); reading the setting is allowed.

use tauri::State;

use super::api::AuthInfo;
use super::CodedError;
use crate::commands::projects::Project;
use crate::db::Db;

/// The setting for every project ("true" = on; anything else, or none, = off).
pub const SETTING: &str = "slack_dm_read";

/// Message of a DM refused because reading DMs is off.
pub const DM_OFF_MESSAGE: &str =
    "reading direct-message threads is off (turn it on for all projects, and for the thread's project)";

/// Whether the setting for every project is on.
pub async fn global_on(db: &Db) -> bool {
    crate::remote::get_setting(db, SETTING).await.is_some_and(|v| v.trim() == "true")
}

/// The project's own switch (true when it follows the setting, or is gone).
pub async fn project_on(db: &Db, project_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT slack_dm_read FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(event = "slack_dm_read_project_failed", project_id, error = %e);
            Some(false)
        })
        .unwrap_or(true)
}

/// Whether a DM thread of `project_id` (or of no project) may be read.
pub async fn allowed(db: &Db, project_id: Option<&str>) -> bool {
    global_on(db).await
        && match project_id.map(str::trim).filter(|p| !p.is_empty()) {
            Some(pid) => project_on(db, pid).await,
            None => true,
        }
}

/// `Ok` when a DM thread of `project_id` may be read, else `dm_unsupported`.
pub async fn check(db: &Db, project_id: Option<&str>) -> Result<(), CodedError> {
    if allowed(db, project_id).await {
        Ok(())
    } else {
        Err(CodedError::new("dm_unsupported", DM_OFF_MESSAGE))
    }
}

/// `missing_scope` when the token is known to lack the history scope of a DM
/// (`mpim` = a group DM). Unknown scopes pass: Slack answers for itself.
pub fn scope_error(auth: &AuthInfo, mpim: bool) -> Option<CodedError> {
    let scope = if mpim { "mpim:history" } else { "im:history" };
    (auth.has_scope(scope) == Some(false)).then(|| CodedError::new("missing_scope", scope))
}

/// The setting for every project. Remote: allowed.
#[tauri::command]
pub async fn get_slack_dm_read(db: State<'_, Db>) -> Result<bool, String> {
    Ok(global_on(db.inner()).await)
}

/// Turn reading DM threads on or off for every project. Remote: local-only.
#[tauri::command]
pub async fn set_slack_dm_read(db: State<'_, Db>, on: bool) -> Result<(), String> {
    crate::commands::settings::set_setting(db.inner(), SETTING, if on { "true" } else { "false" }).await
}

/// Store a project's switch (false = never read DMs for it).
pub async fn set_project(db: &Db, project_id: &str, on: bool) -> Result<(), String> {
    let done = sqlx::query("UPDATE projects SET slack_dm_read = ? WHERE id = ?")
        .bind(on)
        .bind(project_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    if done.rows_affected() == 0 {
        return Err("Project not found".into());
    }
    Ok(())
}

/// A project's switch: on follows the setting, off never reads DMs. Remote: local-only.
#[tauri::command]
pub async fn set_project_slack_dm_read(db: State<'_, Db>, project_id: String, on: bool) -> Result<Project, String> {
    set_project(db.inner(), &project_id, on).await?;
    crate::commands::projects::project_by_id(db.inner(), &project_id).await
}

/// Settings backup import: the backup's switch for a project; `None` (older backup) keeps
/// the local one. True when something changed.
pub(crate) async fn import_project(db: &Db, project_id: &str, backup: Option<bool>) -> Result<bool, String> {
    let Some(on) = backup else { return Ok(false) };
    if project_on(db, project_id).await == on {
        return Ok(false);
    }
    set_project(db, project_id, on).await?;
    Ok(true)
}
