//! Permission request event emitted to the frontend, plus the backend-side
//! record of requests still awaiting an answer.
//!
//! Permission interception is handled by the Agent-SDK sidecar's `canUseTool`
//! callback (see `runs/sidecar.rs`); the drain emits the payload below on
//! `run:permission_request:<run_id>`. That event is fire-and-forget: if no
//! webview listener is attached at that instant (e.g. the live session was
//! evicted), the request is lost and the sidecar waits forever. The pending
//! store lets a (re)attached listener recover still-open requests and lets the
//! conductor see which worker is blocked on an approval.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Serialize, Clone, Default)]
pub struct PermissionRequestEvent {
    pub run_id: String,
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// Bridge-rendered prompt sentence (e.g. "Claude wants to read foo.txt").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Bridge-rendered subtitle describing the effect of the tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Short noun phrase for the action (e.g. "Read file"), for compact UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// A broker prompt (gh / glab / ssh / aws / az / terraform): it can only be
    /// answered once — there is no standing Always for a credential use.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once_only: bool,
    /// The run's engine ("claude" / "codex"); None for a broker prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Model id from the run's latest `system.init`; None before the first one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Which agent asked; None when the engine has no sub-agents (Codex) or
    /// the source does not say (broker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<PermissionAgent>,
    /// When the request arrived. The pending store reuses it, so the UI and
    /// the watchdog read one clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<chrono::DateTime<chrono::Utc>>,
    /// A broker prompt's argv and reason, so the UI can name the action and
    /// explain the reason in the app's language. None for a sidecar prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker: Option<BrokerPrompt>,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq)]
pub struct BrokerPrompt {
    /// The CLI's argv after the tool name, verbatim: `tool_input.command` is
    /// joined by spaces, so it cannot be split back into the real tokens.
    pub argv: Vec<String>,
    /// The policy reason as codes; empty when the English text is not
    /// recognised (the UI then shows `description` as is).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason: Vec<ReasonPart>,
    /// Whether the prompt may offer allowing this command for the rest of the run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_for_run: bool,
}

/// One piece of a broker reason: a code the UI translates and its parameters.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ReasonPart {
    pub code: String,
    pub params: std::collections::BTreeMap<String, String>,
}

/// The agent behind a permission request. The SDK hands `canUseTool` only the
/// sub-agent's id, never its name, so `name` stays None until a source has it.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PermissionAgent {
    Main,
    Sub {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// The agent of a `_puppeteer_permission_request` payload. Claude's sidecar
/// always sends `agent_id` (null for the main agent); Codex has no sub-agents.
pub fn agent_of(engine: &str, v: &Value) -> Option<PermissionAgent> {
    if engine != "claude" {
        return None;
    }
    match v.get("agent_id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => Some(PermissionAgent::Sub { id: Some(id.to_string()), name: None }),
        _ => Some(PermissionAgent::Main),
    }
}

/// The model to show on a request: the last `system.init` model, if any.
pub fn model_of(last_model: Option<&str>) -> Option<String> {
    last_model.filter(|m| !m.is_empty()).map(str::to_string)
}

/// A permission request the sidecar is still blocked on.
#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub event: PermissionRequestEvent,
    pub requested_at: chrono::DateTime<chrono::Utc>,
}

/// run_id -> open requests, in arrival order.
fn pending_store() -> &'static Mutex<HashMap<String, Vec<PendingPermission>>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Vec<PendingPermission>>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record a request the drain just emitted.
pub fn track_pending(event: PermissionRequestEvent) {
    let requested_at = event.requested_at.unwrap_or_else(chrono::Utc::now);
    let mut map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(event.run_id.clone()).or_default().push(PendingPermission { event, requested_at });
}

/// Withdraw a request that ends without an answer: the sidecar settled it
/// itself, or a broker prompt's shim hung up (an answered one was already
/// claimed by [`take_pending`]). Audited as a cancel (AC-22). Returns whether
/// it was still pending.
pub fn resolve_pending(run_id: &str, request_id: &str) -> bool {
    let withdrawn = {
        let mut map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
        let Some(list) = map.get_mut(run_id) else {
            return false;
        };
        let withdrawn = list.iter().find(|p| p.event.request_id == request_id).cloned();
        list.retain(|p| p.event.request_id != request_id);
        if list.is_empty() {
            map.remove(run_id);
        }
        withdrawn
    };
    let Some(p) = withdrawn else {
        return false;
    };
    use crate::runs::broker::audit;
    audit::tool_decision(&p.event, audit::CANCEL, audit::Origin::Engine);
    true
}

/// Claim a request for answering: remove and return it, or None when it was
/// already answered (by the desktop, a phone, a standing choice) or withdrawn.
/// Exactly one answer wins; a loser must not reach the sidecar.
pub fn take_pending(run_id: &str, request_id: &str) -> Option<PendingPermission> {
    let mut map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    let list = map.get_mut(run_id)?;
    let i = list.iter().position(|p| p.event.request_id == request_id)?;
    let taken = list.remove(i);
    if list.is_empty() {
        map.remove(run_id);
    }
    Some(taken)
}

/// Put back a request whose answer could not be delivered, so it can be retried.
pub fn restore_pending(p: PendingPermission) {
    let mut map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    let list = map.entry(p.event.run_id.clone()).or_default();
    if !list.iter().any(|x| x.event.request_id == p.event.request_id) {
        list.push(p);
        list.sort_by_key(|x| x.requested_at);
    }
}

/// A still-open request, without claiming it.
pub fn peek_pending(run_id: &str, request_id: &str) -> Option<PendingPermission> {
    let map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    map.get(run_id)?
        .iter()
        .find(|p| p.event.request_id == request_id)
        .cloned()
}

/// Drop every open request of a run whose sidecar has exited.
pub fn clear_pending(run_id: &str) {
    let mut map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(run_id);
}

/// Snapshot of the requests a run is still waiting on (oldest first).
pub fn pending_for(run_id: &str) -> Vec<PendingPermission> {
    let map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    map.get(run_id).cloned().unwrap_or_default()
}

/// Snapshot of every open request across all runs (oldest first per run).
pub fn all_pending() -> Vec<PendingPermission> {
    let map = pending_store().lock().unwrap_or_else(|e| e.into_inner());
    map.values().flatten().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(run_id: &str, request_id: &str) -> PermissionRequestEvent {
        PermissionRequestEvent {
            run_id: run_id.to_string(),
            request_id: request_id.to_string(),
            tool_name: "Bash".to_string(),
            tool_input: Value::Null,
            session_id: None,
            cwd: None,
            title: None,
            description: None,
            display_name: None,
            once_only: false,
            engine: None,
            model: None,
            agent: None,
            requested_at: None,
            broker: None,
        }
    }

    fn at() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-11-18T10:51:00Z").unwrap().into()
    }

    #[test]
    fn origin_fields_serialize_only_when_present() {
        let full = PermissionRequestEvent {
            engine: Some("claude".to_string()),
            model: Some("claude-opus-4-5".to_string()),
            agent: Some(PermissionAgent::Main),
            requested_at: Some(at()),
            ..evt("r", "a")
        };
        let json = serde_json::to_value(&full).unwrap();
        assert_eq!(json["engine"], "claude");
        assert_eq!(json["model"], "claude-opus-4-5");
        assert_eq!(json["agent"], serde_json::json!({ "kind": "main" }));
        assert_eq!(json["requested_at"], "2026-11-18T10:51:00Z");

        let sub = PermissionAgent::Sub { id: Some("ag-1".to_string()), name: None };
        assert_eq!(serde_json::to_value(&sub).unwrap(), serde_json::json!({ "kind": "sub", "id": "ag-1" }));

        let bare = serde_json::to_value(evt("r", "a")).unwrap();
        for key in ["engine", "model", "agent", "requested_at"] {
            assert!(bare.get(key).is_none(), "{key} must be absent when None");
        }
    }

    #[test]
    fn agent_of_reads_agent_id_for_claude_only() {
        let sub = Some(PermissionAgent::Sub { id: Some("ag-1".to_string()), name: None });
        assert_eq!(agent_of("claude", &serde_json::json!({ "agent_id": "ag-1" })), sub);
        assert_eq!(agent_of("claude", &serde_json::json!({})), Some(PermissionAgent::Main));
        assert_eq!(agent_of("claude", &serde_json::json!({ "agent_id": null })), Some(PermissionAgent::Main));
        assert_eq!(agent_of("claude", &serde_json::json!({ "agent_id": "" })), Some(PermissionAgent::Main));
        assert_eq!(agent_of("codex", &serde_json::json!({ "agent_id": "ag-1" })), None);
        assert_eq!(agent_of("", &serde_json::json!({})), None);
    }

    #[test]
    fn model_of_drops_empty() {
        assert_eq!(model_of(Some("gpt-5.1-codex")), Some("gpt-5.1-codex".to_string()));
        assert_eq!(model_of(Some("")), None);
        assert_eq!(model_of(None), None);
    }

    #[test]
    fn pending_keeps_origin_and_reuses_requested_at() {
        let run = "test-run-origin";
        track_pending(PermissionRequestEvent {
            engine: Some("codex".to_string()),
            model: Some("gpt-5.1-codex".to_string()),
            requested_at: Some(at()),
            ..evt(run, "a")
        });
        let before = chrono::Utc::now();
        track_pending(evt(run, "b"));

        let a = peek_pending(run, "a").expect("pending");
        assert_eq!(a.requested_at, at());
        assert_eq!(a.event.requested_at, Some(at()));
        assert_eq!(a.event.engine.as_deref(), Some("codex"));
        assert_eq!(a.event.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(a.event.agent, None);
        assert_eq!(pending_for(run)[0].event.requested_at, Some(at()));

        let b = peek_pending(run, "b").expect("pending");
        assert!(b.requested_at >= before, "no event time: stamped now");
        assert_eq!(b.event.requested_at, None);
        clear_pending(run);
    }

    #[test]
    fn tracks_resolves_and_clears() {
        let run = "test-run-pending-store";
        track_pending(evt(run, "a"));
        track_pending(evt(run, "b"));
        let ids: Vec<String> = pending_for(run).into_iter().map(|p| p.event.request_id).collect();
        assert_eq!(ids, vec!["a", "b"]);

        resolve_pending(run, "a");
        let ids: Vec<String> = pending_for(run).into_iter().map(|p| p.event.request_id).collect();
        assert_eq!(ids, vec!["b"]);

        clear_pending(run);
        assert!(pending_for(run).is_empty());
    }

    #[test]
    fn only_one_claim_wins_and_a_failed_delivery_can_be_retried() {
        let run = "test-run-claim";
        track_pending(evt(run, "a"));
        let first = take_pending(run, "a").expect("first claim wins");
        assert!(take_pending(run, "a").is_none(), "a second answer must lose");
        assert!(peek_pending(run, "a").is_none());
        restore_pending(first);
        assert!(peek_pending(run, "a").is_some(), "restored for a retry");
        assert!(take_pending(run, "a").is_some());
        assert!(take_pending(run, "missing").is_none());
        clear_pending(run);
    }

    #[test]
    fn all_pending_spans_every_run() {
        let (r1, r2) = ("test-run-all-pending-1", "test-run-all-pending-2");
        track_pending(evt(r1, "x1"));
        track_pending(evt(r2, "x2"));
        let ids: Vec<String> = all_pending()
            .into_iter()
            .filter(|p| p.event.run_id == r1 || p.event.run_id == r2)
            .map(|p| p.event.request_id)
            .collect();
        assert!(ids.contains(&"x1".to_string()) && ids.contains(&"x2".to_string()));
        clear_pending(r1);
        clear_pending(r2);
    }
}
