//! The discovery commands of the MCP views, and the in-memory cache behind them.
//!
//! A scan has two halves: the file sources (a few small reads, run off the async threads) and the
//! engine probes (one CLI per Claude profile, ~12 s). The files are read first and shown at once;
//! the probes follow in the background (NFR-5) and [`CHANGED_EVENT`] tells the views to list
//! again. Triggers: app start ([`start`], never waited on), a project added (files only) and the
//! reload button. The cache keeps rows only: a scanned export (it holds secret VALUEs) is dropped
//! at once, and import / sync scan again.
//!
//! All local-only (`remote::guard`): they read this machine's tool configs.

use super::claude_sources::GLOBAL_PROFILE;
use super::probe::{self, ProbedServer, ProfileIdentity};
use super::state::{self, DiscoveryView, ViewInputs};
use super::{import_sync, scan_files, DiscoveryInputs, DiscoveryReport, Scan};
use crate::commands::mcp::McpServer;
use crate::import::apply::{Outcome, Status};
use std::collections::HashMap;
use crate::db::Db;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

/// Emitted when the cache changed (a scan or the probes finished): `{ probing: bool }`. The views
/// list again; nothing else is in the payload.
pub const CHANGED_EVENT: &str = "mcp_discovery:changed";

#[derive(Default)]
struct Cached {
    /// `None` before the first file scan.
    files: Option<DiscoveryReport>,
    identities: Vec<ProfileIdentity>,
    scanned_at: Option<String>,
    probes: DiscoveryReport,
    probed_at: Option<String>,
    probing: bool,
    /// A reload asked for probes while some were running: run once more after them.
    probe_again: bool,
}

/// What a view is built from, copied out of the lock.
struct Snapshot {
    report: DiscoveryReport,
    identities: Vec<ProfileIdentity>,
    scanned_at: Option<String>,
    probed_at: Option<String>,
    probing: bool,
}

/// App state: the last scan of every source.
#[derive(Default)]
pub struct DiscoveryCache(Mutex<Cached>);

impl DiscoveryCache {
    fn lock(&self) -> MutexGuard<'_, Cached> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn has_files(&self) -> bool {
        self.lock().files.is_some()
    }

    fn set_files(&self, report: DiscoveryReport, identities: Vec<ProfileIdentity>, at: String) {
        let mut cached = self.lock();
        cached.files = Some(report);
        cached.identities = identities;
        cached.scanned_at = Some(at);
    }

    /// `true`: the caller runs the probes; `false`: they are running and will run once more.
    fn begin_probe(&self) -> bool {
        let mut cached = self.lock();
        if cached.probing {
            cached.probe_again = true;
            return false;
        }
        cached.probing = true;
        true
    }

    /// Keep a probe's rows. `true`: run the probes again (a reload came in meanwhile).
    fn end_probe(&self, report: DiscoveryReport, at: String) -> bool {
        let mut cached = self.lock();
        cached.probes = report;
        cached.probed_at = Some(at);
        if cached.probe_again {
            cached.probe_again = false;
            return true;
        }
        cached.probing = false;
        false
    }

    fn snapshot(&self) -> Snapshot {
        let cached = self.lock();
        let mut report = cached.files.clone().unwrap_or_default();
        report.servers.extend(cached.probes.servers.iter().cloned());
        report.problems.extend(cached.probes.problems.iter().cloned());
        Snapshot {
            report,
            identities: cached.identities.clone(),
            scanned_at: cached.scanned_at.clone(),
            probed_at: cached.probed_at.clone(),
            probing: cached.probing,
        }
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn notify(app: &AppHandle) {
    let probing = app.try_state::<DiscoveryCache>().is_some_and(|c| c.lock().probing);
    let _ = app.emit(CHANGED_EVENT, serde_json::json!({ "probing": probing }));
}

/// Read every file source (and the profiles' logins, for C1 / C2 / C4). The scan holds the
/// exports: keep it only as long as an import or sync needs it.
async fn fresh_scan(app: &AppHandle, db: &Db) -> Result<(Scan, Vec<ProfileIdentity>), String> {
    let inputs = DiscoveryInputs::load(app, db).await?;
    let (scan, identities) = tokio::task::spawn_blocking(move || (scan_files(&inputs), probe::compare_identities(&inputs)))
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(event = "mcp_discovery_scan", servers = scan.found.len(), problems = scan.problems.len());
    Ok((scan, identities))
}

/// Read the file sources again into the cache.
async fn scan_files_now(app: &AppHandle, db: &Db, cache: &DiscoveryCache) -> Result<(), String> {
    let (scan, identities) = fresh_scan(app, db).await?;
    cache.set_files(scan.into_report(), identities, now());
    Ok(())
}

/// The probes of every Claude profile. When they cannot even be started, every profile fails:
/// its stored connectors are dropped, so none can enter a run (FR-5).
async fn probe_once(app: &AppHandle) -> DiscoveryReport {
    let db = app.state::<Db>().inner().clone();
    let inputs = match DiscoveryInputs::load(app, &db).await {
        Ok(inputs) => inputs,
        Err(e) => {
            tracing::warn!(event = "mcp_probe_inputs_failed", error = %e);
            return DiscoveryReport::default();
        }
    };
    match probe::spawn_settings(app, &db).await {
        Ok(spawn) => probe::probe_profiles(&db, &spawn, &inputs).await.into_report(),
        Err(e) => {
            let failed: Result<Vec<ProbedServer>, String> = Err(format!("the MCP probe could not start: {e}"));
            let mut profiles = vec![GLOBAL_PROFILE.to_string()];
            profiles.extend(inputs.claude_accounts.iter().map(|a| a.profile.clone()));
            let seen_at = now();
            for profile in &profiles {
                if let Err(e) = probe::store_connectors(&db, profile, &failed, &seen_at).await {
                    tracing::warn!(event = "mcp_probe_store_failed", profile = %profile, error = %e);
                }
            }
            probe::merge_probes(profiles.iter().map(|p| (p.as_str(), &failed))).into_report()
        }
    }
}

/// Probe every Claude profile in the background (once more if already running).
fn start_probe(app: &AppHandle) {
    let Some(cache) = app.try_state::<DiscoveryCache>() else { return };
    if !cache.begin_probe() {
        return;
    }
    notify(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let report = probe_once(&app).await;
            if !app.state::<DiscoveryCache>().end_probe(report, now()) {
                break;
            }
        }
        notify(&app);
    });
}

/// App start: the cache, then files and probes in the background — start never waits.
pub fn start(app: &AppHandle) {
    app.manage(DiscoveryCache::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let db = app.state::<Db>().inner().clone();
        if let Err(e) = scan_files_now(&app, &db, &app.state::<DiscoveryCache>()).await {
            tracing::warn!(event = "mcp_discovery_scan_failed", error = %e);
        }
        notify(&app);
        start_probe(&app);
    });
}

/// A project was added: read the file sources again (its `.mcp.json`, local scope, …).
pub fn rescan_files_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (Some(db), Some(cache)) = (app.try_state::<Db>(), app.try_state::<DiscoveryCache>()) else { return };
        if let Err(e) = scan_files_now(&app, db.inner(), &cache).await {
            tracing::warn!(event = "mcp_discovery_scan_failed", error = %e);
        }
        notify(&app);
    });
}

async fn view(db: &Db, cache: &DiscoveryCache, project_id: Option<&str>) -> Result<DiscoveryView, String> {
    let snap = cache.snapshot();
    let managed = state::managed_servers(db).await?;
    let labels = state::profile_labels(db).await?;
    let mut view = state::build_view(&ViewInputs {
        report: &snap.report,
        managed: &managed,
        identities: &snap.identities,
        labels: &labels,
        project_id,
    });
    view.scanned_at = snap.scanned_at;
    view.probed_at = snap.probed_at;
    view.probing = snap.probing;
    Ok(view)
}

/// The rows of Settings (no project) or of a project's view, from the last scan (the first call
/// scans the files).
#[tauri::command]
pub async fn list_mcp_discovery(
    app: AppHandle,
    db: State<'_, Db>,
    cache: State<'_, DiscoveryCache>,
    project_id: Option<String>,
) -> Result<DiscoveryView, String> {
    if !cache.has_files() {
        scan_files_now(&app, db.inner(), &cache).await?;
    }
    view(db.inner(), &cache, project_id.as_deref()).await
}

/// Read the files again now, start the probes, and return the fresh file rows (the probe rows
/// follow with [`CHANGED_EVENT`]).
#[tauri::command]
pub async fn reload_mcp_discovery(
    app: AppHandle,
    db: State<'_, Db>,
    cache: State<'_, DiscoveryCache>,
    project_id: Option<String>,
) -> Result<DiscoveryView, String> {
    scan_files_now(&app, db.inner(), &cache).await?;
    start_probe(&app);
    view(db.inner(), &cache, project_id.as_deref()).await
}

/// Stores imported / synced VALUEs in the app's vault.
fn vault(id: &str, env: &HashMap<String, String>, headers: &HashMap<String, String>) -> Result<(), String> {
    crate::secrets::set_mcp_secrets(id, env, headers).map_err(|e| e.to_string())
}

/// Import discovered rows into the MCP list (off app-wide), from a fresh scan. One outcome per
/// row id, in the import's shape.
#[tauri::command]
pub async fn import_discovered_mcp(
    app: AppHandle,
    db: State<'_, Db>,
    cache: State<'_, DiscoveryCache>,
    row_ids: Vec<String>,
) -> Result<Vec<Outcome>, String> {
    let (scan, identities) = fresh_scan(&app, db.inner()).await?;
    let outcomes = import_sync::import_rows(db.inner(), &scan.found, &row_ids, &vault).await;
    cache.set_files(scan.into_report(), identities, now());
    let count = |s: Status| outcomes.iter().filter(|o| o.status == s).count();
    tracing::info!(
        event = "mcp_discovery_import",
        imported = count(Status::Imported),
        skipped = count(Status::Skipped),
        failed = count(Status::Failed)
    );
    notify(&app);
    Ok(outcomes)
}

/// Make a managed copy equal its source again (one-way); its switches stay as they are.
#[tauri::command]
pub async fn sync_discovered_mcp(
    app: AppHandle,
    db: State<'_, Db>,
    cache: State<'_, DiscoveryCache>,
    server_id: String,
) -> Result<McpServer, String> {
    let (scan, identities) = fresh_scan(&app, db.inner()).await?;
    let result = import_sync::sync_server(db.inner(), &scan.found, &server_id, &vault).await;
    cache.set_files(scan.into_report(), identities, now());
    tracing::info!(event = "mcp_discovery_sync", ok = result.is_ok());
    notify(&app);
    result
}

#[cfg(test)]
mod tests {
    use super::super::probe::merge_probes;
    use super::*;

    fn probed(ok: bool) -> DiscoveryReport {
        let result: Result<Vec<ProbedServer>, String> = if ok {
            Ok(vec![ProbedServer { name: "lint".into(), source: "plugin".into(), status: "connected".into(), transport: None, connector: None }])
        } else {
            Err("no login".into())
        };
        merge_probes([("global", &result)]).into_report()
    }

    #[test]
    fn a_remote_desktop_cannot_run_discovery() {
        use crate::remote::{guard::refuses_invoke, ipc::BRIDGE_LABEL};
        for name in ["list_mcp_discovery", "reload_mcp_discovery", "import_discovered_mcp", "sync_discovered_mcp"] {
            assert!(refuses_invoke(BRIDGE_LABEL, name), "{name}");
            assert!(!refuses_invoke("main", name), "{name}");
        }
    }

    #[test]
    fn the_cache_merges_files_and_probes_and_reruns_a_probe_asked_for_meanwhile() {
        let cache = DiscoveryCache::default();
        assert!(!cache.has_files());
        let snap = cache.snapshot();
        assert!(snap.report.servers.is_empty() && snap.scanned_at.is_none() && !snap.probing);

        cache.set_files(DiscoveryReport::default(), vec![], "t1".into());
        assert!(cache.has_files());
        assert!(cache.begin_probe());
        assert!(cache.snapshot().probing);
        // A reload while probing: no second probe at once, one more after.
        assert!(!cache.begin_probe());
        assert!(cache.end_probe(probed(false), "t2".into()), "runs again");
        let snap = cache.snapshot();
        assert!(snap.probing);
        assert_eq!(snap.report.problems.len(), 1);
        assert!(!cache.end_probe(probed(true), "t3".into()));
        let snap = cache.snapshot();
        assert!(!snap.probing);
        assert_eq!((snap.scanned_at.as_deref(), snap.probed_at.as_deref()), (Some("t1"), Some("t3")));
        // The newest probe replaces the last one; the file rows stay.
        assert_eq!(snap.report.servers.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["S5|global|lint"]);
        assert!(snap.report.problems.is_empty());
    }
}
