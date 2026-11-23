//! The automatic cleanup: an hourly check while the app runs, which cleans at
//! most once a day. A day that passed while the app was closed is made up for
//! after the next start, never within its first two minutes.

use super::commands::{read_last, run, BUSY};
use super::{days, FEATURE};
use chrono::{DateTime, Utc};
use std::time::Duration;

const FIRST_TICK: Duration = Duration::from_secs(120);
const TICK: Duration = Duration::from_secs(60 * 60);
const EVERY: chrono::TimeDelta = chrono::TimeDelta::hours(24);

/// Whether an automatic cleanup is due. A last run dated after `now` (the
/// clock was set back) does not hold it off.
pub fn should_run(now: DateTime<Utc>, last: Option<DateTime<Utc>>, days: u32, feature_on: bool) -> bool {
    feature_on && days > 0 && last.is_none_or(|at| at > now || now - at >= EVERY)
}

/// Start the hourly check (app setup).
pub fn start(app: tauri::AppHandle, db: crate::db::Db) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_TICK).await;
        loop {
            tick(&app, &db).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &tauri::AppHandle, db: &crate::db::Db) {
    let days = days(db).await;
    let last = read_last(db)
        .await
        .and_then(|l| DateTime::parse_from_rfc3339(&l.at).ok())
        .map(|at| at.with_timezone(&Utc));
    if !should_run(Utc::now(), last, days, crate::features::is_on(db, FEATURE).await) {
        return;
    }
    match run(app, db, days, "auto").await {
        Ok(_) => {}
        // A manual cleanup is running; the next tick looks again.
        Err(e) if e == BUSY => {}
        Err(e) => tracing::warn!(event = "storage_cleanup_auto_failed", error = %e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_once_a_day_only_when_on() {
        let now = DateTime::parse_from_rfc3339("2026-11-23T12:00:00Z").unwrap().with_timezone(&Utc);
        let hours_ago = |h: i64| Some(now - chrono::TimeDelta::hours(h));
        assert!(!should_run(now, None, 30, false), "switch off");
        assert!(!should_run(now, None, 0, true), "N = 0");
        assert!(!should_run(now, hours_ago(23), 30, true), "less than a day ago");
        assert!(should_run(now, hours_ago(24), 30, true), "a day ago");
        assert!(should_run(now, hours_ago(72), 30, true), "the app was closed");
        assert!(should_run(now, None, 30, true), "never ran");
        assert!(should_run(now, hours_ago(-5), 30, true), "last run in the future");
    }
}
