//! Rewind (`sessions.rewind`): take a stopped session back to just before one
//! of its prompts — the workspace files from that turn's snapshot
//! ([`super::snapshots`]), and the conversation when the engine can follow.
//!
//! Each prompt Puppeteer writes into the run log carries the snapshot `turn`
//! it was taken under, so a turn is found by its own log line, never by
//! counting. Taking the conversation back means cutting the log at that line
//! and telling the engine, at its next resume, where to pick up:
//!
//! - Claude: resume with `resumeSessionAt` = the last entry of the kept part
//!   (an `assistant` / `user` record's `uuid` in the log). The next prompt is
//!   chained there in the same session, and later resumes follow that branch.
//! - Codex: `thread/rollback` the number of turns cut, right after
//!   `thread/resume`. Rolled back for good.
//!
//! Neither engine can be told now (the run is stopped), so the cut is written
//! after the cut log as a `_puppeteer_rewind` record. It is pending until the
//! engine next starts on the session (a `system`/`init` record follows it):
//! only then is it consumed, so a resume that fails keeps it. A cut before the
//! session's first kept prompt starts a new session instead. When the log
//! holds turns of another engine session after the cut (the transcript was
//! lost and a new session started), the conversation cannot follow: only the
//! files are restored and the log is left as it is.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use super::snapshots::{self, FEATURE};
use super::RunRegistry;
use crate::db::Db;

pub const MARKER: &str = "_puppeteer_rewind";

/// Where the engine picks up after a rewind (the `_puppeteer_rewind` record).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCut {
    /// The session the cut applies to.
    pub session_id: Option<String>,
    /// Nothing of the session is kept: start a new one.
    pub fresh: bool,
    /// Claude: the last kept transcript entry.
    pub resume_at: Option<String>,
    /// Codex: turns to drop from the end of the thread.
    pub drop_turns: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct LogCut {
    /// Byte offset of the turn's prompt line: the log is cut there.
    offset: u64,
    conversation: Result<EngineCut, &'static str>,
}

fn kind(v: &Value) -> Option<&str> {
    v.get("type").and_then(Value::as_str)
}

fn init_session(v: &Value) -> Option<Option<String>> {
    (kind(v) == Some("system") && v.get("subtype").and_then(Value::as_str) == Some("init"))
        .then(|| v.get("session_id").and_then(Value::as_str).map(str::to_string))
}

/// A prompt Puppeteer handed the engine: a `user` record it wrote (no `uuid`,
/// unlike the engine's own records) that is neither a tool result nor one of
/// its own notes, nor a steer (added to a running turn, so no turn of its own).
fn is_prompt(v: &Value) -> bool {
    if kind(v) != Some("user") || v.get("uuid").is_some() || v.get("note").is_some() || v.get("steer").is_some() {
        return false;
    }
    match v.pointer("/message/content") {
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(blocks)) => {
            !blocks.is_empty() && blocks.iter().all(|b| b.get("type").and_then(Value::as_str) != Some("tool_result"))
        }
        _ => false,
    }
}

/// A transcript entry of the main conversation (not a subagent's), which a
/// Claude resume can be cut after.
fn chain_entry(v: &Value) -> Option<(&str, Option<&str>)> {
    if !matches!(kind(v), Some("assistant") | Some("user")) {
        return None;
    }
    if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
        return None;
    }
    let uuid = v.get("uuid").and_then(Value::as_str)?;
    Some((uuid, v.get("session_id").and_then(Value::as_str)))
}

/// Where to cut `log` (JSON records, one per line) to go back before `turn`,
/// and what the engine must do. None when no prompt line carries that turn.
fn analyze(log: impl BufRead, engine: &str, session: Option<&str>, turn: i64) -> std::io::Result<Option<LogCut>> {
    let mut offset = 0u64;
    let mut cut: Option<u64> = None;
    let mut init_before: Option<Option<String>> = None;
    let mut last_entry: Option<(String, Option<String>)> = None;
    let mut other_session_after = false;
    let mut prompts_after = 0u32;
    // A cut still pending at the end of the log (no engine start since): the
    // turns it drops are still in the engine and go with this cut too.
    let mut pending: Option<(u32, EngineCut)> = None;

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
        if cut.is_none() {
            if is_prompt(&v) && v.get("turn").and_then(Value::as_i64) == Some(turn) {
                cut = Some(at);
                prompts_after = 1;
            } else if let Some(s) = init_session(&v) {
                init_before = Some(s);
            } else if let Some((uuid, sid)) = chain_entry(&v) {
                last_entry = Some((uuid.to_string(), sid.map(str::to_string)));
            }
            continue;
        }
        if let Some(s) = init_session(&v) {
            pending = None;
            if s.as_deref() != session {
                other_session_after = true;
            }
        } else if kind(&v) == Some(MARKER) {
            if let Ok(marker) = serde_json::from_value::<EngineCut>(v.clone()) {
                pending = Some((prompts_after, marker));
            }
        } else if is_prompt(&v) {
            prompts_after += 1;
        }
    }
    let Some(offset) = cut else {
        return Ok(None);
    };
    // Prompts after a pending cut never reached the engine: it had not started.
    let drop_turns = match &pending {
        Some((before_marker, marker)) => before_marker + marker.drop_turns,
        None => prompts_after,
    };
    let conversation = if !matches!(engine, "claude" | "codex") {
        Err("engine")
    } else if other_session_after {
        Err("session_changed")
    } else {
        let fresh = EngineCut { session_id: session.map(str::to_string), fresh: true, resume_at: None, drop_turns };
        let same = session.is_some() && init_before.as_ref().is_some_and(|s| s.as_deref() == session);
        if !same {
            Ok(fresh)
        } else if engine == "codex" {
            Ok(EngineCut { fresh: false, ..fresh })
        } else {
            match last_entry {
                Some((uuid, sid)) if sid.as_deref() == session => {
                    Ok(EngineCut { fresh: false, resume_at: Some(uuid), ..fresh })
                }
                _ => Ok(fresh),
            }
        }
    };
    Ok(Some(LogCut { offset, conversation }))
}

/// The cut a resume must apply: the last `_puppeteer_rewind` record when no
/// engine start follows it. `records` are the log's last records, oldest first.
pub fn pending(records: &[String]) -> Option<EngineCut> {
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

/// Open the run log, never through a link: it lies in the project, where the
/// agent writes, so a link there could point the rewind at any file.
fn open_log(path: &Path, options: &mut std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

/// Cut the log at `offset` and record the pending engine cut after it.
fn cut_log(path: &Path, offset: u64, cut: &EngineCut, turn: i64) -> Result<(), String> {
    use std::io::Write;
    let mut record = serde_json::to_value(cut).map_err(|e| e.to_string())?;
    record["type"] = Value::from(MARKER);
    record["turn"] = Value::from(turn);
    record["at"] = Value::from(chrono::Utc::now().to_rfc3339());
    let f = open_log(path, std::fs::OpenOptions::new().write(true)).map_err(|e| e.to_string())?;
    f.set_len(offset).map_err(|e| e.to_string())?;
    drop(f);
    let mut f = open_log(path, std::fs::OpenOptions::new().append(true)).map_err(|e| e.to_string())?;
    f.write_all(format!("{record}\n").as_bytes()).map_err(|e| e.to_string())
}

struct RunFacts {
    engine: String,
    session_id: Option<String>,
    log: PathBuf,
    /// When the run last stopped: files written after it were not its doing.
    stopped: Option<SystemTime>,
}

async fn run_facts(db: &Db, registry: &RunRegistry, run_id: &str) -> Result<RunFacts, String> {
    crate::features::require(db, FEATURE).await?;
    let (engine, status, session_id, finished_at, project): (String, String, Option<String>, Option<String>, String) =
        sqlx::query_as(
            "SELECT r.engine, r.status, r.session_id, r.finished_at, p.path
             FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
        )
        .bind(run_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    if status == "running" || registry.lock().await.contains_key(run_id) {
        return Err("run_active".into());
    }
    let stopped = finished_at
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(SystemTime::from);
    let log = Path::new(&project).join(".puppeteer").join("runs").join(format!("{run_id}.log"));
    Ok(RunFacts { engine, session_id, log, stopped })
}

/// Another session still running in the same folder would race the restore.
async fn ensure_folder_idle(db: &Db, registry: &RunRegistry, run_id: &str, turn: i64) -> Result<(), String> {
    let workspace = snapshots::workspace_of_turn(db, run_id, turn).await?;
    let active: Vec<String> = registry.lock().await.keys().cloned().collect();
    for other in active.iter().filter(|id| *id != run_id) {
        let same: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM run_snapshots WHERE run_id = ? AND workspace = ? LIMIT 1")
                .bind(other)
                .bind(workspace.to_string_lossy().as_ref())
                .fetch_optional(db)
                .await
                .map_err(|e| e.to_string())?;
        if same.is_some() {
            return Err("folder_busy".into());
        }
    }
    Ok(())
}

async fn log_cut(facts: &RunFacts, turn: i64) -> Result<Option<LogCut>, String> {
    let (path, engine, session) = (facts.log.clone(), facts.engine.clone(), facts.session_id.clone());
    tokio::task::spawn_blocking(move || {
        let file = match open_log(&path, std::fs::OpenOptions::new().read(true)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        analyze(BufReader::new(file), &engine, session.as_deref(), turn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether the conversation goes back too, and if not why: 'engine',
/// 'session_changed', or 'not_in_log' (the prompt's line was not found).
fn conversation_of(cut: &Option<LogCut>) -> (bool, Option<&'static str>) {
    match cut {
        Some(LogCut { conversation: Ok(_), .. }) => (true, None),
        Some(LogCut { conversation: Err(why), .. }) => (false, Some(why)),
        None => (false, Some("not_in_log")),
    }
}

fn fingerprint(files: &str, conversation: bool, cut: &Option<LogCut>) -> String {
    use sha2::Digest;
    let offset = cut.as_ref().map_or(-1, |c| c.offset as i64);
    hex::encode(sha2::Sha256::digest(format!("{files}\n{conversation}\n{offset}")))
}

#[derive(Debug, Serialize)]
pub struct RewindPreview {
    pub turn: i64,
    pub created_at: Option<String>,
    pub files: snapshots::RestorePlan,
    pub conversation: bool,
    pub conversation_reason: Option<&'static str>,
    /// Hand back to [`rewind_run`]: it refuses when anything changed since.
    pub fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct RewindOutcome {
    pub files: snapshots::Restored,
    pub conversation: bool,
}

#[tauri::command]
pub async fn list_snapshots(db: State<'_, Db>, run_id: String) -> Result<Vec<snapshots::SnapshotInfo>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    snapshots::of_run(db.inner(), &run_id).await
}

#[tauri::command]
pub async fn preview_rewind(
    db: State<'_, Db>,
    registry: State<'_, RunRegistry>,
    run_id: String,
    turn: i64,
) -> Result<RewindPreview, String> {
    let (db, registry) = (db.inner(), registry.inner());
    let facts = run_facts(db, registry, &run_id).await?;
    let files = snapshots::preview(db, &run_id, turn, facts.stopped).await?;
    let cut = log_cut(&facts, turn).await?;
    let (conversation, conversation_reason) = conversation_of(&cut);
    let created_at = snapshots::of_run(db, &run_id).await?.into_iter().find(|s| s.turn == turn).map(|s| s.created_at);
    Ok(RewindPreview {
        turn,
        created_at,
        fingerprint: fingerprint(&files.fingerprint, conversation, &cut),
        files,
        conversation,
        conversation_reason,
    })
}

/// Rewind as previewed: restore the files, then (when the engine can follow)
/// cut the log and leave the engine its cut for the next resume.
#[tauri::command]
pub async fn rewind_run(
    db: State<'_, Db>,
    registry: State<'_, RunRegistry>,
    run_id: String,
    turn: i64,
    fingerprint: String,
) -> Result<RewindOutcome, String> {
    let (db, registry) = (db.inner(), registry.inner());
    let facts = run_facts(db, registry, &run_id).await?;
    ensure_folder_idle(db, registry, &run_id, turn).await?;
    let cut = log_cut(&facts, turn).await?;
    let (conversation, _) = conversation_of(&cut);
    // The file part of the fingerprint is checked again by the restore itself,
    // under the snapshot lock.
    let files_now = snapshots::preview(db, &run_id, turn, facts.stopped).await?;
    if self::fingerprint(&files_now.fingerprint, conversation, &cut) != fingerprint {
        return Err("files_changed".into());
    }
    let files = snapshots::restore(db, &run_id, turn, facts.stopped, &files_now.fingerprint).await?;
    if let Some(LogCut { offset, conversation: Ok(engine_cut) }) = &cut {
        let (path, offset, engine_cut) = (facts.log.clone(), *offset, engine_cut.clone());
        tokio::task::spawn_blocking(move || cut_log(&path, offset, &engine_cut, turn))
            .await
            .map_err(|e| e.to_string())??;
        crate::commands::session_search::refresh_run_later(db, &run_id);
        // The cut is done: a failure here only leaves bookmarks the viewer no longer finds.
        if let Err(e) = super::bookmarks::drop_from(db, &run_id, offset).await {
            tracing::warn!(event = "run_bookmarks_drop_failed", run_id = %run_id, error = %e);
        }
    }
    tracing::info!(event = "run_rewound", run_id = %run_id, turn, conversation, restored = files.restored, deleted = files.deleted);
    Ok(RewindOutcome { files, conversation })
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

    fn tool_result(uuid: Option<&str>, session: &str) -> Value {
        let mut v = json!({ "type": "user", "session_id": session, "parent_tool_use_id": null,
                            "message": { "content": [{ "type": "tool_result", "tool_use_id": "t", "content": "x" }] } });
        if let Some(u) = uuid {
            v["uuid"] = json!(u);
        }
        v
    }

    fn log(records: &[Value]) -> String {
        let mut out = String::from("[stderr] warming up\n");
        for r in records {
            out.push_str(&r.to_string());
            out.push('\n');
        }
        out
    }

    fn cut(text: &str, engine: &str, session: Option<&str>, turn: i64) -> Option<LogCut> {
        analyze(text.as_bytes(), engine, session, turn).unwrap()
    }

    fn engine(c: &Option<LogCut>) -> Result<EngineCut, &'static str> {
        c.as_ref().unwrap().conversation.clone()
    }

    /// Three Claude turns in one session, a subagent entry and a tool result in turn 2.
    fn claude_log() -> String {
        log(&[
            prompt("one", Some(1)),
            init("s1"),
            assistant("a1", "s1"),
            prompt("two", Some(2)),
            init("s1"),
            assistant("a2", "s1"),
            json!({ "type": "assistant", "uuid": "sub", "session_id": "s1", "parent_tool_use_id": "t" }),
            tool_result(Some("r2"), "s1"),
            json!({ "type": "result", "uuid": "res2", "session_id": "s1" }),
            prompt("three", Some(3)),
            assistant("a3", "s1"),
        ])
    }

    #[test]
    fn claude_resumes_after_the_last_kept_entry_and_the_log_is_cut_at_the_prompt() {
        let text = claude_log();
        let c = cut(&text, "claude", Some("s1"), 3);
        let offset = c.as_ref().unwrap().offset as usize;
        assert!(text[offset..].starts_with(&prompt("three", Some(3)).to_string()));
        assert_eq!(
            engine(&c),
            Ok(EngineCut { session_id: Some("s1".into()), fresh: false, resume_at: Some("r2".into()), drop_turns: 1 })
        );
        // Turn 2: the cut keeps turn 1, whose last entry is a1.
        assert_eq!(engine(&cut(&text, "claude", Some("s1"), 2)).unwrap().resume_at.as_deref(), Some("a1"));
        // Turn 1: nothing of the session is kept.
        let first = engine(&cut(&text, "claude", Some("s1"), 1)).unwrap();
        assert!(first.fresh);
        assert_eq!(first.drop_turns, 3);
        // A turn no prompt line carries.
        assert_eq!(cut(&text, "claude", Some("s1"), 9), None);
    }

    #[test]
    fn codex_drops_every_prompt_from_the_cut_wakes_included() {
        let text = log(&[
            prompt("one", Some(1)),
            init("th"),
            tool_result(None, "th"),
            json!({ "type": "result" }),
            init("th"),
            prompt("two", Some(2)),
            json!({ "type": "result" }),
            prompt("[session] A worker you spawned has finished", Some(3)),
            json!({ "type": "result" }),
            prompt("four", None),
            json!({ "type": "user", "message": { "role": "user", "content": "Skipped 1 SSE server" }, "note": true }),
        ]);
        let c = engine(&cut(&text, "codex", Some("th"), 2)).unwrap();
        assert_eq!((c.fresh, c.drop_turns, c.resume_at), (false, 3, None));
    }

    #[test]
    fn a_steer_is_no_turn_to_drop_or_cut_at() {
        let steer = json!({ "type": "user", "message": { "role": "user", "content": "use tabs" }, "steer": true });
        let text = log(&[
            init("th"),
            prompt("one", Some(1)),
            prompt("two", Some(2)),
            steer.clone(),
            json!({ "type": "result" }),
            prompt("three", Some(3)),
            steer,
        ]);
        // Codex took the steers through turn/steer: only turns 2 and 3 exist to drop.
        assert_eq!(engine(&cut(&text, "codex", Some("th"), 2)).unwrap().drop_turns, 2);
        assert_eq!(engine(&cut(&text, "codex", Some("th"), 3)).unwrap().drop_turns, 1);
    }

    #[test]
    fn another_session_after_the_cut_means_files_only_and_an_unknown_engine_too() {
        let text = log(&[
            prompt("one", Some(1)),
            init("old"),
            assistant("a1", "old"),
            prompt("two", Some(2)),
            init("new"),
            assistant("a2", "new"),
        ]);
        assert_eq!(engine(&cut(&text, "claude", Some("new"), 1)), Err("session_changed"));
        // The new session began with turn 2: going back before it starts afresh.
        assert!(engine(&cut(&text, "claude", Some("new"), 2)).unwrap().fresh);
        assert_eq!(engine(&cut(&claude_log(), "shell", Some("s1"), 2)), Err("engine"));
    }

    #[test]
    fn a_second_rewind_before_the_first_was_applied_keeps_its_dropped_turns() {
        let first = EngineCut { session_id: Some("th".into()), fresh: false, resume_at: None, drop_turns: 2 };
        let mut marker = serde_json::to_value(&first).unwrap();
        marker["type"] = json!(MARKER);
        let text = log(&[
            init("th"),
            prompt("one", Some(1)),
            prompt("two", Some(2)),
            marker.clone(),
            // Sent after the cut, but the engine never started: not in the thread.
            prompt("three", Some(5)),
        ]);
        assert_eq!(engine(&cut(&text, "codex", Some("th"), 2)).unwrap().drop_turns, 1 + 2);
        // Once the engine started after it, the marker is spent.
        let applied = log(&[init("th"), prompt("one", Some(1)), prompt("two", Some(2)), marker, init("th"), prompt("x", Some(5))]);
        assert_eq!(engine(&cut(&applied, "codex", Some("th"), 2)).unwrap().drop_turns, 2);
    }

    #[test]
    fn a_cut_is_pending_until_the_engine_starts_again() {
        let path = std::env::temp_dir().join(format!("puppeteer-rewind-{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&path, claude_log()).unwrap();
        let c = cut(&claude_log(), "claude", Some("s1"), 2).unwrap();
        let engine_cut = c.conversation.unwrap();
        cut_log(&path, c.offset, &engine_cut, 2).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 5, "warm-up, prompt 1, init, a1, the marker: {text}");
        assert_eq!(pending(&lines), Some(engine_cut.clone()));

        let mut resumed = lines.clone();
        resumed.push("[stderr] --- resumed session s1 ---".into());
        assert_eq!(pending(&resumed), Some(engine_cut));
        resumed.push(init("s1").to_string());
        assert_eq!(pending(&resumed), None);
        assert_eq!(pending(&claude_log().lines().map(str::to_string).collect::<Vec<_>>()), None);
        let _ = std::fs::remove_file(path);
    }

    // The log sits in the project, where the agent writes: a link there must not
    // get the rewind to truncate (or read) a file elsewhere.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_linked_log_is_neither_read_nor_cut() {
        let dir = std::env::temp_dir().join(format!("puppeteer-rewind-link-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (target, log) = (dir.join("notes.txt"), dir.join("r1.log"));
        std::fs::write(&target, claude_log()).unwrap();
        std::os::unix::fs::symlink(&target, &log).unwrap();
        let facts = RunFacts { engine: "claude".into(), session_id: Some("s1".into()), log: log.clone(), stopped: None };
        assert!(log_cut(&facts, 2).await.is_err());
        let c = cut(&claude_log(), "claude", Some("s1"), 2).unwrap();
        assert!(cut_log(&log, c.offset, &c.conversation.unwrap(), 2).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), claude_log());
        let _ = std::fs::remove_dir_all(dir);
    }
}
