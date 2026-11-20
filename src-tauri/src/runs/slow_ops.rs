//! Thresholds for logging DB-heavy work that ran long (RT-29).
//!
//! On 2026-11-16 the pool ran out of connections and the log could not tell
//! which work held them. sqlx already warns about a slow acquire (over 2 s,
//! `sqlx::pool::acquire`) and a slow single statement (over 1 s,
//! `sqlx::query`); these cover what it cannot see: a whole mirror sync, and a
//! write transaction made of many short statements.

use std::time::{Duration, Instant};

/// A mirror sync (parse the transcript, write the log, rewrite usage, update
/// the run) longer than this is logged. Parsing a 21.8 MB transcript took
/// about 360 ms in a debug build, so a pass over 1 s most likely waited on the
/// DB pool or on a busy runtime.
pub const SLOW_MIRROR_SYNC: Duration = Duration::from_secs(1);

/// A write transaction kept open longer than this is logged: with SQLite's
/// default rollback journal it blocks every other writer for that long (and
/// readers while it commits).
pub const SLOW_WRITE_TX: Duration = Duration::from_secs(1);

/// Milliseconds since `started` when that is over `limit`, else `None`.
pub fn over(started: Instant, limit: Duration) -> Option<u64> {
    let elapsed = started.elapsed();
    (elapsed > limit).then(|| elapsed.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_past_the_limit() {
        let now = Instant::now();
        assert_eq!(over(now, Duration::from_secs(60)), None);
        let earlier = now - Duration::from_millis(1500);
        assert!(over(earlier, SLOW_MIRROR_SYNC).is_some_and(|ms| ms >= 1500));
    }
}
