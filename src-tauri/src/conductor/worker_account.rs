//! Which Claude account a conductor's worker runs on (RT-05).
//!
//! A Claude worker of a Claude conductor runs on the conductor's account: the
//! one the user picked or switched the conductor to, which beats the project /
//! default account (the conductor may have been moved off an account that ran
//! out of quota). `reserve_worker` writes it at spawn; a stopped worker that
//! `session_send` resumes picks up the conductor's account as it is now. A
//! conductor with no account of its own (NULL = "Default") hands nothing down,
//! and the worker then resolves like any run.

use crate::db::Db;

/// Point a stopped Claude worker at its Claude conductor's current account.
/// No-op when either is not Claude or the conductor has no account; the worker
/// row must not be running (its sidecar keeps the account it spawned with).
/// The next resume mirrors the transcript into that account's profile.
pub(super) async fn follow_conductor(db: &Db, conductor_run_id: &str, worker_id: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE runs
            SET claude_account_id = (SELECT claude_account_id FROM runs WHERE id = ? AND engine = 'claude')
          WHERE id = ? AND engine = 'claude' AND status != 'running'
            AND (SELECT claude_account_id FROM runs WHERE id = ? AND engine = 'claude') IS NOT NULL",
    )
    .bind(conductor_run_id)
    .bind(worker_id)
    .bind(conductor_run_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::ConductorState;
    use sqlx::sqlite::SqlitePoolOptions;

    /// A conductor `c` on `engine` with Claude account `account`, plus two
    /// managed accounts `a1`, `a2`.
    async fn setup(engine: &str, account: Option<&str>) -> (Db, ConductorState) {
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', 'c')",
            "INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES
                ('a1', 'One', '/tmp/a1', 'c'), ('a2', 'Two', '/tmp/a2', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, created_at, role, claude_account_id)
             VALUES ('c', 'p1', 'session', 'running', ?, 'c', 'conductor', ?)",
        )
        .bind(engine)
        .bind(account)
        .execute(&db)
        .await
        .unwrap();
        let cst = ConductorState::new();
        cst.register_session("c", "p1", 5);
        cst.set_permission_mode("c", "default");
        (db, cst)
    }

    async fn spawn(db: &Db, cst: &ConductorState, engine: &str) -> String {
        super::super::tools::reserve_worker(db, cst, "c", engine, "[w] task", None, None).await.unwrap().0
    }

    async fn account(db: &Db, id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT claude_account_id FROM runs WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    #[tokio::test]
    async fn a_claude_worker_spawns_on_its_claude_conductors_account() {
        let (db, cst) = setup("claude", Some("a1")).await;
        let claude = spawn(&db, &cst, "claude").await;
        let codex = spawn(&db, &cst, "codex").await;
        assert_eq!(account(&db, &claude).await.as_deref(), Some("a1"));
        assert_eq!(account(&db, &codex).await, None, "a Codex worker has no Claude account");
    }

    #[tokio::test]
    async fn a_codex_conductor_or_one_without_an_account_hands_nothing_down() {
        let (db, cst) = setup("codex", Some("a1")).await;
        let w = spawn(&db, &cst, "claude").await;
        assert_eq!(account(&db, &w).await, None);

        let (db, cst) = setup("claude", None).await;
        let w = spawn(&db, &cst, "claude").await;
        assert_eq!(account(&db, &w).await, None, "follows project / default at its start");
    }

    #[tokio::test]
    async fn a_resumed_worker_follows_the_conductors_current_account() {
        let (db, cst) = setup("claude", Some("a1")).await;
        let w = spawn(&db, &cst, "claude").await;
        sqlx::query("UPDATE runs SET status = 'done' WHERE id = ?").bind(&w).execute(&db).await.unwrap();
        // The user switched the conductor after the worker stopped.
        sqlx::query("UPDATE runs SET claude_account_id = 'a2' WHERE id = 'c'").execute(&db).await.unwrap();
        follow_conductor(&db, "c", &w).await.unwrap();
        assert_eq!(account(&db, &w).await.as_deref(), Some("a2"));

        // Back to "Default" on the conductor: the worker keeps what it has.
        sqlx::query("UPDATE runs SET claude_account_id = NULL WHERE id = 'c'").execute(&db).await.unwrap();
        follow_conductor(&db, "c", &w).await.unwrap();
        assert_eq!(account(&db, &w).await.as_deref(), Some("a2"));

        // A running worker keeps the account its sidecar runs on.
        sqlx::query("UPDATE runs SET claude_account_id = 'a1' WHERE id = 'c'").execute(&db).await.unwrap();
        sqlx::query("UPDATE runs SET status = 'running' WHERE id = ?").bind(&w).execute(&db).await.unwrap();
        follow_conductor(&db, "c", &w).await.unwrap();
        assert_eq!(account(&db, &w).await.as_deref(), Some("a2"));
    }
}
