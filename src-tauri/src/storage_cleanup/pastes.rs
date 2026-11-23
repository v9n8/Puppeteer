//! Pastes saved as files: `<app data>/pastes/<run_id>/` (`commands::paste_as_file`).
//! A run's folder goes with the run when it is purged; this only takes a folder
//! whose run no longer exists. The age guard matters too: a paste is saved
//! before its run row exists.

use super::{fs_safe, Category, Ctx, Item, CAT_PASTES};
use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let runs: HashSet<String> = sqlx::query_scalar("SELECT id FROM runs")
        .fetch_all(&ctx.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();
    let (root, cutoff) = (ctx.app_data.join("pastes"), ctx.cutoff());
    tokio::task::spawn_blocking(move || plan_in(&root, &runs, cutoff)).await.map_err(|e| e.to_string())
}

fn plan_in(root: &Path, runs: &HashSet<String>, cutoff: SystemTime) -> Category {
    let mut category = Category::new(CAT_PASTES);
    let Ok(entries) = std::fs::read_dir(root) else { return category };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if entry.file_name().to_str().is_none_or(|run_id| runs.contains(run_id)) {
            continue;
        }
        let path = entry.path();
        if fs_safe::newest_mtime(&path).is_none_or(|t| t >= cutoff) {
            continue;
        }
        let size = fs_safe::size_of(&path);
        category.items.push(Item::path(root, path, size));
    }
    category
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::tests::picked_path;
    use crate::storage_cleanup::fs_safe::tests::{backdate, days_ago, temp_dir, write};

    #[test]
    fn only_an_old_orphan_paste_goes() {
        let root = temp_dir("pastes");
        for (run_id, age) in [("live-old", 90), ("orphan-old", 90), ("orphan-new", 2)] {
            let file = root.join(run_id).join("paste-1.txt");
            write(&file);
            backdate(&file, days_ago(age));
            backdate(&root.join(run_id), days_ago(age));
        }
        let runs: HashSet<String> = ["live-old".to_string()].into();
        let category = plan_in(&root, &runs, days_ago(30));
        let picked: Vec<_> = category.items.iter().map(picked_path).collect();
        assert_eq!(picked, [root.join("orphan-old")]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_new_file_inside_keeps_an_old_orphan_folder() {
        let root = temp_dir("pastes-fresh");
        write(&root.join("orphan/old.txt"));
        backdate(&root.join("orphan/old.txt"), days_ago(90));
        write(&root.join("orphan/new.txt"));
        backdate(&root.join("orphan"), days_ago(90));
        assert!(plan_in(&root, &HashSet::new(), days_ago(30)).items.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
