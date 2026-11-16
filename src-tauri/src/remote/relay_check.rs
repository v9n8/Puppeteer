//! "Check relay" (Settings → Devices): dial the relay once, try `register_host`
//! with a token, close. Reports only what it saw. It starts no agent, changes
//! no setting and stores no token, so it is safe to run while Remote Control is
//! on (a relay does not drop the live Host for a register_host frame — it only
//! compares the token and answers).

use crate::db::Db;
use crate::remote::protocol::{Envelope, FrameType};
use crate::remote::{self, check_relay_url};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::State;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenOutcome {
    Accepted,
    Rejected,
    NoReply,
    NotChecked,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelayCheck {
    pub reachable: bool,
    pub connect_ms: Option<u64>,
    pub token: TokenOutcome,
    /// The relay's error code when it rejected the token (e.g. `auth_failed`).
    pub error_code: Option<String>,
    /// Why it could not connect, or why no verdict came back. Never has the token.
    pub error: Option<String>,
    /// Seconds waited for the relay's reply (only meaningful for `no_reply`).
    pub reply_wait_secs: u64,
}

impl RelayCheck {
    fn failed(error: String) -> Self {
        RelayCheck {
            reachable: false,
            connect_ms: None,
            token: TokenOutcome::NotChecked,
            error_code: None,
            error: Some(error),
            reply_wait_secs: 0,
        }
    }
}

/// Strip the token from text before it leaves this module.
fn scrub(text: String, token: Option<&str>) -> String {
    match token {
        Some(t) if !t.is_empty() => text.replace(t, "***"),
        _ => text,
    }
}

fn dial_error(e: &WsError) -> String {
    match e {
        WsError::Http(resp) => format!("HTTP {}", resp.status().as_u16()),
        other => other.to_string(),
    }
}

pub async fn check_relay(url: &str, token: Option<String>, device_id: String) -> RelayCheck {
    check_relay_with(url, token, device_id, CONNECT_TIMEOUT, REPLY_TIMEOUT).await
}

async fn check_relay_with(
    url: &str,
    token: Option<String>,
    device_id: String,
    connect_timeout: Duration,
    reply_timeout: Duration,
) -> RelayCheck {
    let url = url.trim();
    if let Err(e) = check_relay_url(url) {
        return RelayCheck::failed(e);
    }
    let token = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let started = Instant::now();
    let mut ws = match tokio::time::timeout(connect_timeout, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => return RelayCheck::failed(scrub(dial_error(&e), token.as_deref())),
        Err(_) => return RelayCheck::failed(format!("timed out after {} s", connect_timeout.as_secs())),
    };
    let mut out = RelayCheck {
        reachable: true,
        connect_ms: Some(started.elapsed().as_millis() as u64),
        token: TokenOutcome::NotChecked,
        error_code: None,
        error: None,
        reply_wait_secs: reply_timeout.as_secs(),
    };
    if let Some(token) = token {
        out.token = TokenOutcome::NoReply;
        match serde_json::to_string(&Envelope::register_host(device_id, token.clone())) {
            Ok(text) => {
                if let Err(e) = ws.send(Message::Text(text)).await {
                    out.error = Some(scrub(e.to_string(), Some(&token)));
                } else {
                    wait_verdict(&mut ws, reply_timeout, &token, &mut out).await;
                }
            }
            Err(e) => out.error = Some(e.to_string()),
        }
    }
    let _ = ws.close(None).await;
    out
}

/// Read frames until the relay's first verdict, the timeout, or a close.
async fn wait_verdict<S>(ws: &mut S, wait: Duration, token: &str, out: &mut RelayCheck)
where
    S: futures_util::Stream<Item = Result<Message, WsError>> + Unpin,
{
    let read = async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => return Some(text),
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
                Some(Ok(_)) => continue,
            }
        }
    };
    match tokio::time::timeout(wait, read).await {
        Err(_) => {}
        Ok(None) => out.error = Some("relay closed the connection before answering".into()),
        Ok(Some(text)) => match serde_json::from_str::<Envelope>(&text) {
            Ok(env) if env.t == FrameType::Control => out.token = TokenOutcome::Accepted,
            Ok(env) if env.t == FrameType::Error => {
                out.token = TokenOutcome::Rejected;
                out.error_code = env.code.map(|c| scrub(c, Some(token)));
            }
            Ok(env) => out.error = Some(format!("unexpected frame {:?}", env.t)),
            Err(_) => out.error = Some("unreadable reply from relay".into()),
        },
    }
}

/// Check a relay with the values being typed (not yet saved). An empty token
/// falls back to the one in the keychain.
#[tauri::command]
pub async fn remote_check_relay(
    db: State<'_, Db>,
    relay_url: String,
    auth_token: Option<String>,
) -> Result<RelayCheck, String> {
    let token = auth_token
        .filter(|t| !t.trim().is_empty())
        .or_else(crate::secrets::get_remote_auth_token);
    // Not `ensure_device_id`: this check must not write a setting.
    let device_id = remote::get_setting(db.inner(), remote::KEY_DEVICE_ID)
        .await
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(check_relay(&relay_url, token, device_id).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[derive(Clone, Copy)]
    enum Mode {
        Control,
        AuthFailed,
        Silent,
        CloseNow,
    }

    /// A fake relay on 127.0.0.1 that answers the first frame as `mode` says.
    async fn fake_relay(mode: Mode) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            if matches!(mode, Mode::CloseNow) {
                let _ = ws.close(None).await;
                return;
            }
            let _ = ws.next().await; // register_host
            let reply = match mode {
                Mode::Control => r#"{"t":"control","room_id":""}"#,
                Mode::AuthFailed => r#"{"t":"error","room_id":"","code":"auth_failed","message":"invalid auth token"}"#,
                _ => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    return;
                }
            };
            let _ = ws.send(Message::Text(reply.into())).await;
            let _ = ws.next().await;
        });
        format!("ws://{addr}/ws")
    }

    async fn run(mode: Mode, token: Option<&str>) -> RelayCheck {
        let url = fake_relay(mode).await;
        check_relay_with(&url, token.map(String::from), "dev".into(), Duration::from_secs(5), Duration::from_millis(300)).await
    }

    #[tokio::test]
    async fn accepted_on_control_ack() {
        let r = run(Mode::Control, Some("s3cret")).await;
        assert!(r.reachable && r.connect_ms.is_some());
        assert_eq!(r.token, TokenOutcome::Accepted);
    }

    #[tokio::test]
    async fn rejected_carries_relay_code() {
        let r = run(Mode::AuthFailed, Some("s3cret")).await;
        assert_eq!(r.token, TokenOutcome::Rejected);
        assert_eq!(r.error_code.as_deref(), Some("auth_failed"));
    }

    #[tokio::test]
    async fn silent_relay_is_no_reply() {
        let r = run(Mode::Silent, Some("s3cret")).await;
        assert!(r.reachable);
        assert_eq!(r.token, TokenOutcome::NoReply);
        assert!(r.error.is_none());
    }

    #[tokio::test]
    async fn closing_relay_is_no_reply_with_reason() {
        let r = run(Mode::CloseNow, Some("s3cret")).await;
        assert_eq!(r.token, TokenOutcome::NoReply);
        assert!(r.error.is_some());
    }

    #[tokio::test]
    async fn without_a_token_only_connects() {
        let r = run(Mode::Control, None).await;
        assert!(r.reachable);
        assert_eq!(r.token, TokenOutcome::NotChecked);
    }

    #[tokio::test]
    async fn bad_url_is_refused_before_dialing() {
        let r = check_relay("http://example.com/ws", Some("s3cret".into()), "dev".into()).await;
        assert!(!r.reachable);
        assert!(r.error.unwrap().starts_with("relay URL must start with wss://"));
    }

    #[tokio::test]
    async fn dial_error_never_contains_the_token() {
        // Nothing listens on port 1.
        let r = check_relay("ws://127.0.0.1:1/ws", Some("s3cret-token".into()), "dev".into()).await;
        assert!(!r.reachable);
        assert!(!format!("{r:?}").contains("s3cret-token"));
    }

    #[test]
    fn scrub_hides_the_token() {
        assert_eq!(scrub("bad s3cret here".into(), Some("s3cret")), "bad *** here");
    }
}
