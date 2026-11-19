//! Session flags (V3-18): one coloured label per session ("To Do",
//! "In Progress", …) that the History rows show and filter by.
//!
//! Every flag is a row of `session_flags` (migration 0023): shared when
//! `project_id` is NULL, else that project's own. A project hides shared flags
//! it does not use (`project_hidden_flags`); a session that already has a
//! hidden flag keeps it. A session's flag is `runs.flag_id` (NULL = "No flag");
//! deleting a flag clears it from its sessions. The defaults
//! ([`DEFAULT_FLAGS`]) are rows too, with their key as id and `builtin_key`,
//! put back by [`restore_defaults`] when deleted.
//!
//! Remote Control: `list_session_flags` is allowed; the commands that change a
//! flag's definition are allowed and audited; `set_run_flag` is allowed and not
//! audited, like `set_run_pinned`: it labels a session and changes nothing that
//! runs, and dragging a board card calls it often.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::projects::{check_appearance, PROJECT_COLORS};
use crate::db::Db;

/// A flag definition changed: every window (pop-outs, a remote-desktop window
/// through the bridge) reloads the scope. The payload's `project_id` is `None`
/// for the shared flags (which every project lists too).
pub const CHANGED_EVENT: &str = "session-flags:changed";

/// Longest flag name accepted: the History row has little room for the chip.
/// `src/lib/sessionFlags.ts` mirrors it.
pub const NAME_MAX_CHARS: usize = 24;

/// A default flag as migration 0025 seeds it. The name is the English UI string:
/// while a row still has it the UI shows the translated one.
#[derive(Debug, Clone, Copy)]
pub struct DefaultFlag {
    pub key: &'static str,
    pub name: &'static str,
    pub color: &'static str,
}

/// The defaults, in the seeded order (positions 0, 1, …). Must match migration
/// 0025, which replaced the set 0023 seeded (a test checks it). A restored default goes to the end instead.
pub const DEFAULT_FLAGS: &[DefaultFlag] = &[
    DefaultFlag { key: "backlog", name: "Backlog", color: "neutral" },
    DefaultFlag { key: "todo", name: "To Do", color: "primary" },
    DefaultFlag { key: "in_progress", name: "In Progress", color: "info" },
    DefaultFlag { key: "in_review", name: "In Review", color: "warning" },
    DefaultFlag { key: "done", name: "Done", color: "success" },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SessionFlag {
    pub id: String,
    /// NULL: a shared flag.
    pub project_id: Option<String>,
    pub name: String,
    /// A `PROJECT_COLORS` token.
    pub color: String,
    pub position: i64,
    /// The default this row is (`backlog`, …); `None` for the user's flags.
    pub builtin_key: Option<String>,
    /// A default whose name is still the seeded one: the UI shows it translated.
    #[sqlx(skip)]
    pub stock_name: bool,
    /// Listed for a project: a shared flag that project hides.
    pub hidden: bool,
    /// Sessions outside the Trash that have the flag: in the listed project
    /// only, or in every project when the shared list is asked for.
    pub run_count: i64,
}

impl SessionFlag {
    fn with_stock_name(mut self) -> Self {
        self.stock_name = self.builtin_key.as_deref().and_then(default_flag).is_some_and(|d| d.name == self.name);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionFlagList {
    /// Shared flags, then (when a project is given) that project's own.
    pub flags: Vec<SessionFlag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteFlagOutcome {
    /// Sessions that had the flag and now have none.
    pub cleared_runs: u64,
}

#[derive(Debug, Clone, Serialize)]
struct Changed<'a> {
    project_id: Option<&'a str>,
}

fn emit_changed(app: &AppHandle, project_id: Option<&str>) {
    let _ = app.emit(CHANGED_EVENT, Changed { project_id });
}

/// The default with key `key`, as seeded.
pub fn default_flag(key: &str) -> Option<&'static DefaultFlag> {
    DEFAULT_FLAGS.iter().find(|d| d.key == key)
}

// ---- Rules (pure) -----------------------------------------------------------------

/// The trimmed name, or why it cannot be a flag's.
pub(crate) fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The flag needs a name".into());
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(format!("The name is longer than {NAME_MAX_CHARS} characters"));
    }
    Ok(name.to_string())
}

/// A colour of the project tab palette (blank is not "no colour" here).
pub(crate) fn check_color(color: &str) -> Result<(), String> {
    if color.is_empty() {
        return Err("The flag needs a color".into());
    }
    check_appearance("color", Some(color), PROJECT_COLORS)
}

/// Whether a session of `run_project` may get a flag of scope `flag_project`
/// (`None` = shared) that the project hides or not.
pub(crate) fn check_taggable(run_project: &str, flag_project: Option<&str>, hidden: bool) -> Result<(), String> {
    match flag_project {
        None if hidden => Err("This project hides that flag".into()),
        None => Ok(()),
        Some(p) if p == run_project => Ok(()),
        Some(_) => Err("That flag belongs to another project".into()),
    }
}

/// `ids` must be the scope's flags, each once, in the new order.
pub(crate) fn check_reorder(current: &[String], ids: &[String]) -> Result<(), String> {
    let mut want: Vec<&String> = current.iter().collect();
    let mut got: Vec<&String> = ids.iter().collect();
    want.sort();
    got.sort();
    if want != got {
        return Err("The order must list every flag of this scope exactly once".into());
    }
    Ok(())
}

// ---- Data ---------------------------------------------------------------------------

/// Shared flags, plus `project_id`'s own when given, with its hidden marks and
/// session counts.
pub async fn list(db: &Db, project_id: Option<&str>) -> Result<Vec<SessionFlag>, String> {
    let rows: Vec<SessionFlag> = sqlx::query_as(
        "SELECT f.id, f.project_id, f.name, f.color, f.position, f.builtin_key, \
                EXISTS (SELECT 1 FROM project_hidden_flags h WHERE h.flag_id = f.id AND h.project_id = ?) AS hidden, \
                (SELECT COUNT(*) FROM runs r JOIN projects p ON p.id = r.project_id \
                 WHERE r.flag_id = f.id AND r.deleted_at IS NULL AND p.deleted_at IS NULL \
                   AND (? IS NULL OR r.project_id = ?)) AS run_count \
         FROM session_flags f WHERE f.project_id IS NULL OR f.project_id = ? \
         ORDER BY f.project_id IS NOT NULL, f.position, f.name COLLATE NOCASE",
    )
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(SessionFlag::with_stock_name).collect())
}

/// The scope (`None` = shared) of flag `id`; `Err` when there is no such flag.
async fn scope_of(db: &Db, id: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT project_id FROM session_flags WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Flag not found".to_string())
}

/// Flag `id` as its own scope lists it.
async fn get(db: &Db, id: &str) -> Result<SessionFlag, String> {
    let scope = scope_of(db, id).await?;
    list(db, scope.as_deref())
        .await?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| "Flag not found".to_string())
}

async fn project_exists(db: &Db, project_id: &str) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

/// Whether another flag of the same scope already has `name`.
async fn name_taken(db: &Db, project_id: Option<&str>, name: &str, except_id: Option<&str>) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM session_flags \
         WHERE IFNULL(project_id, '') = IFNULL(?, '') AND name = ? COLLATE NOCASE AND id IS NOT ?",
    )
    .bind(project_id)
    .bind(name)
    .bind(except_id)
    .fetch_one(db)
    .await
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

/// Add a flag at the end of its scope.
pub async fn create(db: &Db, project_id: Option<&str>, name: &str, color: &str) -> Result<SessionFlag, String> {
    let name = check_name(name)?;
    check_color(color)?;
    let project_id = project_id.map(str::trim).filter(|p| !p.is_empty());
    if let Some(pid) = project_id {
        if !project_exists(db, pid).await? {
            return Err("Project not found".into());
        }
    }
    if name_taken(db, project_id, &name, None).await? {
        return Err(format!("A flag named '{name}' already exists"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO session_flags (id, project_id, name, color, position, created_at, updated_at) \
         VALUES (?, ?, ?, ?, \
                 (SELECT IFNULL(MAX(position), -1) + 1 FROM session_flags WHERE IFNULL(project_id, '') = IFNULL(?, '')), \
                 ?, ?)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(&name)
    .bind(color)
    .bind(project_id)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    get(db, &id).await
}

/// Rename and / or recolour a flag (`None` keeps the field). Its scope does not change.
pub async fn update(db: &Db, id: &str, name: Option<&str>, color: Option<&str>) -> Result<SessionFlag, String> {
    let name = name.map(check_name).transpose()?;
    if let Some(color) = color {
        check_color(color)?;
    }
    let scope = scope_of(db, id).await?;
    if let Some(name) = &name {
        if name_taken(db, scope.as_deref(), name, Some(id)).await? {
            return Err(format!("A flag named '{name}' already exists"));
        }
    }
    sqlx::query(
        "UPDATE session_flags SET name = IFNULL(?, name), color = IFNULL(?, color), updated_at = ? WHERE id = ?",
    )
    .bind(name)
    .bind(color)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    get(db, id).await
}

/// Delete a flag: its sessions become "No flag" and the projects hiding it drop
/// the mark (the trigger of migration 0023). Returns the scope it was in too.
pub async fn delete(db: &Db, id: &str) -> Result<(DeleteFlagOutcome, Option<String>), String> {
    let scope = scope_of(db, id).await?;
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let cleared = sqlx::query("UPDATE runs SET flag_id = NULL WHERE flag_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    sqlx::query("DELETE FROM session_flags WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok((DeleteFlagOutcome { cleared_runs: cleared }, scope))
}

/// Put the flags of a scope in the order of `ids` (all of them, each once).
pub async fn reorder(db: &Db, project_id: Option<&str>, ids: &[String]) -> Result<(), String> {
    let current: Vec<String> =
        sqlx::query_scalar("SELECT id FROM session_flags WHERE IFNULL(project_id, '') = IFNULL(?, '')")
            .bind(project_id)
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    check_reorder(&current, ids)?;
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    for (position, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE session_flags SET position = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

/// Put back the deleted defaults at the end of the shared flags, as seeded; one
/// still there is left as it is, edited or not. A default whose name a shared
/// flag of the user's took meanwhile comes back as "Name (2)", …. Returns the
/// flags put back.
pub async fn restore_defaults(db: &Db) -> Result<Vec<SessionFlag>, String> {
    let mut restored = Vec::new();
    for seed in DEFAULT_FLAGS {
        let present: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_flags WHERE id = ? OR builtin_key = ?")
            .bind(seed.key)
            .bind(seed.key)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?;
        if present > 0 {
            continue;
        }
        let mut name = seed.name.to_string();
        let mut n = 2;
        while name_taken(db, None, &name, None).await? {
            name = format!("{} ({n})", seed.name);
            n += 1;
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO session_flags (id, project_id, name, color, position, builtin_key, created_at, updated_at) \
             VALUES (?, NULL, ?, ?, (SELECT IFNULL(MAX(position), -1) + 1 FROM session_flags WHERE project_id IS NULL), \
                     ?, ?, ?)",
        )
        .bind(seed.key)
        .bind(&name)
        .bind(seed.color)
        .bind(seed.key)
        .bind(&now)
        .bind(&now)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
        restored.push(get(db, seed.key).await?);
    }
    Ok(restored)
}

/// Hide (or show again) shared flag `flag_id` in `project_id`'s menus.
pub async fn set_hidden(db: &Db, project_id: &str, flag_id: &str, hidden: bool) -> Result<(), String> {
    if scope_of(db, flag_id).await?.is_some() {
        return Err("Only a shared flag can be hidden in a project".into());
    }
    if !project_exists(db, project_id).await? {
        return Err("Project not found".into());
    }
    let sql = if hidden {
        "INSERT OR IGNORE INTO project_hidden_flags (project_id, flag_id) VALUES (?, ?)"
    } else {
        "DELETE FROM project_hidden_flags WHERE project_id = ? AND flag_id = ?"
    };
    sqlx::query(sql)
        .bind(project_id)
        .bind(flag_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Give session `run_id` the flag `flag_id`, or none: a shared flag its project
/// does not hide, or one of that project's own.
pub async fn tag_run(db: &Db, run_id: &str, flag_id: Option<&str>) -> Result<(), String> {
    let run_project: String = sqlx::query_scalar("SELECT project_id FROM runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Session not found".to_string())?;
    if let Some(flag_id) = flag_id {
        let scope = scope_of(db, flag_id).await?;
        let hidden: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM project_hidden_flags WHERE project_id = ? AND flag_id = ?)",
        )
        .bind(&run_project)
        .bind(flag_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
        check_taggable(&run_project, scope.as_deref(), hidden)?;
    }
    sqlx::query("UPDATE runs SET flag_id = ? WHERE id = ?")
        .bind(flag_id)
        .bind(run_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---- Commands -------------------------------------------------------------------

/// The flags usable in `project_id` (shared only when `None`).
#[tauri::command]
pub async fn list_session_flags(db: State<'_, Db>, project_id: Option<String>) -> Result<SessionFlagList, String> {
    Ok(SessionFlagList { flags: list(db.inner(), project_id.as_deref()).await? })
}

#[tauri::command]
pub async fn create_session_flag(
    app: AppHandle,
    db: State<'_, Db>,
    project_id: Option<String>,
    name: String,
    color: String,
) -> Result<SessionFlag, String> {
    let flag = create(db.inner(), project_id.as_deref(), &name, &color).await?;
    emit_changed(&app, flag.project_id.as_deref());
    Ok(flag)
}

#[tauri::command]
pub async fn update_session_flag(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<SessionFlag, String> {
    let flag = update(db.inner(), &id, name.as_deref(), color.as_deref()).await?;
    emit_changed(&app, flag.project_id.as_deref());
    Ok(flag)
}

#[tauri::command]
pub async fn delete_session_flag(app: AppHandle, db: State<'_, Db>, id: String) -> Result<DeleteFlagOutcome, String> {
    let (outcome, scope) = delete(db.inner(), &id).await?;
    emit_changed(&app, scope.as_deref());
    Ok(outcome)
}

#[tauri::command]
pub async fn reorder_session_flags(
    app: AppHandle,
    db: State<'_, Db>,
    project_id: Option<String>,
    ids: Vec<String>,
) -> Result<(), String> {
    reorder(db.inner(), project_id.as_deref(), &ids).await?;
    emit_changed(&app, project_id.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn restore_default_session_flags(app: AppHandle, db: State<'_, Db>) -> Result<Vec<SessionFlag>, String> {
    let restored = restore_defaults(db.inner()).await?;
    if !restored.is_empty() {
        emit_changed(&app, None);
    }
    Ok(restored)
}

#[tauri::command]
pub async fn set_project_flag_hidden(
    app: AppHandle,
    db: State<'_, Db>,
    project_id: String,
    flag_id: String,
    hidden: bool,
) -> Result<(), String> {
    set_hidden(db.inner(), &project_id, &flag_id, hidden).await?;
    emit_changed(&app, Some(&project_id));
    Ok(())
}

/// No event: the window that tags a session updates its own run record, like a pin.
#[tauri::command]
pub async fn set_run_flag(db: State<'_, Db>, run_id: String, flag_id: Option<String>) -> Result<(), String> {
    tag_run(db.inner(), &run_id, flag_id.as_deref()).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub async fn test_db() -> Db {
        crate::commands::response_styles::tests::test_db().await
    }

    pub async fn add_run(db: &Db, id: &str, project: &str, flag: Option<&str>) {
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, created_at, session_id, flag_id) VALUES (?, ?, 'session', 'c', ?, ?)",
        )
        .bind(id)
        .bind(project)
        .bind(format!("s-{id}"))
        .bind(flag)
        .execute(db)
        .await
        .unwrap();
    }

    pub async fn run_flag(db: &Db, id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT flag_id FROM runs WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    fn ids(flags: &[SessionFlag]) -> Vec<&str> {
        flags.iter().map(|f| f.id.as_str()).collect()
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[tokio::test]
    async fn the_migrations_reset_to_the_defaults_as_in_code_and_leave_old_runs_unflagged() {
        // Migrate up to 0024 (through sqlx like the app does), add old flags and flagged
        // sessions, then the rest: 0025 drops them all and seeds the current defaults.
        let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for entry in std::fs::read_dir(&source).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.as_str() < "0025" {
                std::fs::copy(source.join(&name), dir.join(&name)).unwrap();
            }
        }
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&db).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO session_flags (id, project_id, name, color, position, created_at, updated_at) \
             VALUES ('mine', 'p1', 'Mine', 'info', 0, 'c', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO project_hidden_flags (project_id, flag_id) VALUES ('p1', 'needs_review')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, created_at, flag_id) VALUES \
             ('old', 'p1', 'session', 'c', 'settled'), ('own', 'p1', 'session', 'c', 'mine'), \
             ('none', 'p1', 'session', 'c', NULL)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        let flags = list(&db, None).await.unwrap();
        assert_eq!(flags.len(), DEFAULT_FLAGS.len());
        for ((row, def), position) in flags.iter().zip(DEFAULT_FLAGS).zip(0..) {
            assert_eq!(
                (row.id.as_str(), row.builtin_key.as_deref(), row.project_id.as_deref()),
                (def.key, Some(def.key), None)
            );
            assert_eq!((row.name.as_str(), row.color.as_str(), row.position), (def.name, def.color, position));
            assert!(row.stock_name && !row.hidden && row.run_count == 0, "{}", def.key);
            check_color(def.color).unwrap();
            check_name(def.name).unwrap();
        }
        assert_eq!(list(&db, Some("p1")).await.unwrap().len(), DEFAULT_FLAGS.len());
        for run in ["old", "own", "none"] {
            assert_eq!(run_flag(&db, run).await, None, "{run}");
        }
        let marks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_hidden_flags").fetch_one(&db).await.unwrap();
        assert_eq!(marks, 0);
    }

    #[tokio::test]
    async fn lists_shared_then_own_with_hidden_marks_and_scoped_counts() {
        let db = test_db().await;
        let own = create(&db, Some("p1"), "Waiting on client", "neutral").await.unwrap();
        create(&db, Some("p2"), "Other", "neutral").await.unwrap();
        set_hidden(&db, "p1", "done", true).await.unwrap();
        add_run(&db, "r1", "p1", Some("in_progress")).await;
        add_run(&db, "r2", "p2", Some("in_progress")).await;
        add_run(&db, "r3", "p1", Some(&own.id)).await;
        // In the Trash: not counted.
        add_run(&db, "r4", "p1", Some("in_progress")).await;
        sqlx::query("UPDATE runs SET deleted_at = 'x' WHERE id = 'r4'").execute(&db).await.unwrap();

        let shared = list(&db, None).await.unwrap();
        assert_eq!(ids(&shared), ["backlog", "todo", "in_progress", "in_review", "done"]);
        assert_eq!(shared[2].run_count, 2);
        assert!(shared.iter().all(|f| !f.hidden));

        let p1 = list(&db, Some("p1")).await.unwrap();
        assert_eq!(ids(&p1), ["backlog", "todo", "in_progress", "in_review", "done", own.id.as_str()]);
        assert_eq!(p1.iter().map(|f| f.hidden).collect::<Vec<_>>(), [false, false, false, false, true, false]);
        assert_eq!((p1[2].run_count, p1[5].run_count), (1, 1));
    }

    #[tokio::test]
    async fn create_validates_and_appends_to_its_scope() {
        let db = test_db().await;
        assert!(create(&db, None, "  ", "info").await.is_err());
        assert!(create(&db, None, &"n".repeat(NAME_MAX_CHARS + 1), "info").await.is_err());
        assert!(create(&db, None, "Blocked", "#ff0000").await.is_err());
        assert!(create(&db, None, "Blocked", "").await.is_err());
        assert!(create(&db, None, "in PROGRESS", "info").await.is_err());
        assert!(create(&db, Some("nope"), "Blocked", "info").await.is_err());
        assert_eq!(list(&db, None).await.unwrap().len(), 5);

        let max = "é".repeat(NAME_MAX_CHARS);
        let blocked = create(&db, None, &format!("  {max} "), "destructive").await.unwrap();
        assert_eq!((blocked.name.as_str(), blocked.position, blocked.builtin_key), (max.as_str(), 5, None));
        assert!(!blocked.stock_name);
        // A project may reuse a shared flag's name; its own scope starts at 0.
        let own = create(&db, Some("p1"), "In progress", "info").await.unwrap();
        assert_eq!((own.project_id.as_deref(), own.position), (Some("p1"), 0));
        assert!(create(&db, Some("p1"), "IN PROGRESS", "info").await.is_err());
    }

    #[tokio::test]
    async fn update_keeps_missing_fields_and_checks_like_create() {
        let db = test_db().await;
        let renamed = update(&db, "backlog", Some(" Look again "), None).await.unwrap();
        assert_eq!((renamed.name.as_str(), renamed.color.as_str(), renamed.stock_name), ("Look again", "neutral", false));
        let recoloured = update(&db, "done", None, Some("info")).await.unwrap();
        assert_eq!((recoloured.name.as_str(), recoloured.color.as_str(), recoloured.stock_name), ("Done", "info", true));
        assert!(update(&db, "done", Some("in progress"), None).await.is_err());
        assert!(update(&db, "done", None, Some("red")).await.is_err());
        assert!(update(&db, "done", Some(""), None).await.is_err());
        assert!(update(&db, "missing", Some("x"), None).await.is_err());
        // Its own name in another case is fine.
        assert_eq!(update(&db, "done", Some("DONE"), None).await.unwrap().name, "DONE");
    }

    // The colour list is shared with projects (projects.rs is past the file-size cap,
    // so the project side of the twelve-colour palette is checked here too).
    #[tokio::test]
    async fn the_twelve_palette_colours_save_on_flags_and_projects_and_odd_ones_do_not() {
        let db = test_db().await;
        assert_eq!(PROJECT_COLORS.len(), 12);
        for color in ["teal", "lime", "yellow", "orange", "pink", "violet"] {
            let flag = create(&db, None, &format!("Flag {color}"), color).await.unwrap();
            assert_eq!(flag.color, color);
            assert_eq!(update(&db, "done", None, Some(color)).await.unwrap().color, color);
            let project = crate::commands::projects::update_project_inner(&db, "p1", "a", None, None, Some(color.into()), None, None)
                .await
                .unwrap();
            assert_eq!(project.color.as_deref(), Some(color));
        }
        for odd in ["Teal", "cyan", "tag-teal", "#0f766e", "violet "] {
            assert!(create(&db, None, "Odd", odd).await.is_err(), "{odd}");
            assert!(update(&db, "done", None, Some(odd)).await.is_err(), "{odd}");
            let err = crate::commands::projects::update_project_inner(&db, "p1", "a", None, None, Some(odd.into()), None, None)
                .await
                .unwrap_err();
            assert!(err.starts_with("invalid project color"), "{err}");
        }
        assert_eq!(update(&db, "done", None, None).await.unwrap().color, "violet");
    }

    #[tokio::test]
    async fn delete_clears_its_sessions_and_hidden_marks() {
        let db = test_db().await;
        for i in 0..5 {
            add_run(&db, &format!("r{i}"), "p1", Some("in_progress")).await;
        }
        add_run(&db, "other", "p1", Some("done")).await;
        set_hidden(&db, "p1", "in_progress", true).await.unwrap();
        let (outcome, scope) = delete(&db, "in_progress").await.unwrap();
        assert_eq!((outcome, scope), (DeleteFlagOutcome { cleared_runs: 5 }, None));
        assert_eq!(run_flag(&db, "r0").await, None);
        assert_eq!(run_flag(&db, "other").await.as_deref(), Some("done"));
        let marks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_hidden_flags").fetch_one(&db).await.unwrap();
        assert_eq!(marks, 0);
        assert!(delete(&db, "in_progress").await.is_err());
    }

    #[tokio::test]
    async fn restore_puts_back_only_the_deleted_defaults_at_the_end() {
        let db = test_db().await;
        assert!(restore_defaults(&db).await.unwrap().is_empty());
        add_run(&db, "r1", "p1", Some("backlog")).await;
        delete(&db, "in_review").await.unwrap();
        update(&db, "backlog", Some("Look again"), Some("info")).await.unwrap();
        let mine = create(&db, None, "Blocked", "destructive").await.unwrap();

        let restored = restore_defaults(&db).await.unwrap();
        assert_eq!(ids(&restored), ["in_review"]);
        assert_eq!(
            (restored[0].name.as_str(), restored[0].color.as_str(), restored[0].builtin_key.as_deref()),
            ("In Review", "warning", Some("in_review"))
        );
        assert!(restored[0].position > mine.position);
        let shared = list(&db, None).await.unwrap();
        assert_eq!(shared.last().unwrap().id, "in_review");
        let edited = shared.iter().find(|f| f.id == "backlog").unwrap();
        assert_eq!((edited.name.as_str(), edited.color.as_str(), edited.run_count), ("Look again", "info", 1));
        assert!(restore_defaults(&db).await.unwrap().is_empty());

        // A shared flag of the user's took a default's name meanwhile.
        delete(&db, "done").await.unwrap();
        create(&db, None, "done", "info").await.unwrap();
        let back = restore_defaults(&db).await.unwrap();
        assert_eq!((back[0].id.as_str(), back[0].name.as_str(), back[0].stock_name), ("done", "Done (2)", false));
    }

    #[tokio::test]
    async fn reorder_needs_exactly_the_scopes_flags() {
        let db = test_db().await;
        let own = create(&db, Some("p1"), "Mine", "info").await.unwrap();
        reorder(&db, None, &strings(&["done", "backlog", "in_review", "todo", "in_progress"])).await.unwrap();
        assert_eq!(ids(&list(&db, None).await.unwrap()), ["done", "backlog", "in_review", "todo", "in_progress"]);
        // Missing, extra, duplicated or another scope's id: refused, nothing moves.
        assert!(reorder(&db, None, &strings(&["done", "backlog", "in_review", "todo"])).await.is_err());
        let extra = strings(&["done", "backlog", "in_review", "todo", "in_progress", &own.id]);
        assert!(reorder(&db, None, &extra).await.is_err());
        assert!(reorder(&db, None, &strings(&["done", "done", "in_review", "todo", "in_progress"])).await.is_err());
        assert!(reorder(&db, Some("p1"), &strings(&["done"])).await.is_err());
        reorder(&db, Some("p1"), &strings(&[&own.id])).await.unwrap();
        assert_eq!(ids(&list(&db, None).await.unwrap())[0], "done");
    }

    #[tokio::test]
    async fn a_session_gets_only_a_flag_its_project_offers() {
        let db = test_db().await;
        add_run(&db, "r", "p1", None).await;
        let own = create(&db, Some("p1"), "Mine", "info").await.unwrap();
        let theirs = create(&db, Some("p2"), "Theirs", "info").await.unwrap();
        tag_run(&db, "r", Some("in_progress")).await.unwrap();
        assert_eq!(run_flag(&db, "r").await.as_deref(), Some("in_progress"));
        tag_run(&db, "r", Some(&own.id)).await.unwrap();
        assert_eq!(run_flag(&db, "r").await, Some(own.id.clone()));
        // The History list carries it.
        let history = crate::commands::github::list_runs_inner(&db, "p1").await.unwrap();
        assert_eq!(history[0].flag_id, Some(own.id.clone()));
        tag_run(&db, "r", None).await.unwrap();
        assert_eq!(run_flag(&db, "r").await, None);

        set_hidden(&db, "p1", "done", true).await.unwrap();
        assert!(tag_run(&db, "r", Some("done")).await.is_err());
        assert!(tag_run(&db, "r", Some(&theirs.id)).await.is_err());
        assert!(tag_run(&db, "r", Some("gone")).await.is_err());
        assert!(tag_run(&db, "nope", None).await.is_err());
        assert_eq!(run_flag(&db, "r").await, None);
    }

    #[tokio::test]
    async fn hiding_keeps_the_flag_on_sessions_that_have_it() {
        let db = test_db().await;
        add_run(&db, "r", "p1", Some("in_progress")).await;
        set_hidden(&db, "p1", "in_progress", true).await.unwrap();
        set_hidden(&db, "p1", "in_progress", true).await.unwrap();
        assert_eq!(run_flag(&db, "r").await.as_deref(), Some("in_progress"));
        assert!(list(&db, Some("p1")).await.unwrap()[2].hidden);
        set_hidden(&db, "p1", "in_progress", false).await.unwrap();
        assert!(!list(&db, Some("p1")).await.unwrap()[2].hidden);

        let own = create(&db, Some("p1"), "Mine", "info").await.unwrap();
        assert!(set_hidden(&db, "p1", &own.id, true).await.is_err());
        assert!(set_hidden(&db, "p1", "gone", true).await.is_err());
        assert!(set_hidden(&db, "nope", "done", true).await.is_err());
    }

    #[tokio::test]
    async fn deleting_a_project_drops_its_flags_and_marks() {
        let db = test_db().await;
        let own = create(&db, Some("p1"), "Mine", "info").await.unwrap();
        set_hidden(&db, "p1", "done", true).await.unwrap();
        add_run(&db, "r1", "p1", Some(&own.id)).await;
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM session_flags WHERE project_id IS NOT NULL) \
                  + (SELECT COUNT(*) FROM project_hidden_flags) \
                  + (SELECT COUNT(*) FROM runs WHERE flag_id IS NOT NULL AND flag_id NOT IN (SELECT id FROM session_flags))",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(left, 0);
        assert_eq!(list(&db, None).await.unwrap().len(), 5);
    }

    #[test]
    fn tagging_and_reorder_rules() {
        assert!(check_taggable("p1", None, false).is_ok());
        assert!(check_taggable("p1", None, true).is_err());
        assert!(check_taggable("p1", Some("p1"), false).is_ok());
        assert!(check_taggable("p1", Some("p2"), false).is_err());
        assert!(check_reorder(&strings(&["a", "b"]), &strings(&["b", "a"])).is_ok());
        assert!(check_reorder(&strings(&["a", "b"]), &strings(&["a", "a"])).is_err());
        assert!(check_reorder(&[], &[]).is_ok());
    }
}
