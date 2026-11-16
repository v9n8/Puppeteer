//! Sync conflicts of the old library, found on disk.
//!
//! The old library puts an item into a project as a link back to its central copy in app data
//! (skills, Claude rules, a rule's Codex skill form) or as a block in `AGENTS.md` (a rule's
//! inline Codex form). A link can't drift: editing it edits the central copy. So a conflict is
//! only (a) a real file or folder, or a link elsewhere, where Puppeteer's link should be, whose
//! content differs from the central copy, or (b) an `AGENTS.md` block edited by hand. The new
//! Library (`library_disk`) reads the disk directly and syncs nothing, so it never conflicts.
//!
//! Three versions are compared: the base (`synced_hash_*`, what Puppeteer installed), the
//! project's and the central one ([`classify`]). A scan only reads the disk and only writes the
//! two conflict tables: it opens a row per conflict, closes the rows that no longer conflict or
//! whose item is no longer applied, and folds duplicate open rows into the newest. "Keep local"
//! is remembered by the row it closed: a closed row holding the current pair of hashes is not
//! reopened; once either side changes, the pair differs and the conflict comes back.

use crate::commands::library_disk::{mtime_ms, read_text};
use crate::commands::project_links::{at_dest, parent_is_real, AtDest};
use crate::commands::rules::{
    agents_path, applies_claude, applies_codex, claude_rule_path, codex_skill_dest, codex_skill_source,
    content_hash, extract_agents_block, parse_codex_mode, strip_frontmatter, CodexMode,
};
use crate::commands::skills::skill_engine;
use crate::db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tauri::State;

mod resolve;
// Symlinks throughout, like the other link tests (rules.rs).
#[cfg(all(test, unix))]
mod tests;

pub(crate) use resolve::{detach_materialized, resolve_rule, resolve_skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Skill,
    Rule,
}

impl ItemKind {
    fn table(self) -> &'static str {
        match self {
            ItemKind::Skill => "sync_conflicts",
            ItemKind::Rule => "rule_sync_conflicts",
        }
    }

    fn item_column(self) -> &'static str {
        match self {
            ItemKind::Skill => "skill_id",
            ItemKind::Rule => "rule_id",
        }
    }
}

/// How one (item, engine) of a project compares, see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Puppeteer's own link, files git tracks, nothing, or a folder reached through a symlinked
    /// parent (not Puppeteer's to manage): nothing that can drift.
    NotACopy,
    /// The project's version is the central one.
    Matches,
    /// The project's version is what Puppeteer installed: only the central one moved on.
    Unedited,
    /// The user kept the project's version of this exact pair before.
    Kept,
    Conflict,
}

/// `project`: the project's version's hash (None: [`Verdict::NotACopy`]); `base`: the hash
/// recorded at install; `kept`: (local, source) pairs of the closed rows of this item.
pub fn classify(project: Option<&str>, central: &str, base: Option<&str>, kept: &[(String, String)]) -> Verdict {
    let Some(project) = project else { return Verdict::NotACopy };
    if project == central {
        Verdict::Matches
    } else if Some(project) == base {
        Verdict::Unedited
    } else if kept.iter().any(|(l, s)| l == project && s == central) {
        Verdict::Kept
    } else {
        Verdict::Conflict
    }
}

/// Where one (item, engine) lives in a project and in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    /// Where Puppeteer's link goes (a skill folder, a rule file); None for an `AGENTS.md` block.
    pub link: Option<PathBuf>,
    /// What Puppeteer's own link at `link` points to.
    pub link_target: PathBuf,
    /// The project's file, read through whatever sits at `link`.
    pub project_file: PathBuf,
    /// The `AGENTS.md` block holding the project's version.
    pub block: Option<String>,
    pub central_file: PathBuf,
    /// Compare bodies without frontmatter: a rule's Codex forms, which Codex gets without it.
    pub body_only: bool,
}

/// `source_path` is the item's library path (`skills.source_path` / `rules.source_path`);
/// `codex_mode` only matters for a rule's Codex engine.
pub(crate) fn layout(
    kind: ItemKind,
    engine: &str,
    project_path: &str,
    name: &str,
    source_path: &str,
    codex_mode: CodexMode,
) -> Result<Layout, String> {
    let source = PathBuf::from(source_path);
    Ok(match (kind, engine) {
        (ItemKind::Skill, _) => {
            let link = Path::new(project_path).join(skill_engine(engine)?.project_dir).join(name);
            Layout {
                project_file: link.join("SKILL.md"),
                link: Some(link),
                central_file: source.join("SKILL.md"),
                link_target: source,
                block: None,
                body_only: false,
            }
        }
        (ItemKind::Rule, "claude") => {
            let link = claude_rule_path(project_path, name);
            Layout {
                project_file: link.clone(),
                link: Some(link),
                link_target: source.clone(),
                block: None,
                central_file: source,
                body_only: false,
            }
        }
        (ItemKind::Rule, "codex") if codex_mode == CodexMode::Skill => {
            let link = codex_skill_dest(project_path, name);
            Layout {
                project_file: link.join("SKILL.md"),
                link: Some(link),
                link_target: codex_skill_source(source_path, name),
                block: None,
                // The generated SKILL.md's body is the rule's body; the rule is the one edited.
                central_file: source,
                body_only: true,
            }
        }
        (ItemKind::Rule, "codex") => Layout {
            link: None,
            link_target: source.clone(),
            project_file: agents_path(project_path),
            block: Some(name.to_string()),
            central_file: source,
            body_only: true,
        },
        (ItemKind::Rule, other) => return Err(format!("Unknown engine: {}", other)),
    })
}

/// The hash a version is compared by: what `synced_hash_*` records for that engine.
pub(crate) fn version_hash(text: &str, body_only: bool) -> String {
    if body_only {
        content_hash(strip_frontmatter(text).trim_end_matches('\n'))
    } else {
        content_hash(text)
    }
}

/// An `AGENTS.md` block is stored without frontmatter, so it hashes as is (`sync_rule_to_projects`).
fn block_hash(block: &str) -> String {
    content_hash(block.trim_end_matches('\n'))
}

fn read_err(path: &Path, e: std::io::Error) -> String {
    format!("{}: {}", path.display(), e)
}

/// The rule's Codex form, from its source (inline when the source can't be read: the read error
/// is reported where the central version is read).
fn codex_mode_of(kind: ItemKind, engine: &str, source_path: &str) -> CodexMode {
    if kind == ItemKind::Rule && engine == "codex" {
        fs::read_to_string(source_path).map(|c| parse_codex_mode(&c)).unwrap_or(CodexMode::Inline)
    } else {
        CodexMode::Inline
    }
}

/// The project's version's hash, None when nothing there can drift. Reads only.
pub(crate) fn project_hash(root: &Path, l: &Layout) -> Result<Option<String>, String> {
    if let Some(block) = &l.block {
        match fs::read_to_string(&l.project_file) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(read_err(&l.project_file, e)),
            Ok(_) => {}
        }
        return Ok(extract_agents_block(&l.project_file, block).map(|b| block_hash(&b)));
    }
    let Some(link) = &l.link else { return Ok(None) };
    if !parent_is_real(root, link)? {
        return Ok(None);
    }
    if at_dest(root, link, &l.link_target) != AtDest::Other {
        return Ok(None);
    }
    let text = fs::read_to_string(&l.project_file).map_err(|e| read_err(&l.project_file, e))?;
    Ok(Some(version_hash(&text, l.body_only)))
}

pub(crate) fn central_hash(l: &Layout) -> Result<String, String> {
    let text = fs::read_to_string(&l.central_file).map_err(|e| read_err(&l.central_file, e))?;
    Ok(version_hash(&text, l.body_only))
}

// ---- Scan ------------------------------------------------------------------------------------

/// An (item, engine) of a project, as a scan reports it.
#[derive(Debug, Clone, Serialize)]
pub struct ScanEntry {
    pub project_id: String,
    pub kind: ItemKind,
    pub item_id: String,
    pub name: String,
    pub engine: String,
    /// The open row, for a conflict.
    pub conflict_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// The project's version matches again (or is unedited, or was kept).
    InSync,
    /// The item is no longer applied to the project for that engine.
    NotApplied,
    /// An older open row of the same item and engine.
    Duplicate,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClosedRow {
    pub conflict_id: String,
    pub project_id: String,
    pub kind: ItemKind,
    pub item_id: String,
    pub name: String,
    pub engine: String,
    pub reason: CloseReason,
}

/// An item that couldn't be read: its rows are left as they were.
#[derive(Debug, Clone, Serialize)]
pub struct ScanError {
    pub project_id: String,
    pub kind: Option<ItemKind>,
    pub name: Option<String>,
    pub engine: Option<String>,
    pub error: String,
}

/// What a scan saw and did, nothing else.
#[derive(Debug, Default, Serialize)]
pub struct ScanReport {
    pub projects: usize,
    /// (item, engine) pairs compared.
    pub checked: usize,
    /// Conflicts open after the scan.
    pub open: Vec<ScanEntry>,
    /// Rows this scan opened (a subset of `open`).
    pub opened: usize,
    pub closed: Vec<ClosedRow>,
    pub errors: Vec<ScanError>,
}

struct Applied {
    kind: ItemKind,
    item_id: String,
    name: String,
    source_path: String,
    engine: &'static str,
    base: Option<String>,
}

async fn applied_items(db: &Db, project_id: &str) -> Result<Vec<Applied>, String> {
    let mut out = Vec::new();
    let skills = sqlx::query(
        "SELECT ps.skill_id, s.name, s.source_path, ps.enabled_claude, ps.enabled_codex,
                ps.synced_hash_claude, ps.synced_hash_codex
         FROM project_skills ps JOIN skills s ON s.id = ps.skill_id
         WHERE ps.project_id = ? ORDER BY s.name",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    for row in &skills {
        for (engine, on) in [("claude", row.get::<i64, _>("enabled_claude") != 0), ("codex", row.get::<i64, _>("enabled_codex") != 0)] {
            if on {
                out.push(Applied {
                    kind: ItemKind::Skill,
                    item_id: row.get("skill_id"),
                    name: row.get("name"),
                    source_path: row.get("source_path"),
                    engine,
                    base: row.get(if engine == "claude" { "synced_hash_claude" } else { "synced_hash_codex" }),
                });
            }
        }
    }
    let rules = sqlx::query(
        "SELECT pr.rule_id, r.name, r.source_path, pr.target, pr.synced_hash_claude, pr.synced_hash_codex
         FROM project_rules pr JOIN rules r ON r.id = pr.rule_id
         WHERE pr.project_id = ? ORDER BY r.name",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    for row in &rules {
        let target: String = row.get("target");
        for (engine, on) in [("claude", applies_claude(&target)), ("codex", applies_codex(&target))] {
            if on {
                out.push(Applied {
                    kind: ItemKind::Rule,
                    item_id: row.get("rule_id"),
                    name: row.get("name"),
                    source_path: row.get("source_path"),
                    engine,
                    base: row.get(if engine == "claude" { "synced_hash_claude" } else { "synced_hash_codex" }),
                });
            }
        }
    }
    Ok(out)
}

struct ConflictRow {
    id: String,
    local_hash: String,
    source_hash: String,
    resolved: bool,
}

/// Every row of one (project, item, engine), newest first.
async fn rows_of(db: &Db, kind: ItemKind, project_id: &str, item_id: &str, engine: &str) -> Result<Vec<ConflictRow>, String> {
    let sql = format!(
        "SELECT id, local_hash, source_hash, resolved FROM {} WHERE project_id = ? AND {} = ? AND engine = ?
         ORDER BY detected_at DESC, rowid DESC",
        kind.table(),
        kind.item_column()
    );
    let rows = sqlx::query(&sql).bind(project_id).bind(item_id).bind(engine).fetch_all(db).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| ConflictRow {
            id: r.get("id"),
            local_hash: r.get("local_hash"),
            source_hash: r.get("source_hash"),
            resolved: r.get::<i64, _>("resolved") != 0,
        })
        .collect())
}

async fn close_row(db: &Db, kind: ItemKind, id: &str) -> Result<(), String> {
    sqlx::query(&format!("UPDATE {} SET resolved = 1 WHERE id = ?", kind.table()))
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Scan one project, or every live one. Projects in the Trash are skipped.
pub async fn scan(db: &Db, project_id: Option<&str>) -> Result<ScanReport, String> {
    let projects: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, path FROM projects WHERE deleted_at IS NULL AND (? IS NULL OR id = ?) ORDER BY id",
    )
    .bind(project_id)
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let mut report = ScanReport { projects: projects.len(), ..ScanReport::default() };
    for (pid, path) in projects {
        scan_project(db, &pid, &path, &mut report).await?;
    }
    Ok(report)
}

async fn scan_project(db: &Db, project_id: &str, project_path: &str, report: &mut ScanReport) -> Result<(), String> {
    let root = Path::new(project_path);
    if !root.is_dir() {
        // Nothing can be compared: its rows stay as they are.
        report.errors.push(ScanError {
            project_id: project_id.to_string(),
            kind: None,
            name: None,
            engine: None,
            error: format!("Project folder not found: {}", project_path),
        });
        return Ok(());
    }
    let mut seen: HashSet<(ItemKind, String, String)> = HashSet::new();
    for item in applied_items(db, project_id).await? {
        seen.insert((item.kind, item.item_id.clone(), item.engine.to_string()));
        report.checked += 1;
        let mode = codex_mode_of(item.kind, item.engine, &item.source_path);
        let sides = layout(item.kind, item.engine, project_path, &item.name, &item.source_path, mode)
            .and_then(|l| Ok((project_hash(root, &l)?, central_hash(&l)?)));
        let (local, central) = match sides {
            Ok(v) => v,
            Err(error) => {
                report.errors.push(ScanError {
                    project_id: project_id.to_string(),
                    kind: Some(item.kind),
                    name: Some(item.name.clone()),
                    engine: Some(item.engine.to_string()),
                    error,
                });
                continue;
            }
        };
        let rows = rows_of(db, item.kind, project_id, &item.item_id, item.engine).await?;
        let kept: Vec<(String, String)> =
            rows.iter().filter(|r| r.resolved).map(|r| (r.local_hash.clone(), r.source_hash.clone())).collect();
        let verdict = classify(local.as_deref(), &central, item.base.as_deref(), &kept);
        let mut open = rows.iter().filter(|r| !r.resolved);
        let closed = |id: &str, reason| ClosedRow {
            conflict_id: id.to_string(),
            project_id: project_id.to_string(),
            kind: item.kind,
            item_id: item.item_id.clone(),
            name: item.name.clone(),
            engine: item.engine.to_string(),
            reason,
        };
        let mut keep_id = None;
        if verdict == Verdict::Conflict {
            let local = local.unwrap_or_default();
            if let Some(newest) = open.next() {
                // The current pair, so a "Keep local" on this row remembers what the user saw.
                sqlx::query(&format!("UPDATE {} SET local_hash = ?, source_hash = ? WHERE id = ?", item.kind.table()))
                    .bind(&local)
                    .bind(&central)
                    .bind(&newest.id)
                    .execute(db)
                    .await
                    .map_err(|e| e.to_string())?;
                keep_id = Some(newest.id.clone());
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(&format!(
                    "INSERT INTO {} (id, project_id, {}, engine, detected_at, local_hash, source_hash, resolved)
                     VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
                    item.kind.table(),
                    item.kind.item_column()
                ))
                .bind(&id)
                .bind(project_id)
                .bind(&item.item_id)
                .bind(item.engine)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&local)
                .bind(&central)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
                report.opened += 1;
                keep_id = Some(id);
            }
        }
        let reason = if keep_id.is_some() { CloseReason::Duplicate } else { CloseReason::InSync };
        for row in open {
            close_row(db, item.kind, &row.id).await?;
            report.closed.push(closed(&row.id, reason));
        }
        if let Some(id) = keep_id {
            report.open.push(ScanEntry {
                project_id: project_id.to_string(),
                kind: item.kind,
                item_id: item.item_id.clone(),
                name: item.name.clone(),
                engine: item.engine.to_string(),
                conflict_id: Some(id),
            });
        }
    }

    // Open rows of items no longer applied (or applied for other engines only).
    for kind in [ItemKind::Skill, ItemKind::Rule] {
        let (names, col) = match kind {
            ItemKind::Skill => ("skills", "skill_id"),
            ItemKind::Rule => ("rules", "rule_id"),
        };
        let sql = format!(
            "SELECT c.id, c.{col} AS item_id, c.engine, COALESCE(i.name, '') AS name
             FROM {} c LEFT JOIN {names} i ON i.id = c.{col}
             WHERE c.project_id = ? AND c.resolved = 0",
            kind.table()
        );
        let rows = sqlx::query(&sql).bind(project_id).fetch_all(db).await.map_err(|e| e.to_string())?;
        for row in rows {
            let (id, item_id, engine): (String, String, String) = (row.get("id"), row.get("item_id"), row.get("engine"));
            if seen.contains(&(kind, item_id.clone(), engine.clone())) {
                continue;
            }
            close_row(db, kind, &id).await?;
            report.closed.push(ClosedRow {
                conflict_id: id,
                project_id: project_id.to_string(),
                kind,
                item_id,
                name: row.get("name"),
                engine,
                reason: CloseReason::NotApplied,
            });
        }
    }
    Ok(())
}

// ---- List and read ---------------------------------------------------------------------------

/// An open conflict with both sides located, as `list_project_sync_conflicts` returns it.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSyncConflict {
    pub id: String,
    pub kind: ItemKind,
    pub project_id: String,
    pub item_id: String,
    pub name: String,
    pub engine: String,
    /// The project's file (`AGENTS.md` for a block).
    pub project_path: String,
    /// The `AGENTS.md` block holding the project's version.
    pub block: Option<String>,
    pub central_path: String,
    /// Milliseconds since the epoch, read now; None when it can't be read.
    pub project_mtime: Option<i64>,
    pub central_mtime: Option<i64>,
    pub detected_at: String,
    pub local_hash: String,
    pub source_hash: String,
}

const ROW_SQL: &str = "SELECT 'skill' AS kind, c.id, c.project_id, c.skill_id AS item_id, i.name, i.source_path,
        c.engine, c.detected_at, c.local_hash, c.source_hash, p.path AS project_path
     FROM sync_conflicts c JOIN skills i ON i.id = c.skill_id JOIN projects p ON p.id = c.project_id
     WHERE {skill}
     UNION ALL
     SELECT 'rule' AS kind, c.id, c.project_id, c.rule_id AS item_id, i.name, i.source_path,
        c.engine, c.detected_at, c.local_hash, c.source_hash, p.path AS project_path
     FROM rule_sync_conflicts c JOIN rules i ON i.id = c.rule_id JOIN projects p ON p.id = c.project_id
     WHERE {rule}";

fn located(row: &sqlx::sqlite::SqliteRow) -> Result<(ProjectSyncConflict, Layout), String> {
    let kind = if row.get::<String, _>("kind") == "skill" { ItemKind::Skill } else { ItemKind::Rule };
    let (engine, name): (String, String) = (row.get("engine"), row.get("name"));
    let (source_path, project_path): (String, String) = (row.get("source_path"), row.get("project_path"));
    let l = layout(kind, &engine, &project_path, &name, &source_path, codex_mode_of(kind, &engine, &source_path))?;
    let conflict = ProjectSyncConflict {
        id: row.get("id"),
        kind,
        project_id: row.get("project_id"),
        item_id: row.get("item_id"),
        name,
        engine,
        project_path: l.project_file.display().to_string(),
        block: l.block.clone(),
        central_path: l.central_file.display().to_string(),
        project_mtime: mtime_ms(&l.project_file),
        central_mtime: mtime_ms(&l.central_file),
        detected_at: row.get("detected_at"),
        local_hash: row.get("local_hash"),
        source_hash: row.get("source_hash"),
    };
    Ok((conflict, l))
}

pub async fn list_for_project(db: &Db, project_id: &str) -> Result<Vec<ProjectSyncConflict>, String> {
    let filter = "c.resolved = 0 AND p.deleted_at IS NULL AND c.project_id = ?";
    let sql = format!("{} ORDER BY detected_at DESC", ROW_SQL.replace("{skill}", filter).replace("{rule}", filter));
    let rows = sqlx::query(&sql).bind(project_id).bind(project_id).fetch_all(db).await.map_err(|e| e.to_string())?;
    rows.iter().map(|r| located(r).map(|(c, _)| c)).collect()
}

/// One version of a conflict. `error` says why `content` is missing, for this side only.
#[derive(Debug, Clone, Serialize)]
pub struct ConflictSide {
    pub path: String,
    pub block: Option<String>,
    /// For a rule's Codex forms, the body without frontmatter: what Codex reads.
    pub content: Option<String>,
    /// What the scan compares (see [`version_hash`]).
    pub hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncConflictSides {
    pub conflict: ProjectSyncConflict,
    pub project: ConflictSide,
    pub central: ConflictSide,
}

fn side(path: &Path, block: Option<&str>, body_only: bool) -> ConflictSide {
    let read = read_text(path).and_then(|text| match block {
        Some(name) => extract_agents_block(path, name)
            .map(|b| (b.trim_end_matches('\n').to_string(), block_hash(&b)))
            .ok_or_else(|| format!("{} has no block for {}", path.display(), name)),
        None if body_only => Ok((strip_frontmatter(&text), version_hash(&text, true))),
        None => Ok((text.clone(), version_hash(&text, false))),
    });
    let (content, hash, error) = match read {
        Ok((c, h)) => (Some(c), Some(h), None),
        Err(e) => (None, None, Some(e)),
    };
    ConflictSide { path: path.display().to_string(), block: block.map(str::to_string), content, hash, error }
}

pub async fn read_sides(db: &Db, kind: ItemKind, conflict_id: &str) -> Result<SyncConflictSides, String> {
    let (skill, rule) = match kind {
        ItemKind::Skill => ("c.id = ?", "0"),
        ItemKind::Rule => ("0", "c.id = ?"),
    };
    let sql = ROW_SQL.replace("{skill}", skill).replace("{rule}", rule);
    let row = sqlx::query(&sql)
        .bind(conflict_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Sync conflict not found: {}", conflict_id))?;
    let (conflict, l) = located(&row)?;
    Ok(SyncConflictSides {
        project: side(&l.project_file, l.block.as_deref(), l.body_only),
        central: side(&l.central_file, None, l.body_only),
        conflict,
    })
}

// ---- Commands (remote/guard.rs: all three allowed from a remote desktop) ---------------------

/// Compare what is on disk with the library and update the conflict tables. Reads files only.
#[tauri::command]
pub async fn scan_sync_conflicts(db: State<'_, Db>, project_id: Option<String>) -> Result<ScanReport, String> {
    scan(db.inner(), project_id.as_deref()).await
}

#[tauri::command]
pub async fn list_project_sync_conflicts(db: State<'_, Db>, project_id: String) -> Result<Vec<ProjectSyncConflict>, String> {
    list_for_project(db.inner(), &project_id).await
}

#[tauri::command]
pub async fn read_sync_conflict_sides(
    db: State<'_, Db>,
    kind: ItemKind,
    conflict_id: String,
) -> Result<SyncConflictSides, String> {
    read_sides(db.inner(), kind, &conflict_id).await
}
