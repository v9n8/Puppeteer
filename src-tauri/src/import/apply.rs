//! Copying picked candidates (UX-56): rules, skills, agents and commands become files in the
//! `.claude/` folders of HOME or a project, MCP servers go into the MCP list through its own
//! create path. A pick that fails is reported and the others still run.

use super::{name_taken, with_frontmatter_name, Candidate, ImportKind, Payload};
use crate::commands::library_disk::{claude_folder, remove_link, ItemKind, Trash};
use crate::commands::mcp::{self, McpServerExport};
use crate::commands::memory::write_atomic;
use crate::commands::skills;
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A whole skill folder read into memory.
const MAX_SKILL_BYTES: usize = 20 * 1024 * 1024;

/// What to do when the name is taken where the candidate goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    #[default]
    Skip,
    /// Import under `<name>-2`, `<name>-3`, …
    Rename,
    /// Files only: the one there is removed first (a link) or moved to the trash.
    Overwrite,
}

/// One candidate to import, by its id in the scan.
#[derive(Debug, Clone, Deserialize)]
pub struct Pick {
    pub id: String,
    /// Name instead of the proposed one.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub on_conflict: OnConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Imported,
    /// The name was taken and the pick said to skip: `name` is the one there.
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub id: String,
    pub kind: Option<ImportKind>,
    pub status: Status,
    /// The name it was imported (or skipped) under.
    pub name: Option<String>,
    pub message: Option<String>,
}

/// The `.claude/` folders an import writes files into: HOME's (Global) or a project's.
pub struct FileDest<'a> {
    pub base: PathBuf,
    pub secret: SecretPaths,
    /// Where an overwritten file or skill folder goes (the OS trash in the app). `Sync`: a scan
    /// holds it across awaits.
    pub trash: &'a (dyn Fn(&Path) -> Result<(), String> + Sync),
}

/// Stores an imported MCP server's env / header VALUEs (`secrets::set_mcp_secrets` in the app).
pub type SecretSink<'a> =
    &'a (dyn Fn(&str, &HashMap<String, String>, &HashMap<String, String>) -> Result<(), String> + Sync);

/// The Library folder kind of a file candidate.
fn item_kind(kind: ImportKind) -> Option<ItemKind> {
    match kind {
        ImportKind::Mcp => None,
        ImportKind::Rule => Some(ItemKind::Rule),
        ImportKind::Skill => Some(ItemKind::Skill),
        ImportKind::Agent => Some(ItemKind::Agent),
        ImportKind::Command => Some(ItemKind::Command),
    }
}

/// The entry of item `name` in its folder: a skill's folder, or the markdown file.
fn entry_name(kind: ItemKind, name: &str) -> String {
    if kind == ItemKind::Skill { name.to_string() } else { format!("{name}.md") }
}

/// The entry of `dir` named `entry`, case ignored as on APFS and NTFS (names are ASCII).
fn taken(dir: &Path, entry: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(entry)))
        .map(|e| e.path())
}

/// `<base>-2`, `<base>-3`, … : the first name `dir` does not have.
fn free_entry(dir: &Path, kind: ItemKind, base: &str) -> String {
    let mut n = 2;
    loop {
        let name = format!("{base}-{n}");
        if taken(dir, &entry_name(kind, &name)).is_none() {
            return name;
        }
        n += 1;
    }
}

/// Whether `dest`'s folder for `kind` already has `name`. Nothing is created.
pub(crate) fn file_taken(dest: &FileDest, kind: ImportKind, name: &str) -> bool {
    let Some(kind) = item_kind(kind) else { return false };
    claude_folder(&dest.base, kind, &dest.secret, false).is_ok_and(|dir| taken(&dir, &entry_name(kind, name)).is_some())
}

/// The names every library accepts, as `import::library_name` makes them: a name becomes a path.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn not_found(pick: &Pick) -> Outcome {
    Outcome {
        id: pick.id.clone(),
        kind: None,
        status: Status::Failed,
        name: None,
        message: Some("not found in the source any more".to_string()),
    }
}

fn outcome(pick: &Pick, cand: &Candidate, result: Result<(Status, String), String>) -> Outcome {
    let (status, name, message) = match result {
        Ok((status, name)) => (status, Some(name), None),
        Err(e) => {
            tracing::warn!(event = "import_item_failed", kind = ?cand.kind, error = %e);
            (Status::Failed, None, Some(e))
        }
    };
    Outcome { id: pick.id.clone(), kind: Some(cand.kind), status, name, message }
}

/// Import each pick from `candidates` (a fresh scan of the source) as files of `dest`.
pub fn import_files(dest: &FileDest, candidates: &[Candidate], picks: &[Pick]) -> Vec<Outcome> {
    picks
        .iter()
        .map(|pick| match candidates.iter().find(|c| c.id == pick.id) {
            Some(cand) => outcome(pick, cand, import_file(dest, cand, pick)),
            None => not_found(pick),
        })
        .collect()
}

fn import_file(dest: &FileDest, cand: &Candidate, pick: &Pick) -> Result<(Status, String), String> {
    let kind = item_kind(cand.kind).ok_or("an MCP server is imported into the MCP list, not as a file")?;
    let wanted = pick.name.clone().unwrap_or_else(|| cand.name.clone());
    if !valid_name(&wanted) {
        return Err(format!("\"{wanted}\" is not a valid name: use only letters, digits, - and _"));
    }
    let dir = claude_folder(&dest.base, kind, &dest.secret, true)?;
    let name = match (taken(&dir, &entry_name(kind, &wanted)), pick.on_conflict) {
        (None, _) => wanted,
        (Some(_), OnConflict::Skip) => return Ok((Status::Skipped, wanted)),
        (Some(_), OnConflict::Rename) => free_entry(&dir, kind, &wanted),
        (Some(existing), OnConflict::Overwrite) => {
            clear(&existing, cand, dest.trash)?;
            wanted
        }
    };
    let at = dir.join(entry_name(kind, &name));
    match &cand.payload {
        Payload::Text(content) => {
            // A renamed agent must not keep the name the tool knows the original by.
            let content = if kind == ItemKind::Agent { with_frontmatter_name(content, &name) } else { content.clone() };
            write_atomic(&at, content.as_bytes()).map_err(|e| format!("{}: {}", at.display(), e))?;
        }
        Payload::SkillDir(src) => write_skill(&dir, &at, &skill_files(src, &name)?)?,
        Payload::Mcp(_) => return Err("candidate and payload do not match".to_string()),
    }
    Ok((Status::Imported, name))
}

/// Make room for an overwrite: a link is removed (never what it points at), anything else goes
/// to the trash. Refused when it is, or holds, what is being imported: a tool's folder linked
/// into `.claude/` is never touched.
fn clear(existing: &Path, cand: &Candidate, trash: Trash) -> Result<(), String> {
    if let (Ok(real), Ok(source)) = (fs::canonicalize(existing), fs::canonicalize(&cand.origin)) {
        if source.starts_with(&real) {
            return Err(format!("{} is what is being imported, so it is not overwritten", existing.display()));
        }
    }
    if fs::symlink_metadata(existing).is_ok_and(|m| m.file_type().is_symlink()) { remove_link(existing) } else { trash(existing) }
}

/// Write a skill's files as the folder `at`, through a hidden folder renamed into place, so a
/// failure leaves no half skill (a scan skips dot entries).
fn write_skill(dir: &Path, at: &Path, files: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let tmp = dir.join(format!(".import-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        for (rel, data) in files {
            let path = tmp.join(rel);
            fs::create_dir_all(path.parent().unwrap_or(&tmp))?;
            fs::write(&path, data)?;
        }
        fs::rename(&tmp, at)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result.map_err(|e| format!("{}: {}", at.display(), e))
}

/// Import each MCP pick from `candidates` (a fresh scan of the source) into the MCP list.
pub async fn import_mcp(db: &Db, candidates: &[Candidate], picks: &[Pick], store_secrets: SecretSink<'_>) -> Vec<Outcome> {
    let mut out = Vec::with_capacity(picks.len());
    for pick in picks {
        out.push(match candidates.iter().find(|c| c.id == pick.id) {
            Some(cand) => outcome(pick, cand, import_server(db, cand, pick, store_secrets).await),
            None => not_found(pick),
        });
    }
    out
}

async fn import_server(db: &Db, cand: &Candidate, pick: &Pick, store_secrets: SecretSink<'_>) -> Result<(Status, String), String> {
    let Payload::Mcp(export) = &cand.payload else {
        return Err("only an MCP server is imported into the MCP list".to_string());
    };
    let wanted = pick.name.clone().unwrap_or_else(|| cand.name.clone());
    let name = match (name_taken(db, cand.kind, &wanted).await?, pick.on_conflict) {
        (false, _) => wanted,
        (true, OnConflict::Skip) => return Ok((Status::Skipped, wanted)),
        (true, OnConflict::Rename) => free_name(db, cand.kind, &wanted).await?,
        // Its secrets and the projects using it would have to be replaced too.
        (true, OnConflict::Overwrite) => return Err("an MCP server is not overwritten: skip it or import it under a new name".to_string()),
    };
    let export = McpServerExport { name: name.clone(), ..export.clone() };
    let id = mcp::insert_export(db, &export).await?;
    if let Err(e) = store_secrets(&id, &export.env, &export.headers) {
        // A server whose secrets are missing would start without them.
        let _ = sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(&id).execute(db).await;
        return Err(format!("secrets could not be stored, server not imported: {e}"));
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
    use crate::import::{scan, Adapter, Dest, ScanContext, Scope};
    use sqlx::sqlite::SqlitePoolOptions;
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
        /// The source project.
        root: PathBuf,
        /// The project an import writes into.
        dest: PathBuf,
        ctx: ScanContext,
        base: PathBuf,
        /// Moves into `<base>/trash/` instead of the OS trash.
        trash: Box<dyn Fn(&Path) -> Result<(), String> + Sync>,
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    impl Fixture {
        async fn new(label: &str) -> Self {
            let base = std::env::temp_dir().join(format!("puppeteer-import-{label}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&base).unwrap();
            let base = fs::canonicalize(base).unwrap();
            let root = base.join("proj");
            write(
                &root.join(".mcp.json"),
                r#"{"mcpServers": {
                    "api": {"command": "npx", "args": ["-y", "api"], "env": {"API_KEY": "SECRET-ENV-VALUE"}},
                    "web": {"type": "http", "url": "https://mcp.example", "headers": {"Authorization": "Bearer SECRET-HEADER-VALUE"}},
                    "broken": {"command": "x", "env": "SECRET-BROKEN-VALUE"}
                }}"#,
            );
            write(&root.join(".claude/agents/rev.md"), "---\nname: rev\ndescription: Reviews\n---\nReview.");
            write(&root.join(".claude/commands/ship.md"), "Ship it.");
            write(&root.join(".windsurf/rules/style.md"), "---\ntrigger: always_on\n---\nUse tabs.");
            write(&root.join(".windsurf/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs.");
            write(&root.join(".windsurf/skills/pdf/ref/forms.md"), "forms");
            let dest = base.join("dest");
            fs::create_dir_all(&dest).unwrap();
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            let bin = base.join("trash");
            let trash = Box::new(move |p: &Path| {
                fs::create_dir_all(&bin).unwrap();
                fs::rename(p, bin.join(p.file_name().unwrap())).map_err(|e| e.to_string())
            });
            Fixture { db, root, dest, ctx: ScanContext { library_root: base.join("appdata") }, base, trash }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.base.join(rel)
        }

        fn files(&self) -> FileDest<'_> {
            let secret = SecretPaths::new(Some(&self.path("home")), Some(&self.ctx.library_root));
            FileDest { base: self.dest.clone(), secret, trash: &*self.trash }
        }

        /// As `import_candidates` does for the Library: from the adapter's own scan.
        fn import_files(&self, adapter: &dyn Adapter, picks: &[Pick]) -> Vec<Outcome> {
            let report = adapter.scan(&self.root, Scope::Project, &self.ctx);
            import_files(&self.files(), &report.candidates, picks)
        }

        /// As `import_candidates` does for the MCP list.
        async fn import_mcp(&self, picks: &[Pick], sink: SecretSink<'_>) -> Vec<Outcome> {
            let report = ClaudeCode.scan(&self.root, Scope::Project, &self.ctx);
            import_mcp(&self.db, &report.candidates, picks, sink).await
        }

        async fn names(&self, table: &str) -> Vec<String> {
            sqlx::query_scalar(&format!("SELECT name FROM {table} ORDER BY name")).fetch_all(&self.db).await.unwrap()
        }
    }

    fn pick(id: &str) -> Pick {
        Pick { id: id.into(), name: None, on_conflict: OnConflict::Skip }
    }

    fn with(on_conflict: OnConflict, id: &str) -> Pick {
        Pick { on_conflict, ..pick(id) }
    }

    const RULE: &str = "rules:.windsurf/rules/style.md";
    const SKILL: &str = "skills:.windsurf/skills/pdf";

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

    /// The text a scan of `adapter` read for candidate `id`.
    fn scanned_text(f: &Fixture, adapter: &dyn Adapter, id: &str) -> String {
        let report = adapter.scan(&f.root, Scope::Project, &f.ctx);
        match &report.candidates.iter().find(|c| c.id == id).unwrap().payload {
            Payload::Text(text) => text.clone(),
            _ => panic!("{id} is not a text candidate"),
        }
    }

    #[tokio::test]
    async fn files_land_in_the_claude_folders_and_the_source_is_left_alone() {
        let f = Fixture::new("files").await;
        let before = tree(&f.root);
        let got = f.import_files(&Windsurf, &[pick(RULE), pick(SKILL)]);
        assert!(got.iter().all(|o| o.status == Status::Imported), "{got:?}");
        let got = f.import_files(&ClaudeCode, &[pick("agents:.claude/agents/rev.md"), pick("commands:.claude/commands/ship.md")]);
        assert!(got.iter().all(|o| o.status == Status::Imported), "{got:?}");
        assert_eq!(tree(&f.root), before);

        assert_eq!(read(&f.dest.join(".claude/rules/style.md")), scanned_text(&f, &Windsurf, RULE));
        assert_eq!(read(&f.dest.join(".claude/skills/pdf/SKILL.md")), "---\nname: pdf\ndescription: PDFs\n---\nRead PDFs.");
        assert_eq!(read(&f.dest.join(".claude/skills/pdf/ref/forms.md")), "forms");
        assert_eq!(read(&f.dest.join(".claude/agents/rev.md")), "---\nname: rev\ndescription: Reviews\n---\nReview.");
        assert_eq!(read(&f.dest.join(".claude/commands/ship.md")), "Ship it.");
        // No temp folder is left behind, and no library table is filled.
        assert_eq!(fs::read_dir(f.dest.join(".claude/skills")).unwrap().count(), 1);
        assert!(f.names("rules").await.is_empty() && f.names("skills").await.is_empty());

        // Scanned again, everything is now there; nothing of the MCP list is offered.
        let files = f.files();
        let report = scan(&f.db, &Windsurf, &f.root, Scope::Project, &f.ctx, Dest::Files(&files)).await;
        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates.iter().all(|c| c.exists));
        let report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx, Dest::Files(&files)).await;
        assert!(report.candidates.iter().all(|c| c.kind != ImportKind::Mcp));
    }

    #[tokio::test]
    async fn a_scan_creates_no_folder() {
        let f = Fixture::new("scan-dry").await;
        let files = f.files();
        let report = scan(&f.db, &Windsurf, &f.root, Scope::Project, &f.ctx, Dest::Files(&files)).await;
        assert!(report.candidates.iter().all(|c| !c.exists));
        assert!(!f.dest.join(".claude").exists());
    }

    #[tokio::test]
    async fn a_taken_name_is_skipped_renamed_or_overwritten() {
        let f = Fixture::new("conflict").await;
        // Taken in another case: the file systems the CLIs run on ignore it.
        write(&f.dest.join(".claude/rules/Style.md"), "old rule");
        write(&f.dest.join(".claude/skills/pdf/SKILL.md"), "old skill");

        let got = f.import_files(&Windsurf, &[pick(RULE), pick(SKILL)]);
        assert_eq!(summary(&got), [(RULE.into(), Status::Skipped, Some("style".into())), (SKILL.into(), Status::Skipped, Some("pdf".into()))]);
        assert_eq!(read(&f.dest.join(".claude/rules/Style.md")), "old rule");

        let got = f.import_files(&Windsurf, &[with(OnConflict::Rename, RULE), with(OnConflict::Rename, SKILL)]);
        assert_eq!(summary(&got), [(RULE.into(), Status::Imported, Some("style-2".into())), (SKILL.into(), Status::Imported, Some("pdf-2".into()))]);
        // A renamed copy is not known by the original's name.
        assert!(read(&f.dest.join(".claude/skills/pdf-2/SKILL.md")).starts_with("---\nname: pdf-2\n"));
        let got = f.import_files(&Windsurf, &[with(OnConflict::Rename, RULE)]);
        assert_eq!(got[0].name.as_deref(), Some("style-3"));

        let got = f.import_files(&Windsurf, &[with(OnConflict::Overwrite, RULE), with(OnConflict::Overwrite, SKILL)]);
        assert!(got.iter().all(|o| o.status == Status::Imported), "{got:?}");
        assert_eq!(read(&f.dest.join(".claude/rules/style.md")), scanned_text(&f, &Windsurf, RULE));
        assert_eq!(read(&f.path("trash/Style.md")), "old rule");
        assert_eq!(read(&f.path("trash/pdf/SKILL.md")), "old skill");
        assert!(f.dest.join(".claude/skills/pdf/ref/forms.md").is_file());

        // An explicit name.
        let got = f.import_files(&Windsurf, &[Pick { name: Some("docs".into()), ..pick(SKILL) }]);
        assert_eq!(got[0].status, Status::Imported);
        assert!(read(&f.dest.join(".claude/skills/docs/SKILL.md")).starts_with("---\nname: docs\n"));
    }

    #[tokio::test]
    async fn an_overwritten_link_is_removed_and_its_target_kept() {
        let f = Fixture::new("link").await;
        write(&f.path("elsewhere/style.md"), "shared");
        fs::create_dir_all(f.dest.join(".claude/rules")).unwrap();
        std::os::unix::fs::symlink(f.path("elsewhere/style.md"), f.dest.join(".claude/rules/style.md")).unwrap();
        let got = f.import_files(&Windsurf, &[with(OnConflict::Overwrite, RULE)]);
        assert_eq!(got[0].status, Status::Imported, "{got:?}");
        assert!(!fs::symlink_metadata(f.dest.join(".claude/rules/style.md")).unwrap().file_type().is_symlink());
        assert_eq!(read(&f.path("elsewhere/style.md")), "shared");
        assert!(!f.path("trash").exists());
    }

    #[tokio::test]
    async fn the_source_itself_is_never_overwritten() {
        let f = Fixture::new("self").await;
        // The project keeps Claude's rules as a link to Windsurf's: the target is the source.
        fs::create_dir_all(f.root.join(".claude")).unwrap();
        std::os::unix::fs::symlink(f.root.join(".windsurf/rules"), f.root.join(".claude/rules")).unwrap();
        let before = tree(&f.root.join(".windsurf"));
        let report = Windsurf.scan(&f.root, Scope::Project, &f.ctx);
        let files = FileDest { base: f.root.clone(), ..f.files() };
        let got = import_files(&files, &report.candidates, &[with(OnConflict::Overwrite, RULE)]);
        assert_eq!(got[0].status, Status::Failed);
        assert!(got[0].message.as_deref().unwrap().contains("is what is being imported"), "{got:?}");
        assert_eq!(tree(&f.root.join(".windsurf")), before);
    }

    #[tokio::test]
    async fn nothing_is_written_through_a_claude_folder_linked_out_of_the_project() {
        let f = Fixture::new("outside").await;
        let outside = std::env::temp_dir().join(format!("puppeteer-import-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, f.dest.join(".claude")).unwrap();
        let got = f.import_files(&Windsurf, &[pick(RULE), pick(SKILL)]);
        assert!(got.iter().all(|o| o.status == Status::Failed && o.message.as_deref().unwrap().contains("leads outside")), "{got:?}");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0, "not even a folder is made there");
        fs::remove_dir_all(&outside).unwrap();
    }

    #[tokio::test]
    async fn a_name_that_is_not_a_plain_name_is_refused() {
        let f = Fixture::new("names").await;
        let picks: Vec<Pick> = ["../escape", "a/b", "", "bad name!"].iter().map(|n| Pick { name: Some(n.to_string()), ..pick(RULE) }).collect();
        let got = f.import_files(&Windsurf, &picks);
        assert!(got.iter().all(|o| o.status == Status::Failed), "{got:?}");
        assert!(tree(&f.dest).is_empty() && !f.path("escape.md").exists());
    }

    #[tokio::test]
    async fn a_cursor_rule_comes_in_scoped_as_claude_code_reads_it() {
        let f = Fixture::new("cursor-rule").await;
        write(&f.root.join(".cursor/rules/tsx.mdc"), "---\ndescription: React\nglobs: *.tsx\nalwaysApply: false\n---\nUse hooks.");
        let got = f.import_files(&Cursor, &[pick("rules:.cursor/rules/tsx.mdc")]);
        assert_eq!(summary(&got), [("rules:.cursor/rules/tsx.mdc".to_string(), Status::Imported, Some("tsx".to_string()))]);
        assert_eq!(read(&f.dest.join(".claude/rules/tsx.md")), "---\ndescription: \"React\"\npaths:\n  - \"*.tsx\"\n---\nUse hooks.");
    }

    #[tokio::test]
    async fn mcp_servers_go_into_the_mcp_list_skipped_or_renamed_when_taken() {
        let f = Fixture::new("mcp").await;
        let picks = [pick("mcp:api"), pick("mcp:web")];
        assert!(f.import_mcp(&picks, &no_secrets()).await.iter().all(|o| o.status == Status::Imported));
        assert_eq!(f.names("mcp_servers").await, ["api", "web"]);
        let keys: String = sqlx::query_scalar("SELECT env_keys FROM mcp_servers WHERE name = 'api'").fetch_one(&f.db).await.unwrap();
        assert_eq!(keys, r#"["API_KEY"]"#);
        let files = f.files();
        let report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx, Dest::Mcp).await;
        assert!(report.candidates.iter().all(|c| c.kind == ImportKind::Mcp && c.exists));
        assert!(scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx, Dest::Files(&files)).await.candidates.iter().all(|c| !c.exists));

        assert!(f.import_mcp(&picks, &no_secrets()).await.iter().all(|o| o.status == Status::Skipped));
        let renamed = f.import_mcp(&[with(OnConflict::Rename, "mcp:api")], &no_secrets()).await;
        assert_eq!(summary(&renamed), [("mcp:api".to_string(), Status::Imported, Some("api-2".to_string()))]);
        let overwrite = f.import_mcp(&[with(OnConflict::Overwrite, "mcp:web")], &no_secrets()).await;
        assert_eq!(overwrite[0].status, Status::Failed);
        assert_eq!(f.names("mcp_servers").await, ["api", "api-2", "web"]);
        // A file candidate is not an MCP server, nor the other way round.
        let report = ClaudeCode.scan(&f.root, Scope::Project, &f.ctx);
        assert_eq!(import_mcp(&f.db, &report.candidates, &[pick("commands:.claude/commands/ship.md")], &no_secrets()).await[0].status, Status::Failed);
        assert_eq!(import_files(&files, &report.candidates, &[pick("mcp:api")])[0].status, Status::Failed);
    }

    #[tokio::test]
    async fn one_failing_pick_does_not_stop_the_others() {
        let f = Fixture::new("fail").await;
        let picks = vec![pick("mcp:broken"), pick("rules:gone.md"), pick("mcp:api"), Pick { name: Some("bad name!".into()), ..pick("mcp:web") }];
        let failing_keychain = |_: &str, _: &HashMap<String, String>, _: &HashMap<String, String>| Err("keychain locked".to_string());
        let got = f.import_mcp(&picks, &failing_keychain).await;
        assert_eq!(
            got.iter().map(|o| (o.id.as_str(), o.status)).collect::<Vec<_>>(),
            [("mcp:broken", Status::Failed), ("rules:gone.md", Status::Failed), ("mcp:api", Status::Failed), ("mcp:web", Status::Failed)]
        );
        assert_eq!(got[0].message.as_deref(), Some("not found in the source any more"));
        assert_eq!(got[2].message.as_deref(), Some("secrets could not be stored, server not imported: keychain locked"));
        // The server whose secrets were not stored is not left behind.
        assert!(f.names("mcp_servers").await.is_empty());

        let got = f.import_files(&Windsurf, &[Pick { name: Some("bad name!".into()), ..pick(RULE) }, pick("skills:gone"), pick(SKILL)]);
        assert_eq!(got.iter().map(|o| o.status).collect::<Vec<_>>(), [Status::Failed, Status::Failed, Status::Imported]);
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

        let report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx, Dest::Mcp).await;
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        let mut returned = serde_json::to_string(&report).unwrap();
        let outcomes = import_mcp(&f.db, &report.candidates, &[pick("mcp:api"), pick("mcp:web")], &sink).await;
        returned += &serde_json::to_string(&outcomes).unwrap();
        // A Library import of the same source: its files, and nothing of the config.
        let files = f.files();
        let file_report = scan(&f.db, &ClaudeCode, &f.root, Scope::Project, &f.ctx, Dest::Files(&files)).await;
        returned += &serde_json::to_string(&file_report).unwrap();
        let file_picks = [pick("agents:.claude/agents/rev.md"), pick("commands:.claude/commands/ship.md")];
        returned += &serde_json::to_string(&import_files(&files, &file_report.candidates, &file_picks)).unwrap();
        let home_report = scan(&f.db, &ClaudeCode, &home, Scope::Global, &f.ctx, Dest::Mcp).await;
        returned += &serde_json::to_string(&home_report).unwrap();
        let home_outcomes = import_mcp(&f.db, &home_report.candidates, &[pick("mcp:gh")], &sink).await;
        returned += &serde_json::to_string(&home_outcomes).unwrap();
        assert!(outcomes.iter().chain(&home_outcomes).all(|o| o.status == Status::Imported), "{outcomes:?} {home_outcomes:?}");
        write(
            &home.join(".codex/config.toml"),
            "model_provider = \"ACCOUNT-SECRET\"\n[mcp_servers.cx]\ncommand = \"cx\"\nenv = { CX_TOKEN = \"SECRET-CODEX-VALUE\" }\n[mcp_servers.cxbad]\ncommand = \"x\"\nenv = { T = [\"SECRET-CODEX-BROKEN\"] }\n",
        );
        let codex_report = scan(&f.db, &Codex, &home, Scope::Global, &f.ctx, Dest::Mcp).await;
        assert_eq!(codex_report.problems.len(), 1, "{:?}", codex_report.problems);
        returned += &serde_json::to_string(&codex_report).unwrap();
        let codex_outcomes = import_mcp(&f.db, &codex_report.candidates, &[pick("mcp:cx")], &sink).await;
        returned += &serde_json::to_string(&codex_outcomes).unwrap();
        assert_eq!(codex_outcomes[0].status, Status::Imported, "{codex_outcomes:?}");
        write(
            &home.join(".cursor/mcp.json"),
            r#"{"mcpServers": {"cu": {"url": "https://cu", "headers": {"X-Key": "SECRET-CURSOR-VALUE"}},
                "cuauth": {"url": "https://cu", "auth": {"CLIENT_ID": "id", "CLIENT_SECRET": "SECRET-CURSOR-CLIENT"}}}}"#,
        );
        let cursor_report = scan(&f.db, &Cursor, &home, Scope::Global, &f.ctx, Dest::Mcp).await;
        assert_eq!(cursor_report.problems.len(), 1, "{:?}", cursor_report.problems);
        returned += &serde_json::to_string(&cursor_report).unwrap();
        let cursor_outcomes = import_mcp(&f.db, &cursor_report.candidates, &[pick("mcp:cu")], &sink).await;
        returned += &serde_json::to_string(&cursor_outcomes).unwrap();
        assert_eq!(cursor_outcomes[0].status, Status::Imported, "{cursor_outcomes:?}");
        write(
            &claude_desktop::config_path(&home),
            r#"{"preferences": {"note": "DESKTOP-PREFERENCE"}, "mcpServers": {"cd": {"command": "cd", "env": {"CD_TOKEN": "SECRET-DESKTOP-VALUE"}},
                "cdbad": {"command": "x", "env": {"T": ["SECRET-DESKTOP-BROKEN"]}}}}"#,
        );
        let desktop_report = scan(&f.db, &ClaudeDesktop, &home, Scope::Global, &f.ctx, Dest::Mcp).await;
        assert_eq!(desktop_report.problems.len(), 1, "{:?}", desktop_report.problems);
        returned += &serde_json::to_string(&desktop_report).unwrap();
        let desktop_outcomes = import_mcp(&f.db, &desktop_report.candidates, &[pick("mcp:cd")], &sink).await;
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
            let report = scan(&f.db, adapter, root, scope, &f.ctx, Dest::Mcp).await;
            returned += &serde_json::to_string(&report).unwrap();
            let outcomes = import_mcp(&f.db, &report.candidates, &[pick(id)], &sink).await;
            returned += &serde_json::to_string(&outcomes).unwrap();
            assert_eq!(outcomes[0].status, Status::Imported, "{} {outcomes:?}", adapter.id());
        }
        // A failure is logged too.
        import_mcp(&f.db, &report.candidates, &[Pick { name: Some("bad name!".into()), ..pick("mcp:api") }], &sink).await;

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
        // The written files hold no MCP config either.
        for (path, data) in tree(&f.dest) {
            let text = String::from_utf8_lossy(&data);
            assert!(SECRETS.iter().all(|s| !text.contains(s)), "{}", path.display());
        }
    }
}
