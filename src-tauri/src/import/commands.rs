//! The import commands, all gated on [`FEATURE`] and local-only (`remote::guard`): a remote
//! desktop neither reads this machine's tool configs (`~/.claude.json` holds the Claude account)
//! nor writes files or MCP servers from them. The webview names a source and a destination,
//! never a path.

use super::apply::{self, FileDest, Outcome, Pick};
use super::*;
use crate::commands::library_disk::{os_trash, LibraryScope};
use crate::commands::memory::project_root;
use tauri::{AppHandle, Manager, State};

/// A source as the webview names it: HOME (`global`) or a project by id.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceRef {
    pub adapter: String,
    pub scope: Scope,
    #[serde(default)]
    pub project_id: Option<String>,
}

/// Where an import goes, as the webview names it: the Library's scope (HOME's or a project's
/// `.claude/` folders), or the MCP list.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "into", rename_all = "snake_case")]
pub enum DestRef {
    Library {
        scope: LibraryScope,
        #[serde(default)]
        project_id: Option<String>,
    },
    Mcp,
}

impl DestRef {
    fn fills(&self) -> ImportInto {
        match self {
            DestRef::Library { .. } => ImportInto::Library,
            DestRef::Mcp => ImportInto::Mcp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportSource {
    pub adapter: &'static str,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub root: String,
    /// What it has to import there, from cheap checks: a scan can still find nothing.
    pub kinds: Vec<ImportKind>,
}

/// Every offered adapter's HOME and project (`(id, name, path)`) sources that have something to
/// import into `into`.
pub fn detect_sources(
    home: Option<&Path>,
    projects: &[(String, String, String)],
    ctx: &ScanContext,
    into: ImportInto,
) -> Vec<ImportSource> {
    let mut out = Vec::new();
    for adapter in ADAPTERS.iter().filter(|a| into.offers(a.id())) {
        let roots = home
            .map(|h| (Scope::Global, None, None, h.to_path_buf()))
            .into_iter()
            .chain(projects.iter().map(|(id, name, path)| (Scope::Project, Some(id.clone()), Some(name.clone()), PathBuf::from(path))));
        for (scope, project_id, project_name, root) in roots {
            let kinds: Vec<ImportKind> = adapter.detect(&root, scope, ctx).into_iter().filter(|k| into.takes(*k)).collect();
            if !kinds.is_empty() {
                out.push(ImportSource { adapter: adapter.id(), scope, project_id, project_name, root: root.display().to_string(), kinds });
            }
        }
    }
    out
}

async fn projects(db: &Db) -> Result<Vec<(String, String, String)>, String> {
    sqlx::query_as("SELECT id, name, path FROM projects WHERE deleted_at IS NULL ORDER BY name").fetch_all(db).await.map_err(|e| e.to_string())
}

async fn root_of(app: &AppHandle, db: &Db, source: &SourceRef) -> Result<PathBuf, String> {
    match (source.scope, &source.project_id) {
        (Scope::Global, _) => app.path().home_dir().map_err(|e| e.to_string()),
        (Scope::Project, Some(id)) => sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?
            .map(PathBuf::from)
            .ok_or_else(|| "Project not found".to_string()),
        (Scope::Project, None) => Err("project_id is required for a project source".to_string()),
    }
}

/// The `.claude/` folders a Library import writes into: HOME's, or a live project's.
async fn file_dest(app: &AppHandle, db: &Db, scope: LibraryScope, project_id: Option<&str>) -> Result<FileDest<'static>, String> {
    let base = match scope {
        LibraryScope::Global => app.path().home_dir().map_err(|e| e.to_string())?,
        LibraryScope::Project => project_root(db, project_id.ok_or("A project is required")?).await?,
    };
    Ok(FileDest { base, secret: SecretPaths::for_app(app), trash: &os_trash })
}

fn context(app: &AppHandle) -> Result<ScanContext, String> {
    Ok(ScanContext { library_root: app.path().app_data_dir().map_err(|e| e.to_string())? })
}

/// The adapter `source` names, when it is offered for `dest`.
fn offered(source: &SourceRef, dest: &DestRef) -> Result<&'static dyn Adapter, String> {
    let adapter = adapter(&source.adapter)?;
    if dest.fills().offers(adapter.id()) {
        Ok(adapter)
    } else {
        Err(format!("'{}' is read directly by the Library, there is nothing to import", adapter.id()))
    }
}

#[tauri::command]
pub async fn list_import_sources(app: AppHandle, db: State<'_, Db>, into: ImportInto) -> Result<Vec<ImportSource>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let home = app.path().home_dir().ok();
    Ok(detect_sources(home.as_deref(), &projects(db.inner()).await?, &context(&app)?, into))
}

#[tauri::command]
pub async fn scan_import_source(app: AppHandle, db: State<'_, Db>, source: SourceRef, dest: DestRef) -> Result<ScanReport, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let adapter = offered(&source, &dest)?;
    let root = root_of(&app, db.inner(), &source).await?;
    let ctx = context(&app)?;
    let report = match &dest {
        DestRef::Library { scope, project_id } => {
            let files = file_dest(&app, db.inner(), *scope, project_id.as_deref()).await?;
            scan(db.inner(), adapter, &root, source.scope, &ctx, Dest::Files(&files)).await
        }
        DestRef::Mcp => scan(db.inner(), adapter, &root, source.scope, &ctx, Dest::Mcp).await,
    };
    tracing::info!(
        event = "import_scan",
        adapter = adapter.id(),
        scope = ?source.scope,
        into = ?dest.fills(),
        candidates = report.candidates.len(),
        problems = report.problems.len()
    );
    Ok(report)
}

/// Import `picks` from a fresh scan of `source` into `dest`, one outcome per pick.
#[tauri::command]
pub async fn import_candidates(
    app: AppHandle,
    db: State<'_, Db>,
    source: SourceRef,
    dest: DestRef,
    picks: Vec<Pick>,
) -> Result<Vec<Outcome>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let adapter = offered(&source, &dest)?;
    let root = root_of(&app, db.inner(), &source).await?;
    let mut candidates = adapter.scan(&root, source.scope, &context(&app)?).candidates;
    candidates.retain(|c| dest.fills().takes(c.kind));
    let outcomes = match &dest {
        DestRef::Library { scope, project_id } => {
            let files = file_dest(&app, db.inner(), *scope, project_id.as_deref()).await?;
            apply::import_files(&files, &candidates, &picks)
        }
        DestRef::Mcp => {
            let store = |id: &str, env: &HashMap<String, String>, headers: &HashMap<String, String>| {
                crate::secrets::set_mcp_secrets(id, env, headers).map_err(|e| e.to_string())
            };
            apply::import_mcp(db.inner(), &candidates, &picks, &store).await
        }
    };
    let count = |s: apply::Status| outcomes.iter().filter(|o| o.status == s).count();
    tracing::info!(
        event = "import_done",
        adapter = adapter.id(),
        scope = ?source.scope,
        into = ?dest.fills(),
        imported = count(apply::Status::Imported),
        skipped = count(apply::Status::Skipped),
        failed = count(apply::Status::Failed)
    );
    Ok(outcomes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn sources_list_home_and_projects_with_something_to_import_there() {
        let base = std::env::temp_dir().join(format!("puppeteer-import-sources-{}", uuid::Uuid::new_v4()));
        let home = base.join("home");
        let full = base.join("full");
        let empty = base.join("empty");
        for (path, content) in [
            (home.join(".claude/agents/a.md"), "a"),
            (full.join(".mcp.json"), "{}"),
            (full.join("CLAUDE.md"), "x"),
            (full.join(".cursor/mcp.json"), "{}"),
            (full.join(".cursorrules"), "x"),
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        fs::create_dir_all(&empty).unwrap();
        let projects = [
            ("p1".to_string(), "Full".to_string(), full.display().to_string()),
            ("p2".to_string(), "Empty".to_string(), empty.display().to_string()),
            ("p3".to_string(), "Gone".to_string(), base.join("gone").display().to_string()),
        ];
        let ctx = ScanContext { library_root: base.join("appdata") };
        let source = |adapter, kinds| ImportSource {
            adapter,
            scope: Scope::Project,
            project_id: Some("p1".into()),
            project_name: Some("Full".into()),
            root: full.display().to_string(),
            kinds,
        };
        // MCP servers come from every tool, Claude Code included.
        assert_eq!(
            detect_sources(Some(&home), &projects, &ctx, ImportInto::Mcp),
            [source("claude_code", vec![ImportKind::Mcp]), source("cursor", vec![ImportKind::Mcp])]
        );
        // Files: Claude Code and Codex are left to the Library, which reads them where they are.
        assert_eq!(detect_sources(Some(&home), &projects, &ctx, ImportInto::Library), [source("cursor", vec![ImportKind::Rule])]);
        assert!(detect_sources(None, &[], &ctx, ImportInto::Library).is_empty());
    }

    #[test]
    fn the_contract_is_snake_case() {
        let src: SourceRef = serde_json::from_value(serde_json::json!({"adapter": "claude_code", "scope": "project", "project_id": "p1"})).unwrap();
        assert_eq!((src.scope, src.project_id.as_deref()), (Scope::Project, Some("p1")));
        let dest: DestRef = serde_json::from_value(serde_json::json!({"into": "library", "scope": "project", "project_id": "p1"})).unwrap();
        assert!(matches!(dest, DestRef::Library { scope: LibraryScope::Project, project_id: Some(ref id) } if id == "p1"));
        let dest: DestRef = serde_json::from_value(serde_json::json!({"into": "mcp"})).unwrap();
        assert_eq!(dest.fills(), ImportInto::Mcp);
        let into: ImportInto = serde_json::from_value(serde_json::json!("library")).unwrap();
        assert_eq!(into, ImportInto::Library);
        let pick: Pick = serde_json::from_value(serde_json::json!({"id": "mcp:x", "on_conflict": "overwrite"})).unwrap();
        assert_eq!(pick.on_conflict, apply::OnConflict::Overwrite);
        let pick: Pick = serde_json::from_value(serde_json::json!({"id": "mcp:x"})).unwrap();
        assert_eq!(pick.on_conflict, apply::OnConflict::Skip);
        assert_eq!(serde_json::to_value(ImportKind::Mcp).unwrap(), "mcp");
        assert_eq!(serde_json::to_value(apply::Status::Imported).unwrap(), "imported");
    }
}
