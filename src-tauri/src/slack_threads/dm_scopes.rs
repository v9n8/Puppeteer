//! Whether each Slack token can read DM threads: the check the Slack MCP server
//! detail runs when the global DM switch is turned on. It informs and blocks
//! nothing. Only tokens of Slack servers on app-wide are checked (a server that
//! is off never lends its token, `token`). `auth.test` is asked afresh, its
//! answer refreshes the cache, and the token itself never leaves this module
//! except towards Slack.

use super::api::{AuthInfo, HttpConfig};
use super::token::{self, AuthCache, SecretLookup};
use crate::db::Db;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// The history scopes of a 1:1 DM and of a group DM.
pub const DM_SCOPES: [&str; 2] = ["im:history", "mpim:history"];

/// Longest wait for one token's answer; past it the token reads as `token_error`.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// One token's result. No token field, on purpose.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DmScopeResult {
    pub server_id: String,
    pub server_name: String,
    /// `ok` | `missing` | `unknown` (Slack sent no scope list) | `token_error`
    pub status: &'static str,
    /// The DM scopes the token lacks (`missing` only).
    pub missing: Vec<String>,
}

/// Status and missing scopes from an `auth.test` answer; `None` = it failed.
pub fn classify(auth: Option<&AuthInfo>) -> (&'static str, Vec<String>) {
    let Some(auth) = auth else { return ("token_error", Vec::new()) };
    if auth.scopes.is_none() {
        return ("unknown", Vec::new());
    }
    let missing: Vec<String> = DM_SCOPES
        .iter()
        .filter(|s| auth.has_scope(s) == Some(false))
        .map(|s| s.to_string())
        .collect();
    (if missing.is_empty() { "ok" } else { "missing" }, missing)
}

/// Check every token of a Slack server on app-wide, each within `timeout`.
pub async fn check_all(
    db: &Db,
    lookup: &SecretLookup,
    cfg: &Arc<HttpConfig>,
    http: &reqwest::Client,
    cache: &AuthCache,
    timeout: Duration,
) -> Result<Vec<DmScopeResult>, sqlx::Error> {
    let mut out = Vec::new();
    for src in token::sources(db, lookup, None).await? {
        let auth = match tokio::time::timeout(timeout, src.client(cfg, http).auth_test()).await {
            Ok(Ok(info)) => {
                cache.put(src.cache_key(), info.clone());
                Some(info)
            }
            Ok(Err(e)) => {
                tracing::warn!(event = "slack_dm_scope_check_failed", server_id = %src.server_id, error = %e);
                None
            }
            Err(_) => {
                tracing::warn!(event = "slack_dm_scope_check_timeout", server_id = %src.server_id);
                None
            }
        };
        let (status, missing) = classify(auth.as_ref());
        out.push(DmScopeResult { server_id: src.server_id, server_name: src.server_name, status, missing });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn auth(scopes: Option<&str>) -> AuthInfo {
        AuthInfo {
            team_id: "T1".into(),
            team: "Malme".into(),
            user_id: "U1".into(),
            host: "malme.slack.com".into(),
            scopes: scopes.map(|s| s.split(',').map(str::to_string).collect::<HashSet<_>>()),
        }
    }

    #[test]
    fn both_dm_scopes_read_as_ok_and_a_missing_one_is_named() {
        assert_eq!(classify(Some(&auth(Some("channels:history,im:history,mpim:history")))), ("ok", vec![]));
        assert_eq!(classify(Some(&auth(Some("channels:history,mpim:history")))), ("missing", vec!["im:history".into()]));
        assert_eq!(
            classify(Some(&auth(Some("channels:history")))),
            ("missing", vec!["im:history".into(), "mpim:history".into()])
        );
    }

    #[test]
    fn no_scope_list_is_unknown_and_a_failed_check_is_a_token_error() {
        assert_eq!(classify(Some(&auth(None))), ("unknown", vec![]));
        assert_eq!(classify(None), ("token_error", vec![]));
    }
}
