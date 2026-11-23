//! Storage cleanup: files the app (or a CLI it started) left behind and nothing
//! uses any more, removed after N days. Each kind is cleaned by the code that
//! owns it (a Trash run by the purge flow, a worktree by `conductor::worktree`,
//! snapshots by their own sweep); only orphans that nothing owns are deleted
//! directly, and always through `fs_safe`.
//!
//! N and the last cleanup's record are per machine: local-only for Remote
//! Control (`remote::guard`) and left out of a settings backup.

pub mod fs_safe;
mod logs;
mod pastes;
mod tasks;

use crate::db::Db;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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

/// Category ids shared with the Storage table (`commands::storage`).
pub const CAT_APP_LOGS: &str = "app_logs";
pub const CAT_PASTES: &str = "pastes";
pub const CAT_TASK_ORPHANS: &str = "puppeteer_task_orphans";

/// Held while a cleanup deletes: an automatic and a manual one never overlap.
pub static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What one cleanup looks at.
pub struct Ctx {
    pub db: Db,
    pub app_data: PathBuf,
    pub home: Option<PathBuf>,
    pub now: DateTime<Utc>,
    /// Age limit in days, above 0 (the caller turns "off" into a default).
    pub days: u32,
}

impl Ctx {
    pub fn new(db: Db, app_data: PathBuf, days: u32) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self { db, app_data, home, now: Utc::now(), days }
    }

    /// Anything last changed before this is older than N days. A time after
    /// `now` (a clock set back) is never older.
    fn cutoff(&self) -> SystemTime {
        SystemTime::from(self.now) - Duration::from_secs(u64::from(self.days) * 86_400)
    }
}

/// How an item goes.
pub enum Target {
    /// A file or folder nothing owns, through [`fs_safe::remove_under`].
    Path { root: PathBuf, path: PathBuf },
}

impl Target {
    fn describe(&self) -> String {
        match self {
            Target::Path { path, .. } => path.display().to_string(),
        }
    }
}

pub struct Item {
    pub target: Target,
    pub bytes: u64,
    pub files: u64,
}

impl Item {
    fn path(root: &Path, path: PathBuf, (bytes, files): (u64, u64)) -> Self {
        Self { target: Target::Path { root: root.to_path_buf(), path }, bytes, files }
    }
}

/// One kind's share of a cleanup: what goes, and what is kept although old.
pub struct Category {
    pub id: &'static str,
    pub items: Vec<Item>,
    pub kept_items: u64,
    pub kept_bytes: u64,
}

impl Category {
    fn new(id: &'static str) -> Self {
        Self { id, items: Vec::new(), kept_items: 0, kept_bytes: 0 }
    }

    pub fn summary(&self) -> CategorySummary {
        CategorySummary {
            id: self.id,
            items: self.items.len() as u64,
            bytes: self.items.iter().map(|i| i.bytes).sum(),
            kept_items: self.kept_items,
            kept_bytes: self.kept_bytes,
        }
    }
}

pub struct Plan {
    pub categories: Vec<Category>,
}

#[derive(Debug, Serialize)]
pub struct CategorySummary {
    pub id: &'static str,
    pub items: u64,
    pub bytes: u64,
    pub kept_items: u64,
    pub kept_bytes: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct CategoryOutcome {
    pub id: &'static str,
    pub files: u64,
    pub bytes: u64,
    pub errors: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct CleanupOutcome {
    pub files: u64,
    pub bytes: u64,
    pub errors: u64,
    pub per_category: Vec<CategoryOutcome>,
}

/// What a cleanup with `ctx` would remove. A kind whose inputs cannot be read
/// plans nothing: an unreadable table must never look like "nothing uses it".
pub async fn plan(ctx: &Ctx) -> Plan {
    let mut categories = Vec::new();
    take(&mut categories, CAT_APP_LOGS, logs::plan(ctx).await);
    take(&mut categories, CAT_PASTES, pastes::plan(ctx).await);
    take(&mut categories, CAT_TASK_ORPHANS, tasks::plan(ctx).await);
    Plan { categories }
}

fn take(categories: &mut Vec<Category>, id: &'static str, planned: Result<Category, String>) {
    categories.push(planned.unwrap_or_else(|error| {
        tracing::warn!(event = "storage_cleanup_plan_failed", category = id, error = %error);
        Category::new(id)
    }));
}

/// Remove what `plan` picked. A failed item is counted and logged with its
/// path; the others still go.
pub async fn apply(plan: Plan) -> CleanupOutcome {
    let mut outcome = CleanupOutcome::default();
    for category in plan.categories {
        let done = apply_category(category).await;
        outcome.files += done.files;
        outcome.bytes += done.bytes;
        outcome.errors += done.errors;
        outcome.per_category.push(done);
    }
    outcome
}

async fn apply_category(category: Category) -> CategoryOutcome {
    let (id, count) = (category.id, category.items.len() as u64);
    tokio::task::spawn_blocking(move || remove_items(id, category.items))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(event = "storage_cleanup_category_failed", category = id, error = %e);
            CategoryOutcome { id, errors: count, ..Default::default() }
        })
}

fn remove_items(id: &'static str, items: Vec<Item>) -> CategoryOutcome {
    let mut done = CategoryOutcome { id, ..Default::default() };
    for item in items {
        let removed = match &item.target {
            Target::Path { root, path } => fs_safe::remove_under(root, path),
        };
        match removed {
            Ok(()) => {
                done.files += item.files;
                done.bytes += item.bytes;
            }
            Err(error) => {
                done.errors += 1;
                tracing::warn!(event = "storage_cleanup_item_failed", category = id, path = %item.target.describe(), error = %error);
            }
        }
    }
    done
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use fs_safe::tests::{backdate, days_ago, temp_dir, write};

    pub(crate) fn picked_path(item: &Item) -> PathBuf {
        match &item.target {
            Target::Path { path, .. } => path.clone(),
        }
    }

    pub(crate) fn picked_name(item: &Item) -> String {
        picked_path(item).file_name().unwrap().to_string_lossy().into_owned()
    }

    pub(crate) async fn test_db(dir: &Path) -> Db {
        crate::db::init_db(&dir.join("data.db")).await.unwrap()
    }

    /// The whole path: plan from the database and the folders, then apply.
    #[tokio::test]
    async fn a_cleanup_takes_old_orphans_and_keeps_what_a_run_uses() {
        let app_data = temp_dir("cleanup-e2e");
        let db = test_db(&app_data).await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
            .bind(app_data.join("proj").to_string_lossy().as_ref())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at, status) VALUES ('live', 'p1', 'session', 'c', 'running')")
            .execute(&db)
            .await
            .unwrap();
        for rel in ["logs/puppeteer.log.2020-01-01", "pastes/live/p.txt", "pastes/gone/p.txt", "proj/.puppeteer/tasks/t/issue-1/issue.md"] {
            write(&app_data.join(rel));
            backdate(&app_data.join(rel), days_ago(90));
        }
        for rel in ["pastes/live", "pastes/gone"] {
            backdate(&app_data.join(rel), days_ago(90));
        }
        write(&app_data.join(format!("logs/puppeteer.log.{}", Utc::now().format("%Y-%m-%d"))));

        let ctx = Ctx { home: None, ..Ctx::new(db.clone(), app_data.clone(), 30) };
        let planned = plan(&ctx).await;
        let summary: Vec<_> = planned.categories.iter().map(|c| (c.id, c.summary().items)).collect();
        assert_eq!(summary, [(CAT_APP_LOGS, 1), (CAT_PASTES, 1), (CAT_TASK_ORPHANS, 1)]);
        let outcome = apply(planned).await;
        assert_eq!((outcome.files, outcome.errors), (3, 0));
        assert!(!app_data.join("logs/puppeteer.log.2020-01-01").exists());
        assert!(!app_data.join("pastes/gone").exists());
        assert!(app_data.join("pastes/live/p.txt").exists());
        assert!(!app_data.join("proj/.puppeteer/tasks/t/issue-1/issue.md").exists());
        db.close().await;
        std::fs::remove_dir_all(&app_data).unwrap();
    }

    /// One item that cannot go does not stop the others.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_item_is_counted_and_the_rest_still_go() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir("cleanup-fail");
        write(&root.join("locked/a.log"));
        write(&root.join("b.log"));
        write(&root.join("c.log"));
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
        let mut category = Category::new(CAT_APP_LOGS);
        for rel in ["b.log", "locked/a.log", "c.log"] {
            category.items.push(Item::path(&root, root.join(rel), (1, 1)));
        }
        let outcome = apply(Plan { categories: vec![category] }).await;
        assert_eq!((outcome.files, outcome.bytes, outcome.errors), (2, 2, 1));
        assert_eq!(outcome.per_category[0].errors, 1);
        assert!(!root.join("b.log").exists() && !root.join("c.log").exists());
        assert!(root.join("locked/a.log").exists());
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }

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
        let dir = temp_dir("days");
        let db = test_db(&dir).await;
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
