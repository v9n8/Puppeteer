//! Session flags in a settings backup (V3-18): the flags, the shared flags each
//! project hides, and which session has which flag.
//!
//! A flag is matched by id (the defaults' id is their key), else by name in its
//! scope, so a machine that made its own "Blocked" does not get a second one.
//! A project travels as a [`ProjectRef`] and is found here by path, then name;
//! a session by run id, then engine session id, like a bookmark. What has no
//! counterpart on this machine is skipped with a line in the import report.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::commands::session_flags::{check_color, check_name};
use crate::commands::settings_backup::{Counts, ImportReport, ProjectRef};
use crate::db::Db;

/// The report section the flags are counted under (`settings.backup.sections.sessionFlags`).
pub const SECTION: &str = "sessionFlags";

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SessionFlagsBackup {
    #[serde(default)]
    pub flags: Vec<FlagEntry>,
    #[serde(default)]
    pub hidden: Vec<HiddenEntry>,
    #[serde(default)]
    pub runs: Vec<RunFlagEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FlagEntry {
    pub id: String,
    /// `None`: a shared flag.
    #[serde(default)]
    pub project: Option<ProjectRef>,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub builtin_key: Option<String>,
}

/// A shared flag a project hides.
#[derive(Clone, Serialize, Deserialize)]
pub struct HiddenEntry {
    pub project: ProjectRef,
    pub flag_id: String,
}

/// A session's flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunFlagEntry {
    pub run_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub flag_id: String,
}

impl SessionFlagsBackup {
    /// The project references, for moving their paths under this machine's `$HOME`.
    pub fn project_refs_mut(&mut self) -> impl Iterator<Item = &mut ProjectRef> {
        self.flags
            .iter_mut()
            .filter_map(|f| f.project.as_mut())
            .chain(self.hidden.iter_mut().map(|h| &mut h.project))
    }
}

fn err(e: sqlx::Error) -> String {
    e.to_string()
}

fn project_ref(row: &sqlx::sqlite::SqliteRow) -> Option<ProjectRef> {
    let name: Option<String> = row.get("project_name");
    let path: Option<String> = row.get("project_path");
    Some(ProjectRef { name: name?, path: path? })
}

/// Everything flag-related on this machine; what belongs to a project or a
/// session in the Trash stays local.
pub async fn collect(db: &Db) -> Result<SessionFlagsBackup, String> {
    let flags = sqlx::query(
        "SELECT f.id, f.name, f.color, f.position, f.builtin_key, p.name AS project_name, p.path AS project_path \
         FROM session_flags f LEFT JOIN projects p ON p.id = f.project_id \
         WHERE f.project_id IS NULL OR p.deleted_at IS NULL \
         ORDER BY f.project_id IS NOT NULL, f.project_id, f.position",
    )
    .fetch_all(db)
    .await
    .map_err(err)?
    .iter()
    .map(|row| FlagEntry {
        id: row.get("id"),
        project: project_ref(row),
        name: row.get("name"),
        color: row.get("color"),
        position: row.get("position"),
        builtin_key: row.get("builtin_key"),
    })
    .collect();
    let hidden = sqlx::query(
        "SELECT h.flag_id, p.name AS project_name, p.path AS project_path \
         FROM project_hidden_flags h JOIN projects p ON p.id = h.project_id \
         WHERE p.deleted_at IS NULL ORDER BY p.path, h.flag_id",
    )
    .fetch_all(db)
    .await
    .map_err(err)?
    .iter()
    .filter_map(|row| Some(HiddenEntry { project: project_ref(row)?, flag_id: row.get("flag_id") }))
    .collect();
    let runs = sqlx::query(
        "SELECT r.id, r.session_id, r.flag_id FROM runs r JOIN projects p ON p.id = r.project_id \
         WHERE r.flag_id IS NOT NULL AND r.deleted_at IS NULL AND p.deleted_at IS NULL ORDER BY r.id",
    )
    .fetch_all(db)
    .await
    .map_err(err)?
    .iter()
    .map(|row| RunFlagEntry { run_id: row.get("id"), session_id: row.get("session_id"), flag_id: row.get("flag_id") })
    .collect();
    Ok(SessionFlagsBackup { flags, hidden, runs })
}

/// The local project an exported reference names: same path first, then same name.
async fn find_project(db: &Db, project: &ProjectRef) -> Result<Option<String>, String> {
    for (column, value) in [("path", &project.path), ("name", &project.name)] {
        let id: Option<String> = sqlx::query_scalar(&format!("SELECT id FROM projects WHERE {column} = ? LIMIT 1"))
            .bind(value)
            .fetch_optional(db)
            .await
            .map_err(err)?;
        if id.is_some() {
            return Ok(id);
        }
    }
    Ok(None)
}

/// The local run a session entry names: same run id, else same engine session.
async fn find_run(db: &Db, entry: &RunFlagEntry) -> Result<Option<(String, String)>, String> {
    let by = |column: &str| format!("SELECT id, project_id FROM runs WHERE {column} = ? LIMIT 1");
    let found: Option<(String, String)> =
        sqlx::query_as(&by("id")).bind(&entry.run_id).fetch_optional(db).await.map_err(err)?;
    if found.is_some() {
        return Ok(found);
    }
    let Some(sid) = &entry.session_id else { return Ok(None) };
    sqlx::query_as(&by("session_id")).bind(sid).fetch_optional(db).await.map_err(err)
}

/// Created, updated or unchanged, like the other sections' upserts.
enum Outcome {
    Created,
    Updated,
    Unchanged,
}

fn tally(report: &mut ImportReport, outcome: Outcome) {
    let c: &mut Counts = report.counts.entry(SECTION.to_string()).or_default();
    match outcome {
        Outcome::Created => c.created += 1,
        Outcome::Updated => c.updated += 1,
        Outcome::Unchanged => c.unchanged += 1,
    }
}

/// Upsert one flag into scope `scope`; returns its local id.
async fn import_flag(db: &Db, scope: Option<&str>, entry: &FlagEntry) -> Result<(String, Outcome), String> {
    let name = check_name(&entry.name)?;
    check_color(&entry.color)?;
    let same_scope = "IFNULL(project_id, '') = IFNULL(?, '')";
    let by_id: Option<(String, String, String, i64)> =
        sqlx::query_as(&format!("SELECT id, name, color, position FROM session_flags WHERE id = ? AND {same_scope}"))
            .bind(&entry.id)
            .bind(scope)
            .fetch_optional(db)
            .await
            .map_err(err)?;
    let found = match by_id {
        Some(row) => Some(row),
        None => sqlx::query_as(&format!(
            "SELECT id, name, color, position FROM session_flags WHERE {same_scope} AND name = ? COLLATE NOCASE"
        ))
        .bind(scope)
        .bind(&name)
        .fetch_optional(db)
        .await
        .map_err(err)?,
    };
    let now = chrono::Utc::now().to_rfc3339();
    if let Some((id, local_name, color, position)) = found {
        if (local_name.as_str(), color.as_str(), position) == (name.as_str(), entry.color.as_str(), entry.position) {
            return Ok((id, Outcome::Unchanged));
        }
        let taken: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM session_flags WHERE {same_scope} AND name = ? COLLATE NOCASE AND id <> ?"
        ))
        .bind(scope)
        .bind(&name)
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(err)?;
        if taken > 0 {
            return Err(format!("another flag here is already named '{name}'"));
        }
        sqlx::query("UPDATE session_flags SET name = ?, color = ?, position = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(&entry.color)
            .bind(entry.position)
            .bind(&now)
            .bind(&id)
            .execute(db)
            .await
            .map_err(err)?;
        return Ok((id, Outcome::Updated));
    }
    // New here: keep its id unless this machine uses it already (in another scope).
    let id_free: i64 = sqlx::query_scalar("SELECT COUNT(*) = 0 FROM session_flags WHERE id = ?")
        .bind(&entry.id)
        .fetch_one(db)
        .await
        .map_err(err)?;
    let id = if id_free == 1 { entry.id.clone() } else { uuid::Uuid::new_v4().to_string() };
    // A default comes back as that default only once, and only as a shared flag.
    let builtin_key: Option<String> = match (&entry.builtin_key, scope) {
        (Some(key), None) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_flags WHERE builtin_key = ?")
            .bind(key)
            .fetch_one(db)
            .await
            .map_err(err)?
            .eq(&0)
            .then(|| key.clone()),
        _ => None,
    };
    sqlx::query(
        "INSERT INTO session_flags (id, project_id, name, color, position, builtin_key, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(scope)
    .bind(&name)
    .bind(&entry.color)
    .bind(entry.position)
    .bind(builtin_key)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .map_err(err)?;
    Ok((id, Outcome::Created))
}

/// Bring the flags, hidden marks and session flags of `backup` onto this machine.
/// Run after the projects are imported, so a project's own flags find it.
pub async fn import(db: &Db, backup: &SessionFlagsBackup, report: &mut ImportReport) {
    // Exported flag id → (local id, local scope).
    let mut local: HashMap<&str, (String, Option<String>)> = HashMap::new();
    let mut no_project = 0;
    for entry in &backup.flags {
        let scope = match &entry.project {
            None => None,
            Some(project) => match find_project(db, project).await {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    no_project += 1;
                    continue;
                }
                Err(e) => {
                    report.warnings.push(format!("{SECTION} '{}': {e}", entry.name));
                    continue;
                }
            },
        };
        match import_flag(db, scope.as_deref(), entry).await {
            Ok((id, outcome)) => {
                tally(report, outcome);
                local.insert(entry.id.as_str(), (id, scope));
            }
            Err(e) => report.warnings.push(format!("{SECTION} '{}': {e}", entry.name)),
        }
    }
    if no_project > 0 {
        report
            .warnings
            .push(format!("{SECTION}: {no_project} project flag(s) skipped, their project is not on this machine"));
    }

    let mut skipped_hidden = 0;
    for entry in &backup.hidden {
        let flag = local.get(entry.flag_id.as_str()).filter(|(_, scope)| scope.is_none());
        let project = find_project(db, &entry.project).await.ok().flatten();
        let (Some((flag_id, _)), Some(project_id)) = (flag, project) else {
            skipped_hidden += 1;
            continue;
        };
        if let Err(e) = sqlx::query("INSERT OR IGNORE INTO project_hidden_flags (project_id, flag_id) VALUES (?, ?)")
            .bind(&project_id)
            .bind(flag_id)
            .execute(db)
            .await
        {
            report.warnings.push(format!("{SECTION}: hidden flag in '{}': {e}", entry.project.name));
        }
    }
    if skipped_hidden > 0 {
        report.warnings.push(format!(
            "{SECTION}: {skipped_hidden} hidden flag(s) skipped, their project or flag is not on this machine"
        ));
    }

    let mut skipped_runs = 0;
    for entry in &backup.runs {
        let Some((flag_id, scope)) = local.get(entry.flag_id.as_str()) else {
            skipped_runs += 1;
            continue;
        };
        let run = match find_run(db, entry).await {
            Ok(Some(run)) => run,
            Ok(None) => {
                skipped_runs += 1;
                continue;
            }
            Err(e) => {
                report.warnings.push(format!("{SECTION}: session {}: {e}", entry.run_id));
                continue;
            }
        };
        // Another project's own flag never lands on a session (a session keeps a
        // shared flag its project hides, as on the exporting machine).
        if scope.as_deref().is_some_and(|p| p != run.1) {
            skipped_runs += 1;
            continue;
        }
        if let Err(e) = sqlx::query("UPDATE runs SET flag_id = ? WHERE id = ?").bind(flag_id).bind(&run.0).execute(db).await {
            report.warnings.push(format!("{SECTION}: session {}: {e}", entry.run_id));
        }
    }
    if skipped_runs > 0 {
        report.warnings.push(format!(
            "{SECTION}: {skipped_runs} session flag(s) skipped, their session or flag is not on this machine"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session_flags::tests::{add_run, run_flag, test_db};
    use crate::commands::session_flags::{create, delete, list, set_hidden, tag_run, update};

    #[tokio::test]
    async fn flags_hidden_marks_and_session_flags_round_trip() {
        // Machine A: a renamed default, a new shared flag, a project flag, a hidden
        // flag, flagged sessions (one in a project B does not have).
        let a = test_db().await;
        update(&a, "backlog", Some("Look again"), None).await.unwrap();
        let blocked = create(&a, None, "Blocked", "destructive").await.unwrap();
        let client = create(&a, Some("p1"), "Waiting on client", "neutral").await.unwrap();
        create(&a, Some("p2"), "Only on A", "info").await.unwrap();
        set_hidden(&a, "p1", "done", true).await.unwrap();
        add_run(&a, "r1", "p1", None).await;
        add_run(&a, "r2", "p1", None).await;
        add_run(&a, "r3", "p2", None).await;
        add_run(&a, "gone", "p1", None).await;
        tag_run(&a, "r1", Some(&client.id)).await.unwrap();
        tag_run(&a, "r2", Some(&blocked.id)).await.unwrap();
        tag_run(&a, "gone", Some("in_progress")).await.unwrap();
        let mut backup = collect(&a).await.unwrap();
        assert_eq!((backup.flags.len(), backup.hidden.len(), backup.runs.len()), (8, 1, 3));
        // Through JSON, as the file carries it.
        backup = serde_json::from_str(&serde_json::to_string(&backup).unwrap()).unwrap();
        assert_eq!(backup.project_refs_mut().count(), 3);

        // Machine B: p1 only (same name + path), r1 under another id but the same
        // engine session, r2 with the same id, no "gone"; its own "blocked" already.
        let b = test_db().await;
        sqlx::query("DELETE FROM projects WHERE id = 'p2'").execute(&b).await.unwrap();
        sqlx::query("UPDATE projects SET id = 'b1' WHERE id = 'p1'").execute(&b).await.unwrap();
        let local_blocked = create(&b, None, "blocked", "info").await.unwrap();
        delete(&b, "done").await.unwrap();
        add_run(&b, "x1", "b1", None).await;
        sqlx::query("UPDATE runs SET session_id = 's-r1' WHERE id = 'x1'").execute(&b).await.unwrap();
        add_run(&b, "r2", "b1", None).await;

        let mut report = ImportReport::default();
        import(&b, &backup, &mut report).await;

        let shared = list(&b, Some("b1")).await.unwrap();
        let shown = shared.iter().map(|f| (f.name.as_str(), f.color.as_str(), f.hidden)).collect::<Vec<_>>();
        assert_eq!(shown, [
            ("Look again", "neutral", false),
            ("To Do", "primary", false),
            ("In Progress", "info", false),
            ("In Review", "warning", false),
            ("Done", "success", true),
            ("Blocked", "destructive", false),
            ("Waiting on client", "neutral", false),
        ]);
        // Matched by name, not added twice; the deleted default came back as itself.
        assert_eq!(shared[5].id, local_blocked.id);
        assert_eq!(shared[4].builtin_key.as_deref(), Some("done"));
        assert_eq!(run_flag(&b, "x1").await, Some(shared[6].id.clone()));
        assert_eq!(run_flag(&b, "r2").await, Some(local_blocked.id.clone()));
        let c = &report.counts[SECTION];
        assert_eq!((c.created, c.updated, c.unchanged), (2, 2, 3), "{:?}", report.warnings);
        assert!(report.warnings.iter().any(|w| w.contains("1 project flag(s) skipped")), "{:?}", report.warnings);
        assert!(report.warnings.iter().any(|w| w.contains("1 session flag(s) skipped")), "{:?}", report.warnings);

        // A second import changes nothing.
        let mut again = ImportReport::default();
        import(&b, &backup, &mut again).await;
        let c = &again.counts[SECTION];
        assert_eq!((c.created, c.updated, c.unchanged), (0, 0, 7));
        assert_eq!(list(&b, Some("b1")).await.unwrap().len(), 7);
    }

    #[tokio::test]
    async fn a_trashed_project_keeps_its_flags_local_and_bad_entries_are_reported() {
        let a = test_db().await;
        create(&a, Some("p2"), "Trashed", "info").await.unwrap();
        add_run(&a, "r", "p2", Some("in_progress")).await;
        sqlx::query("UPDATE projects SET deleted_at = 'x' WHERE id = 'p2'").execute(&a).await.unwrap();
        let backup = collect(&a).await.unwrap();
        assert_eq!((backup.flags.len(), backup.runs.len()), (5, 0));

        let b = test_db().await;
        let bad = SessionFlagsBackup {
            flags: vec![FlagEntry {
                id: "x".into(),
                project: None,
                name: "Pink".into(),
                color: "pink".into(),
                position: 9,
                builtin_key: None,
            }],
            ..Default::default()
        };
        let mut report = ImportReport::default();
        import(&b, &bad, &mut report).await;
        assert!(report.warnings.iter().any(|w| w.contains("'Pink'")), "{:?}", report.warnings);
        assert_eq!(list(&b, None).await.unwrap().len(), 5);
    }
}
