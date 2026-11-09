//! Copying picked candidates into the libraries, through each library's own create path. A pick
//! that fails is reported and the others still run.

use super::{name_taken, same_library_skill, with_frontmatter_name, Candidate, ImportKind, Payload};
use crate::commands::library_items::{self, CreateItemPayload, LibraryKind};
use crate::commands::mcp::{self, McpServerExport};
use crate::commands::{rules, skills};
use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A whole skill folder read into memory.
const MAX_SKILL_BYTES: usize = 20 * 1024 * 1024;

/// What to do when the library already has the name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    #[default]
    Skip,
    /// Import under `<name>-2`, `<name>-3`, …
    Rename,
}

/// One candidate to import, by its id in the scan.
#[derive(Debug, Clone, Deserialize)]
pub struct Pick {
    pub id: String,
    /// Library name instead of the proposed one.
    #[serde(default)]
    pub name: Option<String>,
    /// Skills and rules: `claude`, `codex` or `both` instead of the candidate's.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub on_conflict: OnConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Imported,
    /// The name was taken and the pick said to skip, or (a skill) the library already has it:
    /// `name` is then that library skill.
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub id: String,
    pub kind: Option<ImportKind>,
    pub status: Status,
    /// The library name it was imported (or skipped) under.
    pub name: Option<String>,
    pub message: Option<String>,
}

pub struct LibraryDirs {
    pub skills: PathBuf,
    pub rules: PathBuf,
    pub agents: PathBuf,
    pub commands: PathBuf,
}

/// Stores an imported MCP server's env / header VALUEs (`secrets::set_mcp_secrets` in the app).
pub type SecretSink<'a> =
    &'a (dyn Fn(&str, &HashMap<String, String>, &HashMap<String, String>) -> Result<(), String> + Sync);

/// Import each pick from `candidates` (a fresh scan of the source).
pub async fn import(
    db: &Db,
    dirs: &LibraryDirs,
    candidates: &[Candidate],
    picks: &[Pick],
    store_secrets: SecretSink<'_>,
) -> Vec<Outcome> {
    let mut out = Vec::with_capacity(picks.len());
    for pick in picks {
        let Some(cand) = candidates.iter().find(|c| c.id == pick.id) else {
            out.push(Outcome {
                id: pick.id.clone(),
                kind: None,
                status: Status::Failed,
                name: None,
                message: Some("not found in the source any more".to_string()),
            });
            continue;
        };
        let (status, name, message) = match import_one(db, dirs, cand, pick, store_secrets).await {
            Ok((status, name)) => (status, Some(name), None),
            Err(e) => {
                tracing::warn!(event = "import_item_failed", kind = ?cand.kind, error = %e);
                (Status::Failed, None, Some(e))
            }
        };
        out.push(Outcome { id: pick.id.clone(), kind: Some(cand.kind), status, name, message });
    }
    out
}

async fn import_one(
    db: &Db,
    dirs: &LibraryDirs,
    cand: &Candidate,
    pick: &Pick,
    store_secrets: SecretSink<'_>,
) -> Result<(Status, String), String> {
    if let Some(id) = cand.kind.feature() {
        if !crate::features::is_on(db, id).await {
            return Err(format!("feature_disabled: {id}"));
        }
    }
    if let Payload::SkillDir(dir) = &cand.payload {
        if let Some(existing) = same_library_skill(db, dir).await {
            return Ok((Status::Skipped, existing));
        }
    }
    let wanted = pick.name.clone().unwrap_or_else(|| cand.name.clone());
    let name = if !name_taken(db, cand.kind, &wanted).await? {
        wanted
    } else if pick.on_conflict == OnConflict::Rename {
        free_name(db, cand.kind, &wanted).await?
    } else {
        return Ok((Status::Skipped, wanted));
    };
    let target = pick.target.as_deref().or(cand.target.as_deref()).unwrap_or("claude");

    match (&cand.payload, cand.kind) {
        (Payload::Mcp(export), _) => {
            let export = McpServerExport { name: name.clone(), ..export.clone() };
            let id = mcp::insert_export(db, &export).await?;
            if let Err(e) = store_secrets(&id, &export.env, &export.headers) {
                // A server whose secrets are missing would start without them.
                let _ = sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(&id).execute(db).await;
                return Err(format!("secrets could not be stored, server not imported: {e}"));
            }
        }
        (Payload::Text(content), ImportKind::Rule) => {
            rules::import_backup_rule(&dirs.rules, db, "", &name, &cand.description, target, content).await?;
        }
        (Payload::Text(content), ImportKind::Agent | ImportKind::Command) => {
            let (kind, dir, content) = if cand.kind == ImportKind::Agent {
                (LibraryKind::Agent, &dirs.agents, with_frontmatter_name(content, &name))
            } else {
                (LibraryKind::Command, &dirs.commands, content.clone())
            };
            let payload = CreateItemPayload { name: name.clone(), description: cand.description.clone(), content };
            library_items::create_item(db, kind, dir, payload).await?;
        }
        (Payload::SkillDir(dir), ImportKind::Skill) => {
            let files = skill_files(dir, &name)?;
            skills::import_backup_skill(&dirs.skills, db, "", &name, &cand.description, target, &files).await?;
        }
        _ => return Err("candidate and payload do not match".to_string()),
    }
    Ok((Status::Imported, name))
}

/// The skill folder's files, with SKILL.md's `name:` set to `name`.
fn skill_files(dir: &Path, name: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut files = skills::read_skill_tree(dir)?;
    if files.iter().map(|(_, data)| data.len()).sum::<usize>() > MAX_SKILL_BYTES {
        return Err(format!("skill folder larger than {MAX_SKILL_BYTES} bytes"));
    }
    let skill_md = files.iter_mut().find(|(p, _)| p == Path::new("SKILL.md")).ok_or("SKILL.md is missing")?;
    let text = String::from_utf8(std::mem::take(&mut skill_md.1)).map_err(|_| "SKILL.md is not UTF-8 text")?;
    skill_md.1 = with_frontmatter_name(&text, name).into_bytes();
    Ok(files)
}

/// `<base>-2`, `<base>-3`, … : the first name the library does not have.
async fn free_name(db: &Db, kind: ImportKind, base: &str) -> Result<String, String> {
    let mut n = 2;
    loop {
        let name = format!("{base}-{n}");
        if !name_taken(db, kind, &name).await? {
            return Ok(name);
        }
        n += 1;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::import::claude_code::ClaudeCode;
    use crate::import::claude_desktop::{self, ClaudeDesktop};
    use crate::import::codex::Codex;
    use crate::import::cursor::Cursor;
    use crate::import::gemini::Gemini;
    use crate::import::vscode::VsCode;
    use crate::import::windsurf::Windsurf;
    use crate::import::{scan, Adapter, ScanContext, Scope};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::fs;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    const SECRETS: &[&str] = &[
        "SECRET-ENV-VALUE",
        "SECRET-HEADER-VALUE",
        "SECRET-BROKEN-VALUE",
        "ACCOUNT-SECRET",
        "SECRET-CODEX-VALUE",
        "SECRET-CODEX-BROKEN",
        "SECRET-CURSOR-VALUE",
        "SECRET-CURSOR-CLIENT",
        "SECRET-DESKTOP-VALUE",
        "SECRET-DESKTOP-BROKEN",
        "DESKTOP-PREFERENCE",
        "SECRET-GEMINI-VALUE",
        "SECRET-GEMINI-OAUTH",
        "SECRET-VSCODE-VALUE",
        "SECRET-COPILOT-VALUE",
        "SECRET-WINDSURF-VALUE",
        "SECRET-WINDSURF-BROKEN",
    ];

    struct Fixture {
        db: Db,
        root: PathBuf,
        dirs: LibraryDirs,
        ctx: ScanContext,
        base: PathBuf,
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    impl Fixture {
        async fn new(label: &str) -> Self {
            let base = std::env::temp_dir().join(format!("puppeteer-import-{label}-{}", uuid::Uuid::new_v4()));
            let root = base.join("proj");
            write(
                &root.join(".mcp.json"),
                r#"{"mcpServers": {
                    "api": {"command": "npx", "args": ["-y", "api"], "env": {"API_KEY": "SECRET-ENV-VALUE"}},
                    "web": {"type": "http", "url": "https://mcp.example", "headers": {"Authorization": "Bearer SECRET-HEADER-VALUE"}},
                    "broken": {"command": "x", "env": "SECRET-BROKEN-VALUE"}
                }}"#,
            );
            write(&root.join(".claude/rules/style.md"), "---\ndescription: Style\n---\nUse tabs.");
            write(&root.join(".claude/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs.");
            write(&root.join(".claude/skills/pdf/ref/forms.md"), "forms");
            write(&root.join(".claude/agents/rev.md"), "---\nname: rev\ndescription: Reviews\n---\nReview.");
            write(&root.join(".claude/commands/ship.md"), "Ship it.");
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            let lib = base.join("appdata");
            let dirs = LibraryDirs {
                skills: lib.join("skills"),
                rules: lib.join("rules"),
                agents: lib.join("agents"),
                commands: lib.join("commands"),
            };
            Fixture { db, root, dirs, ctx: ScanContext { library_root: lib }, base }
        }

        /// As `import_candidates` does: from the adapter's own scan.
        async fn import(&self, picks: &[Pick], sink: SecretSink<'_>) -> Vec<Outcome> {
            let report = ClaudeCode.scan(&self.root, Scope::Project, &self.ctx);
            import(&self.db, &self.dirs, &report.candidates, picks, sink).await
        }

        async fn names(&self, table: &str) -> Vec<String> {
            sqlx::query_scalar(&format!("SELECT name FROM {table} ORDER BY name")).fetch_all(&self.db).await.unwrap()
        }
    }

    fn pick(id: &str) -> Pick {
        Pick { id: id.into(), name: None, target: None, on_conflict: OnConflict::Skip }
    }

    fn all_picks() -> Vec<Pick> {
        ["mcp:api", "mcp:web", "rules:.claude/rules/style.md", "skills:.claude/skills/pdf", "agents:.claude/agents/rev.md", "commands:.claude/commands/ship.md"]
            .into_iter()
            .map(pick)
            .collect()
    }

    fn summary(outcomes: &[Outcome]) -> Vec<(String, Status, Option<String>)> {
        outcomes.iter().map(|o| (o.id.clone(), o.status, o.name.clone())).collect()
    }

    fn no_secrets() -> impl Fn(&str, &HashMap<String, String>, &HashMap<String, String>) -> Result<(), String> + Sync {
        |_: &str, _: &HashMap<String, String>, _: &HashMap<String, String>| Ok(())
    }

    /// Every file under `dir`, with its content.
    fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out: Vec<_> = walkdir::WalkDir::new(dir)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| (e.path().to_path_buf(), fs::read(e.path()).unwrap()))
            .collect();
        out.sort();
        out
    }

    #[tokio::test]
    async fn picks_are_copied_into_each_library_and_the_source_is_left_alone() {
        let f = Fixture::new("copy").await;
        let before = tree(&f.root);
        let outcomes = f.import(&all_picks(), &no_secrets()).await;
        assert!(outcomes.iter().all(|o| o.status == Status::Imported), "{outcomes:?}");
        assert_eq!(tree(&f.root), before);

        assert_eq!(f.names("mcp_servers").await, ["api", "web"]);
        assert_eq!(f.names("rules").await, ["style"]);
        assert_eq!(f.names("skills").await, ["pdf"]);
        assert_eq!(f.names("agents").await, ["rev"]);
        assert_eq!(f.names("commands").await, ["ship"]);
        assert_eq!(fs::read_to_string(f.dirs.skills.join("pdf/ref/forms.md")).unwrap(), "forms");
        assert_eq!(fs::read_to_string(f.dirs.rules.join("style.md")).unwrap(), "---\ndescription: Style\n---\nUse tabs.");
        assert_eq!(fs::read_to_string(f.dirs.commands.join("ship.md")).unwrap(), "Ship it.");
        let (keys, target): (String, String) =
            sqlx::query_as("SELECT s.env_keys, r.target FROM mcp_servers s, rules r WHERE s.name = 'api'").fetch_one(&f.db).await.unwrap();
        assert_eq!((keys.as_str(), target.as_str()), (r#"["API_KEY"]"#, "claude"));

        // Scanned again, everything now exists in the library.
        let report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx).await;
        assert!(report.candidates.iter().all(|c| c.exists));
    }

    #[tokio::test]
    async fn a_taken_name_is_skipped_or_renamed() {
        let f = Fixture::new("conflict").await;
        f.import(&all_picks(), &no_secrets()).await;
        let mut picks = all_picks();
        assert!(f.import(&picks, &no_secrets()).await.iter().all(|o| o.status == Status::Skipped));
        // An edited skill: the same SKILL.md again would be a duplicate (below).
        let skill_md = f.root.join(".claude/skills/pdf/SKILL.md");
        write(&skill_md, "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs, v2.");
        for p in &mut picks {
            p.on_conflict = OnConflict::Rename;
        }
        let renamed = f.import(&picks, &no_secrets()).await;
        let names: Vec<_> = renamed.iter().map(|o| (o.status, o.name.clone().unwrap())).collect();
        assert_eq!(names, ["api-2", "web-2", "style-2", "pdf-2", "rev-2", "ship-2"].map(|n| (Status::Imported, n.to_string())));
        // A renamed copy is not known by the original's name.
        assert!(fs::read_to_string(f.dirs.agents.join("rev-2.md")).unwrap().starts_with("---\nname: rev-2\n"));
        assert!(fs::read_to_string(f.dirs.skills.join("pdf-2/SKILL.md")).unwrap().starts_with("---\nname: pdf-2\n"));
        // An explicit name and target.
        write(&skill_md, "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs, v3.");
        let custom = Pick { id: "skills:.claude/skills/pdf".into(), name: Some("docs".into()), target: Some("both".into()), on_conflict: OnConflict::Skip };
        assert_eq!(summary(&f.import(&[custom], &no_secrets()).await)[0].1, Status::Imported);
        let target: String = sqlx::query_scalar("SELECT target FROM skills WHERE name = 'docs'").fetch_one(&f.db).await.unwrap();
        assert_eq!(target, "both");
    }

    #[tokio::test]
    async fn a_name_differing_only_in_case_is_taken() {
        let f = Fixture::new("case").await;
        f.import(&all_picks(), &no_secrets()).await;
        write(&f.root.join(".claude/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs, v2.");
        let ids = [
            ("rules:.claude/rules/style.md", "Style"),
            ("skills:.claude/skills/pdf", "PDF"),
            ("agents:.claude/agents/rev.md", "Rev"),
            ("commands:.claude/commands/ship.md", "SHIP"),
        ];
        let mut picks: Vec<Pick> = ids.iter().map(|(id, name)| Pick { name: Some(name.to_string()), ..pick(id) }).collect();
        assert!(f.import(&picks, &no_secrets()).await.iter().all(|o| o.status == Status::Skipped));
        assert_eq!(fs::read_to_string(f.dirs.agents.join("rev.md")).unwrap(), "---\nname: rev\ndescription: Reviews\n---\nReview.");

        for p in &mut picks {
            p.on_conflict = OnConflict::Rename;
        }
        let names: Vec<_> = f.import(&picks, &no_secrets()).await.into_iter().map(|o| (o.status, o.name.unwrap())).collect();
        assert_eq!(names, ["Style-2", "PDF-2", "Rev-2", "SHIP-2"].map(|n| (Status::Imported, n.to_string())));
        assert_eq!(f.names("agents").await, ["Rev-2", "rev"]);
    }

    #[tokio::test]
    async fn one_failing_pick_does_not_stop_the_others() {
        let f = Fixture::new("fail").await;
        let mut picks = vec![pick("mcp:broken"), pick("rules:gone.md"), pick("mcp:api")];
        picks.push(Pick { name: Some("bad name!".into()), ..pick("commands:.claude/commands/ship.md") });
        picks.push(pick("agents:.claude/agents/rev.md"));
        sqlx::query("INSERT INTO settings (key, value) VALUES ('feature.library.agents', 'false')").execute(&f.db).await.unwrap();
        let failing_keychain = |_: &str, _: &HashMap<String, String>, _: &HashMap<String, String>| Err("keychain locked".to_string());
        let got = f.import(&picks, &failing_keychain).await;
        assert_eq!(
            got.iter().map(|o| (o.id.as_str(), o.status, o.message.clone().unwrap_or_default())).collect::<Vec<_>>(),
            [
                ("mcp:broken", Status::Failed, "not found in the source any more".to_string()),
                ("rules:gone.md", Status::Failed, "not found in the source any more".to_string()),
                ("mcp:api", Status::Failed, "secrets could not be stored, server not imported: keychain locked".to_string()),
                (
                    "commands:.claude/commands/ship.md",
                    Status::Failed,
                    "Command name must only contain letters, numbers, hyphens, and underscores".to_string()
                ),
                ("agents:.claude/agents/rev.md", Status::Failed, "feature_disabled: library.agents".to_string()),
            ]
        );
        // The server whose secrets were not stored is not left behind.
        assert!(f.names("mcp_servers").await.is_empty());
        let rest = f.import(&[pick("rules:.claude/rules/style.md")], &no_secrets()).await;
        assert_eq!(rest[0].status, Status::Imported);
    }

    #[tokio::test]
    async fn a_skill_already_in_the_library_is_not_imported_again() {
        let f = Fixture::new("dup").await;
        // The same skill kept for Codex too, under another folder and `name:`, and in the legacy folder.
        write(&f.root.join(".agents/skills/pdf-tools/SKILL.md"), "---\nname: pdf-tools\ndescription: PDFs\n---\nRead PDFs.");
        write(&f.root.join(".codex/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs.");
        write(&f.root.join(".agents/skills/csv/SKILL.md"), "---\nname: csv\n---\nCSVs.");
        f.import(&[pick("skills:.claude/skills/pdf")], &no_secrets()).await;

        let report = scan(&f.db, &Codex, &f.root, Scope::Project, &f.ctx).await;
        let dups: Vec<_> = report.candidates.iter().map(|c| (c.id.as_str(), c.duplicate_of.as_deref())).collect();
        assert_eq!(dups, [("skills:.agents/skills/csv", None), ("skills:.agents/skills/pdf-tools", Some("pdf"))]);
        let picks: Vec<Pick> = ["skills:.agents/skills/pdf-tools", "skills:.agents/skills/csv"]
            .into_iter()
            .map(|id| Pick { on_conflict: OnConflict::Rename, name: None, target: None, id: id.into() })
            .collect();
        let got = import(&f.db, &f.dirs, &report.candidates, &picks, &no_secrets()).await;
        assert_eq!(
            summary(&got),
            [
                ("skills:.agents/skills/pdf-tools".to_string(), Status::Skipped, Some("pdf".to_string())),
                ("skills:.agents/skills/csv".to_string(), Status::Imported, Some("csv".to_string())),
            ]
        );
        assert_eq!(f.names("skills").await, ["csv", "pdf"]);
        let target: String = sqlx::query_scalar("SELECT target FROM skills WHERE name = 'csv'").fetch_one(&f.db).await.unwrap();
        assert_eq!(target, "codex");
    }

    #[tokio::test]
    async fn a_cursor_rule_comes_in_scoped_as_claude_code_reads_it() {
        let f = Fixture::new("cursor-rule").await;
        write(&f.root.join(".cursor/rules/tsx.mdc"), "---\ndescription: React\nglobs: *.tsx\nalwaysApply: false\n---\nUse hooks.");
        let report = scan(&f.db, &Cursor, &f.root, Scope::Project, &f.ctx).await;
        let got = import(&f.db, &f.dirs, &report.candidates, &[pick("rules:.cursor/rules/tsx.mdc")], &no_secrets()).await;
        assert_eq!(summary(&got), [("rules:.cursor/rules/tsx.mdc".to_string(), Status::Imported, Some("tsx".to_string()))]);
        let file = fs::read_to_string(f.dirs.rules.join("tsx.md")).unwrap();
        assert_eq!(file, "---\ndescription: \"React\"\npaths:\n  - \"*.tsx\"\n---\nUse hooks.");
        let row: (String, String) = sqlx::query_as("SELECT description, target FROM rules WHERE name = 'tsx'").fetch_one(&f.db).await.unwrap();
        assert_eq!(row, ("React".to_string(), "both".to_string()));
    }

    /// Every value of every column of every table, as text.
    async fn db_text(db: &Db) -> String {
        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'").fetch_all(db).await.unwrap();
        let mut out = String::new();
        for table in tables {
            let columns: Vec<String> = sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(db)
                .await
                .unwrap();
            for column in columns {
                let sql = format!("SELECT group_concat(CAST(\"{column}\" AS TEXT), '|') FROM \"{table}\"");
                let values: Option<Vec<u8>> = sqlx::query_scalar(&sql).fetch_one(db).await.unwrap();
                out += &String::from_utf8_lossy(&values.unwrap_or_default());
            }
        }
        out
    }

    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<Vec<u8>>>);

    impl Write for Log {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The env / header VALUEs go to the secret store only: not to SQLite, the logs, or anything
    /// the webview gets back. Nor does anything of `~/.claude.json` but its servers.
    #[tokio::test]
    async fn secret_values_reach_only_the_secret_store() {
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // While it knows a single dispatcher, tracing-core gives a callsite first hit on another
        // test's thread that thread's (none) and caches it as off; a second one prevents that.
        let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());

        let f = Fixture::new("secrets").await;
        let home = f.base.join("home");
        write(
            &home.join(".claude.json"),
            r#"{"oauthAccount": {"emailAddress": "ACCOUNT-SECRET"}, "mcpServers": {"gh": {"command": "gh", "env": {"GH_TOKEN": "SECRET-ENV-VALUE"}}}}"#,
        );
        let stored = Mutex::new(Vec::new());
        let sink = |id: &str, env: &HashMap<String, String>, headers: &HashMap<String, String>| {
            stored.lock().unwrap().push((id.to_string(), env.clone(), headers.clone()));
            Ok(())
        };

        let report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx).await;
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        let mut returned = serde_json::to_string(&report).unwrap();
        let outcomes = import(&f.db, &f.dirs, &report.candidates, &all_picks(), &sink).await;
        returned += &serde_json::to_string(&outcomes).unwrap();
        let home_report = scan(&f.db, &ClaudeCode, &home, Scope::Global, &f.ctx).await;
        returned += &serde_json::to_string(&home_report).unwrap();
        let home_outcomes = import(&f.db, &f.dirs, &home_report.candidates, &[pick("mcp:gh")], &sink).await;
        returned += &serde_json::to_string(&home_outcomes).unwrap();
        assert!(outcomes.iter().chain(&home_outcomes).all(|o| o.status == Status::Imported), "{outcomes:?} {home_outcomes:?}");
        write(
            &home.join(".codex/config.toml"),
            "model_provider = \"ACCOUNT-SECRET\"\n[mcp_servers.cx]\ncommand = \"cx\"\nenv = { CX_TOKEN = \"SECRET-CODEX-VALUE\" }\n[mcp_servers.cxbad]\ncommand = \"x\"\nenv = { T = [\"SECRET-CODEX-BROKEN\"] }\n",
        );
        let codex_report = scan(&f.db, &Codex, &home, Scope::Global, &f.ctx).await;
        assert_eq!(codex_report.problems.len(), 1, "{:?}", codex_report.problems);
        returned += &serde_json::to_string(&codex_report).unwrap();
        let codex_outcomes = import(&f.db, &f.dirs, &codex_report.candidates, &[pick("mcp:cx")], &sink).await;
        returned += &serde_json::to_string(&codex_outcomes).unwrap();
        assert_eq!(codex_outcomes[0].status, Status::Imported, "{codex_outcomes:?}");
        write(
            &home.join(".cursor/mcp.json"),
            r#"{"mcpServers": {"cu": {"url": "https://cu", "headers": {"X-Key": "SECRET-CURSOR-VALUE"}},
                "cuauth": {"url": "https://cu", "auth": {"CLIENT_ID": "id", "CLIENT_SECRET": "SECRET-CURSOR-CLIENT"}}}}"#,
        );
        let cursor_report = scan(&f.db, &Cursor, &home, Scope::Global, &f.ctx).await;
        assert_eq!(cursor_report.problems.len(), 1, "{:?}", cursor_report.problems);
        returned += &serde_json::to_string(&cursor_report).unwrap();
        let cursor_outcomes = import(&f.db, &f.dirs, &cursor_report.candidates, &[pick("mcp:cu")], &sink).await;
        returned += &serde_json::to_string(&cursor_outcomes).unwrap();
        assert_eq!(cursor_outcomes[0].status, Status::Imported, "{cursor_outcomes:?}");
        write(
            &claude_desktop::config_path(&home),
            r#"{"preferences": {"note": "DESKTOP-PREFERENCE"}, "mcpServers": {"cd": {"command": "cd", "env": {"CD_TOKEN": "SECRET-DESKTOP-VALUE"}},
                "cdbad": {"command": "x", "env": {"T": ["SECRET-DESKTOP-BROKEN"]}}}}"#,
        );
        let desktop_report = scan(&f.db, &ClaudeDesktop, &home, Scope::Global, &f.ctx).await;
        assert_eq!(desktop_report.problems.len(), 1, "{:?}", desktop_report.problems);
        returned += &serde_json::to_string(&desktop_report).unwrap();
        let desktop_outcomes = import(&f.db, &f.dirs, &desktop_report.candidates, &[pick("mcp:cd")], &sink).await;
        returned += &serde_json::to_string(&desktop_outcomes).unwrap();
        assert_eq!(desktop_outcomes[0].status, Status::Imported, "{desktop_outcomes:?}");
        write(
            &home.join(".gemini/settings.json"),
            r#"{"security": {"auth": {"selectedType": "ACCOUNT-SECRET"}}, "mcpServers": {"gm": {"command": "gm", "env": {"GM_TOKEN": "SECRET-GEMINI-VALUE"}},
                "gmauth": {"httpUrl": "https://gm", "oauth": {"clientSecret": "SECRET-GEMINI-OAUTH"}}}}"#,
        );
        write(
            &f.root.join(".vscode/mcp.json"),
            r#"{"inputs": [{"type": "promptString", "id": "k", "password": true}], "servers": {"vs": {"type": "http", "url": "https://vs", "headers": {"VS-Key": "SECRET-VSCODE-VALUE"}},
                "vsin": {"type": "stdio", "command": "x", "env": {"K": "${input:k}"}}}}"#,
        );
        write(
            &home.join(".copilot/mcp-config.json"),
            r#"{"mcpServers": {"cp": {"type": "local", "command": "cp", "tools": ["*"], "env": {"CP_TOKEN": "SECRET-COPILOT-VALUE"}}}}"#,
        );
        write(
            &home.join(".codeium/windsurf/mcp_config.json"),
            r#"{"mcpServers": {"ws": {"serverUrl": "https://ws", "headers": {"WS-Key": "SECRET-WINDSURF-VALUE"}},
                "wsbad": {"command": "x", "env": {"T": ["SECRET-WINDSURF-BROKEN"]}}}}"#,
        );
        for (adapter, root, id) in [
            (&Gemini as &dyn Adapter, &home, "mcp:gm"),
            (&VsCode, &f.root, "mcp:vs"),
            (&VsCode, &home, "mcp:cp"),
            (&Windsurf, &home, "mcp:ws"),
        ] {
            let scope = if root == &home { Scope::Global } else { Scope::Project };
            let report = scan(&f.db, adapter, root, scope, &f.ctx).await;
            returned += &serde_json::to_string(&report).unwrap();
            let outcomes = import(&f.db, &f.dirs, &report.candidates, &[pick(id)], &sink).await;
            returned += &serde_json::to_string(&outcomes).unwrap();
            assert_eq!(outcomes[0].status, Status::Imported, "{} {outcomes:?}", adapter.id());
        }
        // A failure is logged too.
        import(&f.db, &f.dirs, &report.candidates, &[Pick { name: Some("bad name!".into()), ..pick("mcp:api") }], &sink).await;

        let stored = stored.into_inner().unwrap();
        let values: Vec<_> = stored.iter().flat_map(|(_, env, headers)| env.values().chain(headers.values()).cloned()).collect();
        assert_eq!(
            values,
            [
                "SECRET-ENV-VALUE",
                "Bearer SECRET-HEADER-VALUE",
                "SECRET-ENV-VALUE",
                "SECRET-CODEX-VALUE",
                "SECRET-CURSOR-VALUE",
                "SECRET-DESKTOP-VALUE",
                "SECRET-GEMINI-VALUE",
                "SECRET-VSCODE-VALUE",
                "SECRET-COPILOT-VALUE",
                "SECRET-WINDSURF-VALUE",
            ]
        );

        let db_text = db_text(&f.db).await;
        assert!(
            db_text.contains("GH_TOKEN") && db_text.contains("API_KEY") && db_text.contains("CX_TOKEN") && db_text.contains("X-Key") && db_text.contains("CD_TOKEN")
                && ["GM_TOKEN", "VS-Key", "CP_TOKEN", "WS-Key"].iter().all(|k| db_text.contains(k)),
            "the dump holds the KEY names"
        );
        let logs = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("import_item_failed"), "{logs}");
        for secret in SECRETS {
            assert!(!db_text.contains(secret), "{secret} in the DB");
            assert!(!logs.contains(secret), "{secret} in the logs");
            assert!(!returned.contains(secret), "{secret} returned to the webview");
        }
        // The library files hold no MCP config either.
        for (path, data) in tree(&f.ctx.library_root) {
            let text = String::from_utf8_lossy(&data);
            assert!(SECRETS.iter().all(|s| !text.contains(s)), "{}", path.display());
        }
    }
}
