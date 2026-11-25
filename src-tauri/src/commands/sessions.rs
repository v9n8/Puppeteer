//! Mirroring externally-created Claude sessions into Puppeteer.
//!
//! Puppeteer spawns the Claude Agent SDK, which reads/writes its conversation
//! transcripts to the SAME store the `claude` CLI / VS Code extension use:
//! `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`. That's why sessions
//! started in Puppeteer show up in the CLI. This module does the reverse: it
//! discovers the transcripts living there for a project's working dir and mirrors
//! each as a Puppeteer `session` run — automatically, via `reconcile_claude_sessions`
//! (on project open) and the live `session_watcher` — so the user can read their
//! history and continue them via the normal resume path (`resume_run`). There is
//! no manual import step; the Codex counterpart works the same way.
//!
//! The transcript's `user` / `assistant` entries already carry the
//! `{ type, message }` shape Puppeteer's stream-json renderer expects, so mirroring
//! is mostly: filter to those lines, re-emit them into a `.puppeteer/runs/<id>.log`,
//! and create a run row pointing at the captured `session_id`.

use crate::db::Db;
use crate::runs::sidecar::insert_usage_from_result;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

/// Feature switch for mirroring the Claude transcript stores.
pub(crate) const CLAUDE_MIRROR: &str = "sessions.mirror_claude";

/// Encode an absolute path the way Claude Code names its per-project session
/// directory: every non-alphanumeric character becomes `-`
/// (e.g. `/Users/x/Tools/puppeteer` → `-Users-x-Tools-puppeteer`).
pub(crate) fn encode_project_dir(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub(crate) fn claude_sessions_dir(project_path: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let dir = Path::new(&home)
        .join(".claude")
        .join("projects")
        .join(encode_project_dir(project_path));
    dir.is_dir().then_some(dir)
}

/// All existing transcript dirs for a project's cwd: the global `~/.claude`
/// store plus every managed multi-account config dir (AC-011). External Claude
/// sessions may live under any account profile, so mirroring must scan them all.
pub(crate) async fn claude_sessions_dirs(
    db: &sqlx::SqlitePool,
    project_path: &str,
) -> Vec<PathBuf> {
    let encoded = encode_project_dir(project_path);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = claude_sessions_dir(project_path) {
        dirs.push(dir);
    }
    for cfg in crate::commands::claude_accounts::account_config_dirs(db).await {
        let dir = Path::new(&cfg).join("projects").join(&encoded);
        if dir.is_dir() {
            dirs.push(dir);
        }
    }
    dirs
}

/// Accumulated token usage parsed out of a transcript's assistant turns.
#[derive(Default)]
pub(crate) struct UsageTotals {
    pub input: i64,
    pub output: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
}

/// Everything `import`/sync needs out of a native Claude transcript in one pass.
pub(crate) struct ParsedTranscript {
    /// Puppeteer stream-json log: one `{ type, message }` line per user/assistant
    /// turn (sidechains skipped), which `parseStreamLog` renders directly.
    pub log: String,
    /// Resolved display title (ai-title, else first user message, else first
    /// slash command with arguments).
    pub title: String,
    /// The transcript's last `ai-title`, when it has one (`title` is then this).
    pub ai_title: Option<String>,
    pub model: Option<String>,
    /// Number of assistant turns — also the multiplier for usage.
    pub turn_count: i64,
    pub usage: UsageTotals,
}

/// Parse a native Claude transcript into the shape the live session-sync needs.
/// Returns `None` when the file holds no
/// conversation for `project_path` (e.g. an encoded-dir hash collision).
///
/// The SDK / `claude` CLI / VS Code extension all write the SAME transcript, so
/// this is the single reader Puppeteer uses to mirror externally-created or
/// externally-continued sessions back into its own run/log/usage model.
pub(crate) fn parse_transcript(file: &Path, project_path: &str) -> Option<ParsedTranscript> {
    let raw = fs::read_to_string(file).ok()?;

    let mut log = String::new();
    let mut ai_title: Option<String> = None;
    let mut first_message: Option<String> = None;
    let mut slash_title: Option<String> = None;
    let mut model: Option<String> = None;
    let mut turn_count = 0i64;
    let mut usage = UsageTotals::default();
    let mut cwd_matches = false;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !cwd_matches {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                cwd_matches = c == project_path;
            }
        }
        match v.get("type").and_then(|x| x.as_str()).unwrap_or("") {
            "ai-title" => {
                if let Some(s) = v.get("aiTitle").and_then(|x| x.as_str()) {
                    ai_title = Some(s.to_string());
                }
            }
            t @ ("user" | "assistant") => {
                if v.get("isSidechain").and_then(|x| x.as_bool()) == Some(true) {
                    continue;
                }
                let Some(msg) = v.get("message") else { continue };
                let stripped = if t == "user" { without_session_framing(msg) } else { None };
                let msg = stripped.as_ref().unwrap_or(msg);
                // Drop local-command noise: the `<local-command-caveat>` meta line
                // and the `<command-name>…</command-name>` wrapper Claude Code
                // writes for a bare slash command (e.g. `/usage`). On their own
                // these form a transcript with no real conversation, which must
                // not be mirrored as a phantom "session".
                if t == "user" {
                    if v.get("isMeta").and_then(|x| x.as_bool()) == Some(true) {
                        continue;
                    }
                    if let Some(tx) = message_text(msg) {
                        let head = tx.trim_start();
                        if head.starts_with("<command-name>")
                            || head.starts_with("<command-message>")
                            || head.starts_with("<local-command-")
                        {
                            if slash_title.is_none() {
                                slash_title = super::session_titles::slash_command_title(head);
                            }
                            continue;
                        }
                    }
                }
                if t == "user" && first_message.is_none() {
                    if let Some(tx) = message_text(msg) {
                        if !tx.trim().is_empty() {
                            first_message = Some(tx);
                        }
                    }
                }
                if t == "assistant" {
                    turn_count += 1;
                    if model.is_none() {
                        if let Some(m) = msg.get("model").and_then(|x| x.as_str()) {
                            model = Some(m.to_string());
                        }
                    }
                    if let Some(u) = msg.get("usage") {
                        let g = |k: &str| u.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                        usage.input += g("input_tokens");
                        usage.output += g("output_tokens");
                        usage.cache_creation += g("cache_creation_input_tokens");
                        usage.cache_read += g("cache_read_input_tokens");
                    }
                }
                let out = serde_json::json!({ "type": t, "message": msg });
                log.push_str(&serde_json::to_string(&out).unwrap_or_default());
                log.push('\n');
            }
            _ => {}
        }
    }

    // Require a real exchange: matching cwd, some logged content, and at least
    // one assistant turn. A transcript with only local-command / meta lines (no
    // assistant reply) is not a session worth mirroring.
    if !cwd_matches || log.is_empty() || turn_count == 0 {
        return None;
    }

    let ai_title = ai_title.filter(|t| !t.trim().is_empty()).map(|t| truncate(&t, TITLE_MAX_CHARS));
    let title = match &ai_title {
        Some(t) => t.clone(),
        None => title_from_first_message(first_message.as_deref().or(slash_title.as_deref())),
    };

    Some(ParsedTranscript {
        log,
        title,
        ai_title,
        model,
        turn_count,
        usage,
    })
}

/// Title of an imported session with no message to name it after.
pub(crate) const PLACEHOLDER_TITLE: &str = "Imported session";

/// Resolve a run title from the first user message: trimmed, truncated, with a
/// generic fallback. Shared with the Codex parser.
pub(crate) fn title_from_first_message(first: Option<&str>) -> String {
    let t = truncate(first.unwrap_or("").trim(), TITLE_MAX_CHARS);
    if t.is_empty() {
        PLACEHOLDER_TITLE.to_string()
    } else {
        t
    }
}

/// `msg` without the `<session-framing>` block Puppeteer put before a resumed
/// message (a changed response style, `runs::claude_prompt`): it is not the
/// user's words. `None` when the message has none. Images ride after the text,
/// so only the first text is checked.
fn without_session_framing(msg: &Value) -> Option<Value> {
    let text = message_text(msg)?;
    let kept = crate::conductor::strip_session_framing(&text);
    if kept.len() == text.len() {
        return None;
    }
    let mut msg = msg.clone();
    let slot = match msg.get_mut("content")? {
        Value::Array(blocks) => blocks
            .iter_mut()
            .find(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))?
            .get_mut("text")?,
        content => content,
    };
    *slot = Value::String(kept.to_string());
    Some(msg)
}

/// Pull the first text block (or plain-string content) out of a transcript
/// message. Returns None for messages with no human-readable text (e.g. a
/// user turn carrying only a `tool_result`).
fn message_text(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        for b in arr {
            if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// How much of the first message a stored run title keeps. Long enough that a
/// two-sentence prompt still reads as a sentence in the History list.
pub(crate) const TITLE_MAX_CHARS: usize = 200;

/// Collapse a prompt into a single line and cut it to `max` characters. The cut
/// falls back to the last word boundary so a title never ends mid-word.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let head: String = s.chars().take(max).collect();
    // Keep the whole window unless a word boundary sits reasonably close to the
    // end; cutting back further than that loses more than it gains.
    let cut = head
        .rfind(char::is_whitespace)
        .filter(|i| head[..*i].chars().count() * 4 >= max * 3)
        .map(|i| head[..i].to_string())
        .unwrap_or(head);
    format!("{}…", cut.trim_end())
}

fn file_modified_rfc3339(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        .unwrap_or_default()
}

/// Byte length of a file, used as the "how far have we mirrored this transcript"
/// marker. Transcripts are append-only, so a changed size means new content.
fn file_size(path: &Path) -> Option<i64> {
    fs::metadata(path).ok().map(|m| m.len() as i64)
}

/// Result of mirroring one transcript into Puppeteer's run/log/usage model.
pub(crate) enum SyncOutcome {
    /// A new `session` run row was created for an externally-started session.
    Imported(String),
    /// An existing run's on-disk log was refreshed from the transcript.
    Updated(String),
    /// Nothing to do — no conversation, or a live run owns this session, or a
    /// brand-new session was deferred to avoid racing a still-initializing run.
    Skipped,
}

/// Re-derive a run's token-usage ledger from its (full) transcript, replacing
/// any existing rows for the run. Idempotent: re-running on a grown transcript
/// always lands on the current total without double-counting. Cost is estimated
/// (the transcript carries no SDK `total_cost_usd`); `created_at` should be the
/// transcript mtime so the row sorts with the session.
///
/// For a run that originated in Puppeteer this *replaces* the live drain's accurate
/// per-turn cost rows with one estimated lump — the tradeoff for counting turns
/// that were added outside Puppeteer, whose real cost the transcript never recorded.
async fn sync_run_usage(
    db: &sqlx::SqlitePool,
    run_id: &str,
    project_id: &str,
    project_name: &str,
    engine: &str,
    parsed: &ParsedTranscript,
    created_at: &str,
) {
    let total = parsed.usage.input
        + parsed.usage.output
        + parsed.usage.cache_creation
        + parsed.usage.cache_read;
    // Nothing reliable to record. Leave any existing usage rows untouched rather
    // than wiping them for an empty / usage-less rebuild (a transcript can have
    // turns but no token counts) — otherwise a rebuild could strand the run with
    // no usage at all if the synthetic insert is skipped.
    if parsed.turn_count == 0 || total == 0 {
        return;
    }
    // Safe to clear now: the synthetic carries real tokens, so the insert below
    // is guaranteed to succeed (it only skips when total == 0 && cost == 0).
    let _ = sqlx::query("DELETE FROM run_usage WHERE run_id = ?")
        .bind(run_id)
        .execute(db)
        .await;
    // The synthetic lump replaces every earlier turn, so the next live turn
    // can't be differenced against the old baseline.
    crate::runs::model_usage::clear_baseline(db, run_id).await;
    let synthetic = serde_json::json!({
        "type": "result",
        "usage": {
            "input_tokens": parsed.usage.input,
            "output_tokens": parsed.usage.output,
            "cache_creation_input_tokens": parsed.usage.cache_creation,
            "cache_read_input_tokens": parsed.usage.cache_read,
        },
        "num_turns": parsed.turn_count,
    });
    let _ = insert_usage_from_result(
        db,
        run_id,
        project_id,
        project_name,
        engine,
        parsed.model.as_deref(),
        &synthetic,
        created_at,
    )
    .await;
}

/// Run a transcript parser on the blocking pool. A transcript can be tens of
/// MB; parsing it on an async worker thread stalled every other task on that
/// thread, DB queries included (RT-29). A parser that panics counts as
/// unparsable.
async fn parse_off_runtime(
    parse: fn(&Path, &str) -> Option<ParsedTranscript>,
    file: &Path,
    project_path: &str,
) -> Option<ParsedTranscript> {
    let (file, project_path) = (file.to_path_buf(), project_path.to_string());
    tokio::task::spawn_blocking(move || parse(&file, &project_path))
        .await
        .ok()
        .flatten()
}

/// Write a rebuilt run log (it can be several MB) without blocking the async
/// runtime (RT-29).
async fn write_log(runs_dir: &Path, log_path: &Path, log: &str) -> Result<(), String> {
    tokio::fs::create_dir_all(runs_dir).await.map_err(|e| e.to_string())?;
    tokio::fs::write(log_path, log).await.map_err(|e| e.to_string())
}

/// Mirror a single session transcript into Puppeteer. Engine-agnostic core shared by
/// Claude (`upsert_claude_session`) and Codex (`codex_sessions`). The caller
/// resolves the engine's transcript `file` (it must exist) and supplies the
/// engine's `parse` fn. This reconciles the shared transcript store with runs:
/// - existing run for the session → refresh its rendered log on disk;
/// - no run yet → import it as a `session` run (history + token usage);
/// - the transcript was written by two-way sync → skip (it is already mirrored);
/// - the session is owned by a *running* Puppeteer run → leave it to the live
///   stream; and if any run in the project is still `running` we defer importing
///   *new* sessions, since that run may just not have captured its `session_id`
///   yet (avoids creating a duplicate of a live run).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn upsert_session_run_core(
    db: &sqlx::SqlitePool,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    session_id: &str,
    engine: &str,
    file: &Path,
    parse: fn(&Path, &str) -> Option<ParsedTranscript>,
    defer_when_busy: bool,
    force: bool,
) -> Result<SyncOutcome, String> {
    use crate::runs::slow_ops::{over, SLOW_MIRROR_SYNC};
    let started = std::time::Instant::now();
    let outcome = upsert_session_run_once(
        db, project_id, project_name, project_path, session_id, engine, file, parse, defer_when_busy, force,
    )
    .await;
    // Diagnostics for pool exhaustion (RT-29): which sync ran long, and how long.
    if let Some(elapsed_ms) = over(started, SLOW_MIRROR_SYNC) {
        let (result, run_id) = match &outcome {
            Ok(SyncOutcome::Imported(id)) => ("imported", id.as_str()),
            Ok(SyncOutcome::Updated(id)) => ("updated", id.as_str()),
            Ok(SyncOutcome::Skipped) => ("skipped", ""),
            Err(_) => ("error", ""),
        };
        tracing::warn!(
            event = "mirror_sync_slow",
            run_id,
            session_id,
            engine,
            transcript = %file.display(),
            result,
            elapsed_ms,
        );
    }
    outcome
}

/// One pass of [`upsert_session_run_core`], without the timing.
#[allow(clippy::too_many_arguments)]
async fn upsert_session_run_once(
    db: &sqlx::SqlitePool,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    session_id: &str,
    engine: &str,
    file: &Path,
    parse: fn(&Path, &str) -> Option<ParsedTranscript>,
    // When true (the automatic watcher/reconcile paths), defer importing a
    // brand-new session while any run in the project is still `running`, since
    // that run may simply not have captured its `session_id` yet. `get_run_log`
    // passes false — it always targets an existing run, never a new import.
    defer_when_busy: bool,
    // Rebuild an existing run's log from `file` even when it did not grow
    // ("Re-sync", RT-16 `run_resync`). A running run is still left alone.
    force: bool,
) -> Result<SyncOutcome, String> {
    use sqlx::Row;

    let runs_dir = Path::new(project_path).join(".puppeteer").join("runs");

    // Was this session explicitly deleted? If so, never re-import it from the
    // lingering shared transcript — that's exactly the reappear-after-delete bug
    // the tombstone exists to prevent. A tombstoned session has no run row, so
    // this only ever short-circuits the "new session" import path below.
    let tombstoned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deleted_sessions WHERE project_id = ? AND session_id = ?",
    )
    .bind(project_id)
    .bind(session_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    if tombstoned > 0 {
        return Ok(SyncOutcome::Skipped);
    }

    // Existing run for this session?
    if let Some(row) =
        sqlx::query("SELECT id, status, transcript_synced_size, finished_at, title, title_auto FROM runs WHERE project_id = ? AND session_id = ?")
            .bind(project_id)
            .bind(session_id)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?
    {
        let id: String = row.get("id");
        let status: String = row.get("status");
        let synced_size: Option<i64> = row.get("transcript_synced_size");
        let finished_at: Option<String> = row.get("finished_at");
        let title: Option<String> = row.get("title");
        let title_auto: i64 = row.get("title_auto");
        // A live run streams its own updates; never touch its log from here.
        if status == "running" {
            return Ok(SyncOutcome::Skipped);
        }
        let conv_path = runs_dir.join(format!("{}.log", id));
        let cur_size = file_size(file);
        let newer = force || match synced_size {
            // We know exactly how many bytes we last mirrored. Since transcripts
            // are append-only, rebuild iff it grew — exact, so a short final tail
            // written just after a previous sync is never missed. A shorter file
            // is an older copy of the session (another store, RT-01): rebuilding
            // from it would drop turns the log already has.
            Some(s) => cur_size.is_some_and(|c| c > s),
            // Never mirrored from a transcript before — e.g. a Puppeteer-native run
            // whose richer log (result/cost summary, stderr) the drain wrote
            // directly. Keep the original guard so the engine's own finishing
            // write doesn't clobber it; a real external edit advances the
            // transcript past the margin, and from then on we track size exactly.
            None => {
                let mtime = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
                match (mtime(file), mtime(&conv_path)) {
                    (Some(n), Some(c)) => n > c + std::time::Duration::from_secs(5),
                    (Some(_), None) => true,
                    _ => false,
                }
            }
        };
        if !newer {
            // An "Imported session" that its transcript could not name at import
            // is read once more, for its title only.
            if !super::session_titles::needs_repair(title.as_deref(), title_auto) {
                return Ok(SyncOutcome::Skipped);
            }
            let parsed = parse_off_runtime(parse, file, project_path).await;
            let renamed = super::session_titles::repair_placeholder(db, &id, parsed.as_ref()).await;
            return Ok(if renamed { SyncOutcome::Updated(id) } else { SyncOutcome::Skipped });
        }
        let Some(parsed) = parse_off_runtime(parse, file, project_path).await else {
            return Ok(SyncOutcome::Skipped);
        };
        write_log(&runs_dir, &conv_path, &parsed.log).await?;
        // Re-tally usage from the now-larger transcript so stats track the turns
        // added outside Puppeteer. created_at = transcript mtime.
        let created_at = file_modified_rfc3339(file);
        let created_at = if created_at.is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            created_at
        };
        sync_run_usage(db, &id, project_id, project_name, engine, &parsed, &created_at).await;
        // Remember where the transcript lives and how far we've mirrored it.
        // The transcript grew, so the session WAS worked on — outside Puppeteer, in
        // `claude`/`codex` at a terminal. Stamp last_activity_at with the
        // transcript mtime; without it those turns are invisible to the History
        // order and the session stays buried at its original created_at.
        // Only when the transcript was written after Puppeteer's own last turn ended,
        // though: the tail a Puppeteer turn flushes right before its sidecar exits is
        // not user activity and must not reorder History.
        let external_work = finished_at
            .as_deref()
            .and_then(|f| chrono::DateTime::parse_from_rfc3339(f).ok())
            .zip(fs::metadata(file).and_then(|m| m.modified()).ok())
            .is_none_or(|(fin, mtime)| {
                mtime > std::time::SystemTime::from(fin) + std::time::Duration::from_secs(5)
            });
        let sql = if external_work {
            "UPDATE runs SET transcript_path = ?, transcript_synced_size = ?, last_activity_at = ? WHERE id = ?"
        } else {
            "UPDATE runs SET transcript_path = ?, transcript_synced_size = ?, \
             last_activity_at = COALESCE(last_activity_at, finished_at, started_at, created_at, ?) WHERE id = ?"
        };
        let _ = sqlx::query(sql)
            .bind(file.to_string_lossy().as_ref())
            .bind(cur_size)
            .bind(&created_at)
            .bind(&id)
            .execute(db)
            .await;
        super::session_titles::apply_synced_title(db, &id, title.as_deref(), title_auto, &parsed).await;
        crate::commands::session_search::refresh_run_later(db, &id);
        return Ok(SyncOutcome::Updated(id));
    }

    // New session. Defer if a run is mid-flight here (its session_id may be
    // pending) so the watcher doesn't import a duplicate of a still-initializing
    // run.
    if defer_when_busy {
        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE project_id = ? AND status = 'running'")
            .bind(project_id)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?;
        if running > 0 {
            return Ok(SyncOutcome::Skipped);
        }
    }

    let Some(parsed) = parse_off_runtime(parse, file, project_path).await else {
        return Ok(SyncOutcome::Skipped);
    };

    let new_id = Uuid::new_v4().to_string();
    let log_path = runs_dir.join(format!("{}.log", new_id));
    let output_path = log_path.to_string_lossy().to_string();
    let synced_size = file_size(file);

    // created_at = session mtime so it sorts naturally among runs; mark done so
    // the UI offers resume right away.
    let created_at = {
        let m = file_modified_rfc3339(file);
        if m.is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            m
        }
    };
    let finished_at = chrono::Utc::now().to_rfc3339();

    // `OR IGNORE` makes this idempotent against the unique session index: if the
    // watcher and the on-open reconcile race, the loser is silently rejected
    // (rows_affected == 0) and we treat it as a no-op — the winner already
    // imported the session.
    let res = sqlx::query(
        "INSERT OR IGNORE INTO runs (id, project_id, type, status, engine, session_id, output_path, transcript_path, transcript_synced_size, started_at, finished_at, created_at, last_activity_at, title, title_auto)
         VALUES (?, ?, 'session', 'done', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new_id)
    .bind(project_id)
    .bind(engine)
    .bind(session_id)
    .bind(&output_path)
    .bind(file.to_string_lossy().as_ref())
    .bind(synced_size)
    .bind(&created_at)
    .bind(&finished_at)
    .bind(&created_at)
    // last_activity_at = transcript mtime (not `finished_at`, which is merely
    // when Puppeteer noticed the session): the import must sort where the work
    // actually happened.
    .bind(&created_at)
    .bind(&parsed.title)
    // Not an ai-title: app-set, so a later sync or turn may still name it.
    .bind(i64::from(parsed.ai_title.is_none()))
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    if res.rows_affected() == 0 {
        return Ok(SyncOutcome::Skipped);
    }

    // Persist the converted history where get_run_log / resume_run expect it.
    write_log(&runs_dir, &log_path, &parsed.log).await?;

    // Record the session's accumulated token usage so the imported history shows
    // in stats. Cost is estimated (the transcript has no SDK total_cost_usd).
    sync_run_usage(db, &new_id, project_id, project_name, engine, &parsed, &created_at).await;
    crate::commands::session_search::refresh_run_later(db, &new_id);

    Ok(SyncOutcome::Imported(new_id))
}

/// Claude wrapper around [`upsert_session_run_core`]: resolves the native
/// `<session-id>.jsonl` transcript ([`pick_claude_transcript`]) and uses the
/// Claude parser. `changed` is the file the watcher saw change, if any.
pub(crate) async fn upsert_claude_session(
    db: &sqlx::SqlitePool,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    session_id: &str,
    changed: Option<&Path>,
    defer_when_busy: bool,
) -> Result<SyncOutcome, String> {
    let dirs = claude_sessions_dirs(db, project_path).await;
    upsert_claude_session_in(db, project_id, project_name, project_path, session_id, &dirs, changed, defer_when_busy)
        .await
}

/// [`upsert_claude_session`] over the given transcript `dirs`.
#[allow(clippy::too_many_arguments)]
async fn upsert_claude_session_in(
    db: &sqlx::SqlitePool,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    session_id: &str,
    dirs: &[PathBuf],
    changed: Option<&Path>,
    defer_when_busy: bool,
) -> Result<SyncOutcome, String> {
    let tracked: Option<String> =
        sqlx::query_scalar("SELECT transcript_path FROM runs WHERE project_id = ? AND session_id = ?")
            .bind(project_id)
            .bind(session_id)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?
            .flatten();
    let Some(file) = pick_claude_transcript(dirs, session_id, tracked.as_deref().map(Path::new), changed) else {
        return Ok(SyncOutcome::Skipped);
    };
    upsert_session_run_core(
        db,
        project_id,
        project_name,
        project_path,
        session_id,
        "claude",
        &file,
        parse_transcript,
        defer_when_busy,
        false,
    )
    .await
}

/// The copy of a Claude session's transcript to mirror (RT-01). One session can
/// live in several stores — `~/.claude` and an account profile an in-app resume
/// copied it into — and the copies diverge: the app writes one, the CLI the
/// other. The run follows `tracked` (its `transcript_path`, the copy it last
/// resumed or mirrored) unless another copy was written after it.
/// - `changed` (the file the watcher saw change): taken when it is the tracked
///   copy or newer than it; an older copy changing is ignored (`None`), so a
///   stale store never overwrites the log.
/// - no `changed`: the tracked copy, or the newest copy found in `dirs` when one
///   is newer than it (or nothing is tracked).
pub(crate) fn pick_claude_transcript(
    dirs: &[PathBuf],
    session_id: &str,
    tracked: Option<&Path>,
    changed: Option<&Path>,
) -> Option<PathBuf> {
    let mtime = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let tracked = tracked.filter(|p| p.is_file());
    if let Some(changed) = changed {
        if !changed.is_file() {
            return None;
        }
        return match tracked {
            Some(t) if t != changed && mtime(changed) <= mtime(t) => None,
            _ => Some(changed.to_path_buf()),
        };
    }
    let mut best = tracked.map(Path::to_path_buf);
    for candidate in dirs.iter().map(|d| d.join(format!("{session_id}.jsonl"))) {
        if !candidate.is_file() {
            continue;
        }
        match &best {
            Some(b) if mtime(&candidate) <= mtime(b) => {}
            _ => best = Some(candidate),
        }
    }
    best
}

/// Reconcile every Claude session stored for a project's working dir into Puppeteer
/// runs — importing externally-created ones and refreshing existing ones. Called
/// on project open (and by the live watcher) so the run list mirrors the shared
/// transcript store. Returns the number of runs imported or refreshed.
#[tauri::command]
pub async fn reconcile_claude_sessions(
    db: State<'_, Db>,
    project_id: String,
) -> Result<i64, String> {
    reconcile_claude(db.inner(), &project_id).await
}

/// [`reconcile_claude_sessions`]: 0 without reading the store while
/// [`CLAUDE_MIRROR`] is off.
async fn reconcile_claude(db: &Db, project_id: &str) -> Result<i64, String> {
    use sqlx::Row;

    if !crate::features::is_on(db, CLAUDE_MIRROR).await {
        return Ok(0);
    }
    let row = sqlx::query("SELECT name, path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    let project_name: String = row.get("name");
    let project_path: String = row.get("path");

    let dirs = claude_sessions_dirs(db, &project_path).await;
    if dirs.is_empty() {
        return Ok(0);
    }

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut changed = 0i64;
    for dir in &dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // A session id is unique across profiles; import from whichever dir
            // surfaces it first and skip duplicates in later roots.
            if !seen.insert(session_id.to_string()) {
                continue;
            }
            match upsert_claude_session(
                db,
                project_id,
                &project_name,
                &project_path,
                session_id,
                None,
                true,
            )
            .await
            {
                Ok(SyncOutcome::Imported(_)) | Ok(SyncOutcome::Updated(_)) => changed += 1,
                _ => {}
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("puppeteer_claude_test_{}_{}.jsonl", name, std::process::id()));
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    #[test]
    fn parse_transcript_extracts_turns_title_usage() {
        let content = concat!(
            r#"{"type":"user","cwd":"/proj","message":{"role":"user","content":"Hello there"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-x","content":[{"type":"text","text":"Hi"}],"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":2}}}"#,
            "\n",
        );
        let f = temp_file("basic", content);
        let p = parse_transcript(&f, "/proj").expect("should parse");
        assert_eq!(p.turn_count, 1);
        assert_eq!(p.model.as_deref(), Some("claude-x"));
        assert_eq!(p.usage.input, 10);
        assert_eq!(p.usage.output, 5);
        assert_eq!(p.usage.cache_read, 2);
        assert!(p.title.contains("Hello"));
        assert_eq!(p.log.lines().count(), 2);
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn parse_transcript_drops_a_style_seed_before_a_resumed_message() {
        let seed = crate::commands::response_styles::style_change_seed(None, Some(&crate::commands::response_styles::ResolvedStyle {
            key: "concise".into(),
            body: Some("Be short.".into()),
        }))
        .unwrap();
        let user = |content: Value| serde_json::json!({ "type": "user", "cwd": "/proj", "message": { "role": "user", "content": content } }).to_string();
        let content = [
            user(Value::String(format!("{seed}\n\nHello there"))),
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi"}]}}"#.to_string(),
            user(serde_json::json!([{ "type": "text", "text": format!("{seed}\n\nLook") }, { "type": "image", "source": {} }])),
        ]
        .join("\n");
        let f = temp_file("seed", &content);
        let p = parse_transcript(&f, "/proj").unwrap();
        assert!(!p.log.contains("session-framing"), "seed leaked: {}", p.log);
        assert!(p.log.contains(r#""content":"Hello there""#) && p.log.contains(r#""text":"Look""#));
        assert!(p.title.starts_with("Hello"));
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn truncate_collapses_lines_and_cuts_on_a_word_boundary() {
        assert_eq!(truncate("  hai  dòng\nprompt ", 40), "hai dòng prompt");
        // The 40-char window lands inside "handling"; the cut falls back to the
        // space before it rather than leaving a half word.
        let long = "Please refactor the session title logic so it keeps handling long prompts";
        assert_eq!(truncate(long, 40), "Please refactor the session title logic…");
        // No usable boundary (one long word): keep the full window.
        assert_eq!(truncate(&"a".repeat(50), 10), "aaaaaaaaaa…");
    }

    #[test]
    fn parse_transcript_rejects_wrong_cwd() {
        let content =
            "{\"type\":\"user\",\"cwd\":\"/other\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n";
        let f = temp_file("cwd", content);
        assert!(parse_transcript(&f, "/proj").is_none());
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn parse_transcript_prefers_ai_title_and_skips_sidechain() {
        let content = concat!(
            r#"{"type":"ai-title","aiTitle":"My Title"}"#,
            "\n",
            r#"{"type":"user","cwd":"/proj","isSidechain":true,"message":{"role":"user","content":"sidechain"}}"#,
            "\n",
            r#"{"type":"user","cwd":"/proj","message":{"role":"user","content":"real"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]}}"#,
            "\n",
        );
        let f = temp_file("title", content);
        let p = parse_transcript(&f, "/proj").unwrap();
        assert_eq!(p.title, "My Title");
        // The sidechain user line is skipped: only the real user + assistant logged.
        assert_eq!(p.log.lines().count(), 2);
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn parse_transcript_ignores_bare_slash_command_session() {
        // Byte-faithful copy of a real `/usage` transcript written by Claude Code
        // 2.1.179 (queue-operation + isMeta caveat + `<command-name>` wrapper +
        // system local_command stdout + last-prompt, NO assistant reply). This is
        // exactly the shape that produced the phantom "session lạ" runs. Must not
        // be mirrored as a session.
        let content = concat!(
            r#"{"type":"queue-operation","operation":"enqueue","sessionId":"s","content":"/usage"}"#,
            "\n",
            r#"{"type":"queue-operation","operation":"dequeue","sessionId":"s"}"#,
            "\n",
            r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"<local-command-caveat>Caveat: The messages below were generated by the user while running local commands. DO NOT respond to these messages or otherwise consider them in your response unless the user explicitly asks you to.</local-command-caveat>"},"isMeta":true,"cwd":"/proj"}"#,
            "\n",
            r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/usage</command-name>\n            <command-message>usage</command-message>\n            <command-args></command-args>"},"cwd":"/proj"}"#,
            "\n",
            r#"{"type":"system","subtype":"local_command","content":"<local-command-stdout>...usage...</local-command-stdout>","isMeta":false,"cwd":"/proj"}"#,
            "\n",
            r#"{"type":"last-prompt","sessionId":"s"}"#,
            "\n",
        );
        let f = temp_file("usage", content);
        assert!(parse_transcript(&f, "/proj").is_none());
        let _ = fs::remove_file(&f);
    }

    #[tokio::test]
    async fn reconcile_returns_zero_while_mirror_claude_is_off() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        // On, it looks the project up (and there is none).
        assert!(reconcile_claude(&db, "nope").await.is_err());
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, 'false')")
            .bind(format!("feature.{CLAUDE_MIRROR}"))
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(reconcile_claude(&db, "nope").await, Ok(0));
    }

    /// One user/assistant exchange of a transcript whose cwd is `cwd`.
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
    async fn the_watcher_follows_the_live_transcript_copy_and_never_drops_app_turns() {
        let root = std::env::temp_dir().join(format!("puppeteer-two-copies-{}", Uuid::new_v4()));
        let project = root.join("proj");
        let cwd = project.to_str().unwrap().to_string();
        let encoded = encode_project_dir(&cwd);
        let home = root.join("home").join(&encoded);
        let account = root.join("account").join(&encoded);
        for d in [&project.join(".puppeteer").join("runs"), &home, &account] {
            fs::create_dir_all(d).unwrap();
        }
        let home_copy = home.join("s1.jsonl");
        let account_copy = account.join("s1.jsonl");
        // Both copies share turn 1; the in-app turn only went to the account copy.
        fs::write(&home_copy, exchange(&cwd, "turn one", "ok one")).unwrap();
        let app_turns = exchange(&cwd, "turn one", "ok one") + &exchange(&cwd, "progress so far?", "halfway");
        fs::write(&account_copy, &app_turns).unwrap();
        set_age(&home_copy, 600);
        set_age(&account_copy, 300);

        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
            .bind(&cwd)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, transcript_synced_size, created_at)
             VALUES ('r1', 'p1', 'session', 'done', 'claude', 's1', ?, ?, 'c')",
        )
        .bind(account_copy.to_str().unwrap())
        .bind(app_turns.len() as i64)
        .execute(&db)
        .await
        .unwrap();
        let log = project.join(".puppeteer").join("runs").join("r1.log");
        fs::write(&log, "app log with progress so far?\n").unwrap();
        // `~/.claude` is searched first, as `claude_sessions_dirs` orders them.
        let dirs = [home.clone(), account.clone()];
        let sync = |changed: Option<PathBuf>| {
            let db = db.clone();
            let dirs = dirs.clone();
            let cwd = cwd.clone();
            async move {
                match upsert_claude_session_in(&db, "p1", "p", &cwd, "s1", &dirs, changed.as_deref(), true).await {
                    Ok(SyncOutcome::Updated(id)) => format!("updated {id}"),
                    Ok(SyncOutcome::Skipped) => "skipped".to_string(),
                    Ok(SyncOutcome::Imported(_)) => "imported".to_string(),
                    Err(e) => e,
                }
            }
        };
        let log_text = || fs::read_to_string(&log).unwrap();

        // The stale `~/.claude` copy changes: it must not overwrite the log.
        fs::write(&home_copy, exchange(&cwd, "turn one", "ok one") + "\n").unwrap();
        set_age(&home_copy, 500);
        assert_eq!(sync(Some(home_copy.clone())).await, "skipped");
        // Nor on the paths that name no file (project open, reading the log).
        assert_eq!(sync(None).await, "skipped");
        assert!(log_text().contains("progress so far?"));

        // The account copy (the one the run resumed) grows: the log follows it.
        let mut grown = app_turns.clone();
        grown.push_str(&exchange(&cwd, "next step", "done"));
        fs::write(&account_copy, &grown).unwrap();
        assert_eq!(sync(Some(account_copy.clone())).await, "updated r1");
        let text = log_text();
        assert!(text.contains("progress so far?") && text.contains("next step"), "{text}");

        // A newer copy that is shorter than what was mirrored is an older state
        // of the session: still no overwrite.
        set_age(&home_copy, 0);
        assert_eq!(sync(Some(home_copy.clone())).await, "skipped");
        assert!(log_text().contains("progress so far?"));
        let _ = fs::remove_dir_all(&root);
    }
}
