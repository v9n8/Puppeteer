//! Fork (UX-28): a new session that starts with another one's conversation —
//! all of it, or up to the end of one turn — and leaves the source as it is.
//!
//! The fork is a new run in the same project with the source's settings
//! (engine, model, effort, response style, Claude account, framing, MCP overrides) and
//! a copy of the source log up to the fork point, so it shows the history at
//! once. Files are not copied or restored: the fork works on the working tree
//! as it is now (going back in the files is Rewind's job).
//!
//! The engine session is forked lazily, at the fork's first resume, the way a
//! Rewind is applied: a `_puppeteer_fork` record after the copied log tells
//! `resume_run` which session to copy and where it ends.
//!
//! - Claude: resume the source session with `forkSession` (a new session id)
//!   and `resumeSessionAt` = the last kept transcript entry. That entry pins
//!   the fork point, so turns the source adds meanwhile never reach the fork.
//! - Codex: `thread/fork` the source thread, then `thread/rollback` what the
//!   copy holds past the kept number of turns (counted from the source log, so
//!   turns the source adds meanwhile are rolled back too).
//!
//! Until then the fork has no `session_id` of its own (the record holds the
//! source's), so the transcript watcher and deletes never mistake one for the
//! other. The record is pending until the engine starts on the fork (a
//! `system`/`init` record follows it). When the conversation cannot follow (the
//! kept part belongs to an engine session that is gone, or Codex turns that
//! cannot be counted), the record says so and the fork starts a new session:
//! the copied history stays on screen but the model does not have it.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use super::rewind::{chain_entry, init_session, is_prompt, kind, open_log};
use super::RunRegistry;
use crate::commands::github::RunRecord;
use crate::db::Db;

pub const MARKER: &str = "_puppeteer_fork";

/// What the fork's first resume does (the `_puppeteer_fork` record).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkStart {
    /// The source session to copy; None = nothing to carry, start a new one.
    pub session_id: Option<String>,
    /// The source's transcript, where `resume_run` looks for it first.
    pub transcript_path: Option<String>,
    /// Claude: the last kept transcript entry.
    pub resume_at: Option<String>,
    /// Codex: turns of the source thread the copy keeps.
    pub keep_turns: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
struct ForkCut {
    /// Bytes of the source log the fork keeps; None = all of it.
    len: Option<u64>,
    /// Highest snapshot turn among the kept prompts.
    last_turn: Option<i64>,
    start: ForkStart,
}

/// Where to cut `log` to keep everything up to the end of the turn whose
/// prompt record starts at byte `through` (the whole log when None), and how
/// the engine copies the conversation. None when `through` is no prompt's start.
fn analyze(log: impl BufRead, engine: &str, session: Option<&str>, through: Option<u64>) -> std::io::Result<Option<ForkCut>> {
    let mut offset = 0u64;
    let mut found = through.is_none();
    let mut cut: Option<u64> = None;
    let mut init_before: Option<Option<String>> = None;
    let mut other_session = false;
    let mut last_entry: Option<(String, Option<String>)> = None;
    let mut prompts = 0u32;
    let mut last_turn: Option<i64> = None;

    for line in log.split(b'\n') {
        let line = line?;
        let at = offset;
        offset += line.len() as u64 + 1;
        if line.first() != Some(&b'{') {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if is_prompt(&v) {
            if found && through.is_some() {
                // The next turn starts here: the fork ends before it.
                cut = Some(at);
                break;
            }
            if Some(at) == through {
                found = true;
            }
            prompts += 1;
            if let Some(t) = v.get("turn").and_then(Value::as_i64) {
                last_turn = Some(last_turn.map_or(t, |l| l.max(t)));
            }
        } else if let Some(s) = init_session(&v) {
            if s.as_deref() != session {
                other_session = true;
            }
            init_before = Some(s);
        } else if let Some((uuid, sid)) = chain_entry(&v) {
            last_entry = Some((uuid.to_string(), sid.map(str::to_string)));
        }
    }
    if !found {
        return Ok(None);
    }
    let same = session.is_some() && init_before.as_ref().is_some_and(|s| s.as_deref() == session);
    let carried = |start: ForkStart| ForkStart { session_id: session.map(str::to_string), ..start };
    let start = match engine {
        "claude" if same => match last_entry {
            Some((uuid, sid)) if sid.as_deref() == session => carried(ForkStart { resume_at: Some(uuid), ..Default::default() }),
            _ => ForkStart::default(),
        },
        // The thread's turns are counted from the log: only when it all is this thread.
        "codex" if same && !other_session => carried(ForkStart { keep_turns: Some(prompts), ..Default::default() }),
        _ => ForkStart::default(),
    };
    Ok(Some(ForkCut { len: cut, last_turn, start }))
}

/// The fork a resume must apply: the last `_puppeteer_fork` record when no
/// engine start follows it. `records` are the log's last records, oldest first.
pub fn pending(records: &[String]) -> Option<ForkStart> {
    for line in records.iter().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if init_session(&v).is_some() {
            return None;
        }
        if kind(&v) == Some(MARKER) {
            return serde_json::from_value(v).ok();
        }
    }
    None
}

/// What the resume that starts a fork tells its sidecar: copy the session it
/// resumes (`PUPPETEER_FORK_SESSION`) and where the copy ends. None when the
/// copy cannot end where it should — Claude's last kept entry is not in the
/// `transcript` it resumes — or the engine cannot fork: the fork then starts a
/// new session, and the source session is never resumed in place.
pub fn resume_env(engine: &str, start: &ForkStart, transcript: Option<&[u8]>) -> Option<Vec<(&'static str, String)>> {
    let end = match engine {
        "claude" => {
            let at = start.resume_at.as_deref()?;
            let quoted = format!("\"{at}\"");
            if !transcript.is_some_and(|t| String::from_utf8_lossy(t).contains(&quoted)) {
                return None;
            }
            ("PUPPETEER_RESUME_AT", at.to_string())
        }
        "codex" => ("PUPPETEER_FORK_KEEP_TURNS", start.keep_turns?.to_string()),
        _ => return None,
    };
    Some(vec![("PUPPETEER_FORK_SESSION", "1".to_string()), end])
}

/// The fork still pending at the end of the log at `path` (see [`pending`]).
fn pending_in(path: &Path) -> Option<ForkStart> {
    use std::io::{Seek, SeekFrom};
    let mut file = open_log(path, std::fs::OpenOptions::new().read(true)).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(64 << 10))).ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    let records: Vec<String> = String::from_utf8_lossy(&tail).lines().map(str::to_string).collect();
    pending(&records)
}

/// Write the fork's log: the source's first `len` bytes (all when None), then
/// the record its first resume applies. Never through a link, never over a file.
fn write_log(source: &Path, dest: &Path, len: Option<u64>, start: &ForkStart, from: &str) -> Result<(), String> {
    let src = open_log(source, std::fs::OpenOptions::new().read(true)).map_err(|e| e.to_string())?;
    let mut out = open_log(dest, std::fs::OpenOptions::new().write(true).create_new(true)).map_err(|e| e.to_string())?;
    // Streamed: a long session's log can be large.
    let mut kept = src.take(len.unwrap_or(u64::MAX));
    let mut last = 0u8;
    let mut buf = vec![0u8; 64 << 10];
    loop {
        let n = kept.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        last = buf[n - 1];
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    let mut record = serde_json::to_value(start).map_err(|e| e.to_string())?;
    record["type"] = Value::from(MARKER);
    record["from_run"] = Value::from(from);
    record["at"] = Value::from(chrono::Utc::now().to_rfc3339());
    let sep = if last != 0 && last != b'\n' { "\n" } else { "" };
    out.write_all(format!("{sep}{record}\n").as_bytes()).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct ForkOutcome {
    pub run: RunRecord,
    /// The engine conversation goes with the fork; false = it starts a new
    /// session (the history is still shown).
    pub conversation: bool,
}

#[derive(sqlx::FromRow)]
struct Source {
    project_id: String,
    repo_id: Option<String>,
    #[sqlx(rename = "type")]
    run_type: String,
    ref_number: Option<i64>,
    status: String,
    engine: String,
    model: Option<String>,
    input_path: Option<String>,
    output_path: Option<String>,
    session_id: Option<String>,
    transcript_path: Option<String>,
    claude_account_id: Option<String>,
    title: Option<String>,
    style: Option<String>,
    effort: Option<String>,
    system_instruction: Option<String>,
    framing_seeded: i64,
    role: Option<String>,
    conductor_run_id: Option<String>,
    deleted_at: Option<String>,
    project_path: String,
}

fn log_path(project: &str, run_id: &str) -> PathBuf {
    Path::new(project).join(".puppeteer").join("runs").join(format!("{run_id}.log"))
}

/// Fork `run_id` after the turn whose prompt record starts at byte `at_offset`
/// of its log (the whole conversation when None). Errors: 'run_active',
/// 'run_trashed', 'engine', 'fork_worker', 'not_started', 'no_log',
/// 'fork_point_unknown'.
pub(crate) async fn fork(db: &Db, registry: &RunRegistry, run_id: &str, at_offset: Option<u64>) -> Result<ForkOutcome, String> {
    let src: Source = sqlx::query_as(
        "SELECT r.project_id, r.repo_id, r.type, r.ref_number, r.status, r.engine, r.model, r.input_path, r.output_path,
                r.session_id, r.transcript_path, r.claude_account_id, r.title, r.style, r.effort, r.system_instruction,
                r.framing_seeded, r.role, r.conductor_run_id, r.deleted_at, p.path AS project_path
         FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
    )
    .bind(run_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    if src.status == "running" || registry.lock().await.contains_key(run_id) {
        return Err("run_active".into());
    }
    if src.deleted_at.is_some() {
        return Err("run_trashed".into());
    }
    if !matches!(src.engine.as_str(), "claude" | "codex") {
        return Err("engine".into());
    }
    // A worker runs in its conductor's worktree, which goes with the conductor.
    if src.role.as_deref() == Some("worker") || src.conductor_run_id.as_deref().is_some_and(|c| !c.is_empty()) {
        return Err("fork_worker".into());
    }
    if src.status == "fetched" {
        return Err("not_started".into());
    }

    let source_log = log_path(&src.project_path, run_id);
    let path = source_log.clone();
    let (engine, session, transcript) = (src.engine.clone(), src.session_id.clone(), src.transcript_path.clone());
    let (cut, transcript) = tokio::task::spawn_blocking(move || {
        let file = match open_log(&path, std::fs::OpenOptions::new().read(true)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err("no_log".to_string()),
            Err(e) => return Err(e.to_string()),
        };
        // A fork not started yet has no session of its own: a fork of it copies
        // the session that one will copy.
        let (session, transcript) = match session {
            Some(s) => (Some(s), transcript),
            None => pending_in(&path).map_or((None, None), |f| (f.session_id, f.transcript_path)),
        };
        let cut = analyze(BufReader::new(file), &engine, session.as_deref(), at_offset).map_err(|e| e.to_string())?;
        Ok((cut, transcript))
    })
    .await
    .map_err(|e| e.to_string())??;
    let cut = cut.ok_or("fork_point_unknown")?;
    let mut start = cut.start;
    if start.session_id.is_some() {
        start.transcript_path = transcript;
    }
    let conversation = start.session_id.is_some();

    let new_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let new_log = log_path(&src.project_path, &new_id);
    let written = {
        let (source_log, new_log, from) = (source_log, new_log.clone(), run_id.to_string());
        tokio::task::spawn_blocking(move || write_log(&source_log, &new_log, cut.len, &start, &from))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
    };
    let title = src.title.as_deref().map(|t| format!("{t} (fork)"));
    let saved = match written {
        Ok(()) => save(db, &src, run_id, &new_id, &now, title.as_deref(), cut.len, cut.last_turn).await,
        Err(e) => Err(e),
    };
    if let Err(e) = saved {
        let _ = std::fs::remove_file(&new_log);
        return Err(e);
    }
    tracing::info!(event = "run_forked", run_id = %run_id, fork = %new_id, at_offset = ?at_offset, conversation);
    Ok(ForkOutcome {
        run: RunRecord {
            id: new_id,
            project_id: src.project_id,
            repo_id: src.repo_id,
            run_type: src.run_type,
            ref_number: src.ref_number,
            status: "done".to_string(),
            engine: src.engine,
            model: src.model,
            input_path: src.input_path,
            output_path: src.output_path,
            session_id: None,
            claude_account_id: src.claude_account_id,
            started_at: Some(now.clone()),
            finished_at: Some(now.clone()),
            created_at: now.clone(),
            last_activity_at: Some(now),
            title,
            pinned: false,
            protected: false,
            role: None,
            conductor_run_id: None,
            style: src.style,
            effort: src.effort,
            forked_from_run_id: Some(run_id.to_string()),
        },
        conversation,
    })
}

/// The fork's row, its MCP overrides, and the snapshots and bookmarks of the
/// kept turns (the log is copied byte for byte, so offsets and turns still
/// match; the fork's next turn is numbered after the kept ones).
#[allow(clippy::too_many_arguments)]
async fn save(
    db: &Db,
    src: &Source,
    from: &str,
    id: &str,
    now: &str,
    title: Option<&str>,
    len: Option<u64>,
    last_turn: Option<i64>,
) -> Result<(), String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO runs (id, project_id, repo_id, type, ref_number, status, engine, model, input_path, output_path,
                           claude_account_id, title, style, effort, system_instruction, framing_seeded,
                           started_at, finished_at, created_at, last_activity_at, forked_from_run_id, forked_at)
         VALUES (?, ?, ?, ?, ?, 'done', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(&src.project_id)
    .bind(&src.repo_id)
    .bind(&src.run_type)
    .bind(src.ref_number)
    .bind(&src.engine)
    .bind(&src.model)
    .bind(&src.input_path)
    .bind(&src.output_path)
    .bind(&src.claude_account_id)
    .bind(title)
    .bind(&src.style)
    .bind(&src.effort)
    .bind(&src.system_instruction)
    .bind(src.framing_seeded)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(from)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO run_mcp_overrides (run_id, server_key, enabled, updated_at)
         SELECT ?, server_key, enabled, ? FROM run_mcp_overrides WHERE run_id = ?",
    )
    .bind(id)
    .bind(now)
    .bind(from)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(turn) = last_turn {
        sqlx::query(
            "INSERT INTO run_snapshots (run_id, turn, created_at, workspace, status, reason, manifest_sha, file_count, total_bytes)
             SELECT ?, turn, created_at, workspace, status, reason, manifest_sha, file_count, total_bytes
             FROM run_snapshots WHERE run_id = ? AND turn <= ?",
        )
        .bind(id)
        .bind(from)
        .bind(turn)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        // Whatever the source kept, the fork's next turn must come after the kept ones.
        sqlx::query(
            "INSERT OR IGNORE INTO run_snapshots (run_id, turn, created_at, workspace, status)
             VALUES (?, ?, ?, '', 'pruned')",
        )
        .bind(id)
        .bind(turn)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    let marks: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT log_offset, side, label, created_at FROM run_bookmarks WHERE run_id = ? AND log_offset < ?",
    )
    .bind(from)
    .bind(len.map_or(i64::MAX, |n| n as i64))
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    for (offset, side, label, created_at) in marks {
        sqlx::query("INSERT INTO run_bookmarks (id, run_id, log_offset, side, label, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string())
            .bind(id)
            .bind(offset)
            .bind(side)
            .bind(label)
            .bind(created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fork_run(
    db: State<'_, Db>,
    registry: State<'_, RunRegistry>,
    run_id: String,
    at_offset: Option<u64>,
) -> Result<ForkOutcome, String> {
    let outcome = fork(db.inner(), registry.inner(), &run_id, at_offset).await?;
    crate::commands::session_search::refresh_run_later(db.inner(), &outcome.run.id);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt(text: &str, turn: Option<i64>) -> Value {
        let mut v = json!({ "type": "user", "message": { "role": "user", "content": text } });
        if let Some(t) = turn {
            v["turn"] = json!(t);
        }
        v
    }

    fn init(session: &str) -> Value {
        json!({ "type": "system", "subtype": "init", "session_id": session })
    }

    fn assistant(uuid: &str, session: &str) -> Value {
        json!({ "type": "assistant", "uuid": uuid, "session_id": session, "parent_tool_use_id": null,
                "message": { "content": [{ "type": "text", "text": "ok" }] } })
    }

    fn log(records: &[Value]) -> String {
        let mut out = String::from("[stderr] warming up\n");
        for r in records {
            out.push_str(&r.to_string());
            out.push('\n');
        }
        out
    }

    /// Byte offset of the line that starts with `record`.
    fn offset_of(text: &str, record: &Value) -> u64 {
        text.find(&record.to_string()).unwrap() as u64
    }

    fn claude_log() -> String {
        log(&[
            prompt("one", Some(1)),
            init("s1"),
            assistant("a1", "s1"),
            prompt("two", Some(2)),
            init("s1"),
            assistant("a2", "s1"),
            json!({ "type": "assistant", "uuid": "sub", "session_id": "s1", "parent_tool_use_id": "t" }),
            json!({ "type": "result", "uuid": "res2", "session_id": "s1" }),
            prompt("three", Some(3)),
            assistant("a3", "s1"),
        ])
    }

    fn cut(text: &str, engine: &str, session: Option<&str>, through: Option<u64>) -> Option<ForkCut> {
        analyze(text.as_bytes(), engine, session, through).unwrap()
    }

    #[test]
    fn claude_keeps_the_turn_and_pins_its_last_entry() {
        let text = claude_log();
        let two = offset_of(&text, &prompt("two", Some(2)));
        let c = cut(&text, "claude", Some("s1"), Some(two)).unwrap();
        // Cut where turn 3 starts; the subagent entry is not a fork point.
        assert_eq!(c.len, Some(offset_of(&text, &prompt("three", Some(3)))));
        assert_eq!(c.last_turn, Some(2));
        assert_eq!(c.start, ForkStart { session_id: Some("s1".into()), resume_at: Some("a2".into()), ..Default::default() });

        // The whole conversation: nothing cut, pinned at the very last entry.
        let all = cut(&text, "claude", Some("s1"), None).unwrap();
        assert_eq!((all.len, all.last_turn, all.start.resume_at.as_deref()), (None, Some(3), Some("a3")));
        // The last turn by its prompt is the whole log too.
        let three = offset_of(&text, &prompt("three", Some(3)));
        assert_eq!(cut(&text, "claude", Some("s1"), Some(three)).unwrap().len, None);
    }

    #[test]
    fn an_offset_that_starts_no_prompt_is_no_fork_point() {
        let text = claude_log();
        let a1 = offset_of(&text, &assistant("a1", "s1"));
        assert_eq!(cut(&text, "claude", Some("s1"), Some(a1)), None);
        assert_eq!(cut(&text, "claude", Some("s1"), Some(a1 + 1)), None);
        assert_eq!(cut(&text, "claude", Some("s1"), Some(1 << 40)), None);
    }

    #[test]
    fn codex_counts_the_kept_turns_of_its_one_thread() {
        let steer = json!({ "type": "user", "message": { "role": "user", "content": "use tabs" }, "steer": true });
        let text = log(&[
            prompt("one", Some(1)),
            init("th"),
            json!({ "type": "result" }),
            prompt("two", Some(2)),
            steer,
            json!({ "type": "result" }),
            prompt("[session] A worker you spawned has finished", Some(3)),
            json!({ "type": "result" }),
            prompt("four", Some(4)),
        ]);
        let wake = offset_of(&text, &prompt("[session] A worker you spawned has finished", Some(3)));
        let c = cut(&text, "codex", Some("th"), Some(wake)).unwrap();
        assert_eq!(c.start, ForkStart { session_id: Some("th".into()), keep_turns: Some(3), ..Default::default() });
        assert_eq!(cut(&text, "codex", Some("th"), None).unwrap().start.keep_turns, Some(4));
    }

    #[test]
    fn a_conversation_that_cannot_follow_starts_a_new_session() {
        // The kept part belongs to an engine session that was replaced since.
        let text = log(&[
            prompt("one", Some(1)),
            init("old"),
            assistant("a1", "old"),
            prompt("two", Some(2)),
            init("new"),
            assistant("a2", "new"),
        ]);
        let one = offset_of(&text, &prompt("one", Some(1)));
        assert_eq!(cut(&text, "claude", Some("new"), Some(one)).unwrap().start, ForkStart::default());
        // Claude in the current session still follows; Codex cannot count that thread's turns.
        assert_eq!(cut(&text, "claude", Some("new"), None).unwrap().start.resume_at.as_deref(), Some("a2"));
        assert_eq!(cut(&text, "codex", Some("new"), None).unwrap().start, ForkStart::default());
        // No session captured at all.
        assert_eq!(cut(&claude_log(), "claude", None, None).unwrap().start, ForkStart::default());
    }

    #[test]
    fn the_fork_is_pending_until_the_engine_starts() {
        let start = ForkStart { session_id: Some("s1".into()), resume_at: Some("a1".into()), ..Default::default() };
        let mut marker = serde_json::to_value(&start).unwrap();
        marker["type"] = json!(MARKER);
        let mut lines: Vec<String> = claude_log().lines().map(str::to_string).collect();
        assert_eq!(pending(&lines), None);
        lines.push(marker.to_string());
        lines.push("[stderr] --- forked session s1 ---".into());
        assert_eq!(pending(&lines), Some(start));
        lines.push(init("s2").to_string());
        assert_eq!(pending(&lines), None);
    }

    #[test]
    fn the_first_resume_forks_or_starts_afresh_never_in_place() {
        let claude = ForkStart { session_id: Some("s1".into()), resume_at: Some("a2".into()), ..Default::default() };
        let transcript = br#"{"uuid":"a1"}
{"uuid":"a2"}"#;
        assert_eq!(
            resume_env("claude", &claude, Some(transcript)),
            Some(vec![("PUPPETEER_FORK_SESSION", "1".to_string()), ("PUPPETEER_RESUME_AT", "a2".to_string())])
        );
        // The entry is gone from the transcript (or there is none): no fork.
        assert_eq!(resume_env("claude", &claude, Some(br#"{"uuid":"a22"}"#)), None);
        assert_eq!(resume_env("claude", &claude, None), None);
        let codex = ForkStart { session_id: Some("th".into()), keep_turns: Some(0), ..Default::default() };
        assert_eq!(
            resume_env("codex", &codex, None),
            Some(vec![("PUPPETEER_FORK_SESSION", "1".to_string()), ("PUPPETEER_FORK_KEEP_TURNS", "0".to_string())])
        );
        assert_eq!(resume_env("codex", &ForkStart::default(), None), None);
        assert_eq!(resume_env("claude", &ForkStart::default(), Some(transcript)), None);
        assert_eq!(resume_env("shell", &codex, None), None);
    }

    // ── fork() against a DB and a project folder ──

    struct Fixture {
        db: Db,
        registry: RunRegistry,
        dir: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn fixture(log_text: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("puppeteer-fork-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".puppeteer").join("runs")).unwrap();
        std::fs::write(log_path(dir.to_str().unwrap(), "src"), log_text).unwrap();
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', ?, 'c')")
            .bind(dir.to_str().unwrap())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, model, session_id, transcript_path, title, style, effort,
                               system_instruction, framing_seeded, created_at, finished_at, pinned, protected)
             VALUES ('src', 'p1', 'session', 'done', 'claude', 'claude-opus-4-8', 's1', '/t/s1.jsonl', 'Refactor', 'concise', 'xhigh',
                     'be brief', 1, 'c', 'f', 1, 1)",
        )
        .execute(&db)
        .await
        .unwrap();
        for sql in [
            "INSERT INTO run_mcp_overrides (run_id, server_key, enabled, updated_at) VALUES
                ('src', 'builtin:google', 0, 'c'), ('src', 'server:m1', 1, 'c')",
            "INSERT INTO run_snapshots (run_id, turn, created_at, workspace, status, manifest_sha, file_count, total_bytes) VALUES
                ('src', 1, 'c', '/w', 'ok', 'm1', 1, 1), ('src', 2, 'c', '/w', 'ok', 'm2', 1, 1), ('src', 3, 'c', '/w', 'ok', 'm3', 1, 1)",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        Fixture { db, registry: RunRegistry::default(), dir }
    }

    async fn row(db: &Db, id: &str) -> (String, Option<String>, Option<String>, Option<String>, Option<String>) {
        sqlx::query_as("SELECT status, session_id, forked_from_run_id, title, style FROM runs WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_fork_copies_the_settings_and_history_and_leaves_the_source_alone() {
        let text = claude_log();
        let f = fixture(&text).await;
        let two = offset_of(&text, &prompt("two", Some(2)));
        sqlx::query(
            "INSERT INTO run_bookmarks (id, run_id, log_offset, side, label, created_at) VALUES
                ('b1', 'src', ?, 'answer', 'two', 'c'), ('b2', 'src', ?, 'question', 'three', 'c')",
        )
        .bind(two as i64)
        .bind(offset_of(&text, &prompt("three", Some(3))) as i64)
        .execute(&f.db)
        .await
        .unwrap();
        let source_before = row(&f.db, "src").await;

        let out = fork(&f.db, &f.registry, "src", Some(two)).await.unwrap();
        let id = out.run.id.clone();
        assert!(out.conversation);
        assert_eq!(out.run.forked_from_run_id.as_deref(), Some("src"));
        assert_eq!(
            row(&f.db, &id).await,
            ("done".into(), None, Some("src".into()), Some("Refactor (fork)".into()), Some("concise".into()))
        );
        let (engine, model, effort, instruction, seeded, pinned, forked_at): (String, Option<String>, Option<String>, Option<String>, i64, i64, Option<String>) =
            sqlx::query_as("SELECT engine, model, effort, system_instruction, framing_seeded, pinned, forked_at FROM runs WHERE id = ?")
                .bind(&id)
                .fetch_one(&f.db)
                .await
                .unwrap();
        assert_eq!(
            (engine.as_str(), model.as_deref(), effort.as_deref(), instruction.as_deref(), seeded, pinned),
            ("claude", Some("claude-opus-4-8"), Some("xhigh"), Some("be brief"), 1, 0)
        );
        assert_eq!(out.run.effort.as_deref(), Some("xhigh"));
        assert!(forked_at.is_some());

        // MCP overrides, the kept turns' snapshots and bookmarks go with it.
        let overrides: Vec<(String, i64)> =
            sqlx::query_as("SELECT server_key, enabled FROM run_mcp_overrides WHERE run_id = ? ORDER BY server_key")
                .bind(&id)
                .fetch_all(&f.db)
                .await
                .unwrap();
        assert_eq!(overrides, vec![("builtin:google".into(), 0), ("server:m1".into(), 1)]);
        let turns: Vec<i64> = sqlx::query_scalar("SELECT turn FROM run_snapshots WHERE run_id = ? ORDER BY turn")
            .bind(&id)
            .fetch_all(&f.db)
            .await
            .unwrap();
        assert_eq!(turns, vec![1, 2]);
        let marks: Vec<(i64, String)> = sqlx::query_as("SELECT log_offset, side FROM run_bookmarks WHERE run_id = ?")
            .bind(&id)
            .fetch_all(&f.db)
            .await
            .unwrap();
        assert_eq!(marks, vec![(two as i64, "answer".into())]);

        // The log: the source's up to turn 3, then the pending fork.
        let copied = std::fs::read_to_string(log_path(f.dir.to_str().unwrap(), &id)).unwrap();
        let keep = offset_of(&text, &prompt("three", Some(3))) as usize;
        assert!(copied.starts_with(&text[..keep]));
        let lines: Vec<String> = copied.lines().map(str::to_string).collect();
        assert_eq!(
            pending(&lines),
            Some(ForkStart {
                session_id: Some("s1".into()),
                transcript_path: Some("/t/s1.jsonl".into()),
                resume_at: Some("a2".into()),
                keep_turns: None,
            })
        );

        // The source is as it was.
        assert_eq!(row(&f.db, "src").await, source_before);
        assert_eq!(std::fs::read_to_string(log_path(f.dir.to_str().unwrap(), "src")).unwrap(), text);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_mcp_overrides WHERE run_id = 'src'").fetch_one(&f.db).await.unwrap();
        assert_eq!(n, 2);

        // Deleting the source only drops the link.
        sqlx::query("DELETE FROM runs WHERE id = 'src'").execute(&f.db).await.unwrap();
        assert_eq!(row(&f.db, &id).await.2, None);
    }

    #[tokio::test]
    async fn a_whole_fork_and_the_cases_it_is_refused() {
        let text = claude_log();
        let f = fixture(&text).await;
        let whole = fork(&f.db, &f.registry, "src", None).await.unwrap();
        let copied = std::fs::read_to_string(log_path(f.dir.to_str().unwrap(), &whole.run.id)).unwrap();
        assert!(copied.starts_with(&text));
        let turns: i64 = sqlx::query_scalar("SELECT MAX(turn) FROM run_snapshots WHERE run_id = ?")
            .bind(&whole.run.id)
            .fetch_one(&f.db)
            .await
            .unwrap();
        assert_eq!(turns, 3);

        let a1 = offset_of(&text, &assistant("a1", "s1"));
        assert_eq!(fork(&f.db, &f.registry, "src", Some(a1)).await.unwrap_err(), "fork_point_unknown");
        for (set, err) in [
            ("status = 'running'", "run_active"),
            ("deleted_at = 'x'", "run_trashed"),
            ("engine = 'shell'", "engine"),
            ("conductor_run_id = 'boss'", "fork_worker"),
            ("status = 'fetched'", "not_started"),
        ] {
            sqlx::query(&format!("UPDATE runs SET {set} WHERE id = 'src'")).execute(&f.db).await.unwrap();
            assert_eq!(fork(&f.db, &f.registry, "src", None).await.unwrap_err(), err, "{set}");
            sqlx::query(
                "UPDATE runs SET status = 'done', deleted_at = NULL, engine = 'claude', conductor_run_id = NULL WHERE id = 'src'",
            )
            .execute(&f.db)
            .await
            .unwrap();
        }
        std::fs::remove_file(log_path(f.dir.to_str().unwrap(), "src")).unwrap();
        assert_eq!(fork(&f.db, &f.registry, "src", None).await.unwrap_err(), "no_log");
        // Refusals leave nothing behind: only the whole fork above was made.
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs").fetch_one(&f.db).await.unwrap();
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn deleting_the_source_keeps_its_transcript_for_a_fork_not_started_yet() {
        let f = fixture(&claude_log()).await;
        let (kept, gone) = (f.dir.join("s1.jsonl"), f.dir.join("s9.jsonl"));
        for t in [&kept, &gone] {
            std::fs::write(t, "{}\n").unwrap();
        }
        sqlx::query("UPDATE runs SET transcript_path = ? WHERE id = 'src'")
            .bind(kept.to_str().unwrap())
            .execute(&f.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, created_at)
             VALUES ('lone', 'p1', 'session', 'done', 'claude', 's9', ?, 'c')",
        )
        .bind(gone.to_str().unwrap())
        .execute(&f.db)
        .await
        .unwrap();
        let out = fork(&f.db, &f.registry, "src", None).await.unwrap();

        crate::commands::runs::delete_run_inner(&f.db, &f.registry, "src").await.unwrap();
        assert!(kept.exists(), "the fork copies it at its first resume");
        crate::commands::runs::delete_run_inner(&f.db, &f.registry, "lone").await.unwrap();
        assert!(!gone.exists());
        assert_eq!(row(&f.db, &out.run.id).await.2, None);
    }

    #[tokio::test]
    async fn a_fork_of_a_fork_not_started_yet_copies_the_same_session() {
        let text = claude_log();
        let f = fixture(&text).await;
        let first = fork(&f.db, &f.registry, "src", None).await.unwrap();
        let two = offset_of(&text, &prompt("two", Some(2)));
        let second = fork(&f.db, &f.registry, &first.run.id, Some(two)).await.unwrap();
        assert!(second.conversation);
        assert_eq!(second.run.title.as_deref(), Some("Refactor (fork) (fork)"));
        let copied = std::fs::read_to_string(log_path(f.dir.to_str().unwrap(), &second.run.id)).unwrap();
        let lines: Vec<String> = copied.lines().map(str::to_string).collect();
        let start = pending(&lines).unwrap();
        assert_eq!(
            (start.session_id.as_deref(), start.resume_at.as_deref(), start.transcript_path.as_deref()),
            (Some("s1"), Some("a2"), Some("/t/s1.jsonl"))
        );
    }

    #[tokio::test]
    async fn a_codex_fork_keeps_the_counted_turns() {
        let f = fixture(&claude_log()).await;
        let codex = log(&[prompt("one", Some(1)), init("th"), json!({ "type": "result" })]);
        std::fs::write(log_path(f.dir.to_str().unwrap(), "src"), &codex).unwrap();
        sqlx::query("UPDATE runs SET engine = 'codex', session_id = 'th' WHERE id = 'src'").execute(&f.db).await.unwrap();
        let out = fork(&f.db, &f.registry, "src", None).await.unwrap();
        let copied = std::fs::read_to_string(log_path(f.dir.to_str().unwrap(), &out.run.id)).unwrap();
        let lines: Vec<String> = copied.lines().map(str::to_string).collect();
        assert_eq!(pending(&lines).unwrap().keep_turns, Some(1));
    }
}
