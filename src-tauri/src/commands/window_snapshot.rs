//! Thumbnails of the app's own windows for the "Open windows" overlay (V3-67).
//!
//! macOS only: `WKWebView.takeSnapshot` renders the page of a window without Screen
//! Recording permission (it is our own web view, not the screen). Windows / Linux answer
//! `unsupported` and the overlay keeps its big icons. Windows are shot one after the
//! other so 17 windows do not pile up work on the main thread.
use serde::Serialize;

/// Labels that are never shot: the main window, the transient ones and the secret vault.
const NEVER_SHOT: &[&str] = &["main", "remote-bridge", "secret-notes", "remote-secret-notes"];

#[derive(Serialize)]
pub struct WindowSnapshot {
    pub label: String,
    /// A `data:image/jpeg;base64,...` URL.
    pub image: Option<String>,
    pub error: Option<String>,
}

fn is_snapshot_allowed(label: &str) -> bool {
    let own = label.strip_prefix("remote-").unwrap_or(label);
    !NEVER_SHOT.contains(&label) && !own.starts_with("permission-")
}

/// The labels to shoot: allowed ones only, each once.
pub fn snapshot_targets(labels: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in labels {
        if is_snapshot_allowed(l) && !out.contains(l) {
            out.push(l.clone());
        }
    }
    out
}

#[tauri::command]
pub async fn snapshot_windows(
    app: tauri::AppHandle,
    labels: Vec<String>,
    width: u32,
) -> Vec<WindowSnapshot> {
    let width = width.clamp(64, 1280);
    let mut out = Vec::with_capacity(labels.len());
    for label in labels {
        let res = if is_snapshot_allowed(&label) {
            shoot(&app, &label, width).await
        } else {
            Err("refused".to_string())
        };
        out.push(match res {
            Ok(image) => WindowSnapshot { label, image: Some(image), error: None },
            Err(e) => WindowSnapshot { label, image: None, error: Some(e) },
        });
    }
    out
}

#[cfg(not(target_os = "macos"))]
async fn shoot(_app: &tauri::AppHandle, _label: &str, _width: u32) -> Result<String, String> {
    Err("unsupported".to_string())
}

#[cfg(target_os = "macos")]
async fn shoot(app: &tauri::AppHandle, label: &str, width: u32) -> Result<String, String> {
    use base64::Engine;
    let bytes = macos::snap(app, label, width as f64).await?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Mutex;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError, NSNumber};
    use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};
    use tauri::{AppHandle, Manager};
    use tokio::sync::oneshot;

    /// One window's answer must come within this, else its card keeps the icon.
    const TIMEOUT: Duration = Duration::from_millis(1500);

    pub async fn snap(app: &AppHandle, label: &str, width: f64) -> Result<Vec<u8>, String> {
        let win = app.get_webview_window(label).ok_or("no such window")?;
        let (tx, rx) = oneshot::channel::<Result<Vec<u8>, String>>();
        let tx = Mutex::new(Some(tx));
        win.with_webview(move |w| {
            let fail = |msg: &str| {
                if let Some(t) = tx.lock().unwrap().take() {
                    let _ = t.send(Err(msg.to_string()));
                }
            };
            let ptr = w.inner() as *mut WKWebView;
            // SAFETY: `inner()` is the window's WKWebView, valid on the main thread we run on.
            let Some(wv) = (unsafe { Retained::retain(ptr) }) else {
                return fail("null webview");
            };
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return fail("not on main thread");
            };
            let cfg = unsafe { WKSnapshotConfiguration::new(mtm) };
            unsafe {
                cfg.setSnapshotWidth(Some(&NSNumber::new_f64(width)));
                // Skip waiting for a repaint: a stale frame is fine for a thumbnail.
                cfg.setAfterScreenUpdates(false);
            }
            let block = RcBlock::new(move |img: *mut NSImage, err: *mut NSError| {
                let res = if img.is_null() {
                    Err(unsafe { err.as_ref() }
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "unknown".into()))
                } else {
                    encode_jpeg(unsafe { &*img }).ok_or_else(|| "encode failed".to_string())
                };
                if let Some(t) = tx.lock().unwrap().take() {
                    let _ = t.send(res);
                }
            });
            unsafe { wv.takeSnapshotWithConfiguration_completionHandler(Some(&cfg), &block) };
        })
        .map_err(|e| e.to_string())?;
        match tokio::time::timeout(TIMEOUT, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("timeout".to_string()),
        }
    }

    fn encode_jpeg(img: &NSImage) -> Option<Vec<u8>> {
        let tiff = img.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)?;
        let props: Retained<NSDictionary<_, AnyObject>> = NSDictionary::new();
        let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &props) }?;
        Some(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn targets_skip_main_transient_and_the_vault() {
        let got = snapshot_targets(&s(&[
            "main", "calendar", "secret-notes", "remote-secret-notes", "permission-prompt",
            "remote-permission-prompt", "remote-bridge", "session-1", "calendar", "remote-desktop",
        ]));
        assert_eq!(got, s(&["calendar", "session-1", "remote-desktop"]));
    }

    #[test]
    fn vault_labels_are_never_allowed() {
        assert!(!is_snapshot_allowed("secret-notes"));
        assert!(!is_snapshot_allowed("remote-secret-notes"));
        assert!(is_snapshot_allowed("settings"));
    }
}
