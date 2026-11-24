//! Which MCP servers lend Slack their token: only those that are on (app-wide, and for a
//! project's thread on for that project), and a project's DM threads need a Slack server
//! on for that project. Uses the mock Slack of `tests.rs`.

use super::*;

const DM: &str = "D0B83GHDTFG";
const DM_LINK: &str = "https://malme.slack.com/archives/D0B83GHDTFG/p1789877283026359";

async fn off_for_project(db: &Db, server_id: &str, project_id: &str) {
    sqlx::query("DELETE FROM project_mcp_servers WHERE project_id = ? AND server_id = ?")
        .bind(project_id)
        .bind(server_id)
        .execute(db)
        .await
        .unwrap();
}

async fn off_app_wide(db: &Db, server_id: &str) {
    sqlx::query("UPDATE mcp_servers SET enabled = 0 WHERE id = ?").bind(server_id).execute(db).await.unwrap();
}

#[tokio::test]
async fn a_projects_thread_only_reads_the_token_of_a_server_on_for_that_project() {
    let db = memory_db().await;
    let tokens = Tokens::default();
    tokens.set(&db, "s1", "xoxp-1").await;
    tokens.set(&db, "s2", "xoxp-2").await;
    off_for_project(&db, "s2", "p1").await;

    let read = Arc::new(Mutex::new(Vec::<String>::new()));
    let inner = tokens.lookup();
    let seen = read.clone();
    let lookup: SecretLookup = Arc::new(move |id: &str| {
        seen.lock().unwrap().push(id.to_string());
        inner(id)
    });
    let found = token::sources(&db, &lookup, Some("p1")).await.unwrap();
    assert_eq!(found.iter().map(|s| s.server_id.as_str()).collect::<Vec<_>>(), ["s1"]);
    assert_eq!(*read.lock().unwrap(), ["s1"], "the other server's token is never read");

    // p2 has both; a thread without a project uses every server on app-wide.
    assert_eq!(token::sources(&db, &lookup, Some("p2")).await.unwrap().len(), 2);
    assert_eq!(token::sources(&db, &lookup, None).await.unwrap().len(), 2);
    assert!(token::slack_on_for_project(&db, "p1").await.unwrap());
    off_for_project(&db, "s1", "p1").await;
    assert!(!token::slack_on_for_project(&db, "p1").await.unwrap());
}

#[tokio::test]
async fn the_only_slack_server_off_app_wide_means_no_token_on_add_and_on_fetch() {
    let env = Env::new("off-app-wide").await;
    env.seed_thread();
    let id = env.add(LINK).await;
    env.run(&id).await;
    assert_eq!(env.status(&id).await.0, "ready");

    off_app_wide(&env.db, "srv1").await;
    assert_eq!(token::token_status(&env.db, &env.tokens.lookup()).await.unwrap().state, "none");
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.1.as_deref(), Some("no_token"));
    let added = env.state.add_thread(LINK, Some("p1"), None, false).await.unwrap();
    assert_eq!(added.error_code.as_deref(), Some("no_token"));
}

#[tokio::test]
async fn a_projects_dm_needs_a_slack_server_on_for_that_project() {
    let env = Env::new("dm-slack-off").await;
    env.seed_thread();
    env.mock.with(|d| {
        d.scopes = Some("channels:history,groups:history,channels:read,groups:read,users:read,files:read,im:history,im:read".into());
        d.channel_info.insert(DM.into(), json!({ "ok": true, "channel": { "id": DM, "is_im": true } }));
    });
    crate::commands::settings::set_setting(&env.db, dm::SETTING, "true").await.unwrap();
    dm::set_project(&env.db, "p1", true).await.unwrap();
    let id = env.state.add_thread(DM_LINK, Some("p1"), None, false).await.unwrap().id;
    env.run(&id).await;
    assert_eq!(env.status(&id).await.0, "ready", "{:?}", env.status(&id).await);

    // Slack switched off for p1: the opt-in is kept but no longer reads.
    off_for_project(&env.db, "srv1", "p1").await;
    assert!(dm::project_on(&env.db, "p1").await);
    env.state.refresh(&id).await.unwrap();
    env.run(&id).await;
    assert_eq!(env.status(&id).await.1.as_deref(), Some("dm_unsupported"));
    let err = env.state.add_thread(DM_LINK, Some("p1"), None, false).await.unwrap_err();
    assert_eq!(err.code, "dm_unsupported");
}

#[tokio::test]
async fn reading_dms_is_off_on_a_fresh_database() {
    let db = memory_db().await;
    assert!(!dm::global_on(&db).await);
}
