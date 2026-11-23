//! Folders of deleted Claude accounts: `<app data>/claude-accounts/<id>/` with
//! no `claude_accounts` row left (`delete_claude_account` drops only the row).
//! The whole folder goes, logins included, since the account is gone; its
//! links into the global config (`skills`, `settings.json`…) go as links. A
//! folder still holding a run's transcript is kept, so that run can resume.

use super::{fs_safe, Category, Ctx, Item, Target, CAT_DELETED_ACCOUNTS};
use sqlx::Row;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What still points into the accounts folder.
struct InUse {
    account_ids: HashSet<String>,
    config_dirs: Vec<PathBuf>,
    transcripts: Vec<PathBuf>,
}

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let rows = sqlx::query("SELECT id, config_dir FROM claude_accounts")
        .fetch_all(&ctx.db)
        .await
        .map_err(|e| e.to_string())?;
    let mut in_use = InUse { account_ids: HashSet::new(), config_dirs: Vec::new(), transcripts: transcript_paths(ctx).await? };
    for row in rows {
        in_use.account_ids.insert(row.try_get("id").map_err(|e| e.to_string())?);
        let dir: String = row.try_get("config_dir").map_err(|e| e.to_string())?;
        in_use.config_dirs.push(canonical(Path::new(&dir)));
    }
    let (root, cutoff) = (ctx.app_data.join("claude-accounts"), ctx.cutoff());
    tokio::task::spawn_blocking(move || plan_in(&root, &in_use, cutoff)).await.map_err(|e| e.to_string())
}

/// Every run's transcript, as stored.
pub(super) async fn transcript_paths(ctx: &Ctx) -> Result<Vec<PathBuf>, String> {
    let paths: Vec<String> = sqlx::query_scalar("SELECT transcript_path FROM runs WHERE transcript_path IS NOT NULL AND transcript_path <> ''")
        .fetch_all(&ctx.db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(paths.iter().map(|p| canonical(Path::new(p))).collect())
}

/// `path` with its links resolved when it exists, else as given.
pub(super) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn plan_in(root: &Path, in_use: &InUse, cutoff: SystemTime) -> Category {
    let mut category = Category::new(CAT_DELETED_ACCOUNTS);
    let Ok(entries) = std::fs::read_dir(root) else { return category };
    for entry in entries.flatten() {
        // A link at the top is never an account folder of ours.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let dir = entry.path();
        let real = canonical(&dir);
        let live = entry.file_name().to_str().is_none_or(|name| in_use.account_ids.contains(name))
            || in_use.config_dirs.iter().any(|c| *c == real || *c == dir);
        if live || fs_safe::newest_mtime(&dir).is_none_or(|t| t >= cutoff) {
            continue;
        }
        let size = fs_safe::size_of(&dir);
        if in_use.transcripts.iter().any(|t| t.starts_with(&real) || t.starts_with(&dir)) {
            category.kept_items += 1;
            category.kept_bytes += size.0;
            continue;
        }
        category.items.push(Item { target: Target::AccountDir { root: root.to_path_buf(), path: dir }, bytes: size.0, files: size.1 });
    }
    category
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::{backdate, days_ago, temp_dir, write};
    use crate::storage_cleanup::tests::picked_path;

    fn age(dir: &Path, days: u64) {
        for entry in walkdir::WalkDir::new(dir).follow_links(false).contents_first(true) {
            let entry = entry.unwrap();
            if !entry.path_is_symlink() {
                backdate(entry.path(), days_ago(days));
            }
        }
    }

    #[test]
    fn only_an_old_folder_with_no_account_and_no_transcript_goes() {
        let root = temp_dir("accounts");
        for id in ["live", "gone-old", "gone-new", "gone-transcript", "live-by-dir"] {
            write(&root.join(id).join(".credentials.json"));
        }
        write(&root.join("gone-transcript/projects/p/s.jsonl"));
        for id in ["live", "gone-old", "gone-transcript", "live-by-dir"] {
            age(&root.join(id), 90);
        }
        let in_use = InUse {
            account_ids: ["live".to_string()].into(),
            config_dirs: vec![canonical(&root.join("live-by-dir"))],
            transcripts: vec![canonical(&root.join("gone-transcript/projects/p/s.jsonl"))],
        };
        let category = plan_in(&root, &in_use, days_ago(30));
        let picked: Vec<_> = category.items.iter().map(picked_path).collect();
        assert_eq!(picked, [root.join("gone-old")]);
        assert_eq!(category.kept_items, 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_deleted_account_goes_but_the_global_skills_it_links_to_stay() {
        let base = temp_dir("accounts-link");
        let root = base.join("claude-accounts");
        let global = base.join("dot-claude/skills");
        write(&global.join("s/SKILL.md"));
        write(&root.join("old/.claude.json"));
        std::os::unix::fs::symlink(&global, root.join("old/skills")).unwrap();
        age(&root.join("old"), 90);
        let in_use = InUse { account_ids: HashSet::new(), config_dirs: Vec::new(), transcripts: Vec::new() };
        let category = plan_in(&root, &in_use, days_ago(30));
        assert_eq!(category.items.len(), 1);
        let outcome = crate::storage_cleanup::apply(crate::storage_cleanup::Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        assert!(!root.join("old").exists());
        assert!(global.join("s/SKILL.md").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
