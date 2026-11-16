//! Wall-clock marks for tool steps. A stream-json message has no timestamp, so
//! the drain (`runs::sidecar`) writes one small `_puppeteer_step_at` record next
//! to each message that starts or ends tool calls. The step list of the turn
//! status bar reads them back to show how long each step took, also after the
//! app restarts. The `_puppeteer_` prefix keeps engines from ever reading it, and
//! every log reader skips a record whose `type` it does not know.
//!
//! Claude and Codex share this shape: the Codex sidecar also writes
//! `tool_use` blocks in an `assistant` message and `tool_result` blocks in a
//! `user` message.

use serde_json::{json, Value};

/// The record `type` of a step mark.
pub const MARKER: &str = "_puppeteer_step_at";

fn block_ids(msg: &Value, block_type: &str, id_key: &str) -> Vec<String> {
    let Some(blocks) = msg.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some(block_type))
        .filter_map(|b| b.get(id_key).and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// The mark for `msg` seen at `at_ms` (epoch ms): `start` for an `assistant`
/// message with `tool_use` blocks, `end` for a `user` message with
/// `tool_result` blocks. `None` for every other message.
pub fn step_mark(msg: &Value, at_ms: i64) -> Option<Value> {
    let (phase, ids) = match msg.get("type").and_then(Value::as_str)? {
        "assistant" => ("start", block_ids(msg, "tool_use", "id")),
        "user" => ("end", block_ids(msg, "tool_result", "tool_use_id")),
        _ => return None,
    };
    if ids.is_empty() {
        return None;
    }
    Some(json!({ "type": MARKER, "phase": phase, "ids": ids, "at": at_ms }))
}

/// `step_mark` as one log line (with its newline).
pub fn step_mark_line(msg: &Value, at_ms: i64) -> Option<String> {
    step_mark(msg, at_ms).map(|m| format!("{}\n", m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_use_starts_and_tool_result_ends() {
        let start = json!({ "type": "assistant", "message": { "content": [
            { "type": "text", "text": "hi" },
            { "type": "tool_use", "id": "t1", "name": "Bash", "input": {} },
            { "type": "tool_use", "id": "t2", "name": "Read", "input": {} } ] } });
        let m = step_mark(&start, 1000).unwrap();
        assert_eq!(m["type"], MARKER);
        assert_eq!(m["phase"], "start");
        assert_eq!(m["ids"], json!(["t1", "t2"]));
        assert_eq!(m["at"], 1000);

        let end = json!({ "type": "user", "message": { "content": [
            { "type": "tool_result", "tool_use_id": "t1", "content": "ok" } ] } });
        let m = step_mark(&end, 2500).unwrap();
        assert_eq!(m["phase"], "end");
        assert_eq!(m["ids"], json!(["t1"]));
    }

    #[test]
    fn other_messages_have_no_mark() {
        let text_only = json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "x" }] } });
        assert!(step_mark(&text_only, 1).is_none());
        let prompt = json!({ "type": "user", "message": { "content": "hello" } });
        assert!(step_mark(&prompt, 1).is_none());
        assert!(step_mark(&json!({ "type": "result" }), 1).is_none());
        assert!(step_mark(&json!({ "type": "stream_event" }), 1).is_none());
        assert!(step_mark(&json!({ "type": MARKER, "ids": ["a"] }), 1).is_none());
    }

    #[test]
    fn the_line_is_one_json_record() {
        let end = json!({ "type": "user", "message": { "content": [
            { "type": "tool_result", "tool_use_id": "t1", "content": "ok" } ] } });
        let line = step_mark_line(&end, 5).unwrap();
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(v["at"], 5);
    }

    #[test]
    fn a_log_reader_skips_the_mark() {
        // The log readers (search, transcript export) only look at `user` /
        // `assistant` records; a mark must never look like one.
        let line = step_mark_line(
            &json!({ "type": "user", "message": { "content": [
                { "type": "tool_result", "tool_use_id": "t1", "content": "ok" } ] } }),
            9,
        )
        .unwrap();
        assert!(!line.contains("\"user\"") && !line.contains("\"assistant\""));
    }
}
