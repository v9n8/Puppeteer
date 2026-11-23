//! RT-15 (#15): a Claude plan-usage snapshot is only replaced by a newer one.
//!
//! Two writer families fill the same `plan_usage[:<account>]` key: the idle
//! probe (`stats::refresh_plan_usage`) and live runs (`_puppeteer_usage`, the
//! `system/init` rate limits). Each used to write last-wins, stamped with the
//! time it wrote, so a slow read finishing late put an older % back. Each
//! snapshot now carries `fetched_at` — when its numbers were asked for — and a
//! write whose numbers were asked for before the stored ones is dropped. Every
//! decision is logged at debug (source, run / probe, 5 h %) so the next stale
//! number can be traced.
//!
//! RT-31: within one window the % only grows, yet `/usage` can answer with a %
//! hours old. So per window, a stored % is kept over a lower one for the same
//! reset, and over any from an earlier reset (an older window).

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Who produced a snapshot, and when its numbers were asked for.
pub struct Origin<'a> {
    /// `run` (a live session's `/usage`), `run_init` (its `system/init`),
    /// `run_event` (its `rate_limit_event`) or `probe`.
    pub source: &'static str,
    /// The run id, or the probed account.
    pub id: Option<&'a str>,
    pub fetched_at: DateTime<Utc>,
}

impl<'a> Origin<'a> {
    /// Numbers that arrived just now (a live session's event).
    pub fn now(source: &'static str, id: Option<&'a str>) -> Self {
        Origin { source, id, fetched_at: Utc::now() }
    }
}

/// When the stored snapshot's numbers were asked for: its `fetched_at`, else
/// (written before RT-15) its `captured_at`.
fn fetched_at(snapshot: &Value) -> Option<DateTime<Utc>> {
    let field = |k: &str| {
        snapshot
            .get(k)
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc))
    };
    field("fetched_at").or_else(|| field("captured_at"))
}

/// The stored snapshot holds numbers asked for after `origin`'s.
pub fn superseded(prior: Option<&Value>, origin: &Origin<'_>) -> bool {
    prior.and_then(fetched_at).is_some_and(|stored| stored > origin.fetched_at)
}

fn five_hour_percent(snapshot: &Value) -> Option<f64> {
    snapshot.pointer("/windows/five_hour/utilization").and_then(Value::as_f64)
}

/// A window's reset, to the minute: `/usage` adds fractions of a second the
/// events don't (`07:00:00.465069+00:00` vs `07:00:00`).
fn reset_minute(window: &Value) -> Option<i64> {
    let reset = window.get("resets_at").and_then(Value::as_str)?;
    Some(DateTime::parse_from_rfc3339(reset).ok()?.timestamp().div_euclid(60))
}

/// Put the stored % (and its reset) back on every window `snapshot` reads
/// lower within the same window, or from an earlier one; returns those windows.
fn keep_stored_percent(snapshot: &mut Value, prior: Option<&Value>) -> Vec<String> {
    let Some(prior_windows) = prior.and_then(|p| p.get("windows")).and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(windows) = snapshot.get_mut("windows").and_then(Value::as_object_mut) else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    for (key, fresh) in windows.iter_mut() {
        let Some(stored) = prior_windows.get(key) else {
            continue;
        };
        let Some(stored_percent) = stored.get("utilization").and_then(Value::as_f64) else {
            continue;
        };
        let (Some(stored_reset), Some(fresh_reset)) = (reset_minute(stored), reset_minute(fresh)) else {
            continue;
        };
        let fresh_percent = fresh.get("utilization").and_then(Value::as_f64);
        let stale = fresh_reset < stored_reset
            || (fresh_reset == stored_reset && fresh_percent.is_none_or(|p| p < stored_percent));
        if stale {
            fresh["utilization"] = stored["utilization"].clone();
            fresh["resets_at"] = stored["resets_at"].clone();
            kept.push(key.clone());
        }
    }
    kept
}

/// Write `snapshot` under `key` unless `prior` is newer; true when written.
pub async fn write(
    db: &sqlx::SqlitePool,
    key: &str,
    mut snapshot: Value,
    prior: Option<&Value>,
    origin: &Origin<'_>,
) -> bool {
    if superseded(prior, origin) {
        tracing::debug!(
            event = "plan_usage_skipped_older",
            key = %key,
            source = origin.source,
            id = origin.id.unwrap_or(""),
            fetched_at = %origin.fetched_at.to_rfc3339(),
            five_hour = ?five_hour_percent(&snapshot),
            stored_five_hour = ?prior.and_then(five_hour_percent),
        );
        return false;
    }
    let kept = keep_stored_percent(&mut snapshot, prior);
    if !kept.is_empty() {
        tracing::debug!(
            event = "plan_usage_kept_stored_percent",
            key = %key,
            source = origin.source,
            id = origin.id.unwrap_or(""),
            windows = ?kept,
        );
    }
    snapshot["fetched_at"] = Value::String(origin.fetched_at.to_rfc3339());
    let written = sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(snapshot.to_string())
        .execute(db)
        .await
        .is_ok();
    tracing::debug!(
        event = "plan_usage_written",
        key = %key,
        source = origin.source,
        id = origin.id.unwrap_or(""),
        fetched_at = %origin.fetched_at.to_rfc3339(),
        five_hour = ?five_hour_percent(&snapshot),
        written,
    );
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::sidecar::{persist_plan_init_rate_limits, persist_plan_rate_limit_event, persist_plan_usage};
    use serde_json::json;

    async fn db() -> sqlx::SqlitePool {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    fn usage(five_hour: f64) -> Value {
        json!({
            "rate_limits_available": true,
            "rate_limits": { "five_hour": { "utilization": five_hour, "resets_at": "2026-11-20T10:00:00Z" } },
        })
    }

    fn init(five_hour: f64) -> Value {
        json!({
            "type": "system", "subtype": "init",
            "rate_limits": { "five_hour": { "utilization": five_hour } },
        })
    }

    async fn stored(db: &sqlx::SqlitePool) -> Option<f64> {
        let raw: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'plan_usage'").fetch_one(db).await.unwrap();
        five_hour_percent(&serde_json::from_str(&raw).unwrap())
    }

    #[tokio::test]
    async fn a_read_asked_for_before_the_stored_one_never_replaces_it() {
        let db = db().await;
        let t0 = Utc::now() - chrono::Duration::seconds(10);
        let t1 = t0 + chrono::Duration::seconds(5);
        let probe = Origin { source: "probe", id: None, fetched_at: t0 };
        let run = Origin { source: "run", id: Some("r1"), fetched_at: t1 };

        // The run's newer read lands first; the probe started earlier and
        // finishes late with an older number.
        assert!(persist_plan_usage(&db, "plan_usage", &usage(40.0), &run).await);
        assert!(!persist_plan_usage(&db, "plan_usage", &usage(34.0), &probe).await);
        assert_eq!(stored(&db).await, Some(40.0));
        assert!(!persist_plan_init_rate_limits(&db, "plan_usage", &init(34.0), &probe).await);
        assert_eq!(stored(&db).await, Some(40.0));

        // A newer read still replaces it, from either writer.
        let later = Origin { source: "run_init", id: Some("r2"), fetched_at: t1 + chrono::Duration::seconds(1) };
        assert!(persist_plan_init_rate_limits(&db, "plan_usage", &init(41.0), &later).await);
        assert_eq!(stored(&db).await, Some(41.0));
        // Same instant: not older, written.
        assert!(persist_plan_usage(&db, "plan_usage", &usage(42.0), &later).await);
        assert_eq!(stored(&db).await, Some(42.0));
    }

    // The 5 h window ending 2026-11-23T07:00:00Z, as events and `/usage` write it.
    const RESET_EPOCH: i64 = 1_795_417_200;
    const RESET_USAGE: &str = "2026-11-23T07:00:00.465069+00:00";

    fn usage_at(five_hour: f64, resets_at: &str) -> Value {
        json!({
            "rate_limits_available": true,
            "rate_limits": { "five_hour": { "utilization": five_hour, "resets_at": resets_at } },
        })
    }

    fn event(unified: Option<Value>) -> Value {
        let mut info = json!({ "status": "allowed", "resetsAt": RESET_EPOCH, "rateLimitType": "five_hour" });
        if let Some(unified) = unified {
            info["unifiedWindows"] = unified;
        }
        json!({ "type": "rate_limit_event", "rate_limit_info": info })
    }

    fn live_event(five_hour: f64) -> Value {
        event(Some(json!({
            "five_hour": { "utilization": five_hour, "resetsAt": RESET_EPOCH, "status": "allowed" },
            "seven_day": { "utilization": 0.39, "resetsAt": 1_795_435_200, "status": "allowed" },
        })))
    }

    async fn snapshot(db: &sqlx::SqlitePool) -> Value {
        let raw: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'plan_usage'").fetch_one(db).await.unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn reset(epoch: i64) -> Value {
        json!(DateTime::<Utc>::from_timestamp(epoch, 0).unwrap().to_rfc3339())
    }

    #[tokio::test]
    async fn a_rate_limit_event_stores_each_windows_live_percent() {
        let db = db().await;
        let origin = Origin::now("run_event", Some("r1"));
        assert!(persist_plan_rate_limit_event(&db, "plan_usage", &live_event(0.62), &origin).await);
        let s = snapshot(&db).await;
        assert_eq!(s.pointer("/windows/five_hour/utilization"), Some(&json!(62.0)));
        assert_eq!(s.pointer("/windows/five_hour/resets_at"), Some(&reset(RESET_EPOCH)));
        assert_eq!(s.pointer("/windows/five_hour/status"), Some(&json!("allowed")));
        assert_eq!(s.pointer("/windows/seven_day/utilization"), Some(&json!(39.0)));
        assert_eq!(s.pointer("/windows/seven_day/resets_at"), Some(&reset(1_795_435_200)));
        assert_eq!(s["fetched_at"], json!(origin.fetched_at.to_rfc3339()));
        assert_eq!(s["rate_limits_available"], json!(true));
    }

    #[tokio::test]
    async fn a_lower_percent_for_the_same_window_keeps_the_stored_one() {
        let db = db().await;
        assert!(persist_plan_rate_limit_event(&db, "plan_usage", &live_event(0.62), &Origin::now("run_event", Some("r1"))).await);
        // `/usage` at turn end still answers the 11% it read hours ago.
        assert!(persist_plan_usage(&db, "plan_usage", &usage_at(11.0, RESET_USAGE), &Origin::now("run", Some("r1"))).await);
        assert_eq!(stored(&db).await, Some(62.0));
        // ...and so does one about the window before.
        let older = usage_at(80.0, "2026-11-23T02:00:00.1+00:00");
        assert!(persist_plan_usage(&db, "plan_usage", &older, &Origin::now("run", Some("r1"))).await);
        assert_eq!(stored(&db).await, Some(62.0));
        assert_eq!(snapshot(&db).await.pointer("/windows/five_hour/resets_at"), Some(&reset(RESET_EPOCH)));
        // A higher % for the same window still lands.
        assert!(persist_plan_usage(&db, "plan_usage", &usage_at(64.0, RESET_USAGE), &Origin::now("run", Some("r1"))).await);
        assert_eq!(stored(&db).await, Some(64.0));
    }

    #[tokio::test]
    async fn a_new_window_takes_its_lower_percent() {
        let db = db().await;
        assert!(persist_plan_rate_limit_event(&db, "plan_usage", &live_event(0.62), &Origin::now("run_event", Some("r1"))).await);
        let next = usage_at(3.0, "2026-11-23T12:00:00.2+00:00");
        assert!(persist_plan_usage(&db, "plan_usage", &next, &Origin::now("probe", None)).await);
        assert_eq!(stored(&db).await, Some(3.0));
    }

    #[tokio::test]
    async fn an_event_without_unified_windows_only_moves_status_and_reset() {
        let db = db().await;
        assert!(persist_plan_usage(&db, "plan_usage", &usage_at(40.0, RESET_USAGE), &Origin::now("run", Some("r1"))).await);
        let before = snapshot(&db).await;
        assert!(persist_plan_rate_limit_event(&db, "plan_usage", &event(None), &Origin::now("run_event", Some("r1"))).await);
        let after = snapshot(&db).await;
        assert_eq!(after.pointer("/windows/five_hour/utilization"), Some(&json!(40.0)));
        assert_eq!(after.pointer("/windows/five_hour/resets_at"), Some(&reset(RESET_EPOCH)));
        assert_eq!(after.pointer("/windows/five_hour/status"), Some(&json!("allowed")));
        assert_eq!(after["captured_at"], before["captured_at"]);
        assert_eq!(after["fetched_at"], before["fetched_at"]);
    }

    #[test]
    fn a_snapshot_from_before_rt15_is_dated_by_its_capture() {
        let t = Utc::now();
        let old = json!({ "captured_at": (t + chrono::Duration::seconds(1)).to_rfc3339() });
        let origin = Origin { source: "probe", id: None, fetched_at: t };
        assert!(superseded(Some(&old), &origin));
        assert!(!superseded(Some(&json!({})), &origin));
        assert!(!superseded(None, &origin));
    }
}
