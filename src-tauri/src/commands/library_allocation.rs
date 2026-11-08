//! The Allocation matrix of the Rules, Agents and Commands libraries: every item × project, one
//! chip per engine the kind reaches, read the way the Skills matrix reads its chips
//! (`skill_allocation`).
//!
//! Agents and commands reach Claude only (`.claude/agents`, `.claude/commands`): a chip is the
//! project's link row plus what sits at the link. Rules reach Claude (`.claude/rules`) and Codex
//! (a block in AGENTS.md, or `.agents/skills` with `codex_mode: skill`), but the engines are the
//! rule's `target`, not a switch per project (`project_rules.target` is a copy of it): the screen
//! switches the whole rule with `apply_rule` / `remove_rule_from_project`.
//!
//! Read-only, so a remote desktop may call it; refused while the kind's prop is off.

use crate::commands::groups;
use crate::commands::library_items::{self, LibraryKind};
use crate::commands::project_links;
use crate::commands::rules::{self, content_hash, CodexMode};
use crate::commands::skill_allocation::{chip_state, AllocationProject, ChipState};
use crate::db::Db;
use crate::features;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationKind {
    Rule,
    Agent,
    Command,
}

impl AllocationKind {
    pub fn feature(self) -> &'static str {
        match self {
            AllocationKind::Rule => "library.rules",
            AllocationKind::Agent => LibraryKind::Agent.feature(),
            AllocationKind::Command => LibraryKind::Command.feature(),
        }
    }

    /// Chip columns, in order. An open list: another engine is one more entry and its folder.
    pub fn engines(self) -> &'static [&'static str] {
        match self {
            AllocationKind::Rule => &["claude", "codex"],
            AllocationKind::Agent | AllocationKind::Command => &["claude"],
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AllocationItem {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The engines it reaches once applied: a rule's target, Claude for agents and commands.
    pub engines: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ItemChip {
    pub item_id: String,
    pub project_id: String,
    pub engine: &'static str,
    pub enabled: bool,
    pub state: ChipState,
    /// Rules only: a group enabled in the project keeps the rule there, so switching it off
    /// directly would leave it in place.
    pub held: bool,
}

#[derive(Debug, Serialize)]
pub struct ItemAllocation {
    pub engines: Vec<&'static str>,
    pub projects: Vec<AllocationProject>,
    pub items: Vec<AllocationItem>,
    /// Every chip but the plain "not installed, switched off" ones, which are the default.
    pub chips: Vec<ItemChip>,
}

/// A Codex chip of a rule inlined into AGENTS.md: there is no link to look at, so the block is
/// compared with the hash recorded at the last apply or sync.
pub fn inline_block_state(enabled: bool, block_hash: Option<&str>, synced: Option<&str>, conflict: bool) -> ChipState {
    match (enabled, block_hash) {
        (false, _) => ChipState::NotInstalled,
        (true, Some(local)) if Some(local) == synced && !conflict => ChipState::Installed,
        _ => ChipState::Drifted,
    }
}

struct Item {
    id: String,
    name: String,
    description: String,
    source_path: String,
    engines: Vec<&'static str>,
}

/// What a project has of an item: the engines it is switched on for, and for rules the Codex
/// hash of the last sync and whether a group keeps it.
#[derive(Default)]
struct Applied {
    engines: Vec<&'static str>,
    synced_codex: Option<String>,
    held: bool,
}

struct ProjectRef {
    id: String,
    name: String,
    path: String,
}

fn rule_engines(target: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if rules::applies_claude(target) {
        out.push("claude");
    }
    if rules::applies_codex(target) {
        out.push("codex");
    }
    out
}

pub async fn item_allocation(db: &Db, kind: AllocationKind) -> Result<ItemAllocation, String> {
    features::require(db, kind.feature()).await?;
    let err = |e: sqlx::Error| e.to_string();

    let projects: Vec<ProjectRef> =
        sqlx::query("SELECT id, name, path FROM projects ORDER BY position, name COLLATE NOCASE")
            .fetch_all(db)
            .await
            .map_err(err)?
            .iter()
            .map(|r| ProjectRef { id: r.get("id"), name: r.get("name"), path: r.get("path") })
            .collect();

    let mut applied: HashMap<(String, String), Applied> = HashMap::new();
    let mut conflicts: HashSet<(String, String, String)> = HashSet::new();
    let items: Vec<Item> = match kind {
        AllocationKind::Rule => {
            let items = sqlx::query("SELECT id, name, description, target, source_path FROM rules ORDER BY name")
                .fetch_all(db)
                .await
                .map_err(err)?
                .iter()
                .map(|r| Item {
                    id: r.get("id"),
                    name: r.get("name"),
                    description: r.get("description"),
                    source_path: r.get("source_path"),
                    engines: rule_engines(&r.get::<String, _>("target")),
                })
                .collect();
            // While groups are off no group holds anything (`claimed_by_enabled_group`).
            let groups_on = features::is_on(db, groups::FEATURE).await;
            for r in sqlx::query(
                "SELECT pr.project_id, pr.rule_id, pr.target, pr.synced_hash_codex,
                        (SELECT COUNT(*) FROM rule_group_members m
                           JOIN project_rule_groups pg
                             ON pg.group_id = m.group_id AND pg.project_id = pr.project_id
                          WHERE m.rule_id = pr.rule_id) AS group_count
                 FROM project_rules pr",
            )
            .fetch_all(db)
            .await
            .map_err(err)?
            {
                applied.insert(
                    (r.get("project_id"), r.get("rule_id")),
                    Applied {
                        engines: rule_engines(&r.get::<String, _>("target")),
                        synced_codex: r.get("synced_hash_codex"),
                        held: groups_on && r.get::<i64, _>("group_count") > 0,
                    },
                );
            }
            conflicts = sqlx::query(
                "SELECT DISTINCT project_id, rule_id, engine FROM rule_sync_conflicts WHERE resolved = 0",
            )
            .fetch_all(db)
            .await
            .map_err(err)?
            .iter()
            .map(|r| (r.get("project_id"), r.get("rule_id"), r.get("engine")))
            .collect();
            items
        }
        AllocationKind::Agent | AllocationKind::Command => {
            let lk = if kind == AllocationKind::Agent { LibraryKind::Agent } else { LibraryKind::Command };
            let sql = format!("SELECT project_id, {} AS item_id FROM {}", lk.id_column(), lk.link_table());
            for r in sqlx::query(&sql).fetch_all(db).await.map_err(err)? {
                applied.insert(
                    (r.get("project_id"), r.get("item_id")),
                    Applied { engines: vec!["claude"], ..Applied::default() },
                );
            }
            library_items::list_items(db, lk)
                .await?
                .into_iter()
                .map(|i| Item {
                    id: i.id,
                    name: i.name,
                    description: i.description,
                    source_path: i.source_path,
                    engines: vec!["claude"],
                })
                .collect()
        }
    };

    // One lstat per chip, plus a git call where a real file sits: off the async runtime.
    tokio::task::spawn_blocking(move || {
        let none = Applied::default();
        let mut chips = Vec::new();
        for item in &items {
            let source = Path::new(&item.source_path);
            let codex_mode = match kind {
                AllocationKind::Rule => fs::read_to_string(source)
                    .map(|c| rules::parse_codex_mode(&c))
                    .unwrap_or(CodexMode::Inline),
                _ => CodexMode::Inline,
            };
            for project in &projects {
                let key = (project.id.clone(), item.id.clone());
                let found = applied.get(&key);
                let held = found.is_some_and(|a| a.held);
                let found = found.unwrap_or(&none);
                let root = Path::new(&project.path);
                for &engine in kind.engines() {
                    let enabled = found.engines.contains(&engine);
                    let conflict = conflicts.contains(&(key.0.clone(), key.1.clone(), engine.to_string()));
                    let at = |dest: &Path, target: &Path| project_links::at_dest(root, dest, target);
                    let state = match (kind, engine) {
                        (AllocationKind::Rule, "claude") => {
                            let dest = rules::claude_rule_path(&project.path, &item.name);
                            chip_state(enabled, at(&dest, source), conflict, false)
                        }
                        (AllocationKind::Rule, _) if codex_mode == CodexMode::Skill => {
                            let dest = rules::codex_skill_dest(&project.path, &item.name);
                            let skill = rules::codex_skill_source(&item.source_path, &item.name);
                            chip_state(enabled, at(&dest, &skill), conflict, false)
                        }
                        (AllocationKind::Rule, _) => {
                            let block = enabled
                                .then(|| rules::extract_agents_block(&rules::agents_path(&project.path), &item.name))
                                .flatten()
                                .map(|b| content_hash(b.trim_end_matches('\n')));
                            inline_block_state(enabled, block.as_deref(), found.synced_codex.as_deref(), conflict)
                        }
                        (AllocationKind::Agent, _) | (AllocationKind::Command, _) => {
                            let lk = if kind == AllocationKind::Agent { LibraryKind::Agent } else { LibraryKind::Command };
                            let dest = library_items::project_dest(lk, &project.path, &item.name);
                            chip_state(enabled, at(&dest, source), false, false)
                        }
                    };
                    if enabled || state != ChipState::NotInstalled {
                        chips.push(ItemChip {
                            item_id: item.id.clone(),
                            project_id: project.id.clone(),
                            engine,
                            enabled,
                            state,
                            held,
                        });
                    }
                }
            }
        }
        ItemAllocation {
            engines: kind.engines().to_vec(),
            projects: projects.into_iter().map(|p| AllocationProject { id: p.id, name: p.name }).collect(),
            items: items
                .into_iter()
                .map(|i| AllocationItem { id: i.id, name: i.name, description: i.description, engines: i.engines })
                .collect(),
            chips,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_library_allocation(db: State<'_, Db>, kind: AllocationKind) -> Result<ItemAllocation, String> {
    item_allocation(db.inner(), kind).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::commands::library_items::{apply_item, create_item, CreateItemPayload};
    use crate::commands::rules::apply_rule_to_project;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;
    use std::process::Command;
    use uuid::Uuid;

    struct Fixture {
        dir: PathBuf,
        db: Db,
    }

    impl Fixture {
        /// In-memory DB with real migrations, project `p1` (a git repo) and the libraries, all
        /// under one temp folder.
        async fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-lib-alloc-{}-{}", label, Uuid::new_v4()));
            let project = dir.join("p1");
            fs::create_dir_all(&project).unwrap();
            fs::create_dir_all(dir.join("lib/rules")).unwrap();
            assert!(Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap().success());
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'one', ?, '2026-01-01')")
                .bind(project.to_string_lossy().to_string())
                .execute(&db)
                .await
                .unwrap();
            Fixture { dir, db }
        }

        fn project(&self) -> PathBuf {
            self.dir.join("p1")
        }

        async fn add_rule(&self, name: &str, target: &str, content: &str) {
            let source = self.dir.join("lib/rules").join(format!("{name}.md"));
            fs::write(&source, content).unwrap();
            sqlx::query(
                "INSERT INTO rules (id, name, description, target, source_path, updated_at)
                 VALUES (?, ?, 'd', ?, ?, '2026-01-01')",
            )
            .bind(name)
            .bind(name)
            .bind(target)
            .bind(source.to_string_lossy().to_string())
            .execute(&self.db)
            .await
            .unwrap();
        }

        async fn create(&self, kind: LibraryKind, name: &str) -> String {
            let payload = CreateItemPayload { name: name.into(), description: "d".into(), content: "body".into() };
            create_item(&self.db, kind, &self.dir.join("lib").join(kind.table()), payload).await.unwrap().id
        }

        async fn chips(&self, kind: AllocationKind) -> Vec<(String, &'static str, bool, ChipState, bool)> {
            let mut out: Vec<_> = item_allocation(&self.db, kind)
                .await
                .unwrap()
                .chips
                .into_iter()
                .map(|c| (c.item_id, c.engine, c.enabled, c.state, c.held))
                .collect();
            out.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            out
        }

        fn git_add(&self, path: &str) {
            let add = Command::new("git").arg("-C").arg(self.project()).args(["add", path]).status().unwrap();
            assert!(add.success());
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn inline_block_state_compares_with_the_last_sync() {
        use ChipState::*;
        assert_eq!(inline_block_state(false, Some("h"), Some("h"), false), NotInstalled);
        assert_eq!(inline_block_state(true, Some("h"), Some("h"), false), Installed);
        assert_eq!(inline_block_state(true, Some("h"), Some("h"), true), Drifted);
        assert_eq!(inline_block_state(true, Some("edited"), Some("h"), false), Drifted);
        assert_eq!(inline_block_state(true, None, Some("h"), false), Drifted);
    }

    #[test]
    fn engine_columns_per_kind() {
        assert_eq!(AllocationKind::Rule.engines(), ["claude", "codex"]);
        assert_eq!(AllocationKind::Agent.engines(), ["claude"]);
        assert_eq!(AllocationKind::Command.engines(), ["claude"]);
        assert_eq!(rule_engines("both"), ["claude", "codex"]);
        assert_eq!(rule_engines("codex"), ["codex"]);
        assert_eq!(AllocationKind::Rule.feature(), "library.rules");
        assert_eq!(AllocationKind::Command.feature(), "library.commands");
    }

    #[tokio::test]
    async fn rule_chips_read_the_link_and_the_agents_block() {
        let fx = Fixture::new("rules").await;
        fx.add_rule("style", "both", "---\nname: style\n---\n\nUse tabs.\n").await;
        fx.add_rule("ci", "codex", "---\nname: ci\n---\n\nPin actions.\n").await;
        assert!(fx.chips(AllocationKind::Rule).await.is_empty(), "nothing applied yet");

        let allocation = item_allocation(&fx.db, AllocationKind::Rule).await.unwrap();
        let engines: Vec<_> = allocation.items.iter().map(|i| (i.name.as_str(), i.engines.clone())).collect();
        assert_eq!(engines, [("ci", vec!["codex"]), ("style", vec!["claude", "codex"])]);

        apply_rule_to_project(&fx.db, "p1", "style", true).await.unwrap();
        assert_eq!(
            fx.chips(AllocationKind::Rule).await,
            [
                ("style".into(), "claude", true, ChipState::Installed, false),
                ("style".into(), "codex", true, ChipState::Installed, false),
            ]
        );

        // The block edited in AGENTS.md, the Claude link replaced by a copy.
        let agents = fx.project().join("AGENTS.md");
        fs::write(&agents, fs::read_to_string(&agents).unwrap().replace("Use tabs.", "Use spaces.")).unwrap();
        let claude = fx.project().join(".claude/rules/style.md");
        fs::remove_file(&claude).unwrap();
        fs::write(&claude, "copy").unwrap();
        assert_eq!(
            fx.chips(AllocationKind::Rule).await,
            [
                ("style".into(), "claude", true, ChipState::Drifted, false),
                ("style".into(), "codex", true, ChipState::Drifted, false),
            ]
        );
    }

    #[tokio::test]
    async fn rule_chips_follow_skill_mode_git_and_groups() {
        let fx = Fixture::new("rules-more").await;
        fx.add_rule("ci", "codex", "---\nname: ci\ncodex_mode: skill\n---\n\nPin actions.\n").await;
        fx.add_rule("team", "both", "Team rule.\n").await;
        apply_rule_to_project(&fx.db, "p1", "ci", true).await.unwrap();

        // The repo commits its own `.claude/rules/team.md`; the rule isn't applied here.
        let team = fx.project().join(".claude/rules/team.md");
        fs::create_dir_all(team.parent().unwrap()).unwrap();
        fs::write(&team, "ours").unwrap();
        fx.git_add(".claude/rules/team.md");

        assert_eq!(
            fx.chips(AllocationKind::Rule).await,
            [
                ("ci".into(), "codex", true, ChipState::Installed, false),
                ("team".into(), "claude", false, ChipState::GitManaged, false),
            ]
        );

        // An enabled group holding `ci` keeps it: the screen won't switch it off.
        for sql in [
            "INSERT INTO rule_groups (id, name, description, position, created_at, updated_at) VALUES ('g', 'g', '', 0, '', '')",
            "INSERT INTO rule_group_members (group_id, rule_id, position) VALUES ('g', 'ci', 0)",
            "INSERT INTO project_rule_groups (project_id, group_id, enabled_at) VALUES ('p1', 'g', '')",
        ] {
            sqlx::query(sql).execute(&fx.db).await.unwrap();
        }
        assert_eq!(fx.chips(AllocationKind::Rule).await[0], ("ci".into(), "codex", true, ChipState::Installed, true));
        // With groups switched off, no group holds anything.
        crate::commands::settings::set_setting(&fx.db, "feature.library.groups", "false").await.unwrap();
        assert!(!fx.chips(AllocationKind::Rule).await[0].4);
    }

    #[tokio::test]
    async fn agent_and_command_chips_read_their_link() {
        let fx = Fixture::new("items").await;
        let reviewer = fx.create(LibraryKind::Agent, "reviewer").await;
        let deploy = fx.create(LibraryKind::Command, "deploy").await;
        apply_item(&fx.db, LibraryKind::Agent, "p1", &reviewer).await.unwrap();
        assert_eq!(
            fx.chips(AllocationKind::Agent).await,
            [(reviewer.clone(), "claude", true, ChipState::Installed, false)]
        );
        assert!(fx.chips(AllocationKind::Command).await.is_empty(), "each kind apart");

        // Replaced by the user's own file: drifted.
        let dest = fx.project().join(".claude/agents/reviewer.md");
        fs::remove_file(&dest).unwrap();
        fs::write(&dest, "mine").unwrap();
        assert_eq!(fx.chips(AllocationKind::Agent).await[0].3, ChipState::Drifted);

        // The repo's own command of that name, never applied: git-managed.
        let own = fx.project().join(".claude/commands/deploy.md");
        fs::create_dir_all(own.parent().unwrap()).unwrap();
        fs::write(&own, "team").unwrap();
        fx.git_add(".claude/commands/deploy.md");
        assert_eq!(
            fx.chips(AllocationKind::Command).await,
            [(deploy, "claude", false, ChipState::GitManaged, false)]
        );
    }

    #[tokio::test]
    async fn refused_while_the_kind_is_off() {
        let fx = Fixture::new("off").await;
        crate::commands::settings::set_setting(&fx.db, "feature.library.agents", "false").await.unwrap();
        assert_eq!(
            item_allocation(&fx.db, AllocationKind::Agent).await.err(),
            Some("feature_disabled: library.agents".into())
        );
        assert!(item_allocation(&fx.db, AllocationKind::Command).await.is_ok());
    }
}
