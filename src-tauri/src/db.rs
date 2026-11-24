use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use std::time::Duration;

pub type Db = Pool<Sqlite>;

/// How long a statement waits for another connection's lock before failing with
/// "database is locked". Under WAL only writers wait on each other (the app pool and every
/// `sidecar-mcp` process, which sets the same value in `lib/store.mjs`); sqlx's default is 5 s.
pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

/// The same as sqlx's defaults (what `SqlitePool::connect` used), written out so they are not
/// changed by accident.
const MAX_CONNECTIONS: u32 = 10;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);

/// Opens `data.db`, creating it on a fresh install.
///
/// WAL: readers no longer wait on a writer, and a writer's commit no longer shuts every reader
/// out (the rollback journal did, see RT-29). The mode is stored in the file, so the first start
/// switches an existing database over, and `sidecar-mcp` (node:sqlite) follows it. SQLite then
/// keeps `data.db-wal` and `data.db-shm` beside `data.db`: committed data not yet checkpointed
/// lives in `-wal`, so the three files only ever move together.
///
/// `synchronous = NORMAL` is the usual pairing with WAL: still no corruption on a crash or power
/// loss, only the last few commits may roll back on power loss; FULL would fsync every commit.
fn connect_options(db_path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(BUSY_TIMEOUT)
}

pub async fn init_db(db_path: &Path) -> Result<Db> {
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(connect_options(db_path))
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    // Before the startup code reads settings: the Inbox API listener must
    // already see a fresh install's switch off.
    crate::features::seed::seed(&pool).await?;
    // Before the reconcile turns those `running` rows into `cancelled`.
    crate::runs::turn_cut::mark_app_closed(&pool).await;
    reconcile_interrupted_runs(&pool).await?;
    // Best effort: a failed ledger repair must not keep the app from starting.
    if let Err(e) = crate::runs::usage_cost::repair_cumulative_costs(&pool).await {
        eprintln!("[db] run_usage cumulative-cost repair failed: {e}");
    }
    if let Err(e) = crate::runs::usage_cost::reprice_estimates(&pool).await {
        eprintln!("[db] run_usage estimate re-pricing failed: {e}");
    }
    // Best effort too: a session started without it seeds it on its first bind.
    if let Err(e) = crate::runs::routing::policy::ensure_seeded(&pool).await {
        eprintln!("[db] routing policy seed failed: {e}");
    }
    Ok(pool)
}

/// Runs marked `running` in the DB but with no live process — left behind when a
/// previous app session exited mid-run (a crash and a normal quit alike: the
/// quit path never writes run status). Nothing from that session is driving
/// them any more, so flip any leftover `running` rows to `cancelled` instead of
/// leaving them stuck forever, marked `end_reason = 'app_closed'` and
/// `interrupted_pending = 1` so they stay apart from runs the user stopped and
/// wait for the user to resume or dismiss them; nothing is restarted here.
/// Their processes did NOT necessarily die with the app: the
/// background-process tracker (`procs::startup_sweep`) finds and stops or lists
/// whatever is still alive.
pub(crate) async fn reconcile_interrupted_runs(pool: &Db) -> Result<()> {
    sqlx::query(
        "UPDATE runs SET status = 'cancelled', finished_at = ?, end_reason = 'app_closed', interrupted_pending = 1 \
         WHERE status = 'running'",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "db_wal_tests.rs"]
mod wal_tests;

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

    #[tokio::test]
    async fn existing_runs_have_no_worktree() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at, role) VALUES ('w1', 'p1', 'session', 'c', 'worker')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let path: Option<String> = sqlx::query_scalar("SELECT worktree_path FROM runs WHERE id = 'w1'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(path, None, "an old worker keeps running in the project folder");
    }

    #[tokio::test]
    async fn existing_projects_and_runs_are_not_in_the_trash() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let (project, run): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT p.deleted_at, r.deleted_at FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = 'r1'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!((project, run), (None, None));
        // `path` stays unique, trashed or not: re-adding a path restores the old row (UX-06).
        sqlx::query("UPDATE projects SET deleted_at = 'd' WHERE id = 'p1'").execute(&db).await.unwrap();
        assert!(sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p2', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn run_snapshots_start_empty_and_go_with_their_run() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c'), ('r2', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let count = |sql: &'static str| {
            let db = db.clone();
            async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&db).await.unwrap() }
        };
        assert_eq!(count("SELECT COUNT(*) FROM run_snapshots").await, 0, "old turns have no snapshot");

        sqlx::query(
            "INSERT INTO run_snapshots (run_id, turn, created_at, workspace, status) VALUES
                ('r1', 1, 't', '/a', 'skipped'), ('r2', 1, 't', '/a', 'skipped')",
        )
        .execute(&db)
        .await
        .unwrap();
        // Removing a run, or its project, drops its snapshot rows.
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_snapshots WHERE run_id = 'r1'").await, 0);
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_snapshots").await, 0);
    }

    #[tokio::test]
    async fn schedules_start_empty_and_go_with_their_project() {
        let db = db_at_0001().await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let count = || async { sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schedules").fetch_one(&db).await.unwrap() };
        assert_eq!(count().await, 0);

        sqlx::query(
            "INSERT INTO schedules (id, project_id, name, prompt, timing, created_at, updated_at)
             VALUES ('s1', 'p1', 'n', 'p', '{\"kind\":\"daily\",\"time\":\"08:30\"}', 'c', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        let row = sqlx::query("SELECT enabled, proposed, permission_mode FROM schedules WHERE id = 's1'")
            .fetch_one(&db)
            .await
            .unwrap();
        let got = (row.get::<i64, _>("enabled"), row.get::<i64, _>("proposed"), row.get::<String, _>("permission_mode"));
        assert_eq!(got, (0, 0, "plan".to_string()), "a new row is off and read-only");
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(count().await, 0);
    }

    #[tokio::test]
    async fn run_bookmarks_start_empty_and_go_with_their_run() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c'), ('r2', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let count = |sql: &'static str| {
            let db = db.clone();
            async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&db).await.unwrap() }
        };
        assert_eq!(count("SELECT COUNT(*) FROM run_bookmarks").await, 0);

        sqlx::query(
            "INSERT INTO run_bookmarks (id, run_id, log_offset, side, created_at) VALUES
                ('b1', 'r1', 0, 'question', 't'), ('b2', 'r2', 0, 'answer', 't')",
        )
        .execute(&db)
        .await
        .unwrap();
        // One bookmark per turn and side.
        assert!(sqlx::query("INSERT INTO run_bookmarks (id, run_id, log_offset, side, created_at) VALUES ('b3', 'r1', 0, 'question', 't')")
            .execute(&db)
            .await
            .is_err());
        // Removing a run, or its project, drops its bookmarks.
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_bookmarks WHERE run_id = 'r1'").await, 0);
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM run_bookmarks").await, 0);
    }

    #[tokio::test]
    async fn agents_and_commands_tables_on_an_existing_database() {
        let db = db_at_0001().await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        for (table, link, col) in [("agents", "project_agents", "agent_id"), ("commands", "project_commands", "command_id")] {
            for sql in [
                format!("INSERT INTO {table} (id, name, description, source_path, updated_at) VALUES ('i1', 'x', 'd', '/lib/x.md', 'u')"),
                format!("INSERT INTO {link} (project_id, {col}, synced_hash_claude, applied_at) VALUES ('p1', 'i1', 'h', 't'), ('p2', 'i1', NULL, 't')"),
            ] {
                sqlx::query(&sql).execute(&db).await.unwrap();
            }
            let count = |sql: String| {
                let db = db.clone();
                async move { sqlx::query_scalar::<_, i64>(&sql).fetch_one(&db).await.unwrap() }
            };
            // Removing a project or the item drops its links.
            sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
            assert_eq!(count(format!("SELECT COUNT(*) FROM {link}")).await, 1, "{link}");
            sqlx::query(&format!("DELETE FROM {table} WHERE id = 'i1'")).execute(&db).await.unwrap();
            assert_eq!(count(format!("SELECT COUNT(*) FROM {link}")).await, 0, "{link}");
            sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
                .execute(&db)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn existing_projects_keep_the_default_look_and_stay_visible() {
        let db = db_at_0001().await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let row = sqlx::query("SELECT color, icon, hidden FROM projects WHERE id = 'p1'")
            .fetch_one(&db)
            .await
            .unwrap();
        let look: (Option<String>, Option<String>, bool) = (row.get("color"), row.get("icon"), row.get("hidden"));
        assert_eq!(look, (None, None, false));
    }

    /// UX-19: old projects follow the global defaults; the legacy `default_engine` is untouched.
    #[tokio::test]
    async fn existing_projects_get_no_session_defaults() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at, model) VALUES ('r1', 'p1', 'session', 'c', 'claude-x')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        assert_eq!(
            crate::commands::session_defaults::load(&db, "p1").await,
            crate::commands::session_defaults::ProjectSessionDefaults::default()
        );
        let legacy: String = sqlx::query_scalar("SELECT default_engine FROM projects WHERE id = 'p1'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(legacy, "claude");
        let (model, effort): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT model, effort FROM runs WHERE id = 'r1'").fetch_one(&db).await.unwrap();
        assert_eq!((model.as_deref(), effort), (Some("claude-x"), None));
        // A project added after the migration (no engine given) follows the global default too.
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p2', 'b', '/b', 'c')").execute(&db).await.unwrap();
        assert_eq!(crate::commands::session_defaults::load(&db, "p2").await.engine, None);
    }

    /// UX-31: the table starts empty on an old database (the app copies the
    /// localStorage decisions in itself) and its rows go with their project.
    #[tokio::test]
    async fn tool_permissions_start_empty_and_go_with_their_project() {
        use crate::commands::tool_permissions as perms;
        let db = db_at_0001().await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        assert!(perms::list(&db, None).await.unwrap().is_empty());

        perms::set(&db, "p1", "Bash", Some(perms::ALLOW)).await.unwrap();
        // The decision column takes only allow / deny.
        assert!(sqlx::query(
            "INSERT INTO project_tool_permissions (project_id, tool_name, decision, created_at) VALUES ('p1', 'Read', 'ask', 'c')",
        )
        .execute(&db)
        .await
        .is_err());
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert!(perms::list(&db, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn google_mcp_stays_on_only_where_an_account_is_connected() {
        use crate::commands::mcp::builtin_mcp_globally_enabled;
        let fresh = db_at_0001().await;
        sqlx::migrate!("./migrations").run(&fresh).await.unwrap();
        assert!(!builtin_mcp_globally_enabled(&fresh, "google").await);
        assert!(builtin_mcp_globally_enabled(&fresh, "puppeteer").await);

        let connected = db_at_0001().await;
        sqlx::query("INSERT INTO google_accounts (id, label, created_at) VALUES ('g1', 'work', 'c')")
            .execute(&connected)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&connected).await.unwrap();
        assert!(builtin_mcp_globally_enabled(&connected, "google").await);

        // A switch the user already turned off stays off.
        let switched_off = db_at_0001().await;
        for sql in [
            "INSERT INTO google_accounts (id, label, created_at) VALUES ('g1', 'work', 'c')",
            "INSERT INTO settings (key, value) VALUES ('mcp_builtin_google_enabled', 'false')",
        ] {
            sqlx::query(sql).execute(&switched_off).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&switched_off).await.unwrap();
        assert!(!builtin_mcp_globally_enabled(&switched_off, "google").await);
    }

    /// HD-04: runs from before 0021 ended for a reason the app never recorded.
    #[tokio::test]
    async fn existing_runs_are_not_marked_interrupted() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, status, created_at) VALUES
                ('r1', 'p1', 'session', 'done', 'c'), ('r2', 'p1', 'session', 'cancelled', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let rows: Vec<(String, Option<String>, i64)> =
            sqlx::query_as("SELECT id, end_reason, interrupted_pending FROM runs ORDER BY id")
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(rows, vec![("r1".into(), None, 0), ("r2".into(), None, 0)]);
    }

    /// QO-06: runs from before 0026 are bound to no routing policy, and the
    /// tables start empty until the startup seed writes version 1.
    #[tokio::test]
    async fn existing_runs_get_no_routing_policy() {
        let db = db_at_0001().await;
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, status, created_at, session_id) VALUES
                ('r1', 'p1', 'session', 'done', 'c', 's1'), ('r2', 'p1', 'session', 'fetched', 'c', NULL)",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let versions: Vec<(String, Option<i64>)> =
            sqlx::query_as("SELECT id, routing_policy_version FROM runs ORDER BY id").fetch_all(&db).await.unwrap();
        assert_eq!(versions, vec![("r1".into(), None), ("r2".into(), None)]);
        for table in ["routing_policies", "routing_decisions"] {
            let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&db).await.unwrap();
            assert_eq!(n, 0, "{table}");
        }
        crate::runs::routing::policy::ensure_seeded(&db).await.unwrap();
        let seeded: Vec<(i64, String)> =
            sqlx::query_as("SELECT version, source FROM routing_policies").fetch_all(&db).await.unwrap();
        assert_eq!(seeded, vec![(1, "default".into())]);
    }

    /// HD-10: phones subscribed before 0022 get `stuck` switched on, once, and
    /// keep every kind they already had on or off.
    #[tokio::test]
    async fn push_devices_from_before_stuck_get_it_once_and_keep_their_choices() {
        use crate::remote::push::coalesce::PushEvents;
        let db = db_at_0001().await;
        sqlx::query(
            "INSERT INTO remote_push_subscriptions (id, endpoint, p256dh, auth, events, created_at) VALUES
                ('all', 'https://e/1', 'k', 'a', 'pending,done,failed', 'c'),
                ('no_done', 'https://e/2', 'k', 'a', 'pending,failed', 'c'),
                ('has', 'https://e/3', 'k', 'a', 'pending,stuck', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let events = || async {
            sqlx::query_as::<_, (String, String)>("SELECT id, events FROM remote_push_subscriptions ORDER BY id")
                .fetch_all(&db)
                .await
                .unwrap()
        };
        let want = vec![
            ("all".to_string(), "pending,done,failed,stuck".to_string()),
            ("has".to_string(), "pending,stuck".to_string()),
            ("no_done".to_string(), "pending,failed,stuck".to_string()),
        ];
        assert_eq!(events().await, want);
        let no_done = PushEvents::from_list(&want[2].1);
        assert_eq!(no_done, PushEvents { pending: true, done: false, failed: true, stuck: true });

        // Run again: nothing is added twice.
        sqlx::raw_sql(include_str!("../migrations/0022_push_stuck.sql")).execute(&db).await.unwrap();
        assert_eq!(events().await, want);
    }

    /// HD-AC-01 (data): a run still `running` when the app exited is told apart
    /// from one the user stopped, and History gets the reason.
    #[tokio::test]
    async fn a_run_cut_off_by_the_app_is_marked_apart_from_one_the_user_stopped() {
        let db = db_at_0001().await;
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO runs (id, project_id, type, status, created_at, finished_at) VALUES
                ('a', 'p1', 'session', 'running', 'c', NULL),
                ('b', 'p1', 'session', 'cancelled', 'c', 'stopped'),
                ('d', 'p1', 'session', 'done', 'c', 'done')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        super::reconcile_interrupted_runs(&db).await.unwrap();

        let rows: Vec<(String, String, Option<String>, i64, Option<String>)> = sqlx::query_as(
            "SELECT id, status, end_reason, interrupted_pending, finished_at FROM runs ORDER BY id",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        let (a, rest) = rows.split_first().unwrap();
        assert_eq!((a.1.as_str(), a.2.as_deref(), a.3), ("cancelled", Some("app_closed"), 1));
        assert!(a.4.is_some(), "an interrupted run gets a finish time");
        assert_eq!(
            rest,
            &[
                ("b".into(), "cancelled".into(), None, 0, Some("stopped".into())),
                ("d".into(), "done".into(), None, 0, Some("done".into())),
            ]
        );

        let listed = crate::commands::github::list_runs_inner(&db, "p1").await.unwrap();
        let reason = |id: &str| listed.iter().find(|r| r.id == id).unwrap().end_reason.clone();
        assert_eq!(reason("a").as_deref(), Some("app_closed"));
        assert_eq!((reason("b"), reason("d")), (None, None));
    }

    /// HD-04: a run that runs a turn again loses its "interrupted" mark. Every
    /// write that moves a run to `running` is in a start / resume path that needs
    /// an AppHandle, so the source is checked: each one must clear the mark.
    #[test]
    fn every_switch_to_running_clears_the_interrupted_mark() {
        fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let mut files = Vec::new();
        sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        // Split so this test does not find itself. Test fixtures that only set
        // the status (`... = 'running' WHERE`) are not run starts, and
        // `conductor_sessions` rows are not runs.
        let needle = concat!("UPDATE runs SET status = 'running'", ", ");
        let mut found = 0;
        for (file, src) in &files {
            for (at, _) in src.match_indices(needle) {
                found += 1;
                let rest = &src[at + needle.len()..];
                assert!(rest.starts_with("end_reason = NULL, interrupted_pending = 0,"), "{file}: {}", rest.chars().take(80).collect::<String>());
            }
        }
        // start_run_inner, and resume_run_with's two variants.
        assert_eq!(found, 3);
    }

    /// A database a user had right before 0032: every migration up to 0031 applied.
    async fn db_before_0032() -> super::Db {
        let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let all = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for entry in std::fs::read_dir(&all).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.as_str() < "0032" {
                std::fs::copy(all.join(&name), dir.join(&name)).unwrap();
            }
        }
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&pool).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        pool
    }

    #[tokio::test]
    async fn every_project_starts_with_slack_dms_off_and_opts_in_on_its_own() {
        let db = db_before_0032().await;
        // Before 0032 the column meant "follow the setting" and defaulted to 1.
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE projects SET slack_dm_read = 0 WHERE id = 'p2'").execute(&db).await.unwrap();

        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        let dm = |id: &'static str| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, i64>("SELECT slack_dm_read FROM projects WHERE id = ?")
                    .bind(id)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };
        assert_eq!((dm("p1").await, dm("p2").await), (0, 0));
        // A project inserted without the column is off.
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p3', 'c', '/c', 'c')")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(dm("p3").await, 0);
        // Opting one project in leaves the others off.
        crate::slack_threads::dm::set_project(&db, "p1", true).await.unwrap();
        assert_eq!((dm("p1").await, dm("p2").await, dm("p3").await), (1, 0, 0));
    }
}
