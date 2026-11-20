//! Controller action `save_pasted_text` (RT-26): the phone / web composer saves a long
//! paste of a session as a `.txt` file on the Host, like the desktop composer does through
//! the Tauri command of the same name ([`crate::commands::paste_as_file`]).
//!
//! Payload: `{ action: "save_pasted_text", command_id, run_id, text }`. The run gate in
//! [`super::handler::handle`] has already checked `run_id` (wrong run → `wrong_run`).
//! Answer: a `command_ack`; accepted ones carry `path` (the file's absolute Host path),
//! refused ones a `reason`: `malformed` (no run / empty text / write failed),
//! `too_large` (over [`MAX_TEXT_BYTES`]) or `wrong_run` (the run is gone).
//! Every outcome is audited ([`accept`] / [`reject`], AC-14) without the text: only its size.

use std::path::PathBuf;

use crate::commands::paste_as_file::{save, save_under, MAX_TEXT_BYTES};
use crate::db::Db;
use crate::remote::command::RejectReason;
use crate::remote::handler::{accept, reject, send_payload, HandlerCtx};
use crate::remote::protocol::{CmdPayload, StreamPayload};

const ACTION: &str = "save_pasted_text";

/// Check the command's fields; the run id and text to save.
fn check<'a>(run_id: Option<&'a str>, text: Option<&'a str>) -> Result<(&'a str, &'a str), RejectReason> {
    let run_id = run_id.map(str::trim).filter(|r| !r.is_empty()).ok_or(RejectReason::Malformed)?;
    let text = text.filter(|t| !t.is_empty()).ok_or(RejectReason::Malformed)?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(RejectReason::TooLarge);
    }
    Ok((run_id, text))
}

/// Whether `run_id` names a run on this Host.
async fn run_exists(db: &Db, run_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?)")
        .bind(run_id)
        .fetch_one(db)
        .await
        .unwrap_or(false)
}

/// Save the paste; the file's path. `root`: `None` uses the app's paste folder, a test
/// passes its own.
async fn save_for(
    db: &Db,
    root: Option<Option<PathBuf>>,
    run_id: Option<&str>,
    text: Option<&str>,
) -> Result<String, RejectReason> {
    let (run_id, text) = check(run_id, text)?;
    if !run_exists(db, run_id).await {
        return Err(RejectReason::WrongRun);
    }
    let saved = match root {
        Some(root) => save_under(db, root, run_id, text).await,
        None => save(db, run_id, text).await,
    };
    match saved {
        Ok(path) => Ok(path.to_string_lossy().into_owned()),
        Err(e) => {
            tracing::warn!(event = "remote_save_pasted_text_failed", run_id, error = %e);
            Err(RejectReason::Malformed)
        }
    }
}

/// The ack for a `save_pasted_text` command.
fn ack(cmd: &CmdPayload, result: &Result<String, RejectReason>) -> Option<StreamPayload> {
    Some(StreamPayload::CommandAck {
        command_id: cmd.command_id.clone()?,
        action: ACTION.to_string(),
        accepted: result.is_ok(),
        reason: result.as_ref().err().map(|r| r.as_str().to_string()),
        run_id: None,
        item_id: None,
        path: result.as_ref().ok().cloned(),
    })
}

/// Handle `save_pasted_text`.
pub(super) async fn handle(ctx: &HandlerCtx, cmd: &CmdPayload) {
    let run_id = cmd.run_id.as_deref();
    let result = save_for(&ctx.db, None, run_id, cmd.text.as_deref()).await;
    match &result {
        Ok(_) => {
            let size = format!("bytes={}", cmd.text.as_deref().map_or(0, str::len));
            accept(ctx, ACTION, run_id, Some(&size)).await;
        }
        Err(reason) => reject(ctx, ACTION, run_id, *reason).await,
    }
    if let Some(payload) = ack(cmd, &result) {
        send_payload(ctx, payload).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c')",
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p2', 'b', '/b', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("puppeteer-remote-pastes-{}", uuid::Uuid::new_v4()))
    }

    fn cmd(v: serde_json::Value) -> CmdPayload {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_action_is_allow_listed_and_mutating() {
        use crate::remote::command::{is_allowed_action, is_data_action, is_push_action, is_read_only_action};
        assert!(is_allowed_action(ACTION));
        assert!(!is_read_only_action(ACTION), "writes a file: the mutating budget");
        assert!(!is_data_action(ACTION) && !is_push_action(ACTION), "needs no negotiated feature");
        let c = cmd(json!({ "action": ACTION, "command_id": "c1", "run_id": "r1", "text": "hi" }));
        assert_eq!((c.run_id.as_deref(), c.text.as_deref()), (Some("r1"), Some("hi")));
    }

    #[tokio::test]
    async fn saves_the_text_and_answers_its_path() {
        let db = db().await;
        let root = temp_root();
        let path = save_for(&db, Some(Some(root.clone())), Some("r1"), Some("long\npaste")).await.unwrap();
        assert!(PathBuf::from(&path).starts_with(root.join("r1")) && path.ends_with(".txt"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "long\npaste");

        let c = cmd(json!({ "action": ACTION, "command_id": "c1", "run_id": "r1", "text": "x" }));
        let v = serde_json::to_value(ack(&c, &Ok(path.clone())).unwrap()).unwrap();
        assert_eq!(v["kind"], "command_ack");
        assert_eq!((v["action"].as_str(), v["accepted"].as_bool()), (Some(ACTION), Some(true)));
        assert_eq!(v["path"], path.as_str());
        assert!(v.get("reason").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn refuses_a_wrong_run_a_missing_field_and_a_too_large_text() {
        let db = db().await;
        let root = temp_root();
        let at = || Some(Some(root.clone()));
        assert_eq!(save_for(&db, at(), Some("nope"), Some("x")).await, Err(RejectReason::WrongRun));
        assert_eq!(save_for(&db, at(), None, Some("x")).await, Err(RejectReason::Malformed));
        assert_eq!(save_for(&db, at(), Some("  "), Some("x")).await, Err(RejectReason::Malformed));
        assert_eq!(save_for(&db, at(), Some("r1"), None).await, Err(RejectReason::Malformed));
        assert_eq!(save_for(&db, at(), Some("r1"), Some("")).await, Err(RejectReason::Malformed));
        let big = "x".repeat(MAX_TEXT_BYTES + 1);
        assert_eq!(save_for(&db, at(), Some("r1"), Some(&big)).await, Err(RejectReason::TooLarge));
        // The paste folder not set up: refused, nothing written.
        assert_eq!(save_for(&db, Some(None), Some("r1"), Some("x")).await, Err(RejectReason::Malformed));
        assert!(!root.exists(), "nothing written for a refused paste");

        let c = cmd(json!({ "action": ACTION, "command_id": "c2", "run_id": "nope", "text": "x" }));
        let v = serde_json::to_value(ack(&c, &Err(RejectReason::TooLarge)).unwrap()).unwrap();
        assert_eq!((v["accepted"].as_bool(), v["reason"].as_str()), (Some(false), Some("too_large")));
        assert!(v.get("path").is_none());
        // No command id: nothing to correlate, no ack.
        assert!(ack(&cmd(json!({ "action": ACTION })), &Ok("p".into())).is_none());
    }

    #[tokio::test]
    async fn the_project_list_carries_the_paste_setting() {
        use crate::commands::paste_as_file::{set, PasteAsFile};
        let db = db().await;
        set(&db, "p2", PasteAsFile { enabled: false, max_chars: 42 }).await.unwrap();
        let projects = crate::remote::forwarder::build_project_list(&db).await;
        let got: Vec<(&str, PasteAsFile)> = projects.iter().map(|p| (p.id.as_str(), p.paste_as_file)).collect();
        assert_eq!(got, [("p1", PasteAsFile::default()), ("p2", PasteAsFile { enabled: false, max_chars: 42 })]);
        let v = serde_json::to_value(StreamPayload::ProjectList { projects }).unwrap();
        assert_eq!(v["projects"][1]["paste_as_file"], json!({ "enabled": false, "max_chars": 42 }));
    }
}
