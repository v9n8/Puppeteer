//! Workspace snapshots taken before each turn, for Rewind (`sessions.rewind`).
//!
//! Puppeteer captures the run's workspace (the folder the agent works in, a
//! Conductor worker's worktree included) right before it hands a prompt to the
//! sidecar: the first one in `start_run`, every later one (resumed turns
//! included) in `send_user_message` — or, for a follow-up sent while a turn
//! runs, when that turn ends and it starts (`TurnGate`). Taking it here, not
//! from the engines' edit events, covers Claude and Codex alike, and files a
//! shell command changes, which no event names reliably. A turn that changes
//! nothing costs one row and no new blob, so every turn is captured and a
//! rewind can go back to any of them.
//!
//! Storage, under `<app data>/snapshots/blobs/<2 hex>/<sha256>`: file contents
//! and manifests, content-addressed and shared by every run, so an unchanged
//! file is stored once. A manifest lists the captured files and the in-scope
//! files it does not hold (`unmanaged`: too large or unreadable) and what the
//! walk left out (`ignored`), which a restore must leave alone. `run_snapshots`
//! (migration 0006) has one row per turn pointing at its manifest.
//!
//! Scope: files under the workspace that git would not ignore (`.gitignore`,
//! `.git/info/exclude`, global excludes); `.git` and Puppeteer's own
//! `.puppeteer` folder are left out, symlinks skipped. A file over
//! `max_file_bytes` is left unmanaged. A workspace over `max_files` files or
//! `max_snapshot_bytes` is not captured at all (`skipped`): a partial snapshot
//! would make a restore delete the files it missed.
//!
//! Cleanup: a run keeps its `keep_per_run` newest distinct manifests; a
//! snapshot older than `max_age_days` is dropped; the store stays under
//! `max_store_bytes` by dropping the least recently used manifests. A dropped
//! snapshot keeps its row (`pruned`) so turn numbers never shift. Blobs nothing
//! points at are deleted by [`sweep_at`]: at startup, then at most hourly.
//!
//! Restore ([`preview`], then [`restore`]) is Rewind's file half (`runs::rewind`):
//! files that differ are written back, files created since are deleted, and
//! the unmanaged ones, those the snapshot left out as ignored (whatever the
//! ignore rules say now), and new files too large to have a copy are left alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::db::Db;

pub const FEATURE: &str = "sessions.rewind";

#[derive(Clone, Copy)]
struct Limits {
    max_files: usize,
    max_file_bytes: u64,
    max_snapshot_bytes: u64,
    keep_per_run: usize,
    max_age_days: i64,
    max_store_bytes: u64,
}

const LIMITS: Limits = Limits {
    max_files: 5000,
    max_file_bytes: 4 << 20,
    max_snapshot_bytes: 64 << 20,
    keep_per_run: 20,
    max_age_days: 30,
    max_store_bytes: 2 << 30,
};

const SWEEP_EVERY: Duration = Duration::from_secs(60 * 60);
/// The stat cache is a speed-up only; past this many files it starts over.
const MAX_CACHED: usize = 200_000;
const RACY: Duration = Duration::from_secs(2);

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set once at startup to `<app data>/snapshots`.
pub fn set_root(dir: PathBuf) {
    let _ = ROOT.set(dir);
}

fn root() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(|| std::env::temp_dir().join("puppeteer-snapshots"))
}

/// A file's content hash, valid while its size and mtime stay the same. Like
/// git's index, a file changed less than [`RACY`] ago is not cached: a rewrite
/// within the same mtime tick at the same size would otherwise look unchanged.
struct Stamp {
    len: u64,
    modified: SystemTime,
    sha: String,
}

/// Keyed by absolute path, so runs on the same folder share it.
type StatCache = HashMap<PathBuf, Stamp>;

/// Held by a capture until its row is written, and by the sweep: a blob is
/// never collected between being written and being referenced.
static STATE: LazyLock<Mutex<StatCache>> = LazyLock::new(|| Mutex::new(StatCache::new()));
static LAST_SWEEP: StdMutex<Option<Instant>> = StdMutex::new(None);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManifestFile {
    path: String,
    sha: String,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Manifest {
    files: Vec<ManifestFile>,
    unmanaged: Vec<String>,
    /// Files and folders in the walked folders that the walk left out (ignored,
    /// or unreadable); a folder stands for everything under it. `None` in a
    /// manifest written before this was recorded.
    #[serde(default)]
    ignored: Option<Vec<String>>,
}

#[derive(Debug, PartialEq, Eq)]
enum Captured {
    Saved { manifest: String, files: usize, bytes: u64 },
    Skipped(&'static str),
}

fn blob_path(store: &Path, sha: &str) -> PathBuf {
    store.join("blobs").join(&sha[..2]).join(sha)
}

fn sha256(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Write a blob unless the store already has it. Written aside and renamed in,
/// so a crash never leaves a truncated blob under a valid name.
fn put_blob(store: &Path, sha: &str, data: &[u8]) -> Result<(), String> {
    let path = blob_path(store, sha);
    if path.is_file() {
        return Ok(());
    }
    let dir = path.parent().expect("a blob path has a parent");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{sha}.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

fn read_manifest(store: &Path, sha: &str) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(blob_path(store, sha)).ok()?).ok()
}

struct Listed {
    rel: String,
    abs: PathBuf,
    len: u64,
    modified: SystemTime,
}

fn left_out_by_name(name: &std::ffi::OsStr) -> bool {
    name == ".git" || name == ".puppeteer"
}

/// The workspace's in-scope regular files and the folders walked (`""` is the
/// workspace), or `None` past `max_files`.
fn list(workspace: &Path, max_files: usize) -> Option<(Vec<Listed>, Vec<String>)> {
    // `.gitignore` applies outside a git repo too: a plain folder's `.env` and
    // `node_modules` stay out of the store.
    let walker = ignore::WalkBuilder::new(workspace)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .filter_entry(|e| !left_out_by_name(e.file_name()))
        .build();
    let mut out = Vec::new();
    let mut dirs = Vec::new();
    for entry in walker.flatten() {
        let Ok(rel) = entry.path().strip_prefix(workspace) else {
            continue;
        };
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            dirs.push(rel.to_string_lossy().replace('\\', "/"));
            continue;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if out.len() == max_files {
            return None;
        }
        out.push(Listed {
            rel: rel.to_string_lossy().replace('\\', "/"),
            abs: entry.path().to_path_buf(),
            len: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    Some((out, dirs))
}

/// What sits in the walked folders but was not walked: ignored by the rules of
/// the moment, or unreadable. Links are skipped, not ignored, so not listed.
fn left_out(workspace: &Path, listed: &[Listed], dirs: &[String]) -> Vec<String> {
    let walked: HashSet<&str> = listed.iter().map(|l| l.rel.as_str()).chain(dirs.iter().map(String::as_str)).collect();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(workspace.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if left_out_by_name(&name) || entry.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            let name = name.to_string_lossy();
            let rel = if dir.is_empty() { name.into_owned() } else { format!("{dir}/{name}") };
            if !walked.contains(rel.as_str()) {
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// Capture `workspace` into `store`. Only changed files are read: the rest
/// reuse their hash from `cache`.
fn capture(
    store: &Path,
    workspace: &Path,
    limits: &Limits,
    cache: &mut StatCache,
) -> Result<Captured, String> {
    if !workspace.is_dir() {
        return Err(format!("not a folder: {}", workspace.display()));
    }
    let Some((listed, dirs)) = list(workspace, limits.max_files) else {
        return Ok(Captured::Skipped("too_many_files"));
    };
    let ignored = left_out(workspace, &listed, &dirs);
    // The size limits first, from the walk alone: nothing is read for a
    // workspace that will not be captured.
    let mut unmanaged = Vec::new();
    let mut wanted = Vec::new();
    let mut total = 0u64;
    for item in listed {
        if item.len > limits.max_file_bytes {
            unmanaged.push(item.rel);
            continue;
        }
        total += item.len;
        if total > limits.max_snapshot_bytes {
            return Ok(Captured::Skipped("too_large"));
        }
        wanted.push(item);
    }

    if cache.len() > MAX_CACHED {
        cache.clear();
    }
    let mut files = Vec::with_capacity(wanted.len());
    for item in wanted {
        if let Some(hit) = cache.get(&item.abs) {
            if hit.len == item.len && hit.modified == item.modified && blob_path(store, &hit.sha).is_file() {
                files.push(ManifestFile { path: item.rel, sha: hit.sha.clone(), size: item.len });
                continue;
            }
        }
        let data = match std::fs::read(&item.abs) {
            // Grown past the limit since the walk.
            Ok(data) if data.len() as u64 > limits.max_file_bytes => None,
            Ok(data) => Some(data),
            Err(_) => None,
        };
        let Some(data) = data else {
            unmanaged.push(item.rel);
            continue;
        };
        let sha = sha256(&data);
        put_blob(store, &sha, &data)?;
        if item.modified.elapsed().is_ok_and(|age| age > RACY) {
            cache.insert(
                item.abs,
                Stamp { len: data.len() as u64, modified: item.modified, sha: sha.clone() },
            );
        }
        files.push(ManifestFile { path: item.rel, sha, size: data.len() as u64 });
    }

    // Sorted, so the same workspace always gives the same manifest blob.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    unmanaged.sort();
    let (count, bytes) = (files.len(), files.iter().map(|f| f.size).sum());
    let manifest = serde_json::to_vec(&Manifest { files, unmanaged, ignored: Some(ignored) }).map_err(|e| e.to_string())?;
    let sha = sha256(&manifest);
    put_blob(store, &sha, &manifest)?;
    Ok(Captured::Saved { manifest: sha, files: count, bytes })
}

fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The folder the run's agent works in, as `start_run` / `resume_run` launch it.
async fn workspace_of(db: &Db, run_id: &str) -> Result<String, String> {
    let (project, worktree) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT p.path, r.worktree_path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
    )
    .bind(run_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    crate::conductor::worktree::run_dir(&project, worktree, run_id).await
}

async fn record(
    db: &Db,
    run_id: &str,
    workspace: &str,
    outcome: &Result<Captured, String>,
) -> Result<i64, String> {
    let (status, reason, manifest, files, bytes) = match outcome {
        Ok(Captured::Saved { manifest, files, bytes }) => ("ok", None, Some(manifest.as_str()), *files as i64, *bytes as i64),
        Ok(Captured::Skipped(reason)) => ("skipped", Some(*reason), None, 0, 0),
        Err(_) => ("skipped", Some("error"), None, 0, 0),
    };
    sqlx::query(
        "INSERT INTO run_snapshots (run_id, turn, created_at, workspace, status, reason, manifest_sha, file_count, total_bytes)
         VALUES (?, (SELECT COALESCE(MAX(turn), 0) + 1 FROM run_snapshots WHERE run_id = ?), ?, ?, ?, ?, ?, ?, ?)
         RETURNING turn",
    )
    .bind(run_id)
    .bind(run_id)
    .bind(timestamp(Utc::now()))
    .bind(workspace)
    .bind(status)
    .bind(reason)
    .bind(manifest)
    .bind(files)
    .bind(bytes)
    .fetch_one(db)
    .await
    .map(|row: sqlx::sqlite::SqliteRow| sqlx::Row::get(&row, 0))
    .map_err(|e| e.to_string())
}

const PRUNE: &str = "UPDATE run_snapshots SET status = 'pruned', manifest_sha = NULL";

/// Keep the run's `keep` most recently used manifests. Counted by manifest, not
/// by turn: turns that changed nothing share one and must not push out edits.
async fn prune_run(db: &Db, run_id: &str, keep: usize) -> Result<(), String> {
    let old: Vec<String> = sqlx::query_scalar(
        "SELECT manifest_sha FROM run_snapshots WHERE run_id = ? AND status = 'ok'
         GROUP BY manifest_sha ORDER BY MAX(turn) DESC LIMIT -1 OFFSET ?",
    )
    .bind(run_id)
    .bind(keep as i64)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    for sha in old {
        sqlx::query(&format!("{PRUNE} WHERE run_id = ? AND manifest_sha = ?"))
            .bind(run_id)
            .bind(&sha)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// [`before_turn`] into `store`. None when the switch is off: nothing taken.
async fn before_turn_at(db: &Db, store: &Path, run_id: &str, limits: &Limits) -> Option<Option<i64>> {
    if !crate::features::is_on(db, FEATURE).await {
        return None;
    }
    let workspace = workspace_of(db, run_id).await;
    let mut cache = STATE.lock().await;
    let outcome = match &workspace {
        Ok(dir) => {
            let (store, dir, limits) = (store.to_path_buf(), PathBuf::from(dir), *limits);
            let mut taken = std::mem::take(&mut *cache);
            tokio::task::spawn_blocking(move || {
                let res = capture(&store, &dir, &limits, &mut taken);
                (res, taken)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|(res, taken)| {
                *cache = taken;
                res
            })
        }
        Err(e) => Err(e.clone()),
    };
    if let Err(e) = &outcome {
        tracing::warn!(event = "snapshot_failed", run_id = %run_id, error = %e);
    }
    let written = match record(db, run_id, workspace.as_deref().unwrap_or(""), &outcome).await {
        Ok(turn) => prune_run(db, run_id, limits.keep_per_run).await.map(|()| turn),
        Err(e) => Err(e),
    };
    drop(cache);
    if let Err(e) = &written {
        tracing::warn!(event = "snapshot_record_failed", run_id = %run_id, error = %e);
    }
    Some(written.ok())
}

/// Capture the run's workspace before its next prompt goes to the sidecar.
/// Best effort: a failure is logged and recorded, never stops the turn.
/// Returns the turn the snapshot was recorded under, which the prompt's line
/// in the run log carries so Rewind can find it.
pub async fn before_turn(db: &Db, run_id: &str) -> Option<i64> {
    let turn = before_turn_at(db, &root(), run_id, &LIMITS).await?;
    sweep_in_background(db.clone());
    turn
}

/// [`sweep_at`] off the caller's path, at most once per [`SWEEP_EVERY`].
pub fn sweep_in_background(db: Db) {
    {
        let mut last = LAST_SWEEP.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < SWEEP_EVERY) {
            return;
        }
        *last = Some(Instant::now());
    }
    tauri::async_runtime::spawn(async move {
        match sweep_at(&db, &root(), &LIMITS, Utc::now()).await {
            Ok(0) => {}
            Ok(removed) => tracing::info!(event = "snapshots_swept", removed),
            Err(e) => tracing::warn!(event = "snapshots_sweep_failed", error = %e),
        }
    });
}

/// Every blob in the store: sha (file name) → (path, bytes). Anything else
/// there (a write cut short) is listed too, under its own name.
fn stored_blobs(store: &Path) -> HashMap<String, (PathBuf, u64)> {
    let mut out = HashMap::new();
    let Ok(dirs) = std::fs::read_dir(store.join("blobs")) else {
        return out;
    };
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            out.insert(file.file_name().to_string_lossy().into_owned(), (file.path(), len));
        }
    }
    out
}

/// Which manifests to drop (missing ones, then the least recently used while
/// the store is over `max_store_bytes`) and which blobs the rest still need.
/// `manifests` is oldest first.
fn plan_cleanup(
    store: &Path,
    manifests: &[String],
    blobs: &HashMap<String, (PathBuf, u64)>,
    max_store_bytes: u64,
) -> (Vec<String>, HashSet<String>) {
    let mut dropped = Vec::new();
    let mut kept: Vec<(String, HashSet<String>)> = Vec::new();
    for sha in manifests {
        match read_manifest(store, sha) {
            Some(m) => {
                let mut uses: HashSet<String> = m.files.into_iter().map(|f| f.sha).collect();
                uses.insert(sha.clone());
                kept.push((sha.clone(), uses));
            }
            None => dropped.push(sha.clone()),
        }
    }
    let size = |sha: &str| blobs.get(sha).map_or(0, |b| b.1);
    let mut refs: HashMap<String, usize> = HashMap::new();
    let mut live = 0u64;
    for (_, uses) in &kept {
        for sha in uses {
            let n = refs.entry(sha.clone()).or_default();
            if *n == 0 {
                live += size(sha);
            }
            *n += 1;
        }
    }
    let mut oldest = 0;
    while live > max_store_bytes && oldest < kept.len() {
        let (sha, uses) = &kept[oldest];
        for blob in uses {
            let n = refs.get_mut(blob).expect("counted above");
            *n -= 1;
            if *n == 0 {
                live -= size(blob);
            }
        }
        dropped.push(sha.clone());
        oldest += 1;
    }
    let needed = refs.into_iter().filter(|(_, n)| *n > 0).map(|(sha, _)| sha).collect();
    (dropped, needed)
}

/// Drop expired snapshots, hold the store under its size limit, and delete
/// the blobs no snapshot points at. Returns the number of blobs deleted.
async fn sweep_at(db: &Db, store: &Path, limits: &Limits, now: DateTime<Utc>) -> Result<usize, String> {
    let _held = STATE.lock().await;
    let cutoff = timestamp(now - chrono::Duration::days(limits.max_age_days));
    sqlx::query(&format!("{PRUNE} WHERE status = 'ok' AND created_at < ?"))
        .bind(cutoff)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    let manifests: Vec<String> = sqlx::query_scalar(
        "SELECT manifest_sha FROM run_snapshots WHERE status = 'ok'
         GROUP BY manifest_sha ORDER BY MAX(created_at), MAX(id)",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let (dir, max) = (store.to_path_buf(), limits.max_store_bytes);
    let (dropped, needed, blobs) = tokio::task::spawn_blocking(move || {
        let blobs = stored_blobs(&dir);
        let (dropped, needed) = plan_cleanup(&dir, &manifests, &blobs, max);
        (dropped, needed, blobs)
    })
    .await
    .map_err(|e| e.to_string())?;
    // Rows first: a failure here leaves blobs behind, never a row without its blobs.
    for sha in &dropped {
        sqlx::query(&format!("{PRUNE} WHERE manifest_sha = ?"))
            .bind(sha)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::task::spawn_blocking(move || {
        blobs
            .into_iter()
            .filter(|(sha, _)| !needed.contains(sha))
            .filter(|(_, (path, _))| std::fs::remove_file(path).is_ok())
            .count()
    })
    .await
    .map_err(|e| e.to_string())
}

/// One turn's snapshot, as Rewind lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotInfo {
    pub turn: i64,
    pub created_at: String,
    /// 'ok' (restorable) | 'skipped' | 'pruned'.
    pub status: String,
    pub reason: Option<String>,
    pub file_count: i64,
    pub total_bytes: i64,
}

pub async fn of_run(db: &Db, run_id: &str) -> Result<Vec<SnapshotInfo>, String> {
    let rows: Vec<(i64, String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT turn, created_at, status, reason, file_count, total_bytes FROM run_snapshots
         WHERE run_id = ? ORDER BY turn",
    )
    .bind(run_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(turn, created_at, status, reason, file_count, total_bytes)| SnapshotInfo {
            turn,
            created_at,
            status,
            reason,
            file_count,
            total_bytes,
        })
        .collect())
}

/// What a restore does to one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// Overwritten with the snapshot's copy.
    Restore,
    /// Gone now: written back.
    Create,
    /// Not in the snapshot (created after it): deleted.
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    pub change: Change,
    /// Last written after `since` (the run had stopped): not this session's doing.
    pub outside: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePlan {
    pub changes: Vec<FileChange>,
    /// Files created after the snapshot but over the size limit: left in place,
    /// since nothing could bring them back once deleted.
    pub kept: Vec<String>,
    /// The workspace state this plan was made from; a restore refuses to run
    /// when it no longer matches (the files changed after the user confirmed).
    pub fingerprint: String,
}

struct Planned {
    plan: RestorePlan,
    /// (relative path, blob sha) to write.
    writes: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Restored {
    pub restored: usize,
    pub deleted: usize,
    /// Paths that could not be written or deleted.
    pub failed: Vec<String>,
}

/// The content hash of a listed file, from `cache` while it is still valid.
fn current_sha(item: &Listed, cache: &mut StatCache) -> Option<String> {
    if let Some(hit) = cache.get(&item.abs) {
        if hit.len == item.len && hit.modified == item.modified {
            return Some(hit.sha.clone());
        }
    }
    let data = std::fs::read(&item.abs).ok()?;
    let sha = sha256(&data);
    if item.modified.elapsed().is_ok_and(|age| age > RACY) {
        cache.insert(item.abs.clone(), Stamp { len: data.len() as u64, modified: item.modified, sha: sha.clone() });
    }
    Some(sha)
}

/// A regular file at `rel` that the walk did not list: ignored since the snapshot.
fn unlisted(workspace: &Path, rel: &str) -> Option<Listed> {
    let abs = workspace.join(rel);
    let meta = std::fs::symlink_metadata(&abs).ok().filter(|m| m.is_file())?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Some(Listed { rel: rel.to_string(), abs, len: meta.len(), modified })
}

/// What restoring `manifest` (taken at `taken`) into `workspace` would change.
/// The manifest's unmanaged and ignored files are never touched.
fn plan_restore(
    store: &Path,
    workspace: &Path,
    manifest: &str,
    limits: &Limits,
    since: Option<SystemTime>,
    taken: SystemTime,
    cache: &mut StatCache,
) -> Result<Planned, String> {
    let m = read_manifest(store, manifest).ok_or("snapshot_missing")?;
    if !workspace.is_dir() {
        return Err("workspace_missing".into());
    }
    let (listed, _) = list(workspace, limits.max_files).ok_or("too_many_files")?;
    let mut current: HashMap<String, Listed> = listed.into_iter().map(|l| (l.rel.clone(), l)).collect();
    let unmanaged: HashSet<&str> = m.unmanaged.iter().map(String::as_str).collect();
    let ignored: Option<HashSet<&str>> = m.ignored.as_ref().map(|v| v.iter().map(String::as_str).collect());
    // The snapshot holds no copy of a file it left out, so deleting it would
    // lose it for good, even if the ignore rules no longer leave it out.
    let left_out = |item: &Listed| match &ignored {
        Some(ignored) => std::iter::successors(Some(item.rel.as_str()), |r| r.rfind('/').map(|i| &r[..i]))
            .any(|p| ignored.contains(p)),
        // An older manifest does not say: a file not written since was there then.
        None => item.modified <= taken,
    };
    let outside = |item: &Listed| since.is_some_and(|s| item.modified > s);

    let mut changes = Vec::new();
    let mut writes = Vec::new();
    let mut kept = Vec::new();
    // Each line names a path, what happens to it and the state it is in now.
    let mut seen = Vec::new();
    let stamp = |item: &Listed| {
        let nanos = item.modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        format!("{}:{nanos}", item.len)
    };
    for f in &m.files {
        let change = match current.remove(&f.path).or_else(|| unlisted(workspace, &f.path)) {
            None => Some((Change::Create, false, "-".to_string())),
            Some(item) => {
                let same = item.len == f.size && current_sha(&item, cache).as_deref() == Some(f.sha.as_str());
                (!same).then(|| (Change::Restore, outside(&item), stamp(&item)))
            }
        };
        if let Some((change, outside, now)) = change {
            seen.push(format!("{change:?}\t{}\t{now}\t{}", f.path, f.sha));
            changes.push(FileChange { path: f.path.clone(), change, outside });
            writes.push((f.path.clone(), f.sha.clone()));
        }
    }
    let mut created: Vec<Listed> =
        current.into_values().filter(|l| !unmanaged.contains(l.rel.as_str()) && !left_out(l)).collect();
    created.sort_by(|a, b| a.rel.cmp(&b.rel));
    for item in created {
        if item.len > limits.max_file_bytes {
            kept.push(item.rel);
            continue;
        }
        seen.push(format!("Delete\t{}\t{}", item.rel, stamp(&item)));
        let outside = outside(&item);
        changes.push(FileChange { path: item.rel, change: Change::Delete, outside });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    seen.sort();
    let fingerprint = sha256(seen.join("\n").as_bytes());
    Ok(Planned { plan: RestorePlan { changes, kept, fingerprint }, writes })
}

/// `rel` under `root`, refused when it could land outside it: a path that is
/// not plain names, or a folder on the way that resolves elsewhere (a link).
fn inside(root: &Path, rel: &str, create_parent: bool) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel.is_empty() || !rel_path.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err(format!("unsafe path: {rel}"));
    }
    let path = root.join(rel_path);
    let parent = path.parent().ok_or_else(|| format!("unsafe path: {rel}"))?;
    let check = |dir: &Path| match dir.canonicalize() {
        Ok(real) if real.starts_with(root) => Ok(()),
        Ok(_) => Err(format!("outside the workspace: {rel}")),
        Err(e) => Err(e.to_string()),
    };
    if create_parent {
        // The deepest folder that exists first: folders must not be made
        // through a link that leads out.
        let mut existing = parent;
        while std::fs::symlink_metadata(existing).is_err() {
            existing = existing.parent().ok_or_else(|| format!("unsafe path: {rel}"))?;
        }
        check(existing)?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    check(parent)?;
    Ok(path)
}

/// Write `data` over `path` whole: aside, then renamed in. An existing file
/// keeps its permissions (a script stays executable).
fn write_whole(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path.parent().expect("checked by inside()");
    let tmp = dir.join(format!(".puppeteer-rewind-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_file() {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Carry out `planned`. Every blob is read and checked first, so a snapshot
/// missing a piece changes nothing at all.
fn apply_restore(store: &Path, workspace: &Path, planned: &Planned) -> Result<Restored, String> {
    let mut blobs = Vec::with_capacity(planned.writes.len());
    for (rel, sha) in &planned.writes {
        let data = std::fs::read(blob_path(store, sha)).map_err(|_| format!("snapshot_damaged: {rel}"))?;
        if sha256(&data) != *sha {
            return Err(format!("snapshot_damaged: {rel}"));
        }
        blobs.push((rel, data));
    }
    let root = workspace.canonicalize().map_err(|e| e.to_string())?;
    let mut out = Restored { restored: 0, deleted: 0, failed: Vec::new() };
    for (rel, data) in blobs {
        match inside(&root, rel, true).and_then(|path| write_whole(&path, &data)) {
            Ok(()) => out.restored += 1,
            Err(_) => out.failed.push(rel.clone()),
        }
    }
    for c in planned.plan.changes.iter().filter(|c| c.change == Change::Delete) {
        match inside(&root, &c.path, false).and_then(|path| std::fs::remove_file(path).map_err(|e| e.to_string())) {
            Ok(()) => out.deleted += 1,
            Err(_) => out.failed.push(c.path.clone()),
        }
    }
    Ok(out)
}

/// The folder, manifest and capture time of a turn's snapshot, if it can be restored.
async fn restorable(db: &Db, run_id: &str, turn: i64) -> Result<(PathBuf, String, SystemTime), String> {
    let row: Option<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT workspace, status, manifest_sha, created_at FROM run_snapshots WHERE run_id = ? AND turn = ?",
    )
    .bind(run_id)
    .bind(turn)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    match row {
        Some((workspace, status, Some(manifest), created_at)) if status == "ok" && !workspace.is_empty() => {
            // Recorded once the capture is done, so never before a file it saw.
            let taken = DateTime::parse_from_rfc3339(&created_at).map_err(|e| e.to_string())?;
            Ok((PathBuf::from(workspace), manifest, taken.into()))
        }
        Some(_) => Err("snapshot_unavailable".into()),
        None => Err("snapshot_not_found".into()),
    }
}

/// The folder a turn's snapshot was taken of.
pub async fn workspace_of_turn(db: &Db, run_id: &str, turn: i64) -> Result<PathBuf, String> {
    restorable(db, run_id, turn).await.map(|(dir, _, _)| dir)
}

async fn preview_at(
    db: &Db,
    store: &Path,
    run_id: &str,
    turn: i64,
    since: Option<SystemTime>,
    limits: &Limits,
) -> Result<RestorePlan, String> {
    let (workspace, manifest, taken_at) = restorable(db, run_id, turn).await?;
    let mut cache = STATE.lock().await;
    let (store, limits) = (store.to_path_buf(), *limits);
    let mut taken = std::mem::take(&mut *cache);
    let (res, taken) = tokio::task::spawn_blocking(move || {
        let res = plan_restore(&store, &workspace, &manifest, &limits, since, taken_at, &mut taken);
        (res, taken)
    })
    .await
    .map_err(|e| e.to_string())?;
    *cache = taken;
    res.map(|p| p.plan)
}

/// What restoring the snapshot taken before `turn` would change.
pub async fn preview(db: &Db, run_id: &str, turn: i64, since: Option<SystemTime>) -> Result<RestorePlan, String> {
    preview_at(db, &root(), run_id, turn, since, &LIMITS).await
}

async fn restore_at(
    db: &Db,
    store: &Path,
    run_id: &str,
    turn: i64,
    since: Option<SystemTime>,
    fingerprint: &str,
    limits: &Limits,
) -> Result<Restored, String> {
    let (workspace, manifest, taken_at) = restorable(db, run_id, turn).await?;
    // Held throughout: the sweep must not drop the blobs being read.
    let mut cache = STATE.lock().await;
    let (store, limits, expected) = (store.to_path_buf(), *limits, fingerprint.to_string());
    let mut taken = std::mem::take(&mut *cache);
    let (res, taken) = tokio::task::spawn_blocking(move || {
        let res = plan_restore(&store, &workspace, &manifest, &limits, since, taken_at, &mut taken).and_then(|planned| {
            if planned.plan.fingerprint != expected {
                return Err("files_changed".to_string());
            }
            apply_restore(&store, &workspace, &planned)
        });
        (res, taken)
    })
    .await
    .map_err(|e| e.to_string())?;
    *cache = taken;
    res
}

/// Put the workspace back as the snapshot taken before `turn` holds it, if it
/// is still in the state `fingerprint` (from [`preview`]) describes.
pub async fn restore(
    db: &Db,
    run_id: &str,
    turn: i64,
    since: Option<SystemTime>,
    fingerprint: &str,
) -> Result<Restored, String> {
    restore_at(db, &root(), run_id, turn, since, fingerprint, &LIMITS).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-snap-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_repo(label: &str) -> PathBuf {
        let dir = temp(label);
        assert!(Command::new("git").arg("-C").arg(&dir).args(["init", "-q"]).status().unwrap().success());
        dir
    }

    fn write(dir: &Path, rel: &str, body: &[u8]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn blob_count(store: &Path) -> usize {
        stored_blobs(store).len()
    }

    fn saved(c: Captured) -> String {
        match c {
            Captured::Saved { manifest, .. } => manifest,
            other => panic!("not captured: {other:?}"),
        }
    }

    const SMALL: Limits = Limits {
        max_files: 10,
        max_file_bytes: 16,
        max_snapshot_bytes: 64,
        keep_per_run: 2,
        max_age_days: 30,
        max_store_bytes: 1 << 20,
    };

    #[test]
    fn captures_the_files_git_would_keep_and_leaves_large_ones_unmanaged() {
        let (ws, store) = (git_repo("scope"), temp("store"));
        write(&ws, ".gitignore", b"build/\n*.log\n");
        write(&ws, "src/a.rs", b"fn a() {}");
        write(&ws, "src/b.rs", b"fn a() {}");
        write(&ws, "build/out.bin", b"ignored");
        write(&ws, "debug.log", b"ignored");
        write(&ws, ".puppeteer/runs/r1.log", b"run log");
        write(&ws, "big.bin", &[7u8; 17]);
        #[cfg(unix)]
        std::os::unix::fs::symlink(ws.join("src/a.rs"), ws.join("link.rs")).unwrap();

        let got = capture(&store, &ws, &SMALL, &mut StatCache::new()).unwrap();
        let Captured::Saved { manifest, files, bytes } = got else { panic!("{got:?}") };
        let m = read_manifest(&store, &manifest).unwrap();
        let paths: Vec<&str> = m.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [".gitignore", "src/a.rs", "src/b.rs"]);
        assert_eq!(m.unmanaged, ["big.bin"]);
        assert_eq!((files, bytes), (3, 13 + 9 + 9));
        // Two files with the same content: one blob. Plus .gitignore and the manifest.
        assert_eq!(m.files[1].sha, m.files[2].sha);
        assert_eq!(blob_count(&store), 3);
        assert_eq!(std::fs::read(blob_path(&store, &m.files[1].sha)).unwrap(), b"fn a() {}");
    }

    #[test]
    fn an_unchanged_workspace_reuses_its_manifest_and_a_change_stores_only_that_file() {
        let (ws, store) = (git_repo("dedupe"), temp("store"));
        write(&ws, "a.txt", b"one");
        write(&ws, "b.txt", b"two");
        let mut cache = StatCache::new();
        let first = saved(capture(&store, &ws, &SMALL, &mut cache).unwrap());
        assert_eq!(blob_count(&store), 3);

        assert_eq!(saved(capture(&store, &ws, &SMALL, &mut cache).unwrap()), first);
        // Same answer without the cache: the manifest depends on content only.
        assert_eq!(saved(capture(&store, &ws, &SMALL, &mut StatCache::new()).unwrap()), first);
        assert_eq!(blob_count(&store), 3);

        write(&ws, "b.txt", b"TWO!");
        let second = saved(capture(&store, &ws, &SMALL, &mut cache).unwrap());
        assert_ne!(second, first);
        assert_eq!(blob_count(&store), 5, "the new b.txt and the new manifest");
        let m = read_manifest(&store, &second).unwrap();
        assert_eq!(m.files[1], ManifestFile { path: "b.txt".into(), sha: sha256(b"TWO!"), size: 4 });
    }

    #[test]
    fn a_workspace_over_the_limits_is_not_captured() {
        let store = temp("store");
        let many = git_repo("many");
        for i in 0..=SMALL.max_files {
            write(&many, &format!("f{i}.txt"), b"x");
        }
        assert_eq!(capture(&store, &many, &SMALL, &mut StatCache::new()), Ok(Captured::Skipped("too_many_files")));

        let large = git_repo("large");
        for i in 0..5 {
            write(&large, &format!("f{i}.txt"), &[b'x'; 16]);
        }
        assert_eq!(capture(&store, &large, &SMALL, &mut StatCache::new()), Ok(Captured::Skipped("too_large")));
        assert_eq!(blob_count(&store), 0, "nothing is read for a skipped workspace");
    }

    async fn db_with_run(project: &Path, rewind: bool) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
            .bind(project.to_string_lossy().as_ref())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c'), ('r2', 'p1', 'session', 'c')")
            .execute(&db)
            .await
            .unwrap();
        let value = if rewind { "true" } else { "false" };
        crate::commands::settings::set_setting(&db, "feature.sessions.rewind", value).await.unwrap();
        db
    }

    async fn rows(db: &Db, run: &str) -> Vec<(i64, String, Option<String>, Option<String>)> {
        sqlx::query_as("SELECT turn, status, reason, manifest_sha FROM run_snapshots WHERE run_id = ? ORDER BY turn")
            .bind(run)
            .fetch_all(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn every_turn_gets_a_row_and_the_switch_turns_capture_off() {
        let (ws, store) = (git_repo("turns"), temp("store"));
        write(&ws, "a.txt", b"one");
        let db = db_with_run(&ws, true).await;

        assert_eq!(before_turn_at(&db, &store, "r1", &SMALL).await, Some(Some(1)));
        assert_eq!(before_turn_at(&db, &store, "r1", &SMALL).await, Some(Some(2)));
        for i in 0..5 {
            write(&ws, &format!("f{i}.txt"), &[b'x'; 16]);
        }
        assert_eq!(before_turn_at(&db, &store, "r1", &SMALL).await, Some(Some(3)));
        let got = rows(&db, "r1").await;
        assert_eq!(got.len(), 3);
        assert_eq!((got[0].1.as_str(), &got[0].3), ("ok", &got[1].3), "a chat-only turn shares the manifest");
        assert_eq!((got[2].0, got[2].1.as_str(), got[2].2.as_deref()), (3, "skipped", Some("too_large")));
        let workspace: String = sqlx::query_scalar("SELECT workspace FROM run_snapshots WHERE turn = 1")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(workspace, ws.to_string_lossy());

        crate::commands::settings::set_setting(&db, "feature.sessions.rewind", "false").await.unwrap();
        let before = blob_count(&store);
        assert_eq!(before_turn_at(&db, &store, "r1", &SMALL).await, None);
        assert_eq!(rows(&db, "r1").await.len(), 3);
        assert_eq!(blob_count(&store), before);

        // Removing the run removes its rows.
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        assert!(rows(&db, "r1").await.is_empty());
    }

    #[tokio::test]
    async fn a_run_keeps_its_newest_manifests_and_the_sweep_deletes_what_nothing_uses() {
        let (ws, store) = (git_repo("keep"), temp("store"));
        let db = db_with_run(&ws, true).await;
        for body in ["v1", "v22", "v22", "v333"] {
            write(&ws, "a.txt", body.as_bytes());
            before_turn_at(&db, &store, "r1", &SMALL).await;
        }
        let got = rows(&db, "r1").await;
        let status: Vec<&str> = got.iter().map(|r| r.1.as_str()).collect();
        // keep_per_run = 2 manifests: v22 (turns 2 and 3) and v333.
        assert_eq!(status, ["pruned", "ok", "ok", "ok"]);
        assert_eq!(got[0].3, None);
        assert_eq!(blob_count(&store), 6, "v1's blobs stay until the sweep");

        assert_eq!(sweep_at(&db, &store, &SMALL, Utc::now()).await, Ok(2));
        assert!(!blob_path(&store, &sha256(b"v1")).exists());
        for r in &got[1..] {
            assert!(read_manifest(&store, r.3.as_deref().unwrap()).is_some());
        }

        // A file whose blob was swept is stored again when it comes back.
        let mut cache = StatCache::new();
        write(&ws, "a.txt", b"v1");
        capture(&store, &ws, &SMALL, &mut cache).unwrap();
        assert_eq!(std::fs::read(blob_path(&store, &sha256(b"v1"))).unwrap(), b"v1");
    }

    #[tokio::test]
    async fn the_sweep_drops_old_snapshots_deleted_runs_and_the_oldest_over_the_store_limit() {
        let (ws, store) = (git_repo("sweep"), temp("store"));
        let db = db_with_run(&ws, true).await;
        let limits = Limits { keep_per_run: 10, ..SMALL };
        for (run, body) in [("r1", "a"), ("r1", "bb"), ("r2", "ccc")] {
            write(&ws, "a.txt", body.as_bytes());
            before_turn_at(&db, &store, run, &limits).await;
        }
        sqlx::query("UPDATE run_snapshots SET created_at = '2026-01-01T00:00:00.000Z' WHERE run_id = 'r1' AND turn = 1")
            .execute(&db)
            .await
            .unwrap();

        // Age: within max_age_days nothing goes; past it the backdated turn does.
        let backdated = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let soon = backdated + chrono::Duration::days(limits.max_age_days);
        assert_eq!(sweep_at(&db, &store, &limits, soon).await, Ok(0));
        assert_eq!(rows(&db, "r1").await[0].1, "ok");
        assert_eq!(sweep_at(&db, &store, &limits, Utc::now()).await, Ok(2));
        let r1 = rows(&db, "r1").await;
        assert_eq!((r1[0].1.as_str(), r1[1].1.as_str()), ("pruned", "ok"));
        assert!(!blob_path(&store, &sha256(b"a")).exists());

        // Store limit: only the newest manifest fits, so r1's last one goes.
        let r2_manifest = rows(&db, "r2").await[0].3.clone().unwrap();
        let one = std::fs::metadata(blob_path(&store, &r2_manifest)).unwrap().len() + 3;
        let tight = Limits { max_store_bytes: one, ..limits };
        assert_eq!(sweep_at(&db, &store, &tight, Utc::now()).await, Ok(2));
        assert_eq!(rows(&db, "r1").await[1].1, "pruned");
        assert_eq!(rows(&db, "r2").await[0].1, "ok");
        assert!(blob_path(&store, &sha256(b"ccc")).exists());

        // A deleted run's blobs go at the next sweep.
        sqlx::query("DELETE FROM runs WHERE id = 'r2'").execute(&db).await.unwrap();
        assert_eq!(sweep_at(&db, &store, &limits, Utc::now()).await, Ok(2));
        assert_eq!(blob_count(&store), 0);
    }

    fn changes(plan: &RestorePlan) -> Vec<(&str, Change, bool)> {
        plan.changes.iter().map(|c| (c.path.as_str(), c.change, c.outside)).collect()
    }

    fn set_mtime(path: &Path, at: SystemTime) {
        std::fs::File::options().write(true).open(path).unwrap().set_modified(at).unwrap();
    }

    #[tokio::test]
    async fn a_restore_puts_the_turns_files_back_and_leaves_what_it_cannot_bring_back() {
        let (ws, store) = (git_repo("restore"), temp("store"));
        let limits = Limits { max_files: 50, max_snapshot_bytes: 1 << 10, ..SMALL };
        write(&ws, "keep.txt", b"same");
        write(&ws, "edit.txt", b"before");
        write(&ws, "gone/deep.txt", b"deleted later");
        write(&ws, "big.bin", &[1u8; 17]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            write(&ws, "run.sh", b"echo 1");
            std::fs::set_permissions(ws.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let db = db_with_run(&ws, true).await;
        assert_eq!(before_turn_at(&db, &store, "r1", &limits).await, Some(Some(1)));

        // The turn: an edit, a deletion, new files, the unmanaged file changed.
        write(&ws, "edit.txt", b"after!");
        std::fs::remove_dir_all(ws.join("gone")).unwrap();
        write(&ws, "new/made.txt", b"agent made");
        write(&ws, "huge-new.bin", &[2u8; 17]);
        write(&ws, "big.bin", &[3u8; 18]);
        #[cfg(unix)]
        write(&ws, "run.sh", b"echo 2");

        let plan = preview_at(&db, &store, "r1", 1, None, &limits).await.unwrap();
        let mut want = vec![
            ("edit.txt", Change::Restore, false),
            ("gone/deep.txt", Change::Create, false),
            ("new/made.txt", Change::Delete, false),
        ];
        if cfg!(unix) {
            want.push(("run.sh", Change::Restore, false));
        }
        assert_eq!(changes(&plan), want);
        assert_eq!(plan.kept, ["huge-new.bin"]);

        let done = restore_at(&db, &store, "r1", 1, None, &plan.fingerprint, &limits).await.unwrap();
        assert_eq!((done.restored, done.deleted, done.failed.len()), (want.len() - 1, 1, 0));
        assert_eq!(std::fs::read(ws.join("edit.txt")).unwrap(), b"before");
        assert_eq!(std::fs::read(ws.join("gone/deep.txt")).unwrap(), b"deleted later");
        assert!(!ws.join("new/made.txt").exists());
        assert_eq!(std::fs::read(ws.join("keep.txt")).unwrap(), b"same");
        // Unmanaged and too-large-to-restore files are left as they are.
        assert_eq!(std::fs::read(ws.join("big.bin")).unwrap(), [3u8; 18]);
        assert!(ws.join("huge-new.bin").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::read(ws.join("run.sh")).unwrap(), b"echo 1");
            assert_eq!(std::fs::metadata(ws.join("run.sh")).unwrap().permissions().mode() & 0o777, 0o755);
        }
        // Nothing left to do afterwards.
        assert!(preview_at(&db, &store, "r1", 1, None, &limits).await.unwrap().changes.is_empty());
    }

    #[tokio::test]
    async fn files_written_after_the_run_stopped_are_flagged_and_a_stale_preview_is_refused() {
        let (ws, store) = (git_repo("outside"), temp("store"));
        write(&ws, "agent.txt", b"v1");
        write(&ws, "user.txt", b"v1");
        let db = db_with_run(&ws, true).await;
        before_turn_at(&db, &store, "r1", &SMALL).await;

        let now = SystemTime::now();
        let stopped = now - Duration::from_secs(60);
        write(&ws, "agent.txt", b"v2");
        set_mtime(&ws.join("agent.txt"), stopped - Duration::from_secs(60));
        write(&ws, "user.txt", b"v2");
        write(&ws, "user-new.txt", b"mine");
        let plan = preview_at(&db, &store, "r1", 1, Some(stopped), &SMALL).await.unwrap();
        assert_eq!(
            changes(&plan),
            [
                ("agent.txt", Change::Restore, false),
                ("user-new.txt", Change::Delete, true),
                ("user.txt", Change::Restore, true),
            ]
        );

        // Edited again after the preview: the confirmed list no longer holds.
        write(&ws, "user.txt", b"v3, typed after the dialog opened");
        assert_eq!(
            restore_at(&db, &store, "r1", 1, Some(stopped), &plan.fingerprint, &SMALL).await,
            Err("files_changed".into())
        );
        assert_eq!(std::fs::read(ws.join("agent.txt")).unwrap(), b"v2", "nothing restored");
    }

    #[tokio::test]
    async fn only_a_kept_snapshot_restores_and_a_damaged_one_changes_nothing() {
        let (ws, store) = (git_repo("damaged"), temp("store"));
        write(&ws, "a.txt", b"one");
        write(&ws, "b.txt", b"two");
        let db = db_with_run(&ws, true).await;
        before_turn_at(&db, &store, "r1", &SMALL).await;
        assert_eq!(preview_at(&db, &store, "r1", 2, None, &SMALL).await, Err("snapshot_not_found".into()));
        assert_eq!(preview_at(&db, &store, "r2", 1, None, &SMALL).await, Err("snapshot_not_found".into()));

        write(&ws, "a.txt", b"ONE");
        write(&ws, "b.txt", b"TWO");
        let plan = preview_at(&db, &store, "r1", 1, None, &SMALL).await.unwrap();
        std::fs::remove_file(blob_path(&store, &sha256(b"two"))).unwrap();
        let got = restore_at(&db, &store, "r1", 1, None, &plan.fingerprint, &SMALL).await;
        assert_eq!(got, Err("snapshot_damaged: b.txt".into()));
        assert_eq!(std::fs::read(ws.join("a.txt")).unwrap(), b"ONE", "nothing written");

        sqlx::query(&format!("{PRUNE} WHERE run_id = 'r1'")).execute(&db).await.unwrap();
        assert_eq!(preview_at(&db, &store, "r1", 1, None, &SMALL).await, Err("snapshot_unavailable".into()));
    }

    #[tokio::test]
    async fn a_file_the_snapshot_left_out_as_ignored_is_never_deleted() {
        let (ws, store) = (git_repo("unignored"), temp("store"));
        write(&ws, ".gitignore", b".env\n");
        write(&ws, ".env", b"SECRET=1");
        write(&ws, "a.txt", b"one");
        let db = db_with_run(&ws, true).await;
        before_turn_at(&db, &store, "r1", &SMALL).await;

        // The agent stops ignoring .env, edits it and makes a new file.
        write(&ws, ".gitignore", b"");
        write(&ws, ".env", b"SECRET=2");
        write(&ws, "new.txt", b"made");
        let plan = preview_at(&db, &store, "r1", 1, None, &SMALL).await.unwrap();
        assert_eq!(changes(&plan), [(".gitignore", Change::Restore, false), ("new.txt", Change::Delete, false)]);

        restore_at(&db, &store, "r1", 1, None, &plan.fingerprint, &SMALL).await.unwrap();
        assert_eq!(std::fs::read(ws.join(".env")).unwrap(), b"SECRET=2");
        assert!(!ws.join("new.txt").exists());
    }

    #[test]
    fn a_manifest_without_the_ignored_list_deletes_only_files_written_after_it() {
        let (ws, store) = (temp("old-manifest"), temp("store"));
        write(&ws, "a.txt", b"one");
        write(&ws, "was-ignored.env", b"old");
        write(&ws, "new.txt", b"made");
        let taken = SystemTime::now() - Duration::from_secs(60);
        set_mtime(&ws.join("was-ignored.env"), taken - Duration::from_secs(60));
        // As written before the manifest listed what the walk left out.
        let manifest = format!(r#"{{"files":[{{"path":"a.txt","sha":"{}","size":3}}],"unmanaged":[]}}"#, sha256(b"one"));
        let sha = sha256(manifest.as_bytes());
        put_blob(&store, &sha, manifest.as_bytes()).unwrap();

        let planned = plan_restore(&store, &ws, &sha, &SMALL, None, taken, &mut StatCache::new()).unwrap();
        assert_eq!(changes(&planned.plan), [("new.txt", Change::Delete, false)]);
    }

    #[tokio::test]
    async fn a_file_ignored_since_the_snapshot_is_restored_as_it_is_now() {
        let (ws, store) = (git_repo("ignored-later"), temp("store"));
        write(&ws, "config/app.yml", b"v1");
        write(&ws, "config/same.yml", b"same");
        let db = db_with_run(&ws, true).await;
        before_turn_at(&db, &store, "r1", &SMALL).await;

        let stopped = SystemTime::now() - Duration::from_secs(60);
        write(&ws, ".gitignore", b"config/\n");
        write(&ws, "config/app.yml", b"v2");
        let plan = preview_at(&db, &store, "r1", 1, Some(stopped), &SMALL).await.unwrap();
        assert_eq!(
            changes(&plan),
            [(".gitignore", Change::Delete, true), ("config/app.yml", Change::Restore, true)]
        );

        // The overwrite is part of the confirmed state: editing it again is caught.
        // A new size, as two writes can share one mtime tick (Windows).
        write(&ws, "config/app.yml", b"v3, edited after the dialog opened");
        assert_eq!(
            restore_at(&db, &store, "r1", 1, Some(stopped), &plan.fingerprint, &SMALL).await,
            Err("files_changed".into())
        );
    }

    #[test]
    fn a_workspace_outside_git_still_follows_its_gitignore() {
        let (ws, store) = (temp("no-git"), temp("store"));
        write(&ws, ".gitignore", b".env\nnode_modules/\n");
        write(&ws, ".env", b"SECRET=1");
        write(&ws, "node_modules/x/index.js", b"x");
        write(&ws, "a.txt", b"one");
        let limits = Limits { max_file_bytes: 64, ..SMALL };
        let m = read_manifest(&store, &saved(capture(&store, &ws, &limits, &mut StatCache::new()).unwrap())).unwrap();
        let paths: Vec<&str> = m.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [".gitignore", "a.txt"]);
        assert_eq!(m.ignored, Some(vec![".env".to_string(), "node_modules".to_string()]));
    }

    #[test]
    fn a_restore_never_writes_outside_the_workspace() {
        let (ws, elsewhere) = (temp("inside"), temp("elsewhere"));
        let root = ws.canonicalize().unwrap();
        assert!(inside(&root, "../x", true).is_err());
        assert!(inside(&root, "/etc/passwd", true).is_err());
        assert!(inside(&root, "a/./b", true).is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&elsewhere, ws.join("link")).unwrap();
            assert!(inside(&root, "link/file.txt", true).is_err());
            assert!(inside(&root, "link/sub/file.txt", true).is_err());
            assert!(!elsewhere.join("sub").exists(), "no folder made through the link");
        }
    }
}
