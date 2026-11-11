//! Audit logging for the broker (GĐ2, AC4) and for tool approvals (AC-22).
//!
//! SECURITY (Hard Rule 1): this API **cannot** log a token — by design it takes
//! no token parameter. Every broker request is audited with only
//! tool/argv/decision/reason. Token values live in `ResolvedToken`/`BrokerResponse`
//! and never reach this module.
//!
//! A tool approval (an agent's Bash / Edit / MCP call, a Codex command, a
//! broker prompt) is audited on its own target, `puppeteer::tool_audit`: a
//! broker line says a shim used a credential, this one says who let a tool
//! run. Both land in the same daily log file. Of the tool's input only a
//! summary is kept — the command or the file path, redacted, then cut.

/// Build the audit line as a plain string (separated from I/O so it can be
/// asserted in tests). Never contains a token — the signature has no token arg.
pub fn format_audit_line(
    run_id: &str,
    tool: &str,
    argv: &[String],
    decision: &str,
    reason: Option<&str>,
) -> String {
    let argv_joined = argv.join(" ");
    match reason {
        Some(r) => format!(
            "broker run={run_id} tool={tool} argv=[{argv_joined}] decision={decision} reason={r}"
        ),
        None => format!(
            "broker run={run_id} tool={tool} argv=[{argv_joined}] decision={decision}"
        ),
    }
}

/// Emit an audit record for a broker request. Token is NEVER passed in.
pub fn audit_request(
    run_id: &str,
    tool: &str,
    argv: &[String],
    decision: &str,
    reason: Option<&str>,
) {
    let line = format_audit_line(run_id, tool, argv, decision, reason);
    tracing::info!(target: "puppeteer::broker::audit", "{line}");
}

/// Where the answer to a permission prompt came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Someone at this machine's desktop.
    Local,
    /// A paired phone, or a remote desktop through the bridge webview.
    Remote,
    /// The project's standing Always allow / deny choice, nobody asked.
    AutoRule,
    /// Nobody: the request was withdrawn (the engine cancelled the tool, its
    /// input closed, or a broker shim hung up).
    Engine,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Local => "local",
            Origin::Remote => "remote",
            Origin::AutoRule => "auto_rule",
            Origin::Engine => "engine",
        }
    }
}

/// Decision of a request withdrawn without an answer.
pub const CANCEL: &str = "cancel";

/// Longest input summary kept, in characters.
pub const INPUT_SUMMARY_MAX_CHARS: usize = 200;

/// The audited decision of an answer. Both sidecars treat anything but `allow`
/// as a deny (`ask` included). An Always answer is `*_always`; a standing
/// choice applied again is not — the rule already stood (its `remember` only
/// lets Codex cache the approval).
pub fn decision_label(decision: &str, remember: bool, origin: Origin) -> &'static str {
    let always = remember && origin != Origin::AutoRule;
    match (decision == "allow", always) {
        (true, false) => "allow",
        (true, true) => "allow_always",
        (false, false) => "deny",
        (false, true) => "deny_always",
    }
}

/// Engine of a permission request, read from its id: the Codex sidecar names
/// every request `codex-<nonce>-<rpc id>` (`emitPermission` in
/// `sidecar-codex/index.mjs`); Claude's sidecar and the broker use UUIDs.
pub fn engine_of_request(request_id: &str) -> &'static str {
    if request_id.starts_with("codex-") {
        "codex"
    } else {
        "claude"
    }
}

/// What the audit keeps of a tool's input: its shell command, else its file
/// path, with credentials redacted and cut to [`INPUT_SUMMARY_MAX_CHARS`].
/// Nothing else — never file contents, MCP arguments or question answers.
/// Redacted before the cut, so a token on the boundary is not half kept.
pub fn input_summary(tool_input: &serde_json::Value) -> Option<String> {
    let obj = tool_input.as_object()?;
    let raw = ["command", "file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(serde_json::Value::as_str))?;
    let redacted = crate::runs::transcript::redact(raw);
    if redacted.chars().count() <= INPUT_SUMMARY_MAX_CHARS {
        return Some(redacted);
    }
    let mut cut: String = redacted.chars().take(INPUT_SUMMARY_MAX_CHARS).collect();
    cut.push('…');
    Some(cut)
}

/// Build the tool-approval audit line (separated from I/O so it can be
/// asserted in tests). The tool and the input are quoted: either may hold
/// spaces or line breaks.
pub fn format_tool_decision_line(
    event: &crate::runs::permission::PermissionRequestEvent,
    decision: &str,
    origin: Origin,
    at: chrono::DateTime<chrono::Utc>,
) -> String {
    let mut line = format!(
        "tool_decision run={} request={} engine={} tool={:?} decision={decision} origin={} at={}",
        event.run_id,
        event.request_id,
        engine_of_request(&event.request_id),
        event.tool_name,
        origin.as_str(),
        at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    );
    if let Some(input) = input_summary(&event.tool_input) {
        line.push_str(&format!(" input={input:?}"));
    }
    line
}

/// Emit one audit record for a permission decision on `event`.
pub fn tool_decision(
    event: &crate::runs::permission::PermissionRequestEvent,
    decision: &str,
    origin: Origin,
) {
    let line = format_tool_decision_line(event, decision, origin, chrono::Utc::now());
    tracing::info!(target: "puppeteer::tool_audit", "{line}");
}

/// Captures the audit records a test emits on its own thread.
#[cfg(test)]
pub(crate) mod capture {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    pub struct Lines(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Lines {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Lines {
        /// The `puppeteer::tool_audit` records of `run_id`.
        pub fn tool_records(&self, run_id: &str) -> Vec<String> {
            let run = format!(" run={run_id} ");
            String::from_utf8_lossy(&self.0.lock().unwrap())
                .lines()
                .filter(|l| l.contains("puppeteer::tool_audit") && l.contains(&run))
                .map(str::to_string)
                .collect()
        }
    }

    /// Record every event of this thread until the guard drops. A tokio test
    /// runs on one thread, so this spans its awaits too.
    pub fn start() -> (Lines, tracing::subscriber::DefaultGuard) {
        let lines = Lines::default();
        let writer = lines.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        (lines, tracing::subscriber::set_default(subscriber))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_line_never_contains_token_word() {
        // Even if argv were to contain a token-looking value, the function has no
        // way to receive the resolved token; assert the formatted line only holds
        // what we passed and no "token=" field is produced.
        let argv = vec!["pr".to_string(), "create".to_string()];
        let line = format_audit_line("run-1", "gh", &argv, "ask", Some("write op"));
        assert!(line.contains("decision=ask"));
        assert!(line.contains("tool=gh"));
        assert!(!line.contains("token="), "audit line must not carry a token field");
    }

    #[test]
    fn audit_line_without_reason() {
        let argv = vec!["pr".to_string(), "list".to_string()];
        let line = format_audit_line("run-1", "gh", &argv, "allow", None);
        assert!(line.contains("decision=allow"));
        assert!(!line.contains("reason="));
        assert!(!line.contains("token="));
    }

    fn request(request_id: &str, tool: &str, input: serde_json::Value) -> crate::runs::permission::PermissionRequestEvent {
        crate::runs::permission::PermissionRequestEvent {
            run_id: "run-a".to_string(),
            request_id: request_id.to_string(),
            tool_name: tool.to_string(),
            tool_input: input,
            session_id: None,
            cwd: None,
            title: None,
            description: None,
            display_name: None,
            once_only: false,
        }
    }

    fn at() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-11-11T09:30:00Z").unwrap().into()
    }

    #[test]
    fn tool_decision_line_has_every_field() {
        let evt = request("codex-n1-7", "Bash", serde_json::json!({ "command": "cargo test" }));
        let line = format_tool_decision_line(&evt, "allow", Origin::Local, at());
        assert_eq!(
            line,
            "tool_decision run=run-a request=codex-n1-7 engine=codex tool=\"Bash\" decision=allow \
             origin=local at=2026-11-11T09:30:00.000Z input=\"cargo test\""
        );
        let evt = request("0b6f9a0e-uuid", "Write", serde_json::json!({ "file_path": "/p/a.rs", "content": "fn main() {}" }));
        let line = format_tool_decision_line(&evt, "allow_always", Origin::Remote, at());
        assert!(line.contains("engine=claude"), "{line}");
        assert!(line.contains("origin=remote") && line.contains("decision=allow_always"), "{line}");
        assert!(line.contains("input=\"/p/a.rs\""), "{line}");
        assert!(!line.contains("fn main"), "file contents must not be logged: {line}");
    }

    #[test]
    fn only_a_command_or_a_path_is_summarised() {
        assert_eq!(input_summary(&serde_json::json!({ "file_path": "/a", "old_string": "x" })).as_deref(), Some("/a"));
        assert_eq!(input_summary(&serde_json::json!({ "notebook_path": "/n.ipynb" })).as_deref(), Some("/n.ipynb"));
        // MCP arguments and question answers stay out.
        assert_eq!(input_summary(&serde_json::json!({ "query": "select * from users" })), None);
        assert_eq!(input_summary(&serde_json::json!({ "questions": [] })), None);
        assert_eq!(input_summary(&serde_json::Value::Null), None);
        let evt = request("r", "mcp__db__query", serde_json::json!({ "sql": "secret stuff" }));
        assert!(!format_tool_decision_line(&evt, "deny", Origin::Local, at()).contains("input="));
    }

    #[test]
    fn a_long_command_is_cut() {
        let cmd = "é".repeat(INPUT_SUMMARY_MAX_CHARS + 50);
        let summary = input_summary(&serde_json::json!({ "command": cmd })).unwrap();
        assert_eq!(summary.chars().count(), INPUT_SUMMARY_MAX_CHARS + 1);
        assert!(summary.ends_with('…'));
        let exact = "a".repeat(INPUT_SUMMARY_MAX_CHARS);
        assert_eq!(input_summary(&serde_json::json!({ "command": exact.clone() })), Some(exact));
    }

    #[test]
    fn a_token_in_the_input_is_redacted() {
        let token = "ghp_0123456789abcdefghijABCDEFGHIJ";
        let evt = request("r", "Bash", serde_json::json!({ "command": format!("GH_TOKEN={token} gh pr list") }));
        let line = format_tool_decision_line(&evt, "allow", Origin::Local, at());
        assert!(!line.contains(token), "{line}");
        assert!(line.contains("[redacted]"), "{line}");
        // On the cut boundary it is redacted before the cut, never half kept.
        let cmd = format!("{}{token}", "x ".repeat((INPUT_SUMMARY_MAX_CHARS - 10) / 2));
        let summary = input_summary(&serde_json::json!({ "command": cmd })).unwrap();
        assert!(!summary.contains("ghp_0123"), "{summary}");
        let evt = request("r", "Bash", serde_json::json!({ "command": "curl -H 'Authorization: Bearer abc.def-1234567890XYZ' x" }));
        assert!(!format_tool_decision_line(&evt, "allow", Origin::Local, at()).contains("1234567890XYZ"));
    }

    #[test]
    fn line_breaks_in_the_input_stay_on_one_line() {
        let evt = request("r", "Bash", serde_json::json!({ "command": "echo a\necho b" }));
        let line = format_tool_decision_line(&evt, "allow", Origin::Local, at());
        assert!(!line.contains('\n'), "{line}");
        assert!(line.contains("input=\"echo a\\necho b\""), "{line}");
    }

    #[test]
    fn decision_labels() {
        assert_eq!(decision_label("allow", false, Origin::Local), "allow");
        assert_eq!(decision_label("allow", true, Origin::Local), "allow_always");
        assert_eq!(decision_label("deny", true, Origin::Remote), "deny_always");
        assert_eq!(decision_label("deny", false, Origin::Remote), "deny");
        // Both sidecars deny anything but allow.
        assert_eq!(decision_label("ask", false, Origin::Local), "deny");
        // A standing choice applied again is not a new Always.
        assert_eq!(decision_label("allow", true, Origin::AutoRule), "allow");
    }
}
