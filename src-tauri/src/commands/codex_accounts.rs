//! Codex accounts managed by Puppeteer (UX-39), on the model of `claude_accounts`.
//!
//! Each account is its own `CODEX_HOME` (`<app data>/codex-accounts/<id>`), which
//! is also how Codex does multi-account: one login per home. The login is made by
//! `codex login` run under that home; Puppeteer never writes `auth.json` itself,
//! its shape is Codex's to own. The shared `~/.codex` login stays the default
//! until the user marks an account as default.

use crate::commands::project_links::link_if_free;
use crate::db::Db;
use crate::runs::sidecar::augment_command_path;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::process::Command;
use uuid::Uuid;

const STATUS_UNKNOWN: &str = "unknown";
const STATUS_READY: &str = "ready";
const STATUS_NEEDS_LOGIN: &str = "needs_login";

/// What an account's home shares with the global Codex home, so a session run
/// under it gets the same instructions, config and prompts as one run under
/// `~/.codex`. The login (`auth.json`), the sessions and the history stay per
/// account.
const SHARED_ENTRIES: &[&str] = &["AGENTS.md", "config.toml", "prompts"];

#[derive(Debug, Serialize, Clone)]
pub struct CodexAccount {
    pub id: String,
    pub label: String,
    pub home_dir: String,
    pub email: Option<String>,
    pub auth_method: Option<String>,
    pub status: String,
    pub is_default: bool,
    pub last_checked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CodexValidation {
    pub logged_in: bool,
    pub status: String,
    pub email: Option<String>,
    pub auth_method: Option<String>,
}

/// The account a Codex run uses: start the CLI with `CODEX_HOME = home_dir`.
// Codex runs pick it up in UX-40; this task only adds the accounts.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct CodexRuntimeAccount {
    pub id: String,
    pub home_dir: String,
}

const COLUMNS: &str = "id, label, home_dir, email, auth_method, status, is_default, last_checked_at, created_at";

fn row_to_account(row: &sqlx::sqlite::SqliteRow) -> CodexAccount {
    CodexAccount {
        id: row.get("id"),
        label: row.get("label"),
        home_dir: row.get("home_dir"),
        email: row.get("email"),
        auth_method: row.get("auth_method"),
        status: row.get("status"),
        is_default: row.get::<i64, _>("is_default") != 0,
        last_checked_at: row.get("last_checked_at"),
        created_at: row.get("created_at"),
    }
}

fn clean_label(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        Err("Codex account label is required".to_string())
    } else {
        Ok(label.to_string())
    }
}

fn unique_label_error(e: sqlx::Error, label: &str) -> String {
    if e.to_string().contains("UNIQUE") {
        format!("A Codex account labelled '{label}' already exists.")
    } else {
        e.to_string()
    }
}

pub(crate) fn codex_accounts_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("codex-accounts"))
        .map_err(|e| e.to_string())
}

async fn fetch_account(db: &Db, id: &str) -> Result<CodexAccount, String> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM codex_accounts WHERE id = ?"))
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(|_| "Codex account not found".to_string())?;
    Ok(row_to_account(&row))
}

async fn load_codex_path(db: &Db) -> Result<String, String> {
    Ok(sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'codex_path'")
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

/// Create `home` (owner-only: it will hold the login) and link the shared
/// global entries into it. Runs on create, before a login and before every
/// run, so an entry added to `~/.codex` later is picked up. A link that can't
/// be made is logged and skipped: the account still works, just without it.
fn prepare_home(home: &Path, global: Option<&Path>) -> Result<(), String> {
    std::fs::create_dir_all(home).map_err(|e| format!("{}: {e}", home.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700));
    }
    let Some(global) = global.filter(|g| *g != home) else {
        return Ok(());
    };
    for name in SHARED_ENTRIES {
        let target = global.join(name);
        if !target.exists() {
            continue;
        }
        if let Err(e) = link_if_free(home, &home.join(name), &target) {
            tracing::warn!(event = "codex_account_link_failed", home = %home.display(), entry = name, error = %e);
        }
    }
    Ok(())
}

fn global_codex_home() -> Option<PathBuf> {
    crate::commands::instruction_checks::codex_home()
}

#[tauri::command]
pub async fn list_codex_accounts(db: State<'_, Db>) -> Result<Vec<CodexAccount>, String> {
    list_accounts(db.inner()).await
}

async fn list_accounts(db: &Db) -> Result<Vec<CodexAccount>, String> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM codex_accounts ORDER BY is_default DESC, label COLLATE NOCASE ASC"
    ))
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(row_to_account).collect())
}

#[tauri::command]
pub async fn create_codex_account(
    app: AppHandle,
    db: State<'_, Db>,
    label: String,
) -> Result<CodexAccount, String> {
    let root = codex_accounts_root(&app)?;
    create_account(db.inner(), &root, global_codex_home().as_deref(), &label).await
}

/// A new account is never the default: `~/.codex` stays in use until the user
/// picks one.
async fn create_account(
    db: &Db,
    root: &Path,
    global: Option<&Path>,
    label: &str,
) -> Result<CodexAccount, String> {
    let label = clean_label(label)?;
    let id = Uuid::new_v4().to_string();
    let home = root.join(&id);
    sqlx::query(
        "INSERT INTO codex_accounts (id, label, home_dir, status, is_default, created_at) \
         VALUES (?, ?, ?, ?, 0, ?)",
    )
    .bind(&id)
    .bind(&label)
    .bind(home.to_string_lossy().as_ref())
    .bind(STATUS_UNKNOWN)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(db)
    .await
    .map_err(|e| unique_label_error(e, &label))?;
    prepare_home(&home, global)?;
    fetch_account(db, &id).await
}

#[tauri::command]
pub async fn rename_codex_account(
    db: State<'_, Db>,
    id: String,
    label: String,
) -> Result<CodexAccount, String> {
    let label = clean_label(&label)?;
    sqlx::query("UPDATE codex_accounts SET label = ? WHERE id = ?")
        .bind(&label)
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| unique_label_error(e, &label))?;
    fetch_account(db.inner(), &id).await
}

#[tauri::command]
pub async fn delete_codex_account(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
) -> Result<(), String> {
    if let Some(session) = login_session(&id) {
        session.notify_one();
    }
    let root = codex_accounts_root(&app)?;
    delete_account(db.inner(), &root, &id).await
}

/// Projects and runs on the account fall back to the default (unset), and so
/// does the default itself: no other account is promoted, `~/.codex` takes
/// over. The home goes too, but only when it is a folder of `root`, so a
/// tampered row can't point the delete anywhere else.
async fn delete_account(db: &Db, root: &Path, id: &str) -> Result<(), String> {
    let home: Option<String> = sqlx::query_scalar("SELECT home_dir FROM codex_accounts WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    for sql in [
        "UPDATE projects SET codex_account_id = NULL WHERE codex_account_id = ?",
        "UPDATE runs SET codex_account_id = NULL WHERE codex_account_id = ?",
        "DELETE FROM codex_accounts WHERE id = ?",
    ] {
        sqlx::query(sql)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;

    if let Some(home) = home.map(PathBuf::from) {
        if home.parent() == Some(root) && home.is_dir() {
            // Not followed: the shared entries are links, so `~/.codex` is untouched.
            std::fs::remove_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        }
    }
    Ok(())
}

/// `id = None` (or blank) clears the default: Codex runs go back to `~/.codex`.
#[tauri::command]
pub async fn set_default_codex_account(db: State<'_, Db>, id: Option<String>) -> Result<(), String> {
    set_default_account(db.inner(), id.as_deref()).await
}

async fn set_default_account(db: &Db, id: Option<&str>) -> Result<(), String> {
    let id = id.map(str::trim).filter(|v| !v.is_empty());
    if let Some(id) = id {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM codex_accounts WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err("Codex account not found".to_string());
        }
    }
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE codex_accounts SET is_default = (id IS ?)")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}

/// Link `project_id` to one account, or back to the default with `None` / a
/// blank id.
#[tauri::command]
pub async fn set_project_codex_account(
    db: State<'_, Db>,
    project_id: String,
    account_id: Option<String>,
) -> Result<(), String> {
    set_project_account(db.inner(), &project_id, account_id.as_deref()).await
}

async fn set_project_account(db: &Db, project_id: &str, account_id: Option<&str>) -> Result<(), String> {
    let account_id = account_id.map(str::trim).filter(|v| !v.is_empty());
    sqlx::query("UPDATE projects SET codex_account_id = ? WHERE id = ?")
        .bind(account_id)
        .bind(project_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

/// The browser login waits for its localhost callback; a device code expires
/// after 15 minutes.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Cancel switches of the `codex login` runs in flight, by account.
fn login_sessions() -> &'static std::sync::Mutex<HashMap<String, Arc<tokio::sync::Notify>>> {
    static SESSIONS: OnceLock<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Notify>>>> =
        OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

fn login_session(id: &str) -> Option<Arc<tokio::sync::Notify>> {
    login_sessions().lock().ok()?.get(id).cloned()
}

/// Removes the account's session when the login ends, however it ends.
struct LoginSessionGuard(String);

impl Drop for LoginSessionGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = login_sessions().lock() {
            map.remove(&self.0);
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexLoginUrlEvent {
    account_id: String,
    url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexLoginCodeEvent {
    account_id: String,
    code: String,
}

/// One line of `codex login` output, as the UI needs it.
#[derive(Debug, PartialEq)]
enum LoginLine {
    /// The page to open: the browser login's fallback link, or the device-code page.
    Url(String),
    /// The one-time code to enter on the device-code page.
    Code(String),
    Text(String),
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI: ESC [ params final-byte
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Reads `codex login` output line by line. The device-code flow prints the
/// code on the line after "Enter this one-time code".
#[derive(Default)]
struct LoginOutputParser {
    code_next: bool,
}

impl LoginOutputParser {
    fn line(&mut self, raw: &str) -> Option<LoginLine> {
        let line = strip_ansi(raw);
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if line.starts_with("https://") && !line.contains(char::is_whitespace) {
            return Some(LoginLine::Url(line.to_string()));
        }
        if std::mem::take(&mut self.code_next) && !line.contains(char::is_whitespace) {
            return Some(LoginLine::Code(line.to_string()));
        }
        if line.contains("one-time code") {
            self.code_next = true;
        }
        Some(LoginLine::Text(line.to_string()))
    }
}

fn pump_login_output<R>(reader: R, app: AppHandle, account_id: String) -> tokio::task::JoinHandle<String>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(reader).lines();
        let mut parser = LoginOutputParser::default();
        let mut text = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            match parser.line(&line) {
                Some(LoginLine::Url(url)) => {
                    let _ = app.emit("codex_login_url", CodexLoginUrlEvent { account_id: account_id.clone(), url });
                }
                Some(LoginLine::Code(code)) => {
                    let _ = app.emit("codex_login_code", CodexLoginCodeEvent { account_id: account_id.clone(), code });
                }
                Some(LoginLine::Text(line)) => {
                    text.push_str(&line);
                    text.push('\n');
                }
                None => {}
            }
        }
        text
    })
}

/// A `codex` command run under the account's home, with the binary found by
/// the shared Codex lookup (Codex.app's bundled CLI when none is on PATH).
async fn codex_command(db: &Db, home: &str) -> Result<Command, String> {
    let bin = crate::runs::codex_bin::resolve_codex_bin(&load_codex_path(db).await?)?;
    let mut cmd = Command::new(bin);
    // An npm install is a `#!/usr/bin/env node` script: it needs node on PATH.
    augment_command_path(&mut cmd);
    cmd.env("CODEX_HOME", home);
    Ok(cmd)
}

/// Sign an account in by running `codex login` under its home, in the
/// background. The CLI opens the browser and finishes through its localhost
/// callback; `device_auth` uses the device-code flow instead (for when that
/// callback can't be reached). Resolves once the CLI exits, with the account
/// re-validated. The page link and the device code reach the UI as
/// `codex_login_url` / `codex_login_code` events.
#[tauri::command]
pub async fn login_codex_account(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
    device_auth: Option<bool>,
) -> Result<CodexValidation, String> {
    let account = fetch_account(db.inner(), &id).await?;
    prepare_home(Path::new(&account.home_dir), global_codex_home().as_deref())?;
    let mut cmd = codex_command(db.inner(), &account.home_dir).await?;

    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut map = login_sessions().lock().map_err(|e| e.to_string())?;
        if map.contains_key(&id) {
            return Err("A Codex login is already in progress for this account.".to_string());
        }
        map.insert(id.clone(), cancel.clone());
    }
    let _guard = LoginSessionGuard(id.clone());

    cmd.arg("login");
    if device_auth.unwrap_or(false) {
        cmd.arg("--device-auth");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(home) = std::env::var_os("HOME") {
        cmd.current_dir(home);
    }

    let mut child = cmd.spawn().map_err(|e| format!("Failed to run Codex login: {e}"))?;
    let stdout = child.stdout.take().map(|r| pump_login_output(r, app.clone(), id.clone()));
    let stderr = child.stderr.take().map(|r| pump_login_output(r, app.clone(), id.clone()));

    let status = tokio::select! {
        res = tokio::time::timeout(LOGIN_TIMEOUT, child.wait()) => match res {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => return Err(format!("Codex login failed: {e}")),
            Err(_) => {
                return Err(format!("Codex login timed out after {} minutes", LOGIN_TIMEOUT.as_secs() / 60))
            }
        },
        _ = cancel.notified() => {
            let _ = child.kill().await;
            return Err("Codex login cancelled".to_string());
        }
    };

    if !status.success() {
        let mut detail = String::new();
        for task in [stderr, stdout].into_iter().flatten() {
            if let Ok(text) = task.await {
                detail.push_str(&text);
            }
        }
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            "Codex login failed.".to_string()
        } else {
            format!("Codex login failed: {detail}")
        });
    }

    validate_codex_account_inner(db.inner(), &id).await
}

#[tauri::command]
pub async fn cancel_codex_account_login(id: String) -> Result<(), String> {
    if let Some(cancel) = login_session(&id) {
        cancel.notify_one();
    }
    Ok(())
}

/// Ask `codex login status` whether the account is signed in, and store the
/// answer (the list shows the last one).
#[tauri::command]
pub async fn validate_codex_account(db: State<'_, Db>, id: String) -> Result<CodexValidation, String> {
    validate_codex_account_inner(db.inner(), &id).await
}

pub async fn validate_codex_account_inner(db: &Db, id: &str) -> Result<CodexValidation, String> {
    let account = fetch_account(db, id).await?;
    let home = Path::new(&account.home_dir);
    prepare_home(home, global_codex_home().as_deref())?;
    let mut cmd = codex_command(db, &account.home_dir).await?;
    cmd.arg("login")
        .arg("status")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), cmd.output())
        .await
        .map_err(|_| "Codex login status timed out".to_string())?
        .map_err(|e| format!("Failed to run Codex login status: {e}"))?;

    // The CLI prints the status on stderr.
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let (logged_in, auth_method) = parse_login_status(output.status.success(), &text);
    let email = if logged_in { email_from_auth(home) } else { None };
    let status = if logged_in { STATUS_READY } else { STATUS_NEEDS_LOGIN }.to_string();

    sqlx::query(
        "UPDATE codex_accounts SET email = ?, auth_method = ?, status = ?, last_checked_at = ? WHERE id = ?",
    )
    .bind(&email)
    .bind(&auth_method)
    .bind(&status)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(CodexValidation {
        logged_in,
        status,
        email,
        auth_method,
    })
}

/// `codex login status`: exit 0 with "Logged in using ChatGPT" (or "an API
/// key - sk-…" / "access token"); exit 1 with "Not logged in".
fn parse_login_status(success: bool, text: &str) -> (bool, Option<String>) {
    let Some(line) = text.lines().map(str::trim).find(|l| l.starts_with("Logged in using")) else {
        return (false, None);
    };
    if !success {
        return (false, None);
    }
    let how = &line["Logged in using".len()..];
    let method = if how.contains("ChatGPT") {
        Some("chatgpt")
    } else if how.contains("API key") {
        Some("api_key")
    } else if how.contains("access token") {
        Some("access_token")
    } else {
        None
    };
    (true, method.map(str::to_string))
}

/// The e-mail of a ChatGPT login: the `email` claim of the ID token Codex keeps
/// in the home's `auth.json`. Only that claim is read; no token leaves here.
fn email_from_auth(home: &Path) -> Option<String> {
    use base64::Engine;
    let raw = std::fs::read_to_string(home.join("auth.json")).ok()?;
    let auth: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let payload = auth.pointer("/tokens/id_token")?.as_str()?.split('.').nth(1)?;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&claims).ok()?;
    claims
        .get("email")?
        .as_str()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// The account a Codex run should use. Priority:
/// 1. the run's own account (resume);
/// 2. the project's account;
/// 3. the default account;
/// 4. `None`: the shared `~/.codex` login, as before Codex accounts existed.
///
/// The home is prepared (shared entries re-linked) on the way.
#[allow(dead_code)] // see `CodexRuntimeAccount`
pub async fn resolve_runtime_account(
    db: &Db,
    project_id: Option<&str>,
    run_account_id: Option<&str>,
) -> Result<Option<CodexRuntimeAccount>, String> {
    resolve_runtime_account_with(db, project_id, run_account_id, global_codex_home().as_deref()).await
}

async fn resolve_runtime_account_with(
    db: &Db,
    project_id: Option<&str>,
    run_account_id: Option<&str>,
    global: Option<&Path>,
) -> Result<Option<CodexRuntimeAccount>, String> {
    let run_account_id = run_account_id.map(str::trim).filter(|v| !v.is_empty());
    let project_account_id = match (run_account_id, project_id.map(str::trim).filter(|v| !v.is_empty())) {
        (None, Some(project_id)) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT codex_account_id FROM projects WHERE id = ?",
        )
        .bind(project_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|v| !v.trim().is_empty()),
        _ => None,
    };
    let account_id = match run_account_id.map(str::to_string).or(project_account_id) {
        Some(id) => Some(id),
        None => sqlx::query_scalar::<_, String>(
            "SELECT id FROM codex_accounts WHERE is_default = 1 ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?,
    };
    let Some(account_id) = account_id else {
        return Ok(None);
    };

    let home: Option<String> = sqlx::query_scalar("SELECT home_dir FROM codex_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(home_dir) = home.filter(|h| !h.trim().is_empty()) else {
        return if run_account_id.is_some() {
            Err("Codex account saved on this run no longer exists".to_string())
        } else {
            Ok(None)
        };
    };
    prepare_home(Path::new(&home_dir), global)?;
    Ok(Some(CodexRuntimeAccount { id: account_id, home_dir }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer_codex_accounts_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn account_of(db: &Db, table: &str, id: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(&format!("SELECT codex_account_id FROM {table} WHERE id = ?"))
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn create_rename_and_delete() {
        let db = db().await;
        let root = scratch("crud");
        let a = create_account(&db, &root, None, "  work ").await.unwrap();
        assert_eq!(a.label, "work");
        assert_eq!(a.status, STATUS_UNKNOWN);
        assert!(!a.is_default, "~/.codex stays the default");
        assert_eq!(Path::new(&a.home_dir), root.join(&a.id));
        assert!(root.join(&a.id).is_dir());

        assert!(create_account(&db, &root, None, "work").await.unwrap_err().contains("already exists"));
        assert!(create_account(&db, &root, None, " ").await.is_err());

        let b = create_account(&db, &root, None, "home").await.unwrap();
        set_project_account(&db, "p1", Some(&a.id)).await.unwrap();
        sqlx::query("UPDATE runs SET codex_account_id = ? WHERE id = 'r1'")
            .bind(&a.id)
            .execute(&db)
            .await
            .unwrap();
        set_default_account(&db, Some(&a.id)).await.unwrap();

        delete_account(&db, &root, &a.id).await.unwrap();
        assert!(!root.join(&a.id).exists());
        assert_eq!(account_of(&db, "projects", "p1").await, None);
        assert_eq!(account_of(&db, "runs", "r1").await, None);
        let left = list_accounts(&db).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, b.id);
        assert!(!left[0].is_default, "no other account is promoted");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn delete_never_removes_a_folder_outside_the_root() {
        let db = db().await;
        let root = scratch("outside");
        let elsewhere = scratch("outside_other");
        sqlx::query(
            "INSERT INTO codex_accounts (id, label, home_dir, created_at) VALUES ('x', 'x', ?, 'c')",
        )
        .bind(elsewhere.to_string_lossy().as_ref())
        .execute(&db)
        .await
        .unwrap();
        delete_account(&db, &root, "x").await.unwrap();
        assert!(elsewhere.is_dir());
        assert!(list_accounts(&db).await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[tokio::test]
    async fn one_default_at_most_and_none_clears_it() {
        let db = db().await;
        let root = scratch("default");
        let a = create_account(&db, &root, None, "a").await.unwrap();
        let b = create_account(&db, &root, None, "b").await.unwrap();
        set_default_account(&db, Some(&a.id)).await.unwrap();
        set_default_account(&db, Some(&b.id)).await.unwrap();
        let defaults: Vec<String> = list_accounts(&db)
            .await
            .unwrap()
            .into_iter()
            .filter(|x| x.is_default)
            .map(|x| x.id)
            .collect();
        assert_eq!(defaults, vec![b.id.clone()]);
        assert_eq!(list_accounts(&db).await.unwrap()[0].id, b.id, "default listed first");

        assert!(set_default_account(&db, Some("missing")).await.is_err());
        set_default_account(&db, None).await.unwrap();
        assert!(list_accounts(&db).await.unwrap().iter().all(|x| !x.is_default));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn home_links_the_shared_global_entries_only() {
        let root = scratch("links");
        let global = root.join("global");
        std::fs::create_dir_all(global.join("prompts")).unwrap();
        for f in ["AGENTS.md", "config.toml", "auth.json", "history.jsonl"] {
            std::fs::write(global.join(f), f).unwrap();
        }
        std::fs::create_dir_all(global.join("sessions")).unwrap();
        let home = root.join("acct");
        // The account's own config is never replaced.
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), "mine").unwrap();

        prepare_home(&home, Some(&global)).unwrap();
        assert_eq!(std::fs::read_link(home.join("AGENTS.md")).unwrap(), global.join("AGENTS.md"));
        assert_eq!(std::fs::read_link(home.join("prompts")).unwrap(), global.join("prompts"));
        assert_eq!(std::fs::read_to_string(home.join("config.toml")).unwrap(), "mine");
        for own in ["auth.json", "history.jsonl", "sessions"] {
            assert!(std::fs::symlink_metadata(home.join(own)).is_err(), "{own}");
        }
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&home).unwrap().permissions().mode() & 0o777, 0o700);
        }

        // Run again: an entry added to the global home later gets linked, the rest is as it was.
        std::fs::remove_file(home.join("config.toml")).unwrap();
        prepare_home(&home, Some(&global)).unwrap();
        assert_eq!(std::fs::read_link(home.join("config.toml")).unwrap(), global.join("config.toml"));
        assert_eq!(std::fs::read_link(home.join("AGENTS.md")).unwrap(), global.join("AGENTS.md"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn deleting_an_account_leaves_the_global_home_alone() {
        let db = db().await;
        let root = scratch("delete_links");
        let global = root.join("global");
        std::fs::create_dir_all(global.join("prompts")).unwrap();
        std::fs::write(global.join("prompts/p.md"), "p").unwrap();
        std::fs::write(global.join("AGENTS.md"), "a").unwrap();
        let accounts = root.join("accounts");
        let a = create_account(&db, &accounts, Some(&global), "a").await.unwrap();
        assert!(std::fs::read_link(Path::new(&a.home_dir).join("prompts")).is_ok());

        delete_account(&db, &accounts, &a.id).await.unwrap();
        assert!(!Path::new(&a.home_dir).exists());
        assert_eq!(std::fs::read_to_string(global.join("prompts/p.md")).unwrap(), "p");
        assert_eq!(std::fs::read_to_string(global.join("AGENTS.md")).unwrap(), "a");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn runtime_account_is_run_then_project_then_default_then_global() {
        let db = db().await;
        let root = scratch("resolve");
        let resolve = |project: Option<&'static str>, run: Option<String>| {
            let db = db.clone();
            async move {
                resolve_runtime_account_with(&db, project, run.as_deref(), None)
                    .await
                    .map(|a| a.map(|a| a.id))
            }
        };
        // No account at all: the shared ~/.codex login.
        assert_eq!(resolve(Some("p1"), None).await.unwrap(), None);

        let def = create_account(&db, &root, None, "default").await.unwrap();
        let proj = create_account(&db, &root, None, "project").await.unwrap();
        let run = create_account(&db, &root, None, "run").await.unwrap();
        // Accounts exist but none is default: still ~/.codex.
        assert_eq!(resolve(Some("p1"), None).await.unwrap(), None);

        set_default_account(&db, Some(&def.id)).await.unwrap();
        assert_eq!(resolve(Some("p1"), None).await.unwrap(), Some(def.id.clone()));
        assert_eq!(resolve(None, None).await.unwrap(), Some(def.id.clone()));

        set_project_account(&db, "p1", Some(&proj.id)).await.unwrap();
        assert_eq!(resolve(Some("p1"), None).await.unwrap(), Some(proj.id.clone()));
        assert_eq!(resolve(Some("p2"), None).await.unwrap(), Some(def.id.clone()));

        assert_eq!(resolve(Some("p1"), Some(run.id.clone())).await.unwrap(), Some(run.id.clone()));
        // A blank run account counts as none.
        assert_eq!(resolve(Some("p1"), Some("  ".into())).await.unwrap(), Some(proj.id.clone()));

        // The run's account is gone: an error, not a silent switch to another login.
        assert!(resolve(Some("p1"), Some("gone".into())).await.is_err());

        set_project_account(&db, "p1", None).await.unwrap();
        set_default_account(&db, None).await.unwrap();
        assert_eq!(resolve(Some("p1"), None).await.unwrap(), None);

        // The home comes back if it was removed by hand.
        set_default_account(&db, Some(&def.id)).await.unwrap();
        std::fs::remove_dir_all(&def.home_dir).unwrap();
        resolve(None, None).await.unwrap();
        assert!(Path::new(&def.home_dir).is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_login_status() {
        assert_eq!(parse_login_status(true, "\nLogged in using ChatGPT\n"), (true, Some("chatgpt".into())));
        assert_eq!(
            parse_login_status(true, "Logged in using an API key - sk-proj-***abcd"),
            (true, Some("api_key".into()))
        );
        assert_eq!(parse_login_status(true, "Logged in using access token"), (true, Some("access_token".into())));
        assert_eq!(parse_login_status(false, "Not logged in"), (false, None));
        assert_eq!(parse_login_status(false, "Logged in using ChatGPT"), (false, None));
        assert_eq!(parse_login_status(true, ""), (false, None));
    }

    #[test]
    fn reads_login_links_and_the_device_code() {
        let mut p = LoginOutputParser::default();
        assert_eq!(p.line("Starting local login server on http://localhost:1455."), Some(LoginLine::Text("Starting local login server on http://localhost:1455.".into())));
        assert_eq!(p.line(""), None);
        assert_eq!(
            p.line("https://auth.openai.com/oauth/authorize?client_id=x&state=y"),
            Some(LoginLine::Url("https://auth.openai.com/oauth/authorize?client_id=x&state=y".into()))
        );

        let mut p = LoginOutputParser::default();
        p.line("1. Open this link in your browser and sign in to your account");
        assert_eq!(
            p.line("   \u{1b}[94mhttps://auth.openai.com/codex/device\u{1b}[0m"),
            Some(LoginLine::Url("https://auth.openai.com/codex/device".into()))
        );
        p.line("2. Enter this one-time code \u{1b}[90m(expires in 15 minutes)\u{1b}[0m");
        assert_eq!(p.line("   \u{1b}[94mABCD-12345\u{1b}[0m"), Some(LoginLine::Code("ABCD-12345".into())));
        // Only the line right after is the code.
        assert_eq!(p.line("XYZ"), Some(LoginLine::Text("XYZ".into())));
        // A plain-http or non-URL line is never offered as a link.
        assert_eq!(p.line("http://localhost:1455"), Some(LoginLine::Text("http://localhost:1455".into())));
    }

    #[test]
    fn email_comes_from_the_id_token_claim_only() {
        use base64::Engine;
        let root = scratch("email");
        assert_eq!(email_from_auth(&root), None);
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"me@example.com","sub":"x"}"#);
        std::fs::write(
            root.join("auth.json"),
            format!(r#"{{"tokens":{{"id_token":"h.{claims}.s","access_token":"secret"}}}}"#),
        )
        .unwrap();
        assert_eq!(email_from_auth(&root).as_deref(), Some("me@example.com"));
        std::fs::write(root.join("auth.json"), r#"{"OPENAI_API_KEY":"sk-x"}"#).unwrap();
        assert_eq!(email_from_auth(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
