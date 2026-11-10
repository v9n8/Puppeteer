//! The Skills "Allocation" matrix: every skill × project, one chip per engine in `SKILL_ENGINES`.
//!
//! A chip is read from the project's switch (`project_skills.enabled_*`), what sits at the engine's
//! folder in the project, the open legacy `sync_conflicts`, and whether a skill of the same name in
//! the engine's HOME folder wins over the project's.
//!
//! With `library.skills_global` on, a Global column comes first: one chip per engine with a HOME
//! folder, installed when Puppeteer's own link to the library sits there.

use crate::commands::project_links::{self, AtDest};
use crate::commands::skills::{self, SkillEngineDecl, SkillEngines, SKILL_ENGINES};
use crate::db::Db;
use crate::features;
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
    /// Global only: something that isn't Puppeteer's holds the name in the HOME folder (the user's
    /// own skill, another link) or the folder is a symlink. Left alone.
    Taken,
}

/// Gates the Global column and `set_skill_global`.
pub const GLOBAL_FEATURE: &str = "library.skills_global";

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

/// A Global chip, from what sits at `<HOME>/<engine's folder>/<name>`.
pub fn global_state(at: AtDest) -> ChipState {
    match at {
        AtDest::Nothing => ChipState::NotInstalled,
        AtDest::OwnLink => ChipState::Installed,
        AtDest::GitTracked | AtDest::Other => ChipState::Taken,
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
pub struct GlobalChip {
    pub skill_id: String,
    pub engine: &'static str,
    pub state: ChipState,
}

#[derive(Debug, Serialize)]
pub struct SkillAllocation {
    /// Chip columns, in order.
    pub engines: Vec<SkillEngineDecl>,
    pub projects: Vec<AllocationProject>,
    /// Every chip but the plain "not installed, switched off" ones, which are the default.
    pub chips: Vec<AllocationChip>,
    /// The Global column, `None` while `library.skills_global` is off. Every engine with a HOME
    /// folder, every chip but the plain "not installed" ones.
    pub global: Option<Vec<GlobalChip>>,
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

pub async fn skill_allocation(db: &Db, home: Option<PathBuf>, with_global: bool) -> Result<SkillAllocation, String> {
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
        sqlx::query("SELECT id, name, path FROM projects WHERE deleted_at IS NULL ORDER BY position, name COLLATE NOCASE")
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
        let mut global = Vec::new();
        for skill in &skills {
            let source = Path::new(&skill.source_path);
            for engine in SKILL_ENGINES {
                let at_home = match (&home, engine.home_dir) {
                    (Some(home), Some(dir)) => {
                        let dest = home.join(dir).join(&skill.name);
                        Some((project_links::at_dest(home, &dest, source), dest.exists()))
                    }
                    _ => None,
                };
                // Puppeteer's own global link is the same skill: it hides nothing.
                let shadowed = engine.home_wins
                    && at_home.as_ref().is_some_and(|(at, exists)| *exists && *at != AtDest::OwnLink);
                if let (true, Some((at, _))) = (with_global, at_home) {
                    let state = global_state(at);
                    if state != ChipState::NotInstalled {
                        global.push(GlobalChip { skill_id: skill.id.clone(), engine: engine.id, state });
                    }
                }
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
            global: with_global.then_some(global),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_skill_allocation(app: AppHandle, db: State<'_, Db>) -> Result<SkillAllocation, String> {
    let with_global = features::is_on(db.inner(), GLOBAL_FEATURE).await;
    skill_allocation(db.inner(), app.path().home_dir().ok(), with_global).await
}

/// Switch a skill on or off in one engine's global folder (the Global column's chip).
pub async fn switch_skill_global(db: &Db, home: &Path, skill_id: &str, engine: &str, on: bool) -> Result<(), String> {
    use sqlx::Row;
    features::require(db, GLOBAL_FEATURE).await?;
    let row = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(skill_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    skills::switch_global(home, engine, &row.get::<String, _>("name"), &row.get::<String, _>("source_path"), on)
}

/// Local-only: a remote desktop never writes into this machine's HOME.
#[tauri::command]
pub async fn set_skill_global(
    app: AppHandle,
    db: State<'_, Db>,
    skill_id: String,
    engine: String,
    on: bool,
) -> Result<(), String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    switch_skill_global(db.inner(), &home, &skill_id, &engine, on).await
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

        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }

        fn source(&self, skill: &str) -> PathBuf {
            self.dir.join("library").join(skill)
        }

        async fn global_on(&self) {
            crate::commands::settings::set_setting(&self.db, "feature.library.skills_global", "true").await.unwrap();
        }

        async fn switch_global(&self, skill: &str, engine: &str, on: bool) -> Result<(), String> {
            switch_skill_global(&self.db, &self.home(), skill, engine, on).await
        }

        async fn global(&self) -> Option<Vec<(String, &'static str, ChipState)>> {
            let allocation = skill_allocation(&self.db, Some(self.home()), true).await.unwrap();
            let mut out: Vec<_> = allocation.global?.into_iter().map(|c| (c.skill_id, c.engine, c.state)).collect();
            out.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            Some(out)
        }

        async fn chips(&self) -> Vec<(String, &'static str, bool, ChipState)> {
            let mut out: Vec<_> = skill_allocation(&self.db, Some(self.home()), false)
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

        assert_eq!(global_state(Nothing), ChipState::NotInstalled);
        assert_eq!(global_state(OwnLink), ChipState::Installed);
        assert_eq!(global_state(Other), ChipState::Taken);
        assert_eq!(global_state(GitTracked), ChipState::Taken);
    }

    #[test]
    fn engine_list_matches_the_link_folders() {
        let ids: Vec<_> = SKILL_ENGINES.iter().map(|e| e.id).collect();
        assert_eq!(ids, ["claude", "codex"]);
        let homes: Vec<_> = SKILL_ENGINES.iter().map(|e| (e.home_dir, e.home_wins)).collect();
        assert_eq!(homes, [(Some(".claude/skills"), true), (Some(".agents/skills"), false)]);
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

    #[tokio::test]
    async fn global_scope_stays_off_until_switched_on() {
        let fx = Fixture::new("global-off").await;
        assert_eq!(skill_allocation(&fx.db, Some(fx.home()), false).await.unwrap().global.map(|g| g.len()), None);
        assert_eq!(
            fx.switch_global("alpha", "claude", true).await,
            Err("feature_disabled: library.skills_global".into())
        );
        assert!(!fx.home().join(".claude").exists());
    }

    #[tokio::test]
    async fn global_chips_link_into_home_and_never_touch_the_users_skills() {
        let fx = Fixture::new("global").await;
        fx.global_on().await;
        assert_eq!(fx.global().await, Some(vec![]));

        fx.switch_global("alpha", "claude", true).await.unwrap();
        fx.switch_global("alpha", "codex", true).await.unwrap();
        let claude = fx.home().join(".claude/skills/alpha");
        let codex = fx.home().join(".agents/skills/alpha");
        assert_eq!(fs::read_link(&claude).unwrap(), fx.source("alpha"));
        assert_eq!(fs::read_link(&codex).unwrap(), fx.source("alpha"));

        // The user's own `beta` in HOME: reported as taken, kept byte for byte.
        let mine = fx.home().join(".claude/skills/beta");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("SKILL.md"), "mine").unwrap();
        let err = fx.switch_global("beta", "claude", true).await.unwrap_err();
        assert!(err.contains("isn't Puppeteer's"), "{err}");
        fx.switch_global("beta", "claude", false).await.unwrap();
        assert_eq!(fs::read_to_string(mine.join("SKILL.md")).unwrap(), "mine");

        assert_eq!(
            fx.global().await,
            Some(vec![
                ("alpha".into(), "claude", ChipState::Installed),
                ("alpha".into(), "codex", ChipState::Installed),
                ("beta".into(), "claude", ChipState::Taken),
            ])
        );

        // Our own global link is the same skill: the project's chip isn't shadowed by it.
        switch_skill_engine(&fx.db, "p1", "alpha", "claude", true).await.unwrap();
        assert_eq!(fx.chips().await, vec![("alpha".into(), "claude", true, ChipState::Installed)]);

        fx.switch_global("alpha", "claude", false).await.unwrap();
        assert!(!project_links::is_symlink(&claude));
        assert!(fx.source("alpha").join("SKILL.md").exists(), "the library keeps its source");
        assert!(fx.switch_global("alpha", "gemini", true).await.is_err());
        assert!(fx.switch_global("nope", "claude", true).await.is_err());
    }

    #[tokio::test]
    async fn a_symlinked_home_folder_is_never_written_through() {
        let fx = Fixture::new("global-link").await;
        fx.global_on().await;
        // `~/.agents/skills` pointing at `~/.claude/skills`.
        fs::create_dir_all(fx.home().join(".claude/skills")).unwrap();
        fs::create_dir_all(fx.home().join(".agents")).unwrap();
        std::os::unix::fs::symlink(fx.home().join(".claude/skills"), fx.home().join(".agents/skills")).unwrap();

        let err = fx.switch_global("alpha", "codex", true).await.unwrap_err();
        assert!(err.contains("symlinked folder"), "{err}");
        assert!(!fx.home().join(".claude/skills/alpha").exists());

        fx.switch_global("alpha", "claude", true).await.unwrap();
        // Every Codex chip: the folder itself is out of Puppeteer's hands.
        assert_eq!(
            fx.global().await,
            Some(vec![
                ("alpha".into(), "claude", ChipState::Installed),
                ("alpha".into(), "codex", ChipState::Taken),
                ("beta".into(), "codex", ChipState::Taken),
            ])
        );
        // Through the symlinked folder it looks like ours, but switching Codex off leaves it.
        fx.switch_global("alpha", "codex", false).await.unwrap();
        assert!(project_links::is_symlink(&fx.home().join(".claude/skills/alpha")));
    }

    #[tokio::test]
    async fn a_codex_skill_in_home_shadows_nothing() {
        let fx = Fixture::new("codex-home").await;
        switch_skill_engine(&fx.db, "p1", "alpha", "codex", true).await.unwrap();
        // Codex lists both same-named skills; only Claude lets the personal one win.
        fs::create_dir_all(fx.home().join(".agents/skills/alpha")).unwrap();
        assert_eq!(fx.chips().await, vec![("alpha".into(), "codex", true, ChipState::Installed)]);
    }

    #[tokio::test]
    async fn rename_and_delete_drop_only_puppeteers_global_links() {
        let fx = Fixture::new("global-unlink").await;
        fx.global_on().await;
        fx.switch_global("alpha", "claude", true).await.unwrap();
        // A user's own Codex `alpha`: not ours to drop.
        let mine = fx.home().join(".agents/skills/alpha");
        fs::create_dir_all(&mine).unwrap();
        let source = fx.source("alpha").to_string_lossy().to_string();

        assert_eq!(skills::unlink_global(&fx.home(), "alpha", &source).unwrap(), ["claude"]);
        assert!(!fx.home().join(".claude/skills/alpha").exists());
        assert!(mine.is_dir());
        assert!(skills::unlink_global(&fx.home(), "alpha", &source).unwrap().is_empty());
    }
}
