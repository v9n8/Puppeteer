//! Codex (the `codex` CLI).
//!
//! - Global (HOME): `~/.codex/config.toml` — only its `[mcp_servers.*]` tables; no other key is
//!   even deserialized — then `~/.codex/AGENTS.md`, `~/.agents/skills` (Codex's user scope) and the
//!   custom prompts `~/.codex/prompts/*.md`, which become commands.
//! - Project: `AGENTS.md` and `.agents/skills`.
//! - Both: the legacy `.codex/skills` folder, read only (Codex moved to `.agents/skills`, and
//!   Puppeteer only ever cleans its old copies there). A legacy skill also in `.agents/skills`, by
//!   name or content, is left out.
//!
//! AGENTS.md becomes a rule without the blocks Puppeteer wrote into it. `CODEX_HOME` is not read:
//! the global source is `~/.codex`.

use super::*;
use crate::commands::agents_block;
use serde::Deserialize;
use toml::Value as Toml;

pub struct Codex;

/// Skills and rules from Codex are meant for Codex.
const TARGET: &str = "codex";

struct Layout {
    /// `config.toml`, global only.
    config: Option<PathBuf>,
    /// AGENTS.md with the rule name it is proposed under.
    agents_md: (PathBuf, String),
    skills: PathBuf,
    legacy_skills: PathBuf,
    /// Custom prompts, global only.
    prompts: Option<PathBuf>,
}

fn layout(root: &Path, scope: Scope) -> Layout {
    let codex = root.join(".codex");
    let skills = root.join(".agents").join("skills");
    let legacy_skills = codex.join("skills");
    match scope {
        Scope::Global => Layout {
            config: Some(codex.join("config.toml")),
            agents_md: (codex.join("AGENTS.md"), "global-agents-md".to_string()),
            skills,
            legacy_skills,
            prompts: Some(codex.join("prompts")),
        },
        Scope::Project => {
            let folder = library_name(&root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            Layout { config: None, agents_md: (root.join("AGENTS.md"), format!("{folder}-agents-md")), skills, legacy_skills, prompts: None }
        }
    }
}

/// Only `mcp_servers` is read; serde skips every other key without keeping it.
#[derive(Deserialize)]
struct CodexConfig {
    #[serde(default)]
    mcp_servers: Option<toml::Table>,
}

impl Adapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn detect(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> Vec<ImportKind> {
        let l = layout(root, scope);
        let mut kinds = Vec::new();
        // The config may hold no server: telling would mean parsing it.
        if l.config.as_ref().is_some_and(|p| p.is_file()) {
            kinds.push(ImportKind::Mcp);
        }
        if l.agents_md.0.is_file() {
            kinds.push(ImportKind::Rule);
        }
        if !skill_dirs(&l.skills, ctx).is_empty() || !skill_dirs(&l.legacy_skills, ctx).is_empty() {
            kinds.push(ImportKind::Skill);
        }
        if l.prompts.as_ref().is_some_and(|p| !md_files(p, ctx).is_empty()) {
            kinds.push(ImportKind::Command);
        }
        kinds
    }

    fn scan(&self, root: &Path, scope: Scope, ctx: &ScanContext) -> ScanReport {
        let l = layout(root, scope);
        let mut report = ScanReport::default();
        if let Some(config) = l.config.as_ref().filter(|p| p.is_file()) {
            let parsed = read_config(config, root, scope, ctx)
                .and_then(|raw| toml::from_str::<CodexConfig>(&raw).map_err(|e| toml_error(&e, &raw)));
            match parsed {
                Ok(CodexConfig { mcp_servers: Some(servers) }) => {
                    for (raw_name, value) in &servers {
                        match mcp_export(&library_name(raw_name), value) {
                            Ok(export) => report.candidates.push(mcp_candidate(config, raw_name, export)),
                            Err(e) => report.problem(config, Some(raw_name), e),
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => report.problem(config, None, e),
            }
        }
        let mut found = Vec::new();
        let (path, name) = &l.agents_md;
        if path.is_file() {
            if let Some(mut c) = text_candidate(&mut report, root, scope, ctx, ImportKind::Rule, path, Some(name.clone()), Some(TARGET)) {
                let Payload::Text(text) = &c.payload else { unreachable!("a text candidate") };
                let own = agents_block::strip_all(text);
                // Only blocks Puppeteer wrote: the user has nothing of their own there.
                if !own.is_empty() {
                    c.hash = crate::commands::rules::content_hash(&own);
                    c.payload = Payload::Text(own);
                    found.push(c);
                }
            }
        }
        // (name, SKILL.md hash but for the name) of the skills kept so far.
        let mut kept: Vec<(String, Option<String>)> = Vec::new();
        let current = skill_dirs(&l.skills, ctx).into_iter().map(|d| (d, false));
        for (dir, legacy) in current.chain(skill_dirs(&l.legacy_skills, ctx).into_iter().map(|d| (d, true))) {
            let Some(c) = skill_candidate(&mut report, root, scope, ctx, &dir, TARGET) else { continue };
            let body = skill_body_hash(&dir);
            if legacy && kept.iter().any(|(name, hash)| *name == c.name || (hash.is_some() && *hash == body)) {
                continue;
            }
            kept.push((c.name.clone(), body));
            found.push(c);
        }
        if let Some(prompts) = &l.prompts {
            for path in md_files(prompts, ctx) {
                found.extend(text_candidate(&mut report, root, scope, ctx, ImportKind::Command, &path, None, None));
            }
        }
        report.candidates.extend(found);
        report
    }
}

/// A TOML error without its message, which quotes the line of the file it is on.
fn toml_error(e: &toml::de::Error, raw: &str) -> String {
    let Some(at) = e.span().map(|s| s.start.min(raw.len())) else { return "invalid TOML".to_string() };
    let before = &raw[..at];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
    format!("invalid TOML at line {line} column {column}")
}

/// One `[mcp_servers.<name>]` table. Errors name the field, never its value.
fn mcp_export(name: &str, value: &Toml) -> Result<McpServerExport, String> {
    let table = value.as_table().ok_or("not a table")?;
    // Puppeteer keeps the VALUE of a header or env var itself; these only name a variable of the
    // shell Codex runs in, which an import cannot read.
    for key in ["bearer_token_env_var", "env_http_headers", "env_vars"] {
        if table.contains_key(key) {
            return Err(format!("`{key}` takes its value from the environment, which cannot be imported"));
        }
    }
    let text = |key: &str| match table.get(key) {
        None => Ok(None),
        Some(Toml::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{key}` is not a string")),
    };
    let map = |key: &str| -> Result<HashMap<String, String>, String> {
        match table.get(key) {
            None => Ok(HashMap::new()),
            Some(Toml::Table(t)) => t
                .iter()
                .map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())).ok_or_else(|| format!("`{key}.{k}` is not a string")))
                .collect(),
            Some(_) => Err(format!("`{key}` is not a table")),
        }
    };
    let command = text("command")?;
    let url = text("url")?;
    // Codex speaks stdio and streamable HTTP; there is no `type` key.
    let transport = match (&command, &url) {
        (Some(_), _) => "stdio",
        (None, Some(_)) => "http",
        (None, None) => return Err("needs a `command` or a `url`".to_string()),
    };
    let args = match table.get("args") {
        None => Vec::new(),
        Some(Toml::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or("`args` is not a list of strings")?,
        Some(_) => return Err("`args` is not a list of strings".to_string()),
    };
    let enabled = match table.get("enabled") {
        None => true,
        Some(Toml::Boolean(on)) => *on,
        Some(_) => return Err("`enabled` is not true or false".to_string()),
    };
    Ok(McpServerExport {
        name: name.to_string(),
        description: String::new(),
        transport: transport.to_string(),
        command,
        args,
        url,
        env: map("env")?,
        headers: map("http_headers")?,
        enabled,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-import-codex-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn ids(report: &ScanReport) -> Vec<(ImportKind, String, String)> {
        report.candidates.iter().map(|c| (c.kind, c.id.clone(), c.name.clone())).collect()
    }

    fn problems(report: &ScanReport) -> Vec<(String, Option<String>, String)> {
        report
            .problems
            .iter()
            .map(|p| (p.path.rsplit('/').next().unwrap().to_string(), p.item.clone(), p.message.clone()))
            .collect()
    }

    const CONFIG: &str = r#"
model = "gpt-5"
[model_providers.corp]
base_url = "https://ACCOUNT-URL"
env_key = "ACCOUNT-KEY"

[mcp_servers.docs]
command = "npx"
args = ["-y", "docs-mcp"]
env = { API_KEY = "SECRET-ENV" }
startup_timeout_sec = 20

[mcp_servers."gh.remote"]
url = "https://mcp.example/gh"
http_headers = { Authorization = "Bearer SECRET-HEADER" }

[mcp_servers.off]
command = "off"
enabled = false

[mcp_servers.token]
url = "https://x"
bearer_token_env_var = "SECRET-NAME"

[mcp_servers.wrong]
command = "x"
args = ["SECRET-ARG", 1]
"#;

    #[test]
    fn home_yields_servers_agents_md_skills_and_prompts() {
        let home = tmp("home");
        let lib = home.join("Library/Application Support/io.github.v9n8.puppeteer");
        write(&home.join(".codex/config.toml"), CONFIG);
        write(&home.join(".codex/AGENTS.md"), "Answer in English.\n");
        write(&home.join(".agents/skills/lint/SKILL.md"), "---\nname: lint\ndescription: Lints\n---\nRun the linter.");
        // Legacy folder: `lint` is in `.agents/skills` already, `old` is not; `.system` is Codex's own.
        write(&home.join(".codex/skills/lint/SKILL.md"), "---\nname: lint\n---\nAn older lint.");
        write(&home.join(".codex/skills/old/SKILL.md"), "---\nname: old\ndescription: Old\n---\nOld.");
        write(&home.join(".codex/skills/.system/sys/SKILL.md"), "---\nname: sys\n---\n");
        write(&home.join(".codex/prompts/review.md"), "---\ndescription: Review a diff\nargument-hint: FILE=<path>\n---\nReview $FILE.");
        write(&home.join(".codex/prompts/notes.txt"), "not a prompt");
        // A global skill Puppeteer linked (D2-05).
        write(&lib.join("skills/own/SKILL.md"), "---\nname: own\n---\n");
        std::os::unix::fs::symlink(lib.join("skills/own"), home.join(".agents/skills/own")).unwrap();

        let ctx = ScanContext { library_root: lib };
        let report = Codex.scan(&home, Scope::Global, &ctx);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Mcp, "mcp:docs".into(), "docs".into()),
                (ImportKind::Mcp, "mcp:gh.remote".into(), "gh-remote".into()),
                (ImportKind::Mcp, "mcp:off".into(), "off".into()),
                (ImportKind::Rule, "rules:.codex/AGENTS.md".into(), "global-agents-md".into()),
                (ImportKind::Skill, "skills:.agents/skills/lint".into(), "lint".into()),
                (ImportKind::Skill, "skills:.codex/skills/old".into(), "old".into()),
                (ImportKind::Command, "commands:.codex/prompts/review.md".into(), "review".into()),
            ]
        );
        assert_eq!(
            problems(&report),
            [
                (
                    "config.toml".into(),
                    Some("token".into()),
                    "`bearer_token_env_var` takes its value from the environment, which cannot be imported".into()
                ),
                ("config.toml".into(), Some("wrong".into()), "`args` is not a list of strings".into()),
            ]
        );
        let by_id = |id: &str| report.candidates.iter().find(|c| c.id == id).unwrap();
        let docs = by_id("mcp:docs");
        assert_eq!(docs.mcp.as_ref().unwrap().env_keys, ["API_KEY"]);
        let Payload::Mcp(export) = &docs.payload else { panic!() };
        assert_eq!((export.transport.as_str(), export.args.clone(), export.env["API_KEY"].as_str()), ("stdio", vec!["-y".to_string(), "docs-mcp".to_string()], "SECRET-ENV"));
        let remote = by_id("mcp:gh.remote").mcp.as_ref().unwrap();
        assert_eq!((remote.transport.as_str(), remote.header_keys.clone()), ("http", vec!["Authorization".to_string()]));
        let Payload::Mcp(off) = &by_id("mcp:off").payload else { panic!() };
        assert!(!off.enabled);
        assert_eq!(by_id("rules:.codex/AGENTS.md").target.as_deref(), Some("codex"));
        assert_eq!(by_id("skills:.agents/skills/lint").target.as_deref(), Some("codex"));
        assert_eq!(by_id("commands:.codex/prompts/review.md").description, "Review a diff");
        assert_eq!(Codex.detect(&home, Scope::Global, &ctx), [ImportKind::Mcp, ImportKind::Rule, ImportKind::Skill, ImportKind::Command]);

        let json = serde_json::to_string(&report).unwrap();
        for value in ["SECRET-", "ACCOUNT-"] {
            assert!(!json.contains(value), "{json}");
        }
    }

    #[test]
    fn same_servers_hash_alike_from_claude_and_codex() {
        let root = tmp("hash");
        let ctx = ScanContext { library_root: root.join("appdata") };
        write(&root.join(".codex/config.toml"), "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"fs\"]\nenv = { K = \"codex value\" }\n");
        write(&root.join(".claude.json"), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["fs"], "env": {"K": "claude value"}}}}"#);
        let codex = Codex.scan(&root, Scope::Global, &ctx);
        let claude = super::super::claude_code::ClaudeCode.scan(&root, Scope::Global, &ctx);
        assert_eq!(codex.candidates[0].hash, claude.candidates[0].hash);
    }

    #[test]
    fn a_broken_file_is_reported_without_quoting_it_and_the_rest_still_scanned() {
        let home = tmp("broken");
        let ctx = ScanContext { library_root: home.join("appdata") };
        write(&home.join(".codex/config.toml"), "[mcp_servers.a]\ncommand = \"a\"\nenv = { K = \"SECRET-UNTERMINATED }\n");
        write(&home.join(".codex/AGENTS.md"), [0x66, 0xe9]);
        write(&home.join(".agents/skills/bad/SKILL.md"), [0xff, 0xfe]);
        write(&home.join(".agents/skills/good/SKILL.md"), "---\nname: good\n---\n");
        write(&home.join(".codex/prompts/big.md"), vec![b'a'; MAX_TEXT_BYTES as usize + 1]);
        write(&home.join(".codex/prompts/ok.md"), "ok");

        let report = Codex.scan(&home, Scope::Global, &ctx);
        assert_eq!(ids(&report), [(ImportKind::Skill, "skills:.agents/skills/good".into(), "good".into()), (ImportKind::Command, "commands:.codex/prompts/ok.md".into(), "ok".into())]);
        let got: Vec<_> = problems(&report).into_iter().map(|(file, _, msg)| (file, msg.split(' ').take(2).collect::<Vec<_>>().join(" "))).collect();
        assert_eq!(
            got,
            [
                ("config.toml".to_string(), "invalid TOML".to_string()),
                ("AGENTS.md".to_string(), "not UTF-8".to_string()),
                ("SKILL.md".to_string(), "not UTF-8".to_string()),
                ("big.md".to_string(), "file too".to_string()),
            ]
        );
        assert_eq!(report.problems[0].message, "invalid TOML at line 3 column 35");
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));

        // A config with no server, or `mcp_servers` of the wrong shape.
        write(&home.join(".codex/config.toml"), "model = \"o3\"\n");
        assert!(Codex.scan(&home, Scope::Global, &ctx).problems.iter().all(|p| !p.path.ends_with("config.toml")));
        write(&home.join(".codex/config.toml"), "mcp_servers = \"SECRET-SHAPE\"\n");
        let report = Codex.scan(&home, Scope::Global, &ctx);
        assert!(report.problems[0].message.starts_with("invalid TOML at line 1"), "{:?}", report.problems);
        assert!(!serde_json::to_string(&report).unwrap().contains("SECRET-"));
    }

    #[test]
    fn mcp_tables_keep_values_out_of_errors() {
        let parse = |s: &str| mcp_export("s", &s.parse::<toml::Table>().unwrap()["s"]);
        assert_eq!(parse("s = \"SECRET-1\"").unwrap_err(), "not a table");
        for (bad, want) in [
            ("command = 1", "`command` is not a string"),
            ("command = \"x\"\nenv = \"SECRET-2\"", "`env` is not a table"),
            ("command = \"x\"\nenv = { TOKEN = 7 }", "`env.TOKEN` is not a string"),
            ("url = \"u\"\nhttp_headers = { A = [\"SECRET-3\"] }", "`http_headers.A` is not a string"),
            ("command = \"x\"\nenabled = \"SECRET-4\"", "`enabled` is not true or false"),
            ("url = \"u\"\nenv_http_headers = { A = \"SECRET-5\" }", "`env_http_headers` takes its value from the environment, which cannot be imported"),
            ("command = \"x\"\nenv_vars = [\"SECRET-6\"]", "`env_vars` takes its value from the environment, which cannot be imported"),
            ("cwd = \"/tmp\"", "needs a `command` or a `url`"),
        ] {
            assert_eq!(parse(&format!("[s]\n{bad}")).unwrap_err(), want);
        }
    }

    #[test]
    fn a_project_reads_agents_md_without_puppeteers_blocks_and_skips_duplicate_legacy_skills() {
        let base = tmp("project");
        let root = base.join("web app");
        let ctx = ScanContext { library_root: base.join("appdata") };
        let own = "<!-- puppeteer:rule:style START -->\nUse tabs.\n<!-- puppeteer:rule:style END -->";
        write(&root.join("AGENTS.md"), format!("# Team notes\n\n{own}\n\nShip on Fridays.\n"));
        write(&root.join(".agents/skills/pdf/SKILL.md"), "---\nname: pdf\n---\nPDFs.");
        // A legacy copy of `pdf` under another folder name: the same skill.
        write(&root.join(".codex/skills/pdf-old/SKILL.md"), "---\nname: pdf\n---\nPDFs.");
        // Codex reads prompts and its config from HOME only.
        write(&root.join(".codex/prompts/x.md"), "x");
        write(&root.join(".codex/config.toml"), "[mcp_servers.p]\ncommand = \"p\"\n");

        let report = Codex.scan(&root, Scope::Project, &ctx);
        assert_eq!(report.problems, []);
        assert_eq!(
            ids(&report),
            [
                (ImportKind::Rule, "rules:AGENTS.md".into(), "web-app-agents-md".into()),
                (ImportKind::Skill, "skills:.agents/skills/pdf".into(), "pdf".into()),
            ]
        );
        let Payload::Text(rule) = &report.candidates[0].payload else { panic!() };
        assert_eq!(rule, "# Team notes\n\nShip on Fridays.");
        assert_eq!(report.candidates[0].hash, crate::commands::rules::content_hash(rule));
        assert_eq!(Codex.detect(&root, Scope::Project, &ctx), [ImportKind::Rule, ImportKind::Skill]);

        // An AGENTS.md holding only what Puppeteer wrote has nothing to import.
        write(&root.join("AGENTS.md"), format!("{own}\n"));
        assert!(Codex.scan(&root, Scope::Project, &ctx).candidates.iter().all(|c| c.kind != ImportKind::Rule));
        // A block without its END marker is the user's text.
        write(&root.join("AGENTS.md"), "<!-- puppeteer:rule:x START -->\nmine\n");
        assert_eq!(agents_block::strip_all("<!-- puppeteer:rule:x START -->\nmine\n"), "<!-- puppeteer:rule:x START -->\nmine\n");
        assert_eq!(Codex.scan(&root, Scope::Project, &ctx).candidates[0].kind, ImportKind::Rule);
    }

    #[test]
    fn an_empty_folder_has_nothing_to_import() {
        let root = tmp("empty");
        let ctx = ScanContext { library_root: root.join("appdata") };
        for scope in [Scope::Global, Scope::Project] {
            assert_eq!(Codex.detect(&root, scope, &ctx), []);
            let report = Codex.scan(&root, scope, &ctx);
            assert!(report.candidates.is_empty() && report.problems.is_empty());
        }
    }
}
