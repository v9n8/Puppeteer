//! Find the `codex` binary and read its version, for every place that runs
//! Codex (session start / resume, translation, model list, usage probe).
//!
//! The `codex_path` setting defaults to a bare `codex`, but a GUI-launched app
//! often has no `codex` on PATH even after `augment_command_path`: Codex.app
//! ships its CLI inside the bundle. So a bare `codex` that PATH can't find is
//! looked up in the usual install locations before giving up.

use crate::runs::sidecar::augmented_path;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Oldest codex-cli Puppeteer supports. Older ones still run, with a warning in
/// the run log. Only the numbers count: `0.133.0-alpha.1` meets it.
const MIN_VERSION: (u64, u64, u64) = (0, 133, 0);
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const NOT_FOUND_HINT: &str = "Install Codex, or set the Codex binary path in Settings → Engines.";

/// A resolved Codex binary and the version it reported (None if `--version`
/// failed or timed out).
#[derive(Debug, Clone)]
pub struct CodexBin {
    pub path: PathBuf,
    pub version: Option<String>,
}

impl CodexBin {
    /// Hand the binary to a Codex sidecar. The path is always set, so the
    /// sidecar never falls back to its own PATH lookup.
    pub fn apply_env(&self, cmd: &mut tokio::process::Command) {
        cmd.env("PUPPETEER_CODEX_PATH", &self.path);
        if let Some(v) = &self.version {
            cmd.env("PUPPETEER_CODEX_VERSION", v);
        }
    }

    /// Run-log lines naming the binary and its version, plus a warning when
    /// the version is unknown or below the supported minimum.
    pub fn log_lines(&self) -> String {
        let path = self.path.display();
        let (major, minor, patch) = MIN_VERSION;
        match self.version.as_deref() {
            Some(v) => {
                let mut out = format!("[log:info] [codex] codex-cli {v} at {path}\n");
                if !meets_minimum(v) {
                    out.push_str(&format!(
                        "[log:warn] [codex] codex-cli {v} is older than {major}.{minor}.{patch}, the oldest version Puppeteer supports; some features may not work. Update Codex.\n"
                    ));
                }
                out
            }
            None => format!(
                "[log:warn] [codex] could not read the version of {path} (`--version` failed); Puppeteer supports codex-cli {major}.{minor}.{patch} or newer\n"
            ),
        }
    }
}

/// Resolve the binary from the `codex_path` setting and read its version.
pub async fn resolve_codex(setting: &str) -> Result<CodexBin, String> {
    let path = resolve_codex_bin(setting)?;
    let version = codex_version(&path).await;
    Ok(CodexBin { path, version })
}

/// The binary the `codex_path` setting points to: a path as given; a bare name
/// on the augmented PATH; for the default `codex`, then the usual install
/// locations (npm's global bin is node's dir, already first on that PATH).
pub fn resolve_codex_bin(setting: &str) -> Result<PathBuf, String> {
    resolve_in(setting, augmented_path().as_deref(), &install_locations())
}

fn install_locations() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/Applications/Codex.app/Contents/Resources/codex")];
    if let Some(home) = std::env::var_os("HOME") {
        out.push(Path::new(&home).join("Applications/Codex.app/Contents/Resources/codex"));
    }
    out.push(PathBuf::from("/opt/homebrew/bin/codex"));
    out.push(PathBuf::from("/usr/local/bin/codex"));
    out
}

fn resolve_in(setting: &str, path_var: Option<&OsStr>, fallbacks: &[PathBuf]) -> Result<PathBuf, String> {
    let name = match setting.trim() {
        "" => "codex",
        s => s,
    };
    let given = Path::new(name);
    if given.components().count() > 1 || given.is_absolute() {
        return if is_executable(given) {
            Ok(given.to_path_buf())
        } else {
            Err(format!("Codex binary not found at {name}. {NOT_FOUND_HINT}"))
        };
    }
    if let Some(found) = path_var.and_then(|p| find_on_path(name, p)) {
        return Ok(found);
    }
    if name == "codex" {
        if let Some(found) = fallbacks.iter().find(|p| is_executable(p)) {
            return Ok(found.clone());
        }
    }
    Err(format!("Codex CLI `{name}` not found on PATH or in the usual install locations. {NOT_FOUND_HINT}"))
}

fn find_on_path(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    // Windows: npm installs `codex.cmd`, a native install `codex.exe`.
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    std::env::split_paths(path_var).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|p| is_executable(p))
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// `<bin> --version`, read once per binary. Keyed by the file's mtime too, so
/// an update in place (Codex.app updates itself) is read again.
async fn codex_version(bin: &Path) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, Option<SystemTime>), Option<String>>>> = OnceLock::new();
    let key = (bin.to_path_buf(), std::fs::metadata(bin).and_then(|m| m.modified()).ok());
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let version = probe_version(bin).await;
    if let Ok(mut c) = cache.lock() {
        c.insert(key, version.clone());
    }
    version
}

async fn probe_version(bin: &Path) -> Option<String> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    // An npm install is a `#!/usr/bin/env node` script: it needs node on PATH.
    crate::runs::sidecar::augment_command_path(&mut cmd);
    let out = tokio::time::timeout(VERSION_TIMEOUT, cmd.output()).await.ok()?.ok()?;
    if !out.status.success() {
        tracing::warn!(event = "codex_version_failed", bin = %bin.display(), status = %out.status);
        return None;
    }
    parse_codex_version(&String::from_utf8_lossy(&out.stdout))
}

/// `codex-cli 0.133.0-alpha.1` → `0.133.0-alpha.1`.
fn parse_codex_version(out: &str) -> Option<String> {
    out.split_whitespace()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()) && w.contains('.'))
        .map(str::to_string)
}

fn meets_minimum(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut nums = core.split('.').map(|n| n.parse::<u64>().ok());
    let mut next = || nums.next().flatten();
    match (next(), next()) {
        (Some(major), Some(minor)) => (major, minor, next().unwrap_or(0)) >= MIN_VERSION,
        // Unparseable: don't warn about a version we can't read.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer_codex_bin_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_bin(dir: &Path, name: &str, script: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    #[test]
    fn bare_codex_prefers_path_then_install_locations() {
        let root = scratch("resolve");
        let on_path = root.join("bin");
        let app = root.join("Codex.app");
        std::fs::create_dir_all(&on_path).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let installed = fake_bin(&app, "codex", "#!/bin/sh\n");
        let fallbacks = [root.join("missing/codex"), installed.clone()];

        // Not on PATH: the first install location that exists.
        let empty = std::env::join_paths([root.join("nothing")]).unwrap();
        assert_eq!(resolve_in("codex", Some(&empty), &fallbacks).unwrap(), installed);
        assert_eq!(resolve_in("  ", None, &fallbacks).unwrap(), installed);

        // On PATH: PATH wins.
        let listed = fake_bin(&on_path, "codex", "#!/bin/sh\n");
        let path = std::env::join_paths([on_path.clone()]).unwrap();
        assert_eq!(resolve_in("codex", Some(&path), &fallbacks).unwrap(), listed);

        // A path is used as given, never replaced by another install.
        assert_eq!(resolve_in(installed.to_str().unwrap(), Some(&path), &[]).unwrap(), installed);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_binary_is_a_clear_error() {
        let root = scratch("missing");
        let path = std::env::join_paths([root.clone()]).unwrap();
        let err = resolve_in("codex", Some(&path), &[root.join("Codex.app/codex")]).unwrap_err();
        assert!(err.contains("Settings → Engines"), "{err}");

        let gone = root.join("gone/codex");
        let err = resolve_in(gone.to_str().unwrap(), Some(&path), &[]).unwrap_err();
        assert!(err.contains(gone.to_str().unwrap()) && err.contains("Settings → Engines"), "{err}");

        // A non-executable file is not a binary; another bare name never uses
        // the Codex install locations.
        std::fs::write(root.join("codex"), "").unwrap();
        #[cfg(unix)]
        assert!(resolve_in("codex", Some(&path), &[]).is_err());
        let other = fake_bin(&root, "codex2", "#!/bin/sh\n");
        assert!(resolve_in("codex-beta", None, &[other]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_the_cli_version() {
        assert_eq!(parse_codex_version("codex-cli 0.133.0-alpha.1\n").as_deref(), Some("0.133.0-alpha.1"));
        assert_eq!(parse_codex_version("codex-cli 0.165.2").as_deref(), Some("0.165.2"));
        assert_eq!(parse_codex_version("codex-cli\n"), None);
        assert_eq!(parse_codex_version(""), None);
    }

    #[test]
    fn minimum_version_ignores_prerelease() {
        assert!(meets_minimum("0.133.0-alpha.1"));
        assert!(meets_minimum("0.133.0"));
        assert!(meets_minimum("0.165.2"));
        assert!(meets_minimum("1.0.0"));
        assert!(!meets_minimum("0.132.9"));
        assert!(!meets_minimum("0.99.0"));
        assert!(meets_minimum("garbage"));
    }

    #[test]
    fn log_lines_name_the_binary_and_warn_below_minimum() {
        let bin = |v: Option<&str>| CodexBin { path: PathBuf::from("/x/codex"), version: v.map(str::to_string) };
        assert_eq!(bin(Some("0.133.0-alpha.1")).log_lines(), "[log:info] [codex] codex-cli 0.133.0-alpha.1 at /x/codex\n");
        let old = bin(Some("0.120.0")).log_lines();
        assert!(old.starts_with("[log:info] [codex] codex-cli 0.120.0 at /x/codex\n[log:warn] "), "{old}");
        assert!(bin(None).log_lines().starts_with("[log:warn] [codex] could not read the version of /x/codex"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_is_read_from_the_binary() {
        let root = scratch("version");
        let bin = fake_bin(&root, "codex", "#!/bin/sh\necho 'codex-cli 0.133.0-alpha.1'\n");
        assert_eq!(codex_version(&bin).await.as_deref(), Some("0.133.0-alpha.1"));
        let failing = fake_bin(&root, "broken", "#!/bin/sh\nexit 3\n");
        assert_eq!(codex_version(&failing).await, None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
