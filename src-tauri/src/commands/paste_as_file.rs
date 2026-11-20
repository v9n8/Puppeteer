//! Long pastes as a `.txt` attachment (#16, RT-25). Per project (migration 0028): whether a
//! paste longer than `max_chars` (UTF-16 code units of the pasted chunk, JS `String.length`)
//! is saved to a file and attached instead of going into the composer. The composer decides;
//! this module stores the setting and writes the file.
//!
//! The file goes to `<app data>/pastes/<run id>/paste-<time>.txt`: outside the project, so the
//! repository stays clean, and under HOME, so the file viewer opens it. The folder of a session
//! goes when the session is deleted for good ([`remove_for_run`], called by the run purge).
//!
//! Remote: `save_pasted_text` allowed (the pasted text is the caller's own); the setting is
//! written by `set_project_paste_as_file`, allowed + audited like the project's other defaults.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

use crate::commands::projects::Project;
use crate::db::Db;

/// Default of a project that never set it (and of the migration).
pub const DEFAULT_MAX_CHARS: i64 = 2000;
/// Largest threshold accepted (only stops junk).
pub const MAX_MAX_CHARS: i64 = 10_000_000;
/// Largest paste written to a file, in bytes.
const MAX_TEXT_BYTES: usize = 16 << 20;
/// The project columns, read by [`PasteAsFile::from_row`] by these names.
pub const COLUMNS: &str = "paste_as_file, paste_as_file_max_chars";

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set once at startup to `<app data>/pastes`.
pub fn set_root(dir: PathBuf) {
    let _ = ROOT.set(dir);
}

fn root() -> Option<PathBuf> {
    ROOT.get().cloned()
}

/// A project's "long paste as a file" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasteAsFile {
    pub enabled: bool,
    /// A paste longer than this becomes a file; at least 1.
    pub max_chars: i64,
}

impl Default for PasteAsFile {
    fn default() -> Self {
        Self { enabled: true, max_chars: DEFAULT_MAX_CHARS }
    }
}

impl PasteAsFile {
    /// Read from a row selecting [`COLUMNS`].
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Self {
        Self { enabled: row.get("paste_as_file"), max_chars: row.get("paste_as_file_max_chars") }
    }

    /// Refuse a threshold below 1 or absurdly large.
    pub fn check(self) -> Result<Self, String> {
        if !(1..=MAX_MAX_CHARS).contains(&self.max_chars) {
            return Err(format!("The length must be a whole number from 1 to {MAX_MAX_CHARS}"));
        }
        Ok(self)
    }
}

/// The setting of `project_id`; the default when the project is gone or unreadable.
pub async fn load(db: &Db, project_id: &str) -> PasteAsFile {
    let sql = format!("SELECT {COLUMNS} FROM projects WHERE id = ?");
    match sqlx::query(&sql).bind(project_id).fetch_optional(db).await {
        Ok(Some(row)) => PasteAsFile::from_row(&row),
        Ok(None) => PasteAsFile::default(),
        Err(e) => {
            tracing::warn!(event = "paste_as_file_read_failed", project_id, error = %e);
            PasteAsFile::default()
        }
    }
}

/// Check and store `setting` as the project's.
pub async fn set(db: &Db, project_id: &str, setting: PasteAsFile) -> Result<(), String> {
    let s = setting.check()?;
    let done = sqlx::query("UPDATE projects SET paste_as_file = ?, paste_as_file_max_chars = ? WHERE id = ?")
        .bind(s.enabled)
        .bind(s.max_chars)
        .bind(project_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    if done.rows_affected() == 0 {
        return Err("Project not found".into());
    }
    Ok(())
}

/// Settings backup import: put the backup's setting on `project_id` (a bad length is left
/// out with a warning, the local setting kept). `None` (a backup from before 0028) keeps
/// the local one. True when something changed.
pub(crate) async fn import(
    db: &Db,
    project_id: &str,
    backup: Option<PasteAsFile>,
    warnings: &mut Vec<String>,
) -> Result<bool, String> {
    let Some(backup) = backup else { return Ok(false) };
    let backup = match backup.check() {
        Ok(b) => b,
        Err(e) => {
            warnings.push(format!("long paste setting left out: {e}"));
            return Ok(false);
        }
    };
    if load(db, project_id).await == backup {
        return Ok(false);
    }
    set(db, project_id, backup).await?;
    Ok(true)
}

/// Replace a project's "long paste as a file" setting. Remote: allowed + audited.
#[tauri::command]
pub async fn set_project_paste_as_file(
    db: State<'_, Db>,
    project_id: String,
    setting: PasteAsFile,
) -> Result<Project, String> {
    set(db.inner(), &project_id, setting).await?;
    crate::commands::projects::project_by_id(db.inner(), &project_id).await
}

/// A run id usable as a folder name (ids are UUIDs; this keeps a path out).
fn safe_run_id(run_id: &str) -> Result<&str, String> {
    let ok = !run_id.is_empty()
        && run_id.len() <= 100
        && run_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(run_id)
    } else {
        Err("Invalid session id".into())
    }
}

/// Write `text` to a new `paste-<time>.txt` in `<root>/<run_id>/`; the file's path.
pub fn save_in(root: &Path, run_id: &str, text: &str) -> Result<PathBuf, String> {
    let run_id = safe_run_id(run_id)?;
    if text.is_empty() {
        return Err("Nothing to save".into());
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!("The pasted text is larger than {} MB", MAX_TEXT_BYTES >> 20));
    }
    let dir = root.join(run_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    for n in 1..1000 {
        let name = if n == 1 { format!("paste-{stamp}.txt") } else { format!("paste-{stamp}-{n}.txt") };
        let path = dir.join(name);
        // `create_new`: two pastes within the same millisecond never overwrite each other.
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Could not pick a file name for the paste".into())
}

/// Remove every paste file of `run_id` under `root`.
pub fn remove_in(root: &Path, run_id: &str) {
    if let Ok(run_id) = safe_run_id(run_id) {
        let _ = std::fs::remove_dir_all(root.join(run_id));
    }
}

/// Remove the paste files of a session deleted for good.
pub fn remove_for_run(run_id: &str) {
    if let Some(root) = root() {
        remove_in(&root, run_id);
    }
}

/// Save a long paste of session `run_id` as a `.txt` file; its absolute path, for the
/// composer to attach. Remote: allowed.
#[tauri::command]
pub async fn save_pasted_text(db: State<'_, Db>, run_id: String, text: String) -> Result<String, String> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?)")
        .bind(&run_id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Session not found".into());
    }
    let root = root().ok_or("The paste folder is not ready")?;
    let path = save_in(&root, &run_id, &text)?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Migrate up to 0027 like the app does, add a project, then the rest.
    async fn db_from_before_0028() -> Db {
        let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for entry in std::fs::read_dir(&source).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.as_str() < "0028" {
                std::fs::copy(source.join(&name), dir.join(&name)).unwrap();
            }
        }
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&db).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('old', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn an_existing_project_gets_the_defaults_and_null_is_refused() {
        let db = db_from_before_0028().await;
        assert_eq!(load(&db, "old").await, PasteAsFile { enabled: true, max_chars: 2000 });
        // A new project too, and the columns take no NULL.
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('new', 'b', '/b', 'c')")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(load(&db, "new").await, PasteAsFile::default());
        for col in ["paste_as_file", "paste_as_file_max_chars"] {
            let r = sqlx::query(&format!("UPDATE projects SET {col} = NULL WHERE id = 'old'")).execute(&db).await;
            assert!(r.is_err(), "{col}");
        }
        // The project as the frontend reads it carries the setting.
        let p = crate::commands::projects::project_by_id(&db, "old").await.unwrap();
        assert_eq!(p.paste_as_file, PasteAsFile::default());
    }

    #[tokio::test]
    async fn set_checks_the_length() {
        let db = db_from_before_0028().await;
        set(&db, "old", PasteAsFile { enabled: false, max_chars: 1 }).await.unwrap();
        assert_eq!(load(&db, "old").await, PasteAsFile { enabled: false, max_chars: 1 });
        for bad in [0, -5, MAX_MAX_CHARS + 1] {
            assert!(set(&db, "old", PasteAsFile { enabled: true, max_chars: bad }).await.is_err(), "{bad}");
        }
        assert_eq!(load(&db, "old").await, PasteAsFile { enabled: false, max_chars: 1 });
        assert!(set(&db, "missing", PasteAsFile::default()).await.is_err());
    }

    #[test]
    fn save_writes_a_new_file_per_paste_and_remove_clears_the_session() {
        let root = std::env::temp_dir().join(format!("puppeteer-pastes-{}", uuid::Uuid::new_v4()));
        let a = save_in(&root, "run-1", "hello\nworld").unwrap();
        let b = save_in(&root, "run-1", "second").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "hello\nworld");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "second");
        assert!(a.starts_with(root.join("run-1")) && a.extension().unwrap() == "txt");
        let other = save_in(&root, "run-2", "x").unwrap();

        for bad in ["", "../x", "a/b", "a\\b", ".."] {
            assert!(save_in(&root, bad, "x").is_err(), "{bad:?}");
        }
        assert!(save_in(&root, "run-1", "").is_err());

        remove_in(&root, "run-1");
        assert!(!root.join("run-1").exists());
        assert!(other.exists(), "another session's pastes stay");
        remove_in(&root, "..");
        assert!(root.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
