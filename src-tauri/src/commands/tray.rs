//! System tray icon with a quick window + session switcher: the menu-bar extra on
//! macOS, the notification-area icon on Windows, an AppIndicator / StatusNotifier
//! item on Linux.
//!
//! Clicking the tray icon drops a menu with two sections: every open window (jump
//! straight to a pop-out or back to the main window without hunting through
//! Mission Control) and every running session (open it where it lives — its own
//! pop-out if it has one, otherwise the main window).
//!
//! Per OS (Tauri v2 `TrayIconBuilder` / `TrayIcon` docs):
//! - macOS: monochrome template glyph, menu on any click, running count beside it.
//! - Windows: colored app icon, left click raises the main window and right click
//!   opens the menu (the notification-area convention). No title text there.
//! - Linux: colored app icon. The indicator host opens the menu itself and Tauri
//!   gets no click events, so the menu (which lists the main window) is the only
//!   way in; `show_menu_on_left_click` and the tooltip are ignored.
//!
//! Same split as the app menu (`app_menu.rs`): the menu's *contents* are described
//! by the frontend and sent down via `set_tray_menu`, so labels follow the app
//! language and each session's human title. Rust only owns the icon, routes a
//! click to the matching window, and handles Quit. The frontend re-sends the spec
//! whenever a window opens/closes or a run is renamed (see `src/lib/tray.ts`).
//!
//! The two sections have different owners: ANY window may resend the window list,
//! but only the main window knows every running session. So each section is sent
//! independently and cached here, and the menu is rebuilt from the cached pair —
//! a pop-out refreshing the window list can't wipe the main window's sessions.
//!
//! A third, static section on top holds quick actions (New Todo… / New Note…).
//! Their ids are app-menu ids wrapped as `tray:menu:<id>`, so they run through the
//! exact same frontend handlers as the menu bar. The window list is also shown in
//! the app menu's Window submenu (see `app_menu.rs`), which is rebuilt whenever
//! that list changes.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Monochrome menu-bar glyph (`</>`), black + alpha so macOS can tint it as a
/// template image — it turns white on a dark menu bar, black on a light one.
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/tray-template.png");
/// Only macOS tints template images. Windows and Linux would draw the glyph as
/// plain black, which vanishes on a dark taskbar or panel, so they use the
/// colored app icon instead.
const USE_TEMPLATE_ICON: bool = cfg!(target_os = "macos");
/// Windows opens the menu on right click and leaves left click to the app. Linux
/// ignores this setting (the indicator host decides).
const MENU_ON_LEFT_CLICK: bool = !cfg!(windows);

/// Stable id so `set_tray_menu` can find the tray built at startup.
pub const TRAY_ID: &str = "puppeteer-tray";
/// Reserved menu id: quit the whole app (mirrors closing the main window).
const QUIT_ID: &str = "tray:quit";
/// Prefix for "focus this window" ids; the suffix is the window label.
///
/// Tauri delivers EVERY menu click — app menu AND tray — to one shared global
/// listener list, so both `app_menu::on_menu_event` and `on_menu_event` below run
/// for each click. The `tray:` namespace keeps them from acting on each other's
/// ids: this handler ignores anything without it, and the app-menu handler
/// early-returns on it (see `app_menu::on_menu_event`).
const WIN_PREFIX: &str = "tray:win:";
/// Prefix for "open this running session" ids; the suffix is the run id.
const RUN_PREFIX: &str = "tray:run:";
/// Prefix for quick actions; the suffix is an app-menu item id (e.g. `file.newTodo`).
const MENU_PREFIX: &str = "tray:menu:";
/// Event sent to the main window when a session row is clicked. The main window
/// decides where the run is shown (its pop-out, or itself) — see `src/lib/tray.ts`.
const OPEN_RUN_EVENT: &str = "tray://open-run";
/// Row id that opens the background-process view in the main window.
const PROCS_ID: &str = "tray:procs";
/// Event sent to the main window for [`PROCS_ID`].
const OPEN_PROCS_EVENT: &str = "tray:open-processes";
/// Row id that opens the Focus board (its window; the main window creates it
/// when missing, see `focus::scheduler::open_focus`).
const FOCUS_ID: &str = "tray:focus";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrayItem {
    /// A clickable row. `id` is `tray:win:<label>`, `tray:run:<runId>`,
    /// `tray:menu:<appMenuId>`, `tray:procs`, `tray:focus` or `tray:quit`.
    Item { id: String, label: String },
    /// A greyed-out, non-clickable section title.
    Header { label: String },
    /// A nested menu (a conductor with its workers).
    Submenu { label: String, items: Vec<TrayItem> },
    Separator,
}

#[derive(Debug, Serialize, Clone)]
struct OpenRunPayload {
    #[serde(rename = "runId")]
    run_id: String,
}

/// Last section contents received from the frontend, merged into one menu.
pub struct TrayState {
    actions: Mutex<Vec<TrayItem>>,
    windows: Mutex<Vec<TrayItem>>,
    sessions: Mutex<Vec<TrayItem>>,
    quit_label: Mutex<String>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            actions: Mutex::default(),
            windows: Mutex::default(),
            sessions: Mutex::default(),
            quit_label: Mutex::new("Quit Puppeteer".into()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TraySpec {
    /// The quick-action section (New Todo… / New Note…). Absent → keep the cached one.
    #[serde(default)]
    actions: Option<Vec<TrayItem>>,
    /// The open-window section. Absent → keep the cached one.
    #[serde(default)]
    windows: Option<Vec<TrayItem>>,
    /// The running-session section. Only the main window sends it (pop-outs don't
    /// know every live run). Absent → keep the cached one.
    #[serde(default)]
    sessions: Option<Vec<TrayItem>>,
    /// Localized label for the Quit row. Absent → keep the cached one.
    #[serde(default, rename = "quitLabel")]
    quit_label: Option<String>,
    /// Text shown next to the tray glyph (macOS; the indicator label on Linux;
    /// Windows has no tray title and ignores it) — the count of running
    /// sessions, including ones not popped out into their own window. Only the
    /// main window sends this; other windows omit it so they don't clobber the
    /// count. The double Option lets serde tell "key absent" (outer `None` →
    /// leave the title untouched) from "present but empty" (`Some(Some(""))` →
    /// clear back to icon-only).
    #[serde(default)]
    title: Option<Option<String>>,
}

fn build_submenu<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    items: &[TrayItem],
) -> Result<Submenu<R>, String> {
    let mut sub = SubmenuBuilder::new(app, label);
    for item in items {
        sub = match item {
            TrayItem::Separator => sub.separator(),
            TrayItem::Item { id, label } => sub.item(
                &MenuItemBuilder::with_id(id.clone(), label)
                    .build(app)
                    .map_err(|e| e.to_string())?,
            ),
            TrayItem::Header { label } => sub.item(
                &MenuItemBuilder::new(label)
                    .enabled(false)
                    .build(app)
                    .map_err(|e| e.to_string())?,
            ),
            TrayItem::Submenu { label, items } => sub.item(&build_submenu(app, label, items)?),
        };
    }
    sub.build().map_err(|e| e.to_string())
}

/// Build the tray menu: quick actions, windows, then sessions, then Quit — each
/// non-empty section separated from the next.
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    actions: &[TrayItem],
    windows: &[TrayItem],
    sessions: &[TrayItem],
    quit_label: &str,
) -> Result<tauri::menu::Menu<R>, String> {
    let mut menu = MenuBuilder::new(app);
    for section in [actions, windows, sessions] {
        if section.is_empty() {
            continue;
        }
        for item in section {
            menu = match item {
                TrayItem::Separator => menu.separator(),
                TrayItem::Item { id, label } => menu.item(
                    &MenuItemBuilder::with_id(id.clone(), label)
                        .build(app)
                        .map_err(|e| e.to_string())?,
                ),
                TrayItem::Header { label } => menu.item(
                    &MenuItemBuilder::new(label)
                        .enabled(false)
                        .build(app)
                        .map_err(|e| e.to_string())?,
                ),
                TrayItem::Submenu { label, items } => {
                    menu.item(&build_submenu(app, label, items)?)
                }
            };
        }
        menu = menu.separator();
    }
    let quit = MenuItemBuilder::with_id(QUIT_ID, quit_label)
        .build(app)
        .map_err(|e| e.to_string())?;
    menu.item(&quit).build().map_err(|e| e.to_string())
}

/// Route a tray-menu click. `tray:quit` closes the main window (which runs the
/// normal sidecar-cleanup + exit path, see `lib.rs`); `tray:win:<label>` brings
/// that window to the front; `tray:run:<runId>` is handed to the main window,
/// which focuses the run's pop-out or navigates itself to the run;
/// `tray:menu:<id>` runs app-menu action `<id>` in the main window, without
/// raising it (the actions open their own window).
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().0.as_str();
    if id == QUIT_ID {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.close();
        } else {
            app.exit(0);
        }
        return;
    }
    if id == PROCS_ID {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        let _ = app.emit_to("main", OPEN_PROCS_EVENT, ());
        return;
    }
    if id == FOCUS_ID {
        use crate::focus::scheduler::{FOCUS_WINDOW, OPEN_WINDOW_EVENT};
        if let Some(win) = app.get_webview_window(FOCUS_WINDOW) {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        } else {
            let _ = app.emit_to("main", OPEN_WINDOW_EVENT, serde_json::json!({ "todo_id": null }));
        }
        return;
    }
    if let Some(label) = id.strip_prefix(WIN_PREFIX) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        return;
    }
    if let Some(action) = id.strip_prefix(MENU_PREFIX) {
        let _ = app.emit_to("main", super::app_menu::MENU_ACTION_EVENT, action.to_string());
        return;
    }
    if let Some(run_id) = id.strip_prefix(RUN_PREFIX) {
        let _ = app.emit_to(
            "main",
            OPEN_RUN_EVENT,
            OpenRunPayload {
                run_id: run_id.to_string(),
            },
        );
    }
    // Anything else is an app-menu id; app_menu::on_menu_event owns it.
}

/// Whether a click on the icon itself should raise the main window: a left click
/// on Windows, where the menu sits on the right button. Never on macOS (the
/// left click opens the menu) or Linux (no click events reach the app).
fn left_click_opens_main(button: MouseButton, state: MouseButtonState) -> bool {
    !MENU_ON_LEFT_CLICK && button == MouseButton::Left && state == MouseButtonState::Up
}

fn on_tray_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button,
        button_state,
        ..
    } = event
    else {
        return;
    };
    if !left_click_opens_main(button, button_state) {
        return;
    }
    if let Some(win) = tray.app_handle().get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// The cached open-window section, for the app menu's Window submenu.
pub fn cached_windows<R: Runtime>(app: &AppHandle<R>) -> Vec<TrayItem> {
    app.try_state::<TrayState>()
        .and_then(|s| s.windows.lock().ok().map(|w| w.clone()))
        .unwrap_or_default()
}

/// Create the tray icon once, at startup. The initial menu only carries Quit; the
/// frontend fills in the window list via `set_tray_menu` as soon as it mounts.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = TrayState::default();
    let menu = {
        let quit_label = state.quit_label.lock().map_err(|e| e.to_string())?;
        build_menu(app, &[], &[], &[], &quit_label)?
    };
    app.manage(state);

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Puppeteer")
        .menu(&menu)
        .show_menu_on_left_click(MENU_ON_LEFT_CLICK)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_icon_event);

    // macOS: the monochrome template glyph. Elsewhere — or if the glyph somehow
    // fails to decode — the colored app icon (Tauri's default window icon:
    // `icon.ico` on Windows, the first bundle PNG on Linux), so the tray still
    // appears.
    let template = if USE_TEMPLATE_ICON {
        Image::from_bytes(TRAY_ICON_PNG)
            .inspect_err(|e| tracing::warn!(event = "tray_icon_decode_failed", error = %e))
            .ok()
    } else {
        None
    };
    match template {
        Some(icon) => {
            builder = builder.icon(icon).icon_as_template(true);
        }
        None => {
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
        }
    }

    builder.build(app).map_err(|e| e.to_string())?;
    Ok(())
}

/// Update the section(s) the frontend sent and rebuild the tray menu.
#[tauri::command]
pub fn set_tray_menu<R: Runtime>(app: AppHandle<R>, spec: TraySpec) -> Result<(), String> {
    let Some(state) = app.try_state::<TrayState>() else {
        return Ok(());
    };
    let mut windows_changed = false;
    let menu = {
        let mut actions = state.actions.lock().map_err(|e| e.to_string())?;
        let mut windows = state.windows.lock().map_err(|e| e.to_string())?;
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        let mut quit_label = state.quit_label.lock().map_err(|e| e.to_string())?;
        if let Some(q) = spec.quit_label.filter(|q| !q.is_empty()) {
            *quit_label = q;
        }
        if let Some(a) = spec.actions {
            *actions = a;
        }
        if let Some(w) = spec.windows {
            windows_changed = *windows != w;
            *windows = w;
        }
        if let Some(s) = spec.sessions {
            *sessions = s;
        }
        build_menu(&app, &actions, &windows, &sessions, &quit_label)?
    };
    // The app menu's Window submenu mirrors the window list. Rebuilt only on a
    // real change: the main window resends the tray on every session update.
    if windows_changed {
        super::app_menu::rebuild(&app)?;
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
        // Only touch the title when the frontend actually sent one (outer Some);
        // an empty string collapses back to icon-only, a non-empty one shows the
        // running-session count beside the glyph.
        if let Some(title) = &spec.title {
            let title = title.as_deref().filter(|t| !t.is_empty());
            tray.set_title(title).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_raises_main_on_left_click() {
        assert_eq!(
            left_click_opens_main(MouseButton::Left, MouseButtonState::Up),
            cfg!(windows)
        );
        // The press half of the click, and the other buttons, never do: the
        // right button opens the menu on Windows.
        for (button, state) in [
            (MouseButton::Left, MouseButtonState::Down),
            (MouseButton::Right, MouseButtonState::Up),
            (MouseButton::Middle, MouseButtonState::Up),
        ] {
            assert!(!left_click_opens_main(button, state));
        }
    }

    #[test]
    fn template_glyph_and_left_click_menu_only_where_they_apply() {
        assert_eq!(USE_TEMPLATE_ICON, cfg!(target_os = "macos"));
        assert_eq!(MENU_ON_LEFT_CLICK, !cfg!(windows));
        // macOS relies on the glyph decoding, or it falls back to the colored icon.
        let glyph = Image::from_bytes(TRAY_ICON_PNG).expect("tray glyph decodes");
        assert!(glyph.width() > 0 && glyph.height() > 0);
    }
}
