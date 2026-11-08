//! Cross-session search: a full-text index over what a reader sees of every
//! run log (migration 0003), and `search_sessions` over it.
//!
//! A log is indexed the way `search_run_log` reads one: the user / assistant
//! text of each record ([`record_texts`]), never tool output or thinking, one
//! row per record with its byte offset, so a hit opens its run at that record.
//! Mirrored Claude / Codex sessions have a run log like any run and are indexed
//! the same way. The secret vault is never in a run log or in SQLite, so it
//! cannot reach the index.
//!
//! A run is re-indexed whole whenever its log may have changed
//! ([`refresh_run`]): when a turn's drain ends, when a mirrored session is
//! imported or updated, and after startup for every run ([`catch_up`], which
//! is also the backfill of runs from before the index). The log's path, size
//! and mtime are recorded, so an unchanged log is not read again. While
//! `sessions.cross_session_search` is off nothing is indexed; turning it back
//! on catches up.

use crate::commands::runs::{
    fetch_run_log_row, find_match, match_snippet, record_texts, resolve_log_source, stat_file,
};
use crate::db::Db;
use crate::features::{self, apply::Change};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

/// Prop gating the index and `search_sessions`.
pub const FEATURE: &str = "sessions.cross_session_search";

const MAX_HITS: usize = 200;
/// The trigram index only answers queries of at least this many characters.
const TRIGRAM_CHARS: usize = 3;

/// One matching log record.
#[derive(Debug, Serialize, PartialEq)]
pub struct SessionSearchHit {
    pub run_id: String,
    pub project_id: String,
    pub project_name: String,
    pub title: Option<String>,
    pub engine: String,
    pub run_type: String,
    /// The run's History sort key: last worked on, else created.
    pub activity_at: String,
    /// Byte offset the record starts at — a `get_run_log_forward` cursor.
    pub offset: u64,
    /// The record's first match with some context, as `search_run_log` gives it.
    pub snippet: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SessionSearch {
    pub hits: Vec<SessionSearchHit>,
    /// More records matched than were returned.
    pub truncated: bool,
}

/// Find `query` (a literal substring, case-insensitive) in every indexed run,
/// most recently active runs first. Read-only, like `list_runs`, so a remote
/// Controller may call it too.
#[tauri::command]
pub async fn search_sessions(
    db: State<'_, Db>,
    query: String,
    project_id: Option<String>,
) -> Result<SessionSearch, String> {
    features::require(db.inner(), FEATURE).await?;
    search(db.inner(), &query, project_id.as_deref(), MAX_HITS)
        .await
        .map_err(|e| e.to_string())
}

async fn search(
    db: &Db,
    query: &str,
    project_id: Option<&str>,
    max_hits: usize,
) -> Result<SessionSearch, sqlx::Error> {
    use sqlx::Row;

    if query.is_empty() {
        return Ok(SessionSearch { hits: Vec::new(), truncated: false });
    }
    // A quoted trigram phrase is a substring match. Shorter queries have no
    // trigram, so they scan the records (LIKE folds ASCII case only).
    let (cond, pattern) = if query.chars().count() >= TRIGRAM_CHARS {
        (
            "s.id IN (SELECT rowid FROM session_search_fts WHERE session_search_fts MATCH ?1)",
            format!("\"{}\"", query.replace('"', "\"\"")),
        )
    } else {
        let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        ("s.body LIKE ?1 ESCAPE '\\'", format!("%{escaped}%"))
    };
    let sql = format!(
        "SELECT s.run_id, s.log_offset, s.body, r.project_id, p.name AS project_name, r.title,
                r.engine, r.type AS run_type, COALESCE(r.last_activity_at, r.created_at) AS activity_at
         FROM session_search_records s
         JOIN runs r ON r.id = s.run_id
         JOIN projects p ON p.id = r.project_id
         WHERE {cond} AND (?2 IS NULL OR r.project_id = ?2)
         ORDER BY activity_at DESC, s.run_id, s.log_offset
         LIMIT ?3"
    );
    let rows = sqlx::query(&sql)
        .bind(pattern)
        .bind(project_id)
        .bind(max_hits as i64 + 1)
        .fetch_all(db)
        .await?;
    let truncated = rows.len() > max_hits;
    let hits = rows
        .iter()
        .take(max_hits)
        .map(|r| {
            let body: String = r.get("body");
            let snippet = match find_match(&body, query, false) {
                Some((s, e)) => match_snippet(&body, s, e),
                // The index folds case a little differently from `find_match`.
                None => match_snippet(&body, 0, 0),
            };
            SessionSearchHit {
                run_id: r.get("run_id"),
                project_id: r.get("project_id"),
                project_name: r.get("project_name"),
                title: r.get("title"),
                engine: r.get("engine"),
                run_type: r.get("run_type"),
                activity_at: r.get("activity_at"),
                offset: r.get::<i64, _>("log_offset") as u64,
                snippet,
            }
        })
        .collect();
    Ok(SessionSearch { hits, truncated })
}

/// The records of a log that have user / assistant text, as (byte offset,
/// text blocks joined by a newline). Offsets are counted the way
/// `search_run_log` counts them.
fn log_records(path: &Path) -> std::io::Result<Vec<(i64, String)>> {
    use std::io::{BufRead, BufReader};

    let mut out = Vec::new();
    let mut reader = BufReader::with_capacity(256 * 1024, std::fs::File::open(path)?);
    let mut offset: i64 = 0;
    let mut raw: Vec<u8> = Vec::new();
    loop {
        raw.clear();
        let n = reader.read_until(b'\n', &mut raw)?;
        if n == 0 {
            break;
        }
        let start = offset;
        offset += n as i64;
        let line = String::from_utf8_lossy(&raw);
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        // Cheap reject before paying for a JSON parse.
        if !line.contains("\"user\"") && !line.contains("\"assistant\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let body = record_texts(&v).join("\n");
        if !body.trim().is_empty() {
            out.push((start, body));
        }
    }
    Ok(out)
}

/// Re-index `run_id` if its log changed since it was last indexed. Does nothing
/// while the feature is off. The index is only a cache: a failure is logged and
/// the next change or startup tries again.
pub async fn refresh_run(db: &Db, run_id: &str) {
    if !features::is_on(db, FEATURE).await {
        return;
    }
    if let Err(e) = index_run(db, run_id).await {
        tracing::warn!(event = "session_search_index_failed", run_id = %run_id, error = %e);
    }
}

/// [`refresh_run`] off the caller's task, for paths that answer the UI.
pub fn refresh_run_later(db: &Db, run_id: &str) {
    let (db, run_id) = (db.clone(), run_id.to_string());
    tauri::async_runtime::spawn(async move { refresh_run(&db, &run_id).await });
}

/// Index every run whose log changed since it was last indexed, most recently
/// active first; stops as soon as the feature is turned off.
pub async fn catch_up(db: &Db) {
    let ids: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM runs ORDER BY COALESCE(last_activity_at, created_at) DESC",
    )
    .fetch_all(db)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(event = "session_search_catch_up_failed", error = %e);
            return;
        }
    };
    for id in ids {
        if !features::is_on(db, FEATURE).await {
            return;
        }
        if let Err(e) = index_run(db, &id).await {
            tracing::warn!(event = "session_search_index_failed", run_id = %id, error = %e);
        }
    }
}

/// `features::apply` hook: catch up as soon as the switch is turned back on.
pub fn on_features_changed(app: &AppHandle, change: &Change<'_>) {
    if features::effective(change.before, FEATURE) || !features::effective(change.after, FEATURE) {
        return;
    }
    let db = app.state::<Db>().inner().clone();
    tauri::async_runtime::spawn(async move { catch_up(&db).await });
}

async fn index_run(db: &Db, run_id: &str) -> Result<(), String> {
    let row = fetch_run_log_row(db, run_id).await?;
    let log = resolve_log_source(&row, run_id)
        .and_then(|(_, path)| stat_file(&path).map(|(size, mtime)| (path, size as i64, mtime)));
    let seen: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT log_path, log_size, log_mtime_ns FROM session_search_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;

    let records = match &log {
        Some((path, size, mtime)) => {
            if seen.as_ref() == Some(&(path.to_string_lossy().into_owned(), *size, *mtime)) {
                return Ok(());
            }
            let path = path.clone();
            tokio::task::spawn_blocking(move || log_records(&path))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("read log: {e}"))?
        }
        None if seen.is_none() => return Ok(()),
        // The log is gone: nothing of the run can be found any more.
        None => Vec::new(),
    };

    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    // Delete first: from here this is a write transaction, so a run deleted
    // meanwhile is either seen gone below or deletes these rows after us.
    for sql in [
        "DELETE FROM session_search_records WHERE run_id = ?",
        "DELETE FROM session_search_runs WHERE run_id = ?",
    ] {
        sqlx::query(sql).bind(run_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if let (Some((path, size, mtime)), Some(_)) = (&log, exists) {
        for (offset, body) in &records {
            sqlx::query("INSERT INTO session_search_records (run_id, log_offset, body) VALUES (?, ?, ?)")
                .bind(run_id)
                .bind(offset)
                .bind(body)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        sqlx::query(
            "INSERT INTO session_search_runs (run_id, log_path, log_size, log_mtime_ns) VALUES (?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(path.to_string_lossy().as_ref())
        .bind(size)
        .bind(mtime)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    fn user(text: &str) -> String {
        serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string() + "\n"
    }

    fn assistant(blocks: serde_json::Value) -> String {
        serde_json::json!({ "type": "assistant", "message": { "role": "assistant", "content": blocks } })
            .to_string()
            + "\n"
    }

    fn tool_result(text: &str) -> String {
        serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "tool_result", "content": text }] }
        })
        .to_string()
            + "\n"
    }

    /// A project in its own temp dir, with one run per `(id, activity_at)`.
    async fn project(db: &Db, id: &str, runs: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-search-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".puppeteer/runs")).unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES (?, ?, ?, 'c')")
            .bind(id)
            .bind(format!("{id}-name"))
            .bind(dir.to_string_lossy().as_ref())
            .execute(db)
            .await
            .unwrap();
        for (run, at) in runs {
            sqlx::query(
                "INSERT INTO runs (id, project_id, type, status, engine, created_at, last_activity_at, title)
                 VALUES (?, ?, 'session', 'done', 'claude', 'c', ?, ?)",
            )
            .bind(run)
            .bind(id)
            .bind(at)
            .bind(format!("{run} title"))
            .execute(db)
            .await
            .unwrap();
        }
        dir
    }

    fn write_log(dir: &Path, run: &str, content: &str) {
        fs::write(dir.join(".puppeteer/runs").join(format!("{run}.log")), content).unwrap();
    }

    async fn find(db: &Db, query: &str) -> Vec<(String, u64, String)> {
        search(db, query, None, MAX_HITS)
            .await
            .unwrap()
            .hits
            .into_iter()
            .map(|h| (h.run_id, h.offset, h.snippet))
            .collect()
    }

    async fn count(db: &Db, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(db).await.unwrap()
    }

    #[test]
    fn only_user_and_assistant_text_is_indexed() {
        let first = user("Fix the login bug");
        let log = [
            first.clone(),
            tool_result("secret file body"),
            assistant(serde_json::json!([
                { "type": "thinking", "thinking": "hidden thoughts" },
                { "type": "text", "text": "Done." },
                { "type": "tool_use", "name": "Bash", "input": { "command": "ls" } },
                { "type": "text", "text": "All tests pass." }
            ])),
            "{\"type\":\"result\",\"result\":\"user assistant\"}\n".to_string(),
        ]
        .concat();
        let path = std::env::temp_dir().join(format!("puppeteer-search-{}.log", uuid::Uuid::new_v4()));
        fs::write(&path, &log).unwrap();
        let got = log_records(&path).unwrap();
        let _ = fs::remove_file(&path);
        let second_at = (first.len() + tool_result("secret file body").len()) as i64;
        assert_eq!(
            got,
            vec![(0, "Fix the login bug".to_string()), (second_at, "Done.\nAll tests pass.".to_string())]
        );
    }

    #[tokio::test]
    async fn search_finds_runs_with_a_snippet_and_offset() {
        let db = db().await;
        let dir = project(&db, "p1", &[("old", "2026-01-01"), ("new", "2026-02-01")]).await;
        let intro = user("hello");
        write_log(&dir, "old", &[intro.clone(), user("Please review the 承認フロー before release")].concat());
        write_log(&dir, "new", &assistant(serde_json::json!([{ "type": "text", "text": "承認 is approval" }])));
        catch_up(&db).await;

        // Most recently active run first; offsets point at the matching record.
        assert_eq!(
            find(&db, "承認").await,
            vec![
                ("new".into(), 0, "承認 is approval".into()),
                ("old".into(), intro.len() as u64, "Please review the 承認フロー before release".into()),
            ]
        );
        // 3+ characters go through the trigram index, case-insensitively.
        assert_eq!(find(&db, "REVIEW THE").await.len(), 1);
        assert_eq!(find(&db, "承認フロー").await.len(), 1);
        assert!(find(&db, "absent text").await.is_empty());
        assert!(find(&db, "").await.is_empty());
        // FTS syntax is taken literally.
        assert!(find(&db, "\"review\" OR x*").await.is_empty());
        assert_eq!(find(&db, "_").await, vec![], "LIKE wildcards are escaped");

        let hit = &search(&db, "approval", None, MAX_HITS).await.unwrap().hits[0];
        assert_eq!(
            (hit.project_id.as_str(), hit.project_name.as_str(), hit.title.as_deref(), hit.engine.as_str()),
            ("p1", "p1-name", Some("new title"), "claude")
        );
        assert_eq!((hit.run_type.as_str(), hit.activity_at.as_str()), ("session", "2026-02-01"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn search_filters_by_project_and_caps_the_hits() {
        let db = db().await;
        let a = project(&db, "pa", &[("ra", "2026-01-01")]).await;
        let b = project(&db, "pb", &[("rb", "2026-01-02")]).await;
        write_log(&a, "ra", &[user("needle one"), user("needle two")].concat());
        write_log(&b, "rb", &user("needle three"));
        catch_up(&db).await;

        let only_a = search(&db, "needle", Some("pa"), MAX_HITS).await.unwrap();
        assert_eq!(only_a.hits.iter().map(|h| h.run_id.as_str()).collect::<Vec<_>>(), ["ra", "ra"]);
        let capped = search(&db, "needle", None, 2).await.unwrap();
        assert_eq!((capped.hits.len(), capped.truncated), (2, true));
        assert!(!search(&db, "needle", None, 3).await.unwrap().truncated);
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[tokio::test]
    async fn a_changed_log_is_reindexed_and_an_unchanged_one_is_not_read() {
        let db = db().await;
        let dir = project(&db, "p1", &[("r1", "2026-01-01")]).await;
        write_log(&dir, "r1", &user("first turn"));
        refresh_run(&db, "r1").await;
        assert_eq!(find(&db, "first").await.len(), 1);

        // Unchanged log: the recorded size and mtime match, so it is skipped
        // (rows dropped by hand stay dropped).
        sqlx::query("DELETE FROM session_search_records").execute(&db).await.unwrap();
        refresh_run(&db, "r1").await;
        assert!(find(&db, "first").await.is_empty());

        // A turn appended (as the drain or a mirror rewrite does) is picked up.
        write_log(&dir, "r1", &[user("first turn"), user("second turn")].concat());
        refresh_run(&db, "r1").await;
        assert_eq!(find(&db, "turn").await.len(), 2);

        // A rewritten log replaces the old rows rather than adding to them.
        write_log(&dir, "r1", &user("rewritten"));
        refresh_run(&db, "r1").await;
        assert!(find(&db, "turn").await.is_empty());
        assert_eq!(find(&db, "rewritten").await.len(), 1);

        // The log is gone: the run's rows go too.
        fs::remove_file(dir.join(".puppeteer/runs/r1.log")).unwrap();
        refresh_run(&db, "r1").await;
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_records").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_runs").await, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn deleting_a_run_or_its_project_drops_its_index() {
        let db = db().await;
        let dir = project(&db, "p1", &[("r1", "2026-01-01"), ("r2", "2026-01-02")]).await;
        write_log(&dir, "r1", &user("needle in r1"));
        write_log(&dir, "r2", &user("needle in r2"));
        catch_up(&db).await;
        assert_eq!(find(&db, "needle").await.len(), 2);

        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        assert_eq!(find(&db, "needle").await.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(), ["r2"]);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_runs WHERE run_id = 'r1'").await, 0);

        // A project's cascade deletes its runs, and the trigger fires for them.
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_records").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_runs").await, 0);
        // The FTS index followed the content table.
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM session_search_fts WHERE session_search_fts MATCH '\"needle\"'").await,
            0
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn nothing_is_indexed_while_the_feature_is_off() {
        let db = db().await;
        let dir = project(&db, "p1", &[("r1", "2026-01-01")]).await;
        write_log(&dir, "r1", &user("needle"));
        crate::commands::settings::set_setting(&db, "feature.sessions.cross_session_search", "false")
            .await
            .unwrap();
        refresh_run(&db, "r1").await;
        catch_up(&db).await;
        assert_eq!(count(&db, "SELECT COUNT(*) FROM session_search_records").await, 0);

        // Back on: the catch-up indexes what was missed.
        crate::commands::settings::set_setting(&db, "feature.sessions.cross_session_search", "true")
            .await
            .unwrap();
        catch_up(&db).await;
        assert_eq!(find(&db, "needle").await.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
