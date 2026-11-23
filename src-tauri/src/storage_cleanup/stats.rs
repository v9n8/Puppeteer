//! The Storage table's rows for what the cleanup knows about (`commands::storage`
//! keeps the original rows). Each row is the whole location; what the next
//! cleanup would take from it comes from the preview. A row already counted in
//! another one is marked `subset` and left out of the totals.

use super::accounts::canonical;
use super::fs_safe::size_of;
use super::sidecar_junk::temp_slugs;
use super::trash::run_files;
use crate::commands::storage::StorageCategory;
use crate::db::Db;
use sqlx::Row;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What the rows read from the database before the folders are walked.
struct Inputs {
    app_data: PathBuf,
    home: Option<PathBuf>,
    temp: PathBuf,
    /// Live accounts: (id, config dir).
    accounts: Option<Vec<(String, PathBuf)>>,
    trashed: Vec<PathBuf>,
}

pub async fn categories(db: &Db, app_data: &Path) -> Vec<StorageCategory> {
    let accounts = sqlx::query("SELECT id, config_dir FROM claude_accounts")
        .fetch_all(db)
        .await
        .ok()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| Some((r.try_get::<String, _>("id").ok()?, PathBuf::from(r.try_get::<String, _>("config_dir").ok()?))))
                .collect()
        });
    let trashed: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT r.id, p.path, r.transcript_path FROM runs r JOIN projects p ON p.id = r.project_id \
         WHERE r.deleted_at IS NOT NULL OR p.deleted_at IS NOT NULL",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let inputs = Inputs {
        app_data: app_data.to_path_buf(),
        home: std::env::var_os("HOME").map(PathBuf::from),
        temp: std::env::temp_dir(),
        accounts,
        trashed: trashed.into_iter().flat_map(|(id, project, transcript)| run_files(&project, &id, transcript)).collect(),
    };
    tokio::task::spawn_blocking(move || rows(&inputs)).await.unwrap_or_default()
}

fn row(id: &str, label: &str, description: &str, path: &str, (size_bytes, file_count): (u64, u64), subset: bool) -> StorageCategory {
    StorageCategory {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        path: path.into(),
        size_bytes,
        file_count,
        subset,
    }
}

fn add((a, b): (u64, u64), (c, d): (u64, u64)) -> (u64, u64) {
    (a + c, b + d)
}

fn rows(inputs: &Inputs) -> Vec<StorageCategory> {
    let data = &inputs.app_data;
    let accounts_root = data.join("claude-accounts");
    let live = inputs.accounts.clone().unwrap_or_default();
    let live_ids: HashSet<&str> = live.iter().map(|(id, _)| id.as_str()).collect();
    let live_dirs: HashSet<PathBuf> = live.iter().map(|(_, dir)| canonical(dir)).collect();

    // Accounts: a folder no account owns is a deleted one's; in a live one,
    // everything but the transcripts (`projects/`, a row of its own).
    let (mut deleted, mut other) = ((0, 0), (0, 0));
    if inputs.accounts.is_some() {
        for entry in std::fs::read_dir(&accounts_root).into_iter().flatten().flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let dir = entry.path();
            let name = entry.file_name();
            if live_ids.contains(name.to_string_lossy().as_ref()) || live_dirs.contains(&canonical(&dir)) {
                for part in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    if part.file_name() != "projects" {
                        other = add(other, size_of(&part.path()));
                    }
                }
            } else {
                deleted = add(deleted, size_of(&dir));
            }
        }
    }

    let mut junk = (0, 0);
    let configs = inputs.home.iter().map(|h| h.join(".claude")).chain(live.iter().map(|(_, d)| d.clone()));
    let mut seen = HashSet::new();
    for config in configs {
        for slug in temp_slugs(&inputs.temp) {
            let dir = config.join("projects").join(slug);
            if seen.insert(canonical(&dir)) && dir.is_dir() {
                junk = add(junk, size_of(&dir));
            }
        }
    }

    let trash = inputs.trashed.iter().fold((0, 0), |sum, f| add(sum, size_of(f)));
    let database = ["data.db", "data.db-wal", "data.db-shm"]
        .iter()
        .filter_map(|f| std::fs::metadata(data.join(f)).ok())
        .fold((0, 0), |(bytes, files), m| (bytes + m.len(), files + 1));

    vec![
        row("app_logs", "App logs", "Puppeteer's own log, one file per day. Days older than the auto-clean age are removed; today's file never is.", "<app data>/logs", size_of(&data.join("logs")), false),
        row("pastes", "Pastes saved as files", "Long pastes kept as files for a session. They go with their session; only those whose session is gone are cleaned.", "<app data>/pastes", size_of(&data.join("pastes")), false),
        row("worktrees", "Worker worktrees", "Copies of a project a Conductor worker ran in. Cleaned only when the worker finished long ago and everything it did is committed and merged.", "<app data>/worktrees", size_of(&data.join("worktrees")), false),
        row("snapshots", "Rewind snapshots", "File snapshots taken before each turn, for Rewind. Older than the auto-clean age (30 days when it is off), they are dropped.", "<app data>/snapshots", size_of(&data.join("snapshots")), false),
        row("trash", "Sessions in the Trash", "Logs and transcripts of sessions and projects in the Trash (already counted above). Purged once they have been in the Trash longer than the auto-clean age.", "Trash", trash, true),
        row("deleted_accounts", "Deleted Claude accounts", "Folders of Claude accounts that were removed. Cleaned unless a session's transcript is still inside.", "<app data>/claude-accounts", deleted, false),
        row("sidecar_junk", "Model check / translate transcripts", "Transcripts the model check and translation left in the temp folder's project (already counted in Claude CLI sessions). Never resumed.", "~/.claude/projects/<temp folder>", junk, true),
        row("account_other", "Claude account settings and caches", "Everything in the managed Claude accounts except transcripts: plugins, backups, shell snapshots, caches. Shown only.", "<app data>/claude-accounts", other, false),
        row("codex_accounts", "Codex accounts", "Managed Codex account folders. Shown only.", "<app data>/codex-accounts", size_of(&data.join("codex-accounts")), false),
        row("database", "Database", "Puppeteer's database. Compact it to give back the space of deleted rows.", "<app data>/data.db", database, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::{temp_dir, write};

    fn get<'a>(rows: &'a [StorageCategory], id: &str) -> &'a StorageCategory {
        rows.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn each_row_sizes_its_own_location() {
        let base = temp_dir("stats");
        let data = base.join("data");
        let temp = base.join("tmp");
        std::fs::create_dir_all(&temp).unwrap();
        let home = base.join("home");
        for rel in ["logs/puppeteer.log.2026-11-01", "pastes/r1/p.txt", "worktrees/w1/a.txt", "snapshots/blobs/aa/aa1", "codex-accounts/c1/auth.json", "data.db", "data.db-wal"] {
            write(&data.join(rel));
        }
        write(&data.join("claude-accounts/live/projects/p/s.jsonl"));
        write(&data.join("claude-accounts/live/plugins/x.json"));
        write(&data.join("claude-accounts/live/.claude.json"));
        write(&data.join("claude-accounts/gone/.claude.json"));
        write(&data.join("claude-accounts/gone/projects/p/s.jsonl"));
        let slug = temp_slugs(&temp)[0].clone();
        write(&home.join(".claude/projects").join(&slug).join("j.jsonl"));
        write(&base.join("proj/.puppeteer/runs/r9.log"));
        let inputs = Inputs {
            app_data: data.clone(),
            home: Some(home),
            temp,
            accounts: Some(vec![("live".into(), data.join("claude-accounts/live"))]),
            trashed: run_files(&base.join("proj").to_string_lossy(), "r9", None),
        };
        let rows = rows(&inputs);
        for (id, files) in [("app_logs", 1), ("pastes", 1), ("worktrees", 1), ("snapshots", 1), ("codex_accounts", 1), ("database", 2), ("deleted_accounts", 2), ("account_other", 2), ("sidecar_junk", 1), ("trash", 1)] {
            assert_eq!(get(&rows, id).file_count, files, "{id}");
        }
        assert!(get(&rows, "trash").subset && get(&rows, "sidecar_junk").subset);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn an_unreadable_account_table_shows_no_deleted_account() {
        let base = temp_dir("stats-no-db");
        write(&base.join("claude-accounts/a/.claude.json"));
        let inputs = Inputs { app_data: base.clone(), home: None, temp: base.join("tmp"), accounts: None, trashed: Vec::new() };
        assert_eq!(get(&rows(&inputs), "deleted_accounts").file_count, 0);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
