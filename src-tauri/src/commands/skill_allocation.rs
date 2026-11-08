//! The Skills "Allocation" matrix: every skill × project, one chip per engine in `SKILL_ENGINES`.
//!
//! A chip is read from the project's switch (`project_skills.enabled_*`), what sits at the engine's
//! folder in the project, the open legacy `sync_conflicts`, and whether a skill of the same name in
//! the engine's HOME folder wins over the project's.

use crate::commands::project_links::{self, AtDest};
use crate::commands::skills::{SkillEngineDecl, SkillEngines, SKILL_ENGINES};
use crate::db::Db;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChipState {
    /// Switched on, and the project links to the library.
    Installed,
    /// Switched on, but the project holds something else there (an edited copy, a link elsewhere,
    /// nothing at all) or an open conflict is left.
    Drifted,
    /// The repo's own skill of that name, committed to git: read-only for Puppeteer.
    GitManaged,
    /// Installed, but a skill of the same name in HOME is the one the engine uses.
    Shadowed,
    NotInstalled,
}

pub fn chip_state(enabled: bool, at: AtDest, open_conflict: bool, shadowed: bool) -> ChipState {
    match at {
        AtDest::GitTracked => ChipState::GitManaged,
        _ if !enabled => ChipState::NotInstalled,
        AtDest::OwnLink if !open_conflict => {
            if shadowed {
                ChipState::Shadowed
            } else {
                ChipState::Installed
            }
        }
        _ => ChipState::Drifted,
    }
}

#[derive(Debug, Serialize)]
pub struct AllocationProject {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct AllocationChip {
    pub skill_id: String,
    pub project_id: String,
    pub engine: &'static str,
    pub enabled: bool,
    pub state: ChipState,
}

#[derive(Debug, Serialize)]
pub struct SkillAllocation {
    /// Chip columns, in order.
    pub engines: Vec<SkillEngineDecl>,
    pub projects: Vec<AllocationProject>,
    /// Every chip but the plain "not installed, switched off" ones, which are the default.
    pub chips: Vec<AllocationChip>,
}

struct SkillRef {
    id: String,
    name: String,
    source_path: String,
}

struct ProjectRef {
    id: String,
    name: String,
    path: String,
}

pub async fn skill_allocation(db: &Db, home: Option<PathBuf>) -> Result<SkillAllocation, String> {
    use sqlx::Row;
    let err = |e: sqlx::Error| e.to_string();

    let skills: Vec<SkillRef> = sqlx::query("SELECT id, name, source_path FROM skills ORDER BY name")
        .fetch_all(db)
        .await
        .map_err(err)?
        .iter()
        .map(|r| SkillRef { id: r.get("id"), name: r.get("name"), source_path: r.get("source_path") })
        .collect();
    let projects: Vec<ProjectRef> =
        sqlx::query("SELECT id, name, path FROM projects ORDER BY position, name COLLATE NOCASE")
            .fetch_all(db)
            .await
            .map_err(err)?
            .iter()
            .map(|r| ProjectRef { id: r.get("id"), name: r.get("name"), path: r.get("path") })
            .collect();
    let switches: HashMap<(String, String), SkillEngines> =
        sqlx::query("SELECT project_id, skill_id, enabled_claude, enabled_codex FROM project_skills")
            .fetch_all(db)
            .await
            .map_err(err)?
            .iter()
            .map(|r| ((r.get("project_id"), r.get("skill_id")), SkillEngines::from_row(r)))
            .collect();
    let conflicts: HashSet<(String, String, String)> = sqlx::query(
        "SELECT DISTINCT project_id, skill_id, engine FROM sync_conflicts WHERE resolved = 0",
    )
    .fetch_all(db)
    .await
    .map_err(err)?
    .iter()
    .map(|r| (r.get("project_id"), r.get("skill_id"), r.get("engine")))
    .collect();

    // One lstat per chip, plus a git call where a real folder sits: off the async runtime.
    tokio::task::spawn_blocking(move || {
        let mut chips = Vec::new();
        for skill in &skills {
            let source = Path::new(&skill.source_path);
            for engine in SKILL_ENGINES {
                let shadowed = match (&home, engine.home_dir) {
                    (Some(home), Some(dir)) => home.join(dir).join(&skill.name).exists(),
                    _ => false,
                };
                for project in &projects {
                    let key = (project.id.clone(), skill.id.clone());
                    let enabled = switches.get(&key).is_some_and(|e| e.get(engine.id));
                    let root = Path::new(&project.path);
                    let at = project_links::at_dest(root, &root.join(engine.project_dir).join(&skill.name), source);
                    let conflict = conflicts.contains(&(key.0, key.1, engine.id.to_string()));
                    let state = chip_state(enabled, at, conflict, shadowed);
                    if enabled || state != ChipState::NotInstalled {
                        chips.push(AllocationChip {
                            skill_id: skill.id.clone(),
                            project_id: project.id.clone(),
                            engine: engine.id,
                            enabled,
                            state,
                        });
                    }
                }
            }
        }
        SkillAllocation {
            engines: SKILL_ENGINES.to_vec(),
            projects: projects.into_iter().map(|p| AllocationProject { id: p.id, name: p.name }).collect(),
            chips,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_skill_allocation(app: AppHandle, db: State<'_, Db>) -> Result<SkillAllocation, String> {
    skill_allocation(db.inner(), app.path().home_dir().ok()).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::commands::projects::switch_skill_engine;
    use crate::commands::skills::{claude_skill_dir, codex_skill_dir};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::fs;
    use std::process::Command;

    struct Fixture {
        dir: PathBuf,
        db: Db,
    }

    impl Fixture {
        /// A library with skills `alpha` and `beta`, project `p1` (a git repo) and an empty HOME,
        /// all under one temp folder.
        async fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-alloc-{}-{}", label, uuid::Uuid::new_v4()));
            let project = dir.join("p1");
            fs::create_dir_all(&project).unwrap();
            fs::create_dir_all(dir.join("home")).unwrap();
            let git = Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap();
            assert!(git.success());

            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'one', ?, '2026-01-01')")
                .bind(project.to_string_lossy().to_string())
                .execute(&db)
                .await
                .unwrap();
            for name in ["alpha", "beta"] {
                let source = dir.join("library").join(name);
                fs::create_dir_all(&source).unwrap();
                fs::write(source.join("SKILL.md"), format!("---\nname: {name}\ndescription: d\n---\n")).unwrap();
                sqlx::query(
                    "INSERT INTO skills (id, name, description, target, source_path, updated_at)
                     VALUES (?, ?, 'd', 'both', ?, '2026-01-01')",
                )
                .bind(name)
                .bind(name)
                .bind(source.to_string_lossy().to_string())
                .execute(&db)
                .await
                .unwrap();
            }
            Fixture { dir, db }
        }

        fn project(&self) -> String {
            self.dir.join("p1").to_string_lossy().to_string()
        }

        async fn chips(&self) -> Vec<(String, &'static str, bool, ChipState)> {
            let mut out: Vec<_> = skill_allocation(&self.db, Some(self.dir.join("home")))
                .await
                .unwrap()
                .chips
                .into_iter()
                .map(|c| (c.skill_id, c.engine, c.enabled, c.state))
                .collect();
            out.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            out
        }

        async fn switches(&self, skill: &str) -> Option<(bool, bool)> {
            use sqlx::Row;
            sqlx::query("SELECT enabled_claude, enabled_codex FROM project_skills WHERE project_id = 'p1' AND skill_id = ?")
                .bind(skill)
                .fetch_optional(&self.db)
                .await
                .unwrap()
                .map(|r| (r.get::<i64, _>("enabled_claude") != 0, r.get::<i64, _>("enabled_codex") != 0))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn chip_state_covers_the_five_states() {
        use AtDest::*;
        assert_eq!(chip_state(true, OwnLink, false, false), ChipState::Installed);
        assert_eq!(chip_state(true, OwnLink, false, true), ChipState::Shadowed);
        assert_eq!(chip_state(true, OwnLink, true, false), ChipState::Drifted);
        assert_eq!(chip_state(true, Other, false, false), ChipState::Drifted);
        assert_eq!(chip_state(true, Nothing, false, false), ChipState::Drifted);
        assert_eq!(chip_state(true, GitTracked, false, false), ChipState::GitManaged);
        assert_eq!(chip_state(false, GitTracked, false, false), ChipState::GitManaged);
        assert_eq!(chip_state(false, Other, true, true), ChipState::NotInstalled);
        assert_eq!(chip_state(false, Nothing, false, false), ChipState::NotInstalled);
    }

    #[test]
    fn engine_list_matches_the_link_folders() {
        let ids: Vec<_> = SKILL_ENGINES.iter().map(|e| e.id).collect();
        assert_eq!(ids, ["claude", "codex"]);
        let root = Path::new("/p");
        assert_eq!(root.join(SKILL_ENGINES[0].project_dir).join("x"), claude_skill_dir("/p", "x"));
        assert_eq!(root.join(SKILL_ENGINES[1].project_dir).join("x"), codex_skill_dir("/p", "x"));
        for e in SKILL_ENGINES {
            let mut engines = SkillEngines::NONE;
            engines.set(e.id, true).unwrap();
            assert!(engines.get(e.id));
        }
        let mut engines = SkillEngines::NONE;
        assert!(engines.set("gemini", true).is_err());
    }

    #[tokio::test]
    async fn a_chip_switches_one_engine_in_one_project() {
        let fx = Fixture::new("switch").await;
        assert!(fx.chips().await.is_empty(), "nothing installed yet");

        // `both` is the skill's default, but a chip asks for exactly one engine.
        switch_skill_engine(&fx.db, "p1", "alpha", "codex", true).await.unwrap();
        assert_eq!(fx.switches("alpha").await, Some((false, true)));
        assert!(project_links::is_symlink(&codex_skill_dir(&fx.project(), "alpha")));
        assert!(!claude_skill_dir(&fx.project(), "alpha").exists());
        assert_eq!(fx.chips().await, vec![("alpha".into(), "codex", true, ChipState::Installed)]);

        switch_skill_engine(&fx.db, "p1", "alpha", "claude", true).await.unwrap();
        assert_eq!(fx.switches("alpha").await, Some((true, true)));
        switch_skill_engine(&fx.db, "p1", "alpha", "codex", false).await.unwrap();
        assert_eq!(fx.switches("alpha").await, Some((true, false)));
        assert!(!codex_skill_dir(&fx.project(), "alpha").exists());

        // The last engine off drops the skill from the project.
        switch_skill_engine(&fx.db, "p1", "alpha", "claude", false).await.unwrap();
        assert_eq!(fx.switches("alpha").await, None);
        assert!(!claude_skill_dir(&fx.project(), "alpha").exists());
        assert!(fx.chips().await.is_empty());

        assert!(switch_skill_engine(&fx.db, "p1", "alpha", "gemini", true).await.is_err());
    }

    #[tokio::test]
    async fn a_group_keeps_the_skill_when_every_engine_is_off() {
        let fx = Fixture::new("group").await;
        sqlx::query("INSERT INTO skill_groups (id, name, description, position, created_at, updated_at) VALUES ('g', 'g', '', 0, '', '')")
            .execute(&fx.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO skill_group_members (group_id, skill_id, position) VALUES ('g', 'alpha', 0)")
            .execute(&fx.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_skill_groups (project_id, group_id, enabled_at) VALUES ('p1', 'g', '')")
            .execute(&fx.db)
            .await
            .unwrap();
        switch_skill_engine(&fx.db, "p1", "alpha", "claude", true).await.unwrap();
        switch_skill_engine(&fx.db, "p1", "alpha", "claude", false).await.unwrap();
        assert_eq!(fx.switches("alpha").await, Some((false, false)));
        assert!(!claude_skill_dir(&fx.project(), "alpha").exists());
    }

    #[tokio::test]
    async fn chips_read_drift_git_and_shadowing_from_disk() {
        let fx = Fixture::new("states").await;
        let project = fx.project();
        switch_skill_engine(&fx.db, "p1", "alpha", "claude", true).await.unwrap();
        switch_skill_engine(&fx.db, "p1", "alpha", "codex", true).await.unwrap();

        // Codex: the link replaced by an edited copy.
        let codex = codex_skill_dir(&project, "alpha");
        fs::remove_file(&codex).unwrap();
        fs::create_dir_all(&codex).unwrap();
        fs::write(codex.join("SKILL.md"), "edited").unwrap();
        // Claude: a personal skill of the same name in HOME wins.
        fs::create_dir_all(fx.dir.join("home/.claude/skills/alpha")).unwrap();
        // beta: the repo commits its own `.claude/skills/beta`, never switched on here.
        let beta = claude_skill_dir(&project, "beta");
        fs::create_dir_all(&beta).unwrap();
        fs::write(beta.join("SKILL.md"), "team").unwrap();
        let add = Command::new("git").arg("-C").arg(&project).args(["add", ".claude/skills/beta"]).status().unwrap();
        assert!(add.success());

        assert_eq!(
            fx.chips().await,
            vec![
                ("alpha".into(), "claude", true, ChipState::Shadowed),
                ("alpha".into(), "codex", true, ChipState::Drifted),
                ("beta".into(), "claude", false, ChipState::GitManaged),
            ]
        );

        // An open conflict left from the copy era marks the chip too.
        fs::remove_dir_all(fx.dir.join("home/.claude")).unwrap();
        sqlx::query(
            "INSERT INTO sync_conflicts (id, project_id, skill_id, detected_at, local_hash, source_hash, engine)
             VALUES ('c', 'p1', 'alpha', '', '', '', 'claude')",
        )
        .execute(&fx.db)
        .await
        .unwrap();
        assert_eq!(fx.chips().await[0], ("alpha".into(), "claude", true, ChipState::Drifted));
    }
}
