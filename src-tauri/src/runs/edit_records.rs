//! `get_run_edit_records` (V3-26): the file-editing tool calls of a whole run, WITH
//! their before / after text, for the Session's "Changed files" tab.
//!
//! `get_run_tool_records` cannot serve it: it drops `old_string`, `new_string` and
//! `content` so the payload stays small, and the +x −y counts need exactly those.
//! The frontend does the counting (`src/lib/editStats.ts`, the same function the
//! conversation uses per tool call), so this only filters and prunes.
//!
//! The log is already normalised: Codex `fileChange` / `apply_patch` items are
//! written as Claude-shaped `tool_use` blocks by the Codex sidecar, so both
//! engines look alike here.

use crate::commands::runs::{build_revision, fetch_run_log_row, RunLogRevision};
use crate::db::Db;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tauri::State;

/// A string longer than this is dropped from its call; the call's id goes to `truncated`.
const MAX_STRING_BYTES: usize = 1024 * 1024;

/// `input` keys the "Changed files" tab reads.
const INPUT_KEEP_KEYS: [&str; 10] = [
    "file_path", "notebook_path", "path", "old_string", "new_string", "edits", "content", "changes", "patch", "input",
];

#[derive(Debug, Serialize)]
pub struct RunEditRecords {
    /// Reduced log records: file-editing `tool_use`s, the `is_error` of their
    /// `tool_result`s, and a stub `user` record per prompt (so the frontend can number the turns).
    pub records: Vec<String>,
    pub revision: RunLogRevision,
    /// Ids of calls that had a string over 1 MB; their text is not in `records`.
    pub truncated: Vec<String>,
}

fn is_edit_tool(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "write" || n.contains("edit") || n.contains("patch")
}

/// Empty every string over the limit, anywhere in `v`. True when one was emptied.
fn drop_big_strings(v: &mut Value) -> bool {
    match v {
        Value::String(s) if s.len() > MAX_STRING_BYTES => {
            s.clear();
            true
        }
        Value::Array(a) => a.iter_mut().fold(false, |hit, x| drop_big_strings(x) | hit),
        Value::Object(o) => o.values_mut().fold(false, |hit, x| drop_big_strings(x) | hit),
        _ => false,
    }
}

/// A prompt typed by the person: a `user` record with text and no tool results,
/// not injected by the harness.
fn is_prompt(v: &Value, content: &[Value]) -> bool {
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) || v.get("isSynthetic").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    let has = |t: &str| content.iter().any(|b| b.get("type").and_then(Value::as_str) == Some(t));
    has("text") && !has("tool_result")
}

/// One log line reduced to what the tab needs; `None` when it has nothing for it.
/// Ids of calls whose big strings were dropped are pushed to `truncated`.
fn reduce_line(line: &str, truncated: &mut Vec<String>) -> Option<String> {
    // Cheap reject before a JSON parse: most records are assistant text.
    if !line.contains("tool_use") && !line.contains("tool_result") && !line.contains("\"user\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
    if ty != "assistant" && ty != "user" {
        return None;
    }
    let message = v.get("message")?;
    let content: Vec<Value> = match message.get("content")? {
        Value::Array(a) => a.clone(),
        // A plain-string prompt.
        Value::String(s) if ty == "user" && !s.is_empty() => vec![json!({ "type": "text", "text": "" })],
        _ => return None,
    };

    let mut kept: Vec<Value> = Vec::new();
    for block in &content {
        let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
        match (ty, btype) {
            ("assistant", "tool_use") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                if !is_edit_tool(name) {
                    continue;
                }
                let mut input = Map::new();
                if let Some(obj) = block.get("input").and_then(Value::as_object) {
                    for k in INPUT_KEEP_KEYS {
                        if let Some(x) = obj.get(k) {
                            input.insert(k.to_string(), x.clone());
                        }
                    }
                }
                let mut input = Value::Object(input);
                let id = block.get("id").cloned().unwrap_or(Value::Null);
                if drop_big_strings(&mut input) {
                    if let Some(id) = id.as_str() {
                        truncated.push(id.to_string());
                    }
                }
                kept.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
            }
            ("user", "tool_result") => kept.push(json!({
                "type": "tool_result",
                "tool_use_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                "is_error": block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                // Only `is_error` is read; real contents can be whole file reads.
                "content": "",
            })),
            _ => {}
        }
    }
    if ty == "user" && kept.is_empty() && is_prompt(&v, &content) {
        kept.push(json!({ "type": "text", "text": "." }));
    }
    if kept.is_empty() {
        return None;
    }
    Some(json!({ "type": ty, "message": { "content": kept } }).to_string())
}

/// Scan a whole log for the calls that edit files, keeping their text. Read-only,
/// so a Remote desktop may call it, un-audited (like `get_run_tool_records`).
#[tauri::command]
pub async fn get_run_edit_records(db: State<'_, Db>, run_id: String) -> Result<RunEditRecords, String> {
    let row = fetch_run_log_row(db.inner(), &run_id).await?;
    let revision = build_revision(&row, &run_id);
    let Some(path) = revision.path.as_ref().map(PathBuf::from) else {
        return Ok(RunEditRecords { records: Vec::new(), revision, truncated: Vec::new() });
    };

    let file = std::fs::File::open(&path).map_err(|e| format!("open log: {}", e))?;
    // The reader streams: the log is never held in memory as a whole.
    let reader = BufReader::with_capacity(256 * 1024, file);
    let mut records = Vec::new();
    let mut truncated = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if let Some(r) = reduce_line(&line, &mut truncated) {
            records.push(r);
        }
    }
    Ok(RunEditRecords { records, revision, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reduce(v: Value) -> (Option<Value>, Vec<String>) {
        let mut t = Vec::new();
        let r = reduce_line(&v.to_string(), &mut t).map(|s| serde_json::from_str(&s).unwrap());
        (r, t)
    }

    #[test]
    fn claude_edit_keeps_its_text() {
        let (r, t) = reduce(json!({ "type": "assistant", "message": { "content": [
            { "type": "text", "text": "ok" },
            { "type": "tool_use", "id": "t1", "name": "Edit",
              "input": { "file_path": "a.rs", "old_string": "x", "new_string": "y", "extra": 1 } },
            { "type": "tool_use", "id": "t2", "name": "Read", "input": { "file_path": "b.rs" } } ] } }));
        let blocks = r.unwrap()["message"]["content"].as_array().unwrap().clone();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["input"]["old_string"], "x");
        assert_eq!(blocks[0]["input"]["new_string"], "y");
        assert!(blocks[0]["input"].get("extra").is_none());
        assert!(t.is_empty());
    }

    #[test]
    fn write_multiedit_and_codex_calls_are_kept() {
        for (name, input) in [
            ("Write", json!({ "file_path": "a", "content": "body" })),
            ("MultiEdit", json!({ "file_path": "a", "edits": [{ "old_string": "a", "new_string": "b" }] })),
            ("Edit", json!({ "changes": [{ "path": "a", "kind": "add", "diff": "body" }] })),
            ("apply_patch", json!({ "input": "*** Begin Patch\n*** Add File: a\n+x\n*** End Patch" })),
        ] {
            let (r, _) = reduce(json!({ "type": "assistant", "message": { "content": [
                { "type": "tool_use", "id": "t", "name": name, "input": input } ] } }));
            assert!(r.is_some(), "{name}");
        }
    }

    #[test]
    fn tool_result_keeps_only_is_error() {
        let (r, _) = reduce(json!({ "type": "user", "message": { "content": [
            { "type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": "long file body" } ] } }));
        let b = r.unwrap()["message"]["content"][0].clone();
        assert_eq!(b["is_error"], true);
        assert_eq!(b["content"], "");
    }

    #[test]
    fn prompts_become_stubs_but_harness_messages_do_not() {
        let (r, _) = reduce(json!({ "type": "user", "message": { "content": "fix it" } }));
        assert!(r.is_some());
        let (r, _) = reduce(json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "fix it" }] } }));
        assert!(r.is_some());
        let (r, _) = reduce(json!({ "type": "user", "isMeta": true, "message": { "content": [{ "type": "text", "text": "skill" }] } }));
        assert!(r.is_none());
    }

    #[test]
    fn a_huge_string_is_dropped_and_its_call_listed() {
        let big = "x".repeat(MAX_STRING_BYTES + 1);
        let (r, t) = reduce(json!({ "type": "assistant", "message": { "content": [
            { "type": "tool_use", "id": "w1", "name": "Write", "input": { "file_path": "a", "content": big } } ] } }));
        let b = r.unwrap()["message"]["content"][0].clone();
        assert_eq!(b["input"]["content"], "");
        assert_eq!(b["input"]["file_path"], "a");
        assert_eq!(t, vec!["w1".to_string()]);
    }

    #[test]
    fn other_records_are_skipped() {
        let mut t = Vec::new();
        assert!(reduce_line(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#, &mut t).is_none());
        assert!(reduce_line("not json tool_use", &mut t).is_none());
    }
}
