//! Password login: reach the Host from the bare controller page with only the
//! owner's master password — for when the session link is lost.
//!
//! While a master password is set, the agent also listens on the well-known
//! [`LOGIN_CODE`] in the relay's `login` slot. A Controller joins it, both sides
//! run the usual X25519 handshake, and the E2E key is
//! `derive_effective_psk(LOGIN_LINK_SECRET, hex(PBKDF2(password)))` — so only a
//! peer that knows the password can open or produce a frame. The first frame
//! that opens proves the password; the Host answers with a `login_granted`
//! carrying a normal session link + token, and the Controller continues on the
//! regular session path (fingerprint pinned from then on).
//!
//! Brute force is bounded at the Host by [`FailureLimiter`]: a controller key
//! gets [`MAX_FAILURES`] wrong passwords per [`FAILURE_WINDOW`], and every
//! failure past the first few (from any key) adds a short global backoff capped
//! at [`BACKOFF_CAP`]. PBKDF2 makes an offline guess against a captured frame
//! expensive.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use remote_e2e::derive_effective_psk;

/// Rendezvous code of the password-login slot. Public by design: knowing it
/// only lets a peer *try* a password, which the Host rate limits.
pub const LOGIN_CODE: &str = "puppeteer-login-v1";
/// Longest accepted device alias.
pub const MAX_ALIAS_LEN: usize = 32;

/// Whether `alias` is a valid device alias: `[a-z0-9-]{0,32}` (empty = none).
pub fn valid_alias(alias: &str) -> bool {
    alias.len() <= MAX_ALIAS_LEN
        && alias
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The login rendezvous code for this device: [`LOGIN_CODE`], or
/// `LOGIN_CODE/<alias>` when the owner named it, so hosts sharing a relay (and
/// possibly a master password) each get their own login slot. Must match
/// `loginCodeFor` in `src/lib/remoteLogin.ts`.
pub fn login_code(alias: Option<&str>) -> String {
    match alias.filter(|a| !a.is_empty()) {
        Some(a) => format!("{LOGIN_CODE}/{a}"),
        None => LOGIN_CODE.to_string(),
    }
}

/// Relay listen slot for the login code.
pub const LOGIN_SLOT: &str = "login";
/// Public stand-in for the link secret in the login KDF (hex of
/// `"puppeteer remote login v1 pubkey"`). Must match the controller.
pub const LOGIN_LINK_SECRET: &str = "7075707065746565722072656d6f7465206c6f67696e207631207075626b6579";
/// PBKDF2-HMAC-SHA256 salt and rounds. Must match the controller.
const PBKDF2_SALT: &[u8] = b"puppeteer-remote-login-v1";
pub const PBKDF2_ROUNDS: u32 = 210_000;

/// Wrong passwords one controller key may try per window before it is refused.
pub const MAX_FAILURES: usize = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(300);
/// Failures in a row (any key) that add no global backoff; each later one
/// doubles it from [`BACKOFF_BASE`] up to [`BACKOFF_CAP`].
const BACKOFF_FREE: u32 = 5;
const BACKOFF_BASE: Duration = Duration::from_secs(1);
pub const BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Stretched password → hex, the `secret2` fed to the login KDF.
fn stretch(password: &str) -> String {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), PBKDF2_SALT, PBKDF2_ROUNDS, &mut out);
    crate::remote::pairing_hex(&out)
}

/// The login PSK for `password`. PBKDF2 is deliberately slow, so the result is
/// cached for the current password (it only changes when the owner sets a new
/// one). Run it off the async runtime.
pub fn login_psk(password: &str) -> [u8; 32] {
    static CACHE: Mutex<Option<(String, [u8; 32])>> = Mutex::new(None);
    if let Ok(guard) = CACHE.lock() {
        if let Some((pw, psk)) = guard.as_ref() {
            if pw == password {
                return *psk;
            }
        }
    }
    let psk = derive_login_psk(password);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((password.to_string(), psk));
    }
    psk
}

/// The login PSK for `password`, uncached. Slow: run it off the async runtime.
fn derive_login_psk(password: &str) -> [u8; 32] {
    derive_effective_psk(LOGIN_LINK_SECRET, &stretch(password))
}

/// Failed login attempts (process-wide, so re-dialing the relay does not
/// reset it).
///
/// The login code is public, so a hard global lockout lets any stranger keep
/// the owner out with one bad attempt a minute. Counting per controller key
/// alone does not slow brute force either: a key costs nothing to mint (the
/// controller makes a fresh one per attempt). So a key is refused after
/// [`MAX_FAILURES`], and the global part only spaces attempts out: past
/// [`BACKOFF_FREE`] failures in a row, the next attempt waits a doubling
/// backoff that never exceeds [`BACKOFF_CAP`] — at most one guess per cap,
/// while the owner is never kept out longer than that once the noise stops.
pub struct FailureLimiter {
    by_key: BTreeMap<Vec<u8>, VecDeque<Instant>>,
    /// Failures in a row from any key; reset after a quiet [`FAILURE_WINDOW`].
    streak: u32,
    last: Option<Instant>,
}

impl FailureLimiter {
    pub const fn new() -> Self {
        Self {
            by_key: BTreeMap::new(),
            streak: 0,
            last: None,
        }
    }

    fn prune(&mut self, now: Instant) {
        let fresh = |t: &Instant| now.duration_since(*t) < FAILURE_WINDOW;
        self.by_key.retain(|_, failures| {
            while failures.front().is_some_and(|t| !fresh(t)) {
                failures.pop_front();
            }
            !failures.is_empty()
        });
        if self.last.is_some_and(|t| !fresh(&t)) {
            self.streak = 0;
            self.last = None;
        }
    }

    fn backoff(&self) -> Duration {
        match self.streak.checked_sub(BACKOFF_FREE) {
            None | Some(0) => Duration::ZERO,
            Some(n) => BACKOFF_BASE
                .saturating_mul(1u32.checked_shl(n - 1).unwrap_or(u32::MAX))
                .min(BACKOFF_CAP),
        }
    }

    /// Whether controller key `key` may try a password now.
    pub fn allow(&mut self, key: &[u8], now: Instant) -> bool {
        self.prune(now);
        if self.by_key.get(key).is_some_and(|f| f.len() >= MAX_FAILURES) {
            return false;
        }
        self.last.is_none_or(|t| now >= t + self.backoff())
    }

    pub fn record_failure(&mut self, key: &[u8], now: Instant) {
        self.prune(now);
        self.by_key.entry(key.to_vec()).or_default().push_back(now);
        self.streak = self.streak.saturating_add(1);
        self.last = Some(now);
    }
}

pub static FAILURES: Mutex<FailureLimiter> = Mutex::new(FailureLimiter::new());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psk_is_deterministic_and_password_bound() {
        let a = login_psk("correct horse");
        assert_eq!(a, login_psk("correct horse"));
        assert_ne!(a, login_psk("correct horsf"));
    }

    /// Cross-language vector: `src/controller/__tests__/login.test.mjs` asserts
    /// the controller derives the same bytes.
    #[test]
    fn psk_matches_the_controller_vector() {
        assert_eq!(
            crate::remote::pairing_hex(&login_psk("puppeteer-test-password")),
            LOGIN_PSK_VECTOR
        );
    }
    const LOGIN_PSK_VECTOR: &str = "95db3565ad5fad0b134b257f669773234d2750ebe9ed3350bc4c5f5715f2286b";

    #[test]
    fn login_code_appends_the_alias() {
        assert_eq!(login_code(None), "puppeteer-login-v1");
        assert_eq!(login_code(Some("")), "puppeteer-login-v1");
        assert_eq!(login_code(Some("mac-mini")), "puppeteer-login-v1/mac-mini");
    }

    #[test]
    fn alias_validation() {
        assert!(valid_alias(""));
        assert!(valid_alias("mac-mini-2"));
        assert!(valid_alias(&"a".repeat(MAX_ALIAS_LEN)));
        assert!(!valid_alias(&"a".repeat(MAX_ALIAS_LEN + 1)));
        assert!(!valid_alias("Mac"));
        assert!(!valid_alias("mac mini"));
        assert!(!valid_alias("mac/mini"));
        assert!(!valid_alias("máy"));
    }

    const A: &[u8] = &[0xa; 32];
    const B: &[u8] = &[0xb; 32];

    #[test]
    fn limiter_blocks_a_key_after_max_failures_then_recovers() {
        let mut l = FailureLimiter::new();
        let t0 = Instant::now();
        for _ in 0..MAX_FAILURES {
            assert!(l.allow(A, t0));
            l.record_failure(A, t0);
        }
        assert!(!l.allow(A, t0));
        assert!(!l.allow(A, t0 + BACKOFF_CAP));
        assert!(l.allow(A, t0 + FAILURE_WINDOW));
    }

    #[test]
    fn failures_from_a_do_not_block_b() {
        let mut l = FailureLimiter::new();
        let t0 = Instant::now();
        for _ in 0..MAX_FAILURES {
            l.record_failure(A, t0);
        }
        assert!(!l.allow(A, t0));
        assert!(l.allow(B, t0));
    }

    /// The R-34 lockout: a stranger sending one bad attempt a minute must not
    /// keep the owner out for longer than the backoff cap.
    #[test]
    fn one_garbage_per_minute_does_not_lock_owner_out() {
        let mut l = FailureLimiter::new();
        let t0 = Instant::now();
        let mut last = t0;
        for i in 0..20u8 {
            last = t0 + Duration::from_secs(60 * u64::from(i));
            l.record_failure(&[i; 32], last);
        }
        assert!(!l.allow(B, last + BACKOFF_CAP - Duration::from_secs(1)));
        assert!(l.allow(B, last + BACKOFF_CAP));
    }

    /// Fresh keys do not buy a faster brute force: past the first few, guesses
    /// are spaced at least by the growing backoff, then by the cap.
    #[test]
    fn brute_force_from_fresh_keys_is_slowed() {
        let mut l = FailureLimiter::new();
        let t0 = Instant::now();
        let span = Duration::from_secs(600);
        let mut tried = 0u32;
        let mut t = t0;
        while t < t0 + span {
            let key = tried.to_be_bytes();
            if l.allow(&key, t) {
                l.record_failure(&key, t);
                tried += 1;
            }
            t += Duration::from_millis(100);
        }
        // 5 free + 1 + 2 + 4 + 8 + 16 s of backoff, then one per cap.
        let ramp = 1 + 2 + 4 + 8 + 16;
        let max = BACKOFF_FREE + 5 + (span.as_secs() as u32 - ramp) / BACKOFF_CAP.as_secs() as u32 + 1;
        assert!(tried <= max, "{tried} guesses in 10 min (max {max})");
        assert!(tried >= max - 1, "{tried} guesses in 10 min (expected about {max})");
    }

    #[test]
    fn a_quiet_window_clears_the_backoff() {
        let mut l = FailureLimiter::new();
        let t0 = Instant::now();
        for i in 0..10u8 {
            l.record_failure(&[i; 32], t0);
        }
        assert!(!l.allow(B, t0 + Duration::from_secs(1)));
        let later = t0 + FAILURE_WINDOW;
        assert!(l.allow(B, later));
        l.record_failure(B, later);
        assert!(l.allow(A, later), "one failure after a quiet window is free again");
    }
}
