use anyhow::{anyhow, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[cfg(not(all(debug_assertions, test)))]
const SERVICE_NAME: &str = "io.github.v9n8.puppeteer";

/// The consolidated item's service. Debug builds (`pnpm dev`, "Puppeteer Dev") use
/// their own item: two processes sharing one item each keep a cached copy and
/// write the whole store back, so they would overwrite each other's secrets.
#[cfg(debug_assertions)]
const STORE_SERVICE: &str = "io.github.v9n8.puppeteer.dev";
#[cfg(not(debug_assertions))]
const STORE_SERVICE: &str = SERVICE_NAME;

/// Single consolidated Keychain item that holds ALL PATs (GitHub + GitLab).
///
/// WHY ONE ITEM: on macOS every Keychain item has its own ACL bound to the app's
/// code signature. An ad-hoc / unsigned build gets a NEW signature on each
/// reinstall, so the OS re-prompts ("allow access…") for every item whose ACL no
/// longer matches. With one item per account the user was asked once *per
/// account*. Collapsing everything into ONE item means at most ONE prompt after a
/// reinstall, regardless of how many accounts are stored.
const STORE_KEY: &str = "puppeteer_secret_store_v1";

/// The OS store behind `keyring` (features in `Cargo.toml`), named in errors.
/// There is no fallback: without it secrets are not saved at all, never to a
/// plain file.
#[cfg(target_os = "macos")]
const STORE_NAME: &str = "macOS Keychain";
#[cfg(windows)]
const STORE_NAME: &str = "Windows Credential Manager";
#[cfg(not(any(target_os = "macos", windows)))]
const STORE_NAME: &str = "Secret Service (GNOME Keyring, KWallet… on D-Bus)";

/// Largest value one item may hold, or `None` when the store has no practical
/// limit. A Windows credential blob is at most 2560 bytes
/// (`CRED_MAX_CREDENTIAL_BLOB_SIZE`, which `keyring` enforces as `TooLong`),
/// far less than the store with a few secret notes, so there the store is
/// split across several items (see `write_raw`).
#[cfg(windows)]
const ITEM_MAX_BYTES: Option<usize> = Some(2560);
#[cfg(not(windows))]
const ITEM_MAX_BYTES: Option<usize> = None;

/// Main-item value of a store split into chunks: `<prefix><gen>:<count>`. Not
/// JSON, so a build that predates chunks fails to parse it (and then never
/// writes) instead of reading it as an empty store.
const CHUNK_HEADER: &str = "puppeteer-chunks-v1:";

/// Where a split store's bytes are: chunk `i` of generation `gen` holds bytes
/// `i * max ..` of the JSON.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chunks {
    gen: u8,
    count: usize,
}

/// In-memory view of the consolidated store. Keyed by account id → PAT.
#[derive(Default, Serialize, Deserialize, Clone)]
struct SecretStore {
    #[serde(default)]
    github: HashMap<String, String>,
    #[serde(default)]
    gitlab: HashMap<String, String>,
    /// MCP server secrets, keyed by server_id → env/header VALUEs. Kept in the
    /// SAME consolidated item as PATs so the whole app costs at most ONE Keychain
    /// prompt after a reinstall (see WHY ONE ITEM above).
    #[serde(default)]
    mcp: HashMap<String, McpSecrets>,
    /// Server secrets (SSH private-key passphrase), keyed by server_id. Kept in
    /// the SAME consolidated item as PATs/MCP so the whole app costs at most ONE
    /// Keychain prompt after a reinstall (see WHY ONE ITEM above).
    #[serde(default)]
    servers: HashMap<String, ServerSecrets>,
    /// AWS account secrets, keyed by aws_account_id. Secret Access Key and
    /// Session Token live here; SQLite only stores non-secret metadata.
    #[serde(default)]
    aws: HashMap<String, AwsSecrets>,
    /// Remote Control (C1) secrets: the Host↔relay `auth_token` (SEC-001) and the
    /// per-device pairing `psk` values (SEC-002/BR-003), kept in the SAME
    /// consolidated item so the whole app costs at most ONE Keychain prompt.
    #[serde(default)]
    remote: RemoteSecrets,
    /// Google OAuth client (Drive + Gmail): the user-pasted Desktop-app
    /// client_id/client_secret. Persisted at the APP level and REUSED across
    /// connect/disconnect cycles, so switching accounts never re-prompts for the
    /// credentials — only a fresh consent is needed.
    #[serde(default)]
    google_client: Option<GoogleClientCreds>,
    /// Connected Google accounts, keyed by account id → long-lived refresh_token.
    /// Metadata (label/email/scope/default) lives in the `google_accounts` DB
    /// table; only the secret token is here. Multiple accounts allowed.
    #[serde(default)]
    google_accounts: HashMap<String, String>,
    /// Secret notes: sealed data only, never a plaintext title or content (see
    /// `crate::secret_vault`).
    #[serde(default)]
    secret_vault: crate::secret_vault::VaultData,
    /// Why the store could not be read (access denied, bad JSON, no Secret
    /// Service on Linux). `Some`: this in-memory store is an empty stand-in
    /// and must never be written back, or it would wipe the real item.
    #[serde(skip)]
    unreadable: Option<String>,
    /// The split the OS store has now, so a write replaces it without reading
    /// the store again (a read can prompt on macOS).
    #[serde(skip)]
    layout: Option<Chunks>,
}

/// Process-lifetime cache of the decrypted store.
///
/// The FIRST access reads the Keychain (the single possible prompt); every
/// get/set/delete afterwards works purely in memory, so no further prompts appear
/// while the app is running — even if the user clicked "Allow" (not "Always
/// Allow"). Cleared only when the process exits.
///
/// SECURITY: this retains PATs in RAM for the app lifetime. That is an
/// intentional trade-off for the "prompt once per reinstall" UX; the app already
/// forwards these tokens to the broker/shim, so the retention surface is the same
/// class of secret it already handles.
static CACHE: Mutex<Option<SecretStore>> = Mutex::new(None);

enum Provider {
    Github,
    Gitlab,
}

fn store_entry() -> Result<Entry> {
    Ok(Entry::new(STORE_SERVICE, STORE_KEY)?)
}

/// Opens one item of a store: the main item (`STORE_KEY`) or a chunk.
type Open<'a> = &'a dyn Fn(&str) -> keyring::Result<Entry>;

fn os_item(user: &str) -> keyring::Result<Entry> {
    Entry::new(STORE_SERVICE, user)
}

fn chunk_user(gen: u8, i: usize) -> String {
    format!("{STORE_KEY}.{gen}.{i}")
}

fn parse_header(value: &str) -> Option<Chunks> {
    let (gen, count) = value.strip_prefix(CHUNK_HEADER)?.split_once(':')?;
    Some(Chunks { gen: gen.parse().ok()?, count: count.parse().ok()? })
}

/// The store's JSON and split, or `None` when there is no store yet. Any other
/// failure is an error, never `None`: a missing chunk or an unreachable OS
/// store must not pass for "no secrets", or the next write would wipe them.
fn read_raw(open: Open) -> Result<Option<(String, Option<Chunks>)>> {
    let head = match open(STORE_KEY).and_then(|e| e.get_password()) {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(anyhow!("{STORE_NAME} unavailable: {e}")),
    };
    let Some(chunks) = parse_header(&head) else {
        return Ok(Some((head, None)));
    };
    let mut bytes = Vec::new();
    for i in 0..chunks.count {
        let part = open(&chunk_user(chunks.gen, i))
            .and_then(|e| e.get_secret())
            .map_err(|e| anyhow!("{STORE_NAME}: secret store chunk {i} unreadable: {e}"))?;
        bytes.extend(part);
    }
    let json = String::from_utf8(bytes).map_err(|_| anyhow!("{STORE_NAME}: secret store chunks are not UTF-8"))?;
    Ok(Some((json, Some(chunks))))
}

/// Write `json` as the whole store, replacing the split `prev`; returns the new
/// split. A value over `max` goes to the other generation of chunks, and the
/// main item points to it only once every chunk is written, so a failed write
/// leaves the previous store whole.
fn write_raw(open: Open, json: &str, prev: Option<Chunks>, max: Option<usize>) -> keyring::Result<Option<Chunks>> {
    // Windows keeps a password as UTF-16, two bytes per unit.
    let next = match max.filter(|&m| json.encode_utf16().count() * 2 > m) {
        None => {
            open(STORE_KEY)?.set_password(json)?;
            None
        }
        Some(max) => {
            let gen = prev.map_or(0, |p| 1 - p.gen);
            let mut count = 0;
            for part in json.as_bytes().chunks(max) {
                open(&chunk_user(gen, count))?.set_secret(part)?;
                count += 1;
            }
            // Left by an earlier write that failed before switching.
            delete_chunks(open, gen, count);
            let next = Chunks { gen, count };
            open(STORE_KEY)?.set_password(&format!("{CHUNK_HEADER}{gen}:{count}"))?;
            Some(next)
        }
    };
    if let Some(p) = prev {
        delete_chunks(open, p.gen, 0);
    }
    Ok(next)
}

/// Delete chunks `from..` of `gen` until one is missing. Best effort: the main
/// item names the live chunks, so one left behind is never read.
fn delete_chunks(open: Open, gen: u8, from: usize) {
    for i in from.. {
        if open(&chunk_user(gen, i)).and_then(|e| e.delete_credential()).is_err() {
            break;
        }
    }
}

/// Load the store into the cache if not already loaded. This is the ONLY place
/// that may trigger a Keychain password prompt (besides `loaded_strict`).
fn ensure_loaded(guard: &mut Option<SecretStore>) {
    if guard.is_some() {
        return;
    }
    *guard = Some(read_store().unwrap_or_else(|e| SecretStore { unreadable: Some(e.to_string()), ..Default::default() }));
}

/// The OS store's item, or an empty store when there is none yet. Fails when
/// it exists but cannot be read or parsed, or the OS store is unreachable.
fn read_store() -> Result<SecretStore> {
    match read_raw(&os_item)? {
        Some((json, layout)) => Ok(SecretStore { layout, ..serde_json::from_str(&json)? }),
        None => Ok(seed_from_release()),
    }
}

/// A debug build without its own item yet starts from a copy of the release
/// app's (saved under its own service on the first write), so existing
/// accounts keep working in `pnpm dev`. Not the remote session: the dev app
/// would pose as the release one. Never in tests.
#[cfg(all(debug_assertions, not(test)))]
fn seed_from_release() -> SecretStore {
    let open = |user: &str| Entry::new(SERVICE_NAME, user);
    let mut store: SecretStore = read_raw(&open)
        .ok()
        .flatten()
        .and_then(|(json, _)| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    store.remote.session = None;
    store
}

#[cfg(not(all(debug_assertions, not(test))))]
fn seed_from_release() -> SecretStore {
    SecretStore::default()
}

/// Persist the whole store back to the OS store.
fn persist(store: &mut SecretStore) -> Result<()> {
    if let Some(why) = &store.unreadable {
        return Err(anyhow!("{why}; not overwriting a secret store this run could not read"));
    }
    let json = serde_json::to_string(store)?;
    store.layout = write_raw(&os_item, &json, store.layout, ITEM_MAX_BYTES)?;
    Ok(())
}

/// Whether the consolidated store item exists — metadata-only, prompt-free.
fn store_exists() -> bool {
    store_entry()
        .map(|e| e.get_attributes().is_ok())
        .unwrap_or(false)
}

fn map_of<'a>(store: &'a SecretStore, provider: &Provider) -> &'a HashMap<String, String> {
    match provider {
        Provider::Github => &store.github,
        Provider::Gitlab => &store.gitlab,
    }
}

fn map_of_mut<'a>(
    store: &'a mut SecretStore,
    provider: &Provider,
) -> &'a mut HashMap<String, String> {
    match provider {
        Provider::Github => &mut store.github,
        Provider::Gitlab => &mut store.gitlab,
    }
}

fn set_pat(provider: Provider, account_id: &str, pat: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    map_of_mut(store, &provider).insert(account_id.to_string(), pat.to_string());
    persist(store)?;
    Ok(())
}

fn get_pat(provider: Provider, account_id: &str) -> Result<String> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_ref().expect("store loaded");
    map_of(store, &provider)
        .get(account_id)
        .cloned()
        .ok_or_else(|| anyhow!("No PAT stored for account"))
}

fn delete_pat(provider: Provider, account_id: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    map_of_mut(store, &provider).remove(account_id);
    let _ = persist(store);
    Ok(())
}

/// Check whether a PAT exists WITHOUT reading its secret value (no prompt).
///
/// Resolution order, all prompt-free:
/// 1. If the store is already cached, answer precisely from memory.
/// 2. Otherwise fall back to "the consolidated store exists" — accounts are
///    always created together with their PAT, so this is accurate in practice
///    and never triggers a prompt before the first real read.
fn has_pat(provider: Provider, account_id: &str) -> bool {
    if let Ok(guard) = CACHE.lock() {
        if let Some(store) = guard.as_ref() {
            return map_of(store, &provider).contains_key(account_id);
        }
    }
    store_exists()
}

// ---- Public API (unchanged signatures; callers untouched) ------------------

pub fn set_account_pat(account_id: &str, pat: &str) -> Result<()> {
    set_pat(Provider::Github, account_id, pat)
}

pub fn get_account_pat(account_id: &str) -> Result<String> {
    get_pat(Provider::Github, account_id)
}

pub fn delete_account_pat(account_id: &str) -> Result<()> {
    delete_pat(Provider::Github, account_id)
}

pub fn has_account_pat(account_id: &str) -> bool {
    has_pat(Provider::Github, account_id)
}

pub fn set_gitlab_account_pat(account_id: &str, pat: &str) -> Result<()> {
    set_pat(Provider::Gitlab, account_id, pat)
}

pub fn get_gitlab_account_pat(account_id: &str) -> Result<String> {
    get_pat(Provider::Gitlab, account_id)
}

pub fn delete_gitlab_account_pat(account_id: &str) -> Result<()> {
    delete_pat(Provider::Gitlab, account_id)
}

pub fn has_gitlab_account_pat(account_id: &str) -> bool {
    has_pat(Provider::Gitlab, account_id)
}

// ---- MCP server secrets ----------------------------------------------------
//
// MCP secret VALUEs (`{"env":{KEY:VALUE...},"headers":{KEY:VALUE...}}`) live in
// the SAME consolidated Keychain item as the PATs, keyed by server_id. SQLite
// never holds the VALUEs — only the KEY names, for form rendering.
//
// WHY CONSOLIDATED: previously each server had its OWN item (service
// `puppeteer-mcp`). On macOS every item has a separate ACL bound to the app's code
// signature, so an ad-hoc/unsigned rebuild re-prompts ("allow access…") once
// PER item — i.e. once per MCP server. Folding the secrets into the single
// store means at most ONE prompt for the whole app, matching the PAT behaviour.

/// Decrypted secret payload for a single MCP server.
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct McpSecrets {
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Persist a server's secret env/header VALUEs into the consolidated store.
pub fn set_mcp_secrets(
    server_id: &str,
    env: &HashMap<String, String>,
    headers: &HashMap<String, String>,
) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.mcp.insert(
        server_id.to_string(),
        McpSecrets {
            env: env.clone(),
            headers: headers.clone(),
        },
    );
    persist(store)?;
    Ok(())
}

/// Read a server's secret env/header VALUEs. Fail-closed: returns an empty
/// payload if the item is missing or unreadable (never errors the caller).
pub fn get_mcp_secrets(server_id: &str) -> McpSecrets {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        Err(_) => return McpSecrets::default(),
    };
    ensure_loaded(&mut guard);
    let store = guard.as_ref().expect("store loaded");
    store.mcp.get(server_id).cloned().unwrap_or_default()
}

/// Delete a server's secrets from the consolidated store (no-op if absent).
pub fn delete_mcp_secrets(server_id: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.mcp.remove(server_id);
    let _ = persist(store);
    Ok(())
}

// ---- Google OAuth credentials ----------------------------------------------
//
// Powers the built-in `google` MCP server (Drive + Gmail + Calendar). Two
// layers, both in the consolidated Keychain item:
//   • CLIENT (GoogleClientCreds): the user-pasted OAuth Desktop-app
//     client_id/client_secret. Persisted at the APP level and REUSED across
//     connect/disconnect — switching accounts never re-prompts for these.
//   • ACCOUNT (`google_accounts`): each connected account's long-lived
//     refresh_token. Cleared on disconnect. Access tokens are NEVER stored —
//     the MCP servers mint short-lived ones on demand from the refresh_token.

/// The reusable OAuth Desktop-app client credentials (shared by all accounts).
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct GoogleClientCreds {
    pub client_id: String,
    pub client_secret: String,
}

/// Persist (or update) the reusable OAuth client credentials.
pub fn set_google_client(client_id: &str, client_secret: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.google_client = Some(GoogleClientCreds {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
    });
    persist(store)?;
    Ok(())
}

/// Read the saved OAuth client credentials, or `None` if never entered.
pub fn get_google_client() -> Option<GoogleClientCreds> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard.as_ref().and_then(|s| s.google_client.clone())
}

/// Whether reusable OAuth client credentials are saved.
pub fn has_google_client() -> bool {
    if let Ok(guard) = CACHE.lock() {
        if let Some(store) = guard.as_ref() {
            return store.google_client.is_some();
        }
    }
    store_exists()
}

/// Forget the saved OAuth client credentials AND every connected account's
/// token (they can no longer be refreshed without the client).
pub fn delete_google_client() -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.google_client = None;
    store.google_accounts.clear();
    persist(store)?;
    Ok(())
}

/// Persist an account's refresh_token (keyed by account id).
pub fn set_google_account_token(id: &str, refresh_token: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store
        .google_accounts
        .insert(id.to_string(), refresh_token.to_string());
    persist(store)?;
    Ok(())
}

/// Read one account's refresh_token, or `None` if absent.
pub fn get_google_account_token(id: &str) -> Option<String> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|s| s.google_accounts.get(id).cloned())
}

/// Remove one account's refresh_token (no-op if absent).
pub fn delete_google_account_token(id: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.google_accounts.remove(id);
    persist(store)?;
    Ok(())
}

// ---- VPS/server secrets ----------------------------------------------------
//
// The SSH private-key passphrase (if any) for a managed VPS lives in the SAME
// consolidated Keychain item as the PATs/MCP secrets, keyed by server_id.
// SQLite never holds the passphrase.

/// Decrypted secret payload for a single managed server.
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ServerSecrets {
    #[serde(default)]
    pub passphrase: Option<String>,
}

/// Persist a server's SSH private-key passphrase into the consolidated store.
pub fn set_server_secret(server_id: &str, passphrase: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.servers.insert(
        server_id.to_string(),
        ServerSecrets {
            passphrase: Some(passphrase.to_string()),
        },
    );
    persist(store)?;
    Ok(())
}

/// Read a server's secret payload. Fail-closed: returns an empty payload if the
/// item is missing or unreadable (never errors the caller).
///
/// Foundation API: GĐ1 stores the passphrase; the reader is consumed by the
/// later AI-deploy phase (GĐ3) that actually opens the SSH session.
#[allow(dead_code)]
pub fn get_server_secret(server_id: &str) -> ServerSecrets {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        Err(_) => return ServerSecrets::default(),
    };
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.servers.get(server_id).cloned().unwrap_or_default()
}

/// Delete a server's secret from the consolidated store (no-op if absent).
pub fn delete_server_secret(server_id: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.servers.remove(server_id);
    persist(store)?;
    Ok(())
}

/// Whether a passphrase is stored for `server_id` WITHOUT reading its value.
///
/// LOW-1 fix (AC-108): answer PRECISELY from the consolidated store instead of
/// the old `store_exists()` fallback, which was a false positive — the store
/// item exists as soon as ANY secret (a PAT, an MCP secret) is written, so a
/// server created without a passphrase would wrongly report `has_passphrase =
/// true`. We `ensure_loaded` (the single existing load path — no NEW Keychain
/// prompt beyond the one already amortized across the app) and read the exact
/// `servers` map entry. Fail-closed to `false` if the cache lock is poisoned.
pub fn has_server_secret(server_id: &str) -> bool {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|store| store.servers.get(server_id))
        .map(|s| s.passphrase.is_some())
        .unwrap_or(false)
}

// ---- AWS account secrets ---------------------------------------------------
//
// Secret Access Key and optional Session Token for a managed AWS account live in
// the SAME consolidated Keychain item as the PATs/MCP/server secrets, keyed by
// aws_account_id. SQLite never holds these values.

/// Decrypted secret payload for a single AWS account.
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct AwsSecrets {
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
}

/// Delete an AWS account's secret from the consolidated store (no-op if absent).
pub fn delete_aws_secret(account_id: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.aws.remove(account_id);
    persist(store)?;
    Ok(())
}

/// Whether a Secret Access Key is stored for `account_id` WITHOUT returning it.
pub fn has_aws_secret(account_id: &str) -> bool {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|store| store.aws.get(account_id))
        .and_then(|s| s.secret_access_key.as_ref())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

// ---- Remote Control (C1) secrets -------------------------------------------
//
// The Host↔relay `auth_token` (SEC-001) and each paired device's pairing `psk`
// (SEC-002/BR-003) live in the SAME consolidated Keychain item, so the whole app
// still costs at most ONE Keychain prompt after a reinstall. SQLite never holds
// either value.

/// Remote Control secret material kept in the consolidated store.
#[derive(Default, Serialize, Deserialize, Clone)]
struct RemoteSecrets {
    /// Host→relay auth token (SEC-001). `None` until the Owner enables remote.
    #[serde(default)]
    auth_token: Option<String>,
    /// The single active remote session's reconnect credential. `None` when no
    /// session is bound. Holds the reusable rendezvous code, the Host's E2E
    /// keypair (so the pinned fingerprint stays valid), the `session_secret`
    /// (mixed into the reconnect KDF instead of the OTP) and the sliding idle
    /// expiry.
    #[serde(default)]
    session: Option<RemoteSession>,
    /// Optional owner-chosen master password. When set, a controller may
    /// authenticate with it (mixed into the pairing KDF exactly like the OTP)
    /// instead of reading the per-join one-time code — simpler for a single
    /// owner. `None`/empty falls back to the OTP flow.
    #[serde(default)]
    master_password: Option<String>,
    /// Web Push VAPID private key (P-256 scalar, base64url). Generated once;
    /// the matching public key is what every push subscription is bound to, so
    /// losing it silently invalidates them all.
    #[serde(default)]
    vapid_private_key: Option<String>,
}

/// The single active remote session's durable reconnect credential.
#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteSession {
    /// The single bound run this session controls.
    pub run_id: String,
    /// Reusable relay rendezvous code (bearer routing key) the controller rejoins.
    pub rendezvous_code: String,
    /// Session secret (hex) mixed into the reconnect-KDF in place of the OTP.
    pub session_secret: String,
    /// Host ephemeral X25519 public key (hex) — reused so the fingerprint the
    /// controller pinned still matches on reconnect.
    pub host_pub: String,
    /// Host ephemeral X25519 secret key (hex). Secret — Keychain only.
    pub host_secret: String,
    /// Sliding idle expiry as a Unix timestamp (seconds). Refreshed on each
    /// accepted command; past it the session must re-authenticate with the OTP.
    pub idle_expires_at: u64,
    /// The session's link secret (hex). Both sides key the reconnect KDF with
    /// it, so a rehydrated session needs it to accept a token reconnect, and
    /// password login hands it to a Controller that lost its link. Older
    /// credentials lack it (empty).
    #[serde(default)]
    pub link_secret: String,
}

/// Persist the Host↔relay auth token.
pub fn set_remote_auth_token(token: &str) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.remote.auth_token = Some(token.to_string());
    persist(store)
}

/// Read the Host↔relay auth token, if any.
pub fn get_remote_auth_token() -> Option<String> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|store| store.remote.auth_token.clone())
        .filter(|t| !t.trim().is_empty())
}

/// Whether a non-empty auth token is stored WITHOUT returning its value.
pub fn has_remote_auth_token() -> bool {
    get_remote_auth_token().is_some()
}

/// Delete the Host↔relay auth token (no-op if absent). Part of the Remote
/// Control secret API surface (used when the Owner clears remote config).
#[allow(dead_code)]
pub fn delete_remote_auth_token() -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.remote.auth_token = None;
    persist(store)
}

/// Shortest master password accepted when one is set or changed. A shorter one
/// stored before this rule keeps working: only setting checks it.
pub const MIN_MASTER_PASSWORD_LEN: usize = 12;

fn check_master_password(password: &str) -> Result<()> {
    if !password.trim().is_empty() && password.chars().count() < MIN_MASTER_PASSWORD_LEN {
        return Err(anyhow!("master password must be at least {MIN_MASTER_PASSWORD_LEN} characters"));
    }
    Ok(())
}

/// Persist (or clear) the owner's remote master password. An empty string
/// clears it (falls back to the OTP flow).
pub fn set_remote_master_password(password: &str) -> Result<()> {
    check_master_password(password)?;
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.remote.master_password = if password.trim().is_empty() {
        None
    } else {
        Some(password.to_string())
    };
    persist(store)
}

/// Read the owner's remote master password, if a non-empty one is stored.
pub fn get_remote_master_password() -> Option<String> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|store| store.remote.master_password.clone())
        .filter(|p| !p.trim().is_empty())
}

/// Whether a non-empty master password is stored WITHOUT returning its value.
pub fn has_remote_master_password() -> bool {
    get_remote_master_password().is_some()
}

/// Read the Web Push VAPID private key (base64url), if one was generated.
pub fn get_remote_vapid_key() -> Option<String> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard
        .as_ref()
        .and_then(|store| store.remote.vapid_private_key.clone())
        .filter(|k| !k.trim().is_empty())
}

/// The Web Push VAPID private key (base64url), minted with `generate` and
/// stored if there is none yet. Read-or-create happens under one lock, so two
/// callers racing on first use cannot each keep a different key.
pub fn get_or_create_remote_vapid_key(generate: impl FnOnce() -> String) -> Result<String> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    if let Some(k) = store.remote.vapid_private_key.clone().filter(|k| !k.trim().is_empty()) {
        return Ok(k);
    }
    let key = generate();
    store.remote.vapid_private_key = Some(key.clone());
    if let Err(e) = persist(store) {
        // Not kept: a later attempt mints again rather than trusting a key
        // that would vanish with this process.
        store.remote.vapid_private_key = None;
        return Err(e);
    }
    Ok(key)
}

/// Persist (or replace) the single remote session's reconnect credential.
pub fn set_remote_session(session: &RemoteSession) -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.remote.session = Some(session.clone());
    persist(store)
}

/// Read the single remote session's reconnect credential, if any.
pub fn get_remote_session() -> Option<RemoteSession> {
    let mut guard = CACHE.lock().ok()?;
    ensure_loaded(&mut guard);
    guard.as_ref().and_then(|store| store.remote.session.clone())
}

/// Delete the remote session credential (end-session / idle expiry). No-op if
/// absent.
pub fn delete_remote_session() -> Result<()> {
    let mut guard = CACHE.lock().map_err(|_| anyhow!("secret cache poisoned"))?;
    ensure_loaded(&mut guard);
    let store = guard.as_mut().expect("store loaded");
    store.remote.session = None;
    persist(store)
}

// ---- Secret notes vault -------------------------------------------------------

/// Load the store, failing (instead of defaulting to empty) when the Keychain
/// item exists but cannot be read.
/// An earlier read that failed is retried here (an explicit user action),
/// never answered from the empty stand-in.
fn loaded_strict(guard: &mut Option<SecretStore>) -> Result<&mut SecretStore> {
    if guard.as_ref().is_none_or(|s| s.unreadable.is_some()) {
        *guard = Some(read_store()?);
    }
    Ok(guard.as_mut().expect("store loaded"))
}

/// Run `f` on the vault data. With `write`, the result is persisted only when `f`
/// succeeds. Fails (rather than starting from an empty store) when the Keychain
/// item exists but cannot be read, so a write can never wipe it.
pub fn with_vault<R>(
    write: bool,
    f: impl FnOnce(&mut crate::secret_vault::VaultData) -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = CACHE.lock().map_err(|_| "secret cache poisoned".to_string())?;
    let store = loaded_strict(&mut guard).map_err(|e| e.to_string())?;
    if !write {
        let mut data = store.secret_vault.clone();
        return f(&mut data);
    }
    let mut data = store.secret_vault.clone();
    let out = f(&mut data)?;
    let previous = std::mem::replace(&mut store.secret_vault, data);
    if let Err(e) = persist(store) {
        store.secret_vault = previous;
        return Err(format!("keychain write failed: {e}"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyring::credential::CredentialApi;
    use std::sync::Arc;

    type Items = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    /// One item of an in-memory store that, like a Windows credential, refuses
    /// a value over `limit` bytes (a password counted as UTF-16). Tests never
    /// touch the machine's real store.
    struct MemItem {
        items: Items,
        user: String,
        limit: usize,
        fail: bool,
    }

    impl MemItem {
        fn put(&self, bytes: &[u8]) -> keyring::Result<()> {
            if bytes.len() > self.limit {
                return Err(keyring::Error::TooLong("secret".into(), self.limit as u32));
            }
            self.items.lock().unwrap().insert(self.user.clone(), bytes.to_vec());
            Ok(())
        }
    }

    fn platform_failure() -> keyring::Error {
        keyring::Error::PlatformFailure("no D-Bus session".into())
    }

    impl CredentialApi for MemItem {
        fn set_password(&self, password: &str) -> keyring::Result<()> {
            if password.encode_utf16().count() * 2 > self.limit {
                return Err(keyring::Error::TooLong("password".into(), self.limit as u32));
            }
            self.set_secret(password.as_bytes())
        }
        fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
            if self.fail {
                return Err(platform_failure());
            }
            self.put(secret)
        }
        fn get_secret(&self) -> keyring::Result<Vec<u8>> {
            if self.fail {
                return Err(platform_failure());
            }
            self.items.lock().unwrap().get(&self.user).cloned().ok_or(keyring::Error::NoEntry)
        }
        fn delete_credential(&self) -> keyring::Result<()> {
            self.items.lock().unwrap().remove(&self.user).map(|_| ()).ok_or(keyring::Error::NoEntry)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    const WIN: usize = 2560;

    /// An `Open` over `items`; `fail_user` makes that one item fail like an
    /// unreachable store.
    fn mem(items: &Items, limit: usize, fail_user: Option<String>) -> impl Fn(&str) -> keyring::Result<Entry> {
        let items = items.clone();
        move |user: &str| {
            let fail = fail_user.as_deref().is_some_and(|f| f == user || f == "*");
            Ok(Entry::new_with_credential(Box::new(MemItem { items: items.clone(), user: user.to_string(), limit, fail })))
        }
    }

    /// A store JSON of about `kb` KiB with non-ASCII text, so chunk edges fall
    /// inside multi-byte characters.
    fn big_json(kb: usize) -> String {
        let mut store = SecretStore::default();
        for i in 0..kb * 8 {
            store.github.insert(format!("acc-{i}"), format!("ghp_é😀{i:0>100}"));
        }
        serde_json::to_string(&store).unwrap()
    }

    fn users(items: &Items) -> Vec<String> {
        let mut v: Vec<String> = items.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    #[test]
    fn without_a_limit_the_store_stays_one_item() {
        // macOS / Linux: same single item (service + account) as before, whatever the size.
        let items = Items::default();
        let json = big_json(64);
        assert_eq!(write_raw(&mem(&items, usize::MAX, None), &json, None, None).unwrap(), None);
        assert_eq!(users(&items), vec![STORE_KEY.to_string()]);
        assert_eq!(read_raw(&mem(&items, usize::MAX, None)).unwrap(), Some((json, None)));
    }

    #[test]
    fn a_small_store_is_one_windows_item() {
        let items = Items::default();
        let json = serde_json::to_string(&SecretStore::default()).unwrap();
        assert_eq!(write_raw(&mem(&items, WIN, None), &json, None, Some(WIN)).unwrap(), None);
        assert_eq!(users(&items), vec![STORE_KEY.to_string()]);
        assert_eq!(read_raw(&mem(&items, WIN, None)).unwrap(), Some((json, None)));
    }

    #[test]
    fn a_large_store_is_split_within_the_windows_limit() {
        let items = Items::default();
        let open = mem(&items, WIN, None);
        let json = big_json(20);
        // The single item alone would be refused, as Windows does.
        assert!(matches!(open(STORE_KEY).unwrap().set_password(&json), Err(keyring::Error::TooLong(..))));

        let layout = write_raw(&open, &json, None, Some(WIN)).unwrap().expect("split");
        assert_eq!(layout.gen, 0);
        assert_eq!(layout.count, json.len().div_ceil(WIN));
        assert_eq!(users(&items).len(), layout.count + 1);
        let (back, read_layout) = read_raw(&open).unwrap().unwrap();
        assert_eq!(back, json);
        assert_eq!(read_layout, Some(layout));
        let store: SecretStore = serde_json::from_str(&back).unwrap();
        assert_eq!(store.github.len(), 20 * 8);
    }

    #[test]
    fn a_rewrite_switches_generation_and_drops_old_chunks() {
        let items = Items::default();
        let open = mem(&items, WIN, None);
        let first = write_raw(&open, &big_json(20), None, Some(WIN)).unwrap();
        let smaller = big_json(6);
        let second = write_raw(&open, &smaller, first, Some(WIN)).unwrap().expect("split");
        assert_eq!(second.gen, 1);
        let mut want: Vec<String> = (0..second.count).map(|i| chunk_user(1, i)).collect();
        want.push(STORE_KEY.to_string());
        want.sort();
        assert_eq!(users(&items), want);
        assert_eq!(read_raw(&open).unwrap().unwrap().0, smaller);

        // Shrinking back to one item removes every chunk.
        let tiny = serde_json::to_string(&SecretStore::default()).unwrap();
        assert_eq!(write_raw(&open, &tiny, Some(second), Some(WIN)).unwrap(), None);
        assert_eq!(users(&items), vec![STORE_KEY.to_string()]);
    }

    #[test]
    fn a_failed_split_write_keeps_the_previous_store() {
        let items = Items::default();
        let old = big_json(20);
        let layout = write_raw(&mem(&items, WIN, None), &old, None, Some(WIN)).unwrap();
        let failing = mem(&items, WIN, Some(chunk_user(1, 5)));
        assert!(write_raw(&failing, &big_json(30), layout, Some(WIN)).is_err());
        assert_eq!(read_raw(&mem(&items, WIN, None)).unwrap(), Some((old.clone(), layout)));

        // The next write reuses generation 1 and clears what the failed one left.
        let new = big_json(4);
        let next = write_raw(&mem(&items, WIN, None), &new, layout, Some(WIN)).unwrap().unwrap();
        assert_eq!(users(&items).len(), next.count + 1);
        assert_eq!(read_raw(&mem(&items, WIN, None)).unwrap().unwrap().0, new);
    }

    #[test]
    fn a_missing_chunk_is_an_error_not_an_empty_store() {
        let items = Items::default();
        write_raw(&mem(&items, WIN, None), &big_json(20), None, Some(WIN)).unwrap();
        items.lock().unwrap().remove(&chunk_user(0, 1));
        let err = read_raw(&mem(&items, WIN, None)).unwrap_err().to_string();
        assert!(err.contains("chunk 1"), "{err}");
    }

    #[test]
    fn an_unreachable_store_is_an_error_and_is_never_written() {
        // Linux without a Secret Service: an error, not "no secrets yet".
        let items = Items::default();
        let err = read_raw(&mem(&items, usize::MAX, Some("*".into()))).unwrap_err().to_string();
        assert!(err.starts_with(&format!("{STORE_NAME} unavailable:")), "{err}");

        // The stand-in for it refuses to persist (before reaching any OS store).
        let mut stand_in = SecretStore { unreadable: Some(err.clone()), ..Default::default() };
        let refused = persist(&mut stand_in).unwrap_err().to_string();
        assert!(refused.starts_with(&err), "{refused}");
    }

    #[test]
    fn a_master_password_needs_twelve_characters() {
        assert!(check_master_password(&"a".repeat(11)).is_err());
        // Refused before the store is touched.
        assert!(set_remote_master_password("elevenchars").is_err());
        assert!(check_master_password(&"a".repeat(12)).is_ok());
        assert!(check_master_password(&"é".repeat(12)).is_ok(), "counts characters, not bytes");
        assert!(check_master_password("").is_ok(), "empty clears the password");
    }

    #[test]
    fn the_chunk_header_is_not_a_store() {
        let header = format!("{CHUNK_HEADER}1:12");
        assert_eq!(parse_header(&header), Some(Chunks { gen: 1, count: 12 }));
        assert!(serde_json::from_str::<SecretStore>(&header).is_err());
        assert_eq!(parse_header(&serde_json::to_string(&SecretStore::default()).unwrap()), None);
    }
}
