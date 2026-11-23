//! Rewind snapshots (`<app data>/snapshots`) prune themselves (`runs::snapshots`):
//! the age limit there follows N. The cleanup runs the same sweep, and its
//! preview asks the same question the sweep does: which blobs would the
//! snapshots younger than N no longer need.

use super::{Category, Ctx, Item, Target, CAT_SNAPSHOTS};
use crate::runs::snapshots::{plan_cleanup, stored_blobs};
use chrono::SecondsFormat;

pub(super) async fn plan(ctx: &Ctx) -> Result<Category, String> {
    let max_age_days = i64::from(ctx.days);
    // `created_at` is compared as text, in the format the sweep writes.
    let cutoff = (ctx.now - chrono::Duration::days(max_age_days)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_snapshots WHERE status = 'ok' AND created_at < ?")
        .bind(&cutoff)
        .fetch_one(&ctx.db)
        .await
        .map_err(|e| e.to_string())?;
    let young: Vec<String> = sqlx::query_scalar(
        "SELECT manifest_sha FROM run_snapshots WHERE status = 'ok' AND created_at >= ?
         GROUP BY manifest_sha ORDER BY MAX(created_at), MAX(id)",
    )
    .bind(&cutoff)
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    let store = ctx.app_data.join("snapshots");
    tokio::task::spawn_blocking(move || {
        let blobs = stored_blobs(&store);
        // The store's size cap applies whatever N is: only the age is previewed.
        let (_, needed) = plan_cleanup(&store, &young, &blobs, u64::MAX);
        let (bytes, files) = blobs
            .iter()
            .filter(|(sha, _)| !needed.contains(*sha))
            .fold((0, 0), |(bytes, files), (_, (_, len))| (bytes + len, files + 1));
        let mut category = Category::new(CAT_SNAPSHOTS);
        if old > 0 || files > 0 {
            category.items.push(Item { target: Target::Snapshots { store, max_age_days }, bytes, files });
        }
        category
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_cleanup::fs_safe::tests::temp_dir;
    use crate::storage_cleanup::tests::test_db;
    use crate::storage_cleanup::{apply, Plan};
    use chrono::Utc;

    /// A manifest blob naming one content blob, as `runs::snapshots` stores them.
    fn put(store: &std::path::Path, sha: &str, body: &[u8]) {
        let path = store.join("blobs").join(&sha[..2]).join(sha);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[tokio::test]
    async fn with_n_at_60_a_45_day_old_snapshot_stays_and_a_65_day_old_one_goes() {
        let base = temp_dir("snapshots");
        let store = base.join("snapshots");
        let db = test_db(&base).await;
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/p', 'c')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')").execute(&db).await.unwrap();
        for (turn, days, manifest, content) in [(1, 65, "aa01", "c065"), (2, 45, "bb01", "d045")] {
            put(&store, content, b"file");
            put(&store, manifest, format!(r#"{{"files":[{{"path":"a.txt","sha":"{content}","size":4}}],"unmanaged":[]}}"#).as_bytes());
            let at = (Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Millis, true);
            sqlx::query("INSERT INTO run_snapshots (run_id, turn, created_at, workspace, status, manifest_sha) VALUES ('r1', ?, ?, '/p', 'ok', ?)")
                .bind(turn)
                .bind(at)
                .bind(manifest)
                .execute(&db)
                .await
                .unwrap();
        }
        let ctx = Ctx { home: None, temp: base.join("no-tmp"), ..Ctx::new(db.clone(), base.clone(), 60, crate::runs::new_registry(), None) };
        let category = plan(&ctx).await.unwrap();
        assert_eq!(category.summary().files, 2, "the old manifest and the content only it uses");

        let outcome = apply(&ctx, Plan { categories: vec![category] }).await;
        assert_eq!(outcome.errors, 0);
        let status: Vec<String> = sqlx::query_scalar("SELECT status FROM run_snapshots ORDER BY turn").fetch_all(&db).await.unwrap();
        assert_eq!(status, ["pruned", "ok"]);
        assert!(!store.join("blobs/c0/c065").exists() && !store.join("blobs/aa/aa01").exists());
        assert!(store.join("blobs/d0/d045").exists() && store.join("blobs/bb/bb01").exists());
        // Auto-clean off keeps the 30 days snapshots always had.
        assert_eq!(crate::storage_cleanup::snapshot_days_for(0), 30);
        assert_eq!(crate::storage_cleanup::snapshot_days_for(60), 60);
        db.close().await;
        std::fs::remove_dir_all(&base).unwrap();
    }
}
