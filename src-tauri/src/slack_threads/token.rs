//! Slack token lookup: MCP servers whose env holds `SLACK_MCP_XOXP_TOKEN`
//! (VALUE in the Keychain), workspace matching via `auth.test` and the scope
//! list from its `x-oauth-scopes` header. Only servers that are on count
//! (`mcp_effective::servers_on`): on app-wide, and for a project's thread on
//! for that project; a server switched off never lends its token.

use super::api::{AuthInfo, HttpConfig, SlackClient, SlackError};
use super::CodedError;
use crate::db::Db;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TOKEN_ENV_KEY: &str = "SLACK_MCP_XOXP_TOKEN";
const AUTH_CACHE_TTL: Duration = Duration::from_secs(3600);

/// `server_id → token value` (the Keychain in the app, a map in tests).
pub type SecretLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Reads the token VALUE of an MCP server from the Keychain store.
pub fn keychain_lookup() -> SecretLookup {
    Arc::new(|server_id: &str| {
        crate::secrets::get_mcp_secrets(server_id)
            .env
            .get(TOKEN_ENV_KEY)
            .cloned()
    })
}

/// A usable token and the MCP server it came from. Not `Debug` on purpose.
#[derive(Clone)]
pub struct TokenSource {
    pub server_id: String,
    pub server_name: String,
    token: String,
}

impl TokenSource {
    /// A client that sends this token.
    pub(super) fn client(&self, cfg: &Arc<HttpConfig>, http: &reqwest::Client) -> SlackClient {
        SlackClient::new(cfg.clone(), http.clone(), self.token.clone())
    }

    pub(super) fn cache_key(&self) -> String {
        format!(
            "{}:{}",
            self.server_id,
            hex::encode(Sha256::digest(self.token.as_bytes()))
        )
    }
}

fn looks_like_token(v: &str) -> bool {
    let v = v.trim();
    v.starts_with("xoxp-") || v.starts_with("xoxb-")
}

/// The Slack MCP servers that are on (see the module docs), oldest first: `(id, name)`.
/// `project` = the thread's project; `None` = app-wide.
async fn slack_servers(db: &Db, project: Option<&str>) -> Result<Vec<(String, String)>, sqlx::Error> {
    let on = crate::commands::mcp_effective::servers_on(db, project).await?;
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, env_keys FROM mcp_servers ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|(id, _, keys)| {
            on.contains(id)
                && keys
                    .as_deref()
                    .and_then(|k| serde_json::from_str::<Vec<String>>(k).ok())
                    .is_some_and(|k| k.iter().any(|x| x == TOKEN_ENV_KEY))
        })
        .map(|(id, name, _)| (id, name))
        .collect())
}

/// Whether a Slack MCP server is on for `project_id` (no Keychain read).
pub async fn slack_on_for_project(db: &Db, project_id: &str) -> Result<bool, sqlx::Error> {
    Ok(!slack_servers(db, Some(project_id)).await?.is_empty())
}

/// Every Slack MCP server that is on for `project` (see [`slack_servers`]) with a usable token.
pub async fn sources(
    db: &Db,
    lookup: &SecretLookup,
    project: Option<&str>,
) -> Result<Vec<TokenSource>, sqlx::Error> {
    let candidates = slack_servers(db, project).await?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    // The Keychain read may block (first access), keep it off the runtime.
    let lookup = lookup.clone();
    let found = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|(id, name)| {
                let token = lookup(&id)?.trim().to_string();
                looks_like_token(&token).then_some(TokenSource {
                    server_id: id,
                    server_name: name,
                    token,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Ok(found)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenSourceInfo {
    pub server_id: String,
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SlackTokenStatus {
    /// `ok` | `none` | `multiple`
    pub state: &'static str,
    pub sources: Vec<TokenSourceInfo>,
}

/// Error code of a project's thread when a Slack server on app-wide holds a token
/// but none is switched on for that project.
pub const OFF_FOR_PROJECT: &str = "slack_off_for_project";

/// Why [`sources`] for `project` came back empty: [`OFF_FOR_PROJECT`] when a Slack
/// server on app-wide holds a usable token, else `no_token`.
pub async fn missing_token_code(
    db: &Db,
    lookup: &SecretLookup,
    project: Option<&str>,
) -> Result<&'static str, sqlx::Error> {
    if project.is_some() && !sources(db, lookup, None).await?.is_empty() {
        return Ok(OFF_FOR_PROJECT);
    }
    Ok("no_token")
}

/// The English detail stored with a [`missing_token_code`].
pub fn missing_token_detail(code: &str) -> &'static str {
    if code == OFF_FOR_PROJECT {
        "no Slack MCP server is switched on for the thread's project"
    } else {
        "no Slack token configured"
    }
}

/// Token availability (servers on app-wide) without any network call.
pub async fn token_status(db: &Db, lookup: &SecretLookup) -> Result<SlackTokenStatus, sqlx::Error> {
    let found = sources(db, lookup, None).await?;
    Ok(SlackTokenStatus {
        state: match found.len() {
            0 => "none",
            1 => "ok",
            _ => "multiple",
        },
        sources: found
            .into_iter()
            .map(|s| TokenSourceInfo {
                server_id: s.server_id,
                server_name: s.server_name,
            })
            .collect(),
    })
}

/// In-memory `auth.test` cache keyed by `server_id + sha256(token)`.
#[derive(Clone, Default)]
pub struct AuthCache(Arc<Mutex<HashMap<String, (Instant, AuthInfo)>>>);

impl AuthCache {
    fn get(&self, key: &str) -> Option<AuthInfo> {
        let g = self.0.lock().ok()?;
        g.get(key)
            .filter(|(at, _)| at.elapsed() < AUTH_CACHE_TTL)
            .map(|(_, a)| a.clone())
    }

    #[cfg(test)]
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    pub(super) fn put(&self, key: String, info: AuthInfo) {
        if let Ok(mut g) = self.0.lock() {
            g.insert(key, (Instant::now(), info));
        }
    }
}

/// Fatal mapping for `auth.test` failures.
pub fn auth_error(e: SlackError) -> CodedError {
    match e {
        SlackError::Api { code, .. } => match code.as_str() {
            "invalid_auth" | "not_authed" | "token_revoked" | "token_expired"
            | "account_inactive" => CodedError::new("invalid_auth", code),
            _ => CodedError::new("slack_error", code),
        },
        SlackError::RateLimited => CodedError::new("rate_limited", "rate limited by Slack"),
        SlackError::Network(e) => CodedError::new("network", e),
        SlackError::Http(e) => CodedError::new("slack_error", e),
    }
}

/// Pick the token for `host` and return a client plus its `auth.test` info.
/// One token → used as is; several → the one whose `auth.test` URL host is
/// `host` (earliest server wins).
pub async fn resolve(
    found: &[TokenSource],
    host: &str,
    cfg: &Arc<HttpConfig>,
    http: &reqwest::Client,
    cache: &AuthCache,
) -> Result<(SlackClient, AuthInfo), CodedError> {
    if found.is_empty() {
        return Err(CodedError::new("no_token", "no Slack token configured"));
    }
    for src in found {
        let client = src.client(cfg, http);
        let key = src.cache_key();
        let info = match cache.get(&key) {
            Some(i) => i,
            None => match client.auth_test().await {
                Ok(i) => {
                    cache.put(key, i.clone());
                    i
                }
                // With several tokens a failing one simply doesn't match.
                Err(e) if found.len() == 1 => return Err(auth_error(e)),
                Err(_) => continue,
            },
        };
        if found.len() == 1 || info.host == host {
            return Ok((client, info));
        }
    }
    Err(CodedError::new(
        "no_token_for_workspace",
        format!("no Slack token for {host}"),
    ))
}
