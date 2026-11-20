//! Log records that code without an `AppHandle` pushes straight into a live
//! run's log buffer: the turn-cut mark (`turn_cut::push_live`, reached from
//! `cancel_run_inner`) and a conductor's worker result cards
//! (`conductor::worker_results`). The drain writes them to the file with the
//! rest of the buffer; it also hands each one to the open session on
//! `run:event:<id>` when it flushes, the way it emits step marks, so the
//! session shows them without reloading its log.
//!
//! A prompt the user sent (`send_prompt`) goes the same way, through a queue
//! of its own rather than a scan of the buffer: its log line is a plain
//! stream-json `user` message, like the engine's own. Every window that
//! follows the run gets it on `run:event:<id>`; the window that sent it
//! already shows it and drops the echo (`lib/promptEcho`).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tauri::{AppHandle, Emitter};

/// Logged prompt lines waiting for the drain to emit them, by run.
static PROMPTS: LazyLock<Mutex<HashMap<String, Vec<serde_json::Value>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Queue the log line of a prompt just handed to `run_id`'s engine for the
/// open windows. A line that is not JSON is skipped.
pub fn push_prompt(run_id: &str, log_line: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(log_line.trim_end()) else { return };
    PROMPTS.lock().unwrap_or_else(|e| e.into_inner()).entry(run_id.to_string()).or_default().push(v);
}

/// The queued prompt lines of `run_id`, in the order they were sent; each once.
fn take_prompts(run_id: &str) -> Vec<serde_json::Value> {
    PROMPTS.lock().unwrap_or_else(|e| e.into_inner()).remove(run_id).unwrap_or_default()
}

/// Emit the queued prompt lines of `run_id`. The drain calls it before every
/// stream event it emits, so a prompt shows ahead of the turn it starts.
pub fn emit_prompts(app: &AppHandle, run_id: &str) {
    for v in take_prompts(run_id) {
        let _ = app.emit(&format!("run:event:{run_id}"), v);
    }
}

/// The record types pushed from outside the drain. Everything else in the
/// buffer the drain wrote itself and has already emitted.
const TYPES: [&str; 2] = [super::turn_cut::MARKER, crate::conductor::worker_results::RECORD_TYPE];

/// How far into the run's log buffer the drain has looked.
#[derive(Default)]
pub struct LiveRecords {
    scanned: usize,
}

impl LiveRecords {
    /// Emit the records in the part of `buf` not looked at yet.
    pub fn emit_new(&mut self, app: &AppHandle, run_id: &str, buf: &str) {
        emit_prompts(app, run_id);
        for v in self.take(buf) {
            let _ = app.emit(&format!("run:event:{run_id}"), v);
        }
    }

    /// The records among the whole lines of `buf` after the last call. A line
    /// still being written waits for the next call. The buffer only grows.
    fn take(&mut self, buf: &str) -> Vec<serde_json::Value> {
        let Some(rest) = buf.get(self.scanned..) else { return Vec::new() };
        let Some(end) = rest.rfind('\n') else { return Vec::new() };
        self.scanned += end + 1;
        rest[..end]
            .lines()
            // Cheap filter first: only these lines are worth parsing.
            .filter(|l| TYPES.iter().any(|t| l.contains(t)))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            // An agent line that merely quotes a record keeps its own type.
            .filter(|v| v.get("type").and_then(|t| t.as_str()).is_some_and(|t| TYPES.contains(&t)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(v: &[serde_json::Value]) -> Vec<&str> {
        v.iter().map(|r| r["type"].as_str().unwrap()).collect()
    }

    #[test]
    fn takes_only_records_pushed_from_outside_once_each() {
        let mut live = LiveRecords::default();
        let mut buf = String::from("{\"type\":\"assistant\"}\n[log:warn] quota\n");
        buf.push_str(&super::super::turn_cut::line("user", chrono::Utc::now()));
        // A step mark the drain emitted itself, and an agent quoting a record.
        buf.push_str("{\"type\":\"_puppeteer_step_mark\"}\n");
        buf.push_str("{\"type\":\"assistant\",\"text\":\"{\\\"type\\\":\\\"_puppeteer_turn_cut\\\"}\"}\n");
        assert_eq!(types(&live.take(&buf)), ["_puppeteer_turn_cut"]);
        assert!(live.take(&buf).is_empty());

        buf.push_str("{\"type\":\"_puppeteer_worker_results\",\"workers\":[]}\n");
        assert_eq!(types(&live.take(&buf)), ["_puppeteer_worker_results"]);
    }

    #[test]
    fn a_sent_prompt_is_emitted_once_in_order() {
        let run = "live-records-prompt-test";
        push_prompt(run, "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"one\"},\"turn\":4}\n");
        push_prompt(run, "not json\n");
        push_prompt(run, "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"two\"}}\n");
        let taken = take_prompts(run);
        let texts: Vec<&str> = taken.iter().map(|v| v["message"]["content"].as_str().unwrap()).collect();
        assert_eq!(texts, ["one", "two"]);
        assert_eq!(taken[0]["turn"], 4);
        assert!(take_prompts(run).is_empty());
        assert!(take_prompts("another-run").is_empty());
    }

    #[test]
    fn a_line_still_being_written_waits() {
        let mut live = LiveRecords::default();
        let mut buf = String::from("{\"type\":\"_puppeteer_turn_cut\",\"reason\":\"user\"");
        assert!(live.take(&buf).is_empty());
        buf.push_str(",\"at\":\"2026-11-16T10:52:03Z\"}\n");
        assert_eq!(types(&live.take(&buf)), ["_puppeteer_turn_cut"]);
    }
}
