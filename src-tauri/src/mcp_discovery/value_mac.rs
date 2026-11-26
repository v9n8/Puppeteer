//! Whether a source's env / header VALUEs changed since import, without keeping the values.
//!
//! A row's fingerprint is the hash of the launch fields and key names (`import::mcp_candidate`),
//! then `.<key id>.<mac>`: an HMAC-SHA256 of the values under a random key of this install. The
//! key is a file in the app data (0600), not in the database, so neither the database nor a
//! settings backup holds anything a value could be guessed from; no vault (Keychain) read is
//! needed to compare, at reload or anywhere else.
//!
//! A fingerprint stored before this (no `.`), or made under another install's key (a restored
//! backup: other key id), is compared on its launch-field hash only, as before.

use crate::commands::mcp::McpServerExport;
use pbkdf2::hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::path::Path;

/// The key's file in the app data folder.
const KEY_FILE: &str = "mcp-source-key";
const KEY_BYTES: usize = 32;

/// This install's key. No `Serialize`, and `Debug` never shows it.
pub struct ValueKey([u8; KEY_BYTES]);

impl std::fmt::Debug for ValueKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ValueKey(..)")
    }
}

impl ValueKey {
    /// The key in `dir`, made on first use. An unreadable or malformed file is an error: the
    /// caller then compares launch fields only, rather than replace a key other copies used.
    pub fn load_or_create(dir: &Path) -> Result<Self, String> {
        let path = dir.join(KEY_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::create(dir, &path, write_private),
            Err(e) => Err(format!("{KEY_FILE} could not be read: {e}")),
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
        let bytes = hex::decode(text.trim()).map_err(|_| format!("{KEY_FILE} is not a key"))?;
        let key: [u8; KEY_BYTES] = bytes.try_into().map_err(|_| format!("{KEY_FILE} is not a key"))?;
        Ok(Self(key))
    }

    /// `write` puts the new key in a file; a test makes it fail half way.
    fn create(dir: &Path, path: &Path, write: fn(&Path, &str) -> std::io::Result<()>) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        use rand::RngCore;
        let mut key = [0u8; KEY_BYTES];
        rand::thread_rng().fill_bytes(&mut key);
        // Written under a temporary name and linked into place, so a failed or interrupted
        // write never leaves a partial key file, which would turn value tracking off for good.
        let mut tag = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut tag);
        let tmp = dir.join(format!("{KEY_FILE}.{}.tmp", hex::encode(tag)));
        let linked = write(&tmp, &hex::encode(key)).and_then(|()| std::fs::hard_link(&tmp, path));
        // The temporary name has done its job whether or not the link worked.
        let _ = std::fs::remove_file(&tmp);
        match linked {
            Ok(()) => Ok(Self(key)),
            // Another scan made it first: use that one.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            }
            Err(e) => Err(format!("{KEY_FILE} could not be created: {e}")),
        }
    }

    fn mac(&self, data: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.0).expect("HMAC takes a key of any length");
        mac.update(data);
        hex::encode(mac.finalize().into_bytes())
    }

    /// Names which key made a MAC; says nothing about the key.
    fn id(&self) -> String {
        self.mac(b"puppeteer mcp source key id")[..8].to_string()
    }

    /// `hash` with the MAC of `export`'s env and header values.
    pub fn fingerprint(&self, hash: &str, export: &McpServerExport) -> String {
        let values: (BTreeMap<_, _>, BTreeMap<_, _>) = (export.env.iter().collect(), export.headers.iter().collect());
        let data = serde_json::to_vec(&values).unwrap_or_default();
        format!("{hash}.{}.{}", self.id(), self.mac(&data))
    }
}

/// A new file at `path` holding `text`, readable by the user only.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Whether the source (`current`) differs from what the copy was made from (`stored`).
pub fn changed(stored: Option<&str>, current: &str) -> bool {
    let Some(stored) = stored else { return true };
    let split = |f: &str| -> (String, Option<(String, String)>) {
        let mut parts = f.splitn(3, '.');
        let hash = parts.next().unwrap_or_default().to_string();
        let mac = match (parts.next(), parts.next()) {
            (Some(id), Some(mac)) => Some((id.to_string(), mac.to_string())),
            _ => None,
        };
        (hash, mac)
    };
    let ((stored_hash, stored_mac), (current_hash, current_mac)) = (split(stored), split(current));
    if stored_hash != current_hash {
        return true;
    }
    match (stored_mac, current_mac) {
        (Some((stored_id, stored_mac)), Some((current_id, current_mac))) if stored_id == current_id => stored_mac != current_mac,
        _ => false,
    }
}

#[cfg(test)]
impl ValueKey {
    pub(crate) fn of(byte: u8) -> Self {
        Self([byte; KEY_BYTES])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn export(env: &[(&str, &str)], headers: &[(&str, &str)]) -> McpServerExport {
        let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>();
        McpServerExport {
            name: "x".into(),
            description: String::new(),
            transport: "stdio".into(),
            command: Some("x".into()),
            args: vec![],
            url: None,
            env: map(env),
            headers: map(headers),
            enabled: false,
        }
    }

    #[test]
    fn a_value_change_alone_changes_the_fingerprint_but_never_shows_the_value() {
        let key = ValueKey::of(1);
        let before = key.fingerprint("h", &export(&[("TOKEN", "SECRET-A"), ("REGION", "eu")], &[("Authorization", "Bearer SECRET-H")]));
        // Same values in another order: same fingerprint.
        let same = key.fingerprint("h", &export(&[("REGION", "eu"), ("TOKEN", "SECRET-A")], &[("Authorization", "Bearer SECRET-H")]));
        let rotated = key.fingerprint("h", &export(&[("TOKEN", "SECRET-B"), ("REGION", "eu")], &[("Authorization", "Bearer SECRET-H")]));
        let header = key.fingerprint("h", &export(&[("TOKEN", "SECRET-A"), ("REGION", "eu")], &[("Authorization", "Bearer SECRET-I")]));
        assert_eq!(before, same);
        assert!(!changed(Some(&before), &same));
        assert!(changed(Some(&before), &rotated));
        assert!(changed(Some(&before), &header));
        assert!(!before.contains("SECRET") && before.starts_with("h."), "{before}");
        // Another install's key gives another MAC: the key, not the values alone, decides it.
        assert_ne!(ValueKey::of(2).fingerprint("h", &export(&[("TOKEN", "SECRET-A")], &[])), key.fingerprint("h", &export(&[("TOKEN", "SECRET-A")], &[])));
        assert_eq!(format!("{key:?}"), "ValueKey(..)");
    }

    #[test]
    fn older_or_foreign_fingerprints_compare_on_the_launch_fields_only() {
        let key = ValueKey::of(1);
        let current = key.fingerprint("h", &export(&[("TOKEN", "new")], &[]));
        // Stored before values were compared.
        assert!(!changed(Some("h"), &current));
        assert!(changed(Some("other"), &current));
        // Made under another install's key (a restored backup).
        let foreign = ValueKey::of(2).fingerprint("h", &export(&[("TOKEN", "old")], &[]));
        assert!(!changed(Some(&foreign), &current));
        // No key at this scan.
        assert!(!changed(Some(&current), "h"));
        assert!(changed(None, "h"));
    }

    #[test]
    fn the_key_is_made_once_private_and_kept() {
        let dir = std::env::temp_dir().join(format!("value-mac-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = ValueKey::load_or_create(&dir).unwrap();
        let again = ValueKey::load_or_create(&dir).unwrap();
        assert_eq!(first.0, again.0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::write(dir.join(KEY_FILE), "not hex").unwrap();
        assert!(ValueKey::load_or_create(&dir).is_err());
        // Only the key file is left behind, never a temporary one.
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, [KEY_FILE]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_whose_write_fails_half_way_leaves_no_file() {
        let dir = std::env::temp_dir().join(format!("value-mac-half-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        fn half_write(path: &Path, _: &str) -> std::io::Result<()> {
            std::fs::File::create(path)?;
            Err(std::io::Error::other("disk full"))
        }
        let made = ValueKey::create(&dir, &dir.join(KEY_FILE), half_write);
        let left = std::fs::read_dir(&dir).unwrap().count();
        // The next scan makes a whole key instead of finding an empty one.
        let next = ValueKey::load_or_create(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(made.is_err());
        assert_eq!(left, 0);
        assert!(next.is_ok());
    }
}
