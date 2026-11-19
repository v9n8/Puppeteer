//! What a conductor falls back to when nothing more specific was chosen, read
//! from Settings › New sessions each time it is needed (never cached), so a
//! change applies to the next conductor without a restart.
//!
//! - [`max_workers`]: the worker cap a conductor is created with. Only a new
//!   `conductor_sessions` row takes it; an existing conductor keeps its cap and
//!   so its framing byte for byte (the cap is part of it).
//!
//! The keys are portable (`commands::settings::PORTABLE_SETTING_KEYS`) and a
//! Remote Control controller may set them, like `default_effort`: they lower
//! cost, they are not a safety limit of an unattended turn.

use crate::db::Db;

use super::DEFAULT_MAX_WORKERS;

/// Setting: the worker cap of a new conductor, a whole number 1–100.
pub const MAX_WORKERS_SETTING: &str = "default_max_workers";
/// Bounds for the worker cap, mirrored by the composer / sidebar / Settings
/// number inputs (`src/lib/workerCap.ts`).
pub const MIN_MAX_WORKERS: u32 = 1;
pub const MAX_MAX_WORKERS: u32 = 100;

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// A cap the user asked for, kept within [`MIN_MAX_WORKERS`]–[`MAX_MAX_WORKERS`].
pub fn clamp_max_workers(value: i64) -> usize {
    value.clamp(MIN_MAX_WORKERS as i64, MAX_MAX_WORKERS as i64) as usize
}

/// The setting's value: unset, blank or not a whole number → [`DEFAULT_MAX_WORKERS`].
fn parse_max_workers(value: Option<&str>) -> usize {
    value
        .map(str::trim)
        .and_then(|v| v.parse::<i64>().ok())
        .map(clamp_max_workers)
        .unwrap_or(DEFAULT_MAX_WORKERS)
}

/// The worker cap a new conductor gets when its launch names none.
pub async fn max_workers(db: &Db) -> usize {
    parse_max_workers(setting(db, MAX_WORKERS_SETTING).await.as_deref())
}

/// The cap a launch starts with: the one it names (clamped), else the setting.
pub async fn initial_max_workers(db: &Db, requested: Option<u32>) -> usize {
    match requested {
        Some(m) => clamp_max_workers(m as i64),
        None => max_workers(db).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::set_setting;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    #[test]
    fn the_cap_setting_is_clamped_and_falls_back_to_three() {
        assert_eq!(DEFAULT_MAX_WORKERS, 3);
        assert_eq!(parse_max_workers(None), 3);
        assert_eq!(parse_max_workers(Some("")), 3);
        assert_eq!(parse_max_workers(Some("many")), 3);
        assert_eq!(parse_max_workers(Some("2.5")), 3);
        assert_eq!(parse_max_workers(Some(" 5 ")), 5);
        assert_eq!(parse_max_workers(Some("0")), 1);
        assert_eq!(parse_max_workers(Some("-4")), 1);
        assert_eq!(parse_max_workers(Some("500")), 100);
    }

    #[tokio::test]
    async fn the_cap_is_read_on_every_launch() {
        let db = db().await;
        assert_eq!(max_workers(&db).await, 3);
        assert_eq!(initial_max_workers(&db, None).await, 3);
        set_setting(&db, MAX_WORKERS_SETTING, "5").await.unwrap();
        assert_eq!(initial_max_workers(&db, None).await, 5);
        // A launch that names its cap keeps it, within bounds.
        assert_eq!(initial_max_workers(&db, Some(8)).await, 8);
        assert_eq!(initial_max_workers(&db, Some(0)).await, 1);
        assert_eq!(initial_max_workers(&db, Some(500)).await, 100);
    }

    #[test]
    fn the_cap_key_is_portable_and_remote_may_set_it() {
        assert!(crate::commands::settings::is_portable_setting(MAX_WORKERS_SETTING));
        assert!(crate::remote::guard::remote_may_set_setting(MAX_WORKERS_SETTING));
    }
}
