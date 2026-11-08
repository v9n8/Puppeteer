//! Auto-update (`app.auto_update`) through Tauri's updater plugin.
//!
//! The release workflow publishes `latest.json` (version, archive URL and its
//! minisign signature) next to the `.app.tar.gz`; the endpoint is in
//! `tauri.conf.json` (`plugins.updater`). The plugin's own JS commands are not
//! granted to any window (`capabilities/`), so these two commands are the only
//! way in:
//!
//! - `check_update`: asks the endpoint whether a newer release exists.
//! - `install_update` (local-only): downloads it, checks the signature against
//!   `plugins.updater.pubkey` and replaces the installed app. The new version
//!   runs from the next launch; nothing is restarted here, so live runs go on.
//!
//! Neither touches the network while the prop is off, nor while the build
//! cannot update itself ([`can_update`]).

use crate::db::Db;
use crate::features;
use semver::Version;
use serde::Serialize;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Updater, UpdaterExt};

/// Prop gating both commands.
pub const FEATURE: &str = "app.auto_update";

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateCheck {
    /// This build cannot update itself; nothing was fetched.
    Unsupported,
    UpToDate,
    Available { version: String, current_version: String },
}

/// Whether `remote` is a newer release than `current`, by semver precedence:
/// build metadata (`+…`) never makes a release newer, and a pre-release ranks
/// below its release.
pub fn is_newer(current: &Version, remote: &Version) -> bool {
    remote.cmp_precedence(current).is_gt()
}

/// Whether this build can update itself: it has the key update signatures are
/// checked against (empty until the release signing key is set up), and on
/// macOS it runs from an app bundle. The plugin replaces the folder above the
/// executable's `Contents/MacOS`, or else the executable's own folder, which in
/// a dev build is `target/debug`.
pub fn can_update(pubkey: &str, exe: &Path) -> bool {
    !pubkey.trim().is_empty() && (!cfg!(target_os = "macos") || in_app_bundle(exe))
}

/// `…/<name>.app/Contents/MacOS/<binary>`.
fn in_app_bundle(exe: &Path) -> bool {
    let mut dirs = exe.ancestors().skip(1).filter_map(|p| p.file_name()).map(|n| n.to_string_lossy());
    dirs.next().as_deref() == Some("MacOS")
        && dirs.next().as_deref() == Some("Contents")
        && dirs.next().is_some_and(|n| n.ends_with(".app"))
}

/// The gate both commands share: `fetch`, the only network access, runs only
/// while the prop is on and the build can update itself.
async fn gated<T, F, Fut>(db: &Db, can_update: bool, unsupported: T, fetch: F) -> Result<T, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    features::require(db, FEATURE).await?;
    if !can_update {
        return Ok(unsupported);
    }
    fetch().await
}

fn this_build_can_update(app: &AppHandle) -> bool {
    let pubkey = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .unwrap_or_default();
    std::env::current_exe().is_ok_and(|exe| can_update(pubkey, &exe))
}

fn updater(app: &AppHandle) -> Result<Updater, String> {
    app.updater_builder()
        .version_comparator(|current, release| is_newer(&current, &release.version))
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_update(app: AppHandle, db: State<'_, Db>) -> Result<UpdateCheck, String> {
    gated(db.inner(), this_build_can_update(&app), UpdateCheck::Unsupported, || async {
        Ok(match updater(&app)?.check().await.map_err(|e| e.to_string())? {
            Some(u) => UpdateCheck::Available { version: u.version, current_version: u.current_version },
            None => UpdateCheck::UpToDate,
        })
    })
    .await
}

/// Set while an install runs: two at once would both replace the app bundle.
static INSTALLING: AtomicBool = AtomicBool::new(false);

struct InstallLock;

impl InstallLock {
    fn take() -> Result<Self, String> {
        if INSTALLING.swap(true, Ordering::SeqCst) {
            return Err("update_in_progress".into());
        }
        Ok(Self)
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// Installs the release the endpoint serves now (checked again, so it is never
/// older than what is running) and returns its version.
#[tauri::command]
pub async fn install_update(app: AppHandle, db: State<'_, Db>) -> Result<String, String> {
    let _lock = InstallLock::take()?;
    gated(db.inner(), this_build_can_update(&app), None, || async {
        let update = updater(&app)?.check().await.map_err(|e| e.to_string())?.ok_or("update_not_available")?;
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
        Ok(Some(update.version))
    })
    .await?
    .ok_or_else(|| "update_unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    async fn db_with(pairs: &[(&str, &str)]) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for (key, value) in pairs {
            crate::commands::settings::set_setting(&db, key, value).await.unwrap();
        }
        db
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn newer_follows_semver_precedence() {
        assert!(is_newer(&v("0.13.0"), &v("0.13.1")));
        assert!(is_newer(&v("0.13.9"), &v("0.14.0")));
        assert!(is_newer(&v("0.9.0"), &v("0.10.0")), "numeric, not text");
        assert!(is_newer(&v("0.99.99"), &v("1.0.0")));
        assert!(!is_newer(&v("0.13.0"), &v("0.13.0")));
        assert!(!is_newer(&v("0.14.0"), &v("0.13.9")), "no downgrade");
        assert!(!is_newer(&v("0.13.0"), &v("0.13.0+build.7")), "build metadata is not a release");
        assert!(is_newer(&v("0.13.0"), &v("0.14.0-rc.1")));
        assert!(!is_newer(&v("0.14.0"), &v("0.14.0-rc.1")), "a pre-release is older than its release");
        assert!(is_newer(&v("0.14.0-rc.1"), &v("0.14.0")));
    }

    #[test]
    fn only_a_keyed_build_from_an_app_bundle_can_update() {
        let bundled = Path::new("/Applications/Puppeteer.app/Contents/MacOS/puppeteer");
        let dev = Path::new("/repo/src-tauri/target/debug/puppeteer");
        assert!(!can_update("", bundled));
        assert!(!can_update("  \n", bundled));
        assert!(can_update("dW50cnVzdGVk", bundled));
        assert_eq!(can_update("dW50cnVzdGVk", dev), !cfg!(target_os = "macos"));

        assert!(in_app_bundle(bundled));
        assert!(in_app_bundle(Path::new("/Users/me/Apps/Puppeteer 2.app/Contents/MacOS/puppeteer")));
        for exe in [dev, Path::new("/x/Contents/MacOS/puppeteer"), Path::new("/x.app/MacOS/puppeteer"), Path::new("puppeteer")] {
            assert!(!in_app_bundle(exe), "{}", exe.display());
        }
    }

    /// Prop off: refused before any network access, whatever the build.
    #[tokio::test]
    async fn the_prop_off_checks_nothing() {
        let fetched = Cell::new(0);
        let fetch = || async {
            fetched.set(fetched.get() + 1);
            Ok(UpdateCheck::UpToDate)
        };
        let off = Err(format!("feature_disabled: {FEATURE}"));

        let db = db_with(&[("feature.app.auto_update", "false")]).await;
        assert_eq!(gated(&db, true, UpdateCheck::Unsupported, fetch).await, off);
        let db = db_with(&[("feature.app", "false")]).await;
        assert_eq!(gated(&db, true, UpdateCheck::Unsupported, fetch).await, off);
        assert_eq!(fetched.get(), 0);

        // On by default; a build that cannot update fetches nothing either.
        let db = db_with(&[]).await;
        assert_eq!(gated(&db, false, UpdateCheck::Unsupported, fetch).await, Ok(UpdateCheck::Unsupported));
        assert_eq!(fetched.get(), 0);
        assert_eq!(gated(&db, true, UpdateCheck::Unsupported, fetch).await, Ok(UpdateCheck::UpToDate));
        assert_eq!(fetched.get(), 1);
    }

    #[test]
    fn one_install_at_a_time() {
        let first = InstallLock::take().unwrap();
        assert_eq!(InstallLock::take().err().as_deref(), Some("update_in_progress"));
        drop(first);
        drop(InstallLock::take().unwrap());
    }

    /// The plugin reads this block at startup and refuses to load without it;
    /// the release workflow reads the pubkey from it too.
    #[test]
    fn the_updater_config_parses() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let updater = &conf["plugins"]["updater"];
        assert!(updater["pubkey"].is_string());
        let config: tauri_plugin_updater::Config = serde_json::from_value(updater.clone()).unwrap();
        assert!(!config.endpoints.is_empty());
        assert!(config.endpoints.iter().all(|u| u.scheme() == "https" && u.path().ends_with("/latest.json")));
    }

    #[test]
    fn check_result_is_tagged_for_the_frontend() {
        let available = UpdateCheck::Available { version: "0.14.0".into(), current_version: "0.13.0".into() };
        assert_eq!(
            serde_json::to_value(&available).unwrap(),
            serde_json::json!({ "status": "available", "version": "0.14.0", "current_version": "0.13.0" })
        );
        assert_eq!(serde_json::to_value(UpdateCheck::UpToDate).unwrap(), serde_json::json!({ "status": "up_to_date" }));
    }
}
