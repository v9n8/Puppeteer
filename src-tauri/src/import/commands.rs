//! The import commands, all gated on [`FEATURE`] and local-only (`remote::guard`): a remote
//! desktop neither reads this machine's tool configs (`~/.claude.json` holds the Claude account)
//! nor fills the libraries from them. The webview names a source, never a path.

use super::apply::{self, LibraryDirs, Outcome, Pick};
use super::*;
use crate::commands::library_items::{library_dir, LibraryKind};
use crate::commands::{rules, skills};
use tauri::{AppHandle, Manager, State};

/// A source as the webview names it: HOME (`global`) or a project by id.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceRef {
    pub adapter: String,
    pub scope: Scope,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportSource {
    pub adapter: &'static str,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub root: String,
    /// What it has to import, from cheap checks: a scan can still find nothing.
    pub kinds: Vec<ImportKind>,
}

/// Every adapter's HOME and project (`(id, name, path)`) sources that have something to import.
pub fn detect_sources(home: Option<&Path>, projects: &[(String, String, String)], ctx: &ScanContext) -> Vec<ImportSource> {
    let mut out = Vec::new();
    for adapter in ADAPTERS {
        let roots = home
            .map(|h| (Scope::Global, None, None, h.to_path_buf()))
            .into_iter()
            .chain(projects.iter().map(|(id, name, path)| (Scope::Project, Some(id.clone()), Some(name.clone()), PathBuf::from(path))));
        for (scope, project_id, project_name, root) in roots {
            let kinds = adapter.detect(&root, scope, ctx);
            if !kinds.is_empty() {
                out.push(ImportSource { adapter: adapter.id(), scope, project_id, project_name, root: root.display().to_string(), kinds });
            }
        }
    }
    out
}

async fn projects(db: &Db) -> Result<Vec<(String, String, String)>, String> {
    sqlx::query_as("SELECT id, name, path FROM projects ORDER BY name").fetch_all(db).await.map_err(|e| e.to_string())
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

fn context(app: &AppHandle) -> Result<ScanContext, String> {
    Ok(ScanContext { library_root: app.path().app_data_dir().map_err(|e| e.to_string())? })
}

#[tauri::command]
pub async fn list_import_sources(app: AppHandle, db: State<'_, Db>) -> Result<Vec<ImportSource>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let home = app.path().home_dir().ok();
    Ok(detect_sources(home.as_deref(), &projects(db.inner()).await?, &context(&app)?))
}

#[tauri::command]
pub async fn scan_import_source(app: AppHandle, db: State<'_, Db>, source: SourceRef) -> Result<ScanReport, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let adapter = adapter(&source.adapter)?;
    let root = root_of(&app, db.inner(), &source).await?;
    let report = scan(db.inner(), adapter, &root, source.scope, &context(&app)?).await;
    tracing::info!(
        event = "import_scan",
        adapter = adapter.id(),
        scope = ?source.scope,
        candidates = report.candidates.len(),
        problems = report.problems.len()
    );
    Ok(report)
}

/// Import `picks` from a fresh scan of `source`, one outcome per pick.
#[tauri::command]
pub async fn import_candidates(
    app: AppHandle,
    db: State<'_, Db>,
    source: SourceRef,
    picks: Vec<Pick>,
) -> Result<Vec<Outcome>, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let adapter = adapter(&source.adapter)?;
    let root = root_of(&app, db.inner(), &source).await?;
    let candidates = adapter.scan(&root, source.scope, &context(&app)?).candidates;
    let dirs = LibraryDirs {
        skills: skills::skills_dir(&app),
        rules: rules::rules_dir(&app),
        agents: library_dir(&app, LibraryKind::Agent)?,
        commands: library_dir(&app, LibraryKind::Command)?,
    };
    let store = |id: &str, env: &HashMap<String, String>, headers: &HashMap<String, String>| {
        crate::secrets::set_mcp_secrets(id, env, headers).map_err(|e| e.to_string())
    };
    let outcomes = apply::import(db.inner(), &dirs, &candidates, &picks, &store).await;
    let count = |s: apply::Status| outcomes.iter().filter(|o| o.status == s).count();
    tracing::info!(
        event = "import_done",
        adapter = adapter.id(),
        scope = ?source.scope,
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
    fn sources_list_home_and_projects_with_something_to_import() {
        let base = std::env::temp_dir().join(format!("puppeteer-import-sources-{}", uuid::Uuid::new_v4()));
        let home = base.join("home");
        let full = base.join("full");
        let empty = base.join("empty");
        for (path, content) in [
            (home.join(".claude/agents/a.md"), "a"),
            (full.join(".mcp.json"), "{}"),
            (full.join("CLAUDE.md"), "x"),
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
        let got = detect_sources(Some(&home), &projects, &ctx);
        assert_eq!(
            got,
            [
                ImportSource {
                    adapter: "claude_code",
                    scope: Scope::Global,
                    project_id: None,
                    project_name: None,
                    root: home.display().to_string(),
                    kinds: vec![ImportKind::Agent],
                },
                ImportSource {
                    adapter: "claude_code",
                    scope: Scope::Project,
                    project_id: Some("p1".into()),
                    project_name: Some("Full".into()),
                    root: full.display().to_string(),
                    kinds: vec![ImportKind::Mcp, ImportKind::Rule],
                },
            ]
        );
        assert!(detect_sources(None, &[], &ctx).is_empty());
    }

    #[test]
    fn the_contract_is_snake_case() {
        let src: SourceRef = serde_json::from_value(serde_json::json!({"adapter": "claude_code", "scope": "project", "project_id": "p1"})).unwrap();
        assert_eq!((src.scope, src.project_id.as_deref()), (Scope::Project, Some("p1")));
        let pick: Pick = serde_json::from_value(serde_json::json!({"id": "mcp:x", "on_conflict": "rename"})).unwrap();
        assert_eq!(pick.on_conflict, apply::OnConflict::Rename);
        let pick: Pick = serde_json::from_value(serde_json::json!({"id": "mcp:x"})).unwrap();
        assert_eq!(pick.on_conflict, apply::OnConflict::Skip);
        assert_eq!(serde_json::to_value(ImportKind::Mcp).unwrap(), "mcp");
        assert_eq!(serde_json::to_value(apply::Status::Imported).unwrap(), "imported");
    }
}
