use anyhow::Result;
use sqlx::{sqlite::SqlitePool, Pool, Sqlite};
use std::path::Path;

pub type Db = Pool<Sqlite>;

pub async fn init_db(db_path: &Path) -> Result<Db> {
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());
    let pool = SqlitePool::connect(&db_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    // Before the startup code reads settings: the Inbox API listener must
    // already see a fresh install's switch off.
    crate::features::seed::seed(&pool).await?;
    reconcile_interrupted_runs(&pool).await?;
    // Best effort: a failed ledger repair must not keep the app from starting.
    if let Err(e) = crate::runs::usage_cost::repair_cumulative_costs(&pool).await {
        eprintln!("[db] run_usage cumulative-cost repair failed: {e}");
    }
    if let Err(e) = crate::runs::usage_cost::reprice_estimates(&pool).await {
        eprintln!("[db] run_usage estimate re-pricing failed: {e}");
    }
    Ok(pool)
}

/// Runs marked `running` in the DB but with no live process — left behind when a
/// previous app session exited mid-run. Nothing from that session is driving
/// them any more, so flip any leftover `running` rows to `cancelled` instead of
/// leaving them stuck forever. Their processes did NOT necessarily die with the
/// app: the background-process tracker (`procs::startup_sweep`) finds and stops
/// or lists whatever is still alive.
async fn reconcile_interrupted_runs(pool: &Db) -> Result<()> {
    sqlx::query("UPDATE runs SET status = 'cancelled', finished_at = ? WHERE status = 'running'")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Row;
    use std::path::Path;

    /// A database a user had before 0002: only 0001 applied, through sqlx like the app does.
    async fn db_at_0001() -> super::Db {
        let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/0001_init.sql");
        std::fs::copy(&first, dir.join("0001_init.sql")).unwrap();
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&pool).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        pool
    }

    #[tokio::test]
    async fn project_skill_engines_keep_what_each_project_had_installed() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')",
            // The library target moved on since; each project's snapshot is what it has installed.
            "INSERT INTO skills (id, name, description, source_path, updated_at, target) VALUES
                ('s-claude', 'one', 'd', '/lib/one', 'u', 'codex'),
                ('s-codex', 'two', 'd', '/lib/two', 'u', 'both'),
                ('s-both', 'three', 'd', '/lib/three', 'u', 'claude')",
            "INSERT INTO project_skills (project_id, skill_id, target, synced_hash_claude, synced_hash_codex, applied_at, manual) VALUES
                ('p1', 's-claude', 'claude', 'hc1', NULL, 't1', 1),
                ('p1', 's-codex', 'codex', NULL, 'hx2', 't2', 0),
                ('p1', 's-both', 'both', 'hc3', 'hx3', 't3', 1),
                ('p2', 's-both', 'both', NULL, 'hx4', 't4', 0)",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        // A row left behind without its project (written with foreign keys off).
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO project_skills (project_id, skill_id, target, applied_at) VALUES ('gone', 's-both', 'both', 't')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await.unwrap();

        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        let rows = sqlx::query(
            "SELECT project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude,
                    synced_hash_codex, applied_at, manual
             FROM project_skills ORDER BY project_id, applied_at",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.get::<String, _>("project_id"),
                    r.get::<String, _>("skill_id"),
                    r.get::<i64, _>("enabled_claude"),
                    r.get::<i64, _>("enabled_codex"),
                    r.get::<Option<String>, _>("synced_hash_claude"),
                    r.get::<Option<String>, _>("synced_hash_codex"),
                    r.get::<String, _>("applied_at"),
                    r.get::<i64, _>("manual"),
                )
            })
            .collect();
        let s = |v: &str| Some(v.to_string());
        let row = |p: &str, sk: &str, c, x, hc, hx, at: &str, m| {
            (p.to_string(), sk.to_string(), c, x, hc, hx, at.to_string(), m)
        };
        assert_eq!(
            got,
            vec![
                row("p1", "s-claude", 1, 0, s("hc1"), None, "t1", 1),
                row("p1", "s-codex", 0, 1, None, s("hx2"), "t2", 0),
                row("p1", "s-both", 1, 1, s("hc3"), s("hx3"), "t3", 1),
                // Switched on for Claude though git tracks that path (no hash): still on.
                row("p2", "s-both", 1, 1, None, s("hx4"), "t4", 0),
            ]
        );

        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('project_skills')")
            .fetch_all(&db)
            .await
            .unwrap();
        assert!(!columns.iter().any(|c| c == "target"), "{columns:?}");
        let schema: String =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'project_skills'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(schema.contains(".agents/skills") && !schema.contains(".codex/skills"), "{schema}");

        // Foreign keys still hold on the rebuilt table.
        sqlx::query("DELETE FROM projects WHERE id = 'p2'").execute(&db).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_skills WHERE project_id = 'p2'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn session_search_starts_empty_on_a_database_with_runs() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        // The backfill is the app's background catch-up, not the migration's.
        for table in ["session_search_records", "session_search_runs"] {
            let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&db).await.unwrap();
            assert_eq!(n, 0, "{table}");
        }
        // The old runs are still there and can still be deleted.
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
    }
}
