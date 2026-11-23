//! Transcripts the model check and translate sidecars wrote before they ran
//! without one: they ran with the OS temp dir as cwd, so the CLI filed them
//! under `projects/<temp dir as a slug>/` of every Claude config dir. Entries
//! older than N go; `memory/` and any run's transcript stay.

use super::accounts::{canonical, transcript_paths};
use super::{fs_safe, Category, Ctx, Item, CAT_SIDECAR_JUNK};
use crate::commands::sessions::encode_project_dir;
use sqlx::Row;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let mut config_dirs: Vec<PathBuf> = ctx.home.iter().map(|h| h.join(".claude")).collect();
    for row in sqlx::query("SELECT config_dir FROM claude_accounts").fetch_all(&ctx.db).await.map_err(|e| e.to_string())? {
        let dir: String = row.try_get("config_dir").map_err(|e| e.to_string())?;
        if !dir.trim().is_empty() {
            config_dirs.push(PathBuf::from(dir));
        }
    }
    let transcripts = transcript_paths(ctx).await?;
    let (slugs, cutoff) = (temp_slugs(&ctx.temp), ctx.cutoff());
    tokio::task::spawn_blocking(move || {
        let mut category = Category::new(CAT_SIDECAR_JUNK);
        let mut seen = std::collections::HashSet::new();
        for config in &config_dirs {
            for slug in &slugs {
                let dir = config.join("projects").join(slug);
                if seen.insert(canonical(&dir)) {
                    plan_in(&mut category, &dir, &transcripts, cutoff);
                }
            }
        }
        category
    })
    .await
    .map_err(|e| e.to_string())
}

/// The temp dir's slug both as the CLI sees it (links resolved: macOS
/// `/var/…` is `/private/var/…`) and as given.
pub(super) fn temp_slugs(temp: &Path) -> Vec<String> {
    let mut slugs = Vec::new();
    for path in [canonical(temp), temp.to_path_buf()] {
        let text = path.to_string_lossy();
        let slug = encode_project_dir(text.trim_end_matches(['/', '\\']));
        if !slugs.contains(&slug) {
            slugs.push(slug);
        }
    }
    slugs
}

fn plan_in(category: &mut Category, dir: &Path, transcripts: &[PathBuf], cutoff: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name() == "memory" || fs_safe::newest_mtime(&path).is_none_or(|t| t >= cutoff) {
            continue;
        }
        let real = canonical(&path);
        if transcripts.iter().any(|t| t.starts_with(&real)) {
            continue;
        }
        let size = fs_safe::size_of(&path);
        category.items.push(Item::path(dir, path, size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::{backdate, days_ago, temp_dir, write};
    use crate::storage_cleanup::tests::{picked_path, test_db};
    use crate::storage_cleanup::{apply, Plan};

    #[test]
    fn the_slug_matches_what_the_cli_writes() {
        let slugs = temp_slugs(Path::new("/no-such-tmp/folders/ab/T/"));
        assert_eq!(slugs, ["-no-such-tmp-folders-ab-T"]);
    }

    #[tokio::test]
    async fn old_junk_goes_but_memory_other_projects_and_run_transcripts_stay() {
        let base = temp_dir("junk");
        let temp = base.join("tmp");
        std::fs::create_dir_all(&temp).unwrap();
        let home = base.join("home");
        let account = base.join("claude-accounts/a1");
        let slug = temp_slugs(&temp)[0].clone();
        let junk = home.join(".claude/projects").join(&slug);
        let other = home.join(".claude/projects/-Users-me-proj");
        let files = [
            junk.join("old.jsonl"),
            junk.join("old-session/tool-results/x.txt"),
            junk.join("memory/MEMORY.md"),
            junk.join("kept-by-run.jsonl"),
            other.join("memory/MEMORY.md"),
            other.join("old.jsonl"),
            account.join("projects").join(&slug).join("old.jsonl"),
        ];
        for file in &files {
            write(file);
            backdate(file, days_ago(90));
        }
        backdate(&junk.join("old-session/tool-results"), days_ago(90));
        backdate(&junk.join("old-session"), days_ago(90));
        write(&junk.join("new.jsonl"));

        let db = test_db(&base).await;
        sqlx::query("INSERT INTO claude_accounts (id, label, config_dir, created_at) VALUES ('a1', 'a', ?, 'c')")
            .bind(account.to_string_lossy().as_ref())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/p', 'c')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at, transcript_path) VALUES ('r1', 'p1', 'session', 'c', ?)")
            .bind(junk.join("kept-by-run.jsonl").to_string_lossy().as_ref())
            .execute(&db)
            .await
            .unwrap();
        let ctx = Ctx { home: Some(home.clone()), temp, ..Ctx::new(db.clone(), base.clone(), 30, crate::runs::new_registry(), None) };
        let category = plan(&ctx).await.unwrap();
        let mut picked: Vec<_> = category.items.iter().map(picked_path).collect();
        picked.sort();
        let mut want = vec![junk.join("old-session"), junk.join("old.jsonl"), account.join("projects").join(&slug).join("old.jsonl")];
        want.sort();
        assert_eq!(picked, want);

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        for kept in [junk.join("memory/MEMORY.md"), junk.join("kept-by-run.jsonl"), junk.join("new.jsonl"), other.join("memory/MEMORY.md"), other.join("old.jsonl")] {
            assert!(kept.exists(), "{}", kept.display());
        }
        db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }
}
