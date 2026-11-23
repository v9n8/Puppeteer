//! App logs: `<app data>/logs/puppeteer.log.<YYYY-MM-DD>`, one file per UTC day
//! (`tracing_appender::rolling::daily`, `lib.rs`). A day older than N goes;
//! today's file, which the app still writes, never does.

use super::{fs_safe, Category, Ctx, Item, CAT_APP_LOGS};
use chrono::NaiveDate;
use std::path::Path;

const PREFIX: &str = "puppeteer.log.";

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let dir = ctx.app_data.join("logs");
    let (today, days) = (ctx.now.date_naive(), ctx.days);
    tokio::task::spawn_blocking(move || plan_in(&dir, today, days)).await.map_err(|e| e.to_string())
}

fn plan_in(dir: &Path, today: NaiveDate, days: u32) -> Category {
    let mut category = Category::new(CAT_APP_LOGS);
    let Ok(entries) = std::fs::read_dir(dir) else { return category };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let Some(date) = entry.file_name().to_str().and_then(log_date) else { continue };
        if date >= today || (today - date).num_days() <= i64::from(days) {
            continue;
        }
        let path = entry.path();
        let size = fs_safe::size_of(&path);
        category.items.push(Item::path(dir, path, size));
    }
    category
}

/// The day in a rolled log's name; `None` for any other file.
fn log_date(name: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(name.strip_prefix(PREFIX)?, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::tests::picked_name;
    use crate::storage_cleanup::fs_safe::tests::{temp_dir, write};

    #[test]
    fn only_a_log_older_than_n_days_goes() {
        let dir = temp_dir("logs");
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        for name in [
            "puppeteer.log.2026-11-23",
            "puppeteer.log.2026-10-24",
            "puppeteer.log.2026-10-23",
            "puppeteer.log.2025-01-01",
            "puppeteer.log",
            "puppeteer.log.backup",
            "other.log.2020-01-01",
            "puppeteer.log.2099-01-01",
        ] {
            write(&dir.join(name));
        }
        let category = plan_in(&dir, today, 30);
        let mut picked: Vec<String> = category.items.iter().map(picked_name).collect();
        picked.sort();
        assert_eq!(picked, ["puppeteer.log.2025-01-01", "puppeteer.log.2026-10-23"]);
        assert_eq!(category.summary().bytes, 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_logs_folder_plans_nothing() {
        let dir = temp_dir("logs-missing").join("logs");
        assert!(plan_in(&dir, NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(), 30).items.is_empty());
    }
}
