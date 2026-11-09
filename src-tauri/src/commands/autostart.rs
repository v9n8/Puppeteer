//! Launch Puppeteer at login, through the OS's own per-user login item:
//!
//! - macOS: a LaunchAgent at `~/Library/LaunchAgents/<bundle identifier>.plist`
//!   with `RunAtLoad`.
//! - Windows: a `REG_SZ` value named `<bundle identifier>` under
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, holding a command line
//!   of at most 260 characters (Microsoft Learn, "Run and RunOnce Registry Keys").
//! - Linux and other Unix: an entry at `$XDG_CONFIG_HOME/autostart/<bundle
//!   identifier>.desktop`, `~/.config` when unset (freedesktop "Desktop Application
//!   Autostart Specification" + "XDG Base Directory Specification").
//!
//! The OS item IS the setting — nothing is stored in SQLite, so the toggle can't
//! drift from what the OS actually does. The identifier keeps the dev build
//! ("Puppeteer Dev", `io.github.v9n8.puppeteer.dev`) and the release build apart.
//!
//! The item may pass [`MINIMIZED_ARG`]: the app then starts with its main
//! window minimized (closing it would quit the app, see `lib.rs`).

use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

/// Launch argument: start with the main window minimized.
pub const MINIMIZED_ARG: &str = "--minimized";

/// Longest command line a `Run` value may hold (Microsoft Learn, "Run and
/// RunOnce Registry Keys").
const RUN_VALUE_MAX_CHARS: usize = 260;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub struct AutostartStatus {
    pub enabled: bool,
    pub minimized: bool,
}

const OFF: AutostartStatus = AutostartStatus {
    enabled: false,
    minimized: false,
};

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn render_plist(label: &str, exe: &Path, minimized: bool) -> String {
    let mut args = format!(
        "        <string>{}</string>\n",
        xml_escape(&exe.to_string_lossy())
    );
    if minimized {
        args.push_str(&format!("        <string>{MINIMIZED_ARG}</string>\n"));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(label)
    )
}

/// Whether an existing agent file asks for a minimized start. Only Puppeteer writes
/// this file, so a plain substring check is enough.
fn plist_is_minimized(content: &str) -> bool {
    content.contains(&format!("<string>{MINIMIZED_ARG}</string>"))
}

/// The `Run` value: the executable, always quoted since install paths such as
/// `C:\Program Files\…` hold spaces. Windows paths can't contain `"`, so no
/// escaping is needed — a path with one is refused rather than written broken.
fn render_run_value(exe: &Path, minimized: bool) -> Result<String, String> {
    let exe = exe.to_string_lossy();
    if exe.contains('"') {
        return Err(format!("Executable path contains a quote: {exe}"));
    }
    let mut value = format!("\"{exe}\"");
    if minimized {
        value.push(' ');
        value.push_str(MINIMIZED_ARG);
    }
    if value.encode_utf16().count() > RUN_VALUE_MAX_CHARS {
        return Err(format!(
            "Launch command is longer than the {RUN_VALUE_MAX_CHARS} characters Windows allows at login: {value}"
        ));
    }
    Ok(value)
}

fn run_value_is_minimized(value: &str) -> bool {
    value.ends_with(&format!(" {MINIMIZED_ARG}"))
}

/// One `Exec=` argument, quoted per the Desktop Entry Specification ("The Exec
/// key"): always inside double quotes, with `"`, `` ` ``, `$` and `\` escaped by
/// a backslash. `%` is refused by [`render_desktop_entry`].
fn desktop_exec_arg(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// The general escapes of a desktop entry string value. Applied on top of the
/// `Exec` quoting, so a literal `\` in a path ends up as `\\\\` in the file, as
/// the spec requires.
fn desktop_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn render_desktop_entry(name: &str, exe: &Path, minimized: bool) -> Result<String, String> {
    let exe = exe.to_string_lossy();
    // The spec writes a literal `%` as `%%`, but GLib (GNOME's autostart) looks
    // the program up before undoing that and drops the entry; plain `%` is a
    // field code. Refuse rather than write an item that never starts.
    if exe.contains('%') {
        return Err(format!(
            "Launch at login can't start an executable whose path contains '%': {exe}"
        ));
    }
    let mut exec = desktop_exec_arg(&exe);
    if minimized {
        exec.push(' ');
        exec.push_str(MINIMIZED_ARG);
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec={}\nTerminal=false\n",
        desktop_string(name),
        desktop_string(&exec)
    ))
}

/// Status of an autostart entry. `Hidden=true` (spec) and
/// `X-GNOME-Autostart-enabled=false` (GNOME's own switch) mean the desktop
/// won't start it, so the entry counts as off even though the file exists.
fn desktop_entry_status(content: &str) -> AutostartStatus {
    let mut in_main_group = false;
    let mut enabled = true;
    let mut minimized = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_main_group {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("Hidden", "true") | ("X-GNOME-Autostart-enabled", "false") => enabled = false,
            ("Exec", exec) => minimized = exec.ends_with(&format!(" {MINIMIZED_ARG}")),
            _ => {}
        }
    }
    if enabled {
        AutostartStatus { enabled, minimized }
    } else {
        OFF
    }
}

/// `$XDG_CONFIG_HOME/autostart`. Per the Base Directory Specification an unset,
/// empty or relative `$XDG_CONFIG_HOME` is ignored in favour of `$HOME/.config`.
fn xdg_autostart_dir(xdg_config_home: Option<OsString>, home: &Path) -> PathBuf {
    let config = xdg_config_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    config.join("autostart")
}

/// Inside an AppImage the executable runs from a mount that changes on every
/// start; the AppImage runtime puts the file itself in `$APPIMAGE`.
fn appimage_or(exe: PathBuf, appimage: Option<OsString>) -> PathBuf {
    appimage
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or(exe)
}

/// The executable the login item should start.
fn launch_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(if cfg!(target_os = "linux") {
        appimage_or(exe, std::env::var_os("APPIMAGE"))
    } else {
        exe
    })
}

/// Where the macOS / Linux login item lives (Windows uses the Run key).
fn item_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let id = &app.config().identifier;
    Ok(if cfg!(target_os = "macos") {
        home.join("Library/LaunchAgents")
            .join(format!("{id}.plist"))
    } else {
        xdg_autostart_dir(std::env::var_os("XDG_CONFIG_HOME"), &home).join(format!("{id}.desktop"))
    })
}

fn read_item<R: Runtime>(app: &AppHandle<R>) -> Result<AutostartStatus, String> {
    if cfg!(windows) {
        return Ok(match run_key::get(&app.config().identifier)? {
            Some(value) => AutostartStatus {
                enabled: true,
                minimized: run_value_is_minimized(&value),
            },
            None => OFF,
        });
    }
    let Ok(content) = std::fs::read_to_string(item_path(app)?) else {
        return Ok(OFF);
    };
    Ok(if cfg!(target_os = "macos") {
        AutostartStatus {
            enabled: true,
            minimized: plist_is_minimized(&content),
        }
    } else {
        desktop_entry_status(&content)
    })
}

fn write_item<R: Runtime>(app: &AppHandle<R>, minimized: bool) -> Result<(), String> {
    let exe = launch_exe()?;
    if cfg!(windows) {
        let id = &app.config().identifier;
        let value = render_run_value(&exe, minimized)?;
        // A program started from the Run key should not write to it while
        // running (Microsoft Learn). Started from our own value, the rendered
        // value is identical and nothing is written.
        if run_key::get(id)?.as_deref() == Some(value.as_str()) {
            return Ok(());
        }
        return run_key::set(id, &value);
    }
    let path = item_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let content = if cfg!(target_os = "macos") {
        render_plist(&app.config().identifier, &exe, minimized)
    } else {
        render_desktop_entry(&app.package_info().name, &exe, minimized)?
    };
    // Skip identical rewrites (startup refresh) so macOS doesn't re-announce
    // the login item.
    if std::fs::read_to_string(&path).ok().as_deref() == Some(content.as_str()) {
        return Ok(());
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

fn remove_item<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if cfg!(windows) {
        return run_key::delete(&app.config().identifier);
    }
    let path = item_path(app)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The per-user Run key. Off Windows a stub, so the Windows branches above
/// still compile (and are checked) on every OS; `cfg!` never takes them there.
#[cfg(not(windows))]
mod run_key {
    const UNSUPPORTED: &str = "The Run key only exists on Windows";

    pub fn get(_name: &str) -> Result<Option<String>, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn set(_name: &str, _data: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub fn delete(_name: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

#[cfg(windows)]
mod run_key {
    //! `HKCU\…\Run` through the three one-shot Advapi32 calls (no key handle to
    //! manage), declared here instead of pulling in a Windows crate.

    use std::ffi::c_void;
    use std::ptr::null_mut;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// `HKEY_CURRENT_USER`: `(HKEY)(ULONG_PTR)((LONG)0x80000001)`, sign-extended.
    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const REG_SZ: u32 = 1;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_MORE_DATA: i32 = 234;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
        fn RegSetKeyValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            kind: u32,
            data: *const c_void,
            data_len: u32,
        ) -> i32;
        fn RegDeleteKeyValueW(hkey: isize, sub_key: *const u16, value: *const u16) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn error(code: i32) -> String {
        std::io::Error::from_raw_os_error(code).to_string()
    }

    /// The `REG_SZ` value `name`, or `None` when there is none.
    pub fn get(name: &str) -> Result<Option<String>, String> {
        let (key, name) = (wide(RUN_KEY), wide(name));
        // The value may change between the size query and the read; retry then.
        for _ in 0..3 {
            let mut len = 0u32;
            // SAFETY: both strings are NUL-terminated; a null buffer with a
            // valid length pointer only asks for the size.
            let code = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    null_mut(),
                    null_mut(),
                    &mut len,
                )
            };
            match code {
                ERROR_SUCCESS => {}
                ERROR_FILE_NOT_FOUND => return Ok(None),
                c => return Err(error(c)),
            }
            // One spare unit: a value stored without its NUL gets one appended.
            let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
            let mut len = (buf.len() * 2) as u32;
            // SAFETY: `buf` holds `len` bytes.
            let code = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut len,
                )
            };
            match code {
                ERROR_SUCCESS => {
                    let text = &buf[..(len as usize / 2).min(buf.len())];
                    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
                    return Ok(Some(String::from_utf16_lossy(&text[..end])));
                }
                ERROR_FILE_NOT_FOUND => return Ok(None),
                ERROR_MORE_DATA => continue,
                c => return Err(error(c)),
            }
        }
        Err("Run key value kept changing while being read".into())
    }

    /// Writes `data` as the `REG_SZ` value `name` (the key is created if missing).
    pub fn set(name: &str, data: &str) -> Result<(), String> {
        let (key, name, data) = (wide(RUN_KEY), wide(name), wide(data));
        // SAFETY: all three strings are NUL-terminated, and the length counts
        // `data`'s terminating NUL as REG_SZ requires.
        let code = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if code == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(error(code))
        }
    }

    /// Removes the value `name`; a missing one is not an error.
    pub fn delete(name: &str) -> Result<(), String> {
        let (key, name) = (wide(RUN_KEY), wide(name));
        // SAFETY: both strings are NUL-terminated.
        let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        match code {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            c => Err(error(c)),
        }
    }
}

// ---- Commands ----------------------------------------------------------------

/// Startup: an enabled item still points at the previous executable after the
/// app was moved or updated in place — rewrite it with the current path.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let status = match read_item(app) {
        Ok(status) => status,
        Err(e) => {
            tracing::warn!(event = "autostart_refresh_failed", error = %e);
            return;
        }
    };
    if status.enabled {
        if let Err(e) = write_item(app, status.minimized) {
            tracing::warn!(event = "autostart_refresh_failed", error = %e);
        }
    }
}

/// Whether this process was started with [`MINIMIZED_ARG`].
pub fn launched_minimized() -> bool {
    std::env::args().any(|a| a == MINIMIZED_ARG)
}

#[tauri::command]
pub fn autostart_get<R: Runtime>(app: AppHandle<R>) -> Result<AutostartStatus, String> {
    read_item(&app)
}

#[tauri::command]
pub fn autostart_set<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
    minimized: bool,
) -> Result<AutostartStatus, String> {
    if enabled {
        write_item(&app, minimized)?;
    } else {
        remove_item(&app)?;
    }
    read_item(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_has_label_exe_and_run_at_load() {
        let p = render_plist(
            "io.github.v9n8.puppeteer",
            Path::new("/Applications/Puppeteer.app/Contents/MacOS/puppeteer"),
            false,
        );
        assert!(p.contains("<string>io.github.v9n8.puppeteer</string>"));
        assert!(p.contains("<string>/Applications/Puppeteer.app/Contents/MacOS/puppeteer</string>"));
        assert!(p.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(!plist_is_minimized(&p));
    }

    #[test]
    fn plist_minimized_round_trips() {
        let p = render_plist("x", Path::new("/a/b"), true);
        assert!(plist_is_minimized(&p));
    }

    #[test]
    fn plist_escapes_xml_in_paths() {
        let p = render_plist("x", Path::new("/Users/a&b/<Puppeteer>.app"), false);
        assert!(p.contains("/Users/a&amp;b/&lt;Puppeteer&gt;.app"));
    }

    #[test]
    fn run_value_quotes_paths_with_spaces() {
        let exe = Path::new(r"C:\Program Files\Puppeteer\Puppeteer.exe");
        let v = render_run_value(exe, false).unwrap();
        assert_eq!(v, r#""C:\Program Files\Puppeteer\Puppeteer.exe""#);
        assert!(!run_value_is_minimized(&v));
        let v = render_run_value(exe, true).unwrap();
        assert_eq!(
            v,
            r#""C:\Program Files\Puppeteer\Puppeteer.exe" --minimized"#
        );
        assert!(run_value_is_minimized(&v));
    }

    #[test]
    fn run_value_refuses_quotes_and_overlong_commands() {
        assert!(render_run_value(Path::new(r#"C:\a"b.exe"#), false).is_err());
        // 2 quotes + 258 chars = 260: allowed; one more is not.
        let fits = format!(r"C:\{}.exe", "a".repeat(258 - 7));
        assert_eq!(
            render_run_value(Path::new(&fits), false).unwrap().len(),
            260
        );
        let too_long = format!(r"C:\{}.exe", "a".repeat(258 - 6));
        assert!(render_run_value(Path::new(&too_long), false).is_err());
        // --minimized counts too.
        assert!(render_run_value(Path::new(&fits), true).is_err());
    }

    #[test]
    fn desktop_entry_quotes_exec_per_spec() {
        let e =
            render_desktop_entry("Puppeteer", Path::new("/opt/My App/puppeteer"), false).unwrap();
        assert_eq!(
            e,
            "[Desktop Entry]\nType=Application\nName=Puppeteer\nExec=\"/opt/My App/puppeteer\"\nTerminal=false\n"
        );
        assert_eq!(
            desktop_entry_status(&e),
            AutostartStatus {
                enabled: true,
                minimized: false
            }
        );
    }

    #[test]
    fn desktop_entry_escapes_reserved_characters() {
        // Spec: `$` in a quoted argument is `\\$` in the file, `\` is `\\\\`,
        // and `"` / `` ` `` get one backslash each (doubled by the string
        // escape). Checked with `gio launch` (GLib 2.84) on such a path.
        let e =
            render_desktop_entry("Puppeteer", Path::new(r#"/h/a$b\c"e`f'g;h(i)"#), false).unwrap();
        assert!(e.contains(r#"Exec="/h/a\\$b\\\\c\\"e\\`f'g;h(i)""#), "{e}");
        let e = render_desktop_entry("A\\B\nC", Path::new("/x"), false).unwrap();
        assert!(e.contains("Name=A\\\\B\\nC\n"), "{e}");
    }

    #[test]
    fn desktop_entry_refuses_percent_in_paths() {
        assert!(render_desktop_entry("P", Path::new("/opt/100%/p"), false).is_err());
    }

    #[test]
    fn desktop_entry_minimized_round_trips() {
        let e = render_desktop_entry("Puppeteer", Path::new("/opt/p"), true).unwrap();
        assert!(e.contains("Exec=\"/opt/p\" --minimized\n"));
        assert_eq!(
            desktop_entry_status(&e),
            AutostartStatus {
                enabled: true,
                minimized: true
            }
        );
    }

    #[test]
    fn desktop_entry_disabled_by_the_desktop_counts_as_off() {
        let base = render_desktop_entry("Puppeteer", Path::new("/opt/p"), true).unwrap();
        for extra in ["Hidden=true\n", "X-GNOME-Autostart-enabled=false\n"] {
            assert_eq!(desktop_entry_status(&format!("{base}{extra}")), OFF);
        }
        // Keys of another group don't count.
        let other = format!("{base}[Desktop Action x]\nHidden=true\n");
        assert!(desktop_entry_status(&other).enabled);
    }

    #[test]
    fn xdg_autostart_dir_falls_back_to_home_config() {
        let home = Path::new("/home/u");
        let fallback = PathBuf::from("/home/u/.config/autostart");
        assert_eq!(xdg_autostart_dir(None, home), fallback);
        assert_eq!(xdg_autostart_dir(Some("".into()), home), fallback);
        assert_eq!(xdg_autostart_dir(Some("rel/cfg".into()), home), fallback);
        assert_eq!(
            xdg_autostart_dir(Some("/xdg/cfg".into()), home),
            PathBuf::from("/xdg/cfg/autostart")
        );
    }

    #[test]
    fn appimage_path_wins_over_the_mounted_exe() {
        let exe = PathBuf::from("/tmp/.mount_PuppeXYZ/usr/bin/puppeteer");
        assert_eq!(
            appimage_or(exe.clone(), Some("/home/u/Apps/Puppeteer.AppImage".into())),
            PathBuf::from("/home/u/Apps/Puppeteer.AppImage")
        );
        assert_eq!(appimage_or(exe.clone(), None), exe);
        assert_eq!(appimage_or(exe.clone(), Some("rel.AppImage".into())), exe);
    }
}
