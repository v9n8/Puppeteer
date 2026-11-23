//! Orphaned task files: issue / PR markdown under `<project>/.puppeteer/tasks`
//! that no run points at (the Storage table's orphan row, `commands::storage`),
//! last changed more than N days ago. Files skills write there are never task
//! files, so they are never picked.

use super::{Category, Ctx, Item, CAT_TASK_ORPHANS};
use crate::commands::storage::{is_puppeteer_task_file, puppeteer_dirs, try_referenced_run_paths};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;
use walkdir::WalkDir;

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let referenced = try_referenced_run_paths(&ctx.db).await?;
    // A path can belong to two project rows (removed, then added again).
    let dirs: BTreeSet<PathBuf> = puppeteer_dirs(&ctx.db, "tasks").await.into_iter().collect();
    let cutoff = ctx.cutoff();
    tokio::task::spawn_blocking(move || plan_in(&dirs, &referenced, cutoff)).await.map_err(|e| e.to_string())
}

fn plan_in(dirs: &BTreeSet<PathBuf>, referenced: &HashSet<String>, cutoff: SystemTime) -> Category {
    let mut category = Category::new(CAT_TASK_ORPHANS);
    for dir in dirs {
        for entry in WalkDir::new(dir).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !entry.file_type().is_file()
                || !is_puppeteer_task_file(path)
                || referenced.contains(path.to_string_lossy().as_ref())
            {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.modified().map_or(true, |t| t >= cutoff) {
                continue;
            }
            category.items.push(Item::path(dir, path.to_path_buf(), (meta.len(), 1)));
        }
    }
    category
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::tests::picked_path;
    use crate::storage_cleanup::fs_safe::tests::{backdate, days_ago, temp_dir, write};

    #[test]
    fn only_an_old_unreferenced_task_file_goes() {
        let dir = temp_dir("tasks");
        let files = ["a/issue-1/issue.md", "a/issue-2/issue.md", "a/issue-3/issue.md", "a/issue-1/pr-draft.md"];
        for rel in files {
            write(&dir.join(rel));
            backdate(&dir.join(rel), days_ago(90));
        }
        write(&dir.join("a/issue-4/pr.md"));
        let referenced: HashSet<String> = [dir.join("a/issue-2/issue.md").to_string_lossy().into_owned()].into();
        let category = plan_in(&[dir.clone()].into(), &referenced, days_ago(30));
        let mut picked: Vec<_> = category.items.iter().map(picked_path).collect();
        picked.sort();
        assert_eq!(picked, [dir.join("a/issue-1/issue.md"), dir.join("a/issue-3/issue.md")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
