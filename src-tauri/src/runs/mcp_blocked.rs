//! An MCP server Puppeteer does not manage, blocked in a run. The sidecars
//! lock a run down to the servers Puppeteer passed (`sidecar/mcp-guard.mjs`)
//! and report each block as `_puppeteer_mcp_blocked {server, source, reason}`:
//! reason `call` (a tool call was denied), `no_provenance` (denied because the
//! Claude CLI did not say where the tool's server came from — an older CLI,
//! which gets every MCP tool denied, Puppeteer's own included) or `loaded` (the
//! engine still started a server that was not passed; its tools stay denied).
//!
//! The drain logs it (`mcp_unmanaged_blocked`) and tells the session view on
//! `run:mcp_blocked:{run_id}`. Only these three fields go on: the server name is
//! untrusted text from another tool's config, so it is capped and stripped of
//! control characters, and nothing else of the message (no config, no input)
//! reaches the log or the webview.
//!
//! The same event carries the servers Puppeteer meant to pass but left out at
//! start (reason `skipped`, source `puppeteer`): a claude.ai connector whose
//! run key another server has or whose profile's last probe did not report it,
//! a server the engine cannot take. Each was logged where it was skipped; the
//! session shows the names so a missing tool is not silent. The start sends
//! them before the sidecar runs, so a session whose listener attaches later (a
//! conductor worker, a schedule, a Remote start), or that is opened only after
//! its run ended or the window reloaded, would miss them: the last start's list
//! is kept per run until the run is deleted, for [`list_run_mcp_skipped`].

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// Longest server name or source kept, in characters.
const MAX_FIELD_CHARS: usize = 120;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct McpBlocked {
    pub server: String,
    pub source: String,
    /// `call`, `no_provenance`, `loaded` or `skipped`.
    pub reason: &'static str,
}

/// `text` without control characters (a newline would forge a log line),
/// cut to [`MAX_FIELD_CHARS`].
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(MAX_FIELD_CHARS).collect()
}

/// The report a sidecar message carries. A missing source reads `unknown`;
/// any reason but `loaded` or `no_provenance` reads `call`.
pub fn parse(v: &Value) -> McpBlocked {
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
    McpBlocked {
        server: text("server").unwrap_or_else(|| "unknown".to_string()),
        source: text("source").unwrap_or_else(|| "unknown".to_string()),
        reason: match v.get("reason").and_then(Value::as_str) {
            Some("loaded") => "loaded",
            Some("no_provenance") => "no_provenance",
            _ => "call",
        },
    }
}

/// The reports for the servers left out of a run at start (`skipped` of
/// `resolve_run_mcp_servers`).
fn skipped_reports(servers: &[String]) -> Vec<McpBlocked> {
    servers
        .iter()
        .map(|server| McpBlocked { server: clean(server), source: "puppeteer".to_string(), reason: "skipped" })
        .collect()
}

/// The servers each run's last start left out, by run.
static SKIPPED: LazyLock<Mutex<HashMap<String, Vec<McpBlocked>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Keep `reports` as `run_id`'s left-out servers, replacing an earlier start's.
fn remember_skipped(run_id: &str, reports: &[McpBlocked]) {
    let mut map = SKIPPED.lock().unwrap_or_else(|e| e.into_inner());
    if reports.is_empty() {
        map.remove(run_id);
    } else {
        map.insert(run_id.to_string(), reports.to_vec());
    }
}

/// The servers `run_id`'s last start left out (none once the run is deleted).
fn skipped_of(run_id: &str) -> Vec<McpBlocked> {
    SKIPPED.lock().unwrap_or_else(|e| e.into_inner()).get(run_id).cloned().unwrap_or_default()
}

/// Drop `run_id`'s list: the run is deleted.
pub fn forget_skipped(run_id: &str) {
    SKIPPED.lock().unwrap_or_else(|e| e.into_inner()).remove(run_id);
}

/// Tell the session view which servers were left out of the run at start.
/// Each was already logged where it was skipped.
pub fn on_skipped(app: &AppHandle, run_id: &str, servers: &[String]) {
    let reports = skipped_reports(servers);
    remember_skipped(run_id, &reports);
    for report in &reports {
        let _ = app.emit(&format!("run:mcp_blocked:{run_id}"), report);
    }
}

/// The servers a run's last start left out, for a session view that attached
/// after the start sent them. Names only, like the event.
#[tauri::command]
pub async fn list_run_mcp_skipped(run_id: String) -> Result<Vec<McpBlocked>, String> {
    Ok(skipped_of(&run_id))
}

/// Log the block and tell the session view.
pub fn on_blocked(app: &AppHandle, run_id: &str, v: &Value) {
    let blocked = parse(v);
    tracing::warn!(
        event = "mcp_unmanaged_blocked",
        run_id = %run_id,
        server = %blocked.server,
        source = %blocked.source,
        reason = blocked.reason,
    );
    let _ = app.emit(&format!("run:mcp_blocked:{run_id}"), &blocked);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_only_server_source_and_reason() {
        let v = json!({
            "type": "_puppeteer_mcp_blocked",
            "server": "claude_ai_Slack",
            "source": "claudeai",
            "reason": "call",
            "tool_input": { "token": "secret-value" },
            "config": { "headers": { "Authorization": "Bearer x" } },
        });
        let out = serde_json::to_value(parse(&v)).unwrap();
        assert_eq!(out, json!({ "server": "claude_ai_Slack", "source": "claudeai", "reason": "call" }));
    }

    #[test]
    fn caps_the_name_and_strips_control_characters() {
        let long = "x".repeat(500);
        let blocked = parse(&json!({ "server": long, "source": "user\nevent=forged", "reason": "loaded" }));
        assert_eq!(blocked.server.chars().count(), MAX_FIELD_CHARS);
        assert_eq!(blocked.source, "userevent=forged");
        assert_eq!(blocked.reason, "loaded");
        // Multi-byte names are cut on a character, not a byte.
        let wide = parse(&json!({ "server": "é".repeat(300) }));
        assert_eq!(wide.server.chars().count(), MAX_FIELD_CHARS);
    }

    #[test]
    fn a_skipped_server_is_reported_by_name_only() {
        let reports = skipped_reports(&["claude_ai_Slack-global".to_string(), "bad\nname".to_string()]);
        let out = serde_json::to_value(&reports).unwrap();
        assert_eq!(
            out,
            json!([
                { "server": "claude_ai_Slack-global", "source": "puppeteer", "reason": "skipped" },
                { "server": "badname", "source": "puppeteer", "reason": "skipped" },
            ])
        );
        // A sidecar cannot claim `skipped`: only the start reports it.
        assert_eq!(parse(&json!({ "server": "x", "reason": "skipped" })).reason, "call");
    }

    #[test]
    fn a_later_listener_reads_the_last_starts_skipped_servers_until_the_run_is_deleted() {
        let run = "run-skipped-kept";
        assert!(skipped_of(run).is_empty());
        remember_skipped(run, &skipped_reports(&["claude_ai_Slack-global".to_string()]));
        let out = serde_json::to_value(skipped_of(run)).unwrap();
        assert_eq!(out, json!([{ "server": "claude_ai_Slack-global", "source": "puppeteer", "reason": "skipped" }]));
        // A resume that leaves nothing out replaces the list.
        remember_skipped(run, &[]);
        assert!(skipped_of(run).is_empty());
        remember_skipped(run, &skipped_reports(&["pms".to_string()]));
        assert_eq!(skipped_of(run).len(), 1);
        forget_skipped(run);
        assert!(skipped_of(run).is_empty());
    }

    #[test]
    fn the_skipped_list_outlives_the_sidecar_and_goes_with_the_run() {
        // A run that ends before its view attaches still shows what it left out.
        assert!(!include_str!("sidecar.rs").contains("forget_skipped"));
        let runs = include_str!("../commands/runs.rs");
        let delete = &runs[runs.find("async fn delete_single_run(").unwrap()..];
        let delete = &delete[..delete.find("\n}\n").unwrap()];
        assert!(delete.contains("crate::runs::mcp_blocked::forget_skipped(run_id);"));
    }

    #[test]
    fn a_remote_desktop_may_read_the_skipped_servers() {
        // Names of servers the session view shows anyway; read-only.
        let lib = include_str!("../lib.rs");
        assert!(lib.contains("runs::mcp_blocked::list_run_mcp_skipped,"));
        assert!(!crate::remote::guard::is_local_only_command("list_run_mcp_skipped"));
    }

    #[test]
    fn a_call_denied_for_missing_provenance_keeps_its_reason() {
        let blocked = parse(&json!({ "server": "pms", "source": "unknown", "reason": "no_provenance" }));
        assert_eq!(blocked, McpBlocked { server: "pms".into(), source: "unknown".into(), reason: "no_provenance" });
    }

    #[test]
    fn fills_missing_fields() {
        let blocked = parse(&json!({ "reason": "something-else", "server": 7 }));
        assert_eq!(blocked, McpBlocked { server: "unknown".into(), source: "unknown".into(), reason: "call" });
    }
}
