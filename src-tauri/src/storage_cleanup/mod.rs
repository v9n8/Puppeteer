//! Storage cleanup: files the app (or a CLI it started) left behind and nothing
//! uses any more, removed after N days. Each kind is cleaned by the code that
//! owns it (a Trash run by the purge flow, a worktree by `conductor::worktree`,
//! snapshots by their own sweep); only orphans that nothing owns are deleted
//! directly, and always through `fs_safe`.
//!
//! N and the last cleanup's record are per machine: local-only for Remote
//! Control (`remote::guard`) and left out of a settings backup.

use crate::db::Db;

/// Age limit in days, as a whole number; "0" = auto-clean off.
pub const DAYS_SETTING: &str = "storage_cleanup_days";
/// The last cleanup as JSON: `{at, trigger, files, bytes, errors}`.
pub const LAST_SETTING: &str = "storage_cleanup_last";
/// Switch of the automatic cleanup (a manual one stays on the Stats page).
pub const FEATURE: &str = "app.storage_cleanup";
pub const DEFAULT_DAYS: u32 = 30;
const MIN_DAYS: u32 = 7;
const MAX_DAYS: u32 = 3650;

/// A stored N: "0" (off) or 7–3650; unset, blank or anything else → the default.
fn parse_days(value: Option<&str>) -> u32 {
    match value.map(str::trim).and_then(|v| v.parse::<u32>().ok()) {
        Some(0) => 0,
        Some(n) if (MIN_DAYS..=MAX_DAYS).contains(&n) => n,
        _ => DEFAULT_DAYS,
    }
}

/// N as stored on this machine (0 = auto-clean off).
pub async fn days(db: &Db) -> u32 {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(DAYS_SETTING)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    parse_days(value.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_takes_zero_or_the_valid_range_else_the_default() {
        for (value, want) in [
            (None, 30),
            (Some(""), 30),
            (Some("abc"), 30),
            (Some("3"), 30),
            (Some("-1"), 30),
            (Some("0"), 0),
            (Some(" 7 "), 7),
            (Some("3650"), 3650),
            (Some("5000"), 30),
        ] {
            assert_eq!(parse_days(value), want, "{value:?}");
        }
    }

    #[tokio::test]
    async fn days_reads_the_stored_value() {
        let dir = std::env::temp_dir().join(format!("puppeteer-cleanup-days-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::init_db(&dir.join("data.db")).await.unwrap();
        assert_eq!(days(&db).await, DEFAULT_DAYS);
        crate::commands::settings::set_setting(&db, DAYS_SETTING, "60").await.unwrap();
        assert_eq!(days(&db).await, 60);
        db.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_remote_controller_cannot_set_the_cleanup_keys() {
        assert!(!crate::remote::guard::remote_may_set_setting(DAYS_SETTING));
        assert!(!crate::remote::guard::remote_may_set_setting(LAST_SETTING));
        let (module, prop) = crate::features::entry(FEATURE).unwrap();
        assert!(!crate::remote::guard::remote_may_set_setting(&prop.unwrap().key(module)));
    }

    #[test]
    fn a_settings_backup_leaves_the_cleanup_keys_out() {
        assert!(!crate::commands::settings::is_portable_setting(DAYS_SETTING));
        assert!(!crate::commands::settings::is_portable_setting(LAST_SETTING));
    }
}
