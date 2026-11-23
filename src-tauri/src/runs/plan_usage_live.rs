//! RT-31: a Claude `rate_limit_event` carries the live % of every window.
//!
//! `/usage` can answer with numbers hours old (2026-11-23: 11% while the run's
//! own events said 62%). Newer CLIs list each window under the event's
//! `unifiedWindows` — utilization as a 0–1 fraction, reset in epoch seconds,
//! and status — so those land in the snapshot the same way `/usage` would.

use crate::runs::sidecar::{plan_reset_to_string, plan_status_severity, plan_window_key};
use serde_json::{json, Map, Value};

/// Write the event's `unifiedWindows` into `snapshot`, stamped as captured now;
/// false (and `snapshot` untouched) when the event has none, as from an older CLI.
pub fn apply_unified_windows(snapshot: &mut Value, info: &Map<String, Value>) -> bool {
    let Some(unified) = info.get("unifiedWindows").and_then(Value::as_object) else {
        return false;
    };
    let event_window = info
        .get("rateLimitType")
        .or_else(|| info.get("rate_limit_type"))
        .and_then(Value::as_str);
    let now = chrono::Utc::now().to_rfc3339();
    let mut applied = false;
    for (raw, live) in unified {
        let Some(key) = plan_window_key(raw) else {
            continue;
        };
        let Some(fraction) = live.get("utilization").and_then(Value::as_f64) else {
            continue;
        };
        // The event-level status describes the event's own window; use it when
        // the window lists none, as the pre-RT-31 path did.
        let status = live
            .get("status")
            .or_else(|| (event_window == Some(raw.as_str())).then(|| info.get("status")).flatten())
            .and_then(Value::as_str)
            .map(plan_status_severity);
        if !snapshot.get("windows").is_some_and(Value::is_object) {
            snapshot["windows"] = json!({});
        }
        let entry = &mut snapshot["windows"][key];
        if !entry.is_object() {
            *entry = json!({ "utilization": null, "resets_at": null, "status": null, "status_at": null });
        }
        // Round to hundredths: 0.29 * 100 is 28.999999999999996.
        entry["utilization"] = json!((fraction * 10_000.0).round() / 100.0);
        if let Some(resets_at) = live.get("resetsAt").and_then(plan_reset_to_string) {
            entry["resets_at"] = json!(resets_at);
        }
        if let Some(status) = status {
            entry["status"] = json!(status);
            entry["status_at"] = json!(now);
        }
        applied = true;
    }
    if applied {
        snapshot["rate_limits_available"] = json!(true);
        snapshot["captured_at"] = json!(now);
    }
    applied
}
