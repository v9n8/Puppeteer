//! Mirroring Codex sessions into Puppeteer — the Codex counterpart of [`sessions`].
//!
//! `codex app-server` (spawned by Puppeteer), the `codex` CLI, and the Codex VS Code
//! extension all persist conversations to the SAME rollout store under
//! `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-<ts>-<thread-id>.jsonl`. Resuming a
//! thread (including across processes) APPENDS to the same file — verified — so a
//! rollout maps 1:1 to a Puppeteer run via the thread id Puppeteer stores as `session_id`.
//!
//! Unlike Claude (whose transcript already uses Puppeteer's `{type,message}` shape and
//! whose dir name IS the project), Codex needs:
//!   1. a translator from its rollout vocabulary into Puppeteer stream-json, and
//!   2. reading each file's `session_meta.cwd` to map it to a project (the tree is
//!      organized by date, not project).
//! Everything after parsing reuses the engine-agnostic core in [`sessions`].

use super::sessions::{
    upsert_session_run_core, ParsedTranscript, SyncOutcome, UsageTotals,
};
use crate::db::Db;
use chrono::TimeZone;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tauri::State;

/// Feature switch for mirroring the Codex rollout store.
pub(crate) const CODEX_MIRROR: &str = "sessions.mirror_codex";

/// `~/.codex/sessions`, the rollout store of the shared login.
pub(crate) fn codex_sessions_root() -> Option<PathBuf> {
    sessions_root_for(None).filter(|dir| dir.is_dir())
}

/// The rollout store of a login: `<home>/sessions` for a Codex account's home,
/// `~/.codex/sessions` for the shared login (`None`). It may not exist yet.
pub(crate) fn sessions_root_for(home: Option<&str>) -> Option<PathBuf> {
    match home.map(str::trim).filter(|h| !h.is_empty()) {
        Some(home) => Some(Path::new(home).join("sessions")),
        None => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex").join("sessions")),
    }
}

/// Every rollout store that exists: the shared `~/.codex` one, then each Codex
/// account's (a session runs under its account's `CODEX_HOME`).
pub(crate) async fn codex_sessions_roots(db: &Db) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = codex_sessions_root().into_iter().collect();
    for home in crate::commands::codex_accounts::account_homes(db).await {
        let dir = Path::new(&home).join("sessions");
        if dir.is_dir() {
            roots.push(dir);
        }
    }
    roots
}

/// Read just the first line of a rollout to get `(cwd, thread_id)` without
/// parsing the whole conversation — used to map a file to a project.
///
/// Returns `None` for subagent/forked rollouts. Codex's multi-agent mode
/// (`multi_agent_version: v2`, triggered e.g. by slash commands) spawns child
/// threads that each write their OWN rollout file with the SAME `cwd` as the
/// parent. Since Puppeteer maps a rollout to a project purely by `cwd`, importing
/// these forks would surface one conversation as several duplicate sessions
/// after a restart. Puppeteer only ever spawns top-level threads (`thread/start`,
/// `thread_source: "user"`), so skipping subagent/fork rollouts is safe.
pub(crate) fn rollout_meta(file: &Path) -> Option<(String, String)> {
    let v = read_session_meta(file)?;
    let p = v.get("payload")?;
    // Skip subagent spawns / forks — they belong to their parent thread, not a
    // standalone session.
    if p.get("thread_source").and_then(|x| x.as_str()) == Some("subagent")
        || p.get("parent_thread_id").is_some()
        || p.get("forked_from_id").is_some()
    {
        return None;
    }
    let cwd = p.get("cwd").and_then(|x| x.as_str())?.to_string();
    let id = p.get("id").and_then(|x| x.as_str())?.to_string();
    Some((cwd, id))
}

/// The rollout's first line when it is a `session_meta` record. Only that line
/// is needed, so stream it rather than reading the whole (possibly large) file.
fn read_session_meta(file: &Path) -> Option<Value> {
    let reader = BufReader::new(fs::File::open(file).ok()?);
    let first = reader
        .lines()
        .map_while(Result::ok)
        .find(|l| !l.trim().is_empty())?;
    let v: Value = serde_json::from_str(&first).ok()?;
    (v.get("type").and_then(|x| x.as_str()) == Some("session_meta")).then_some(v)
}

/// Every rollout belonging to a thread under `root`: its own rollout plus the
/// subagent/fork rollouts spawned from it, transitively (a subagent can spawn
/// its own). Used when a session is deleted so no child rollout is left behind.
/// A child in `keep` (the thread of another session, e.g. a fork of this one)
/// is left out, with its own children.
pub(crate) fn codex_thread_files(root: &Path, session_id: &str, keep: &std::collections::HashSet<String>) -> Vec<PathBuf> {
    let suffix = format!("-{}.jsonl", session_id);
    let mut owned = Vec::new();
    // (file, own id, parent id) for every child rollout.
    let mut children: Vec<(PathBuf, String, String)> = Vec::new();
    for file in all_rollout_files(root) {
        if file
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(&suffix))
        {
            owned.push(file);
            continue;
        }
        let Some(p) = read_session_meta(&file).and_then(|v| v.get("payload").cloned()) else {
            continue;
        };
        let parent = p
            .get("parent_thread_id")
            .or_else(|| p.get("forked_from_id"))
            .and_then(|x| x.as_str());
        let id = p.get("id").and_then(|x| x.as_str());
        if let (Some(parent), Some(id)) = (parent, id) {
            if !keep.contains(id) {
                children.push((file, id.to_string(), parent.to_string()));
            }
        }
    }

    let mut ids = std::collections::HashSet::from([session_id.to_string()]);
    loop {
        let before = children.len();
        let (hit, rest): (Vec<_>, Vec<_>) =
            children.into_iter().partition(|(_, _, parent)| ids.contains(parent));
        children = rest;
        for (file, id, _) in hit {
            ids.insert(id);
            owned.push(file);
        }
        if children.len() == before {
            break;
        }
    }
    owned
}

/// Walk the date-organized rollout tree and return every `rollout-*.jsonl` path.
fn all_rollout_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-"))
            {
                out.push(path);
            }
        }
    }
    out
}

/// Locate the rollout file for a thread id in any store. The id is the
/// filename suffix (`rollout-<ts>-<id>.jsonl`), so match on that rather than
/// reading every file.
pub(crate) async fn codex_session_file(db: &Db, session_id: &str) -> Option<PathBuf> {
    codex_session_file_in(&codex_sessions_roots(db).await, session_id)
}

/// [`codex_session_file`] over `roots`. A thread that switched accounts has a
/// copy of its rollout in each home it ran under; the newest is the live one.
pub(crate) fn codex_session_file_in(roots: &[PathBuf], session_id: &str) -> Option<PathBuf> {
    let suffix = format!("-{}.jsonl", session_id);
    roots
        .iter()
        .flat_map(|root| all_rollout_files(root))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .reduce(|newest, p| if is_newer(&p, &newest) { p } else { newest })
}

/// `a` was modified strictly after `b` (false when either mtime is unreadable).
fn is_newer(a: &Path, b: &Path) -> bool {
    let mtime = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    match (mtime(a), mtime(b)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Make thread `session_id` resumable from the rollout store `target`: Codex
/// finds a thread by its rollout under the active `CODEX_HOME`, so a session
/// that ran under another login gets a copy there, at the same dated path. A
/// copy already in `target` is refreshed when the live rollout (`live`, else
/// the newest in `roots`) is newer, or resuming it would drop the later turns.
/// Returns the rollout the resumed thread appends to; `None` when the thread
/// has no rollout anywhere.
pub(crate) fn mirror_rollout(
    session_id: &str,
    live: Option<&Path>,
    roots: &[PathBuf],
    target: &Path,
) -> Result<Option<PathBuf>, String> {
    let live = live
        .filter(|p| p.is_file())
        .map(Path::to_path_buf)
        .or_else(|| codex_session_file_in(roots, session_id));
    let copy = |src: &Path, dest: &Path| {
        fs::copy(src, dest)
            .map(|_| ())
            .map_err(|e| format!("Failed to copy Codex rollout from {} to {}: {e}", src.display(), dest.display()))
    };
    if let Some(dest) = codex_session_file_in(&[target.to_path_buf()], session_id) {
        if let Some(live) = live.filter(|l| *l != dest && is_newer(l, &dest)) {
            copy(&live, &dest)?;
        }
        return Ok(Some(dest));
    }
    let Some(src) = live else {
        return Ok(None);
    };
    let rel = roots
        .iter()
        .find_map(|root| src.strip_prefix(root).ok())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dated_path(&src));
    let dest = target.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    copy(&src, &dest)?;
    Ok(Some(dest))
}

/// `<YYYY>/<MM>/<DD>/<file name>`, the way Codex files a rollout named
/// `rollout-<YYYY>-<MM>-<DD>T…`; just the file name when it is not one.
fn dated_path(file: &Path) -> PathBuf {
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
        return PathBuf::new();
    };
    let date = name
        .strip_prefix("rollout-")
        .and_then(|rest| rest.get(..10))
        .filter(|d| {
            d.bytes()
                .enumerate()
                .all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
        });
    match date {
        Some(d) => Path::new(&d[..4]).join(&d[5..7]).join(&d[8..10]).join(name),
        None => PathBuf::from(name),
    }
}

/// Map a Codex `function_call` to a Puppeteer tool_use `(name, input)`. Shell execs
/// render as `Bash`; everything else (MCP tools, etc.) keeps its own name.
fn map_function_call(name: &str, args: &Value) -> (String, Value) {
    match name {
        "exec_command" | "shell" | "local_shell" => {
            let command = args
                .get("cmd")
                .or_else(|| args.get("command"))
                .cloned()
                .unwrap_or_else(|| args.clone());
            (
                "Bash".to_string(),
                serde_json::json!({ "command": command }),
            )
        }
        "apply_patch" | "edit" | "edit_file" => ("Edit".to_string(), args.clone()),
        other => (other.to_string(), args.clone()),
    }
}

/// Translate a Codex rollout into a Puppeteer [`ParsedTranscript`].
///
/// Text turns come from the clean `event_msg` user/agent messages; tool calls
/// from `response_item` `function_call`/`function_call_output`. Codex `reasoning`
/// is encrypted (no plaintext) so it is omitted. Token usage is the last
/// cumulative `token_count` (mapped like the live sidecar's `mapCodexUsage`).
pub(crate) fn parse_codex_rollout(file: &Path, project_path: &str) -> Option<ParsedTranscript> {
    let raw = fs::read_to_string(file).ok()?;

    let mut log = String::new();
    let mut first_user: Option<String> = None;
    let mut model: Option<String> = None;
    let mut turn_count = 0i64;
    let mut usage = UsageTotals::default();
    let mut cwd_matches = false;

    let mut push = |obj: Value| {
        log.push_str(&serde_json::to_string(&obj).unwrap_or_default());
        log.push('\n');
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = v.get("payload");
        // Capture cwd (project match) and a model id wherever they first appear.
        if let Some(p) = payload {
            if !cwd_matches {
                if let Some(c) = p.get("cwd").and_then(|x| x.as_str()) {
                    cwd_matches = c == project_path;
                }
            }
            if model.is_none() {
                if let Some(m) = p.get("model").and_then(|x| x.as_str()) {
                    model = Some(m.to_string());
                }
            }
        }

        match v.get("type").and_then(|x| x.as_str()).unwrap_or("") {
            "event_msg" => {
                let Some(p) = payload else { continue };
                match p.get("type").and_then(|x| x.as_str()).unwrap_or("") {
                    "user_message" => {
                        if let Some(text) = p.get("message").and_then(|x| x.as_str()) {
                            // Drop a one-time <session-framing> seed Puppeteer prepended
                            // on resume — it is system framing, not the user's words.
                            let text = crate::conductor::strip_session_framing(text).trim();
                            if !text.is_empty() {
                                if first_user.is_none() {
                                    first_user = Some(text.to_string());
                                }
                                push(serde_json::json!({
                                    "type": "user",
                                    "message": { "role": "user", "content": text },
                                }));
                            }
                        }
                    }
                    "agent_message" => {
                        if let Some(text) = p.get("message").and_then(|x| x.as_str()) {
                            let text = text.trim();
                            if !text.is_empty() {
                                turn_count += 1;
                                push(serde_json::json!({
                                    "type": "assistant",
                                    "message": { "role": "assistant",
                                        "content": [{ "type": "text", "text": text }] },
                                }));
                            }
                        }
                    }
                    "token_count" => {
                        // Cumulative session total; the last one wins.
                        if let Some(info) = p.get("info") {
                            let tot = info.get("total_token_usage").unwrap_or(info);
                            let g = |k: &str| tot.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                            let cached = g("cached_input_tokens");
                            let input_total = g("input_tokens");
                            usage = UsageTotals {
                                input: (input_total - cached).max(0),
                                output: g("output_tokens") + g("reasoning_output_tokens"),
                                cache_creation: 0,
                                cache_read: cached,
                            };
                        }
                    }
                    _ => {}
                }
            }
            "response_item" => {
                let Some(p) = payload else { continue };
                match p.get("type").and_then(|x| x.as_str()).unwrap_or("") {
                    "function_call" => {
                        let name = p.get("name").and_then(|x| x.as_str()).unwrap_or("tool");
                        let call_id = p.get("call_id").and_then(|x| x.as_str()).unwrap_or("");
                        // `arguments` is a JSON-encoded string.
                        let args: Value = p
                            .get("arguments")
                            .and_then(|x| x.as_str())
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(Value::Null);
                        let (tool_name, input) = map_function_call(name, &args);
                        push(serde_json::json!({
                            "type": "assistant",
                            "message": { "role": "assistant", "content": [
                                { "type": "tool_use", "id": call_id, "name": tool_name, "input": input }
                            ] },
                        }));
                    }
                    "function_call_output" => {
                        let call_id = p.get("call_id").and_then(|x| x.as_str()).unwrap_or("");
                        let output = p
                            .get("output")
                            .and_then(|x| x.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| p.get("output").map(|v| v.to_string()).unwrap_or_default());
                        // Best-effort error flag from the shell exit footer.
                        let is_error = output.contains("exited with code")
                            && !output.contains("exited with code 0");
                        push(serde_json::json!({
                            "type": "user",
                            "message": { "role": "user", "content": [
                                { "type": "tool_result", "tool_use_id": call_id,
                                  "content": output, "is_error": is_error }
                            ] },
                        }));
                    }
                    // message (duplicates event_msg text) and reasoning (encrypted)
                    // are intentionally skipped.
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if !cwd_matches || log.is_empty() {
        return None;
    }

    let title = super::sessions::title_from_first_message(first_user.as_deref());

    Some(ParsedTranscript {
        log,
        title,
        model,
        turn_count,
        usage,
    })
}

/// Codex upsert when the rollout path is already known (the watcher has it) —
/// avoids re-walking the whole `~/.codex/sessions` tree to locate the file.
pub(crate) async fn upsert_codex_session_file(
    db: &sqlx::SqlitePool,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    session_id: &str,
    file: &Path,
    defer_when_busy: bool,
) -> Result<SyncOutcome, String> {
    upsert_session_run_core(
        db,
        project_id,
        project_name,
        project_path,
        session_id,
        "codex",
        file,
        parse_codex_rollout,
        defer_when_busy,
    )
    .await
}

/// Reconcile every Codex rollout whose `cwd` matches a project into Puppeteer runs.
/// Scans the rollout tree once, reading only each file's first line to match the
/// project, then upserts the matching threads. Returns how many runs changed.
#[tauri::command]
pub async fn reconcile_codex_sessions(
    db: State<'_, Db>,
    project_id: String,
) -> Result<i64, String> {
    reconcile_codex(db.inner(), &project_id).await
}

/// [`reconcile_codex_sessions`]: 0 without reading the store while
/// [`CODEX_MIRROR`] is off.
async fn reconcile_codex(db: &Db, project_id: &str) -> Result<i64, String> {
    use sqlx::Row;

    if !crate::features::is_on(db, CODEX_MIRROR).await {
        return Ok(0);
    }
    let row = sqlx::query("SELECT name, path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    let project_name: String = row.get("name");
    let project_path: String = row.get("path");

    let roots = codex_sessions_roots(db).await;
    if roots.is_empty() {
        return Ok(0);
    }

    // A thread that switched accounts has a rollout in each home it ran
    // under: mirror the newest.
    let mut threads: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
    for file in roots.iter().flat_map(|root| all_rollout_files(root)) {
        let Some((cwd, thread_id)) = rollout_meta(&file) else {
            continue;
        };
        if cwd != project_path {
            continue;
        }
        match threads.entry(thread_id) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if is_newer(&file, e.get()) {
                    e.insert(file);
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(file);
            }
        }
    }

    let mut changed = 0i64;
    for (thread_id, file) in threads {
        match upsert_codex_session_file(
            db,
            project_id,
            &project_name,
            &project_path,
            &thread_id,
            &file,
            true,
        )
        .await
        {
            Ok(SyncOutcome::Imported(_)) | Ok(SyncOutcome::Updated(_)) => changed += 1,
            _ => {}
        }
    }

    // Populate the Codex plan-usage snapshot from the newest rollout so the
    // budget badge / settings panel show a % even before the first live run.
    // The snapshot is the shared login's, so only its store feeds it.
    if let Some(rl) = codex_sessions_root().and_then(|root| latest_codex_rate_limits_any(&root)) {
        persist_codex_plan_usage(db, &rl).await;
    }
    Ok(changed)
}

// ── Codex plan-usage (rate-limit) capture ─────────────────────────────────
//
// Codex exposes the account's plan rate-limits (the "% used" behind `/status`)
// via the app-server `account/rateLimits/read` RPC and writes the same data into
// each rollout's `token_count` events. Both shapes are normalized here into the
// SAME `plan_usage` snapshot the Claude path stores, under the settings key
// `plan_usage_codex`, so the badge / settings / budget logic are shared.

/// One RateLimitWindow field, tolerating both the app-server camelCase
/// (`usedPercent`, `windowDurationMins`, `resetsAt`) and the rollout snake_case
/// (`used_percent`, `window_minutes`, `resets_at`).
fn cx_num(w: &Value, camel: &str, snake: &str) -> Option<f64> {
    w.get(camel).and_then(|v| v.as_f64()).or_else(|| w.get(snake).and_then(|v| v.as_f64()))
}

/// Build a normalized plan-usage window `{ utilization, resets_at, status, status_at }`
/// from a codex RateLimitWindow, converting the unix-seconds reset to RFC3339.
fn cx_window(w: &Value) -> Value {
    let util = cx_num(w, "usedPercent", "used_percent");
    let resets_at = cx_num(w, "resetsAt", "resets_at")
        .and_then(|ts| chrono::Utc.timestamp_opt(ts as i64, 0).single())
        .map(|dt| dt.to_rfc3339());
    serde_json::json!({
        "utilization": util,
        "resets_at": resets_at,
        "status": null,
        "status_at": null,
    })
}

/// Convert a codex RateLimitSnapshot (`{ primary, secondary, planType, ... }`)
/// into the shared plan-usage snapshot. Windows are routed by their duration:
/// ≤ 1 day → the rolling 5h window, else the weekly window (codex reports them
/// as `primary`/`secondary` in no fixed order). Returns `None` when neither
/// window carries a usable `%`.
pub(crate) fn codex_rate_limits_to_snapshot(rl: &Value) -> Option<Value> {
    // Accept either the bare snapshot or the `{ rateLimits: {...} }` wrapper.
    let rl = rl.get("rateLimits").unwrap_or(rl);
    let mut five_hour = serde_json::json!({ "utilization": null, "resets_at": null, "status": null, "status_at": null });
    let mut seven_day = five_hour.clone();
    let mut have = false;

    for key in ["primary", "secondary"] {
        let Some(w) = rl.get(key).filter(|v| v.is_object()) else { continue };
        if cx_num(w, "usedPercent", "used_percent").is_none() {
            continue;
        }
        let mins = cx_num(w, "windowDurationMins", "window_minutes").unwrap_or(0.0);
        if mins > 0.0 && mins <= 1440.0 {
            five_hour = cx_window(w);
        } else {
            seven_day = cx_window(w);
        }
        have = true;
    }
    if !have {
        return None;
    }

    let plan_type = rl
        .get("planType")
        .or_else(|| rl.get("plan_type"))
        .and_then(|v| v.as_str());
    Some(serde_json::json!({
        "captured_at": chrono::Utc::now().to_rfc3339(),
        "subscription_type": plan_type,
        "rate_limits_available": true,
        "windows": {
            "five_hour": five_hour,
            "seven_day": seven_day,
            "seven_day_opus": { "utilization": null, "resets_at": null, "status": null, "status_at": null },
            "seven_day_sonnet": { "utilization": null, "resets_at": null, "status": null, "status_at": null },
        },
    }))
}

/// Scan a rollout for the LAST `token_count` event that carries `rate_limits`.
pub(crate) fn latest_codex_rate_limits(file: &Path) -> Option<Value> {
    let f = fs::File::open(file).ok()?;
    let mut latest: Option<Value> = None;
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() || !line.contains("rate_limits") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let p = v.get("payload").unwrap_or(&v);
        if p.get("type").and_then(|t| t.as_str()) != Some("token_count") {
            continue;
        }
        if let Some(rl) = p.get("rate_limits").filter(|x| x.is_object()) {
            latest = Some(rl.clone());
        }
    }
    latest
}

/// Newest rollout across the whole store that has usable rate-limits — used at
/// reconcile time to seed the snapshot regardless of which project it came from.
fn latest_codex_rate_limits_any(root: &Path) -> Option<Value> {
    let mut files = all_rollout_files(root);
    files.sort();
    for file in files.into_iter().rev() {
        if let Some(rl) = latest_codex_rate_limits(&file) {
            if codex_rate_limits_to_snapshot(&rl).is_some() {
                return Some(rl);
            }
        }
    }
    None
}

/// Persist a codex RateLimitSnapshot as the latest `plan_usage_codex` state.
/// No-op (returns false) when the snapshot has no usable window.
pub(crate) async fn persist_codex_plan_usage(db_pool: &sqlx::SqlitePool, rl: &Value) -> bool {
    let Some(snapshot) = codex_rate_limits_to_snapshot(rl) else {
        return false;
    };
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('plan_usage_codex', ?)")
        .bind(snapshot.to_string())
        .execute(db_pool)
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("puppeteer_codex_test_{}_{}.jsonl", name, std::process::id()));
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    const ROLLOUT: &str = concat!(
        r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-1","model":"gpt-x"}}"#,
        "\n",
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"Build me a thing"}}"#,
        "\n",
        r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Sure"}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"type":"function_call","name":"shell","call_id":"c1","arguments":"{\"command\":\"ls\"}"}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"file.txt"}}"#,
        "\n",
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#,
        "\n",
    );

    #[test]
    fn parse_codex_rollout_translates_turns_tools_usage() {
        let f = temp_file("basic", ROLLOUT);
        let p = parse_codex_rollout(&f, "/proj").expect("should parse");
        assert_eq!(p.turn_count, 1); // one agent_message
        assert_eq!(p.model.as_deref(), Some("gpt-x"));
        assert_eq!(p.usage.input, 80); // 100 input - 20 cached
        assert_eq!(p.usage.cache_read, 20);
        assert_eq!(p.usage.output, 35); // 30 output + 5 reasoning
        assert!(p.title.contains("Build"));
        // user + agent text + tool_use + tool_result = 4 log lines.
        assert_eq!(p.log.lines().count(), 4);
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn parse_codex_rollout_hides_session_framing_seed() {
        let rollout = concat!(
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-2"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"<session-framing>\nYou are a normal coding assistant session...\n</session-framing>\n\nFix the login bug"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Done"}}"#,
            "\n",
        );
        let f = temp_file("framing", rollout);
        let p = parse_codex_rollout(&f, "/proj").expect("should parse");
        assert!(!p.log.contains("session-framing"), "seed leaked: {}", p.log);
        assert!(p.log.contains("Fix the login bug"));
        assert_eq!(p.title, "Fix the login bug");
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn parse_codex_rollout_rejects_wrong_cwd() {
        let f = temp_file("cwd", ROLLOUT);
        assert!(parse_codex_rollout(&f, "/somewhere-else").is_none());
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn codex_rate_limits_routes_windows_by_duration() {
        // app-server camelCase shape: primary is the weekly window (10080 min),
        // secondary is the 5h window (300 min) — routed by duration, not name.
        let rl = serde_json::json!({
            "primary": { "usedPercent": 38.0, "windowDurationMins": 10080, "resetsAt": 1785213453i64 },
            "secondary": { "usedPercent": 12.0, "windowDurationMins": 300, "resetsAt": 1782373251i64 },
            "planType": "team",
        });
        let snap = codex_rate_limits_to_snapshot(&rl).expect("snapshot");
        assert_eq!(snap["subscription_type"], "team");
        assert_eq!(snap["rate_limits_available"], true);
        assert_eq!(snap["windows"]["five_hour"]["utilization"], 12.0);
        assert_eq!(snap["windows"]["seven_day"]["utilization"], 38.0);
        assert_eq!(snap["windows"]["seven_day_opus"]["utilization"], Value::Null);
        // resets_at converted from unix seconds to RFC3339.
        assert!(snap["windows"]["seven_day"]["resets_at"].as_str().unwrap().starts_with("20"));
    }

    #[test]
    fn codex_rate_limits_snake_case_and_null_secondary() {
        // Rollout snake_case shape with only a weekly window populated.
        let rl = serde_json::json!({
            "primary": { "used_percent": 77.0, "window_minutes": 10080, "resets_at": 1776707194i64 },
            "secondary": null,
            "plan_type": "free",
        });
        let snap = codex_rate_limits_to_snapshot(&rl).expect("snapshot");
        assert_eq!(snap["subscription_type"], "free");
        assert_eq!(snap["windows"]["seven_day"]["utilization"], 77.0);
        assert_eq!(snap["windows"]["five_hour"]["utilization"], Value::Null);
    }

    #[test]
    fn codex_rate_limits_empty_returns_none() {
        let rl = serde_json::json!({ "primary": null, "secondary": null, "planType": "pro" });
        assert!(codex_rate_limits_to_snapshot(&rl).is_none());
    }

    #[test]
    fn rollout_meta_reads_first_line_only() {
        let f = temp_file("meta", ROLLOUT);
        let (cwd, id) = rollout_meta(&f).expect("meta");
        assert_eq!(cwd, "/proj");
        assert_eq!(id, "thread-1");
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn rollout_meta_skips_subagent_and_fork_threads() {
        // A subagent spawn (multi-agent v2) shares the parent's cwd but must not
        // be imported as its own session.
        let subagent = concat!(
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-2","thread_source":"subagent","parent_thread_id":"thread-1","forked_from_id":"thread-1"}}"#,
            "\n",
        );
        let f = temp_file("subagent", subagent);
        assert!(rollout_meta(&f).is_none());
        let _ = fs::remove_file(&f);

        // A plain fork (parent_thread_id present, no explicit subagent source).
        let fork = concat!(
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-3","parent_thread_id":"thread-1"}}"#,
            "\n",
        );
        let f = temp_file("fork", fork);
        assert!(rollout_meta(&f).is_none());
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn codex_thread_files_collects_subagents_transitively() {
        let root = std::env::temp_dir().join(format!("puppeteer_codex_thread_{}", std::process::id()));
        let day = root.join("2026").join("10").join("17");
        fs::create_dir_all(&day).unwrap();
        let write = |name: &str, meta: &str| {
            let p = day.join(name);
            fs::write(&p, format!("{meta}\n")).unwrap();
            p
        };
        let main = write(
            "rollout-1-thread-1.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-1"}}"#,
        );
        let sub = write(
            "rollout-2-thread-2.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-2","thread_source":"subagent","parent_thread_id":"thread-1"}}"#,
        );
        // A subagent of the subagent, and a fork of the main thread.
        let nested = write(
            "rollout-3-thread-3.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-3","parent_thread_id":"thread-2"}}"#,
        );
        let fork = write(
            "rollout-4-thread-4.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-4","forked_from_id":"thread-1"}}"#,
        );
        let other = write(
            "rollout-5-thread-9.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-9"}}"#,
        );

        let mut got = codex_thread_files(&root, "thread-1", &Default::default());
        got.sort();
        let mut want = vec![main.clone(), sub.clone(), nested.clone(), fork];
        want.sort();
        assert_eq!(got, want);
        assert!(!got.contains(&other));
        // A fork that is a session of its own (UX-28) stays, with its children.
        let fork_sub = write(
            "rollout-6-thread-6.jsonl",
            r#"{"type":"session_meta","payload":{"cwd":"/proj","id":"thread-6","parent_thread_id":"thread-4"}}"#,
        );
        let mut got = codex_thread_files(&root, "thread-1", &["thread-4".to_string()].into_iter().collect());
        got.sort();
        let mut want = vec![main, sub, nested];
        want.sort();
        assert_eq!(got, want);
        assert!(!got.contains(&fork_sub));
        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn reconcile_returns_zero_while_mirror_codex_is_off() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        // On, it looks the project up (and there is none).
        assert!(reconcile_codex(&db, "nope").await.is_err());
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, 'false')")
            .bind(format!("feature.{CODEX_MIRROR}"))
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(reconcile_codex(&db, "nope").await, Ok(0));
    }

    fn store(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer_codex_store_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_rollout(root: &Path, thread: &str, body: &str, age_secs: u64) -> PathBuf {
        let file = root.join("2026/11/10").join(format!("rollout-2026-11-10T10-00-00-{thread}.jsonl"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, body).unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
        fs::File::options().write(true).open(&file).unwrap().set_modified(at).unwrap();
        file
    }

    #[test]
    fn the_newest_copy_of_a_thread_wins() {
        let (a, b) = (store("a"), store("b"));
        write_rollout(&a, "t1", "old", 60);
        let newer = write_rollout(&b, "t1", "new", 0);
        assert_eq!(codex_session_file_in(&[a.clone(), b.clone()], "t1"), Some(newer.clone()));
        assert_eq!(codex_session_file_in(&[b.clone(), a.clone()], "t1"), Some(newer));
        assert_eq!(codex_session_file_in(&[a.clone(), b.clone()], "t2"), None);
        let _ = (fs::remove_dir_all(a), fs::remove_dir_all(b));
    }

    #[test]
    fn a_rollout_is_copied_into_the_new_home_and_a_stale_copy_refreshed() {
        let (old, new) = (store("old"), store("new"));
        let src = write_rollout(&old, "t1", "a\n", 60);
        let roots = [old.clone(), new.clone()];

        // A thread with no rollout anywhere: nothing to resume.
        assert_eq!(mirror_rollout("t2", None, &roots, &new).unwrap(), None);

        // Copied at the same dated path, which is where Codex looks.
        let dest = mirror_rollout("t1", None, &roots, &new).unwrap().unwrap();
        assert_eq!(dest, new.join("2026/11/10").join(src.file_name().unwrap()));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "a\n");

        // The copy is the live one now: resuming there again copies nothing.
        fs::write(&dest, "a\nb\n").unwrap();
        assert_eq!(mirror_rollout("t1", Some(&dest), &roots, &new).unwrap(), Some(dest.clone()));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "a\nb\n");

        // Back on the old login, which went on meanwhile: its stale copy is refreshed.
        let back = mirror_rollout("t1", Some(&dest), &roots, &old).unwrap().unwrap();
        assert_eq!(back, src);
        assert_eq!(fs::read_to_string(&src).unwrap(), "a\nb\n");

        // A live rollout outside every store is filed by its date.
        let loose = store("loose").join("rollout-2026-11-09T08-00-00-t3.jsonl");
        fs::write(&loose, "x").unwrap();
        let dest = mirror_rollout("t3", Some(&loose), &roots, &new).unwrap().unwrap();
        assert_eq!(dest, new.join("2026/11/09/rollout-2026-11-09T08-00-00-t3.jsonl"));
        let _ = (fs::remove_dir_all(old), fs::remove_dir_all(new), fs::remove_dir_all(loose.parent().unwrap()));
    }

    #[test]
    fn dated_path_follows_the_rollout_name() {
        assert_eq!(
            dated_path(Path::new("/x/rollout-2026-01-02T03-04-05-id.jsonl")),
            PathBuf::from("2026/01/02/rollout-2026-01-02T03-04-05-id.jsonl")
        );
        assert_eq!(dated_path(Path::new("/x/rollout-latest-id.jsonl")), PathBuf::from("rollout-latest-id.jsonl"));
    }
}
