use super::resolve::{detach_paths, materialized, target_without, Materialized};
use super::*;
use crate::commands::project_links::{is_symlink, project_backup_dir};
use crate::commands::projects::apply_skill_with_engines;
use crate::commands::rules::apply_rule_to_project;
use crate::commands::skills::{skill_hash, SkillEngines};
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::BTreeMap;
use std::process::Command;

const CLAUDE: SkillEngines = SkillEngines { claude: true, codex: false };
const CODEX: SkillEngines = SkillEngines { claude: false, codex: true };

/// A git project `p1` and a library with skill `ui-review` and rule `commit-rules`, under one
/// temp folder; an in-memory DB with the real migrations.
struct Fx {
    dir: PathBuf,
    db: Db,
}

impl Fx {
    async fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("puppeteer-syncconf-{}-{}", label, uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("p1")).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        assert!(Command::new("git").arg("-C").arg(dir.join("p1")).args(["init", "-q"]).status().unwrap().success());
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'one', ?, '2026-01-01')")
            .bind(dir.join("p1").to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        let skill = dir.join("lib/skills/ui-review");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: ui-review\n---\ncentral\n").unwrap();
        sqlx::query("INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES ('s1', 'ui-review', 'd', 'claude', ?, '')")
            .bind(skill.to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        fs::create_dir_all(dir.join("lib/rules")).unwrap();
        fs::write(dir.join("lib/rules/commit-rules.md"), "---\nname: commit-rules\n---\n\nShort subjects.\n").unwrap();
        sqlx::query("INSERT INTO rules (id, name, description, target, source_path, updated_at) VALUES ('r1', 'commit-rules', 'd', 'codex', ?, '')")
            .bind(dir.join("lib/rules/commit-rules.md").to_string_lossy().to_string())
            .execute(&db)
            .await
            .unwrap();
        Fx { dir, db }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.join("p1").join(rel)
    }

    /// Link the skill for `engines`, then put a real folder with `text` where `engine`'s link is.
    async fn skill_copy(&self, engines: SkillEngines, engine_dir: &str, text: &str) {
        apply_skill_with_engines(&self.db, "p1", "s1", true, Some(engines)).await.unwrap();
        let at = self.p(engine_dir).join("ui-review");
        assert!(is_symlink(&at));
        fs::remove_file(&at).unwrap();
        fs::create_dir_all(&at).unwrap();
        fs::write(at.join("SKILL.md"), text).unwrap();
    }

    async fn open_rows(&self, table: &str) -> Vec<(String, String, String)> {
        let col = if table == "sync_conflicts" { "skill_id" } else { "rule_id" };
        sqlx::query_as(&format!("SELECT project_id, {col}, engine FROM {table} WHERE resolved = 0 ORDER BY engine"))
            .fetch_all(&self.db)
            .await
            .unwrap()
    }

    async fn scan(&self) -> ScanReport {
        scan(&self.db, Some("p1")).await.unwrap()
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        let _ = fs::remove_dir_all(project_backup_dir(&self.dir.join("p1")));
    }
}

fn key(table_item: &str, engine: &str) -> (String, String, String) {
    ("p1".into(), table_item.into(), engine.into())
}

#[test]
fn classify_follows_the_three_versions() {
    let kept = vec![("l".to_string(), "c".to_string())];
    assert_eq!(classify(None, "c", None, &[]), Verdict::NotACopy);
    assert_eq!(classify(Some("c"), "c", None, &[]), Verdict::Matches);
    assert_eq!(classify(Some("b"), "c", Some("b"), &[]), Verdict::Unedited);
    assert_eq!(classify(Some("l"), "c", Some("b"), &kept), Verdict::Kept);
    assert_eq!(classify(Some("l"), "c2", Some("b"), &kept), Verdict::Conflict);
    assert_eq!(classify(Some("l2"), "c", None, &kept), Verdict::Conflict);
}

#[test]
fn materialized_paths_map_to_items() {
    let m = |p: &str| materialized(Path::new(p));
    assert_eq!(m(".claude/skills/a"), Some(Materialized::SkillLink { name: "a".into(), engine: "claude" }));
    assert_eq!(m(".agents/skills/a"), Some(Materialized::SkillLink { name: "a".into(), engine: "codex" }));
    assert_eq!(m(".claude/rules/r.md"), Some(Materialized::ClaudeRule("r".into())));
    assert_eq!(m("AGENTS.md"), Some(Materialized::AgentsMd));
    for other in [".claude/rules/.md", ".claude/commands/x.md", ".claude/skills/a/SKILL.md", "../AGENTS.md", "x/AGENTS.md"] {
        assert_eq!(m(other), None, "{other}");
    }
    assert_eq!(target_without("both", "claude"), Some("codex"));
    assert_eq!(target_without("both", "codex"), Some("claude"));
    assert_eq!(target_without("codex", "codex"), None);
    assert_eq!(target_without("claude", "codex"), Some("claude"));
}

#[tokio::test]
async fn ac1_ac5_edited_copy_conflicts_once_and_closes_when_fixed() {
    let fx = Fx::new("ac1").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;

    let r = fx.scan().await;
    assert_eq!((r.checked, r.open.len(), r.opened), (1, 1, 1));
    assert_eq!(fx.open_rows("sync_conflicts").await, vec![key("s1", "claude")]);
    // A second scan keeps the same row.
    let again = fx.scan().await;
    assert_eq!((again.open.len(), again.opened), (1, 0));
    assert_eq!(again.open[0].conflict_id, r.open[0].conflict_id);

    // AC-5: the user makes it match the central copy by hand.
    fs::copy(fx.dir.join("lib/skills/ui-review/SKILL.md"), fx.p(".claude/skills/ui-review/SKILL.md")).unwrap();
    let fixed = fx.scan().await;
    assert!(fixed.open.is_empty());
    assert_eq!(fixed.closed.len(), 1);
    assert_eq!((fixed.closed[0].name.as_str(), fixed.closed[0].reason), ("ui-review", CloseReason::InSync));
    assert!(fx.open_rows("sync_conflicts").await.is_empty());
}

#[tokio::test]
async fn ac2_own_link_tracked_empty_and_symlinked_parent_are_not_copies() {
    let fx = Fx::new("ac2").await;
    apply_skill_with_engines(&fx.db, "p1", "s1", true, Some(SkillEngines { claude: true, codex: true })).await.unwrap();
    assert!(fx.scan().await.open.is_empty(), "own links");

    // Empty: nothing where the link goes.
    fs::remove_file(fx.p(".claude/skills/ui-review")).unwrap();
    // A folder reached through a symlinked parent isn't Puppeteer's to manage.
    fs::remove_file(fx.p(".agents/skills/ui-review")).unwrap();
    fs::remove_dir(fx.p(".agents/skills")).unwrap();
    let elsewhere = fx.dir.join("elsewhere/ui-review");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("SKILL.md"), "other").unwrap();
    std::os::unix::fs::symlink(fx.dir.join("elsewhere"), fx.p(".agents/skills")).unwrap();
    let r = fx.scan().await;
    assert!(r.open.is_empty() && r.errors.is_empty(), "{:?}", r.errors);

    // Git tracked: the repo's own.
    fs::create_dir_all(fx.p(".claude/skills/ui-review")).unwrap();
    fs::write(fx.p(".claude/skills/ui-review/SKILL.md"), "team").unwrap();
    let add = Command::new("git").arg("-C").arg(fx.p("")).args(["add", ".claude/skills/ui-review"]).status().unwrap();
    assert!(add.success());
    assert!(fx.scan().await.open.is_empty(), "git tracked");
}

#[tokio::test]
async fn ac3_matching_or_unedited_copy_is_not_a_conflict() {
    let fx = Fx::new("ac3").await;
    let central = fs::read_to_string(fx.dir.join("lib/skills/ui-review/SKILL.md")).unwrap();
    fx.skill_copy(CLAUDE, ".claude/skills", &central).await;
    assert!(fx.scan().await.open.is_empty(), "same as central");

    // The central copy moved on; the project's is what was installed (the base).
    fs::write(fx.dir.join("lib/skills/ui-review/SKILL.md"), "newer central").unwrap();
    assert!(fx.scan().await.open.is_empty(), "unedited old copy");
}

#[tokio::test]
async fn ac4_edited_agents_md_block_is_a_codex_rule_conflict() {
    let fx = Fx::new("ac4").await;
    apply_rule_to_project(&fx.db, "p1", "r1", true).await.unwrap();
    assert!(fx.scan().await.open.is_empty());
    let agents = fs::read_to_string(fx.p("AGENTS.md")).unwrap();
    fs::write(fx.p("AGENTS.md"), agents.replace("Short subjects.", "Long subjects.")).unwrap();

    let r = fx.scan().await;
    assert_eq!(r.open.len(), 1);
    assert_eq!((r.open[0].kind, r.open[0].engine.as_str()), (ItemKind::Rule, "codex"));
    assert_eq!(fx.open_rows("rule_sync_conflicts").await, vec![key("r1", "codex")]);

    // AC-8 for a block, AC-9 both bodies.
    let list = list_for_project(&fx.db, "p1").await.unwrap();
    assert_eq!(list[0].project_path, fx.p("AGENTS.md").display().to_string());
    assert_eq!(list[0].block.as_deref(), Some("commit-rules"));
    let sides = read_sides(&fx.db, ItemKind::Rule, &list[0].id).await.unwrap();
    assert_eq!(sides.project.content.as_deref(), Some("Long subjects."));
    assert_eq!(sides.central.content.as_deref(), Some("Short subjects.\n"));
    assert_eq!(sides.project.hash.as_deref(), Some(list[0].local_hash.as_str()));
    assert_eq!(sides.central.hash.as_deref(), Some(list[0].source_hash.as_str()));
}

#[tokio::test]
async fn ac6_kept_local_stays_closed_until_a_side_changes() {
    let fx = Fx::new("ac6").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    let id = fx.scan().await.open[0].conflict_id.clone().unwrap();
    resolve_skill(&fx.db, &id, false).await.unwrap();
    assert!(fx.scan().await.open.is_empty(), "kept pair");
    assert_eq!(fs::read_to_string(fx.p(".claude/skills/ui-review/SKILL.md")).unwrap(), "mine\n");

    fs::write(fx.p(".claude/skills/ui-review/SKILL.md"), "mine, edited again\n").unwrap();
    assert_eq!(fx.scan().await.open.len(), 1, "project side changed");
    let id = fx.scan().await.open[0].conflict_id.clone().unwrap();
    resolve_skill(&fx.db, &id, false).await.unwrap();
    fs::write(fx.dir.join("lib/skills/ui-review/SKILL.md"), "central moved on").unwrap();
    assert_eq!(fx.scan().await.open.len(), 1, "central side changed");
}

#[tokio::test]
async fn ac7_duplicates_fold_and_orphans_close() {
    let fx = Fx::new("ac7").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    for (id, at, engine) in [("d1", "2026-01-01", "claude"), ("d2", "2026-01-03", "claude"), ("d3", "2026-01-02", "claude"), ("o1", "2026-01-01", "codex")] {
        sqlx::query("INSERT INTO sync_conflicts (id, project_id, skill_id, engine, detected_at, local_hash, source_hash) VALUES (?, 'p1', 's1', ?, ?, 'x', 'y')")
            .bind(id)
            .bind(engine)
            .bind(at)
            .execute(&fx.db)
            .await
            .unwrap();
    }
    let r = fx.scan().await;
    assert_eq!(r.open.len(), 1);
    assert_eq!(r.open[0].conflict_id.as_deref(), Some("d2"), "the newest is kept");
    let mut closed: Vec<_> = r.closed.iter().map(|c| (c.conflict_id.as_str(), c.reason)).collect();
    closed.sort_by_key(|c| c.0);
    assert_eq!(closed, vec![("d1", CloseReason::Duplicate), ("d3", CloseReason::Duplicate), ("o1", CloseReason::NotApplied)]);
    assert_eq!(fx.open_rows("sync_conflicts").await, vec![key("s1", "claude")]);
}

#[tokio::test]
async fn ac8_ac9_list_and_read_both_sides() {
    let fx = Fx::new("ac8").await;
    fx.skill_copy(CODEX, ".agents/skills", "mine\n").await;
    fx.scan().await;
    let list = list_for_project(&fx.db, "p1").await.unwrap();
    assert_eq!(list.len(), 1);
    let c = &list[0];
    assert_eq!((c.kind, c.name.as_str(), c.engine.as_str()), (ItemKind::Skill, "ui-review", "codex"));
    assert_eq!(c.project_path, fx.p(".agents/skills/ui-review/SKILL.md").display().to_string());
    assert_eq!(c.central_path, fx.dir.join("lib/skills/ui-review/SKILL.md").display().to_string());
    assert!(c.project_mtime.is_some() && c.central_mtime.is_some() && c.block.is_none());
    assert!(!c.detected_at.is_empty());

    let sides = read_sides(&fx.db, ItemKind::Skill, &c.id).await.unwrap();
    assert_eq!(sides.project.content.as_deref(), Some("mine\n"));
    assert_eq!(sides.project.hash, Some(skill_hash("mine\n")));
    assert!(sides.central.content.as_deref().is_some_and(|t| t.contains("central")));

    // One side unreadable: its error only, the other side still read.
    fs::write(fx.p(".agents/skills/ui-review/SKILL.md"), "x".repeat(1024 * 1024 + 1)).unwrap();
    let sides = read_sides(&fx.db, ItemKind::Skill, &c.id).await.unwrap();
    assert!(sides.project.content.is_none() && sides.project.error.as_deref().is_some_and(|e| e.contains("too large")));
    assert!(sides.central.content.is_some() && sides.central.error.is_none());
    fs::write(fx.p(".agents/skills/ui-review/SKILL.md"), [0xff, 0xfe, 0x00]).unwrap();
    let sides = read_sides(&fx.db, ItemKind::Skill, &c.id).await.unwrap();
    assert!(sides.project.error.as_deref().is_some_and(|e| e.contains("UTF-8")));
    assert!(read_sides(&fx.db, ItemKind::Rule, &c.id).await.is_err(), "wrong kind");
}

fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Option<std::time::SystemTime>, Vec<u8>)> {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == ".git"))
        .map(|e| {
            let meta = fs::symlink_metadata(e.path()).unwrap();
            let bytes = if meta.is_file() { fs::read(e.path()).unwrap() } else { Vec::new() };
            (e.path().to_path_buf(), (meta.modified().ok(), bytes))
        })
        .collect()
}

#[tokio::test]
async fn ac10_a_scan_changes_no_file() {
    let fx = Fx::new("ac10").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    apply_rule_to_project(&fx.db, "p1", "r1", true).await.unwrap();
    let agents = fs::read_to_string(fx.p("AGENTS.md")).unwrap();
    fs::write(fx.p("AGENTS.md"), agents.replace("Short", "Long")).unwrap();
    let before = snapshot(&fx.dir);
    let r = fx.scan().await;
    assert_eq!(r.open.len(), 2);
    assert_eq!(snapshot(&fx.dir), before);
}

#[tokio::test]
async fn ac11_trashed_project_is_skipped() {
    let fx = Fx::new("ac11").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    fx.scan().await;
    sqlx::query("UPDATE projects SET deleted_at = '2026-01-02' WHERE id = 'p1'").execute(&fx.db).await.unwrap();
    let r = scan(&fx.db, None).await.unwrap();
    assert_eq!((r.projects, r.checked), (0, 0));
    assert!(list_for_project(&fx.db, "p1").await.unwrap().is_empty());
}

fn backups(fx: &Fx, rel: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(project_backup_dir(&fx.dir.join("p1"))) else { return Vec::new() };
    entries.flatten().map(|e| e.path().join(rel)).filter(|p| p.exists()).collect()
}

#[tokio::test]
async fn ac12_overwrite_relinks_and_backs_up_skills_for_both_engines() {
    for (engines, dir, label) in [(CLAUDE, ".claude/skills", "ac12-claude"), (CODEX, ".agents/skills", "ac12-codex")] {
        let fx = Fx::new(label).await;
        fx.skill_copy(engines, dir, "mine\n").await;
        let id = fx.scan().await.open[0].conflict_id.clone().unwrap();
        resolve_skill(&fx.db, &id, true).await.unwrap();
        assert!(is_symlink(&fx.p(dir).join("ui-review")), "{label}");
        let kept = backups(&fx, &format!("{dir}/ui-review/SKILL.md"));
        assert_eq!(kept.len(), 1, "{label}");
        assert_eq!(fs::read_to_string(&kept[0]).unwrap(), "mine\n");
        let r = fx.scan().await;
        assert!(r.open.is_empty() && r.closed.is_empty(), "{label}");
    }
}

#[tokio::test]
async fn ac12_overwrite_backs_up_rule_copies_and_agents_md() {
    let fx = Fx::new("ac12-rule").await;
    sqlx::query("UPDATE rules SET target = 'both' WHERE id = 'r1'").execute(&fx.db).await.unwrap();
    apply_rule_to_project(&fx.db, "p1", "r1", true).await.unwrap();
    fs::remove_file(fx.p(".claude/rules/commit-rules.md")).unwrap();
    fs::write(fx.p(".claude/rules/commit-rules.md"), "mine").unwrap();
    let agents = fs::read_to_string(fx.p("AGENTS.md")).unwrap();
    fs::write(fx.p("AGENTS.md"), agents.replace("Short", "Long")).unwrap();
    let open = fx.scan().await.open;
    assert_eq!(open.len(), 2);
    for entry in &open {
        resolve_rule(&fx.db, entry.conflict_id.as_deref().unwrap(), true).await.unwrap();
    }
    assert!(is_symlink(&fx.p(".claude/rules/commit-rules.md")));
    assert!(fs::read_to_string(fx.p("AGENTS.md")).unwrap().contains("Short subjects."));
    let rule_copy = backups(&fx, ".claude/rules/commit-rules.md");
    assert_eq!(fs::read_to_string(&rule_copy[0]).unwrap(), "mine");
    let agents_copy = backups(&fx, "AGENTS.md");
    assert!(fs::read_to_string(&agents_copy[0]).unwrap().contains("Long subjects."));
    assert!(fx.scan().await.open.is_empty());
}

#[tokio::test]
async fn ac13_unreadable_items_are_reported_and_their_rows_kept() {
    let fx = Fx::new("ac13").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    fx.scan().await;
    // A folder without SKILL.md where the link goes: can't be compared.
    fs::remove_file(fx.p(".claude/skills/ui-review/SKILL.md")).unwrap();
    let r = fx.scan().await;
    assert_eq!((r.checked, r.open.len(), r.closed.len(), r.errors.len()), (1, 0, 0, 1));
    assert_eq!(r.errors[0].name.as_deref(), Some("ui-review"));
    assert!(r.errors[0].error.contains("SKILL.md"));
    assert_eq!(fx.open_rows("sync_conflicts").await.len(), 1, "left as it was");
}

#[tokio::test]
async fn ac14_materialized_items_are_unlinked_and_their_conflicts_closed() {
    let fx = Fx::new("ac14").await;
    fx.skill_copy(CLAUDE, ".claude/skills", "mine\n").await;
    apply_rule_to_project(&fx.db, "p1", "r1", true).await.unwrap();
    let agents = fs::read_to_string(fx.p("AGENTS.md")).unwrap();
    fs::write(fx.p("AGENTS.md"), agents.replace("Short", "Long")).unwrap();
    assert_eq!(fx.scan().await.open.len(), 2);
    let before = snapshot(&fx.dir);

    // Only listed in a preview, not confirmed: nothing changes.
    detach_paths(&fx.db, &[]).await.unwrap();
    assert_eq!(fx.open_rows("sync_conflicts").await.len(), 1);

    let done = [fx.p(".claude/skills/ui-review").display().to_string(), fx.p("AGENTS.md").display().to_string()];
    detach_paths(&fx.db, &done).await.unwrap();
    let ps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_skills").fetch_one(&fx.db).await.unwrap();
    let pr: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_rules").fetch_one(&fx.db).await.unwrap();
    assert_eq!((ps, pr), (0, 0));
    assert!(fx.open_rows("sync_conflicts").await.is_empty());
    assert!(fx.open_rows("rule_sync_conflicts").await.is_empty());
    assert_eq!(snapshot(&fx.dir), before, "files stay as they are");
    let r = fx.scan().await;
    assert!(r.open.is_empty() && r.checked == 0);
}

#[tokio::test]
async fn ac14_one_engine_of_two_keeps_the_other() {
    let fx = Fx::new("ac14-both").await;
    apply_skill_with_engines(&fx.db, "p1", "s1", true, Some(SkillEngines { claude: true, codex: true })).await.unwrap();
    detach_paths(&fx.db, &[fx.p(".agents/skills/ui-review").display().to_string()]).await.unwrap();
    let row: (i64, i64, Option<String>) =
        sqlx::query_as("SELECT enabled_claude, enabled_codex, synced_hash_codex FROM project_skills").fetch_one(&fx.db).await.unwrap();
    assert_eq!(row, (1, 0, None));
}
