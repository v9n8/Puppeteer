//! The clipboard's plain text, for the composer's "paste the other way" shortcut
//! (⌘⇧V): macOS fires no `paste` event for it, and `navigator.clipboard.readText`
//! makes WKWebView ask the user to click a native "Paste" bubble first.
//!
//! Remote: local-only. A controller must never read this machine's clipboard (the
//! secret vault copies secrets to it); the frontend calls it with `localInvoke`.

/// The clipboard's plain text, or `None` when it holds none.
#[tauri::command]
pub fn read_clipboard_text() -> Option<String> {
    read()
}

#[cfg(target_os = "macos")]
fn read() -> Option<String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    let pb = NSPasteboard::generalPasteboard();
    pb.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string())
}

// WINDOWS-LINUX-REVIEW: unverified that the composer never needs this off macOS; there it
// relies on the native `paste` event WebView2 / WebKitGTK fire for Ctrl+Shift+V instead.
#[cfg(not(target_os = "macos"))]
fn read() -> Option<String> {
    None
}
