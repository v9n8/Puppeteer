//! Bulk switch from the Skills Allocation matrix: several skills (picked one by one, or a skill
//! group's members) switched on or off for some engines across several projects at once.
//!
//! `plan_skill_bulk` reads the project folders and lists the files the switch would create or
//! remove, touching nothing. `apply_skill_bulk` reads them again and runs that plan through
//! `switch_skill_engine`, the matrix chip's own path, so the matrix afterwards shows what the
//! preview listed.
//!
//! Bulk is more careful than a single chip: where something that isn't Puppeteer's link sits in a
//! folder the switch would write to (an edited copy, a link elsewhere, a symlinked folder), the
//! skill is skipped for that project instead of having it moved to the backups. Skills the repo
//! commits to git are never switched. A group only picks the skills: the projects don't enable
//! the group itself (that stays the project's "enable group").

use crate::commands::groups::{self, GroupKind};
use crate::commands::project_links::{self, AtDest};
use crate::commands::projects::switch_skill_engine;
use crate::commands::rules::{ApplyAllOutcome, ApplyFailure};
use crate::commands::skills::{self, SkillEngines, SKILL_ENGINES};
use crate::db::Db;
use crate::features;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillBulkRequest {
    #[serde(default)]
    pub skill_ids: Vec<String>,
    /// Adds every member of this skill group (needs `library.groups`).
    #[serde(default)]
    pub group_id: Option<String>,
    pub project_ids: Vec<String>,
    /// Ids from `SKILL_ENGINES`.
    pub engines: Vec<String>,
    pub on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    /// A link to the library where nothing is yet.
    Create,
    /// Puppeteer's link (or a copy-era `.codex/skills` folder, which goes to the backups unless
    /// it is provably unedited).
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub action: FileAction,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BulkChange {
    pub project_id: String,
    pub project_name: String,
    pub skill_id: String,
    pub skill_name: String,
    /// Engines switched, in `SKILL_ENGINES` order.
    pub engines: Vec<&'static str>,
    /// Empty when only the switch changes (the link is already there, or nothing is to remove).
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The repo commits its own skill of that name.
    GitManaged,
    /// Something that isn't Puppeteer's link sits where the switch would write.
    Occupied,
    ProjectMissing,
    SourceMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BulkSkip {
    pub project_id: String,
    pub project_name: String,
    pub skill_id: String,
    pub skill_name: String,
    /// `None` when the whole project is skipped.
    pub engine: Option<&'static str>,
    pub reason: SkipReason,
    pub path: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct SkillBulkPlan {
    pub changes: Vec<BulkChange>,
    pub skipped: Vec<BulkSkip>,
    /// Skill × project pairs already as asked.
    pub unchanged: usize,
}

/// One engine's folder for a skill in a project, as read from disk.
#[derive(Debug)]
pub struct EngineDest {
    pub engine: &'static str,
    pub path: String,
    pub at: AtDest,
}

/// A skill × project pair before the switch.
#[derive(Debug)]
pub struct PairState {
    pub project_id: String,
    pub project_name: String,
    pub skill_id: String,
    pub skill_name: String,
    /// `project_skills.enabled_*`.
    pub enabled: SkillEngines,
    /// Engines with a synced hash: what Puppeteer linked last and removes when switched off.
    pub installed: SkillEngines,
    /// One per `SKILL_ENGINES`, in order.
    pub dests: Vec<EngineDest>,
    /// A copy-era `.codex/skills/<name>` that any Codex write cleans up.
    pub legacy_codex: Option<String>,
    pub project_missing: bool,
    pub source_missing: bool,
}

/// Plan one pair: switch `wanted` engines to `on`, then list what `write_skill_artifacts` would do
/// for every engine the project ends up with (a switch re-links the others too).
pub fn plan_pair(pair: &PairState, wanted: &[&str], on: bool, plan: &mut SkillBulkPlan) {
    let skip = |engine: Option<&'static str>, reason: SkipReason, path: Option<&str>| BulkSkip {
        project_id: pair.project_id.clone(),
        project_name: pair.project_name.clone(),
        skill_id: pair.skill_id.clone(),
        skill_name: pair.skill_name.clone(),
        engine,
        reason,
        path: path.map(str::to_string),
    };
    let skipped_before = plan.skipped.len();
    let mut after = pair.enabled;
    let mut switched = Vec::new();
    for dest in pair.dests.iter().filter(|d| wanted.contains(&d.engine)) {
        let enabled = pair.enabled.get(dest.engine);
        if dest.at == AtDest::GitTracked {
            if on || enabled {
                plan.skipped.push(skip(Some(dest.engine), SkipReason::GitManaged, Some(&dest.path)));
            }
            continue;
        }
        // Switched on but the link is missing or replaced: switching it on again repairs it.
        if enabled != on || (on && dest.at != AtDest::OwnLink) {
            // `dests` only holds engines of `SKILL_ENGINES`, which `set` knows.
            let _ = after.set(dest.engine, on);
            switched.push(dest.engine);
        }
    }
    if switched.is_empty() {
        if plan.skipped.len() == skipped_before {
            plan.unchanged += 1;
        }
        return;
    }
    if pair.project_missing {
        plan.skipped.push(skip(None, SkipReason::ProjectMissing, None));
        return;
    }
    if pair.source_missing && after != SkillEngines::NONE {
        plan.skipped.push(skip(None, SkipReason::SourceMissing, None));
        return;
    }

    let mut files = Vec::new();
    let mut occupied = Vec::new();
    for dest in &pair.dests {
        let action = if after.get(dest.engine) {
            match dest.at {
                AtDest::Nothing => Some(FileAction::Create),
                AtDest::Other => {
                    occupied.push(dest);
                    None
                }
                AtDest::OwnLink | AtDest::GitTracked => None,
            }
        } else if pair.installed.get(dest.engine) {
            match dest.at {
                AtDest::OwnLink => Some(FileAction::Remove),
                AtDest::Other => {
                    occupied.push(dest);
                    None
                }
                AtDest::Nothing | AtDest::GitTracked => None,
            }
        } else {
            None
        };
        if let Some(action) = action {
            files.push(FileChange { action, path: dest.path.clone() });
        }
    }
    if !occupied.is_empty() {
        for dest in occupied {
            plan.skipped.push(skip(Some(dest.engine), SkipReason::Occupied, Some(&dest.path)));
        }
        return;
    }
    if let (true, Some(legacy)) = (after.codex || pair.installed.codex, &pair.legacy_codex) {
        files.push(FileChange { action: FileAction::Remove, path: legacy.clone() });
    }
    plan.changes.push(BulkChange {
        project_id: pair.project_id.clone(),
        project_name: pair.project_name.clone(),
        skill_id: pair.skill_id.clone(),
        skill_name: pair.skill_name.clone(),
        engines: switched,
        files,
    });
}

/// Read the requested skills × projects from the DB and their folders, and plan the switch.
pub async fn plan_bulk(db: &Db, req: &SkillBulkRequest) -> Result<SkillBulkPlan, String> {
    use sqlx::Row;
    let err = |e: sqlx::Error| e.to_string();
    for engine in &req.engines {
        skills::skill_engine(engine)?;
    }
    let wanted: Vec<&'static str> =
        SKILL_ENGINES.iter().map(|e| e.id).filter(|id| req.engines.iter().any(|w| w == id)).collect();

    let mut skill_ids: HashSet<String> = req.skill_ids.iter().cloned().collect();
    if let Some(group_id) = &req.group_id {
        features::require(db, groups::FEATURE).await?;
        skill_ids.extend(groups::member_ids(db, GroupKind::Skill, group_id).await?);
    }
    let project_ids: HashSet<&String> = req.project_ids.iter().collect();

    let skills: Vec<(String, String, String)> = sqlx::query("SELECT id, name, source_path FROM skills ORDER BY name")
        .fetch_all(db)
        .await
        .map_err(err)?
        .iter()
        .map(|r| (r.get("id"), r.get("name"), r.get("source_path")))
        .filter(|(id, _, _)| skill_ids.contains(id))
        .collect();
    let projects: Vec<(String, String, String)> =
        sqlx::query("SELECT id, name, path FROM projects WHERE deleted_at IS NULL ORDER BY position, name COLLATE NOCASE")
            .fetch_all(db)
            .await
            .map_err(err)?
            .iter()
            .map(|r| (r.get("id"), r.get("name"), r.get("path")))
            .filter(|(id, _, _)| project_ids.contains(id))
            .collect();
    let rows: HashMap<(String, String), (SkillEngines, SkillEngines)> = sqlx::query(
        "SELECT project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude, synced_hash_codex
         FROM project_skills",
    )
    .fetch_all(db)
    .await
    .map_err(err)?
    .iter()
    .map(|r| {
        let installed = SkillEngines {
            claude: r.get::<Option<String>, _>("synced_hash_claude").is_some(),
            codex: r.get::<Option<String>, _>("synced_hash_codex").is_some(),
        };
        ((r.get("project_id"), r.get("skill_id")), (SkillEngines::from_row(r), installed))
    })
    .collect();

    let on = req.on;
    // An lstat per folder, plus a git call where a real folder sits: off the async runtime.
    tokio::task::spawn_blocking(move || {
        let mut plan = SkillBulkPlan::default();
        for (project_id, project_name, path) in &projects {
            let root = Path::new(path);
            for (skill_id, skill_name, source_path) in &skills {
                let source = Path::new(source_path);
                let (enabled, installed) = rows
                    .get(&(project_id.clone(), skill_id.clone()))
                    .copied()
                    .unwrap_or((SkillEngines::NONE, SkillEngines::NONE));
                let dests = SKILL_ENGINES
                    .iter()
                    .map(|e| {
                        let dest = root.join(e.project_dir).join(skill_name);
                        let at = project_links::at_dest(root, &dest, source);
                        EngineDest { engine: e.id, path: dest.to_string_lossy().into_owned(), at }
                    })
                    .collect();
                let legacy = skills::legacy_codex_skill_dir(path, skill_name);
                // Behind a symlinked `.codex` the apply leaves the folder alone, so it isn't listed.
                let legacy_codex = (project_links::parent_is_real(root, &legacy) == Ok(true)
                    && matches!(project_links::at_dest(root, &legacy, source), AtDest::OwnLink | AtDest::Other))
                    .then(|| legacy.to_string_lossy().into_owned());
                let pair = PairState {
                    project_id: project_id.clone(),
                    project_name: project_name.clone(),
                    skill_id: skill_id.clone(),
                    skill_name: skill_name.clone(),
                    enabled,
                    installed,
                    dests,
                    legacy_codex,
                    project_missing: !root.is_dir(),
                    source_missing: !source.exists(),
                };
                plan_pair(&pair, &wanted, on, &mut plan);
            }
        }
        plan
    })
    .await
    .map_err(|e| e.to_string())
}

/// Plan again (the folders may have changed since the preview) and switch every planned engine.
pub async fn apply_bulk(db: &Db, req: &SkillBulkRequest) -> Result<ApplyAllOutcome, String> {
    let plan = plan_bulk(db, req).await?;
    let mut applied = 0;
    let mut failures = Vec::new();
    for change in &plan.changes {
        let mut result = Ok(());
        for engine in &change.engines {
            result = switch_skill_engine(db, &change.project_id, &change.skill_id, engine, req.on).await;
            if result.is_err() {
                break;
            }
        }
        match result {
            Ok(()) => applied += 1,
            Err(error) => failures.push(ApplyFailure {
                project_id: change.project_id.clone(),
                project_name: change.project_name.clone(),
                error: format!("{}: {}", change.skill_name, error),
            }),
        }
    }
    Ok(ApplyAllOutcome { applied, failures })
}

/// Read-only preview of a bulk switch.
#[tauri::command]
pub async fn plan_skill_bulk(db: State<'_, Db>, request: SkillBulkRequest) -> Result<SkillBulkPlan, String> {
    plan_bulk(db.inner(), &request).await
}

#[tauri::command]
pub async fn apply_skill_bulk(db: State<'_, Db>, request: SkillBulkRequest) -> Result<ApplyAllOutcome, String> {
    apply_bulk(db.inner(), &request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(enabled: (bool, bool), installed: (bool, bool), at: (AtDest, AtDest)) -> PairState {
        PairState {
            project_id: "p".into(),
            project_name: "P".into(),
            skill_id: "s".into(),
            skill_name: "x".into(),
            enabled: SkillEngines { claude: enabled.0, codex: enabled.1 },
            installed: SkillEngines { claude: installed.0, codex: installed.1 },
            dests: vec![
                EngineDest { engine: "claude", path: "/p/.claude/skills/x".into(), at: at.0 },
                EngineDest { engine: "codex", path: "/p/.agents/skills/x".into(), at: at.1 },
            ],
            legacy_codex: None,
            project_missing: false,
            source_missing: false,
        }
    }

    fn run(pair: &PairState, wanted: &[&str], on: bool) -> SkillBulkPlan {
        let mut plan = SkillBulkPlan::default();
        plan_pair(pair, wanted, on, &mut plan);
        plan
    }

    fn files(plan: &SkillBulkPlan) -> Vec<(FileAction, &str)> {
        plan.changes.iter().flat_map(|c| c.files.iter().map(|f| (f.action, f.path.as_str()))).collect()
    }

    fn reasons(plan: &SkillBulkPlan) -> Vec<(Option<&str>, SkipReason)> {
        plan.skipped.iter().map(|s| (s.engine, s.reason)).collect()
    }

    use AtDest::{GitTracked, Nothing, Other, OwnLink};
    use FileAction::{Create, Remove};

    #[test]
    fn switching_on_links_where_nothing_is() {
        let plan = run(&pair((false, false), (false, false), (Nothing, Nothing)), &["claude", "codex"], true);
        assert_eq!(plan.changes[0].engines, ["claude", "codex"]);
        assert_eq!(files(&plan), [(Create, "/p/.claude/skills/x"), (Create, "/p/.agents/skills/x")]);
        assert_eq!((plan.skipped.len(), plan.unchanged), (0, 0));
    }

    #[test]
    fn a_pair_already_as_asked_is_unchanged() {
        let plan = run(&pair((true, false), (true, false), (OwnLink, Nothing)), &["claude"], true);
        assert_eq!((plan.changes.len(), plan.unchanged), (0, 1));
        let plan = run(&pair((true, false), (true, false), (OwnLink, Nothing)), &["codex"], false);
        assert_eq!((plan.changes.len(), plan.unchanged), (0, 1));
    }

    #[test]
    fn switching_off_removes_only_puppeteers_link() {
        let plan = run(&pair((true, true), (true, true), (OwnLink, OwnLink)), &["codex"], false);
        assert_eq!(plan.changes[0].engines, ["codex"]);
        assert_eq!(files(&plan), [(Remove, "/p/.agents/skills/x")]);
        // Switched on but never linked (the link went missing): only the switch changes.
        let plan = run(&pair((true, false), (true, false), (Nothing, Nothing)), &["claude"], false);
        assert_eq!(plan.changes[0].engines, ["claude"]);
        assert!(files(&plan).is_empty());
    }

    #[test]
    fn switching_on_repairs_a_missing_link_and_relinks_the_other_engine() {
        // Claude on but its link deleted; asking for Codex re-links Claude too.
        let plan = run(&pair((true, false), (true, false), (Nothing, Nothing)), &["claude", "codex"], true);
        assert_eq!(plan.changes[0].engines, ["claude", "codex"]);
        assert_eq!(files(&plan), [(Create, "/p/.claude/skills/x"), (Create, "/p/.agents/skills/x")]);
        let plan = run(&pair((true, false), (true, false), (Nothing, Nothing)), &["codex"], true);
        assert_eq!(plan.changes[0].engines, ["codex"]);
        assert_eq!(files(&plan), [(Create, "/p/.claude/skills/x"), (Create, "/p/.agents/skills/x")]);
    }

    #[test]
    fn git_managed_engines_are_never_switched() {
        let plan = run(&pair((false, false), (false, false), (GitTracked, Nothing)), &["claude", "codex"], true);
        assert_eq!(reasons(&plan), [(Some("claude"), SkipReason::GitManaged)]);
        assert_eq!(plan.changes[0].engines, ["codex"]);
        assert_eq!(files(&plan), [(Create, "/p/.agents/skills/x")]);
        // Off and already off: nothing to report.
        let plan = run(&pair((false, false), (false, false), (GitTracked, Nothing)), &["claude"], false);
        assert_eq!((plan.skipped.len(), plan.unchanged), (0, 1));
        // Only a git-managed engine asked for: skipped, not "unchanged".
        let plan = run(&pair((false, false), (false, false), (GitTracked, Nothing)), &["claude"], true);
        assert_eq!((plan.changes.len(), plan.skipped.len(), plan.unchanged), (0, 1, 0));
    }

    #[test]
    fn something_else_in_the_way_skips_the_pair() {
        // An edited copy where the Codex link would go.
        let plan = run(&pair((false, false), (false, false), (Nothing, Other)), &["claude", "codex"], true);
        assert!(plan.changes.is_empty());
        assert_eq!(reasons(&plan), [(Some("codex"), SkipReason::Occupied)]);
        assert_eq!(plan.skipped[0].path.as_deref(), Some("/p/.agents/skills/x"));
        // Switching Codex off would re-link Claude over the copy there.
        let plan = run(&pair((true, true), (true, true), (Other, OwnLink)), &["codex"], false);
        assert_eq!(reasons(&plan), [(Some("claude"), SkipReason::Occupied)]);
        // Switching off what was installed, with something else there now.
        let plan = run(&pair((true, false), (true, false), (Other, Nothing)), &["claude"], false);
        assert_eq!(reasons(&plan), [(Some("claude"), SkipReason::Occupied)]);
    }

    #[test]
    fn a_missing_project_or_source_is_skipped() {
        let mut p = pair((false, false), (false, false), (Nothing, Nothing));
        p.project_missing = true;
        assert_eq!(reasons(&run(&p, &["claude"], true)), [(None, SkipReason::ProjectMissing)]);
        let mut p = pair((false, false), (false, false), (Nothing, Nothing));
        p.source_missing = true;
        assert_eq!(reasons(&run(&p, &["claude"], true)), [(None, SkipReason::SourceMissing)]);
        // Switching everything off needs no source: the dangling link still goes.
        let mut p = pair((true, false), (true, false), (OwnLink, Nothing));
        p.source_missing = true;
        assert_eq!(files(&run(&p, &["claude"], false)), [(Remove, "/p/.claude/skills/x")]);
    }

    #[test]
    fn a_codex_write_cleans_up_the_copy_era_folder() {
        let mut p = pair((false, false), (false, false), (Nothing, Nothing));
        p.legacy_codex = Some("/p/.codex/skills/x".into());
        assert_eq!(files(&run(&p, &["codex"], true)), [(Create, "/p/.agents/skills/x"), (Remove, "/p/.codex/skills/x")]);
        // Claude alone never touches it.
        assert_eq!(files(&run(&p, &["claude"], true)), [(Create, "/p/.claude/skills/x")]);
    }
}

#[cfg(all(test, unix))]
mod fs_tests {
    use super::*;
    use crate::commands::skill_allocation::{skill_allocation, ChipState};
    use crate::commands::skills::{claude_skill_dir, codex_skill_dir};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    struct Fixture {
        dir: PathBuf,
        db: Db,
    }

    impl Fixture {
        /// Skills `alpha`, `beta` (group `g` holds `alpha`) and projects `p1`, `p2` (git repos).
        async fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-bulk-{}-{}", label, uuid::Uuid::new_v4()));
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            for (i, id) in ["p1", "p2"].into_iter().enumerate() {
                let project = dir.join(id);
                fs::create_dir_all(&project).unwrap();
                assert!(Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap().success());
                sqlx::query("INSERT INTO projects (id, name, path, position, created_at) VALUES (?, ?, ?, ?, '2026-01-01')")
                    .bind(id)
                    .bind(id)
                    .bind(project.to_string_lossy().to_string())
                    .bind(i as i64)
                    .execute(&db)
                    .await
                    .unwrap();
            }
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
            sqlx::query("INSERT INTO skill_groups (id, name, description, position, created_at, updated_at) VALUES ('g', 'g', '', 0, '', '')")
                .execute(&db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO skill_group_members (group_id, skill_id, position) VALUES ('g', 'alpha', 0)")
                .execute(&db)
                .await
                .unwrap();
            Fixture { dir, db }
        }

        fn project(&self, id: &str) -> String {
            self.dir.join(id).to_string_lossy().to_string()
        }

        /// Installed chips of the matrix, as (skill, project, engine).
        async fn installed(&self) -> Vec<(String, String, &'static str)> {
            let mut out: Vec<_> = skill_allocation(&self.db, None, false)
                .await
                .unwrap()
                .chips
                .into_iter()
                .filter(|c| c.state == ChipState::Installed)
                .map(|c| (c.skill_id, c.project_id, c.engine))
                .collect();
            out.sort();
            out
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn request(skills: &[&str], group: Option<&str>, engines: &[&str], on: bool) -> SkillBulkRequest {
        SkillBulkRequest {
            skill_ids: skills.iter().map(|s| s.to_string()).collect(),
            group_id: group.map(str::to_string),
            project_ids: vec!["p1".into(), "p2".into()],
            engines: engines.iter().map(|s| s.to_string()).collect(),
            on,
        }
    }

    /// Every link the plan creates.
    fn created(plan: &SkillBulkPlan) -> Vec<String> {
        plan.changes
            .iter()
            .flat_map(|c| c.files.iter().filter(|f| f.action == FileAction::Create).map(|f| f.path.clone()))
            .collect()
    }

    #[tokio::test]
    async fn the_preview_lists_what_the_apply_writes_and_the_matrix_follows() {
        let fx = Fixture::new("apply").await;
        let req = request(&["beta"], Some("g"), &["claude", "codex"], true);
        let plan = plan_bulk(&fx.db, &req).await.unwrap();
        assert_eq!(plan.changes.len(), 4, "2 skills × 2 projects");
        let paths = created(&plan);
        assert_eq!(paths.len(), 8);
        assert!(paths.iter().all(|p| !Path::new(p).exists()), "the preview writes nothing");

        let outcome = apply_bulk(&fx.db, &req).await.unwrap();
        assert_eq!((outcome.applied, outcome.failures.len()), (4, 0));
        assert!(paths.iter().all(|p| project_links::is_symlink(Path::new(p))));
        let mut want = Vec::new();
        for skill in ["alpha", "beta"] {
            for project in ["p1", "p2"] {
                for engine in ["claude", "codex"] {
                    want.push((skill.to_string(), project.to_string(), engine));
                }
            }
        }
        assert_eq!(fx.installed().await, want);
        // Applied, the same request has nothing left to do.
        let again = plan_bulk(&fx.db, &req).await.unwrap();
        assert_eq!((again.changes.len(), again.unchanged), (0, 4));

        // Codex off for the group only: its links go, Claude's stay.
        let off = request(&[], Some("g"), &["codex"], false);
        let plan = plan_bulk(&fx.db, &off).await.unwrap();
        let removed: Vec<_> = plan.changes.iter().flat_map(|c| c.files.iter().map(|f| (f.action, f.path.clone()))).collect();
        assert_eq!(
            removed,
            [
                (FileAction::Remove, codex_skill_dir(&fx.project("p1"), "alpha").to_string_lossy().into_owned()),
                (FileAction::Remove, codex_skill_dir(&fx.project("p2"), "alpha").to_string_lossy().into_owned()),
            ]
        );
        apply_bulk(&fx.db, &off).await.unwrap();
        assert!(!codex_skill_dir(&fx.project("p1"), "alpha").exists());
        assert!(project_links::is_symlink(&claude_skill_dir(&fx.project("p1"), "alpha")));
        assert_eq!(fx.installed().await.len(), 6);

        // Everything off: the skills leave both projects.
        apply_bulk(&fx.db, &request(&["alpha", "beta"], None, &["claude", "codex"], false)).await.unwrap();
        assert!(fx.installed().await.is_empty());
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_skills").fetch_one(&fx.db).await.unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn the_users_files_are_skipped_and_kept() {
        let fx = Fixture::new("skip").await;
        // p1: an edited copy of `alpha` for Codex; p2: the repo commits its own Claude `alpha`.
        let copy = codex_skill_dir(&fx.project("p1"), "alpha");
        fs::create_dir_all(&copy).unwrap();
        fs::write(copy.join("SKILL.md"), "mine").unwrap();
        let team = claude_skill_dir(&fx.project("p2"), "alpha");
        fs::create_dir_all(&team).unwrap();
        fs::write(team.join("SKILL.md"), "team").unwrap();
        let add = Command::new("git").arg("-C").arg(fx.project("p2")).args(["add", ".claude/skills/alpha"]).status().unwrap();
        assert!(add.success());

        let req = request(&["alpha"], None, &["claude", "codex"], true);
        let plan = plan_bulk(&fx.db, &req).await.unwrap();
        let skipped: Vec<_> = plan.skipped.iter().map(|s| (s.project_id.as_str(), s.engine, s.reason)).collect();
        assert_eq!(
            skipped,
            [("p1", Some("codex"), SkipReason::Occupied), ("p2", Some("claude"), SkipReason::GitManaged)]
        );
        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].engines, ["codex"]);

        apply_bulk(&fx.db, &req).await.unwrap();
        assert_eq!(fs::read_to_string(copy.join("SKILL.md")).unwrap(), "mine");
        assert_eq!(fs::read_to_string(team.join("SKILL.md")).unwrap(), "team");
        assert_eq!(fx.installed().await, [("alpha".to_string(), "p2".to_string(), "codex")]);
    }

    #[tokio::test]
    async fn a_copy_era_folder_behind_a_symlinked_parent_is_left_out() {
        let fx = Fixture::new("legacy-link").await;
        // `.codex` shared from elsewhere: what sits in it isn't the project's, and the apply leaves it.
        let shared = fx.dir.join("shared");
        fs::create_dir_all(shared.join("skills/alpha")).unwrap();
        std::os::unix::fs::symlink(&shared, Path::new(&fx.project("p1")).join(".codex")).unwrap();
        let req = SkillBulkRequest { project_ids: vec!["p1".into()], ..request(&["alpha"], None, &["codex"], true) };
        let plan = plan_bulk(&fx.db, &req).await.unwrap();
        let files: Vec<_> = plan.changes.iter().flat_map(|c| c.files.iter().map(|f| (f.action, f.path.clone()))).collect();
        assert_eq!(files, [(FileAction::Create, codex_skill_dir(&fx.project("p1"), "alpha").to_string_lossy().into_owned())]);
        apply_bulk(&fx.db, &req).await.unwrap();
        assert!(shared.join("skills/alpha").is_dir());
    }

    #[tokio::test]
    async fn a_group_needs_its_prop_and_engines_must_be_known() {
        let fx = Fixture::new("gate").await;
        crate::commands::settings::set_setting(&fx.db, "feature.library.groups", "false").await.unwrap();
        assert_eq!(
            plan_bulk(&fx.db, &request(&[], Some("g"), &["claude"], true)).await,
            Err("feature_disabled: library.groups".into())
        );
        assert!(apply_bulk(&fx.db, &request(&[], Some("g"), &["claude"], true)).await.is_err());
        assert!(!claude_skill_dir(&fx.project("p1"), "alpha").exists());
        // Picking skills one by one doesn't need groups.
        assert_eq!(plan_bulk(&fx.db, &request(&["alpha"], None, &["claude"], true)).await.unwrap().changes.len(), 2);
        assert!(plan_bulk(&fx.db, &request(&["alpha"], None, &["gemini"], true)).await.is_err());
    }
}
