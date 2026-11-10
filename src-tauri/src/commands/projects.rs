use crate::commands::rules::{ApplyAllOutcome, ApplyFailure};
use crate::commands::skills::{
    claude_skill_dir, legacy_codex_skill_dir, remove_skill_artifacts, write_skill_artifacts,
    SkillEngines,
};
use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    pub created_at: String,
    pub github_account_id: Option<String>,
    /// GitHub accounts enabled for the project (`github_account_id` is the
    /// default among them). Runs may pick any of these per command.
    #[serde(default)]
    pub github_account_ids: Vec<String>,
    /// GitLab accounts enabled for the project (`gitlab_account_id` = default).
    #[serde(default)]
    pub gitlab_account_ids: Vec<String>,
    /// GĐ6: linked GitLab account (mirrors `github_account_id`). `None` when the
    /// project has no GitLab account attached (backward-compatible default).
    #[serde(default)]
    pub gitlab_account_id: Option<String>,
    /// Linked AWS account, mirroring the Git account model. `None` when the
    /// project has no AWS account attached.
    #[serde(default)]
    pub aws_account_id: Option<String>,
    /// Linked Azure account (isolated AZURE_CONFIG_DIR managed by Puppeteer).
    /// `None` when the project has no Azure account attached.
    #[serde(default)]
    pub azure_account_id: Option<String>,
    /// AWS / Azure accounts enabled for the project (`aws_account_id` /
    /// `azure_account_id` = default). Runs may pick any of these per command.
    #[serde(default)]
    pub aws_account_ids: Vec<String>,
    #[serde(default)]
    pub azure_account_ids: Vec<String>,
    /// Linked Claude account (isolated by CLAUDE_CONFIG_DIR). `None` = use the
    /// default Claude account, or the global profile when none exists.
    #[serde(default)]
    pub claude_account_id: Option<String>,
    /// GitHub Project V2 board URL for milestone/issue tracking (`None` = not linked).
    #[serde(default)]
    pub github_project_board_url: Option<String>,
    /// JSON-encoded field mappings for the linked board (start/deadline/status field ids).
    #[serde(default)]
    pub github_project_field_mappings: Option<String>,
    /// Number of runs recorded against this project (usage frequency).
    #[serde(default)]
    pub run_count: i64,
    /// Timestamp of the most recent run, if any (recency).
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// Manual display order (ascending). Drag-and-drop reorder rewrites this.
    #[serde(default)]
    pub position: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppliedSkill {
    pub skill_id: String,
    pub skill_name: String,
    pub skill_description: String,
    /// Engines the skill is switched on for in this project.
    pub enabled_claude: bool,
    pub enabled_codex: bool,
    /// Installed (linked by Puppeteer) for that engine; false for a path git tracks.
    pub has_claude: bool,
    pub has_codex: bool,
    pub applied_at: String,
    /// The user switched this skill on for the project directly.
    pub manual: bool,
    /// Some group the project has enabled contains this skill. Independent of `manual`: either
    /// one on its own keeps the skill applied.
    pub from_group: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddRepoPayload {
    pub name: String,
    pub path: String,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    /// Git host provider: `github` (default) or `gitlab`.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub gitlab_project_path: Option<String>,
    #[serde(default)]
    pub gitlab_project_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AddProjectPayload {
    pub path: String,
    pub name: Option<String>,
    pub repos: Option<Vec<AddRepoPayload>>,
}

/// Coordinates read off a repo's `origin` remote, mirroring the shapes the
/// frontend URL parser accepts (see `src/lib/repoUrl.ts`).
struct DetectedRemote {
    provider: String,
    github_owner: Option<String>,
    github_repo: Option<String>,
    gitlab_project_path: Option<String>,
}

/// Split a remote URL into host + path, accepting https/ssh/git schemes as well
/// as scp-style `git@host:group/repo`.
fn split_remote_host_path(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = trimmed.strip_suffix(".git").unwrap_or(trimmed);

    // scp-style has no scheme but carries a colon before the path.
    if !url.contains("://") {
        if let Some((userhost, path)) = url.split_once(':') {
            let host = userhost.rsplit('@').next().unwrap_or(userhost);
            return Some((host.to_lowercase(), path.to_string()));
        }
    }

    let no_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let (host, path) = no_scheme.split_once('/')?;
    // Strip userinfo (git@host) without eating an `@` that lives in the path.
    let host = host.rsplit('@').next().unwrap_or(host);
    Some((host.to_lowercase(), path.to_string()))
}

fn parse_remote_url(raw: &str) -> Option<DetectedRemote> {
    let (host, path) = split_remote_host_path(raw)?;
    let path = path.split(['?', '#']).next().unwrap_or(&path).to_string();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let name = segments[segments.len() - 1].to_string();

    if host.contains("gitlab") {
        return Some(DetectedRemote {
            provider: "gitlab".to_string(),
            github_owner: None,
            github_repo: None,
            gitlab_project_path: Some(segments.join("/")),
        });
    }
    if host.contains("github") {
        return Some(DetectedRemote {
            provider: "github".to_string(),
            github_owner: Some(segments[0].to_string()),
            github_repo: Some(name),
            gitlab_project_path: None,
        });
    }
    None
}

fn detect_remote(project_path: &str) -> Option<DetectedRemote> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_remote_url(&url)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Repo {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    pub created_at: String,
    /// Git host provider: `github` (default) or `gitlab` (BR-001).
    #[serde(default = "default_provider")]
    pub provider: String,
    /// GitLab `namespace/project` path (display + fallback API id). None for GitHub.
    #[serde(default)]
    pub gitlab_project_path: Option<String>,
    /// GitLab numeric project id (preferred API id). None for GitHub.
    #[serde(default)]
    pub gitlab_project_id: Option<i64>,
}

fn default_provider() -> String {
    "github".to_string()
}

#[derive(Debug, Serialize, Clone)]
pub struct DetectedRepo {
    pub name: String,
    pub path: String,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    /// Provider read off the `origin` remote; `github` when there's nothing to go on.
    pub provider: String,
    pub gitlab_project_path: Option<String>,
}

impl DetectedRepo {
    fn from_remote(name: String, path: String, remote: Option<DetectedRemote>) -> Self {
        Self {
            name,
            path,
            github_owner: remote.as_ref().and_then(|r| r.github_owner.clone()),
            github_repo: remote.as_ref().and_then(|r| r.github_repo.clone()),
            provider: remote
                .as_ref()
                .map(|r| r.provider.clone())
                .unwrap_or_else(default_provider),
            gitlab_project_path: remote.as_ref().and_then(|r| r.gitlab_project_path.clone()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DetectedProjectInfo {
    pub name: String,
    pub path: String,
    pub repos: Vec<DetectedRepo>,
}

/// A remote desktop may only open what it may read ([`crate::remote::fs_policy`]).
#[tauri::command]
pub async fn open_in_vscode(
    webview: tauri::Webview,
    db: State<'_, Db>,
    path: String,
    file: Option<String>,
) -> Result<(), String> {
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    let path = crate::remote::fs_policy::read(policy.as_ref(), Path::new(&path))?.to_string_lossy().into_owned();
    let file = match file {
        Some(f) => Some(crate::remote::fs_policy::read(policy.as_ref(), Path::new(&f))?.to_string_lossy().into_owned()),
        None => None,
    };
    open_in_vscode_inner(path, file)
}

fn open_in_vscode_inner(path: String, file: Option<String>) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    // When a file is provided, open the project folder AND reveal that file
    // (e.g. the most recently fetched issue/PR markdown).
    let file = file.filter(|f| Path::new(f).exists());

    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        cmd.args(["-a", "Visual Studio Code", &path]);
        if let Some(ref f) = file {
            cmd.arg(f);
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to open VS Code: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        let (program, args) = windows_vscode_command(&path, file.as_deref());
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map_err(|e| format!("Failed to open VS Code: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("code");
        cmd.arg(&path);
        if let Some(ref f) = file {
            cmd.arg(f);
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to open VS Code: {}", e))?;
    }

    Ok(())
}

/// VS Code's launcher on Windows, run as itself rather than as `cmd /C code …`: std quotes the
/// arguments of a `.cmd` for cmd (CVE-2024-24576) and refuses what it cannot quote, whereas
/// through `cmd /C` a `&`, `^` or `%VAR%` in a file name would run as a command.
#[cfg(any(target_os = "windows", test))]
fn windows_vscode_command(path: &str, file: Option<&str>) -> (&'static str, Vec<String>) {
    let mut args = vec![path.to_string()];
    args.extend(file.map(str::to_string));
    ("code.cmd", args)
}

/// A remote desktop may only open what it may read ([`crate::remote::fs_policy`]).
#[tauri::command]
pub async fn open_in_chrome(
    webview: tauri::Webview,
    db: State<'_, Db>,
    path: String,
) -> Result<(), String> {
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    let path = crate::remote::fs_policy::read(policy.as_ref(), Path::new(&path))?.to_string_lossy().into_owned();
    open_in_chrome_inner(path)
}

fn open_in_chrome_inner(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-a", "Google Chrome", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Chrome: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        // ShellExecute finds Chrome as `start chrome` did (App Paths), with no cmd to read the path.
        let (app, params) = windows_chrome_launch(&path);
        tauri_plugin_opener::open_path(params, Some(app))
            .map_err(|e| format!("Failed to open Chrome: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("google-chrome")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open Chrome: {}", e))?;
    }

    Ok(())
}

/// The app and parameters `ShellExecute` gets to open `path` in Chrome on Windows. Chrome splits
/// the parameters like any command line: a Windows path has no `"`, so quoting it whole keeps it
/// one argument, with the backslashes before the closing quote doubled (`C:\"` escapes it).
#[cfg(any(target_os = "windows", test))]
fn windows_chrome_launch(path: &str) -> (&'static str, String) {
    let trailing = path.len() - path.trim_end_matches('\\').len();
    ("chrome", format!("\"{path}{}\"", "\\".repeat(trailing)))
}

/// A remote desktop may only open what it may read ([`crate::remote::fs_policy`]).
#[tauri::command]
pub async fn open_in_folder(
    webview: tauri::Webview,
    db: State<'_, Db>,
    path: String,
) -> Result<(), String> {
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    let path = crate::remote::fs_policy::read(policy.as_ref(), Path::new(&path))?.to_string_lossy().into_owned();
    open_in_folder_inner(path)
}

fn open_in_folder_inner(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

/// A remote desktop may only open what it may read ([`crate::remote::fs_policy`]).
#[tauri::command]
pub async fn open_in_terminal(
    webview: tauri::Webview,
    db: State<'_, Db>,
    path: String,
    terminal_app: String,
) -> Result<(), String> {
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    let path = crate::remote::fs_policy::read(policy.as_ref(), Path::new(&path))?.to_string_lossy().into_owned();
    open_in_terminal_inner(path, terminal_app)
}

fn open_in_terminal_inner(path: String, terminal_app: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        let use_iterm = terminal_app == "iterm";

        // The canonical (symlink-resolved) path is what `lsof` reports as a
        // process cwd, so match against that.
        let target = std::fs::canonicalize(&path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.clone());

        // If a tab is already open on this project folder, just bring it to the
        // front instead of spawning yet another tab/window.
        let focused = if use_iterm {
            focus_existing_iterm_tab(&target)
        } else {
            focus_existing_terminal_tab(&target)
        };
        if focused {
            return Ok(());
        }

        // `open -a <App> <path>` launches the app with a new window/tab cd'd into `path`.
        let app = if use_iterm { "iTerm" } else { "Terminal" };
        std::process::Command::new("open")
            .args(["-a", app, &path])
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", app, e))?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let app = resolve_terminal_app(std::env::consts::OS, &terminal_app);
        spawn_first_terminal(app, terminal_launches(std::env::consts::OS, app, &path)?)?;
    }

    Ok(())
}

/// Terminal apps Settings offers on `os` (`std::env::consts::OS`), the default first. Mirrored by
/// `TERMINAL_APPS` in `src/lib/terminalApps.ts`.
fn terminal_apps(os: &str) -> &'static [&'static str] {
    match os {
        "macos" => &["terminal", "iterm"],
        // PowerShell ships with every Windows; Windows Terminal does not with Windows 10.
        "windows" => &["powershell", "wt", "cmd"],
        _ => &["auto", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"],
    }
}

/// The saved `terminal_app` when `os` offers it, else that OS's default: a value saved on another
/// OS (a settings backup from a Mac restored on Windows) falls back instead of failing.
pub fn resolve_terminal_app(os: &str, saved: &str) -> &'static str {
    let apps = terminal_apps(os);
    apps.iter().copied().find(|a| *a == saved).unwrap_or(apps[0])
}

/// One way to open a terminal in `cwd`: an argv, never a shell command line, so no character of
/// the folder's name is interpreted.
#[cfg(any(not(target_os = "macos"), test))]
#[derive(Debug, PartialEq)]
struct TerminalLaunch {
    program: &'static str,
    args: Vec<String>,
    /// Also the process's working directory: the terminals without a folder flag start there.
    cwd: String,
    /// A console program (PowerShell, cmd) needs a console window of its own: the app has none.
    new_console: bool,
}

/// What to run to open `dir` in `app` on Windows / Linux, tried in order until one is installed.
/// Folder flags: `wt -d` (learn.microsoft.com/windows/terminal/command-line-arguments),
/// `gnome-terminal --working-directory=` (gnome-terminal(1)), `konsole --workdir`
/// (docs.kde.org, Konsole "Command-line Options"), `xfce4-terminal --working-directory=`
/// (docs.xfce.org, xfce4-terminal "Command line"). PowerShell, cmd, xterm and
/// `x-terminal-emulator` (Debian policy only requires `-e` / `-T`) have none: `cwd` does it.
#[cfg(any(not(target_os = "macos"), test))]
fn terminal_launches(os: &str, app: &str, dir: &str) -> Result<Vec<TerminalLaunch>, String> {
    // A remote caller's path is canonical, `\\?\C:\…` on Windows, which cmd does not take as its
    // working directory.
    let dir = match dir.strip_prefix(r"\\?\") {
        Some(rest) if os == "windows" && rest.as_bytes().get(1) == Some(&b':') => rest,
        _ => dir,
    };
    let launch = |program: &'static str, args: Vec<String>, new_console: bool| TerminalLaunch {
        program,
        args,
        cwd: dir.to_string(),
        new_console,
    };
    if os == "windows" {
        return Ok(vec![match app {
            "wt" => {
                // `wt` splits its arguments into commands at every `;`, and `\;` keeps the backslash.
                if dir.contains(';') {
                    return Err("Windows Terminal cannot open a folder whose path contains ';'. Choose PowerShell or Command Prompt in Settings.".to_string());
                }
                launch("wt.exe", vec!["-d".to_string(), dir.to_string()], false)
            }
            "cmd" => launch("cmd.exe", vec![], true),
            _ => launch("powershell.exe", vec![], true),
        }]);
    }
    let gnome = || launch("gnome-terminal", vec![format!("--working-directory={dir}")], false);
    let konsole = || launch("konsole", vec!["--workdir".to_string(), dir.to_string()], false);
    let xfce = || launch("xfce4-terminal", vec![format!("--working-directory={dir}")], false);
    let xterm = || launch("xterm", vec![], false);
    Ok(match app {
        "gnome-terminal" => vec![gnome()],
        "konsole" => vec![konsole()],
        "xfce4-terminal" => vec![xfce()],
        "xterm" => vec![xterm()],
        // "auto": the distribution's chosen terminal, then the common ones.
        _ => vec![launch("x-terminal-emulator", vec![], false), gnome(), konsole(), xfce(), xterm()],
    })
}

/// Start the first of `launches` that is installed.
#[cfg(not(target_os = "macos"))]
fn spawn_first_terminal(app: &str, launches: Vec<TerminalLaunch>) -> Result<(), String> {
    for l in launches {
        let mut cmd = std::process::Command::new(l.program);
        cmd.args(&l.args).current_dir(&l.cwd);
        #[cfg(windows)]
        if l.new_console {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            cmd.creation_flags(CREATE_NEW_CONSOLE);
        }
        #[cfg(not(windows))]
        let _ = l.new_console;
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Failed to open {}: {}", l.program, e)),
        }
    }
    Err(format!("No terminal found for \"{app}\". Choose another one in Settings."))
}

/// Run an AppleScript snippet via `osascript`, returning stdout on success.
#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("osascript")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open osascript stdin".to_string())?
        .write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Working directories of the processes attached to a tty (e.g. `/dev/ttys001`).
#[cfg(target_os = "macos")]
fn cwds_for_tty(tty: &str) -> Vec<String> {
    let short = tty.trim_start_matches("/dev/");
    let mut cwds = Vec::new();
    let pids = match std::process::Command::new("ps")
        .args(["-t", short, "-o", "pid="])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return cwds,
    };
    for pid in pids.split_whitespace() {
        if let Ok(o) = std::process::Command::new("lsof")
            .args(["-a", "-d", "cwd", "-p", pid, "-Fn"])
            .output()
        {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some(p) = line.strip_prefix('n') {
                    cwds.push(p.to_string());
                }
            }
        }
    }
    cwds
}

#[cfg(target_os = "macos")]
fn paths_match(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// Focus an existing Terminal.app tab whose cwd is `target`. Returns true if found.
#[cfg(target_os = "macos")]
fn focus_existing_terminal_tab(target: &str) -> bool {
    // `tab`/`linefeed` are shadowed by Terminal's own `tab` class inside a
    // `tell` block, so build the field/record separators outside it.
    let list_script = r#"
tell application "System Events"
    if not (exists process "Terminal") then return ""
end tell
set d to "|"
set nl to linefeed
set output to ""
tell application "Terminal"
    set winList to windows
    repeat with wi from 1 to count of winList
        set w to item wi of winList
        set tabList to tabs of w
        repeat with ti from 1 to count of tabList
            set t to item ti of tabList
            set output to output & ((id of w) as string) & d & ti & d & (tty of t) & nl
        end repeat
    end repeat
end tell
return output
"#;
    let listing = match run_osascript(list_script) {
        Ok(s) => s,
        Err(_) => return false,
    };
    for line in listing.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 3 {
            continue;
        }
        let (win_id, tab_idx, tty) = (parts[0], parts[1], parts[2]);
        if cwds_for_tty(tty).iter().any(|c| paths_match(c, target)) {
            let activate = format!(
                r#"
tell application "Terminal"
    set w to (first window whose id is {win})
    set index of w to 1
    set selected tab of w to (item {tab} of tabs of w)
    activate
end tell
"#,
                win = win_id,
                tab = tab_idx
            );
            let _ = run_osascript(&activate);
            return true;
        }
    }
    false
}

/// Focus an existing iTerm session whose cwd is `target`. Returns true if found.
#[cfg(target_os = "macos")]
fn focus_existing_iterm_tab(target: &str) -> bool {
    let list_script = r#"
tell application "System Events"
    if not (exists process "iTerm2") then return ""
end tell
set d to "|"
set nl to linefeed
set output to ""
tell application "iTerm"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                set output to output & ((id of s) as string) & d & (tty of s) & nl
            end repeat
        end repeat
    end repeat
end tell
return output
"#;
    let listing = match run_osascript(list_script) {
        Ok(s) => s,
        Err(_) => return false,
    };
    for line in listing.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 2 {
            continue;
        }
        let (session_id, tty) = (parts[0], parts[1]);
        if cwds_for_tty(tty).iter().any(|c| paths_match(c, target)) {
            let activate = format!(
                r#"
tell application "iTerm"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if (id of s) is "{sid}" then
                    select s
                end if
            end repeat
        end repeat
    end repeat
    activate
end tell
"#,
                sid = session_id
            );
            let _ = run_osascript(&activate);
            return true;
        }
    }
    false
}

#[tauri::command]
pub async fn detect_project_info(webview: tauri::Webview, path: String) -> Result<DetectedProjectInfo, String> {
    // Picking a NEW folder to add: outside every project by definition, so a
    // remote desktop is only kept out of the credential stores here.
    if crate::remote::guard::from_remote_bridge(&webview) {
        use tauri::Manager;
        crate::remote::guard::SecretPaths::for_app(webview.app_handle()).check(&path)?;
    }
    if !Path::new(&path).exists() {
        return Err("Path does not exist".to_string());
    }
    let project_name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Project".to_string());

    let mut repos: Vec<DetectedRepo> = Vec::new();

    // Check root for .git
    if Path::new(&path).join(".git").exists() {
        repos.push(DetectedRepo::from_remote(
            project_name.clone(),
            path.clone(),
            detect_remote(&path),
        ));
    }

    // Scan immediate subdirectories for .git
    if let Ok(entries) = std::fs::read_dir(&path) {
        let mut subdirs: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        for subdir in subdirs {
            if subdir.join(".git").exists() {
                let subdir_str = subdir.to_string_lossy().to_string();
                let subdir_name = subdir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| subdir_str.clone());
                let remote = detect_remote(&subdir_str);
                repos.push(DetectedRepo::from_remote(subdir_name, subdir_str, remote));
            }
        }
    }

    Ok(DetectedProjectInfo {
        name: project_name,
        path,
        repos,
    })
}

/// `GROUP_CONCAT` of account ids → ids (account ids are UUIDs, no commas).
fn split_ids(csv: Option<String>) -> Vec<String> {
    csv.map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn list_projects(db: State<'_, Db>) -> Result<Vec<Project>, String> {
    list_projects_inner(db.inner()).await
}

/// Live projects only; `run_count` / `last_used_at` count only runs outside the trash.
pub(crate) async fn list_projects_inner(db: &Db) -> Result<Vec<Project>, String> {
    use sqlx::Row;
    // Manual order first (drag-and-drop), then usage frequency / recency / name
    // as a stable tiebreaker for any projects sharing a position.
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.path, p.created_at, p.github_account_id, p.gitlab_account_id, \
                (SELECT GROUP_CONCAT(account_id) FROM project_github_accounts WHERE project_id = p.id) AS github_account_ids, \
                (SELECT GROUP_CONCAT(account_id) FROM project_gitlab_accounts WHERE project_id = p.id) AS gitlab_account_ids, p.aws_account_id, p.azure_account_id, p.claude_account_id, \
                (SELECT GROUP_CONCAT(account_id) FROM project_aws_accounts WHERE project_id = p.id) AS aws_account_ids, \
                (SELECT GROUP_CONCAT(account_id) FROM project_azure_accounts WHERE project_id = p.id) AS azure_account_ids, \
                p.github_project_board_url, p.github_project_field_mappings, p.position, \
                COUNT(r.id) AS run_count, MAX(r.created_at) AS last_used_at \
         FROM projects p \
         LEFT JOIN runs r ON r.project_id = p.id AND r.deleted_at IS NULL \
         WHERE p.deleted_at IS NULL \
         GROUP BY p.id \
         ORDER BY p.position ASC, run_count DESC, last_used_at DESC, p.name COLLATE NOCASE ASC"
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let projects = rows
        .iter()
        .map(|row| Project {
            id: row.get("id"),
            name: row.get("name"),
            path: row.get("path"),
            github_owner: None,
            github_repo: None,
            created_at: row.get("created_at"),
            github_account_id: row.get("github_account_id"),
            github_account_ids: split_ids(row.get("github_account_ids")),
            gitlab_account_ids: split_ids(row.get("gitlab_account_ids")),
            gitlab_account_id: row.get("gitlab_account_id"),
            aws_account_id: row.get("aws_account_id"),
            azure_account_id: row.get("azure_account_id"),
            aws_account_ids: split_ids(row.get("aws_account_ids")),
            azure_account_ids: split_ids(row.get("azure_account_ids")),
            claude_account_id: row.get("claude_account_id"),
            github_project_board_url: row.get("github_project_board_url"),
            github_project_field_mappings: row.get("github_project_field_mappings"),
            run_count: row.get("run_count"),
            last_used_at: row.get("last_used_at"),
            position: row.get("position"),
        })
        .collect();

    Ok(projects)
}

#[tauri::command]
pub async fn add_project(webview: tauri::Webview, db: State<'_, Db>, payload: AddProjectPayload) -> Result<Project, String> {
    // A registered project becomes a writable root of the remote desktop: it
    // may only register folders it already reaches (HOME, volumes, projects).
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    if let Some(policy) = &policy {
        for path in std::iter::once(&payload.path).chain(payload.repos.iter().flatten().map(|r| &r.path)) {
            policy.read(Path::new(path))?;
        }
    }
    if !Path::new(&payload.path).exists() {
        return Err("Project path does not exist".to_string());
    }
    // Skills etc. would link into HOME's own `.claude/skills`, bypassing `library.skills_global`.
    if crate::commands::project_links::is_home(Path::new(&payload.path)) {
        return Err("project_is_home".to_string());
    }
    // A path whose project is in the Trash brings that project back (same id,
    // sessions, links and settings; the payload's name and repos are not
    // applied) instead of failing on UNIQUE(path).
    if let Some(id) = restore_trashed_path(db.inner(), &payload.path).await? {
        crate::commands::runs::notify_trash_changed(webview.app_handle());
        return project_by_id(db.inner(), &id).await;
    }

    let name = payload.name.unwrap_or_else(|| {
        Path::new(&payload.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Project".to_string())
    });

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Append new projects to the end of the manual order.
    use sqlx::Row;
    let position: i64 = sqlx::query("SELECT COALESCE(MAX(position), -1) + 1 AS next FROM projects")
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())?
        .get("next");

    sqlx::query("INSERT INTO projects (id, name, path, created_at, position) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&payload.path)
        .bind(&now)
        .bind(position)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    if let Some(repos) = payload.repos {
        for repo in repos {
            let repo_id = Uuid::new_v4().to_string();
            let provider = repo
                .provider
                .filter(|p| !p.is_empty())
                .unwrap_or_else(default_provider);
            sqlx::query(
                "INSERT INTO repos (id, project_id, name, path, github_owner, github_repo, created_at, provider, gitlab_project_path, gitlab_project_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&repo_id)
            .bind(&id)
            .bind(&repo.name)
            .bind(&repo.path)
            .bind(&repo.github_owner)
            .bind(&repo.github_repo)
            .bind(&now)
            .bind(&provider)
            .bind(&repo.gitlab_project_path)
            .bind(repo.gitlab_project_id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(Project {
        id,
        name,
        path: payload.path,
        github_owner: None,
        github_repo: None,
        created_at: now,
        github_account_id: None,
        github_account_ids: Vec::new(),
        gitlab_account_ids: Vec::new(),
        gitlab_account_id: None,
        aws_account_id: None,
        azure_account_id: None,
        aws_account_ids: Vec::new(),
        azure_account_ids: Vec::new(),
        claude_account_id: None,
        github_project_board_url: None,
        github_project_field_mappings: None,
        run_count: 0,
        last_used_at: None,
        position,
    })
}

/// Move a project to the Trash ([`trash_project_inner`]).
#[tauri::command]
pub async fn trash_project(
    app: AppHandle,
    db: State<'_, Db>,
    registry: State<'_, crate::runs::RunRegistry>,
    id: String,
) -> Result<(), String> {
    let run_ids = trash_project_inner(db.inner(), registry.inner(), &id).await?;
    if let Some(conductor) = app.try_state::<crate::conductor::ConductorState>() {
        for run_id in &run_ids {
            conductor.unregister_session(run_id);
        }
    }
    crate::commands::runs::notify_trash_changed(&app);
    Ok(())
}

/// Stamp a project's `deleted_at`. Its runs are NOT stamped, so a restore
/// brings back exactly the sessions that were not trashed on their own. Only
/// the row changes: links, AWS config and the folder stay until a purge.
/// Refused while one of its runs is active. Returns the project's run ids, to
/// drop from the in-memory conductor state.
pub(crate) async fn trash_project_inner(
    db: &Db,
    registry: &crate::runs::RunRegistry,
    id: &str,
) -> Result<Vec<String>, String> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    if found.is_none() {
        return Err("project not found".to_string());
    }
    let runs: Vec<(String, String)> = sqlx::query_as("SELECT id, status FROM runs WHERE project_id = ?")
        .bind(id)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    let active = {
        let reg = registry.lock().await;
        runs.iter().any(|(run_id, status)| status == "running" || reg.contains_key(run_id))
    };
    if active {
        return Err("A session of this project is still running — stop it before deleting".to_string());
    }
    sqlx::query("UPDATE projects SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(runs.into_iter().map(|(run_id, _)| run_id).collect())
}

/// Bring a project back from the Trash, with its sessions that were not
/// trashed on their own.
#[tauri::command]
pub async fn restore_project(app: AppHandle, db: State<'_, Db>, id: String) -> Result<(), String> {
    restore_project_inner(db.inner(), &id).await?;
    crate::commands::runs::notify_trash_changed(&app);
    Ok(())
}

/// Bring back the project in the Trash that owns `path`, if any. Returns its id.
pub(crate) async fn restore_trashed_path(db: &Db, path: &str) -> Result<Option<String>, String> {
    let trashed: Option<String> =
        sqlx::query_scalar("SELECT id FROM projects WHERE path = ? AND deleted_at IS NOT NULL")
            .bind(path)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(id) = &trashed {
        restore_project_inner(db, id).await?;
    }
    Ok(trashed)
}

pub(crate) async fn restore_project_inner(db: &Db, id: &str) -> Result<(), String> {
    let found = sqlx::query("UPDATE projects SET deleted_at = NULL WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    if found == 0 {
        return Err("project not found".to_string());
    }
    Ok(())
}

/// Delete a project in the Trash for good: its row, runs (cascade), session
/// links and AWS config. A project outside the Trash is refused. Local only:
/// a remote desktop may not purge.
#[tauri::command]
pub async fn purge_project(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
) -> Result<(), String> {
    let in_trash: Option<bool> = sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM projects WHERE id = ?")
        .bind(&id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    match in_trash {
        None => return Err("project not found".to_string()),
        Some(false) => return Err("Project is not in the Trash — move it there first".to_string()),
        Some(true) => {}
    }
    // Session links of Slack threads / notes / todos: the project's runs cascade
    // away below and the link tables have no FK to runs.
    let unlinked = crate::slack_threads::run_links::delete_for_project(db.inner(), &id)
        .await
        .map_err(|e| e.to_string())?;
    let items_unlinked = crate::item_links::delete_for_project(db.inner(), &id)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    if unlinked > 0 {
        crate::slack_threads::emit_all_changed(&app);
    }
    if items_unlinked > 0 {
        crate::item_links::emit_changed(&app, None);
    }
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        crate::aws_sandbox::remove_project_aws_config(&app_data_dir, &id);
    }
    crate::commands::trash::emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn update_project(
    db: State<'_, Db>,
    id: String,
    name: String,
    github_project_board_url: Option<String>,
    github_project_field_mappings: Option<String>,
) -> Result<Project, String> {
    sqlx::query(
        "UPDATE projects SET name = ?, github_project_board_url = ?, github_project_field_mappings = ? WHERE id = ?"
    )
        .bind(&name)
        .bind(&github_project_board_url)
        .bind(&github_project_field_mappings)
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    project_by_id(db.inner(), &id).await
}

/// One project by id, trashed or not, with the same counts as the list.
pub(crate) async fn project_by_id(db: &Db, id: &str) -> Result<Project, String> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT p.id, p.name, p.path, p.created_at, p.github_account_id, p.gitlab_account_id, \
                (SELECT GROUP_CONCAT(account_id) FROM project_github_accounts WHERE project_id = p.id) AS github_account_ids, \
                (SELECT GROUP_CONCAT(account_id) FROM project_gitlab_accounts WHERE project_id = p.id) AS gitlab_account_ids, p.aws_account_id, p.azure_account_id, p.claude_account_id, \
                (SELECT GROUP_CONCAT(account_id) FROM project_aws_accounts WHERE project_id = p.id) AS aws_account_ids, \
                (SELECT GROUP_CONCAT(account_id) FROM project_azure_accounts WHERE project_id = p.id) AS azure_account_ids, \
                p.github_project_board_url, p.github_project_field_mappings, p.position, \
                COUNT(r.id) AS run_count, MAX(r.created_at) AS last_used_at \
         FROM projects p LEFT JOIN runs r ON r.project_id = p.id AND r.deleted_at IS NULL WHERE p.id = ? GROUP BY p.id"
    )
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Project {
        id: row.get("id"),
        name: row.get("name"),
        path: row.get("path"),
        github_owner: None,
        github_repo: None,
        created_at: row.get("created_at"),
        github_account_id: row.get("github_account_id"),
        github_account_ids: split_ids(row.get("github_account_ids")),
        gitlab_account_ids: split_ids(row.get("gitlab_account_ids")),
        gitlab_account_id: row.get("gitlab_account_id"),
        aws_account_id: row.get("aws_account_id"),
        azure_account_id: row.get("azure_account_id"),
        aws_account_ids: split_ids(row.get("aws_account_ids")),
        azure_account_ids: split_ids(row.get("azure_account_ids")),
        claude_account_id: row.get("claude_account_id"),
        github_project_board_url: row.get("github_project_board_url"),
        github_project_field_mappings: row.get("github_project_field_mappings"),
        run_count: row.get("run_count"),
        last_used_at: row.get("last_used_at"),
        position: row.get("position"),
    })
}

/// Persist a new project order. The frontend sends the full list of ids
/// top-to-bottom; each row's position becomes its index, so ordering is stable
/// regardless of prior values. Done in one transaction to avoid partial writes.
#[tauri::command]
pub async fn reorder_projects(db: State<'_, Db>, ids: Vec<String>) -> Result<(), String> {
    let mut tx = db.inner().begin().await.map_err(|e| e.to_string())?;
    for (index, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE projects SET position = ? WHERE id = ?")
            .bind(index as i64)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Map a `repos` row into a `Repo`, tolerating older rows that predate the
/// provider/gitlab columns (NULL provider -> `github`, per BR-001).
fn row_to_repo(row: &sqlx::sqlite::SqliteRow) -> Repo {
    use sqlx::Row;
    Repo {
        id: row.get("id"),
        project_id: row.get("project_id"),
        name: row.get("name"),
        path: row.get("path"),
        github_owner: row.get("github_owner"),
        github_repo: row.get("github_repo"),
        created_at: row.get("created_at"),
        provider: row
            .get::<Option<String>, _>("provider")
            .filter(|p| !p.is_empty())
            .unwrap_or_else(default_provider),
        gitlab_project_path: row.get("gitlab_project_path"),
        gitlab_project_id: row.get("gitlab_project_id"),
    }
}

const REPO_COLUMNS: &str = "id, project_id, name, path, github_owner, github_repo, created_at, provider, gitlab_project_path, gitlab_project_id";

#[tauri::command]
pub async fn list_repos(db: State<'_, Db>, project_id: String) -> Result<Vec<Repo>, String> {
    let rows = sqlx::query(&format!(
        "SELECT {REPO_COLUMNS} FROM repos WHERE project_id = ? ORDER BY name"
    ))
    .bind(&project_id)
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_repo).collect())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn add_repo(
    webview: tauri::Webview,
    db: State<'_, Db>,
    project_id: String,
    name: String,
    path: String,
    github_owner: Option<String>,
    github_repo: Option<String>,
    provider: Option<String>,
    gitlab_project_path: Option<String>,
    gitlab_project_id: Option<i64>,
) -> Result<Repo, String> {
    // As in `add_project`: a remote desktop registers only what it reaches.
    let policy = crate::remote::fs_policy::RemoteFs::for_caller(&webview, db.inner()).await?;
    crate::remote::fs_policy::read(policy.as_ref(), Path::new(&path))?;
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let provider = provider
        .filter(|p| !p.is_empty())
        .unwrap_or_else(default_provider);
    sqlx::query(
        "INSERT INTO repos (id, project_id, name, path, github_owner, github_repo, created_at, provider, gitlab_project_path, gitlab_project_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&name)
    .bind(&path)
    .bind(&github_owner)
    .bind(&github_repo)
    .bind(&now)
    .bind(&provider)
    .bind(&gitlab_project_path)
    .bind(gitlab_project_id)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(Repo {
        id,
        project_id,
        name,
        path,
        github_owner,
        github_repo,
        created_at: now,
        provider,
        gitlab_project_path,
        gitlab_project_id,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_repo(
    db: State<'_, Db>,
    id: String,
    name: String,
    github_owner: Option<String>,
    github_repo: Option<String>,
    provider: Option<String>,
    gitlab_project_path: Option<String>,
    gitlab_project_id: Option<i64>,
) -> Result<Repo, String> {
    // Preserve existing provider identity when the caller omits it — the current
    // GitHub-only repo editor doesn't send these fields, and we must not silently
    // downgrade a GitLab repo back to `github` (BR-001).
    let provider = provider.filter(|p| !p.is_empty());
    if provider.is_some() {
        sqlx::query(
            "UPDATE repos SET name = ?, github_owner = ?, github_repo = ?, provider = ?, gitlab_project_path = ?, gitlab_project_id = ? WHERE id = ?"
        )
        .bind(&name)
        .bind(&github_owner)
        .bind(&github_repo)
        .bind(&provider)
        .bind(&gitlab_project_path)
        .bind(gitlab_project_id)
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    } else {
        sqlx::query("UPDATE repos SET name = ?, github_owner = ?, github_repo = ? WHERE id = ?")
            .bind(&name)
            .bind(&github_owner)
            .bind(&github_repo)
            .bind(&id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
    }

    let row = sqlx::query(&format!("SELECT {REPO_COLUMNS} FROM repos WHERE id = ?"))
        .bind(&id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(row_to_repo(&row))
}

#[tauri::command]
pub async fn remove_repo(db: State<'_, Db>, id: String) -> Result<(), String> {
    sqlx::query("DELETE FROM repos WHERE id = ?")
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_applied_skills(
    db: State<'_, Db>,
    project_id: String,
) -> Result<Vec<AppliedSkill>, String> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT ps.skill_id, s.name as skill_name, s.description as skill_description,
                ps.enabled_claude, ps.enabled_codex, ps.synced_hash_claude, ps.synced_hash_codex,
                ps.applied_at, ps.manual,
                (SELECT COUNT(*) FROM skill_group_members m
                   JOIN project_skill_groups pg
                     ON pg.group_id = m.group_id AND pg.project_id = ps.project_id
                  WHERE m.skill_id = ps.skill_id) as group_count
         FROM project_skills ps
         JOIN skills s ON s.id = ps.skill_id
         WHERE ps.project_id = ?
         ORDER BY s.name",
    )
    .bind(&project_id)
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|row| {
            let claude: Option<String> = row.get("synced_hash_claude");
            let codex: Option<String> = row.get("synced_hash_codex");
            let manual: i64 = row.get("manual");
            let group_count: i64 = row.get("group_count");
            let engines = SkillEngines::from_row(row);
            AppliedSkill {
                skill_id: row.get("skill_id"),
                skill_name: row.get("skill_name"),
                skill_description: row.get("skill_description"),
                enabled_claude: engines.claude,
                enabled_codex: engines.codex,
                has_claude: claude.is_some(),
                has_codex: codex.is_some(),
                applied_at: row.get("applied_at"),
                manual: manual != 0,
                from_group: group_count > 0,
            }
        })
        .collect())
}

/// One engine switched on or off (a chip of the Skills allocation matrix).
#[derive(Debug, Deserialize)]
pub struct EngineSwitch {
    pub engine: String,
    pub on: bool,
}

#[tauri::command]
pub async fn apply_skill(
    db: State<'_, Db>,
    project_id: String,
    skill_id: String,
    set_engine: Option<EngineSwitch>,
) -> Result<(), String> {
    match set_engine {
        Some(s) => switch_skill_engine(db.inner(), &project_id, &skill_id, &s.engine, s.on).await,
        None => apply_skill_to_project(db.inner(), &project_id, &skill_id, true).await,
    }
}

/// Switch a skill on or off for one engine in one project, leaving its other engines as they are.
/// A project new to the skill starts from no engine, not the skill's target: the user asked for
/// exactly one. With the last engine off the skill leaves the project unless an enabled group
/// still provides it; then it stays, with no engine on.
pub async fn switch_skill_engine(
    db: &Db,
    project_id: &str,
    skill_id: &str,
    engine: &str,
    on: bool,
) -> Result<(), String> {
    let row = sqlx::query("SELECT enabled_claude, enabled_codex FROM project_skills WHERE project_id = ? AND skill_id = ?")
        .bind(project_id)
        .bind(skill_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let mut engines = row.as_ref().map(SkillEngines::from_row).unwrap_or(SkillEngines::NONE);
    engines.set(engine, on)?;
    if engines == SkillEngines::NONE {
        if row.is_none() {
            return Ok(());
        }
        let kind = crate::commands::groups::GroupKind::Skill;
        if !crate::commands::groups::claimed_by_enabled_group(db, kind, project_id, skill_id).await? {
            return remove_skill_from_project_inner(db, project_id, skill_id).await;
        }
    }
    apply_skill_with_engines(db, project_id, skill_id, row.is_none(), Some(engines)).await
}

#[tauri::command]
pub async fn apply_skill_to_all_projects(
    db: State<'_, Db>,
    skill_id: String,
) -> Result<ApplyAllOutcome, String> {
    use sqlx::Row;
    let rows = sqlx::query("SELECT id, name FROM projects WHERE deleted_at IS NULL ORDER BY name")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let mut applied = 0;
    let mut failures = Vec::new();
    for row in &rows {
        let project_id: String = row.get("id");
        let project_name: String = row.get("name");
        match apply_skill_to_project(db.inner(), &project_id, &skill_id, true).await {
            Ok(()) => applied += 1,
            Err(error) => failures.push(ApplyFailure {
                project_id,
                project_name,
                error,
            }),
        }
    }
    Ok(ApplyAllOutcome { applied, failures })
}

/// Re-write a skill (or every skill when `skill_id` is None) into each project that already has it
/// applied. Unlike auto-sync this overwrites local edits, so any open conflicts for the re-applied
/// pairs are marked resolved. `manual` / group flags are left untouched.
#[tauri::command]
pub async fn reapply_skills(
    db: State<'_, Db>,
    skill_id: Option<String>,
) -> Result<ApplyAllOutcome, String> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT ps.project_id, ps.skill_id, p.name as project_name, p.path, s.name as skill_name
         FROM project_skills ps
         JOIN projects p ON p.id = ps.project_id
         JOIN skills s ON s.id = ps.skill_id
         WHERE ?1 IS NULL OR ps.skill_id = ?1
         ORDER BY p.name, s.name",
    )
    .bind(&skill_id)
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let mut applied = 0;
    let mut failures = Vec::new();
    for row in &rows {
        let project_id: String = row.get("project_id");
        let sid: String = row.get("skill_id");
        let project_name: String = row.get("project_name");
        let project_path: String = row.get("path");
        let skill_name: String = row.get("skill_name");
        // Don't recreate a project folder that was moved or deleted on disk.
        let result = if !std::path::Path::new(&project_path).is_dir() {
            Err(format!("Project folder not found: {}", project_path))
        } else {
            apply_skill_to_project(db.inner(), &project_id, &sid, false).await
        };
        match result {
            Ok(()) => {
                applied += 1;
                let _ = sqlx::query(
                    "UPDATE sync_conflicts SET resolved = 1 WHERE project_id = ? AND skill_id = ? AND resolved = 0",
                )
                .bind(&project_id)
                .bind(&sid)
                .execute(db.inner())
                .await;
            }
            Err(error) => failures.push(ApplyFailure {
                project_id,
                project_name,
                error: format!("{}: {}", skill_name, error),
            }),
        }
    }
    Ok(ApplyAllOutcome { applied, failures })
}

/// Core apply logic shared by `apply_skill`, `apply_skill_to_all_projects` and group enablement.
///
/// `manual` marks who asked for it: `true` when the user flipped the skill on directly, `false`
/// when a skill group brought it in. An existing manual row never gets downgraded, so disabling a
/// group leaves hand-picked skills alone.
///
/// The engines stay what the project has; a project new to the skill starts from its target.
pub async fn apply_skill_to_project(
    db: &Db,
    project_id: &str,
    skill_id: &str,
    manual: bool,
) -> Result<(), String> {
    apply_skill_with_engines(db, project_id, skill_id, manual, None).await
}

/// `apply_skill_to_project` with the engines to switch the skill on for in this project;
/// `None` keeps the project's own (or the skill's target for a project new to it).
pub async fn apply_skill_with_engines(
    db: &Db,
    project_id: &str,
    skill_id: &str,
    manual: bool,
    engines: Option<SkillEngines>,
) -> Result<(), String> {
    use sqlx::Row;

    let skill_row =
        sqlx::query("SELECT name, target, source_path FROM skills WHERE id = ?")
            .bind(skill_id)
            .fetch_one(db)
            .await
            .map_err(|e| format!("Skill not found: {}", e))?;

    let skill_name: String = skill_row.get("name");
    let target: String = skill_row.get("target");
    let source_path: String = skill_row.get("source_path");

    let project_row = sqlx::query("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map_err(|e| format!("Project not found: {}", e))?;

    let project_path: String = project_row.get("path");

    let prev = sqlx::query(
        "SELECT enabled_claude, enabled_codex, synced_hash_claude, synced_hash_codex
         FROM project_skills WHERE project_id = ? AND skill_id = ?",
    )
    .bind(project_id)
    .bind(skill_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    let prev_claude: Option<String> = prev.as_ref().and_then(|r| r.get("synced_hash_claude"));
    let prev_codex: Option<String> = prev.as_ref().and_then(|r| r.get("synced_hash_codex"));
    let engines = engines.unwrap_or_else(|| match &prev {
        Some(r) => SkillEngines::from_row(r),
        None => SkillEngines::from_target(&target),
    });

    let (hash_claude, hash_codex) = write_skill_artifacts(
        &project_path,
        &skill_name,
        engines,
        &source_path,
        prev_claude.as_deref(),
        prev_codex.as_deref(),
    )?;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO project_skills
         (project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude, synced_hash_codex,
          applied_at, manual)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(project_id, skill_id) DO UPDATE SET
            enabled_claude = excluded.enabled_claude,
            enabled_codex = excluded.enabled_codex,
            synced_hash_claude = excluded.synced_hash_claude,
            synced_hash_codex = excluded.synced_hash_codex,
            applied_at = excluded.applied_at,
            manual = CASE WHEN excluded.manual = 1 THEN 1 ELSE project_skills.manual END",
    )
    .bind(project_id)
    .bind(skill_id)
    .bind(engines.claude)
    .bind(engines.codex)
    .bind(&hash_claude)
    .bind(&hash_codex)
    .bind(&now)
    .bind(if manual { 1 } else { 0 })
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
/// Drop the *direct* link between a project and a skill. The group link is separate: if some
/// enabled group still contains the skill, it stays applied and only loses its `manual` flag —
/// otherwise nothing is keeping it and the artifacts go.
pub async fn remove_skill_from_project(
    db: State<'_, Db>,
    project_id: String,
    skill_id: String,
) -> Result<(), String> {
    unlink_skill_from_project(db.inner(), &project_id, &skill_id).await
}

pub async fn unlink_skill_from_project(
    db: &Db,
    project_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    if crate::commands::groups::claimed_by_enabled_group(
        db,
        crate::commands::groups::GroupKind::Skill,
        project_id,
        skill_id,
    )
    .await?
    {
        sqlx::query("UPDATE project_skills SET manual = 0 WHERE project_id = ? AND skill_id = ?")
            .bind(project_id)
            .bind(skill_id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    remove_skill_from_project_inner(db, project_id, skill_id).await
}

/// Core removal logic shared by the command above and group disablement.
pub async fn remove_skill_from_project_inner(
    db: &Db,
    project_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    use sqlx::Row;

    let skill_row = sqlx::query("SELECT name, source_path FROM skills WHERE id = ?")
        .bind(skill_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    let skill_name: String = skill_row.get("name");
    let source_path: String = skill_row.get("source_path");

    let project_row = sqlx::query("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    let project_path: String = project_row.get("path");

    let installed = sqlx::query(
        "SELECT synced_hash_claude, synced_hash_codex FROM project_skills WHERE project_id = ? AND skill_id = ?",
    )
    .bind(project_id)
    .bind(skill_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;
    let prev_claude: Option<String> = installed.as_ref().and_then(|r| r.get("synced_hash_claude"));
    let prev_codex: Option<String> = installed.as_ref().and_then(|r| r.get("synced_hash_codex"));
    remove_skill_artifacts(&project_path, &skill_name, &source_path, prev_claude.as_deref(), prev_codex.as_deref())?;

    sqlx::query("DELETE FROM project_skills WHERE project_id = ? AND skill_id = ?")
        .bind(project_id)
        .bind(skill_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SyncConflict {
    pub id: String,
    pub project_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub project_name: String,
    pub engine: String,
    pub detected_at: String,
    pub local_hash: String,
    pub source_hash: String,
    pub resolved: bool,
}

#[tauri::command]
pub async fn list_sync_conflicts(db: State<'_, Db>) -> Result<Vec<SyncConflict>, String> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT sc.id, sc.project_id, sc.skill_id, s.name as skill_name, p.name as project_name,
                sc.engine, sc.detected_at, sc.local_hash, sc.source_hash, sc.resolved
         FROM sync_conflicts sc
         JOIN skills s ON s.id = sc.skill_id
         JOIN projects p ON p.id = sc.project_id
         WHERE sc.resolved = 0 AND p.deleted_at IS NULL
         ORDER BY sc.detected_at DESC",
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|row| {
            let resolved: i64 = row.get("resolved");
            SyncConflict {
                id: row.get("id"),
                project_id: row.get("project_id"),
                skill_id: row.get("skill_id"),
                skill_name: row.get("skill_name"),
                project_name: row.get("project_name"),
                engine: row.get("engine"),
                detected_at: row.get("detected_at"),
                local_hash: row.get("local_hash"),
                source_hash: row.get("source_hash"),
                resolved: resolved != 0,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn resolve_sync_conflict(
    db: State<'_, Db>,
    conflict_id: String,
    overwrite: bool,
) -> Result<(), String> {
    use sqlx::Row;
    if overwrite {
        // Legacy conflicts (from the copy era). The user chose the central version, so the edited
        // copy for that engine goes and the skill is re-linked from source. A conflict whose skill
        // is no longer applied here has nothing left to overwrite.
        let row = sqlx::query(
            "SELECT sc.project_id, sc.skill_id, sc.engine, s.source_path, s.name as skill_name,
                    ps.enabled_claude, ps.enabled_codex, p.path as project_path,
                    ps.synced_hash_claude, ps.synced_hash_codex
             FROM sync_conflicts sc
             JOIN skills s ON s.id = sc.skill_id
             JOIN projects p ON p.id = sc.project_id
             JOIN project_skills ps ON ps.project_id = sc.project_id AND ps.skill_id = sc.skill_id
             WHERE sc.id = ?",
        )
        .bind(&conflict_id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?;

        if let Some(row) = row {
            let project_path: String = row.get("project_path");
            let source_path: String = row.get("source_path");
            let skill_name: String = row.get("skill_name");
            let engines = SkillEngines::from_row(&row);
            let project_id: String = row.get("project_id");
            let skill_id: String = row.get("skill_id");
            let engine: String = row.get("engine");
            let prev_claude: Option<String> = row.get("synced_hash_claude");
            let prev_codex: Option<String> = row.get("synced_hash_codex");

            // Only where the copy era wrote (`.codex/skills` for Codex, `.claude/skills` for
            // Claude), through the same guard as every other removal so a symlinked parent folder
            // is never followed. `known = true`: the user chose to discard this copy.
            let copy = if engine == "codex" {
                legacy_codex_skill_dir(&project_path, &skill_name)
            } else {
                claude_skill_dir(&project_path, &skill_name)
            };
            crate::commands::project_links::remove_installed(
                std::path::Path::new(&project_path),
                &copy,
                std::path::Path::new(&source_path),
                |_: &std::path::Path| true,
            )?;

            let (hash_claude, hash_codex) = write_skill_artifacts(
                &project_path,
                &skill_name,
                engines,
                &source_path,
                prev_claude.as_deref(),
                prev_codex.as_deref(),
            )?;

            sqlx::query(
                "UPDATE project_skills SET synced_hash_claude = ?, synced_hash_codex = ? WHERE project_id = ? AND skill_id = ?",
            )
            .bind(&hash_claude)
            .bind(&hash_codex)
            .bind(&project_id)
            .bind(&skill_id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    sqlx::query("UPDATE sync_conflicts SET resolved = 1 WHERE id = ?")
        .bind(&conflict_id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_remote_url, resolve_terminal_app, terminal_launches, windows_chrome_launch, windows_vscode_command,
        TerminalLaunch,
    };

    #[test]
    fn parses_github_https_and_scp_remotes() {
        let https = parse_remote_url("https://github.com/acme/widget.git").unwrap();
        assert_eq!(https.provider, "github");
        assert_eq!(https.github_owner.as_deref(), Some("acme"));
        assert_eq!(https.github_repo.as_deref(), Some("widget"));

        let scp = parse_remote_url("git@github.com:acme/widget.git").unwrap();
        assert_eq!(scp.provider, "github");
        assert_eq!(scp.github_owner.as_deref(), Some("acme"));
        assert_eq!(scp.github_repo.as_deref(), Some("widget"));
    }

    #[test]
    fn parses_gitlab_remotes_keeping_nested_groups() {
        let https = parse_remote_url("https://gitlab.com/group/sub/widget.git").unwrap();
        assert_eq!(https.provider, "gitlab");
        assert_eq!(https.gitlab_project_path.as_deref(), Some("group/sub/widget"));
        assert!(https.github_owner.is_none());

        let scp = parse_remote_url("git@gitlab.example.com:group/widget.git").unwrap();
        assert_eq!(scp.provider, "gitlab");
        assert_eq!(scp.gitlab_project_path.as_deref(), Some("group/widget"));
    }

    #[test]
    fn ignores_unknown_hosts_and_incomplete_urls() {
        assert!(parse_remote_url("https://bitbucket.org/acme/widget.git").is_none());
        assert!(parse_remote_url("https://github.com/acme").is_none());
        assert!(parse_remote_url("").is_none());
    }

    #[test]
    fn terminal_app_falls_back_to_the_os_default() {
        assert_eq!(resolve_terminal_app("macos", "iterm"), "iterm");
        assert_eq!(resolve_terminal_app("macos", "wt"), "terminal");
        // A Mac backup restored on Windows / Linux.
        assert_eq!(resolve_terminal_app("windows", "iterm"), "powershell");
        assert_eq!(resolve_terminal_app("windows", "terminal"), "powershell");
        assert_eq!(resolve_terminal_app("windows", "cmd"), "cmd");
        assert_eq!(resolve_terminal_app("linux", "iterm"), "auto");
        assert_eq!(resolve_terminal_app("linux", ""), "auto");
        assert_eq!(resolve_terminal_app("linux", "konsole"), "konsole");
    }

    fn launch(program: &'static str, args: &[&str], cwd: &str, new_console: bool) -> TerminalLaunch {
        TerminalLaunch { program, args: args.iter().map(|a| a.to_string()).collect(), cwd: cwd.to_string(), new_console }
    }

    #[test]
    fn windows_terminals_get_the_folder_as_an_argument_or_cwd() {
        let dir = r"C:\Users\me\My Project & 'x' $(y)";
        assert_eq!(terminal_launches("windows", "wt", dir).unwrap(), vec![launch("wt.exe", &["-d", dir], dir, false)]);
        assert_eq!(terminal_launches("windows", "powershell", dir).unwrap(), vec![launch("powershell.exe", &[], dir, true)]);
        assert_eq!(terminal_launches("windows", "cmd", dir).unwrap(), vec![launch("cmd.exe", &[], dir, true)]);
        // A remote caller's canonical path loses its `\\?\` prefix; a UNC share keeps it.
        assert_eq!(terminal_launches("windows", "wt", r"\\?\C:\p").unwrap(), vec![launch("wt.exe", &["-d", r"C:\p"], r"C:\p", false)]);
        assert_eq!(terminal_launches("windows", "cmd", r"\\?\UNC\srv\share").unwrap()[0].cwd, r"\\?\UNC\srv\share");
        // `wt` would split the path at `;` into two commands.
        assert!(terminal_launches("windows", "wt", r"C:\a;b").is_err());
        assert!(terminal_launches("windows", "cmd", r"C:\a;b").is_ok());
    }

    #[test]
    fn linux_terminals_get_the_folder_as_one_argument() {
        let dir = "/home/me/a b;$(rm -rf ~)";
        assert_eq!(
            terminal_launches("linux", "gnome-terminal", dir).unwrap(),
            vec![launch("gnome-terminal", &["--working-directory=/home/me/a b;$(rm -rf ~)"], dir, false)]
        );
        assert_eq!(terminal_launches("linux", "konsole", dir).unwrap(), vec![launch("konsole", &["--workdir", dir], dir, false)]);
        assert_eq!(terminal_launches("linux", "xterm", dir).unwrap(), vec![launch("xterm", &[], dir, false)]);
        let auto: Vec<_> = terminal_launches("linux", "auto", dir).unwrap().into_iter().map(|l| l.program).collect();
        assert_eq!(auto, ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"]);
        // Only Windows has the `\\?\` prefix.
        assert_eq!(terminal_launches("linux", "xterm", r"\\?\C:\p").unwrap()[0].cwd, r"\\?\C:\p");
    }

    #[test]
    fn windows_vscode_runs_its_launcher_not_cmd() {
        let path = r"C:\repo\x&calc";
        let file = r"C:\repo\%PATH%^&notes.md";
        assert_eq!(windows_vscode_command(path, Some(file)), ("code.cmd", vec![path.to_string(), file.to_string()]));
        assert_eq!(windows_vscode_command(path, None), ("code.cmd", vec![path.to_string()]));
    }

    #[test]
    fn windows_chrome_gets_the_path_as_one_quoted_parameter() {
        assert_eq!(windows_chrome_launch(r"C:\repo\x&calc.html"), ("chrome", r#""C:\repo\x&calc.html""#.to_string()));
        assert_eq!(windows_chrome_launch(r"C:\My Site\%TEMP%.html"), ("chrome", r#""C:\My Site\%TEMP%.html""#.to_string()));
        // `C:\"` would read as an escaped quote, not the closing one.
        assert_eq!(windows_chrome_launch(r"C:\"), ("chrome", r#""C:\\""#.to_string()));
    }

    /// Two projects with three runs each in `sqlite::memory:`; `r2` and project `p2` in the trash.
    async fn db_with_trash() -> crate::db::Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in [
            "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')",
            "INSERT INTO runs (id, project_id, type, created_at) VALUES \
             ('r1', 'p1', 'session', '2026-01-01'), ('r2', 'p1', 'session', '2026-01-02'), \
             ('r3', 'p1', 'session', '2026-01-03'), ('r4', 'p2', 'session', '2026-01-04')",
            "UPDATE runs SET deleted_at = 'd' WHERE id = 'r2'",
            "UPDATE projects SET deleted_at = 'd' WHERE id = 'p2'",
        ] {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn lists_leave_out_the_trash() {
        let db = db_with_trash().await;
        // Projects (sidebar, palette): the trashed project is gone, its count skips trashed runs.
        let projects = super::list_projects_inner(&db).await.unwrap();
        let got: Vec<(&str, i64, Option<&str>)> =
            projects.iter().map(|p| (p.id.as_str(), p.run_count, p.last_used_at.as_deref())).collect();
        assert_eq!(got, [("p1", 2, Some("2026-01-03"))]);
        // History (and the palette's sessions): the trashed run is gone.
        let runs = crate::commands::github::list_runs_inner(&db, "p1").await.unwrap();
        assert_eq!(runs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r3", "r1"]);
        // A lookup by id still reaches a trashed row (open, restore, purge).
        let path: String = sqlx::query_scalar("SELECT p.path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = 'r4'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(path, "/b");
    }

    #[tokio::test]
    async fn remote_lists_leave_out_the_trash() {
        let db = db_with_trash().await;
        let runs = crate::remote::forwarder::build_run_list(&db, None).await;
        assert_eq!(runs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r3", "r1"]);
        let projects = crate::remote::forwarder::build_project_list(&db).await;
        assert_eq!(projects.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["p1"]);
        // The newest run is r4, but its project is in the trash.
        assert_eq!(crate::remote::agent::latest_run_id(&db).await.as_deref(), Some("r3"));
    }
}

#[cfg(test)]
mod trash_tests {
    use super::*;

    async fn db_with(rows: &[&str]) -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for sql in rows {
            sqlx::query(sql).execute(&db).await.unwrap();
        }
        db
    }

    const SHOP: &[&str] = &[
        "INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'shop', '/w/shop', 'c')",
        "INSERT INTO repos (id, project_id, name, path, created_at) VALUES ('rp1', 'p1', 'api', '/w/shop/api', 'c')",
        "INSERT INTO runs (id, project_id, type, status, engine, created_at) VALUES \
         ('r1', 'p1', 'session', 'done', 'claude', '2026-01-01'), \
         ('r2', 'p1', 'session', 'done', 'claude', '2026-01-02'), \
         ('r3', 'p1', 'session', 'done', 'claude', '2026-01-03')",
    ];

    async fn deleted_at(db: &Db, table: &str, id: &str) -> Option<String> {
        sqlx::query_scalar(&format!("SELECT deleted_at FROM {table} WHERE id = ?"))
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn trashing_a_project_stamps_only_the_project_and_keeps_its_rows() {
        let db = db_with(SHOP).await;
        let registry = crate::runs::new_registry();
        let run_ids = trash_project_inner(&db, &registry, "p1").await.unwrap();
        assert_eq!(run_ids.len(), 3, "every run is handed back for the conductor state");
        assert!(deleted_at(&db, "projects", "p1").await.is_some());
        for r in ["r1", "r2", "r3"] {
            assert_eq!(deleted_at(&db, "runs", r).await, None, "{r} must not be stamped");
        }
        let repos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repos WHERE project_id = 'p1'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(repos, 1, "links stay until a purge");
        assert!(list_projects_inner(&db).await.unwrap().is_empty());
        // Idempotent: a second trash keeps the first stamp.
        let first = deleted_at(&db, "projects", "p1").await;
        trash_project_inner(&db, &registry, "p1").await.unwrap();
        assert_eq!(deleted_at(&db, "projects", "p1").await, first);
        assert!(trash_project_inner(&db, &registry, "nope").await.is_err());
    }

    #[tokio::test]
    async fn a_project_with_a_running_session_is_not_trashed() {
        let db = db_with(SHOP).await;
        sqlx::query("UPDATE runs SET status = 'running' WHERE id = 'r2'").execute(&db).await.unwrap();
        let err = trash_project_inner(&db, &crate::runs::new_registry(), "p1").await.unwrap_err();
        assert!(err.contains("still running"), "{err}");
        assert_eq!(deleted_at(&db, "projects", "p1").await, None);
    }

    #[tokio::test]
    async fn adding_the_path_again_restores_the_same_project_with_its_sessions() {
        let db = db_with(SHOP).await;
        trash_project_inner(&db, &crate::runs::new_registry(), "p1").await.unwrap();
        // A session trashed on its own stays in the Trash after the project comes back.
        sqlx::query("UPDATE runs SET deleted_at = 'd' WHERE id = 'r3'").execute(&db).await.unwrap();

        assert_eq!(restore_trashed_path(&db, "/w/other").await.unwrap(), None);
        assert_eq!(restore_trashed_path(&db, "/w/shop").await.unwrap().as_deref(), Some("p1"));
        let project = project_by_id(&db, "p1").await.unwrap();
        assert_eq!((project.id.as_str(), project.path.as_str(), project.run_count), ("p1", "/w/shop", 2));
        let listed: Vec<String> = list_projects_inner(&db).await.unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(listed, ["p1"]);
        // A live path is not "restored" (add_project then fails on UNIQUE as before).
        assert_eq!(restore_trashed_path(&db, "/w/shop").await.unwrap(), None);
    }

    #[tokio::test]
    async fn restore_project_brings_back_sessions_not_trashed_on_their_own() {
        let db = db_with(SHOP).await;
        sqlx::query("UPDATE runs SET deleted_at = 'd' WHERE id = 'r1'").execute(&db).await.unwrap();
        trash_project_inner(&db, &crate::runs::new_registry(), "p1").await.unwrap();
        restore_project_inner(&db, "p1").await.unwrap();
        assert_eq!(deleted_at(&db, "projects", "p1").await, None);
        assert!(deleted_at(&db, "runs", "r1").await.is_some());
        assert_eq!(project_by_id(&db, "p1").await.unwrap().run_count, 2);
        assert!(restore_project_inner(&db, "nope").await.is_err());
    }
}
