//! The DM scope check against the mock Slack of `tests.rs`: one result per token
//! of a Slack server on app-wide, never the token itself, within the timeout.

use super::*;
use crate::slack_threads::dm_scopes::{check_all, CHECK_TIMEOUT};

async fn check(env: &Env, timeout: Duration) -> Vec<dm_scopes::DmScopeResult> {
    let ctx = env.state.ctx();
    check_all(&ctx.db, &ctx.lookup, &ctx.http, &ctx.token_http, &ctx.auth_cache, timeout).await.unwrap()
}

#[tokio::test]
async fn each_token_says_whether_it_can_read_dms() {
    let env = Env::new("dm-scopes").await;
    env.mock.with(|d| d.scopes = Some("channels:history,im:history,mpim:history".into()));
    let r = check(&env, CHECK_TIMEOUT).await;
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].server_id.as_str(), r[0].status, r[0].missing.len()), ("srv1", "ok", 0));

    env.mock.with(|d| d.scopes = Some("channels:history,mpim:history".into()));
    let r = check(&env, CHECK_TIMEOUT).await;
    assert_eq!((r[0].status, r[0].missing.clone()), ("missing", vec!["im:history".to_string()]));

    env.mock.with(|d| d.scopes = None);
    assert_eq!(check(&env, CHECK_TIMEOUT).await[0].status, "unknown");

    // A token Slack refuses; a server off app-wide is not checked at all.
    env.tokens.set(&env.db, "srv2", "xoxp-revoked").await;
    env.tokens.set(&env.db, "srv3", "xoxp-other").await;
    sqlx::query("UPDATE mcp_servers SET enabled = 0 WHERE id = 'srv3'").execute(&env.db).await.unwrap();
    let r = check(&env, CHECK_TIMEOUT).await;
    let ids: Vec<_> = r.iter().map(|x| (x.server_id.as_str(), x.status)).collect();
    assert_eq!(ids, [("srv1", "unknown"), ("srv2", "token_error")]);

    // The result carries no token.
    let json = serde_json::to_value(&r).unwrap();
    assert!(!json.to_string().contains("xox"));
    let mut keys: Vec<_> = json[0].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["missing", "serverId", "serverName", "status"]);
}

#[tokio::test]
async fn a_slack_that_does_not_answer_reads_as_a_token_error_once_the_timeout_passes() {
    let env = Env::with_config("dm-scopes-slow", FakeEngine::new(), |_| {}).await;
    // Point the check at a server that accepts the connection and never answers.
    let silent = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = format!("http://127.0.0.1:{}", silent.local_addr().unwrap().port());
    let cfg = Arc::new(HttpConfig {
        api_base: format!("{base}/api/"),
        trusted_test_origin: Some(base),
        ..Default::default()
    });
    // Two silent tokens: checked at the same time, so the whole check stays near one timeout.
    env.tokens.set(&env.db, "srv2", "xoxp-2").await;
    let started = std::time::Instant::now();
    let ctx = env.state.ctx();
    let r = check_all(&ctx.db, &ctx.lookup, &cfg, &cfg.token_client(), &ctx.auth_cache, Duration::from_millis(500))
        .await
        .unwrap();
    assert_eq!(r.iter().map(|x| (x.server_id.as_str(), x.status)).collect::<Vec<_>>(), [("srv1", "token_error"), ("srv2", "token_error")]);
    assert!(started.elapsed() < Duration::from_millis(950), "{:?}", started.elapsed());
    drop(silent);
}
