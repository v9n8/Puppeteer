//! RT-27: reading Slack DM threads, off unless turned on for every project and opted in by the
//! thread's project. Uses the mock Slack of `tests.rs`.

use super::*;

const DM: &str = "D0B83GHDTFG";
const DM_LINK: &str = "https://malme.slack.com/archives/D0B83GHDTFG/p1789877283026359";

async fn set_global(db: &Db, on: bool) {
    crate::commands::settings::set_setting(db, dm::SETTING, &on.to_string()).await.unwrap();
}

async fn add_in(env: &Env, url: &str, project: Option<&str>) -> Result<String, CodedError> {
    env.state.add_thread(url, project, None, false).await.map(|r| r.id)
}

#[tokio::test]
async fn off_refuses_global_on_and_project_off_refuses_both_on_reads() {
    let env = Env::new("dm").await;
    env.seed_thread();
    env.mock.with(|d| {
        d.scopes = Some("channels:history,groups:history,channels:read,groups:read,users:read,files:read,im:history,im:read".into());
        d.channel_info.insert(DM.into(), json!({ "ok": true, "channel": { "id": DM, "is_im": true } }));
    });

    // Off (the default): refused, nothing added.
    assert!(!dm::global_on(&env.db).await);
    assert_eq!(add_in(&env, DM_LINK, None).await.unwrap_err().code, "dm_unsupported");
    assert_eq!(add_in(&env, DM_LINK, Some("p1")).await.unwrap_err().code, "dm_unsupported");

    // On for every project, opted in by p2 only: p1 refused, p2 and no project read.
    set_global(&env.db, true).await;
    dm::set_project(&env.db, "p2", true).await.unwrap();
    assert_eq!(add_in(&env, DM_LINK, Some("p1")).await.unwrap_err().code, "dm_unsupported");
    let id = add_in(&env, DM_LINK, Some("p2")).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.0, "ready", "{:?}", env.status(&id).await);
    assert_eq!(env.view(&id).await.messages.len(), 3);

    // Moved to p1 (its switch off), or the setting turned off: the next fetch is refused.
    sqlx::query("UPDATE slack_threads SET project_id = 'p1' WHERE id = ?").bind(&id).execute(&env.db).await.unwrap();
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.1.as_deref(), Some("dm_unsupported"));
    sqlx::query("UPDATE slack_threads SET project_id = NULL WHERE id = ?").bind(&id).execute(&env.db).await.unwrap();
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.0, "ready");
    set_global(&env.db, false).await;
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.1.as_deref(), Some("dm_unsupported"));
}

#[tokio::test]
async fn a_token_without_the_dm_scope_says_which_one() {
    let env = Env::new("dm-scope").await;
    env.seed_thread();
    set_global(&env.db, true).await;
    // A group DM behind a C… id; the token reads 1:1 DMs only.
    env.mock.with(|d| {
        d.scopes = Some("channels:history,groups:history,channels:read,groups:read,im:history".into());
        d.channel_info.insert(CH.into(), json!({ "ok": true, "channel": { "id": CH, "is_mpim": true } }));
    });
    let id = env.add(LINK).await;
    env.run(&id).await;
    let (s, code, _) = env.status(&id).await;
    assert_eq!((s.as_str(), code.as_deref()), ("error", Some("missing_scope")));
    let detail: Option<String> = sqlx::query_scalar("SELECT fetch_error FROM slack_threads WHERE id = ?")
        .bind(&id)
        .fetch_one(&env.db)
        .await
        .unwrap();
    assert_eq!(detail.as_deref(), Some("mpim:history"));

    // Slack's own answer: a missing DM scope while DMs are on is a missing scope too.
    env.mock.with(|d| {
        d.channel_info.remove(CH);
        d.replies_error = Some(json!({ "ok": false, "error": "missing_scope", "needed": "mpim:history" }));
    });
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.1.as_deref(), Some("missing_scope"));
}

#[tokio::test]
async fn the_project_opt_in_is_off_by_default_and_round_trips_in_a_backup_entry() {
    let db = memory_db().await;
    assert!(!dm::project_on(&db, "p1").await, "a project is off until it opts in");
    assert!(!dm::project_on(&db, "missing").await);
    set_global(&db, true).await;
    // A Slack MCP server on for every project, which a project's DMs also need.
    Tokens::default().set(&db, "s", "xoxp-1").await;
    assert!(!dm::allowed(&db, Some("p1")).await && dm::allowed(&db, None).await);
    assert!(dm::import_project(&db, "p1", Some(true)).await.unwrap());
    assert!(!dm::import_project(&db, "p1", None).await.unwrap(), "an older backup keeps it");
    assert!(dm::allowed(&db, Some("p1")).await);
    let p = crate::commands::projects::project_by_id(&db, "p1").await.unwrap();
    assert!(p.slack_dm_read);
    assert!(dm::set_project(&db, "missing", true).await.is_err());
}
