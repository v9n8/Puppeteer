//! A project's standing tool permissions (UX-31, migration 0016): the "Always
//! allow" / "Always deny" answers of a permission prompt, per project and by
//! exact tool name (`mcp__<server>__<tool>` for an MCP tool). The desktop UI
//! (`stores/toolPermissions.ts`) seeds every session of the project with them,
//! so they apply again without asking.
//!
//! Remote Control: `list_` is allowed; `set_` is allowed and audited with the
//! decision (`remote::handler::ipc_audit`), the same as a paired phone's
//! `allow_always` / `deny_always` answer, which already stores a standing
//! choice; `import_` is local only — it copies this machine's own webview
//! storage, which a remote window does not have.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::db::Db;

/// A project's standing permissions changed: every window (pop-outs, and a
/// remote-desktop window through the bridge) reloads them. The payload's
/// `project_id` is `None` when several projects may have changed.
pub const CHANGED_EVENT: &str = "tool-permissions:changed";

/// The command a remote desktop runs audited, with its decision.
pub const SET_COMMAND: &str = "set_project_tool_permission";

/// Longest tool name accepted (MCP names are `mcp__<server>__<tool>`; this only stops junk).
const TOOL_MAX_CHARS: usize = 256;

pub const ALLOW: &str = "allow";
pub const DENY: &str = "deny";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ToolPermission {
    pub project_id: String,
    pub tool_name: String,
    /// `allow` or `deny`.
    pub decision: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct Changed<'a> {
    project_id: Option<&'a str>,
}

pub(crate) fn emit_changed(app: &AppHandle, project_id: Option<&str>) {
    let _ = app.emit(CHANGED_EVENT, Changed { project_id });
}

/// A name that can be stored at all: not blank, no padding or control
/// characters, not too long.
fn check_name(tool: &str) -> Result<(), String> {
    if tool.is_empty()
        || tool.trim() != tool
        || tool.chars().count() > TOOL_MAX_CHARS
        || tool.chars().any(char::is_control)
    {
        return Err(format!("'{tool}' is not a tool name"));
    }
    Ok(())
}

/// A tool that may get a standing decision: the same rule as the prompt's
/// Always buttons and a phone's Always (`remote::handler::remember_applies`).
/// Bare `MCP` would cover every server's tools; AskUserQuestion and
/// ExitPlanMode must always reach the user.
pub(crate) fn check_rememberable(tool: &str) -> Result<(), String> {
    check_name(tool)?;
    if !crate::remote::handler::remember_applies(tool, false) {
        return Err(format!("'{tool}' cannot be allowed or denied for good"));
    }
    Ok(())
}

fn check_decision(decision: &str) -> Result<(), String> {
    match decision {
        ALLOW | DENY => Ok(()),
        other => Err(format!("Unknown decision '{other}'")),
    }
}

async fn project_exists(db: &Db, project_id: &str) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

/// Every standing permission, or `project_id`'s, oldest decision first.
pub async fn list(db: &Db, project_id: Option<&str>) -> Result<Vec<ToolPermission>, String> {
    sqlx::query_as(
        "SELECT project_id, tool_name, decision, created_at FROM project_tool_permissions \
         WHERE ? IS NULL OR project_id = ? ORDER BY project_id, created_at, rowid",
    )
    .bind(project_id)
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

/// Set `tool`'s standing decision in `project_id` (`None` forgets it). A new
/// decision replaces the other one: a tool is never both allowed and denied.
pub async fn set(db: &Db, project_id: &str, tool: &str, decision: Option<&str>) -> Result<(), String> {
    let Some(decision) = decision else {
        // Forgetting needs no rule beyond the row existing: it also clears rows
        // an older version stored.
        sqlx::query("DELETE FROM project_tool_permissions WHERE project_id = ? AND tool_name = ?")
            .bind(project_id)
            .bind(tool)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    };
    check_decision(decision)?;
    check_rememberable(tool)?;
    if !project_exists(db, project_id).await? {
        return Err("Project not found".into());
    }
    sqlx::query(
        "INSERT INTO project_tool_permissions (project_id, tool_name, decision, created_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT (project_id, tool_name) DO UPDATE SET decision = excluded.decision, created_at = excluded.created_at \
         WHERE decision <> excluded.decision",
    )
    .bind(project_id)
    .bind(tool)
    .bind(decision)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// One decision of the webview's old localStorage copy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ImportEntry {
    pub project_id: String,
    pub tool_name: String,
    pub decision: String,
}

/// Add decisions saved before UX-31 (the webview's localStorage). A tool the
/// project already has a decision for keeps it, so a second run of the import
/// changes nothing. Entries of a project that is gone, or that a standing
/// decision may not hold, are skipped. Returns how many were added.
pub async fn import(db: &Db, entries: &[ImportEntry]) -> Result<u64, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut added = 0;
    for e in entries {
        if check_decision(&e.decision).is_err() || check_rememberable(&e.tool_name).is_err() {
            continue;
        }
        let done = sqlx::query(
            "INSERT OR IGNORE INTO project_tool_permissions (project_id, tool_name, decision, created_at) \
             SELECT id, ?, ?, ? FROM projects WHERE id = ?",
        )
        .bind(&e.tool_name)
        .bind(&e.decision)
        .bind(&now)
        .bind(&e.project_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
        added += done.rows_affected();
    }
    Ok(added)
}

#[tauri::command]
pub async fn list_project_tool_permissions(
    db: State<'_, Db>,
    project_id: Option<String>,
) -> Result<Vec<ToolPermission>, String> {
    list(db.inner(), project_id.as_deref()).await
}

/// Allow or deny `tool_name` for good in a project, or forget it (`decision`
/// null). Returns the project's standing permissions.
#[tauri::command]
pub async fn set_project_tool_permission(
    app: AppHandle,
    db: State<'_, Db>,
    project_id: String,
    tool_name: String,
    decision: Option<String>,
) -> Result<Vec<ToolPermission>, String> {
    set(db.inner(), &project_id, &tool_name, decision.as_deref()).await?;
    emit_changed(&app, Some(&project_id));
    list(db.inner(), Some(&project_id)).await
}

/// The one-time copy of the webview's localStorage decisions ([`import`]).
#[tauri::command]
pub async fn import_project_tool_permissions(
    app: AppHandle,
    db: State<'_, Db>,
    entries: Vec<ImportEntry>,
) -> Result<u64, String> {
    let added = import(db.inner(), &entries).await?;
    if added > 0 {
        emit_changed(&app, None);
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::response_styles::tests::test_db;

    fn decisions(rows: &[ToolPermission]) -> Vec<(&str, &str, &str)> {
        rows.iter().map(|r| (r.project_id.as_str(), r.tool_name.as_str(), r.decision.as_str())).collect()
    }

    #[tokio::test]
    async fn set_flips_and_forgets() {
        let db = test_db().await;
        set(&db, "p1", "Bash", Some(ALLOW)).await.unwrap();
        set(&db, "p1", "mcp__google__drive_search", Some(DENY)).await.unwrap();
        set(&db, "p2", "Bash", Some(DENY)).await.unwrap();
        assert_eq!(
            decisions(&list(&db, Some("p1")).await.unwrap()),
            vec![("p1", "Bash", "allow"), ("p1", "mcp__google__drive_search", "deny")]
        );
        assert_eq!(list(&db, None).await.unwrap().len(), 3);

        // Same decision again: nothing changes (the time it was set included).
        let before = list(&db, Some("p1")).await.unwrap();
        set(&db, "p1", "Bash", Some(ALLOW)).await.unwrap();
        assert_eq!(list(&db, Some("p1")).await.unwrap(), before);

        // A flip replaces the decision: never allowed and denied at once.
        set(&db, "p1", "Bash", Some(DENY)).await.unwrap();
        let p1 = list(&db, Some("p1")).await.unwrap();
        assert_eq!(p1.iter().filter(|r| r.tool_name == "Bash").count(), 1);
        assert_eq!(p1.iter().find(|r| r.tool_name == "Bash").unwrap().decision, "deny");

        // Forget; forgetting what is not there is fine.
        set(&db, "p1", "Bash", None).await.unwrap();
        set(&db, "p1", "Nothing", None).await.unwrap();
        assert_eq!(decisions(&list(&db, Some("p1")).await.unwrap()), vec![("p1", "mcp__google__drive_search", "deny")]);
        // The other project is untouched.
        assert_eq!(decisions(&list(&db, Some("p2")).await.unwrap()), vec![("p2", "Bash", "deny")]);
    }

    #[tokio::test]
    async fn set_refuses_what_a_prompt_cannot_remember() {
        let db = test_db().await;
        let long = "x".repeat(TOOL_MAX_CHARS + 1);
        for tool in ["MCP", "AskUserQuestion", "ExitPlanMode", "", " Bash", "Ba\nsh", long.as_str()] {
            assert!(set(&db, "p1", tool, Some(ALLOW)).await.is_err(), "{tool:?}");
            assert!(set(&db, "p1", tool, Some(DENY)).await.is_err(), "{tool:?}");
        }
        assert!(set(&db, "p1", "Bash", Some("always")).await.is_err());
        assert!(set(&db, "missing", "Bash", Some(ALLOW)).await.is_err());
        // A per-server MCP key (codex names no tool) is a name like any other.
        set(&db, "p1", "mcp__github", Some(ALLOW)).await.unwrap();
        assert_eq!(list(&db, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_never_overwrites_and_skips_bad_entries() {
        let db = test_db().await;
        set(&db, "p1", "Bash", Some(DENY)).await.unwrap();
        let e = |p: &str, t: &str, d: &str| ImportEntry { project_id: p.into(), tool_name: t.into(), decision: d.into() };
        let entries = vec![
            e("p1", "Bash", ALLOW),     // the DB row wins
            e("p1", "Read", ALLOW),
            e("p1", "Write", DENY),
            e("p1", "Edit", ALLOW),
            e("gone", "Bash", ALLOW),   // project no longer here
            e("p1", "MCP", ALLOW),      // never rememberable
            e("p1", "Grep", "maybe"),   // unknown decision
        ];
        assert_eq!(import(&db, &entries).await.unwrap(), 3);
        // Same input again: nothing added (idempotent).
        assert_eq!(import(&db, &entries).await.unwrap(), 0);
        assert_eq!(
            decisions(&list(&db, None).await.unwrap()),
            vec![("p1", "Bash", "deny"), ("p1", "Read", "allow"), ("p1", "Write", "deny"), ("p1", "Edit", "allow")]
        );
    }

    #[tokio::test]
    async fn rows_go_with_their_project() {
        let db = test_db().await;
        set(&db, "p1", "Bash", Some(ALLOW)).await.unwrap();
        set(&db, "p2", "Bash", Some(ALLOW)).await.unwrap();
        // Moving a project to the Trash keeps them; a purge (row deleted) drops them.
        sqlx::query("UPDATE projects SET deleted_at = 'now' WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(list(&db, Some("p1")).await.unwrap().len(), 1);
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert!(list(&db, Some("p1")).await.unwrap().is_empty());
        // Without foreign keys the trigger still clears them.
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&db).await.unwrap();
        sqlx::query("DELETE FROM projects WHERE id = 'p2'").execute(&db).await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await.unwrap();
        assert!(list(&db, None).await.unwrap().is_empty());
    }
}
