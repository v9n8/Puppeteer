//! Log records that code without an `AppHandle` pushes straight into a live
//! run's log buffer: the turn-cut mark (`turn_cut::push_live`, reached from
//! `cancel_run_inner`) and a conductor's worker result cards
//! (`conductor::worker_results`). The drain writes them to the file with the
//! rest of the buffer; it also hands each one to the open session on
//! `run:event:<id>` when it flushes, the way it emits step marks, so the
//! session shows them without reloading its log.

use tauri::{AppHandle, Emitter};

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
    fn a_line_still_being_written_waits() {
        let mut live = LiveRecords::default();
        let mut buf = String::from("{\"type\":\"_puppeteer_turn_cut\",\"reason\":\"user\"");
        assert!(live.take(&buf).is_empty());
        buf.push_str(",\"at\":\"2026-11-16T10:52:03Z\"}\n");
        assert_eq!(types(&live.take(&buf)), ["_puppeteer_turn_cut"]);
    }
}
