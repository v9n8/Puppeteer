//! Live mirroring of the shared Claude/Codex transcript stores into Puppeteer.
//!
//! Both engines persist conversations to a store their CLIs / VS Code extensions
//! also write — Claude at `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`,
//! Codex at `~/.codex/sessions/<date>/rollout-<ts>-<thread-id>.jsonl`. This
//! watcher tails both so anything done OUTSIDE Puppeteer (a new session, or more
//! turns added to one Puppeteer started) shows up without a manual refresh.
//!
//! On a debounced change it maps the file back to a project and upserts the
//! session into Puppeteer's run/log/usage model, then emits `sessions:changed` (with
//! the legacy `claude:sessions_changed` alias) so the UI refetches the run list
//! and reloads the open run. Live runs are left untouched — the upsert skips
//! sessions owned by a still-running Puppeteer run.
//!
//! Each store is watched only while its switch (`sessions.mirror_claude`,
//! `sessions.mirror_codex`) is on; flipping one restarts the watcher at once.

use crate::commands::codex_sessions::{rollout_meta, upsert_codex_session_file, CODEX_MIRROR};
use crate::commands::sessions::{
    encode_project_dir, upsert_claude_session, SyncOutcome, CLAUDE_MIRROR,
};
use crate::features::{self, apply::Change};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Coalesce the burst of events the engines emit while appending to a transcript.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// The OS watcher now running. Dropping it unregisters every watch and closes
/// its channel, which ends the batching thread. The lock is held while the
/// switches are read, so racing restarts end on the latest write.
static WATCHER: tokio::sync::Mutex<Option<RecommendedWatcher>> =
    tokio::sync::Mutex::const_new(None);

/// Start watching the transcript stores whose switch is on (app setup). A
/// failure to set up the watcher is logged and otherwise ignored (the on-focus /
/// project-open reconcile paths still keep Puppeteer in sync).
pub fn start(db: Pool<Sqlite>, app: AppHandle) {
    tauri::async_runtime::block_on(follow(db, app));
}

/// `features::apply` hook: re-watch as soon as either mirror switch changes,
/// including through its module.
pub fn on_features_changed(app: &AppHandle, change: &Change<'_>) {
    let same = |id| features::effective(change.before, id) == features::effective(change.after, id);
    if same(CLAUDE_MIRROR) && same(CODEX_MIRROR) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let db = app.state::<crate::db::Db>().inner().clone();
        follow(db, app).await;
    });
}

/// Replace the running watcher with one over the stores the switches allow now.
async fn follow(db: Pool<Sqlite>, app: AppHandle) {
    let mut slot = WATCHER.lock().await;
    *slot = None;
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let claude = features::is_on(&db, CLAUDE_MIRROR).await;
    let codex = features::is_on(&db, CODEX_MIRROR).await;
    let accounts = if claude {
        crate::commands::claude_accounts::account_config_dirs(&db).await
    } else {
        Vec::new()
    };
    let roots = watch_roots(Path::new(&home), &accounts, claude, codex);
    *slot = watch(&roots, move |path| {
        let db = db.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            handle_change(&db, &app, &path).await;
        });
    });
}

/// The directories to watch: `~/.claude/projects` plus every managed Claude
/// account's `projects` while `claude` is on (so external CLI/VS Code sessions
/// run under a multi-account profile mirror in live too, AC-011; accounts added
/// later are still picked up by the on-open reconcile), and `~/.codex/sessions`
/// while `codex` is on.
fn watch_roots(home: &Path, account_dirs: &[String], claude: bool, codex: bool) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if claude {
        roots.push(home.join(".claude").join("projects"));
        roots.extend(account_dirs.iter().map(|cfg| PathBuf::from(cfg).join("projects")));
    }
    if codex {
        roots.push(home.join(".codex").join("sessions"));
    }
    roots
}

/// Watch the `roots` that exist, handing each touched transcript to
/// `on_change` from a background batching thread. `None` when there is nothing
/// to watch or the OS watcher cannot be created.
fn watch(roots: &[PathBuf], on_change: impl Fn(PathBuf) + Send + 'static) -> Option<RecommendedWatcher> {
    let roots: Vec<&PathBuf> = roots.iter().filter(|r| r.is_dir()).collect();
    if roots.is_empty() {
        return None;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[session_watcher] failed to create watcher: {e}");
            return None;
        }
    };
    for root in roots {
        if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
            eprintln!("[session_watcher] failed to watch {}: {e}", root.display());
        }
    }

    std::thread::spawn(move || loop {
        // Block until something changes, then drain a short debounce window
        // so a flurry of line-appends becomes one reconcile per file.
        let first = match rx.recv() {
            Ok(ev) => ev,
            Err(_) => break, // watcher dropped — switched off or app shutting down
        };
        let mut files: HashSet<PathBuf> = HashSet::new();
        collect(first, &mut files);
        let deadline = std::time::Instant::now() + DEBOUNCE;
        while let Ok(ev) =
            rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            collect(ev, &mut files);
        }
        for path in files {
            on_change(path);
        }
    });
    Some(watcher)
}

/// Collect touched `*.jsonl` transcript paths from a watcher event.
fn collect(res: notify::Result<notify::Event>, out: &mut HashSet<PathBuf>) {
    let Ok(event) = res else { return };
    for path in event.paths {
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.insert(path);
        }
    }
}

/// Map a changed transcript back to its project and mirror the session in,
/// dispatching on which store the file lives in.
async fn handle_change(db: &Pool<Sqlite>, app: &AppHandle, path: &Path) {
    let is_codex = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rollout-"));

    if is_codex {
        // Codex: the project is recorded as `cwd` inside the rollout. We already
        // hold the file path, so pass it through (no tree walk to locate it).
        let Some((cwd, thread_id)) = rollout_meta(path) else {
            return;
        };
        upsert_for_project(db, app, ProjectMatch::Cwd(&cwd), &thread_id, EngineCall::Codex(path)).await;
        // Plan rate-limits are account-global (not per-project): refresh the Codex
        // budget snapshot whenever any rollout gains new usage data, so the badge
        // stays current during live runs and from external codex CLI use.
        if let Some(rl) = crate::commands::codex_sessions::latest_codex_rate_limits(path) {
            if crate::commands::codex_sessions::persist_codex_plan_usage(db, &rl).await {
                let _ = app.emit("plan_usage_updated", serde_json::json!({ "provider": "codex" }));
            }
        }
    } else {
        // Claude: the parent directory name is the encoded project path.
        let Some(dir_name) = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
        else {
            return;
        };
        let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) else {
            return;
        };
        upsert_for_project(db, app, ProjectMatch::EncodedDir(dir_name), session_id, EngineCall::Claude).await;
    }
}

enum ProjectMatch<'a> {
    /// Match a project whose `encode_project_dir(path)` equals this dir name.
    EncodedDir(&'a str),
    /// Match a project whose `path` equals this cwd.
    Cwd(&'a str),
}

enum EngineCall<'a> {
    Claude,
    /// Carries the already-known rollout path so the upsert skips the tree walk.
    Codex(&'a Path),
}

/// Find the project a transcript belongs to and upsert the session, emitting on
/// a real change.
async fn upsert_for_project(
    db: &Pool<Sqlite>,
    app: &AppHandle,
    matcher: ProjectMatch<'_>,
    session_id: &str,
    engine: EngineCall<'_>,
) {
    use sqlx::Row;

    let Ok(projects) = sqlx::query("SELECT id, name, path FROM projects")
        .fetch_all(db)
        .await
    else {
        return;
    };

    for row in projects {
        let path: String = row.get("path");
        let matched = match matcher {
            ProjectMatch::EncodedDir(dir) => encode_project_dir(&path) == dir,
            ProjectMatch::Cwd(cwd) => path == cwd,
        };
        if !matched {
            continue;
        }
        let project_id: String = row.get("id");
        let project_name: String = row.get("name");
        let outcome = match engine {
            EngineCall::Codex(file) => {
                upsert_codex_session_file(db, &project_id, &project_name, &path, session_id, file, true).await
            }
            EngineCall::Claude => {
                upsert_claude_session(db, &project_id, &project_name, &path, session_id, true).await
            }
        };
        if let Ok(SyncOutcome::Imported(run_id)) | Ok(SyncOutcome::Updated(run_id)) = outcome {
            let payload = serde_json::json!({
                "project_id": project_id,
                "session_id": session_id,
                "run_id": run_id,
            });
            // Engine-neutral name (used for both Claude and Codex); the old
            // `claude:` name is kept as an alias for any older listener.
            let _ = app.emit("sessions:changed", &payload);
            let _ = app.emit("claude:sessions_changed", &payload);
        }
        return; // a project is uniquely identified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_roots_follow_the_switches() {
        let home = Path::new("/h");
        let accounts = ["/acc".to_string()];
        let roots = |claude, codex| watch_roots(home, &accounts, claude, codex);
        let claude = [PathBuf::from("/h/.claude/projects"), PathBuf::from("/acc/projects")];
        let codex = PathBuf::from("/h/.codex/sessions");

        assert_eq!(roots(true, true), [claude[0].clone(), claude[1].clone(), codex.clone()]);
        assert_eq!(roots(true, false), claude);
        assert_eq!(roots(false, true), [codex]);
        assert!(roots(false, false).is_empty());
    }

    #[test]
    fn dropping_the_watcher_ends_its_thread() {
        let dir = std::env::temp_dir().join(format!("puppeteer_watch_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(watch(&[dir.join("missing")], |_| {}).is_none(), "nothing to watch");

        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        let watcher = watch(&[dir.clone()], move |p| {
            let _ = tx.send(p);
        })
        .expect("watching");
        drop(watcher);
        // The thread owns `on_change` (and so `tx`): the channel closes only
        // once the thread has exited.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let ended = loop {
            match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
                Ok(_) => continue,
                Err(e) => break e == std::sync::mpsc::RecvTimeoutError::Disconnected,
            }
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ended, "thread still running after the watcher was dropped");
    }
}
