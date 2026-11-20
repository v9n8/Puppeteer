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

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Who produced a snapshot, and when its numbers were asked for.
pub struct Origin<'a> {
    /// `run` (a live session's `/usage`), `run_init` (its `system/init`) or `probe`.
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
    use crate::runs::sidecar::{persist_plan_init_rate_limits, persist_plan_usage};
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
