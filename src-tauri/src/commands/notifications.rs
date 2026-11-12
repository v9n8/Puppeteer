//! Native permission notifications with working click-to-open handling.
//!
//! `tauri-plugin-notification`'s `onAction` never fires on desktop: its
//! `desktop.rs` calls `notify_rust::Notification::show()` and throws the handle
//! away, so no click event is ever emitted. We bypass that here and drive
//! `notify-rust` directly, keeping the handle so we can wait for the click and
//! emit an app event carrying the originating `projectId` / `runId`. The
//! frontend listens for that event and routes to the exact waiting run.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Event emitted when the user clicks a permission or run-finished notification. The payload
/// tells the frontend which run to open. Serialized camelCase for JS.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationClick {
    project_id: String,
    run_id: String,
}

/// Name of the event the frontend listens on.
pub const NOTIFICATION_CLICKED_EVENT: &str = "permission-notification-clicked";

/// Emitted when the user clicks a calendar-reminder notification. Payload tells
/// the frontend which event to open.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderClick {
    event_id: String,
}

/// Name of the event the frontend listens on for calendar reminders.
pub const CALENDAR_REMINDER_CLICKED_EVENT: &str = "calendar-reminder-clicked";

/// Emitted when the user clicks a "schedule did not start" notification: the
/// frontend opens the Schedules screen.
pub const SCHEDULE_FAILED_CLICKED_EVENT: &str = "schedule-failed-notification-clicked";

#[cfg(target_os = "macos")]
static SET_APP: std::sync::Once = std::sync::Once::new();

/// Show a native notification whose click (body or "Open" button) raises the
/// main window and then runs `on_click`. Returns immediately; the blocking
/// wait for the user's click runs on a dedicated OS thread. `label` only names
/// the notification in the failure log.
pub(crate) fn show_clickable(
    app: AppHandle,
    title: String,
    body: String,
    label: &'static str,
    on_click: impl FnOnce(&AppHandle) + Send + 'static,
) {
    show_clickable_opts(app, title, body, label, true, on_click)
}

/// `show_clickable`; `raise_main = false` leaves window raising to `on_click`
/// (a Focus reminder raises the Focus window instead).
pub(crate) fn show_clickable_opts(
    app: AppHandle,
    title: String,
    body: String,
    label: &'static str,
    raise_main: bool,
    on_click: impl FnOnce(&AppHandle) + Send + 'static,
) {
    show_notification(
        app,
        title,
        body,
        label,
        raise_main,
        open_button_label(None),
        on_click,
    )
}

/// The macOS "Open" button's text: the frontend sends it translated; missing
/// or blank falls back to English.
fn open_button_label(label: Option<String>) -> String {
    label
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "Open".to_string())
}

fn show_notification(
    app: AppHandle,
    title: String,
    body: String,
    label: &'static str,
    raise_main: bool,
    open_label: String,
    on_click: impl FnOnce(&AppHandle) + Send + 'static,
) {
    // macOS needs a bundle identifier to own the notification via
    // NSUserNotificationCenter. Dev builds aren't bundled, so we borrow
    // Terminal's identity exactly like tauri-plugin-notification does.
    #[cfg(target_os = "macos")]
    SET_APP.call_once(|| {
        let ident = if tauri::is_dev() {
            "com.apple.Terminal"
        } else {
            "io.github.v9n8.puppeteer"
        };
        let _ = notify_rust::set_application(ident);
    });

    // `wait_for_action` blocks until the user interacts AND requires the main
    // run loop (Tauri's event loop) to keep pumping to receive the delegate
    // callback — so it must run on a background thread, never the main one.
    std::thread::spawn(move || {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&title).body(&body);
        #[cfg(target_os = "macos")]
        {
            notification.sound_name("default");
            // Critical: NSUserNotificationCenter only *waits for* (and thus
            // reports) a click when the notification "needs a response" — i.e.
            // it has at least one action/close button. Without this, the
            // notification is sent fire-and-forget and `wait_for_action` returns
            // immediately with no click ever captured. Adding an action makes
            // both the "Open" button AND a plain body click deliver an event.
            notification.action("open", &open_label);
        }
        // Only macOS shows the button.
        #[cfg(not(target_os = "macos"))]
        drop(open_label);

        match notification.show() {
            Ok(handle) => {
                handle.wait_for_action(|action| {
                    // "default" = body click, an action id = a button, and
                    // "__closed" = dismissed without acting (ignore that one).
                    if action != "__closed" {
                        // Clicking a notification we handle ourselves does NOT
                        // auto-activate a backgrounded app on macOS, so raise the
                        // main window explicitly before routing.
                        if let Some(win) = app.get_webview_window("main").filter(|_| raise_main) {
                            let _ = win.unminimize();
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                        on_click(&app);
                    }
                });
            }
            Err(err) => {
                tracing::warn!("failed to show {label}: {err}");
            }
        }
    });
}

/// Show a native notification for a pending permission/question on a run the
/// user isn't currently viewing. A click emits `NOTIFICATION_CLICKED_EVENT`
/// with the originating project/run. `open_label`: the translated "Open".
#[tauri::command]
pub async fn show_permission_notification(
    app: AppHandle,
    title: String,
    body: String,
    project_id: String,
    run_id: String,
    open_label: Option<String>,
) -> Result<(), String> {
    show_run_notification(
        app,
        title,
        body,
        "permission notification",
        open_label,
        project_id,
        run_id,
    );
    Ok(())
}

/// Show a native notification that a run finished while the app was in the
/// background. A click emits `NOTIFICATION_CLICKED_EVENT` with the finished
/// project/run, so the frontend opens it exactly like a permission click.
/// `open_label`: the translated "Open".
#[tauri::command]
pub async fn show_run_finished_notification(
    app: AppHandle,
    title: String,
    body: String,
    project_id: String,
    run_id: String,
    open_label: Option<String>,
) -> Result<(), String> {
    show_run_notification(
        app,
        title,
        body,
        "run finished notification",
        open_label,
        project_id,
        run_id,
    );
    Ok(())
}

fn show_run_notification(
    app: AppHandle,
    title: String,
    body: String,
    label: &'static str,
    open_label: Option<String>,
    project_id: String,
    run_id: String,
) {
    show_notification(
        app,
        title,
        body,
        label,
        true,
        open_button_label(open_label),
        move |app| {
            let _ = app.emit(
                NOTIFICATION_CLICKED_EVENT,
                NotificationClick { project_id, run_id },
            );
        },
    );
}

/// Show a native calendar-reminder notification for an upcoming event. A click
/// emits `CALENDAR_REMINDER_CLICKED_EVENT` with the event id so the frontend can
/// open that event's detail drawer.
#[tauri::command]
pub async fn show_calendar_reminder(
    app: AppHandle,
    title: String,
    body: String,
    event_id: String,
) -> Result<(), String> {
    show_clickable(app, title, body, "calendar reminder", move |app| {
        let _ = app.emit(CALENDAR_REMINDER_CLICKED_EVENT, ReminderClick { event_id });
    });
    Ok(())
}

/// Show a native notification that a schedule came due but could not start
/// its session (`schedules::runner::FAILED_EVENT`). There is no run to open,
/// so a click emits `SCHEDULE_FAILED_CLICKED_EVENT` and the frontend opens the
/// Schedules screen. `open_label`: the translated "Open".
#[tauri::command]
pub async fn show_schedule_failed_notification(
    app: AppHandle,
    title: String,
    body: String,
    open_label: Option<String>,
) -> Result<(), String> {
    show_notification(
        app,
        title,
        body,
        "schedule failed notification",
        true,
        open_button_label(open_label),
        |app| {
            let _ = app.emit(SCHEDULE_FAILED_CLICKED_EVENT, ());
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::open_button_label;

    #[test]
    fn open_button_uses_the_translated_label() {
        assert_eq!(open_button_label(Some("Mở".into())), "Mở");
        assert_eq!(open_button_label(Some("  Open  ".into())), "Open");
    }

    #[test]
    fn open_button_falls_back_to_english() {
        assert_eq!(open_button_label(None), "Open");
        assert_eq!(open_button_label(Some("   ".into())), "Open");
    }
}
