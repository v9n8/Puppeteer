//! An MCP server Puppeteer does not manage, blocked in a run. The sidecars
//! lock a run down to the servers Puppeteer passed (`sidecar/mcp-guard.mjs`)
//! and report each block as `_puppeteer_mcp_blocked {server, source, reason}`:
//! reason `call` (a tool call was denied) or `loaded` (the engine still
//! started a server that was not passed; its tools stay denied).
//!
//! The drain logs it (`mcp_unmanaged_blocked`) and tells the session view on
//! `run:mcp_blocked:{run_id}`. Only these three fields go on: the server name is
//! untrusted text from another tool's config, so it is capped and stripped of
//! control characters, and nothing else of the message (no config, no input)
//! reaches the log or the webview.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// Longest server name or source kept, in characters.
const MAX_FIELD_CHARS: usize = 120;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct McpBlocked {
    pub server: String,
    pub source: String,
    /// `call` or `loaded`.
    pub reason: &'static str,
}

/// `text` without control characters (a newline would forge a log line),
/// cut to [`MAX_FIELD_CHARS`].
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(MAX_FIELD_CHARS).collect()
}

/// The report a sidecar message carries. A missing source reads `unknown`;
/// any reason but `loaded` reads `call`.
pub fn parse(v: &Value) -> McpBlocked {
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
    McpBlocked {
        server: text("server").unwrap_or_else(|| "unknown".to_string()),
        source: text("source").unwrap_or_else(|| "unknown".to_string()),
        reason: if v.get("reason").and_then(Value::as_str) == Some("loaded") { "loaded" } else { "call" },
    }
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
    fn fills_missing_fields() {
        let blocked = parse(&json!({ "reason": "something-else", "server": 7 }));
        assert_eq!(blocked, McpBlocked { server: "unknown".into(), source: "unknown".into(), reason: "call" });
    }
}
