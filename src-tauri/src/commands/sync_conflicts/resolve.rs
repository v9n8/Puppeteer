//! Resolving a sync conflict, and letting go of items UX-58 turned into files.
//!
//! "Overwrite with the central version" re-links the item; the project's version is never lost:
//! a copy goes to the legacy backups (`project_links`), and an `AGENTS.md` whose block is
//! rewritten is copied there first. "Keep local" only closes the row, whose hash pair then stands
//! for the user's choice (see the module docs above); every other close here uses [`CLOSE_NOT_KEPT`].

use super::{ItemKind, CLOSE_NOT_KEPT};
use crate::commands::library_disk::materialize::MaterializeOutcome;
use crate::commands::project_links::{self, no_copies, project_backup_dir};
use crate::commands::rules::{
    agents_path, applies_claude, applies_codex, install_codex_rule, link_claude_rule, parse_codex_mode,
    read_rule_source, CodexMode,
};
use crate::commands::skills::{claude_skill_dir, legacy_codex_skill_dir, write_skill_artifacts, SkillEngines};
use crate::db::Db;
use sqlx::Row;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// `resolve_sync_conflict`. A conflict whose skill is no longer applied has nothing left to
/// overwrite and is only closed.
pub(crate) async fn resolve_skill(db: &Db, conflict_id: &str, overwrite: bool) -> Result<(), String> {
    if overwrite {
        let row = sqlx::query(
            "SELECT sc.project_id, sc.skill_id, sc.engine, s.source_path, s.name as skill_name,
                    ps.enabled_claude, ps.enabled_codex, p.path as project_path,
                    ps.synced_hash_claude, ps.synced_hash_codex
             FROM sync_conflicts sc
             JOIN skills s ON s.id = sc.skill_id
             JOIN projects p ON p.id = sc.project_id
             JOIN project_skills ps ON ps.project_id = sc.project_id AND ps.skill_id = sc.skill_id
             WHERE sc.id = ?",
        )
        .bind(conflict_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;

        if let Some(row) = row {
            let project_path: String = row.get("project_path");
            let source_path: String = row.get("source_path");
            let skill_name: String = row.get("skill_name");
            let engine: String = row.get("engine");
            let prev_claude: Option<String> = row.get("synced_hash_claude");
            let prev_codex: Option<String> = row.get("synced_hash_codex");

            // Codex: a copy-era `.codex/skills` copy (a real folder in `.agents/skills` is backed up
            // by the re-link below). Through the guard every removal uses, so a symlinked parent is
            // never followed; `no_copies`: backed up, not deleted.
            let copy = if engine == "codex" {
                legacy_codex_skill_dir(&project_path, &skill_name)
            } else {
                claude_skill_dir(&project_path, &skill_name)
            };
            project_links::remove_installed(Path::new(&project_path), &copy, Path::new(&source_path), no_copies)?;

            let (hash_claude, hash_codex) = write_skill_artifacts(
                &project_path,
                &skill_name,
                SkillEngines::from_row(&row),
                &source_path,
                prev_claude.as_deref(),
                prev_codex.as_deref(),
            )?;

            sqlx::query(
                "UPDATE project_skills SET synced_hash_claude = ?, synced_hash_codex = ? WHERE project_id = ? AND skill_id = ?",
            )
            .bind(&hash_claude)
            .bind(&hash_codex)
            .bind(row.get::<String, _>("project_id"))
            .bind(row.get::<String, _>("skill_id"))
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    sqlx::query(&close_sql("sync_conflicts", overwrite))
        .bind(conflict_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// `resolve_rule_sync_conflict`.
pub(crate) async fn resolve_rule(db: &Db, conflict_id: &str, overwrite: bool) -> Result<(), String> {
    if overwrite {
        let row = sqlx::query(
            "SELECT rc.project_id, rc.rule_id, rc.engine, r.name, r.description, r.source_path,
                    p.path as project_path
             FROM rule_sync_conflicts rc
             JOIN rules r ON r.id = rc.rule_id
             JOIN projects p ON p.id = rc.project_id
             WHERE rc.id = ?",
        )
        .bind(conflict_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;

        let engine: String = row.get("engine");
        let name: String = row.get("name");
        let source_path: String = row.get("source_path");
        let project_path: String = row.get("project_path");

        let content = read_rule_source(&source_path)?;
        let (sql, hash) = if engine == "claude" {
            // No base hash: a copy that isn't the source is backed up, not deleted.
            let h = link_claude_rule(&project_path, &name, &source_path, &content, None)?;
            ("UPDATE project_rules SET synced_hash_claude = ? WHERE project_id = ? AND rule_id = ?", h)
        } else {
            back_up_agents_md(&project_path)?;
            let description: String = row.get("description");
            let h = install_codex_rule(&project_path, &name, &description, &source_path, &content)?;
            ("UPDATE project_rules SET synced_hash_codex = ? WHERE project_id = ? AND rule_id = ?", h)
        };
        sqlx::query(sql)
            .bind(&hash)
            .bind(row.get::<String, _>("project_id"))
            .bind(row.get::<String, _>("rule_id"))
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
    }

    sqlx::query(&close_sql("rule_sync_conflicts", overwrite))
        .bind(conflict_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Close one row; only "Keep local" (`overwrite` false) keeps its pair.
fn close_sql(table: &str, overwrite: bool) -> String {
    let set = if overwrite { CLOSE_NOT_KEPT } else { "resolved = 1" };
    format!("UPDATE {table} SET {set} WHERE id = ?")
}

/// Copy a project's `AGENTS.md` to `<legacy backups>/<project>/<timestamp>/AGENTS.md` before a
/// block in it is rewritten. Missing, or a link (never written through), there is nothing to keep.
pub(crate) fn back_up_agents_md(project_path: &str) -> Result<Option<PathBuf>, String> {
    let file = agents_path(project_path);
    match fs::symlink_metadata(&file) {
        Ok(meta) if meta.is_file() => {}
        _ => return Ok(None),
    }
    let base = project_backup_dir(Path::new(project_path));
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut dir = base.join(&stamp);
    let mut n = 1;
    while dir.join("AGENTS.md").exists() {
        dir = base.join(format!("{}-{}", stamp, n));
        n += 1;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let target = dir.join("AGENTS.md");
    fs::copy(&file, &target).map_err(|e| format!("{}: {}", target.display(), e))?;
    tracing::info!("backed up {} to {} before overwriting a block", file.display(), target.display());
    Ok(Some(target))
}

// ---- Items turned into files (UX-58) ---------------------------------------------------------

/// What a path `library_materialize` changed was, relative to its project.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Materialized {
    /// A link in an engine's skill folder (`.agents/skills` also holds a rule's Codex skill form).
    SkillLink { name: String, engine: &'static str },
    ClaudeRule(String),
    /// Every Puppeteer block in it lost its markers.
    AgentsMd,
}

pub(crate) fn materialized(rel: &Path) -> Option<Materialized> {
    let parts: Option<Vec<&str>> =
        rel.components().map(|c| if let Component::Normal(s) = c { s.to_str() } else { None }).collect();
    match parts?.as_slice() {
        [".claude", "skills", name] => Some(Materialized::SkillLink { name: name.to_string(), engine: "claude" }),
        [".agents", "skills", name] => Some(Materialized::SkillLink { name: name.to_string(), engine: "codex" }),
        [".claude", "rules", file] => {
            file.strip_suffix(".md").filter(|n| !n.is_empty()).map(|n| Materialized::ClaudeRule(n.to_string()))
        }
        ["AGENTS.md"] => Some(Materialized::AgentsMd),
        _ => None,
    }
}

/// A rule's target with `engine` dropped; None when nothing is left.
pub(crate) fn target_without(target: &str, engine: &str) -> Option<&'static str> {
    match (applies_claude(target) && engine != "claude", applies_codex(target) && engine != "codex") {
        (true, true) => Some("both"),
        (true, false) => Some("claude"),
        (false, true) => Some("codex"),
        (false, false) => None,
    }
}

/// After `library_materialize`: the items whose links (or `AGENTS.md` blocks) became the user's
/// own files are no longer linked to the project for that engine, and their open conflicts close.
/// The files stay as they are. A failure is logged; the files were already changed.
pub async fn detach_materialized(db: &Db, outcome: &MaterializeOutcome) {
    if let Err(e) = detach_paths(db, &outcome.done).await {
        tracing::warn!(error = %e, "library: unlinking turned-into-file items from projects failed");
    }
}

pub(crate) async fn detach_paths(db: &Db, done: &[String]) -> Result<(), String> {
    let projects: Vec<(String, String)> = sqlx::query_as("SELECT id, path FROM projects WHERE deleted_at IS NULL")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    for path in done {
        for (project_id, root) in &projects {
            let Some(item) = Path::new(path).strip_prefix(root).ok().and_then(materialized) else { continue };
            match item {
                Materialized::SkillLink { name, engine } => {
                    detach_skill(db, project_id, &name, engine).await?;
                    if engine == "codex" {
                        for rule_id in codex_rules(db, project_id, CodexMode::Skill, Some(&name)).await? {
                            detach_rule(db, project_id, &rule_id, "codex").await?;
                        }
                    }
                }
                Materialized::ClaudeRule(name) => {
                    let id: Option<String> = sqlx::query_scalar("SELECT id FROM rules WHERE name = ?")
                        .bind(&name)
                        .fetch_optional(db)
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(id) = id {
                        detach_rule(db, project_id, &id, "claude").await?;
                    }
                }
                Materialized::AgentsMd => {
                    for rule_id in codex_rules(db, project_id, CodexMode::Inline, None).await? {
                        detach_rule(db, project_id, &rule_id, "codex").await?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// The project's rules applied for Codex in `mode` (by its source now), optionally by name.
async fn codex_rules(db: &Db, project_id: &str, mode: CodexMode, name: Option<&str>) -> Result<Vec<String>, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT r.id, r.source_path, pr.target FROM project_rules pr JOIN rules r ON r.id = pr.rule_id
         WHERE pr.project_id = ? AND (? IS NULL OR r.name = ?)",
    )
    .bind(project_id)
    .bind(name)
    .bind(name)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|(_, source, target)| {
            applies_codex(target)
                && fs::read_to_string(source).map(|c| parse_codex_mode(&c)).unwrap_or(CodexMode::Inline) == mode
        })
        .map(|(id, _, _)| id)
        .collect())
}

async fn detach_skill(db: &Db, project_id: &str, name: &str, engine: &str) -> Result<(), String> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM skills WHERE name = ?")
        .bind(name)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(skill_id) = id else { return Ok(()) };
    let sql = match engine {
        "claude" => "UPDATE project_skills SET enabled_claude = 0, synced_hash_claude = NULL WHERE project_id = ? AND skill_id = ?",
        _ => "UPDATE project_skills SET enabled_codex = 0, synced_hash_codex = NULL WHERE project_id = ? AND skill_id = ?",
    };
    for sql in [
        sql,
        "DELETE FROM project_skills WHERE project_id = ? AND skill_id = ? AND enabled_claude = 0 AND enabled_codex = 0",
    ] {
        sqlx::query(sql).bind(project_id).bind(&skill_id).execute(db).await.map_err(|e| e.to_string())?;
    }
    close_open(db, ItemKind::Skill, project_id, &skill_id, engine).await
}

async fn detach_rule(db: &Db, project_id: &str, rule_id: &str, engine: &str) -> Result<(), String> {
    let target: Option<String> = sqlx::query_scalar("SELECT target FROM project_rules WHERE project_id = ? AND rule_id = ?")
        .bind(project_id)
        .bind(rule_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(target) = target else { return Ok(()) };
    let query = match target_without(&target, engine) {
        Some(rest) if engine == "claude" => {
            sqlx::query("UPDATE project_rules SET target = ?, synced_hash_claude = NULL WHERE project_id = ? AND rule_id = ?")
                .bind(rest)
        }
        Some(rest) => {
            sqlx::query("UPDATE project_rules SET target = ?, synced_hash_codex = NULL WHERE project_id = ? AND rule_id = ?")
                .bind(rest)
        }
        None => sqlx::query("DELETE FROM project_rules WHERE project_id = ? AND rule_id = ?"),
    };
    query.bind(project_id).bind(rule_id).execute(db).await.map_err(|e| e.to_string())?;
    close_open(db, ItemKind::Rule, project_id, rule_id, engine).await
}

async fn close_open(db: &Db, kind: ItemKind, project_id: &str, item_id: &str, engine: &str) -> Result<(), String> {
    let sql = format!(
        "UPDATE {} SET {CLOSE_NOT_KEPT} WHERE project_id = ? AND {} = ? AND engine = ? AND resolved = 0",
        kind.table(),
        kind.item_column()
    );
    sqlx::query(&sql).bind(project_id).bind(item_id).bind(engine).execute(db).await.map_err(|e| e.to_string())?;
    Ok(())
}
