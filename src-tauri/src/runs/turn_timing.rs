//! How long a turn takes, phase by phase (AC-09).
//!
//! Every turn currently spawns a fresh sidecar (Node, the CLI or
//! `codex app-server`, every MCP server) and reloads the transcript. Before
//! deciding whether to keep a process per session instead (AC-20), each turn
//! records where its time went: one `turn_timing` record, written by the drain
//! to the run's log and to tracing (`puppeteer::turn_timing`). Measuring only:
//! nothing reads these records back.

use serde_json::{json, Value};
use std::time::Instant;

/// When the sidecar of a run was asked for and when it came up: the marks the
/// first turn on a freshly spawned process is measured from.
#[derive(Clone, Copy, Debug)]
pub struct SpawnMarks {
    /// The start / resume command began.
    pub command_at: Instant,
    /// The sidecar process was spawned.
    pub spawned_at: Instant,
    /// MCP servers handed to the sidecar. Claude's `system/init` replaces it
    /// with the servers the CLI actually loaded (user settings included).
    pub mcp_server_count: Option<usize>,
}

/// The marks of the turn in flight, kept by the drain.
pub struct TurnTiming {
    /// Until the first measured turn of this process ends: its later turns run
    /// on a warm process, so the start-up phases don't apply to them.
    spawn: Option<SpawnMarks>,
    ready_at: Option<Instant>,
    init_at: Option<Instant>,
    first_token_at: Option<Instant>,
    mcp_server_count: Option<usize>,
}

impl TurnTiming {
    pub fn new(spawn: SpawnMarks) -> Self {
        Self {
            mcp_server_count: spawn.mcp_server_count,
            spawn: Some(spawn),
            ready_at: None,
            init_at: None,
            first_token_at: None,
        }
    }

    /// The sidecar sent `_puppeteer_ready`.
    pub fn on_ready(&mut self, now: Instant) {
        if self.spawn.is_some() {
            self.ready_at.get_or_insert(now);
        }
    }

    /// A raw engine message: `system/init`, or the turn's first output.
    pub fn on_event(&mut self, now: Instant, value: &Value) {
        let main_thread = matches!(value.get("parent_tool_use_id"), None | Some(Value::Null));
        match value.get("type").and_then(Value::as_str) {
            Some("system") if value.get("subtype").and_then(Value::as_str) == Some("init") => {
                if self.spawn.is_some() {
                    self.init_at.get_or_insert(now);
                }
                if let Some(servers) = value.get("mcp_servers").and_then(Value::as_array) {
                    self.mcp_server_count = Some(servers.len());
                }
            }
            // Claude streams deltas; Codex (until it streams them) sends whole
            // items, so its first item stands for the first token.
            Some("stream_event") | Some("assistant") if main_thread => {
                self.first_token_at.get_or_insert(now);
            }
            _ => {}
        }
    }

    /// A turn the engine ran by itself ended (an origin-tagged `result`). It
    /// had no prompt, so it isn't measured, and its output must not pass for
    /// the next turn's first token.
    pub fn skip_turn(&mut self) {
        self.first_token_at = None;
    }

    /// The user's turn ended at `now`: its record, with every interval in ms
    /// (`null` when a mark is missing). `prompt_at` is when its prompt reached
    /// the sidecar's input. Resets for the next turn, which runs on this
    /// (by then warm) process.
    pub fn finish(&mut self, now: Instant, prompt_at: Option<Instant>, engine: &str) -> Value {
        let spawn = self.spawn.take();
        let spawned_at = spawn.map(|s| s.spawned_at);
        let record = json!({
            "type": "turn_timing",
            "engine": engine,
            "fresh_process": spawn.is_some(),
            "mcp_server_count": self.mcp_server_count,
            "spawn_ms": interval_ms(spawn.map(|s| s.command_at), spawned_at),
            "ready_ms": interval_ms(spawned_at, self.ready_at),
            "init_ms": interval_ms(spawned_at, self.init_at),
            "first_token_ms": interval_ms(prompt_at, self.first_token_at),
            "turn_ms": interval_ms(prompt_at, Some(now)),
        });
        self.ready_at = None;
        self.init_at = None;
        self.first_token_at = None;
        record
    }
}

/// `to - from` in ms; `null` when either mark is missing or `to` came first
/// (output that preceded the prompt is not this turn's).
fn interval_ms(from: Option<Instant>, to: Option<Instant>) -> Value {
    match (from, to) {
        (Some(from), Some(to)) => to
            .checked_duration_since(from)
            .map_or(Value::Null, |d| json!(d.as_millis() as u64)),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn marks(base: Instant, mcp: Option<usize>) -> SpawnMarks {
        SpawnMarks { command_at: base, spawned_at: at(base, 40), mcp_server_count: mcp }
    }

    fn assert_non_negative(record: &Value) {
        for key in ["spawn_ms", "ready_ms", "init_ms", "first_token_ms", "turn_ms"] {
            let v = &record[key];
            assert!(v.is_null() || v.as_u64().is_some(), "{key}: {v}");
        }
    }

    #[test]
    fn a_fresh_turn_records_every_phase() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, Some(2)));
        timing.on_ready(at(t0, 100));
        let prompt_at = at(t0, 110);
        timing.on_event(
            at(t0, 900),
            &json!({ "type": "system", "subtype": "init", "mcp_servers": [{ "name": "a" }, { "name": "b" }, { "name": "c" }] }),
        );
        timing.on_event(at(t0, 1500), &json!({ "type": "stream_event", "parent_tool_use_id": null }));
        timing.on_event(at(t0, 1600), &json!({ "type": "stream_event" }));
        let record = timing.finish(at(t0, 4000), Some(prompt_at), "claude");
        assert_eq!(
            record,
            json!({
                "type": "turn_timing",
                "engine": "claude",
                "fresh_process": true,
                "mcp_server_count": 3,
                "spawn_ms": 40,
                "ready_ms": 60,
                "init_ms": 860,
                "first_token_ms": 1390,
                "turn_ms": 3890,
            })
        );
        assert_non_negative(&record);
    }

    #[test]
    fn a_turn_without_stream_events_has_no_first_token() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, None));
        timing.on_ready(at(t0, 50));
        timing.on_event(at(t0, 60), &json!({ "type": "system", "subtype": "init" }));
        let record = timing.finish(at(t0, 500), Some(at(t0, 55)), "codex");
        assert!(record["first_token_ms"].is_null());
        assert_eq!(record["turn_ms"], 445);
        assert!(record["mcp_server_count"].is_null());
        assert_non_negative(&record);
    }

    #[test]
    fn codex_counts_its_first_item_and_keeps_the_spawn_count() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, Some(4)));
        // Codex announces init (without a server list) before any prompt.
        timing.on_event(at(t0, 300), &json!({ "type": "system", "subtype": "init" }));
        timing.on_ready(at(t0, 310));
        timing.on_event(at(t0, 800), &json!({ "type": "assistant", "message": {} }));
        let record = timing.finish(at(t0, 900), Some(at(t0, 400)), "codex");
        assert_eq!(record["mcp_server_count"], 4);
        assert_eq!(record["init_ms"], 260);
        assert_eq!(record["first_token_ms"], 400);
    }

    #[test]
    fn later_turns_on_the_process_are_warm() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, Some(1)));
        timing.on_ready(at(t0, 10));
        timing.on_event(at(t0, 20), &json!({ "type": "system", "subtype": "init" }));
        timing.on_event(at(t0, 30), &json!({ "type": "stream_event" }));
        timing.finish(at(t0, 40), Some(at(t0, 15)), "claude");

        // Claude sends a fresh init on every turn of a live process.
        timing.on_event(at(t0, 120), &json!({ "type": "system", "subtype": "init" }));
        timing.on_event(at(t0, 150), &json!({ "type": "stream_event" }));
        let record = timing.finish(at(t0, 200), Some(at(t0, 100)), "claude");
        assert_eq!(record["fresh_process"], false);
        assert!(record["spawn_ms"].is_null());
        assert!(record["ready_ms"].is_null());
        assert!(record["init_ms"].is_null());
        assert_eq!(record["first_token_ms"], 50);
        assert_eq!(record["turn_ms"], 100);
    }

    #[test]
    fn subagent_output_and_engine_turns_are_not_the_first_token() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, None));
        timing.on_event(at(t0, 50), &json!({ "type": "stream_event", "parent_tool_use_id": "toolu_1" }));
        // A leftover background task's turn, before the user's.
        timing.on_event(at(t0, 60), &json!({ "type": "assistant" }));
        timing.skip_turn();
        let record = timing.finish(at(t0, 300), Some(at(t0, 100)), "claude");
        assert!(record["first_token_ms"].is_null());
        assert_eq!(record["fresh_process"], true);
    }

    #[test]
    fn output_before_the_prompt_or_no_prompt_mark_gives_null_not_a_negative() {
        let t0 = Instant::now();
        let mut timing = TurnTiming::new(marks(t0, None));
        timing.on_event(at(t0, 50), &json!({ "type": "assistant" }));
        let record = timing.finish(at(t0, 300), Some(at(t0, 100)), "claude");
        assert!(record["first_token_ms"].is_null());
        assert_non_negative(&record);

        let record = timing.finish(at(t0, 400), None, "claude");
        assert!(record["turn_ms"].is_null());
        assert!(record["first_token_ms"].is_null());
    }
}
