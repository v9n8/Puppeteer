use crate::commands::project_links::{self, Linked};
use crate::commands::agents_block;
use crate::db::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target: String,
    pub source_path: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillContent {
    pub skill: Skill,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillPayload {
    pub name: String,
    pub description: String,
    pub target: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkillPayload {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target: String,
    pub content: String,
}

pub(crate) fn skills_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap().join("skills")
}

pub fn skill_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

pub(crate) fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name cannot be empty".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Skill name must only contain letters, numbers, hyphens, and underscores".to_string());
    }
    Ok(())
}

fn parse_frontmatter(content: &str) -> (String, String) {
    // Extract name and description from YAML frontmatter
    if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("---") {
            let fm = &rest[..end];
            let name = fm.lines()
                .find(|l| l.starts_with("name:"))
                .map(|l| l["name:".len()..].trim().trim_matches('"').to_string())
                .unwrap_or_default();
            let description = fm.lines()
                .find(|l| l.starts_with("description:"))
                .map(|l| l["description:".len()..].trim().trim_matches('"').to_string())
                .unwrap_or_default();
            return (name, description);
        }
    }
    (String::new(), String::new())
}

/// Read the `target:` field from a skill's SKILL.md frontmatter (defaults to "claude").
fn parse_target(content: &str) -> String {
    if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            if let Some(t) = fm.lines().find(|l| l.trim_start().starts_with("target:")) {
                let v = t.trim_start()["target:".len()..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                if !v.is_empty() {
                    return v;
                }
            }
        }
    }
    "claude".to_string()
}

fn validate_target(target: &str) -> Result<(), String> {
    match target {
        "claude" | "codex" | "both" => Ok(()),
        _ => Err("target must be one of: claude, codex, both".to_string()),
    }
}

pub fn applies_claude(target: &str) -> bool {
    target == "claude" || target == "both"
}
pub fn applies_codex(target: &str) -> bool {
    target == "codex" || target == "both"
}

/// Engines a skill is switched on for in one project (`project_skills.enabled_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEngines {
    pub claude: bool,
    pub codex: bool,
}

impl SkillEngines {
    pub const NONE: SkillEngines = SkillEngines { claude: false, codex: false };

    /// A skill's `target` is only the default for a project it is newly applied to.
    pub fn from_target(target: &str) -> Self {
        SkillEngines { claude: applies_claude(target), codex: applies_codex(target) }
    }

    /// From a `project_skills` row selected with its `enabled_claude` / `enabled_codex`.
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Self {
        use sqlx::Row;
        SkillEngines {
            claude: row.get::<i64, _>("enabled_claude") != 0,
            codex: row.get::<i64, _>("enabled_codex") != 0,
        }
    }

    /// Whether the engine with this `SKILL_ENGINES` id is switched on.
    pub fn get(&self, engine: &str) -> bool {
        match engine {
            "claude" => self.claude,
            "codex" => self.codex,
            _ => false,
        }
    }

    /// Switch one engine on or off; an id this table has no column for is an error.
    pub fn set(&mut self, engine: &str, on: bool) -> Result<(), String> {
        match engine {
            "claude" => self.claude = on,
            "codex" => self.codex = on,
            _ => return Err(format!("Unknown engine: {}", engine)),
        }
        Ok(())
    }
}

/// An engine skills can be switched on for, and where it reads them.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SkillEngineDecl {
    pub id: &'static str,
    /// Folder holding `<name>/` links, relative to the project root.
    pub project_dir: &'static str,
    /// The engine's global (personal) folder, relative to HOME: the Allocation matrix's Global
    /// column installs there. `None` while it is unverified.
    pub home_dir: Option<&'static str>,
    /// Whether a skill in `home_dir` wins over the project's own skill of that name.
    #[serde(skip)]
    pub home_wins: bool,
}

/// The engines, in chip order. The allocation matrix is built from this list, so another
/// engine is one more entry here (plus its `project_skills` switch and `SkillEngines` field).
pub const SKILL_ENGINES: &[SkillEngineDecl] = &[
    // Personal skills beat project ones in Claude Code ("personal over project").
    SkillEngineDecl { id: "claude", project_dir: ".claude/skills", home_dir: Some(".claude/skills"), home_wins: true },
    // Codex's user scope is `$HOME/.agents/skills`; it doesn't merge same-named skills, both
    // show up (developers.openai.com/codex/skills).
    SkillEngineDecl { id: "codex", project_dir: ".agents/skills", home_dir: Some(".agents/skills"), home_wins: false },
];

pub fn skill_engine(id: &str) -> Result<&'static SkillEngineDecl, String> {
    SKILL_ENGINES.iter().find(|e| e.id == id).ok_or_else(|| format!("Unknown engine: {}", id))
}

/// `<HOME>/<engine's global folder>/<name>`, for each engine that has one.
pub fn home_skill_dirs(home: &Path, name: &str) -> Vec<(&'static SkillEngineDecl, PathBuf)> {
    SKILL_ENGINES
        .iter()
        .filter_map(|e| e.home_dir.map(|dir| (e, home.join(dir).join(name))))
        .collect()
}

// ---- Per-engine destinations -------------------------------------------------

/// Claude reads skills from `<project>/.claude/skills/<name>/`.
pub fn claude_skill_dir(project_path: &str, name: &str) -> PathBuf {
    Path::new(project_path).join(".claude").join("skills").join(name)
}

/// Codex discovers skills natively in `<project>/.agents/skills/<name>/` (Claude never reads
/// `.agents/`, so the two engines don't see each other's skills).
pub fn codex_skill_dir(project_path: &str, name: &str) -> PathBuf {
    Path::new(project_path).join(".agents").join("skills").join(name)
}

/// Pre-`.agents/skills` location: a copy in `.codex/skills/<name>/` plus a pointer block in
/// AGENTS.md. Only cleaned up now.
pub fn legacy_codex_skill_dir(project_path: &str, name: &str) -> PathBuf {
    Path::new(project_path).join(".codex").join("skills").join(name)
}

pub fn agents_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join("AGENTS.md")
}

/// Read a skill's source SKILL.md content (empty string if missing).
pub fn read_skill_md(source_path: &str) -> String {
    fs::read_to_string(Path::new(source_path).join("SKILL.md")).unwrap_or_default()
}

/// Link a skill into a project for `engines` and drop what no longer applies, returning
/// (claude_hash, codex_hash) — the source SKILL.md hash per engine it is installed for.
///
/// `prev_*` are the SKILL.md hashes recorded at the last sync (None = not installed for that
/// engine); only an engine Puppeteer installed before is cleaned up. A copy-era folder is deleted when
/// swapped for a link (or removed) only if it is unedited: SKILL.md matches the source or the last
/// sync, and every other file matches the source or was not written after that SKILL.md. Any
/// other copy is moved to the legacy backups, never deleted.
pub fn write_skill_artifacts(
    project_path: &str,
    name: &str,
    engines: SkillEngines,
    source_path: &str,
    prev_claude: Option<&str>,
    prev_codex: Option<&str>,
) -> Result<(Option<String>, Option<String>), String> {
    let hash = skill_hash(&read_skill_md(source_path));
    let root = Path::new(project_path);
    let source = Path::new(source_path);
    let unedited_for = |prev: Option<&str>| {
        let hash = hash.clone();
        let prev = prev.map(str::to_string);
        move |copy: &Path| {
            let skill_md = copy.join("SKILL.md");
            let Ok(md) = fs::read_to_string(&skill_md) else { return false };
            let md = skill_hash(&md);
            // Other files: identical to the source, or not written since the sync pass that wrote
            // this SKILL.md (a stale copy whose source moved on is still unedited).
            (md == hash || Some(&md) == prev.as_ref())
                && (project_links::same_tree_except(copy, source, Some(Path::new("SKILL.md")))
                    || project_links::untouched_since(copy, &skill_md))
        }
    };
    let had_claude = prev_claude.is_some();
    let had_codex = prev_codex.is_some();

    let claude_dest = claude_skill_dir(project_path, name);
    // A path git tracks is the repo's own skill: left alone and not recorded as installed.
    let hash_claude = if engines.claude {
        match project_links::ensure_symlink(root, &claude_dest, source, unedited_for(prev_claude))? {
            Linked::Yes => Some(hash.clone()),
            Linked::TrackedByGit => None,
        }
    } else {
        if had_claude {
            project_links::remove_installed(root, &claude_dest, source, unedited_for(prev_claude))?;
        }
        None
    };

    let codex_dest = codex_skill_dir(project_path, name);
    let legacy = legacy_codex_skill_dir(project_path, name);
    let unedited = unedited_for(prev_codex);
    let hash_codex = if engines.codex {
        let linked = project_links::ensure_symlink(root, &codex_dest, source, &unedited)?;
        project_links::remove_installed(root, &legacy, source, &unedited)?;
        match linked {
            Linked::Yes => Some(hash),
            Linked::TrackedByGit => None,
        }
    } else {
        if had_codex {
            // Puppeteer only ever linked into `.agents/skills`; a real folder there is someone else's.
            project_links::remove_own_link(root, &codex_dest, source)?;
            project_links::remove_installed(root, &legacy, source, &unedited)?;
        }
        None
    };
    // The copy era's AGENTS.md pointer; `.agents/skills` needs none.
    if engines.codex || had_codex {
        agents_block::remove(&agents_path(project_path), "skill", name);
    }
    Ok((hash_claude, hash_codex))
}

/// Switch a skill on or off in one engine's global folder. Only Puppeteer's own link is ever
/// created or removed there: a skill of that name the user keeps in HOME is reported, not touched.
pub fn switch_global(home: &Path, engine: &str, name: &str, source_path: &str, on: bool) -> Result<(), String> {
    let engine = skill_engine(engine)?;
    let dir = engine.home_dir.ok_or_else(|| format!("No global skills folder for {}", engine.id))?;
    let (dest, source) = (home.join(dir).join(name), Path::new(source_path));
    if on {
        project_links::link_if_free(home, &dest, source)
    } else {
        project_links::unlink_if_own(home, &dest, source).map(|_| ())
    }
}

/// Drop a skill's own links from the global folders (before a rename or a delete), returning the
/// engines it was linked for.
pub fn unlink_global(home: &Path, name: &str, source_path: &str) -> Result<Vec<&'static str>, String> {
    let mut engines = Vec::new();
    for (engine, dest) in home_skill_dirs(home, name) {
        if project_links::unlink_if_own(home, &dest, Path::new(source_path))? {
            engines.push(engine.id);
        }
    }
    Ok(engines)
}

/// Remove a skill's artifacts (legacy layout included) for the engines it was installed for.
/// Legacy copies Puppeteer can't vouch for are moved to the backups.
pub fn remove_skill_artifacts(
    project_path: &str,
    name: &str,
    source_path: &str,
    prev_claude: Option<&str>,
    prev_codex: Option<&str>,
) -> Result<(), String> {
    write_skill_artifacts(project_path, name, SkillEngines::NONE, source_path, prev_claude, prev_codex)
        .map(|_| ())
}

#[tauri::command]
pub async fn list_skills(db: State<'_, Db>) -> Result<Vec<Skill>, String> {
    let rows = sqlx::query("SELECT id, name, description, target, source_path, updated_at FROM skills ORDER BY name")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let skills = rows.iter().map(|row| {
        use sqlx::Row;
        Skill {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            target: row.get("target"),
            source_path: row.get("source_path"),
            updated_at: row.get("updated_at"),
        }
    }).collect();

    Ok(skills)
}

#[tauri::command]
pub async fn get_skill(db: State<'_, Db>, id: String) -> Result<SkillContent, String> {
    use sqlx::Row;
    let row = sqlx::query("SELECT id, name, description, target, source_path, updated_at FROM skills WHERE id = ?")
        .bind(&id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let source_path: String = row.get("source_path");
    let skill_md = Path::new(&source_path).join("SKILL.md");
    let content = fs::read_to_string(&skill_md).unwrap_or_default();

    Ok(SkillContent {
        skill: Skill {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            target: row.get("target"),
            source_path,
            updated_at: row.get("updated_at"),
        },
        content,
    })
}

#[tauri::command]
pub async fn create_skill(
    app: AppHandle,
    db: State<'_, Db>,
    payload: CreateSkillPayload,
) -> Result<Skill, String> {
    validate_skill_name(&payload.name)?;
    validate_target(&payload.target)?;

    if payload.description.is_empty() {
        return Err("Description is required".to_string());
    }

    // Check name uniqueness
    let existing = sqlx::query("SELECT id FROM skills WHERE name = ? COLLATE NOCASE")
        .bind(&payload.name)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_some() {
        return Err(format!("Skill '{}' already exists", payload.name));
    }

    let id = Uuid::new_v4().to_string();
    let skill_dir = skills_dir(&app).join(&payload.name);
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    fs::write(skill_dir.join("SKILL.md"), &payload.content).map_err(|e| e.to_string())?;

    let source_path = skill_dir.to_string_lossy().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&payload.name)
        .bind(&payload.description)
        .bind(&payload.target)
        .bind(&source_path)
        .bind(&now)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(Skill { id, name: payload.name, description: payload.description, target: payload.target, source_path, updated_at: now })
}

/// Rename a skill's library folder to `new_dir`, then drop its links under the old name from every
/// applied project and from HOME. Returns the engines it was linked for in HOME, and what couldn't
/// be cleaned up. The move comes first: if it fails, every link still works.
async fn rename_skill_dir(
    db: &Db,
    home: Option<&Path>,
    skill_id: &str,
    old_name: &str,
    old_path: &str,
    new_dir: &Path,
) -> Result<(Vec<&'static str>, Vec<String>), String> {
    use sqlx::Row;
    fs::rename(old_path, new_dir).map_err(|e| e.to_string())?;
    let projects = sqlx::query(
        "SELECT p.path, ps.synced_hash_claude, ps.synced_hash_codex
         FROM project_skills ps JOIN projects p ON p.id = ps.project_id WHERE ps.skill_id = ?"
    )
    .bind(skill_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for prow in &projects {
        let ppath: String = prow.get("path");
        let prev_claude: Option<String> = prow.get("synced_hash_claude");
        let prev_codex: Option<String> = prow.get("synced_hash_codex");
        if let Err(e) = remove_skill_artifacts(&ppath, old_name, old_path, prev_claude.as_deref(), prev_codex.as_deref()) {
            errors.push(e);
        }
    }
    let mut relink_global = Vec::new();
    if let Some(home) = home {
        match unlink_global(home, old_name, old_path) {
            Ok(engines) => relink_global = engines,
            Err(e) => errors.push(e),
        }
    }
    Ok((relink_global, errors))
}

#[tauri::command]
pub async fn update_skill(
    app: AppHandle,
    db: State<'_, Db>,
    payload: UpdateSkillPayload,
) -> Result<Skill, String> {
    validate_skill_name(&payload.name)?;
    validate_target(&payload.target)?;

    if payload.description.is_empty() {
        return Err("Description is required".to_string());
    }

    // Get current skill
    use sqlx::Row;
    let row = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(&payload.id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| format!("Skill not found: {}", e))?;

    let old_name: String = row.get("name");
    let old_path: String = row.get("source_path");

    if old_name != payload.name {
        let taken = sqlx::query("SELECT id FROM skills WHERE name = ? COLLATE NOCASE AND id != ?")
            .bind(&payload.name)
            .bind(&payload.id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        let new_dir = skills_dir(&app).join(&payload.name);
        let case_only = project_links::same_file(&new_dir, Path::new(&old_path));
        if taken.is_some() {
            return Err(format!("Skill '{}' already exists", payload.name));
        }
        if new_dir.exists() && !case_only {
            // Not in the DB: a leftover from an old rename/delete. Move it aside, don't overwrite.
            project_links::back_up_library_orphan(&new_dir, "skills")?;
        }
    }

    let home = app.path().home_dir().ok();
    let (new_source_path, relink_global, mut failures) = if old_name != payload.name {
        let new_dir = skills_dir(&app).join(&payload.name);
        let (relink, errors) =
            rename_skill_dir(db.inner(), home.as_deref(), &payload.id, &old_name, &old_path, &new_dir).await?;
        (new_dir.to_string_lossy().to_string(), relink, errors)
    } else {
        (old_path, Vec::new(), Vec::new())
    };

    fs::write(Path::new(&new_source_path).join("SKILL.md"), &payload.content)
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE skills SET name = ?, description = ?, target = ?, source_path = ?, updated_at = ? WHERE id = ?")
        .bind(&payload.name)
        .bind(&payload.description)
        .bind(&payload.target)
        .bind(&new_source_path)
        .bind(&now)
        .bind(&payload.id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Projects link to the source, so content is already live; sync covers renames. A new target
    // is only the default for projects applied from now on. Failures surface so a broken link is
    // never silent.
    failures.extend(sync_skill_to_projects(&db, &payload.id).await.map_err(|e| e.to_string())?);
    // A global link follows the rename, unless the new name is taken in that folder.
    if let Some(home) = &home {
        for engine in relink_global {
            if let Err(e) = switch_global(home, engine, &payload.name, &new_source_path, true) {
                failures.push(e);
            }
        }
    }
    if !failures.is_empty() {
        return Err(format!("Saved, but syncing to projects failed:\n{}", failures.join("\n")));
    }

    Ok(Skill {
        id: payload.id,
        name: payload.name,
        description: payload.description,
        target: payload.target,
        source_path: new_source_path,
        updated_at: now,
    })
}

#[tauri::command]
pub async fn delete_skill(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
) -> Result<(), String> {
    use sqlx::Row;
    let row = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(&id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| format!("Skill not found: {}", e))?;

    let source_path: String = row.get("source_path");

    // Drop the project links first — they would dangle once the source is gone.
    let projects = sqlx::query(
        "SELECT p.path, s.name, ps.synced_hash_claude, ps.synced_hash_codex FROM project_skills ps
         JOIN projects p ON p.id = ps.project_id
         JOIN skills s ON s.id = ps.skill_id
         WHERE ps.skill_id = ?",
    )
    .bind(&id)
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for prow in &projects {
        let ppath: String = prow.get("path");
        let name: String = prow.get("name");
        let prev_claude: Option<String> = prow.get("synced_hash_claude");
        let prev_codex: Option<String> = prow.get("synced_hash_codex");
        if let Err(e) = remove_skill_artifacts(&ppath, &name, &source_path, prev_claude.as_deref(), prev_codex.as_deref()) {
            errors.push(e);
        }
    }
    // Global links too, whether or not the Global scope is switched on now.
    if let Ok(home) = app.path().home_dir() {
        if let Err(e) = unlink_global(&home, &row.get::<String, _>("name"), &source_path) {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        return Err(format!("Not deleted — clean these up first:\n{}", errors.join("\n")));
    }

    let _ = fs::remove_dir_all(&source_path);

    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn export_skill_zip(
    webview: tauri::Webview,
    db: State<'_, Db>,
    id: String,
    dest_path: String,
) -> Result<(), String> {
    // A remote desktop exports anywhere it may write.
    let dest_path = match crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await? {
        Some(p) => p.write(Path::new(&dest_path))?.to_string_lossy().into_owned(),
        None => dest_path,
    };
    use sqlx::Row;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let row = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(&id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let name: String = row.get("name");
    let source_path: String = row.get("source_path");

    let file = fs::File::create(&dest_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    for entry in WalkDir::new(&source_path) {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            let rel = path.strip_prefix(&source_path).map_err(|e| e.to_string())?;
            let zip_path = format!("{}/{}", name, rel.to_string_lossy());
            zip.start_file(&zip_path, options).map_err(|e| e.to_string())?;
            let data = fs::read(path).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn import_skill_zip(
    app: AppHandle,
    webview: tauri::Webview,
    db: State<'_, Db>,
    zip_path: String,
) -> Result<Skill, String> {
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    let zip_path = crate::remote::fs_policy::read(policy.as_ref(), Path::new(&zip_path))?
        .to_string_lossy()
        .into_owned();
    use std::io::Read;
    use zip::ZipArchive;

    let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    // Detect skill name from first path component
    let skill_name = {
        let first = archive.by_index(0).map_err(|e| e.to_string())?;
        first.name().split('/').next().unwrap_or("imported").to_string()
    };

    validate_skill_name(&skill_name)?;

    let dest_dir = skills_dir(&app).join(&skill_name);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    // Extract files
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // Strip first component (skill name prefix)
        let rel: PathBuf = name.split('/').skip(1).collect();
        if rel.to_string_lossy().is_empty() {
            continue;
        }
        let out = dest_dir.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            fs::write(&out, &data).map_err(|e| e.to_string())?;
        }
    }

    // Read SKILL.md to get description
    let skill_md_path = dest_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md_path).unwrap_or_default();
    let (_, description) = parse_frontmatter(&content);
    let description = if description.is_empty() { skill_name.clone() } else { description };
    let target = parse_target(&content);

    // Upsert into DB
    let existing = sqlx::query("SELECT id FROM skills WHERE name = ? COLLATE NOCASE")
        .bind(&skill_name)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    use sqlx::Row;
    let id = if let Some(row) = existing {
        let id: String = row.get("id");
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE skills SET description = ?, target = ?, source_path = ?, updated_at = ? WHERE id = ?")
            .bind(&description)
            .bind(&target)
            .bind(dest_dir.to_string_lossy().as_ref())
            .bind(&now)
            .bind(&id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        id
    } else {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&skill_name)
            .bind(&description)
            .bind(&target)
            .bind(dest_dir.to_string_lossy().as_ref())
            .bind(&now)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        id
    };

    let now = chrono::Utc::now().to_rfc3339();
    Ok(Skill {
        id,
        name: skill_name,
        description,
        target,
        source_path: dest_dir.to_string_lossy().to_string(),
        updated_at: now,
    })
}

#[tauri::command]
pub async fn open_skill_folder(db: State<'_, Db>, id: String) -> Result<(), String> {
    use sqlx::Row;
    let row = sqlx::query("SELECT source_path FROM skills WHERE id = ?")
        .bind(&id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| format!("Skill not found: {}", e))?;

    let source_path: String = row.get("source_path");

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&source_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&source_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&source_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Every regular file under a skill folder as (relative path → bytes), skipping Finder litter.
pub(crate) fn read_skill_tree(dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    for entry in WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() || entry.file_name() == ".DS_Store" {
            continue;
        }
        let rel = entry.path().strip_prefix(dir).map_err(|e| e.to_string())?.to_path_buf();
        let data = fs::read(entry.path()).map_err(|e| e.to_string())?;
        files.push((rel, data));
    }
    Ok(files)
}

/// Create or overwrite (by name) a library skill from a settings backup. A differing local folder
/// is moved to the legacy backups before the imported files are written; projects that use the
/// skill are re-synced. `files` paths must already be validated as relative. `skills_root` is the
/// library folder ([`skills_dir`]).
pub(crate) async fn import_backup_skill(
    skills_root: &Path,
    db: &Db,
    id_hint: &str,
    name: &str,
    description: &str,
    target: &str,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<(crate::commands::settings_backup::Upsert, Vec<String>), String> {
    use crate::commands::settings_backup::Upsert;
    use sqlx::Row;
    validate_skill_name(name)?;
    validate_target(target)?;

    let local = sqlx::query("SELECT id, description, target, source_path FROM skills WHERE name = ? COLLATE NOCASE")
        .bind(name)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let dir = match &local {
        Some(row) => PathBuf::from(row.get::<String, _>("source_path")),
        None => skills_root.join(name),
    };

    let mut wanted: Vec<(PathBuf, Vec<u8>)> = files
        .iter()
        .filter(|(p, _)| p.file_name().is_some_and(|f| f != ".DS_Store"))
        .cloned()
        .collect();
    wanted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut current = read_skill_tree(&dir)?;
    current.sort_by(|a, b| a.0.cmp(&b.0));
    let same_files = current == wanted;
    if let Some(row) = &local {
        if same_files
            && row.get::<String, _>("description") == description
            && row.get::<String, _>("target") == target
        {
            return Ok((Upsert::Unchanged, Vec::new()));
        }
    }

    if !same_files {
        if dir.exists() {
            project_links::back_up_library_orphan(&dir, "skills")?;
        }
        for (rel, data) in &wanted {
            let out = dir.join(rel);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&out, data).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let source_path = dir.to_string_lossy().to_string();
    match local {
        Some(row) => {
            let id: String = row.get("id");
            sqlx::query("UPDATE skills SET description = ?, target = ?, updated_at = ? WHERE id = ?")
                .bind(description)
                .bind(target)
                .bind(&now)
                .bind(&id)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
            let failures = sync_skill_to_projects(db, &id).await.map_err(|e| e.to_string())?;
            Ok((Upsert::Updated, failures))
        }
        None => {
            let id_taken = sqlx::query("SELECT 1 FROM skills WHERE id = ?")
                .bind(id_hint)
                .fetch_optional(db)
                .await
                .map_err(|e| e.to_string())?
                .is_some();
            let id = if id_hint.is_empty() || id_taken { Uuid::new_v4().to_string() } else { id_hint.to_string() };
            sqlx::query("INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(name)
                .bind(description)
                .bind(target)
                .bind(&source_path)
                .bind(&now)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
            Ok((Upsert::Created, Vec::new()))
        }
    }
}

/// Sync a skill to every project that has it applied: (re)create links for the current name and
/// the engines each project has it switched on for. The skill's target is not applied here — it
/// is only the default for a project the skill is newly applied to. Content needs no copying —
/// projects link to the source. Returns one message per project that failed.
pub async fn sync_skill_to_projects(db: &Db, skill_id: &str) -> Result<Vec<String>> {
    use sqlx::Row;

    let skill = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(skill_id)
        .fetch_one(db)
        .await?;
    let name: String = skill.get("name");
    let source_path: String = skill.get("source_path");

    let rows = sqlx::query(
        "SELECT ps.project_id, ps.enabled_claude, ps.enabled_codex, ps.synced_hash_claude,
                ps.synced_hash_codex, p.path, p.name as project_name
         FROM project_skills ps
         JOIN projects p ON p.id = ps.project_id
         WHERE ps.skill_id = ?"
    )
    .bind(skill_id)
    .fetch_all(db)
    .await?;

    let mut failures = Vec::new();
    for row in rows {
        let project_id: String = row.get("project_id");
        let project_path: String = row.get("path");
        let project_name: String = row.get("project_name");

        let engines = SkillEngines::from_row(&row);
        let prev_claude: Option<String> = row.get("synced_hash_claude");
        let prev_codex: Option<String> = row.get("synced_hash_codex");
        let result = write_skill_artifacts(
            &project_path,
            &name,
            engines,
            &source_path,
            prev_claude.as_deref(),
            prev_codex.as_deref(),
        );
        let (next_claude, next_codex) = match result {
            Ok(hashes) => hashes,
            Err(e) => {
                failures.push(format!("{}: {}", project_name, e));
                continue;
            }
        };

        sqlx::query(
            "UPDATE project_skills SET synced_hash_claude = ?, synced_hash_codex = ? WHERE project_id = ? AND skill_id = ?"
        )
        .bind(&next_claude)
        .bind(&next_codex)
        .bind(&project_id)
        .bind(skill_id)
        .execute(db)
        .await?;
    }

    Ok(failures)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Skill `old` in `<dir>/lib`, linked into project `<dir>/p` and into `<dir>/home` for Claude.
    async fn fixture(label: &str) -> (PathBuf, Db) {
        let dir = std::env::temp_dir().join(format!("puppeteer-skills-{}-{}", label, Uuid::new_v4()));
        let source = dir.join("lib/old");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "---\nname: old\ndescription: d\n---\n").unwrap();
        for root in ["p", "home"] {
            let dest = dir.join(root).join(".claude/skills");
            fs::create_dir_all(&dest).unwrap();
            std::os::unix::fs::symlink(&source, dest.join("old")).unwrap();
        }
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p', 'p', ?, 'c')")
            .bind(dir.join("p").to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES ('s', 'old', 'd', 'claude', ?, 'c')",
        )
        .bind(source.to_string_lossy().to_string())
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO project_skills (project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude, applied_at, manual)
             VALUES ('p', 's', 1, 0, 'h', 'c', 1)",
        )
        .execute(&db)
        .await
        .unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn a_failed_rename_keeps_every_link() {
        let (dir, db) = fixture("rename-fails").await;
        let old = dir.join("lib/old").to_string_lossy().to_string();
        let home = dir.join("home");
        // No such parent folder: the rename itself fails.
        let err = rename_skill_dir(&db, Some(&home), "s", "old", &old, &dir.join("missing/new")).await;
        assert!(err.is_err());
        for root in ["p", "home"] {
            let link = dir.join(root).join(".claude/skills/old");
            assert_eq!(fs::read_link(&link).unwrap(), Path::new(&old), "{root}");
        }

        let (relink, errors) = rename_skill_dir(&db, Some(&home), "s", "old", &old, &dir.join("lib/new")).await.unwrap();
        assert_eq!((relink, errors), (vec!["claude"], Vec::<String>::new()));
        assert!(dir.join("lib/new/SKILL.md").exists());
        for root in ["p", "home"] {
            assert!(fs::symlink_metadata(dir.join(root).join(".claude/skills/old")).is_err(), "{root}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
