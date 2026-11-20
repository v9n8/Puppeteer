//! A project's "long paste as a file" setting (#16, RT-25) in a settings backup. Declared as a
//! child of `settings_backup.rs` so it can use the private collect / import.

use super::{collect_projects, import_project, ProjectEntry};
use crate::commands::paste_as_file::{self as paste, PasteAsFile};
use crate::db::Db;

async fn db_with(rows: &str) -> Db {
    let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    sqlx::query(&format!("INSERT INTO projects (id, name, path, created_at) VALUES {rows}")).execute(&db).await.unwrap();
    db
}

#[tokio::test]
async fn the_setting_round_trips_and_an_older_backup_keeps_the_local_one() {
    let source = db_with("('p1', 'web', '/src/web', 'c')").await;
    let web = PasteAsFile { enabled: false, max_chars: 500 };
    paste::set(&source, "p1", web).await.unwrap();
    let projects: Vec<ProjectEntry> =
        serde_json::from_value(serde_json::to_value(collect_projects(&source).await.unwrap()).unwrap()).unwrap();
    let entry = &projects[0];
    assert_eq!(entry.paste_as_file, Some(web));
    // RT-27: the project's Slack DM switch travels too.
    assert_eq!(entry.slack_dm_read, Some(true));

    let target = db_with("('w', 'web', '/other/web', 'c')").await;
    let (_, id, warnings) = import_project(&target, entry, false, None).await.unwrap();
    assert_eq!(id, "w");
    assert!(!warnings.iter().any(|w| w.contains("paste")), "{warnings:?}");
    assert_eq!(paste::load(&target, "w").await, web);

    // A backup from before 0028: the local setting stays.
    let local = PasteAsFile { enabled: true, max_chars: 42 };
    paste::set(&target, "w", local).await.unwrap();
    let old: ProjectEntry = serde_json::from_value(serde_json::json!({
        "id": "x", "name": "web", "path": "/other/web", "default_engine": "claude", "created_at": "c",
    }))
    .unwrap();
    assert!(old.paste_as_file.is_none());
    import_project(&target, &old, false, None).await.unwrap();
    assert_eq!(paste::load(&target, "w").await, local);

    // A bad length in the backup: left out with a warning.
    let bad: ProjectEntry = serde_json::from_value(serde_json::json!({
        "id": "x", "name": "web", "path": "/other/web", "default_engine": "claude", "created_at": "c",
        "paste_as_file": { "enabled": false, "max_chars": 0 },
    }))
    .unwrap();
    let (_, _, warnings) = import_project(&target, &bad, false, None).await.unwrap();
    assert!(warnings.iter().any(|w| w.contains("long paste")), "{warnings:?}");
    assert_eq!(paste::load(&target, "w").await, local);
}
