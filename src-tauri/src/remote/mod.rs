//! Remote Control (C1) — Host agent (U3).
//!
//! Backend that lets a Controller device drive a running Puppeteer session over a
//! self-hosted Cloud Relay, end-to-end encrypted (SRS-RC-C1). This module owns:
//!   - the outbound WSS client to the relay ([`agent`]),
//!   - the per-session bound model + link/OTP/session-secret generation ([`session`]),
//!   - the in-process event tap the run drain publishes to ([`bus`]),
//!   - the prioritised outbound queue to the relay ([`outbound`]),
//!   - sealing + forwarding run events and history replay ([`forwarder`]),
//!   - the security-gated command handler ([`handler`]) built on the pure rules
//!     in [`command`],
//!   - the append-only audit ([`audit`]),
//!   - the wire protocol ([`protocol`]),
//!   - the Tauri commands the Host UI (U5) calls ([`commands`]).
//!
//! Secrets never touch SQLite or logs: the relay `auth_token` and per-device
//! `psk` live in the Keychain (`crate::secrets`). The relay only ever sees
//! ciphertext + `room_id` (SEC-006 / CON-04).

pub mod agent;
pub mod audit;
pub mod bus;
pub mod command;
pub mod commands;
pub mod data;
pub mod file_proxy;
pub mod forwarder;
pub mod fs_bridge;
pub mod fs_policy;
pub mod guard;
pub mod handler;
pub mod ipc;
pub mod login;
pub mod meta;
pub mod outbound;
pub mod protocol;
pub mod push;
pub mod session;
pub mod transfer;
pub mod upload;

pub use bus::{RemoteBus, RemoteRunEvent};
pub use meta::{apply_run_meta, RunMetaStore};

use crate::db::Db;
use crate::features::{self, apply::Change, ModuleState};
use crate::runs::{BrokerApprovals, RunRegistry};
use session::BoundSession;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as TokioMutex;

/// Runs whose live transcript the Controller is currently streaming.
///
/// Shared between the command handler (which mutates it on `open_run` /
/// `close_run`) and the forwarder (which reads it per event), so navigation
/// takes effect on the very next event rather than at the next reconnect.
pub type RunSubscriptions = Arc<TokioMutex<HashSet<String>>>;

/// The run the Controller is currently looking at. Read by `remote_status` so
/// the desktop can show where the phone is; `None` before authentication.
pub type FocusRun = Arc<TokioMutex<Option<String>>>;

/// Persisted-config settings keys (DATA-001). `relay_url` + `enabled` live in
/// the `settings` KV; the `device_id` is generated once and also kept there.
pub const KEY_RELAY_URL: &str = "remote_relay_url";
pub const KEY_ENABLED: &str = "remote_enabled";
pub const KEY_DEVICE_ID: &str = "remote_device_id";
/// Optional short name of this device; selects its password-login code so
/// several Puppeteer hosts can share one relay (see `login::login_code`).
pub const KEY_DEVICE_ALIAS: &str = "remote_device_alias";

/// Feature switches (`crate::features`): the relay agent, its Web Push
/// dispatcher, and what a remote desktop may do through the bridge.
pub const FEATURE: &str = "remote.remote_control";
pub const PUSH_FEATURE: &str = "remote.web_push";
pub const DESKTOP_FEATURE: &str = "remote.remote_desktop";
pub const TRANSFER_FEATURE: &str = "remote.file_transfer";

/// How long stopping waits for the agent to leave on its own (see
/// `RemoteState::stop_locked`).
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Live control handle for the running agent task.
struct AgentControl {
    stop: tokio::sync::watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
}

/// App-wide Remote Control state (Tauri managed).
#[derive(Clone)]
pub struct RemoteState {
    pub bus: RemoteBus,
    /// The single bound remote session (one run/room at a time). `None` when no
    /// remote control is active.
    pub bound: Arc<TokioMutex<Option<BoundSession>>>,
    pub connected: Arc<AtomicBool>,
    /// True while the agent's host↔relay socket is down and it is retrying with
    /// backoff (as opposed to a clean idle wait, where the socket is up). Lets the
    /// desktop badge show a truthful "reconnecting…" instead of a bare
    /// "disconnected" during a transient network blip.
    pub reconnecting: Arc<AtomicBool>,
    /// Signalled whenever the bound session changes (new link or ended) so a
    /// running agent re-syncs its relay listen without waiting for a reconnect.
    pub announce_signal: Arc<tokio::sync::Notify>,
    /// Signalled by the owner's kill switch: the agent revokes the current
    /// Controller room (it may re-join; ending the session blocks it).
    pub kick_signal: Arc<tokio::sync::Notify>,
    /// Runs the Controller is streaming right now. Lives here (not in the agent
    /// task) so `remote_status` can report it to the desktop UI.
    pub subscriptions: RunSubscriptions,
    /// The run the Controller is currently viewing.
    pub focus_run_id: FocusRun,
    /// Remote-desktop IPC routing (another Puppeteer driving this one).
    pub ipc: ipc::IpcHub,
    /// Web Push to subscribed phones; its dispatcher runs with the agent.
    pub push: push::PushHub,
    agent: Arc<TokioMutex<Option<AgentControl>>>,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteState {
    pub fn new() -> Self {
        RemoteState {
            bus: RemoteBus::new(),
            bound: Arc::new(TokioMutex::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
            reconnecting: Arc::new(AtomicBool::new(false)),
            announce_signal: Arc::new(tokio::sync::Notify::new()),
            kick_signal: Arc::new(tokio::sync::Notify::new()),
            subscriptions: Arc::new(TokioMutex::new(HashSet::new())),
            focus_run_id: Arc::new(TokioMutex::new(None)),
            ipc: ipc::IpcHub::default(),
            push: push::PushHub::default(),
            agent: Arc::new(TokioMutex::new(None)),
        }
    }

    /// Whether the agent task is currently running (not the socket state).
    pub async fn is_running(&self) -> bool {
        self.agent.lock().await.is_some()
    }

    /// Turn Remote Control on (`remote_enable`): persist `remote_enabled` and
    /// start the agent. Refused while [`FEATURE`] is off, before anything is
    /// written, so switching the feature on later does not connect by itself.
    /// Starting also needs a relay URL and an auth_token in the keyring
    /// (SEC-001).
    pub async fn enable(
        &self,
        app: &AppHandle,
        db: &Db,
        registry: &RunRegistry,
        broker_approvals: &BrokerApprovals,
    ) -> Result<(), String> {
        let mut slot = self.agent.lock().await;
        // Checked under the agent lock: a switch turned off meanwhile has its
        // `follow` wait here, and it stops what this starts.
        features::require(db, FEATURE).await?;
        set_setting(db, KEY_ENABLED, "true").await?;
        self.start_locked(&mut slot, app, db, registry, broker_approvals).await
    }

    /// Stop the agent task if running and wait for it to tear down its room.
    pub async fn stop(&self) {
        let mut slot = self.agent.lock().await;
        self.stop_locked(&mut slot).await;
    }

    /// Start or stop the agent to match `remote_enabled` and the switches as
    /// stored now (startup, `features::apply`). They are read under the agent
    /// lock, so racing calls end on the latest write.
    pub async fn follow(
        &self,
        app: &AppHandle,
        db: &Db,
        registry: &RunRegistry,
        broker_approvals: &BrokerApprovals,
    ) {
        let mut slot = self.agent.lock().await;
        let enabled = get_setting(db, KEY_ENABLED).await.is_some_and(|v| v == "true");
        // A failed read counts as off, as in `features::is_on`.
        let tree = features::commands::load(db).await.unwrap_or_default();
        if !wants_agent(enabled, &tree) {
            self.stop_locked(&mut slot).await;
            return;
        }
        if let Err(e) = self.start_locked(&mut slot, app, db, registry, broker_approvals).await {
            tracing::warn!(event = "remote_autostart_failed", error = %e);
        }
    }

    /// Spawn the agent unless it runs, and run its Web Push dispatcher while
    /// [`PUSH_FEATURE`] is on. Caller holds the agent lock.
    async fn start_locked(
        &self,
        slot: &mut Option<AgentControl>,
        app: &AppHandle,
        db: &Db,
        registry: &RunRegistry,
        broker_approvals: &BrokerApprovals,
    ) -> Result<(), String> {
        if slot.is_none() {
            let relay_url = get_setting(db, KEY_RELAY_URL).await.unwrap_or_default();
            if relay_url.trim().is_empty() {
                return Err("relay URL is not configured".to_string());
            }
            let auth_token = crate::secrets::get_remote_auth_token()
                .ok_or_else(|| "relay auth token is not set (enable requires a token)".to_string())?;
            let device_id = ensure_device_id(db).await?;

            let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
            let deps = agent::AgentDeps {
                app: app.clone(),
                db: db.clone(),
                registry: registry.clone(),
                broker_approvals: broker_approvals.clone(),
                bus: self.bus.clone(),
                bound: self.bound.clone(),
                connected: self.connected.clone(),
                reconnecting: self.reconnecting.clone(),
                announce_signal: self.announce_signal.clone(),
                kick_signal: self.kick_signal.clone(),
                subscriptions: self.subscriptions.clone(),
                focus_run_id: self.focus_run_id.clone(),
                ipc: self.ipc.clone(),
            };
            let config = agent::AgentConfig {
                relay_url,
                device_id,
            };
            let handle = tokio::spawn(agent::run_agent(config, deps, auth_token, stop_rx));
            *slot = Some(AgentControl {
                stop: stop_tx,
                handle,
            });
        }
        if features::is_on(db, PUSH_FEATURE).await {
            self.push.start(db.clone(), self.bus.clone());
        } else {
            self.push.stop_dispatch();
        }
        Ok(())
    }

    /// Caller holds the agent lock.
    async fn stop_locked(&self, slot: &mut Option<AgentControl>) {
        if let Some(mut ctrl) = slot.take() {
            let _ = ctrl.stop.send(true);
            // On its way out the agent drops its room (`agent::drop_room`:
            // clears `authenticated`, detaches a remote desktop, ends the
            // forwarder) and its socket writer. Aborting at once skipped all of
            // it. Abort only an agent stuck in a command past the grace period.
            if tokio::time::timeout(STOP_GRACE, &mut ctrl.handle).await.is_err() {
                tracing::warn!(event = "remote_agent_stop_timeout", secs = STOP_GRACE.as_secs());
                ctrl.handle.abort();
            }
        }
        self.push.stop();
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.reconnecting
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Whether the relay agent should run: the owner turned Remote Control on
/// (`remote_enabled`) and [`FEATURE`] is on in `tree`.
fn wants_agent(enabled: bool, tree: &[ModuleState]) -> bool {
    enabled && features::effective(tree, FEATURE)
}

/// `features::apply` hook: start or stop the agent, or its Web Push
/// dispatcher, as soon as their switch changes, including through the module.
/// The remote-desktop switches are read per command (`handler`).
pub fn on_features_changed(app: &AppHandle, change: &Change<'_>) {
    let same = |id| features::effective(change.before, id) == features::effective(change.after, id);
    if same(FEATURE) && same(PUSH_FEATURE) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<RemoteState>().inner().clone();
        let db = app.state::<Db>().inner().clone();
        let registry = app.state::<RunRegistry>().inner().clone();
        let approvals = app.state::<BrokerApprovals>().inner().clone();
        state.follow(&app, &db, &registry, &approvals).await;
    });
}

/// Lowercase hex encode (shared by agent/pairing for pubkey exchange).
pub fn pairing_hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 0x0f) as usize] as char);
    }
    s
}

/// Decode a lowercase/uppercase hex string; `None` on any invalid character.
pub fn pairing_unhex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    let val = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks_exact(2) {
        out.push((val(pair[0])? << 4) | val(pair[1])?);
    }
    Some(out)
}

/// Read a `settings` value.
pub(crate) async fn get_setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
}

/// Write a `settings` value.
pub(crate) async fn set_setting(db: &Db, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The configured device alias, when set and valid.
pub(crate) async fn device_alias(db: &Db) -> Option<String> {
    get_setting(db, KEY_DEVICE_ALIAS)
        .await
        .filter(|a| login::valid_alias(a))
}

/// Return the Host's stable `device_id`, generating + persisting one on first
/// use (DATA-001).
pub(crate) async fn ensure_device_id(db: &Db) -> Result<String, String> {
    if let Some(id) = get_setting(db, KEY_DEVICE_ID).await {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    set_setting(db, KEY_DEVICE_ID, &id).await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn agent_runs_only_when_enabled_and_switched_on() {
        let tree = |pairs: &[(&str, &str)]| {
            features::resolve(&pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>())
        };
        let on = tree(&[("feature.remote", "true"), ("feature.remote.remote_control", "true")]);
        assert!(wants_agent(true, &on));
        assert!(!wants_agent(false, &on), "turning the module back on does not connect by itself");
        // Fresh install: the module and the prop are off.
        assert!(!wants_agent(true, &tree(&[])));
        let module_off = tree(&[("feature.remote", "false"), ("feature.remote.remote_control", "true")]);
        assert!(!wants_agent(true, &module_off));
        let prop_off = tree(&[("feature.remote", "true"), ("feature.remote.remote_control", "false")]);
        assert!(!wants_agent(true, &prop_off));
    }

    #[tokio::test]
    async fn enabling_is_refused_while_the_feature_is_off() {
        let db = push::store::tests::mem_db().await;
        // Fresh install: module and prop are off.
        assert_eq!(features::require(&db, FEATURE).await.unwrap_err(), "feature_disabled: remote.remote_control");
        set_setting(&db, "feature.remote", "true").await.unwrap();
        assert!(features::require(&db, FEATURE).await.is_err(), "the prop is still off");
        set_setting(&db, "feature.remote.remote_control", "true").await.unwrap();
        assert_eq!(features::require(&db, FEATURE).await, Ok(()));
        set_setting(&db, "feature.remote", "false").await.unwrap();
        assert!(features::require(&db, FEATURE).await.is_err(), "module off wins over the prop");
    }

    #[tokio::test]
    async fn stopping_lets_the_agent_drop_its_room() {
        use std::sync::atomic::Ordering::SeqCst;
        let state = RemoteState::new();
        let mut bound = session::generate("run_1").unwrap();
        bound.authenticated = true;
        *state.bound.lock().await = Some(bound);
        state.connected.store(true, SeqCst);
        // Stands in for `agent::run_agent`: on stop it leaves through an await,
        // then clears the session as `agent::drop_room` does.
        let (stop, mut stop_rx) = tokio::sync::watch::channel(false);
        let agent_bound = state.bound.clone();
        let handle = tokio::spawn(async move {
            let _ = stop_rx.changed().await;
            tokio::task::yield_now().await;
            if let Some(b) = agent_bound.lock().await.as_mut() {
                b.authenticated = false;
            }
        });
        *state.agent.lock().await = Some(AgentControl { stop, handle });

        state.stop().await;

        assert!(!state.is_running().await);
        assert!(!state.bound.lock().await.as_ref().unwrap().authenticated, "the agent's teardown ran");
        assert!(!state.connected.load(SeqCst));
        assert!(!state.reconnecting.load(SeqCst));
    }

    #[test]
    fn hex_roundtrips() {
        let bytes = [0u8, 1, 15, 16, 255, 128, 64];
        let s = pairing_hex(&bytes);
        assert_eq!(s, "00010f10ff8040");
        assert_eq!(pairing_unhex(&s).unwrap(), bytes);
        assert_eq!(pairing_unhex("00FF").unwrap(), vec![0x00, 0xff]);
        assert!(pairing_unhex("xyz").is_none());
        assert!(pairing_unhex("abc").is_none(), "odd length rejected");
    }
}
