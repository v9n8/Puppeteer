//! Tauri commands linking notes / todos to sessions (runs) — the sibling of
//! the Slack thread link commands. Errors are `"<code>: <detail>"` strings.
//! Writes broadcast `item:changed` so the list windows resync. A kind whose
//! switch is off is refused, and left out of a session's items.

use crate::db::Db;
use crate::item_links::{self, ItemKind, ItemRunLink, RunLinkedItem};
use crate::slack_threads::run_links::LinkRunResult;
use tauri::{AppHandle, State};

type CmdResult<T> = Result<T, String>;

/// The switch of `kind`, the one its own commands check.
pub fn feature(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Note => crate::commands::notes::FEATURE,
        ItemKind::Todo => crate::commands::todos::FEATURE,
    }
}

/// The notes / todos linked to a run, without the kinds switched off.
async fn visible_items(db: &Db, run_id: &str) -> CmdResult<Vec<RunLinkedItem>> {
    let items = item_links::items_of_run(db, run_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut on = Vec::new();
    for kind in ItemKind::ALL {
        if crate::features::is_on(db, feature(kind)).await {
            on.push(kind);
        }
    }
    Ok(items.into_iter().filter(|i| on.contains(&i.kind)).collect())
}

/// Link a note / todo to a session (idempotent). An item without a project
/// takes the run's.
#[tauri::command]
pub async fn link_item_run(
    app: AppHandle,
    db: State<'_, Db>,
    kind: ItemKind,
    item_id: String,
    run_id: String,
    source: String,
) -> CmdResult<LinkRunResult> {
    crate::features::require(db.inner(), feature(kind)).await?;
    let r = item_links::link(db.inner(), kind, &item_id, &run_id, &source)
        .await
        .map_err(|e| e.to_string())?;
    if r.created || r.project_set {
        item_links::emit_changed(&app, Some(kind));
    }
    Ok(r)
}

#[tauri::command]
pub async fn unlink_item_run(
    app: AppHandle,
    db: State<'_, Db>,
    kind: ItemKind,
    item_id: String,
    run_id: String,
) -> CmdResult<()> {
    crate::features::require(db.inner(), feature(kind)).await?;
    if item_links::unlink(db.inner(), kind, &item_id, &run_id)
        .await
        .map_err(|e| e.to_string())?
    {
        item_links::emit_changed(&app, Some(kind));
    }
    Ok(())
}

/// Sessions linked to a note / todo (runs that still exist).
#[tauri::command]
pub async fn list_item_runs(
    db: State<'_, Db>,
    kind: ItemKind,
    item_id: String,
) -> CmdResult<Vec<ItemRunLink>> {
    crate::features::require(db.inner(), feature(kind)).await?;
    item_links::runs_of_item(db.inner(), kind, &item_id)
        .await
        .map_err(|e| e.to_string())
}

/// Notes / todos linked to a session.
#[tauri::command]
pub async fn list_run_items(db: State<'_, Db>, run_id: String) -> CmdResult<Vec<RunLinkedItem>> {
    visible_items(db.inner(), &run_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::set_setting;

    #[tokio::test]
    async fn a_session_lists_only_the_kinds_switched_on() {
        let db = crate::slack_threads::tests::memory_db().await;
        for sql in [
            "INSERT INTO runs (id, project_id, type, status, engine, created_at) \
             VALUES ('r1', 'p1', 'session', 'done', 'claude', '2026-01-01')",
            "INSERT INTO notes (id, title, content, position, created_at, updated_at) \
             VALUES ('n1', 'T', 'body', 0, '2026-01-01', '2026-01-01')",
            "INSERT INTO todos (id, text, done, position, created_at) VALUES ('t1', 'do it', 0, 0, '2026-01-01')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        item_links::link(&db, ItemKind::Note, "n1", "r1", "manual").await.unwrap();
        item_links::link(&db, ItemKind::Todo, "t1", "r1", "manual").await.unwrap();
        let kinds = |items: Vec<RunLinkedItem>| items.into_iter().map(|i| i.kind).collect::<Vec<_>>();

        // Fresh install: notes on, todos off.
        assert_eq!(kinds(visible_items(&db, "r1").await.unwrap()), [ItemKind::Note]);
        set_setting(&db, "feature.productivity.todos", "true").await.unwrap();
        assert_eq!(kinds(visible_items(&db, "r1").await.unwrap()), [ItemKind::Note, ItemKind::Todo]);
        set_setting(&db, "feature.productivity", "false").await.unwrap();
        assert!(visible_items(&db, "r1").await.unwrap().is_empty());
    }
}
