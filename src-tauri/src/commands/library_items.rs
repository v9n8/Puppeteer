//! The Agents and Commands libraries: Claude Code subagents (`.claude/agents/<name>.md`) and slash
//! commands (`.claude/commands/<name>.md`), kept the way skills are.
//!
//! Each item is one markdown file in Puppeteer's library (`<app data>/agents/`,
//! `<app data>/commands/`). A project gets it as a symlink back to that file (`project_links`), so
//! editing the source updates every project. Claude only: Codex has no per-project folder for
//! either (its custom prompts live in `~/.codex/prompts` alone).
//!
//! Puppeteer never wrote copies of these, so a real file of the same name in a project is the
//! user's own: applying over it is refused as a conflict (skills move an unknown copy to the
//! backups instead, since the copy era left Puppeteer's own copies there), and removing only ever
//! deletes Puppeteer's link. A file git tracks is the repo's own and is left alone.
//!
//! Every command takes the kind and is refused while that kind's prop is off.

use crate::commands::project_links::{self, AtDest, Linked};
use crate::commands::rules::content_hash;
use crate::db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Agent,
    Command,
}

impl LibraryKind {
    /// The prop gating this kind's commands.
    pub fn feature(self) -> &'static str {
        match self {
            LibraryKind::Agent => "library.agents",
            LibraryKind::Command => "library.commands",
        }
    }

    /// The library table; also the folder name under app data and under `.claude/`.
    fn table(self) -> &'static str {
        match self {
            LibraryKind::Agent => "agents",
            LibraryKind::Command => "commands",
        }
    }

    fn link_table(self) -> &'static str {
        match self {
            LibraryKind::Agent => "project_agents",
            LibraryKind::Command => "project_commands",
        }
    }

    fn id_column(self) -> &'static str {
        match self {
            LibraryKind::Agent => "agent_id",
            LibraryKind::Command => "command_id",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LibraryKind::Agent => "Agent",
            LibraryKind::Command => "Command",
        }
    }
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct LibraryItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_path: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct LibraryItemContent {
    pub item: LibraryItem,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateItemPayload {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItemPayload {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
}

/// What a project holds where an applied item should be linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// The project links to the library.
    Installed,
    /// Something else is there: the user's own file, a link elsewhere, or nothing at all.
    Drifted,
    /// A file git tracks: the repo's own, never touched.
    GitManaged,
}

#[derive(Debug, Serialize)]
pub struct AppliedItem {
    pub item_id: String,
    pub name: String,
    pub description: String,
    pub applied_at: String,
    pub state: LinkState,
}

/// Where Claude reads an item in a project: `<project>/.claude/<agents|commands>/<name>.md`.
pub fn project_dest(kind: LibraryKind, project_path: &str, name: &str) -> PathBuf {
    Path::new(project_path).join(".claude").join(kind.table()).join(format!("{name}.md"))
}

pub(crate) fn library_dir(app: &AppHandle, kind: LibraryKind) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join(kind.table()))
}

fn validate(kind: LibraryKind, name: &str, description: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} name cannot be empty", kind.label()));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!(
            "{} name must only contain letters, numbers, hyphens, and underscores",
            kind.label()
        ));
    }
    if description.is_empty() {
        return Err("Description is required".to_string());
    }
    Ok(())
}

/// Link `source` into the project at `dest`. Returns the source's hash, or `None` where git tracks a
/// file of that name (the repo's own: left alone and not recorded as installed).
fn link_into(project_path: &str, dest: &Path, source: &Path) -> Result<Option<String>, String> {
    let root = Path::new(project_path);
    let real = fs::symlink_metadata(dest).is_ok_and(|m| !m.file_type().is_symlink());
    if real && project_links::at_dest(root, dest, source) != AtDest::GitTracked {
        return Err(format!(
            "{} already exists and was not made by Puppeteer — rename or remove it, then apply again",
            dest.display()
        ));
    }
    let hash = content_hash(&fs::read_to_string(source).unwrap_or_default());
    match project_links::ensure_symlink(root, dest, source, project_links::no_copies)? {
        Linked::Yes => Ok(Some(hash)),
        Linked::TrackedByGit => Ok(None),
    }
}

fn unlink_from(project_path: &str, dest: &Path, source: &Path) -> Result<(), String> {
    project_links::remove_own_link(Path::new(project_path), dest, source)
}

fn row_to_item(row: &sqlx::sqlite::SqliteRow) -> LibraryItem {
    LibraryItem {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        source_path: row.get("source_path"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn list_items(db: &Db, kind: LibraryKind) -> Result<Vec<LibraryItem>, String> {
    let sql = format!("SELECT id, name, description, source_path, updated_at FROM {} ORDER BY name", kind.table());
    let rows = sqlx::query(&sql).fetch_all(db).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(row_to_item).collect())
}

async fn find_item(db: &Db, kind: LibraryKind, id: &str) -> Result<LibraryItem, String> {
    let sql = format!("SELECT id, name, description, source_path, updated_at FROM {} WHERE id = ?", kind.table());
    sqlx::query(&sql)
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .map(|row| row_to_item(&row))
        .ok_or_else(|| format!("{} not found", kind.label()))
}

async fn project_path(db: &Db, project_id: &str) -> Result<String, String> {
    sqlx::query_scalar("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())
}

/// (id, name, path) of every project the item is applied to.
async fn applied_projects(db: &Db, kind: LibraryKind, item_id: &str) -> Result<Vec<(String, String, String)>, String> {
    let sql = format!(
        "SELECT p.id, p.name, p.path FROM {} l JOIN projects p ON p.id = l.project_id WHERE l.{} = ? ORDER BY p.name",
        kind.link_table(),
        kind.id_column()
    );
    let rows = sqlx::query(&sql).bind(item_id).fetch_all(db).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| (r.get("id"), r.get("name"), r.get("path"))).collect())
}

/// Remove the item's link from every project that has it, one message per project that failed.
async fn unlink_everywhere(db: &Db, kind: LibraryKind, item: &LibraryItem) -> Result<Vec<String>, String> {
    let source = Path::new(&item.source_path);
    Ok(applied_projects(db, kind, &item.id)
        .await?
        .iter()
        .filter_map(|(_, name, path)| {
            unlink_from(path, &project_dest(kind, path, &item.name), source).err().map(|e| format!("{name}: {e}"))
        })
        .collect())
}

pub async fn create_item(
    db: &Db,
    kind: LibraryKind,
    lib_dir: &Path,
    payload: CreateItemPayload,
) -> Result<LibraryItem, String> {
    validate(kind, &payload.name, &payload.description)?;
    let taken_sql = format!("SELECT 1 FROM {} WHERE name = ?", kind.table());
    let taken: Option<i64> =
        sqlx::query_scalar(&taken_sql).bind(&payload.name).fetch_optional(db).await.map_err(|e| e.to_string())?;
    if taken.is_some() {
        return Err(format!("{} '{}' already exists", kind.label(), payload.name));
    }

    fs::create_dir_all(lib_dir).map_err(|e| e.to_string())?;
    let source = lib_dir.join(format!("{}.md", payload.name));
    fs::write(&source, &payload.content).map_err(|e| e.to_string())?;

    let item = LibraryItem {
        id: Uuid::new_v4().to_string(),
        name: payload.name,
        description: payload.description,
        source_path: source.to_string_lossy().to_string(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let sql = format!(
        "INSERT INTO {} (id, name, description, source_path, updated_at) VALUES (?, ?, ?, ?, ?)",
        kind.table()
    );
    sqlx::query(&sql)
        .bind(&item.id)
        .bind(&item.name)
        .bind(&item.description)
        .bind(&item.source_path)
        .bind(&item.updated_at)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(item)
}

pub async fn update_item(
    db: &Db,
    kind: LibraryKind,
    lib_dir: &Path,
    payload: UpdateItemPayload,
) -> Result<LibraryItem, String> {
    validate(kind, &payload.name, &payload.description)?;
    let old = find_item(db, kind, &payload.id).await?;
    let renamed = old.name != payload.name;
    let mut source_path = PathBuf::from(&old.source_path);

    if renamed {
        let taken_sql = format!("SELECT 1 FROM {} WHERE name = ? AND id != ?", kind.table());
        let taken: Option<i64> = sqlx::query_scalar(&taken_sql)
            .bind(&payload.name)
            .bind(&payload.id)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
        if taken.is_some() {
            return Err(format!("{} '{}' already exists", kind.label(), payload.name));
        }
        let new_path = lib_dir.join(format!("{}.md", payload.name));
        if new_path.exists() && !project_links::same_file(&new_path, &source_path) {
            // Not in the DB: a leftover from an old rename/delete. Move it aside, don't overwrite.
            project_links::back_up_library_orphan(&new_path, kind.table())?;
        }
        // The old links would dangle once the source moves.
        let errors = unlink_everywhere(db, kind, &old).await?;
        if !errors.is_empty() {
            return Err(format!("Not renamed — clean these up first:\n{}", errors.join("\n")));
        }
        fs::rename(&source_path, &new_path).map_err(|e| e.to_string())?;
        source_path = new_path;
    }
    fs::write(&source_path, &payload.content).map_err(|e| e.to_string())?;

    let item = LibraryItem {
        id: payload.id,
        name: payload.name,
        description: payload.description,
        source_path: source_path.to_string_lossy().to_string(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let sql = format!(
        "UPDATE {} SET name = ?, description = ?, source_path = ?, updated_at = ? WHERE id = ?",
        kind.table()
    );
    sqlx::query(&sql)
        .bind(&item.name)
        .bind(&item.description)
        .bind(&item.source_path)
        .bind(&item.updated_at)
        .bind(&item.id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;

    // Projects link to the source, so content is already live; this re-links after a rename and
    // surfaces a link that broke, so it is never silent.
    let failures = sync_to_projects(db, kind, &item.id).await?;
    if !failures.is_empty() {
        return Err(format!("Saved, but syncing to projects failed:\n{}", failures.join("\n")));
    }
    Ok(item)
}

pub async fn delete_item(db: &Db, kind: LibraryKind, id: &str) -> Result<(), String> {
    let item = find_item(db, kind, id).await?;
    // Drop the project links first: they would dangle once the source is gone.
    let errors = unlink_everywhere(db, kind, &item).await?;
    if !errors.is_empty() {
        return Err(format!("Not deleted — clean these up first:\n{}", errors.join("\n")));
    }
    let _ = fs::remove_file(&item.source_path);
    sqlx::query(&format!("DELETE FROM {} WHERE id = ?", kind.table()))
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Link an item into every project that has it, recording what was installed. Returns one message
/// per project that failed.
pub async fn sync_to_projects(db: &Db, kind: LibraryKind, item_id: &str) -> Result<Vec<String>, String> {
    let item = find_item(db, kind, item_id).await?;
    let source = Path::new(&item.source_path);
    let update = format!(
        "UPDATE {} SET synced_hash_claude = ? WHERE project_id = ? AND {} = ?",
        kind.link_table(),
        kind.id_column()
    );
    let mut failures = Vec::new();
    for (project_id, name, path) in applied_projects(db, kind, item_id).await? {
        match link_into(&path, &project_dest(kind, &path, &item.name), source) {
            Ok(hash) => {
                sqlx::query(&update)
                    .bind(&hash)
                    .bind(&project_id)
                    .bind(item_id)
                    .execute(db)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    Ok(failures)
}

pub async fn apply_item(db: &Db, kind: LibraryKind, project_id: &str, item_id: &str) -> Result<(), String> {
    let item = find_item(db, kind, item_id).await?;
    let path = project_path(db, project_id).await?;
    let hash = link_into(&path, &project_dest(kind, &path, &item.name), Path::new(&item.source_path))?;
    let sql = format!(
        "INSERT INTO {table} (project_id, {col}, synced_hash_claude, applied_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(project_id, {col}) DO UPDATE SET
            synced_hash_claude = excluded.synced_hash_claude,
            applied_at = excluded.applied_at",
        table = kind.link_table(),
        col = kind.id_column()
    );
    sqlx::query(&sql)
        .bind(project_id)
        .bind(item_id)
        .bind(&hash)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn remove_item_from_project(
    db: &Db,
    kind: LibraryKind,
    project_id: &str,
    item_id: &str,
) -> Result<(), String> {
    let item = find_item(db, kind, item_id).await?;
    let path = project_path(db, project_id).await?;
    unlink_from(&path, &project_dest(kind, &path, &item.name), Path::new(&item.source_path))?;
    let sql = format!("DELETE FROM {} WHERE project_id = ? AND {} = ?", kind.link_table(), kind.id_column());
    sqlx::query(&sql)
        .bind(project_id)
        .bind(item_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The items applied to a project, each with what the project holds at its link.
pub async fn applied_items(db: &Db, kind: LibraryKind, project_id: &str) -> Result<Vec<AppliedItem>, String> {
    let path = project_path(db, project_id).await?;
    let sql = format!(
        "SELECT i.id, i.name, i.description, i.source_path, l.applied_at
         FROM {link} l JOIN {table} i ON i.id = l.{col}
         WHERE l.project_id = ? ORDER BY i.name",
        link = kind.link_table(),
        table = kind.table(),
        col = kind.id_column()
    );
    let rows = sqlx::query(&sql).bind(project_id).fetch_all(db).await.map_err(|e| e.to_string())?;
    let rows: Vec<(LibraryItem, String)> = rows
        .iter()
        .map(|r| {
            let item = LibraryItem {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                source_path: r.get("source_path"),
                updated_at: String::new(),
            };
            (item, r.get("applied_at"))
        })
        .collect();

    // One lstat per item, plus a git call where a real file sits: off the async runtime.
    tokio::task::spawn_blocking(move || {
        let root = Path::new(&path);
        rows.into_iter()
            .map(|(item, applied_at)| {
                let dest = project_dest(kind, &path, &item.name);
                let state = match project_links::at_dest(root, &dest, Path::new(&item.source_path)) {
                    AtDest::OwnLink => LinkState::Installed,
                    AtDest::GitTracked => LinkState::GitManaged,
                    AtDest::Nothing | AtDest::Other => LinkState::Drifted,
                };
                AppliedItem { item_id: item.id, name: item.name, description: item.description, applied_at, state }
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

// ---- Tauri commands ----------------------------------------------------------

#[tauri::command]
pub async fn list_library_items(db: State<'_, Db>, kind: LibraryKind) -> Result<Vec<LibraryItem>, String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    list_items(db.inner(), kind).await
}

#[tauri::command]
pub async fn get_library_item(db: State<'_, Db>, kind: LibraryKind, id: String) -> Result<LibraryItemContent, String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    let item = find_item(db.inner(), kind, &id).await?;
    let content = fs::read_to_string(&item.source_path).unwrap_or_default();
    Ok(LibraryItemContent { item, content })
}

#[tauri::command]
pub async fn create_library_item(
    app: AppHandle,
    db: State<'_, Db>,
    kind: LibraryKind,
    payload: CreateItemPayload,
) -> Result<LibraryItem, String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    create_item(db.inner(), kind, &library_dir(&app, kind)?, payload).await
}

#[tauri::command]
pub async fn update_library_item(
    app: AppHandle,
    db: State<'_, Db>,
    kind: LibraryKind,
    payload: UpdateItemPayload,
) -> Result<LibraryItem, String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    update_item(db.inner(), kind, &library_dir(&app, kind)?, payload).await
}

#[tauri::command]
pub async fn delete_library_item(db: State<'_, Db>, kind: LibraryKind, id: String) -> Result<(), String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    delete_item(db.inner(), kind, &id).await
}

#[tauri::command]
pub async fn get_applied_library_items(
    db: State<'_, Db>,
    kind: LibraryKind,
    project_id: String,
) -> Result<Vec<AppliedItem>, String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    applied_items(db.inner(), kind, &project_id).await
}

#[tauri::command]
pub async fn apply_library_item(
    db: State<'_, Db>,
    kind: LibraryKind,
    project_id: String,
    item_id: String,
) -> Result<(), String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    apply_item(db.inner(), kind, &project_id, &item_id).await
}

#[tauri::command]
pub async fn remove_library_item_from_project(
    db: State<'_, Db>,
    kind: LibraryKind,
    project_id: String,
    item_id: String,
) -> Result<(), String> {
    crate::features::require(db.inner(), kind.feature()).await?;
    remove_item_from_project(db.inner(), kind, &project_id, &item_id).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::commands::project_links::is_symlink;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::process::Command;
    use LibraryKind::{Agent, Command as Cmd};

    struct Fixture {
        db: Db,
        project: PathBuf,
        lib: PathBuf,
    }

    impl Fixture {
        /// In-memory DB with real migrations, project `p1` (a git repo) at `<tmp>/proj` and the
        /// libraries under `<tmp>/lib`.
        async fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-items-{}-{}", label, Uuid::new_v4()));
            let project = dir.join("proj");
            fs::create_dir_all(&project).unwrap();
            assert!(Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap().success());
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', ?, '2026-01-01')")
                .bind(project.to_string_lossy().to_string())
                .execute(&db)
                .await
                .unwrap();
            Fixture { db, project, lib: dir.join("lib") }
        }

        fn lib(&self, kind: LibraryKind) -> PathBuf {
            self.lib.join(kind.table())
        }

        async fn create(&self, kind: LibraryKind, name: &str, content: &str) -> LibraryItem {
            let payload =
                CreateItemPayload { name: name.into(), description: "d".into(), content: content.into() };
            create_item(&self.db, kind, &self.lib(kind), payload).await.unwrap()
        }

        async fn update(&self, kind: LibraryKind, id: &str, name: &str, content: &str) -> Result<LibraryItem, String> {
            let payload =
                UpdateItemPayload { id: id.into(), name: name.into(), description: "d".into(), content: content.into() };
            update_item(&self.db, kind, &self.lib(kind), payload).await
        }

        async fn states(&self, kind: LibraryKind) -> Vec<(String, LinkState)> {
            applied_items(&self.db, kind, "p1").await.unwrap().into_iter().map(|a| (a.name, a.state)).collect()
        }

        async fn hash(&self, kind: LibraryKind, item_id: &str) -> Option<Option<String>> {
            let sql = format!(
                "SELECT synced_hash_claude FROM {} WHERE project_id = 'p1' AND {} = ?",
                kind.link_table(),
                kind.id_column()
            );
            sqlx::query_scalar(&sql).bind(item_id).fetch_optional(&self.db).await.unwrap()
        }
    }

    #[tokio::test]
    async fn crud_keeps_one_file_per_item_and_each_kind_apart() {
        let f = Fixture::new("crud").await;
        let agent = f.create(Agent, "reviewer", "---\nname: reviewer\n---\nReview.").await;
        assert_eq!(fs::read_to_string(&agent.source_path).unwrap(), "---\nname: reviewer\n---\nReview.");
        assert_eq!(Path::new(&agent.source_path), f.lib(Agent).join("reviewer.md"));
        // Same name in the other kind is a different item.
        let command = f.create(Cmd, "reviewer", "Review $ARGUMENTS").await;
        assert_eq!(Path::new(&command.source_path), f.lib(Cmd).join("reviewer.md"));

        let dup = CreateItemPayload { name: "reviewer".into(), description: "d".into(), content: String::new() };
        let err = create_item(&f.db, Agent, &f.lib(Agent), dup).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        for (name, description) in [("../x", "d"), ("", "d"), ("ok", "")] {
            let bad = CreateItemPayload { name: name.into(), description: description.into(), content: String::new() };
            assert!(create_item(&f.db, Agent, &f.lib(Agent), bad).await.is_err(), "{name:?} / {description:?}");
        }

        let renamed = f.update(Agent, &agent.id, "critic", "Criticise.").await.unwrap();
        assert!(!Path::new(&agent.source_path).exists());
        assert_eq!(fs::read_to_string(&renamed.source_path).unwrap(), "Criticise.");
        let names: Vec<String> = list_items(&f.db, Agent).await.unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["critic"]);
        assert!(f.update(Agent, &agent.id, "reviewer", "x").await.is_ok(), "the command's name is free for agents");

        delete_item(&f.db, Agent, &agent.id).await.unwrap();
        assert!(list_items(&f.db, Agent).await.unwrap().is_empty());
        assert!(!f.lib(Agent).join("reviewer.md").exists());
        assert_eq!(list_items(&f.db, Cmd).await.unwrap(), vec![command]);
    }

    #[tokio::test]
    async fn apply_links_into_claude_and_remove_unlinks() {
        let f = Fixture::new("apply").await;
        for (kind, folder) in [(Agent, ".claude/agents"), (Cmd, ".claude/commands")] {
            let item = f.create(kind, "fix", "body").await;
            apply_item(&f.db, kind, "p1", &item.id).await.unwrap();
            apply_item(&f.db, kind, "p1", &item.id).await.unwrap();

            let dest = f.project.join(folder).join("fix.md");
            assert_eq!(fs::read_link(&dest).unwrap(), PathBuf::from(&item.source_path));
            assert_eq!(f.hash(kind, &item.id).await, Some(Some(content_hash("body"))));
            assert_eq!(f.states(kind).await, vec![("fix".to_string(), LinkState::Installed)]);

            remove_item_from_project(&f.db, kind, "p1", &item.id).await.unwrap();
            assert!(!is_symlink(&dest));
            assert!(Path::new(&item.source_path).exists(), "the source stays");
            assert_eq!(f.hash(kind, &item.id).await, None);
        }
    }

    #[tokio::test]
    async fn a_file_of_the_same_name_is_a_conflict_and_never_touched() {
        let f = Fixture::new("conflict").await;
        let item = f.create(Agent, "fix", "library").await;
        let dest = f.project.join(".claude/agents/fix.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();

        // The user's own file: refused, kept, nothing recorded.
        fs::write(&dest, "mine").unwrap();
        let err = apply_item(&f.db, Agent, "p1", &item.id).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(fs::read_to_string(&dest).unwrap(), "mine");
        assert_eq!(f.hash(Agent, &item.id).await, None);

        // A link to something else: refused too.
        fs::remove_file(&dest).unwrap();
        let other = f.project.join("other.md");
        fs::write(&other, "other").unwrap();
        std::os::unix::fs::symlink(&other, &dest).unwrap();
        assert!(apply_item(&f.db, Agent, "p1", &item.id).await.is_err());
        assert_eq!(fs::read_link(&dest).unwrap(), other);
        fs::remove_file(&dest).unwrap();

        // Applied, then replaced by hand with a real file: drifted, and removal leaves it.
        apply_item(&f.db, Agent, "p1", &item.id).await.unwrap();
        fs::remove_file(&dest).unwrap();
        fs::write(&dest, "mine again").unwrap();
        assert_eq!(f.states(Agent).await, vec![("fix".to_string(), LinkState::Drifted)]);
        remove_item_from_project(&f.db, Agent, "p1", &item.id).await.unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "mine again");
    }

    #[tokio::test]
    async fn a_file_git_tracks_is_the_repos_own() {
        let f = Fixture::new("tracked").await;
        let item = f.create(Cmd, "deploy", "library").await;
        let dest = f.project.join(".claude/commands/deploy.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "team version").unwrap();
        assert!(Command::new("git").arg("-C").arg(&f.project).args(["add", ".claude"]).status().unwrap().success());

        apply_item(&f.db, Cmd, "p1", &item.id).await.unwrap();
        assert_eq!(f.hash(Cmd, &item.id).await, Some(None), "applied, not installed");
        assert_eq!(f.states(Cmd).await, vec![("deploy".to_string(), LinkState::GitManaged)]);
        remove_item_from_project(&f.db, Cmd, "p1", &item.id).await.unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "team version");
    }

    #[tokio::test]
    async fn rename_and_delete_follow_into_projects() {
        let f = Fixture::new("rename").await;
        let item = f.create(Agent, "old", "v1").await;
        apply_item(&f.db, Agent, "p1", &item.id).await.unwrap();
        let old_dest = f.project.join(".claude/agents/old.md");

        let renamed = f.update(Agent, &item.id, "new", "v2").await.unwrap();
        let new_dest = f.project.join(".claude/agents/new.md");
        assert!(!is_symlink(&old_dest));
        assert_eq!(fs::read_link(&new_dest).unwrap(), PathBuf::from(&renamed.source_path));
        assert_eq!(fs::read_to_string(&new_dest).unwrap(), "v2");
        assert_eq!(f.hash(Agent, &item.id).await, Some(Some(content_hash("v2"))));

        // A rename onto the user's own file saves, but says the project could not be linked.
        let mine = f.project.join(".claude/agents/taken.md");
        fs::write(&mine, "mine").unwrap();
        let err = f.update(Agent, &item.id, "taken", "v3").await.unwrap_err();
        assert!(err.contains("syncing to projects failed") && err.contains("proj"), "{err}");
        assert_eq!(fs::read_to_string(&mine).unwrap(), "mine");
        fs::remove_file(&mine).unwrap();
        assert_eq!(f.states(Agent).await, vec![("taken".to_string(), LinkState::Drifted)]);
        assert!(sync_to_projects(&f.db, Agent, &item.id).await.unwrap().is_empty());

        delete_item(&f.db, Agent, &item.id).await.unwrap();
        assert!(!is_symlink(&mine));
        assert_eq!(f.hash(Agent, &item.id).await, None, "the project row cascades");
    }
}
