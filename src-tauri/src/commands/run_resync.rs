//! "Re-sync" a session (#19, RT-16): rebuild its log (`.puppeteer/runs/<id>.log`) from the
//! engine's transcript now, even when the transcript did not grow — the watcher and the
//! project-open reconcile only follow a transcript that grew (`sessions::upsert_session_run_core`).
//!
//! The transcript is the copy the run follows (RT-01 `pick_claude_transcript`: its
//! `transcript_path`, or a copy written after it); a Codex thread's tracked rollout, else
//! the newest one found. A running session is refused: its live stream owns the log.
//! The rebuild replaces the whole log, so lines only the app wrote (stderr, its own notes)
//! go, and bookmarks / quotes placed by position may move: the UI asks first.
//!
//! Remote: allowed + audited.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::commands::codex_sessions::{codex_session_file_in, codex_sessions_roots, parse_codex_rollout};
use crate::commands::sessions::{
    claude_sessions_dirs, parse_transcript, pick_claude_transcript, upsert_session_run_core, SyncOutcome,
};
use crate::db::Db;
use crate::runs::RunRegistry;

/// What a re-sync did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResyncOutcome {
    /// The transcript the log was rebuilt from.
    pub transcript_path: String,
}

/// Where to look for transcripts (the stores of this machine, or a test's folders).
pub(crate) struct Stores {
    pub claude_dirs: Vec<PathBuf>,
    pub codex_roots: Vec<PathBuf>,
}

/// Rebuild `run_id`'s log from its transcript (see the module docs).
pub(crate) async fn resync(db: &Db, registry: &RunRegistry, run_id: &str, stores: &Stores) -> Result<ResyncOutcome, String> {
    let row = sqlx::query(
        "SELECT r.status, r.session_id, r.engine, r.transcript_path, r.project_id, p.name AS project_name, \
                p.path AS project_path \
         FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ? AND r.deleted_at IS NULL",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or("Session not found")?;
    let status: String = row.get("status");
    if status == "running" || registry.lock().await.contains_key(run_id) {
        return Err("The session is running: re-sync it once the turn has finished".into());
    }
    let session_id: String = row
        .get::<Option<String>, _>("session_id")
        .ok_or("This session has no engine transcript yet")?;
    let engine: String = row.get("engine");
    let tracked: Option<String> = row.get("transcript_path");
    let tracked = tracked.as_deref().map(Path::new);
    let project_id: String = row.get("project_id");
    let project_name: String = row.get("project_name");
    let project_path: String = row.get("project_path");

    let codex = engine == "codex";
    let file = if codex {
        tracked
            .filter(|p| p.is_file())
            .map(Path::to_path_buf)
            .or_else(|| codex_session_file_in(&stores.codex_roots, &session_id))
    } else {
        pick_claude_transcript(&stores.claude_dirs, &session_id, tracked, None)
    }
    .ok_or("The session's transcript was not found on this machine")?;

    let parse = if codex { parse_codex_rollout } else { parse_transcript };
    let outcome = upsert_session_run_core(
        db,
        &project_id,
        &project_name,
        &project_path,
        &session_id,
        &engine,
        &file,
        parse,
        false,
        true,
    )
    .await?;
    match outcome {
        SyncOutcome::Updated(id) if id == run_id => {
            tracing::info!(event = "run_resynced", run_id, transcript = %file.display());
            Ok(ResyncOutcome { transcript_path: file.to_string_lossy().into_owned() })
        }
        _ => Err("The transcript could not be read for this session".into()),
    }
}

/// Rebuild a session's log from its engine transcript. Remote: allowed + audited.
#[tauri::command]
pub async fn resync_run(
    db: State<'_, Db>,
    registry: State<'_, RunRegistry>,
    run_id: String,
) -> Result<ResyncOutcome, String> {
    let db = db.inner();
    let project_path: Option<String> = sqlx::query_scalar(
        "SELECT p.path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
    )
    .bind(&run_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    let stores = Stores {
        claude_dirs: match project_path {
            Some(p) => claude_sessions_dirs(db, &p).await,
            None => Vec::new(),
        },
        codex_roots: codex_sessions_roots(db).await,
    };
    resync(db, registry.inner(), &run_id, &stores).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn exchange(cwd: &str, prompt: &str, reply: &str) -> String {
        let user = serde_json::json!({"type": "user", "cwd": cwd, "message": {"role": "user", "content": prompt}});
        let assistant = serde_json::json!({"type": "assistant", "cwd": cwd, "message": {"role": "assistant", "content": [{"type": "text", "text": reply}]}});
        format!("{user}\n{assistant}\n")
    }

    fn set_age(path: &Path, secs_ago: u64) {
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    #[tokio::test]
    async fn rebuilds_from_the_tracked_copy_and_refuses_a_running_session() {
        let root = std::env::temp_dir().join(format!("puppeteer-resync-{}", uuid::Uuid::new_v4()));
        let project = root.join("proj");
        let cwd = project.to_str().unwrap().to_string();
        let home = root.join("home");
        let account = root.join("account");
        for d in [&project.join(".puppeteer").join("runs"), &home, &account] {
            fs::create_dir_all(d).unwrap();
        }
        // Two copies of the session: the run follows the account one, which is newer.
        let home_copy = home.join("s1.jsonl");
        let account_copy = account.join("s1.jsonl");
        fs::write(&home_copy, exchange(&cwd, "stale turn", "old")).unwrap();
        let turns = exchange(&cwd, "turn one", "ok one") + &exchange(&cwd, "progress so far?", "halfway");
        fs::write(&account_copy, &turns).unwrap();
        set_age(&home_copy, 600);
        set_age(&account_copy, 300);

        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
            .bind(&cwd)
            .execute(&db)
            .await
            .unwrap();
        // Already mirrored at this size: the watcher would skip it.
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, transcript_synced_size, created_at)
             VALUES ('r1', 'p1', 'session', 'done', 'claude', 's1', ?, ?, 'c'),
                    ('r2', 'p1', 'session', 'done', 'claude', NULL, NULL, NULL, 'c')",
        )
        .bind(account_copy.to_str().unwrap())
        .bind(turns.len() as i64)
        .execute(&db)
        .await
        .unwrap();
        let log = project.join(".puppeteer").join("runs").join("r1.log");
        fs::write(&log, "a log that lost its turns\n").unwrap();
        let stores = Stores { claude_dirs: vec![home.clone(), account.clone()], codex_roots: Vec::new() };
        let registry = crate::runs::new_registry();

        // The watcher path leaves it (same size), a re-sync rebuilds it from the account copy.
        let watcher =
            upsert_session_run_core(&db, "p1", "p", &cwd, "s1", "claude", &account_copy, parse_transcript, false, false)
                .await
                .unwrap();
        assert!(matches!(watcher, SyncOutcome::Skipped));
        let out = resync(&db, &registry, "r1", &stores).await.unwrap();
        assert_eq!(out.transcript_path, account_copy.to_string_lossy());
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("progress so far?") && !text.contains("stale turn"), "{text}");

        // Running: refused, the log untouched.
        fs::write(&log, "live\n").unwrap();
        sqlx::query("UPDATE runs SET status = 'running' WHERE id = 'r1'").execute(&db).await.unwrap();
        assert!(resync(&db, &registry, "r1", &stores).await.unwrap_err().contains("running"));
        sqlx::query("UPDATE runs SET status = 'done' WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(fs::read_to_string(&log).unwrap(), "live\n");

        // No session id, no transcript, or no such session: refused.
        assert!(resync(&db, &registry, "r2", &stores).await.is_err());
        assert!(resync(&db, &registry, "missing", &stores).await.is_err());
        let empty = Stores { claude_dirs: Vec::new(), codex_roots: Vec::new() };
        fs::remove_file(&account_copy).unwrap();
        assert!(resync(&db, &registry, "r1", &empty).await.unwrap_err().contains("not found"));
        let _ = fs::remove_dir_all(&root);
    }
}
