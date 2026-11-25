//! Short session names for the sidebar / History list.
//!
//! A session starts out titled after its first prompt, cut to a sentence
//! (`runs.title_auto = 1`: the app chose it). When a turn ends, that title is
//! replaced, once, by a short name: for Claude, the `ai-title` the CLI writes
//! into the session transcript (a file read, no AI call). Until one exists the
//! title stays, and the next turn end looks again.
//!
//! The write is a compare-and-set on `title_auto = 1`, so a name the user picks
//! while a name is being looked up always wins, and the first short name stays.

use crate::commands::sessions::{claude_sessions_dirs, pick_claude_transcript, truncate, TITLE_MAX_CHARS};
use crate::db::Db;
use serde_json::Value;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// App event for a run's new automatic title: `{ run_id, title }`. Global, not
/// per run: the name can land after the run's own listeners stopped at `run:done`.
pub const EVENT: &str = "run:title";

/// After a turn of `run_id` ended: name the session if it still has an app-set
/// title. Spawned, so the sidecar read loop never waits on it.
pub fn refresh_later(app: &AppHandle, db: &Db, run_id: &str) {
    let (app, db, run_id) = (app.clone(), db.clone(), run_id.to_string());
    tauri::async_runtime::spawn(async move {
        if let Some(title) = retitle(&db, &run_id).await {
            let _ = app.emit(EVENT, serde_json::json!({ "run_id": run_id, "title": title }));
        }
    });
}

/// A session whose title the app may still replace.
struct Candidate {
    engine: String,
    session_id: Option<String>,
    transcript_path: Option<String>,
    project_path: String,
}

async fn candidate(db: &Db, run_id: &str) -> Option<Candidate> {
    let row: Option<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT r.engine, r.session_id, r.transcript_path, p.path FROM runs r JOIN projects p ON p.id = r.project_id
         WHERE r.id = ? AND r.type = 'session' AND r.title_auto = 1",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    row.map(|(engine, session_id, transcript_path, project_path)| Candidate {
        engine,
        session_id,
        transcript_path,
        project_path,
    })
}

/// Replace the app-set title of `run_id` with a short name, if one is ready.
/// Returns the stored name; `None` when nothing changed.
async fn retitle(db: &Db, run_id: &str) -> Option<String> {
    let c = candidate(db, run_id).await?;
    let title = match c.engine.as_str() {
        "claude" => {
            let dirs = claude_sessions_dirs(db, &c.project_path).await;
            claude_title(&dirs, c.session_id.as_deref()?, c.transcript_path.as_deref()).await?
        }
        _ => return None,
    };
    replace(db, run_id, &title).await.then_some(title)
}

/// The last `ai-title` of the session's transcript (the copy the session sync
/// would follow), read on the blocking pool: a transcript can be tens of MB.
async fn claude_title(dirs: &[PathBuf], session_id: &str, tracked: Option<&str>) -> Option<String> {
    let file = pick_claude_transcript(dirs, session_id, tracked.map(Path::new), None)?;
    tokio::task::spawn_blocking(move || last_ai_title(&file)).await.ok().flatten()
}

/// The last non-empty `{"type":"ai-title","aiTitle":…}` line of a Claude
/// transcript, cut like any title. The CLI rewrites it as the talk moves on.
pub(crate) fn last_ai_title(file: &Path) -> Option<String> {
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let mut last = None;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        // Cheap filter: nearly every line is a turn that holds no title.
        if !line.contains("\"ai-title\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) != Some("ai-title") {
            continue;
        }
        if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()) {
            last = Some(t.to_string());
        }
    }
    last.map(|t| truncate(&t, TITLE_MAX_CHARS))
}

/// Store an automatic title, only over an app-set one (compare-and-set), and
/// mark it final. `true` when the row changed.
pub(crate) async fn replace(db: &Db, run_id: &str, title: &str) -> bool {
    let res = sqlx::query("UPDATE runs SET title = ?, title_auto = 0 WHERE id = ? AND title_auto = 1")
        .bind(title)
        .bind(run_id)
        .execute(db)
        .await;
    match res {
        Ok(r) => r.rows_affected() == 1,
        Err(e) => {
            tracing::warn!(event = "auto_title_write_failed", run_id, error = %e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn db() -> Db {
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/nowhere/auto-title', 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    /// A run `id` of `kind` on `engine` with the given title state.
    async fn run(db: &Db, id: &str, kind: &str, engine: &str, title: &str, title_auto: i64, transcript: Option<&Path>) {
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, title, title_auto, created_at)
             VALUES (?, 'p1', ?, 'running', ?, ?, ?, ?, ?, 'c')",
        )
        .bind(id)
        .bind(kind)
        .bind(engine)
        .bind(format!("s-{id}"))
        .bind(transcript.map(|p| p.to_string_lossy().to_string()))
        .bind(title)
        .bind(title_auto)
        .execute(db)
        .await
        .unwrap();
    }

    async fn title_of(db: &Db, id: &str) -> (String, i64) {
        sqlx::query_as("SELECT title, title_auto FROM runs WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-auto-title-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ai_title(t: &str) -> String {
        serde_json::json!({ "type": "ai-title", "aiTitle": t, "sessionId": "s" }).to_string()
    }

    fn user(text: &str) -> String {
        serde_json::json!({ "type": "user", "cwd": "/x", "message": { "role": "user", "content": text } }).to_string()
    }

    #[test]
    fn the_last_ai_title_wins_and_blank_ones_are_skipped() {
        let dir = temp_dir();
        let file = dir.join("s.jsonl");
        let lines = [user("a prompt about \"ai-title\""), ai_title("First"), user("more"), ai_title("Second"), ai_title("  ")];
        std::fs::write(&file, lines.join("\n")).unwrap();
        assert_eq!(last_ai_title(&file).as_deref(), Some("Second"));

        std::fs::write(&file, user("no title yet")).unwrap();
        assert_eq!(last_ai_title(&file), None);
        assert_eq!(last_ai_title(&dir.join("missing.jsonl")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_claude_session_takes_its_transcript_title_once() {
        let db = db().await;
        let dir = temp_dir();
        let file = dir.join("s-r1.jsonl");
        std::fs::write(&file, user("Explain the usage guard hook")).unwrap();
        run(&db, "r1", "session", "claude", "Explain the usage guard hook", 1, Some(&file)).await;

        // No ai-title yet: nothing changes, the next turn end looks again.
        assert_eq!(retitle(&db, "r1").await, None);
        assert_eq!(title_of(&db, "r1").await, ("Explain the usage guard hook".into(), 1));

        std::fs::write(&file, [user("Explain"), ai_title("First"), ai_title("Usage guard hook")].join("\n")).unwrap();
        assert_eq!(retitle(&db, "r1").await.as_deref(), Some("Usage guard hook"));
        assert_eq!(title_of(&db, "r1").await, ("Usage guard hook".into(), 0));

        // Replaced once: a newer ai-title later does not move it again.
        std::fs::write(&file, ai_title("Something else")).unwrap();
        assert_eq!(retitle(&db, "r1").await, None);
        assert_eq!(title_of(&db, "r1").await.0, "Usage guard hook");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_app_titled_sessions_are_renamed() {
        let db = db().await;
        let dir = temp_dir();
        let file = dir.join("t.jsonl");
        std::fs::write(&file, ai_title("Short name")).unwrap();
        // A user / Focus AI / pre-migration title, and a review run.
        run(&db, "mine", "session", "claude", "Mine", 0, Some(&file)).await;
        run(&db, "review", "review_pr", "claude", "PR #1", 1, Some(&file)).await;
        assert_eq!(retitle(&db, "mine").await, None);
        assert_eq!(retitle(&db, "review").await, None);
        assert_eq!(title_of(&db, "mine").await, ("Mine".into(), 0));
        assert_eq!(title_of(&db, "review").await, ("PR #1".into(), 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_transcript_is_found_by_session_id_when_no_path_is_stored() {
        let dir = temp_dir();
        std::fs::write(dir.join("s-r1.jsonl"), ai_title("Found by id")).unwrap();
        assert_eq!(claude_title(&[dir.clone()], "s-r1", None).await.as_deref(), Some("Found by id"));
        assert_eq!(claude_title(&[dir.clone()], "s-other", None).await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_automatic_write_never_overwrites_a_name_the_user_chose() {
        let db = db().await;
        run(&db, "r1", "session", "claude", "Mine", 0, None).await;
        assert!(!replace(&db, "r1", "Automatic").await);
        assert_eq!(title_of(&db, "r1").await, ("Mine".into(), 0));

        run(&db, "r2", "session", "claude", "Provisional", 1, None).await;
        assert!(replace(&db, "r2", "Automatic").await);
        assert!(!replace(&db, "r2", "Again").await);
        assert_eq!(title_of(&db, "r2").await, ("Automatic".into(), 0));
    }
}
