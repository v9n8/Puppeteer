//! The flag board (V3-22): every session of one project, or of all of them, with
//! the flags that make its columns. Dragging a card calls `set_run_flag`
//! (commands/session_flags.rs), so this file only reads.
//!
//! Remote Control: `list_board_runs` is allowed and not audited (a read, QĐ-25).

use std::collections::HashMap;

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::commands::github::{list_runs_inner, RunRecord};
use crate::commands::session_flags::{self, SessionFlag};
use crate::db::Db;

/// The prop that gates the board (`sessions.board`).
pub const BOARD_FEATURE: &str = "sessions.board";

/// A session as `list_runs` gives it (the card's menu needs the whole record),
/// with the name of its project and the number of its workers.
#[derive(Debug, Clone, Serialize)]
pub struct BoardRun {
    #[serde(flatten)]
    pub run: RunRecord,
    pub project_name: String,
    pub worker_count: i64,
}

/// A shared flag one project hides (its sessions cannot be dropped on that column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HiddenMark {
    pub project_id: String,
    pub flag_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardData {
    pub runs: Vec<BoardRun>,
    /// One project: the flags it lists (`list_session_flags(Some(p))`). All
    /// projects: the shared flags, then the own flags of projects that have a session here.
    pub flags: Vec<SessionFlag>,
    /// Hidden marks of the projects in `runs` (the shared list has no `hidden` of its own).
    pub hidden: Vec<HiddenMark>,
}

/// Sessions outside the Trash, workers left out (they are counted on their conductor).
pub async fn board_runs(db: &Db, project_id: Option<&str>) -> Result<Vec<BoardRun>, String> {
    let projects: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM projects WHERE deleted_at IS NULL AND (? IS NULL OR id = ?)")
            .bind(project_id)
            .bind(project_id)
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (id, name) in projects {
        let all = list_runs_inner(db, &id).await?;
        let mut workers: HashMap<String, i64> = HashMap::new();
        for conductor in all.iter().filter_map(|r| r.conductor_run_id.as_deref()) {
            *workers.entry(conductor.to_string()).or_default() += 1;
        }
        let top = all.into_iter().filter(|r| r.conductor_run_id.is_none() && r.role.as_deref() != Some("worker"));
        out.extend(top.map(|run| BoardRun {
            worker_count: workers.get(&run.id).copied().unwrap_or(0),
            project_name: name.clone(),
            run,
        }));
    }
    // Pinned first, then the latest activity: the History order, over every project.
    let activity = |r: &BoardRun| r.run.last_activity_at.clone().unwrap_or_else(|| r.run.created_at.clone());
    out.sort_by(|a, b| b.run.pinned.cmp(&a.run.pinned).then_with(|| activity(b).cmp(&activity(a))));
    Ok(out)
}

pub async fn board(db: &Db, project_id: Option<&str>) -> Result<BoardData, String> {
    let runs = board_runs(db, project_id).await?;
    let mut flags = session_flags::list(db, project_id).await?;
    if project_id.is_none() {
        let mut seen: Vec<&str> = Vec::new();
        for run in &runs {
            if seen.contains(&run.run.project_id.as_str()) {
                continue;
            }
            seen.push(&run.run.project_id);
            let own = session_flags::list(db, Some(&run.run.project_id)).await?;
            flags.extend(own.into_iter().filter(|f| f.project_id.as_deref() == Some(run.run.project_id.as_str())));
        }
    }
    let marks = sqlx::query("SELECT project_id, flag_id FROM project_hidden_flags")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    let hidden = marks
        .iter()
        .map(|m| HiddenMark { project_id: m.get("project_id"), flag_id: m.get("flag_id") })
        .filter(|m| project_id.is_none_or(|p| p == m.project_id))
        .collect();
    Ok(BoardData { runs, flags, hidden })
}

/// `project_id` None: every project that is not in the Trash.
#[tauri::command]
pub async fn list_board_runs(db: State<'_, Db>, project_id: Option<String>) -> Result<BoardData, String> {
    crate::features::require(db.inner(), BOARD_FEATURE).await?;
    board(db.inner(), project_id.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session_flags::tests::{add_run, test_db};

    #[tokio::test]
    async fn one_project_lists_its_sessions_and_flags_without_workers_or_trash() {
        let db = test_db().await;
        add_run(&db, "a", "p1", Some("backlog")).await;
        add_run(&db, "b", "p1", None).await;
        add_run(&db, "other", "p2", None).await;
        add_run(&db, "trashed", "p1", None).await;
        sqlx::query("UPDATE runs SET deleted_at = 'x' WHERE id = 'trashed'").execute(&db).await.unwrap();
        add_run(&db, "w1", "p1", None).await;
        sqlx::query("UPDATE runs SET conductor_run_id = 'a', role = 'worker' WHERE id = 'w1'").execute(&db).await.unwrap();

        let data = board(&db, Some("p1")).await.unwrap();
        let mut ids: Vec<&str> = data.runs.iter().map(|r| r.run.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["a", "b"]);
        let a = data.runs.iter().find(|r| r.run.id == "a").unwrap();
        assert_eq!((a.worker_count, a.project_name.as_str()), (1, "a"));
        assert_eq!(data.flags.len(), session_flags::DEFAULT_FLAGS.len());
    }

    #[tokio::test]
    async fn all_projects_adds_own_flags_of_projects_with_sessions_and_the_hidden_marks() {
        let db = test_db().await;
        add_run(&db, "a", "p1", None).await;
        let own = session_flags::create(&db, Some("p1"), "Waiting client", "info").await.unwrap();
        let unused = session_flags::create(&db, Some("p2"), "Nobody here", "info").await.unwrap();
        session_flags::set_hidden(&db, "p1", "done", true).await.unwrap();

        let data = board(&db, None).await.unwrap();
        assert!(data.flags.iter().any(|f| f.id == own.id));
        assert!(!data.flags.iter().any(|f| f.id == unused.id));
        assert_eq!(data.hidden, [HiddenMark { project_id: "p1".into(), flag_id: "done".into() }]);

        let one = board(&db, Some("p2")).await.unwrap();
        assert!(one.hidden.is_empty());
    }

    #[tokio::test]
    async fn a_project_in_the_trash_leaves_the_all_projects_board() {
        let db = test_db().await;
        add_run(&db, "a", "p1", None).await;
        add_run(&db, "b", "p2", None).await;
        sqlx::query("UPDATE projects SET deleted_at = 'x' WHERE id = 'p2'").execute(&db).await.unwrap();
        let data = board(&db, None).await.unwrap();
        assert_eq!(data.runs.iter().map(|r| r.run.id.as_str()).collect::<Vec<_>>(), ["a"]);
    }

    #[tokio::test]
    async fn the_command_is_off_with_the_prop() {
        let db = test_db().await;
        crate::commands::settings::set_setting(&db, "feature.sessions.board", "false").await.unwrap();
        assert!(crate::features::require(&db, BOARD_FEATURE).await.is_err());
    }
}
