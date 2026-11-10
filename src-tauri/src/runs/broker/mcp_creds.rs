//! MCP credentials served by the broker.
//!
//! The MCP config handed to the Claude / Codex CLI ends up on their command line
//! (`--mcp-config <json>`, `-c mcp_servers.*`) and in process environments —
//! both readable by any process of the same user, the agent's own shell
//! included. So the config carries references only, and the secrets are pulled
//! here at the moment a server needs them, the same way the `gh` shim gets its
//! token:
//!
//! - `google`: the built-in Google server asks for a short-lived access token for
//!   one connected account. Refresh tokens and the OAuth client secret never
//!   leave this process.
//! - `mcp`: `sidecar-proxy/puppeteer-mcp-launch.mjs` asks for a user MCP server's
//!   env (before starting a stdio server) or headers (Claude's `headersHelper`,
//!   the Codex sidecar's header resolution). `server` is the server's id, or
//!   [`CONDUCTOR_SERVER`] for the loopback conductor's bearer token.
//!
//! Fail-closed like the rest of the broker: no live run, a request naming a
//! project other than the run's own, a server that is not in the run's set
//! (`run_mcp`), an unknown account or any resolve error → deny.
//! Every request is audited without the secret.

use std::collections::HashMap;

use sqlx::Row;
use tauri::{AppHandle, Manager};

use super::{audit, ApproverResolver, BrokerRequest, BrokerResponse};
use crate::db::Db;
use crate::secrets;

/// `server` value that selects the conductor's loopback MCP server.
pub const CONDUCTOR_SERVER: &str = "@conductor";

/// Whether `tool` is served here rather than by the command-policy path.
pub fn handles(tool: &str) -> bool {
    matches!(tool, "google" | "mcp")
}

pub async fn process(
    req: &BrokerRequest,
    db: &Db,
    resolver: &dyn ApproverResolver,
    app: Option<&AppHandle>,
) -> BrokerResponse {
    let run_id = req.run_id.as_deref().unwrap_or("?");
    let target = match req.tool.as_str() {
        "google" => req.account.clone().unwrap_or_else(|| "default".into()),
        _ => req.server.clone().unwrap_or_default(),
    };
    let argv = [req.tool.clone(), target];
    let deny = |reason: &str| {
        audit::audit_request(run_id, &req.tool, &argv, "deny", Some(reason));
        BrokerResponse::deny(reason)
    };

    // Credentials only go to a run that is alive right now, and only for the
    // project that run was started in — the request's own `project_id` is just
    // a claim (any same-user process can send one).
    match req.run_id.as_deref().and_then(|r| resolver.run_project(r)) {
        None => return deny("no live run for this request"),
        Some(p) if p != req.project_id => return deny("run does not belong to this project"),
        Some(_) => {}
    }

    let resp = match req.tool.as_str() {
        "google" => google_token(req, db).await,
        _ => match req.server.as_deref() {
            Some(CONDUCTOR_SERVER) => conductor_headers(req, app),
            Some(id) if !id.is_empty() => user_server(req, db, id).await,
            _ => Err("missing server".to_string()),
        },
    };
    match resp {
        Ok(r) => {
            audit::audit_request(run_id, &req.tool, &argv, "allow", None);
            r
        }
        Err(reason) => deny(&reason),
    }
}

/// Access token for one of the connected Google accounts (label or email; the
/// default account when absent). `refresh` forces a fresh mint after a 401.
async fn google_token(req: &BrokerRequest, db: &Db) -> Result<BrokerResponse, String> {
    // Checked per request: a run launched with the server keeps it after the
    // switch goes off.
    crate::features::require(db, crate::commands::google::FEATURE).await?;
    let run_id = req.run_id.as_deref().unwrap_or_default();
    let on = crate::commands::run_mcp::run_uses_builtin(db, &req.project_id, run_id, "google")
        .await
        .map_err(|_| "Google MCP lookup failed".to_string())?;
    if !on {
        return Err("the Google MCP server is switched off for this session".into());
    }
    let rows = sqlx::query(
        "SELECT id, label, email, is_default FROM google_accounts ORDER BY is_default DESC, label",
    )
    .fetch_all(db)
    .await
    .map_err(|_| "Google account lookup failed".to_string())?;
    let pick = match req.account.as_deref().filter(|a| !a.is_empty()) {
        Some(sel) => rows.iter().find(|r| {
            r.get::<String, _>("label") == sel
                || r.try_get::<String, _>("email").map(|e| e == sel).unwrap_or(false)
        }),
        None => rows.first(),
    };
    let Some(row) = pick else {
        // Labels only — no secret.
        let labels: Vec<String> = rows.iter().map(|r| r.get::<String, _>("label")).collect();
        return Err(format!("unknown Google account; connected: {}", labels.join(", ")));
    };
    let id: String = row.get("id");
    let client = reqwest::Client::new();
    let token = crate::commands::gcalendar::access_token_for(&client, &id, req.refresh)
        .await
        // The messages name the failing step only, never a credential.
        .map_err(|e| format!("Google token refresh failed: {e}"))?;
    Ok(BrokerResponse::allow_token(token, "googleapis.com".into()))
}

/// Env + headers of a user MCP server that is enabled and in the requesting
/// run's set (its project's, with the session's own switches).
async fn user_server(req: &BrokerRequest, db: &Db, id: &str) -> Result<BrokerResponse, String> {
    let run_id = req.run_id.as_deref().unwrap_or_default();
    let on = crate::commands::run_mcp::run_uses_server(db, &req.project_id, run_id, id)
        .await
        .map_err(|_| "MCP server lookup failed".to_string())?;
    if !on {
        return Err("MCP server is not enabled for this session".into());
    }
    let secret = secrets::get_mcp_secrets(id);
    Ok(BrokerResponse::allow_mcp(secret.env, secret.headers))
}

/// The conductor's bearer header. The session is keyed by the conductor's own
/// run id, so a run can only ever obtain its own token.
fn conductor_headers(req: &BrokerRequest, app: Option<&AppHandle>) -> Result<BrokerResponse, String> {
    let run_id = req.run_id.as_deref().ok_or("missing run")?;
    let cst = app
        .and_then(|a| a.try_state::<crate::conductor::ConductorState>())
        .ok_or("conductor unavailable")?;
    let session = cst.session(run_id).ok_or("not a conductor run")?;
    if session.project_id != req.project_id {
        return Err("not a conductor run".into());
    }
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", session.token));
    Ok(BrokerResponse::allow_mcp(HashMap::new(), headers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::broker::approver::{Approver, FixedApprover};
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Arc;

    /// Run `r1` is live in project `p1`.
    struct Live;
    #[async_trait]
    impl ApproverResolver for Live {
        async fn resolve(&self, _run_id: Option<&str>) -> Option<Arc<dyn Approver>> {
            Some(Arc::new(FixedApprover(true)))
        }
        fn run_project(&self, run_id: &str) -> Option<String> {
            (run_id == "r1").then(|| "p1".to_string())
        }
    }
    struct Ended;
    #[async_trait]
    impl ApproverResolver for Ended {
        async fn resolve(&self, _run_id: Option<&str>) -> Option<Arc<dyn Approver>> {
            None
        }
    }

    async fn db() -> Db {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')",
            "INSERT INTO mcp_servers (id, name, transport, enabled, created_at) VALUES
                ('on', 'on', 'http', 1, 'c'), ('off', 'off', 'http', 0, 'c'), ('spare', 'spare', 'http', 1, 'c')",
            "INSERT INTO project_mcp_servers (project_id, server_id, enabled_at) VALUES ('p1', 'on', 'c'), ('p1', 'off', 'c')",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool
    }

    fn req(json: &str) -> BrokerRequest {
        serde_json::from_str(json).unwrap()
    }

    fn assert_denied(resp: &BrokerResponse) {
        assert_eq!(resp.decision, "deny");
        assert!(resp.token.is_none() && resp.env.is_none() && resp.headers.is_none());
    }

    #[tokio::test]
    async fn nothing_is_served_outside_a_live_run() {
        let db = db().await;
        for json in [
            r#"{"project_id":"p1","run_id":"r1","tool":"google","argv":[]}"#,
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"on"}"#,
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"@conductor"}"#,
        ] {
            assert_denied(&process(&req(json), &db, &Ended, None).await);
        }
    }

    #[tokio::test]
    async fn a_server_not_enabled_for_the_project_is_denied() {
        let db = db().await;
        for json in [
            // Disabled server.
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"off"}"#,
            // Unknown / missing server.
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"nope"}"#,
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[]}"#,
            // On app-wide, but not in the project's set.
            r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"spare"}"#,
        ] {
            assert_denied(&process(&req(json), &db, &Live, None).await);
        }
    }

    /// The session's own switches (UX-27) decide over the project's.
    #[tokio::test]
    async fn a_server_the_session_switched_off_is_denied() {
        let db = db().await;
        sqlx::query("INSERT INTO runs (id, project_id, type, created_at) VALUES ('r1', 'p1', 'session', 'c')")
            .execute(&db)
            .await
            .unwrap();
        crate::commands::settings::set_setting(&db, "feature.integrations.google", "true").await.unwrap();
        crate::commands::run_mcp::set_override(&db, "r1", "server:on", Some(false)).await.unwrap();
        crate::commands::run_mcp::set_override(&db, "r1", "builtin:google", Some(false)).await.unwrap();
        let on = req(r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"on"}"#);
        let resp = process(&on, &db, &Live, None).await;
        assert_denied(&resp);
        assert_eq!(resp.reason.as_deref(), Some("MCP server is not enabled for this session"));
        let google = req(r#"{"project_id":"p1","run_id":"r1","tool":"google","argv":[]}"#);
        let resp = process(&google, &db, &Live, None).await;
        assert_denied(&resp);
        assert_eq!(resp.reason.as_deref(), Some("the Google MCP server is switched off for this session"));
    }

    #[tokio::test]
    async fn a_run_cannot_claim_another_project() {
        let db = db().await;
        sqlx::query("INSERT INTO project_mcp_servers (project_id, server_id, enabled_at) VALUES ('p2', 'on', 'c')")
            .execute(&db)
            .await
            .unwrap();
        for json in [
            // `on` is enabled for p2, but r1 is p1's run.
            r#"{"project_id":"p2","run_id":"r1","tool":"mcp","argv":[],"server":"on"}"#,
            r#"{"project_id":"p2","run_id":"r1","tool":"google","argv":[]}"#,
            // Unknown run.
            r#"{"project_id":"p1","run_id":"r9","tool":"mcp","argv":[],"server":"on"}"#,
        ] {
            assert_denied(&process(&req(json), &db, &Live, None).await);
        }
    }

    /// A run launched while `integrations.google` was on keeps asking after
    /// the switch goes off: no token then.
    #[tokio::test]
    async fn google_token_needs_the_google_switch() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        crate::commands::settings::set_setting(&db, "feature.integrations.google", "false").await.unwrap();
        let r = req(r#"{"project_id":"p1","run_id":"r1","tool":"google","argv":[]}"#);
        let resp = process(&r, &db, &Live, None).await;
        assert_denied(&resp);
        assert_eq!(resp.reason.as_deref(), Some("feature_disabled: integrations.google"));
    }

    #[tokio::test]
    async fn conductor_token_needs_a_conductor_session() {
        let db = db().await;
        let r = req(r#"{"project_id":"p1","run_id":"r1","tool":"mcp","argv":[],"server":"@conductor"}"#);
        assert_denied(&process(&r, &db, &Live, None).await);
    }
}
