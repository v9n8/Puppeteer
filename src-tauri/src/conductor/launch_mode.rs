//! The permission mode a conductor last launched with, kept in memory and in
//! `conductor_sessions.permission_mode` (CO-05). The auto-wake resumes a
//! conductor in that mode only (`tools::wake_permission_mode`); with it in
//! memory alone, an app restart left every conductor unwakeable until the user
//! resumed it by hand. The stored value is the one the conductor actually ran
//! with, never a default, so restoring it cannot loosen it.

use super::ConductorState;
use crate::db::Db;

/// Record the mode a conductor's sidecar was just launched with (start or resume).
pub async fn remember(db: &Db, conductor: &ConductorState, conductor_run_id: &str, mode: &str) {
    conductor.set_permission_mode(conductor_run_id, mode);
    if let Err(e) = sqlx::query("UPDATE conductor_sessions SET permission_mode = ? WHERE id = ?")
        .bind(mode)
        .bind(conductor_run_id)
        .execute(db)
        .await
    {
        // The launch goes on; only a wake after a restart would be refused.
        tracing::warn!(event = "conductor_mode_save_failed", run_id = %conductor_run_id, error = %e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_mode_survives_a_restart() {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', '2026-01-01')",
            "INSERT INTO runs (id, project_id, type, engine, status, created_at, role) VALUES
             ('c', 'p1', 'session', 'claude', 'done', '2026-01-01', 'conductor'),
             ('c-old', 'p1', 'session', 'claude', 'done', '2026-01-01', 'conductor')",
            "INSERT INTO conductor_sessions (id, project_id, goal, status, max_workers, created_at) VALUES
             ('c', 'p1', '', 'running', 3, '2026-01-01'),
             ('c-old', 'p1', '', 'running', 3, '2026-01-01')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        let before = ConductorState::new();
        before.reregister_session("c", "p1", 3, Vec::new());
        remember(&db, &before, "c", "plan").await;
        assert_eq!(before.session("c").unwrap().permission_mode.as_deref(), Some("plan"));

        // A fresh process: the stored mode comes back; one never stored stays unknown.
        let after = ConductorState::new();
        super::super::commands::reconnect_running(&db, &after).await;
        assert_eq!(after.session("c").unwrap().permission_mode.as_deref(), Some("plan"));
        assert_eq!(after.session("c-old").unwrap().permission_mode, None);
    }
}
