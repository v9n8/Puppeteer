//! Memory: the instruction files the CLIs read on their own at the start of every session — the
//! user's `~/.claude/CLAUDE.md` (Claude Code) and Codex's `AGENTS.md` in its home (Settings › AI ›
//! Memory), a project's `CLAUDE.md` and `AGENTS.md` (Library › By project). No store of its own:
//! the file on disk is the memory.
//!
//! The frontend names a target, never a path; the path is resolved here. A file that is a link is
//! followed only where the import rule (R-03) lets it lead: a project's file must stay in the
//! project, a user's file may link into a dotfiles folder but not into a credential store or the
//! app's data. Writes are atomic (temp file + rename, keeping the file's permissions) and refused
//! when the file changed on disk since it was read, so a stale buffer can't drop what another
//! editor — or a Rules / Skills sync, which keeps managed blocks in `AGENTS.md` — wrote meanwhile.
//! Both commands are local-only (remote/guard.rs): the content goes into every session.

use crate::commands::{agents_block, instruction_checks};
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tauri::{Manager, State};

/// Error prefix of a write refused because the file changed on disk; the UI offers reload /
/// overwrite on it.
pub const CONFLICT: &str = "memory_conflict";

/// Larger than any instruction file worth keeping in context; bigger files aren't loaded into
/// the editor.
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryTarget {
    User,
    /// Codex's global `AGENTS.md`, in its home (`$CODEX_HOME`, else `~/.codex`).
    CodexUser,
    ProjectClaude { project_id: String },
    ProjectAgents { project_id: String },
}

#[derive(Debug, Serialize)]
pub struct MemoryFile {
    /// Where the CLI reads it.
    pub path: String,
    /// Where it really is, when `path` is a link.
    pub link_target: Option<String>,
    pub exists: bool,
    pub content: String,
    pub size: u64,
    /// SHA-256 of the bytes read; passed back to [`memory_write`] to detect outside changes.
    pub hash: Option<String>,
    /// `kind:name` of the blocks Rules / Skills keep in this file (`agents_block`).
    pub managed_blocks: Vec<String>,
}

/// Where the target's file may lead.
enum Scope<'a> {
    /// A user's file: anywhere but a credential store or the app's data.
    Home { home: &'a Path, app_data: Option<&'a Path> },
    /// A project's file: inside the project.
    Project { root: &'a Path },
}

/// The file the CLI reads, and the real file behind it.
#[derive(Debug)]
struct Located {
    shown: PathBuf,
    real: PathBuf,
    link: bool,
}

/// The target's file, `home` / `codex_home` / `project_root` already resolved.
fn target_file(
    target: &MemoryTarget,
    home: &Path,
    codex_home: &Path,
    project_root: Option<&Path>,
) -> Result<PathBuf, String> {
    match (target, project_root) {
        (MemoryTarget::User, _) => Ok(home.join(".claude").join("CLAUDE.md")),
        (MemoryTarget::CodexUser, _) => Ok(codex_home.join("AGENTS.md")),
        (MemoryTarget::ProjectClaude { .. }, Some(root)) => Ok(root.join("CLAUDE.md")),
        (MemoryTarget::ProjectAgents { .. }, Some(root)) => Ok(root.join("AGENTS.md")),
        _ => Err("Project not found".to_string()),
    }
}

/// Resolve `file` (a link followed) and refuse where `scope` doesn't let it lead. A missing file
/// resolves to its name in the real parent folder, which must exist for a project (the project
/// root) and is created for a user's file (`~/.claude`, `~/.codex`) on write.
fn locate(file: &Path, scope: &Scope) -> Result<Located, String> {
    let name = file.file_name().ok_or("no file name")?;
    let parent = file.parent().ok_or("no parent folder")?;
    let (real, link) = match fs::symlink_metadata(file) {
        Ok(meta) => {
            let real = fs::canonicalize(file).map_err(|_| {
                format!("{} is a link to a file that doesn't exist — fix or remove the link first", file.display())
            })?;
            if !fs::metadata(&real).is_ok_and(|m| m.is_file()) {
                return Err(format!("{} is not a file", file.display()));
            }
            (real, meta.file_type().is_symlink())
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let real_parent = match fs::canonicalize(parent) {
                Ok(p) => p,
                Err(_) if matches!(scope, Scope::Home { .. }) => parent.to_path_buf(),
                Err(_) => return Err(format!("Project folder not found: {}", parent.display())),
            };
            (real_parent.join(name), false)
        }
        Err(e) => return Err(format!("{}: {}", file.display(), e)),
    };
    match scope {
        Scope::Project { root } => {
            let base = fs::canonicalize(root).map_err(|_| format!("Project folder not found: {}", root.display()))?;
            if !real.starts_with(&base) {
                return Err(format!("{} links outside the project — Puppeteer won't open it", file.display()));
            }
        }
        Scope::Home { home, app_data } => {
            // Only where a link leads is checked: the file at its own place is the CLI's, and
            // `~/.codex` is a credential store (`auth.json`) although its `AGENTS.md` isn't.
            if link && SecretPaths::new(Some(home), *app_data).denies(&real) {
                return Err(format!(
                    "{} links into a credential store or Puppeteer's data — Puppeteer won't open it",
                    file.display()
                ));
            }
        }
    }
    Ok(Located { shown: file.to_path_buf(), real, link })
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The bytes at `real`, or None when there is no file.
fn read_bytes(real: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::metadata(real) {
        Ok(meta) if meta.len() > MAX_BYTES => {
            Err(format!("{} is too large to edit here ({} bytes, limit {MAX_BYTES})", real.display(), meta.len()))
        }
        Ok(_) => fs::read(real).map(Some).map_err(|e| format!("{}: {}", real.display(), e)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {}", real.display(), e)),
    }
}

fn read_at(loc: &Located) -> Result<MemoryFile, String> {
    let bytes = read_bytes(&loc.real)?;
    let content = match &bytes {
        // Not UTF-8 (a Shift_JIS file): a save from the editor would rewrite it, so don't open it.
        Some(b) => String::from_utf8(b.clone()).map_err(|_| format!("{} is not UTF-8 text", loc.shown.display()))?,
        None => String::new(),
    };
    Ok(MemoryFile {
        path: loc.shown.display().to_string(),
        link_target: loc.link.then(|| loc.real.display().to_string()),
        exists: bytes.is_some(),
        size: bytes.as_ref().map_or(0, |b| b.len() as u64),
        hash: bytes.as_deref().map(hash),
        managed_blocks: agents_block::block_names(&content),
        content,
    })
}

/// Write `content` unless the file is no longer what was read (`expected_hash`: None = it didn't
/// exist).
fn write_at(loc: &Located, content: &str, expected_hash: Option<&str>) -> Result<MemoryFile, String> {
    if content.len() as u64 > MAX_BYTES {
        return Err(format!("Content is too large ({} bytes, limit {MAX_BYTES})", content.len()));
    }
    let current = read_bytes(&loc.real)?;
    if current.as_deref().map(hash).as_deref() != expected_hash {
        return Err(format!("{CONFLICT}: {} changed on disk since it was opened", loc.shown.display()));
    }
    write_atomic(&loc.real, content.as_bytes()).map_err(|e| format!("{}: {}", loc.shown.display(), e))?;
    read_at(loc)
}

/// Temp file next to `path`, then rename over it: a CLI starting meanwhile reads the old file or
/// the new one, never half of it. The temp file takes the old file's permissions.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "no parent folder"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("memory");
    let tmp = dir.join(format!(".{name}.puppeteer-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        // `create_new`: never through a link planted at the temp name.
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// A live project's folder.
pub(crate) async fn project_root(db: &Db, project_id: &str) -> Result<PathBuf, String> {
    sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ? AND deleted_at IS NULL")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .map(PathBuf::from)
        .ok_or_else(|| "Project not found".to_string())
}

async fn resolve(
    db: &Db,
    target: &MemoryTarget,
    home: &Path,
    codex_home: &Path,
    app_data: Option<&Path>,
) -> Result<Located, String> {
    let root = match target {
        MemoryTarget::User | MemoryTarget::CodexUser => None,
        MemoryTarget::ProjectClaude { project_id } | MemoryTarget::ProjectAgents { project_id } => {
            Some(project_root(db, project_id).await?)
        }
    };
    let file = target_file(target, home, codex_home, root.as_deref())?;
    let scope = match &root {
        Some(root) => Scope::Project { root },
        None => Scope::Home { home, app_data },
    };
    locate(&file, &scope)
}

/// HOME, Codex's global home (the one Codex accounts share their `AGENTS.md` from), app data.
fn dirs(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let codex_home = instruction_checks::codex_home().unwrap_or_else(|| home.join(".codex"));
    Ok((home, codex_home, app.path().app_data_dir().ok()))
}

#[tauri::command]
pub async fn memory_read(app: tauri::AppHandle, db: State<'_, Db>, target: MemoryTarget) -> Result<MemoryFile, String> {
    let (home, codex_home, app_data) = dirs(&app)?;
    let loc = resolve(db.inner(), &target, &home, &codex_home, app_data.as_deref()).await?;
    read_at(&loc)
}

#[tauri::command]
pub async fn memory_write(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    target: MemoryTarget,
    content: String,
    expected_hash: Option<String>,
) -> Result<MemoryFile, String> {
    let (home, codex_home, app_data) = dirs(&app)?;
    let loc = resolve(db.inner(), &target, &home, &codex_home, app_data.as_deref()).await?;
    write_at(&loc, &content, expected_hash.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-memory-{}-{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(dir).unwrap()
    }

    fn project(id: &str) -> MemoryTarget {
        MemoryTarget::ProjectClaude { project_id: id.into() }
    }

    #[test]
    fn each_target_has_its_own_file() {
        let (home, codex, root) = (Path::new("/h"), Path::new("/c"), Path::new("/p"));
        assert_eq!(target_file(&MemoryTarget::User, home, codex, None).unwrap(), Path::new("/h/.claude/CLAUDE.md"));
        assert_eq!(target_file(&MemoryTarget::CodexUser, home, codex, None).unwrap(), Path::new("/c/AGENTS.md"));
        assert_eq!(target_file(&project("p"), home, codex, Some(root)).unwrap(), Path::new("/p/CLAUDE.md"));
        let agents = MemoryTarget::ProjectAgents { project_id: "p".into() };
        assert_eq!(target_file(&agents, home, codex, Some(root)).unwrap(), Path::new("/p/AGENTS.md"));
        assert!(target_file(&project("p"), home, codex, None).is_err());
    }

    #[test]
    fn target_comes_from_a_kind_not_a_path() {
        let t: MemoryTarget = serde_json::from_str(r#"{"kind":"project_agents","project_id":"x"}"#).unwrap();
        assert!(matches!(t, MemoryTarget::ProjectAgents { ref project_id } if project_id == "x"));
        let t: MemoryTarget = serde_json::from_str(r#"{"kind":"codex_user"}"#).unwrap();
        assert!(matches!(t, MemoryTarget::CodexUser));
        assert!(serde_json::from_str::<MemoryTarget>(r#"{"kind":"path","path":"/etc/hosts"}"#).is_err());
    }

    #[tokio::test]
    async fn project_targets_resolve_from_live_project_rows_only() {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let (live, gone) = (tmp("live"), tmp("gone"));
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('a', 'a', ?, 'c'), ('b', 'b', ?, 'c')")
            .bind(live.display().to_string())
            .bind(gone.display().to_string())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE projects SET deleted_at = 'd' WHERE id = 'b'").execute(&db).await.unwrap();
        let home = tmp("home");
        let codex = home.join(".codex");

        let loc = resolve(&db, &project("a"), &home, &codex, None).await.unwrap();
        assert_eq!(loc.real, live.join("CLAUDE.md"));
        assert!(resolve(&db, &project("b"), &home, &codex, None).await.is_err(), "trashed project");
        assert!(resolve(&db, &project("nope"), &home, &codex, None).await.is_err());
        let loc = resolve(&db, &MemoryTarget::User, &home, &codex, None).await.unwrap();
        assert_eq!(loc.real, home.join(".claude/CLAUDE.md"));
        let loc = resolve(&db, &MemoryTarget::CodexUser, &home, &codex, None).await.unwrap();
        assert_eq!(loc.real, codex.join("AGENTS.md"));
    }

    #[test]
    fn create_then_edit_with_conflict_detection() {
        let root = tmp("write");
        let loc = locate(&root.join("CLAUDE.md"), &Scope::Project { root: &root }).unwrap();
        let empty = read_at(&loc).unwrap();
        assert!(!empty.exists && empty.hash.is_none());

        let created = write_at(&loc, "# Mine\n", None).unwrap();
        assert!(created.exists);
        assert_eq!(created.size, 7);
        assert_eq!(fs::read_to_string(root.join("CLAUDE.md")).unwrap(), "# Mine\n");
        // Creating again from the "missing" state: someone created it meanwhile.
        assert!(write_at(&loc, "x", None).unwrap_err().starts_with(CONFLICT));

        // Changed outside since it was read: refused, the outside change kept.
        fs::write(root.join("CLAUDE.md"), "# Theirs\n").unwrap();
        let err = write_at(&loc, "# Stale\n", created.hash.as_deref()).unwrap_err();
        assert!(err.starts_with(CONFLICT), "{err}");
        assert_eq!(fs::read_to_string(root.join("CLAUDE.md")).unwrap(), "# Theirs\n");

        // Deleted outside: also a conflict.
        let fresh = read_at(&loc).unwrap();
        fs::remove_file(root.join("CLAUDE.md")).unwrap();
        assert!(write_at(&loc, "y", fresh.hash.as_deref()).unwrap_err().starts_with(CONFLICT));
    }

    #[test]
    fn a_save_from_a_stale_buffer_cannot_drop_a_managed_block() {
        let root = tmp("blocks");
        let agents = root.join("AGENTS.md");
        fs::write(&agents, "# Team\n").unwrap();
        let loc = locate(&agents, &Scope::Project { root: &root }).unwrap();
        let opened = read_at(&loc).unwrap();
        assert!(opened.managed_blocks.is_empty());

        agents_block::upsert(&agents, "rule", "style", "Use tabs.").unwrap();
        assert!(write_at(&loc, "# Team, edited\n", opened.hash.as_deref()).unwrap_err().starts_with(CONFLICT));
        let reloaded = read_at(&loc).unwrap();
        assert_eq!(reloaded.managed_blocks, vec!["rule:style".to_string()]);
    }

    #[test]
    fn write_is_atomic_and_keeps_permissions() {
        let root = tmp("atomic");
        let file = root.join("CLAUDE.md");
        fs::write(&file, "old\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let loc = locate(&file, &Scope::Project { root: &root }).unwrap();
        let opened = read_at(&loc).unwrap();
        write_at(&loc, "new\n", opened.hash.as_deref()).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // No temp file left behind.
        let names: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("CLAUDE.md")]);
    }

    #[test]
    fn refuses_non_utf8_and_oversized_files() {
        let root = tmp("bad");
        let file = root.join("AGENTS.md");
        fs::write(&file, b"# \x93\xfa\x96\x7b\x8c\xea\n").unwrap();
        let loc = locate(&file, &Scope::Project { root: &root }).unwrap();
        assert!(read_at(&loc).is_err());
        fs::write(&file, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(read_at(&loc).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn project_links_stay_inside_the_project() {
        use std::os::unix::fs::symlink;
        let base = tmp("links");
        let root = base.join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(base.join("secret.txt"), "SECRET").unwrap();
        fs::write(root.join("CLAUDE.md"), "# Shared\n").unwrap();
        let scope = Scope::Project { root: &root };

        // AGENTS.md -> ../secret.txt: refused, nothing read or written through it.
        symlink(base.join("secret.txt"), root.join("AGENTS.md")).unwrap();
        let err = locate(&root.join("AGENTS.md"), &scope).unwrap_err();
        assert!(err.contains("outside the project") && !err.contains("SECRET"), "{err}");

        // A dangling link: refused rather than creating whatever it points at.
        fs::remove_file(root.join("AGENTS.md")).unwrap();
        symlink(base.join("missing.md"), root.join("AGENTS.md")).unwrap();
        assert!(locate(&root.join("AGENTS.md"), &scope).is_err());
        assert!(!base.join("missing.md").exists());

        // AGENTS.md -> CLAUDE.md inside the project: followed, the link itself kept.
        fs::remove_file(root.join("AGENTS.md")).unwrap();
        symlink("CLAUDE.md", root.join("AGENTS.md")).unwrap();
        let loc = locate(&root.join("AGENTS.md"), &scope).unwrap();
        let opened = read_at(&loc).unwrap();
        assert_eq!(opened.content, "# Shared\n");
        assert_eq!(opened.link_target.as_deref(), Some(root.join("CLAUDE.md").display().to_string().as_str()));
        write_at(&loc, "# Shared, edited\n", opened.hash.as_deref()).unwrap();
        assert!(fs::symlink_metadata(root.join("AGENTS.md")).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(root.join("CLAUDE.md")).unwrap(), "# Shared, edited\n");
    }

    #[cfg(unix)]
    #[test]
    fn user_file_may_link_to_dotfiles_but_not_into_a_credential_store() {
        use std::os::unix::fs::symlink;
        let home = tmp("home");
        let app_data = home.join("Library/Application Support/puppeteer");
        for d in [".claude", ".ssh", "dotfiles", "Library/Application Support/puppeteer"] {
            fs::create_dir_all(home.join(d)).unwrap();
        }
        let scope = Scope::Home { home: &home, app_data: Some(&app_data) };
        let file = home.join(".claude/CLAUDE.md");

        fs::write(home.join("dotfiles/CLAUDE.md"), "# Me\n").unwrap();
        symlink(home.join("dotfiles/CLAUDE.md"), &file).unwrap();
        assert_eq!(read_at(&locate(&file, &scope).unwrap()).unwrap().content, "# Me\n");

        for target in [home.join(".ssh/id_ed25519"), app_data.join("secrets.json")] {
            fs::write(&target, "SECRET").unwrap();
            fs::remove_file(&file).unwrap();
            symlink(&target, &file).unwrap();
            let err = locate(&file, &scope).unwrap_err();
            assert!(err.contains("credential store") && !err.contains("SECRET"), "{err}");
        }
    }

    /// `~/.codex` is a credential store (`auth.json`): its `AGENTS.md` still opens and is created
    /// there, but a link from it into the store is refused.
    #[test]
    fn codex_agents_md_opens_although_codex_home_is_a_credential_store() {
        let home = tmp("codex");
        let scope = Scope::Home { home: &home, app_data: None };
        let file = home.join(".codex/AGENTS.md");

        let loc = locate(&file, &scope).unwrap();
        assert!(!read_at(&loc).unwrap().exists);
        let created = write_at(&loc, "# Codex\n", None).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "# Codex\n");
        let loc = locate(&file, &scope).unwrap();
        assert_eq!(read_at(&loc).unwrap().hash, created.hash);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::write(home.join(".codex/auth.json"), "SECRET").unwrap();
            fs::remove_file(&file).unwrap();
            symlink(home.join(".codex/auth.json"), &file).unwrap();
            let err = locate(&file, &scope).unwrap_err();
            assert!(err.contains("credential store") && !err.contains("SECRET"), "{err}");
        }
    }

    #[test]
    fn user_file_is_created_with_its_folder() {
        let home = tmp("fresh");
        let file = home.join(".claude/CLAUDE.md");
        let loc = locate(&file, &Scope::Home { home: &home, app_data: None }).unwrap();
        assert!(!read_at(&loc).unwrap().exists);
        write_at(&loc, "# Me\n", None).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "# Me\n");
    }

    #[test]
    fn commands_are_local_only() {
        for name in ["memory_read", "memory_write"] {
            assert!(crate::remote::guard::is_local_only_command(name), "{name}");
        }
    }
}
