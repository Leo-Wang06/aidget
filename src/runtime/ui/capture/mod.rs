//! Native WebView screenshot orchestration.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::common::error::{Error, Result};
use crate::widgets::store::Store;

use super::state::{label, record_revision, widget_id, UiState};

#[cfg(target_os = "macos")]
mod apple;
#[cfg(target_os = "linux")]
mod webkitgtk;
#[cfg(windows)]
mod webview2;

// Keep card previews sharp when the image is scaled down into the grid.
pub(super) const MAIN_PREVIEW_SCALE: f64 = 2.0;

type SnapshotSender = Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>>>>;

/// Capture strategy for a native WebView snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScreenshotMode {
    /// Capture the viewport without changing it; used by the local widget panel.
    PanelPreview,
    /// Capture at an explicit number of output pixels per layout point.
    ViewportScaled(f64),
}

/// Select a snapshot strategy that preserves the requested output scale.
pub fn screenshot_mode(preview: bool, scale: f64) -> ScreenshotMode {
    if preview {
        ScreenshotMode::PanelPreview
    } else {
        ScreenshotMode::ViewportScaled(scale)
    }
}

/// Capture a widget WebView as a PNG at `scale` pixels per layout point.
pub async fn screenshot(app: &AppHandle, id: &str, scale: f64) -> Result<Vec<u8>> {
    capture_widget(app, id, screenshot_mode(false, scale)).await
}

pub(super) async fn capture_widget(
    app: &AppHandle,
    id: &str,
    mode: ScreenshotMode,
) -> Result<Vec<u8>> {
    let scale = match mode {
        ScreenshotMode::PanelPreview => MAIN_PREVIEW_SCALE,
        ScreenshotMode::ViewportScaled(scale) => scale,
    };
    let expected = Store::new(app.state::<UiState>().root.lock().unwrap().clone())
        .load_meta(id)?
        .revision;
    if wait_loaded(app, id, expected, Duration::from_secs(3)).await {
        wait_ready(app, id, expected, Duration::from_millis(500)).await;
    }
    let window = app
        .get_webview_window(&label(id))
        .ok_or_else(|| Error::NotFound(id.to_string()))?;
    // The webview runs on the main thread, so the viewport is read here and
    // handed to the capture as a size in layout points.
    let viewport = window
        .inner_size()
        .and_then(|size| window.scale_factor().map(|factor| (size, factor)))
        .map(|(size, factor)| {
            (
                f64::from(size.width) / factor,
                f64::from(size.height) / factor,
            )
        })
        .unwrap_or((0.0, 0.0));
    #[cfg(target_os = "macos")]
    let _ = viewport;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(tx)));
    let sender_for_cb = Arc::clone(&sender);
    window
        .with_webview(move |platform| {
            let sender = Arc::clone(&sender_for_cb);
            #[cfg(target_os = "macos")]
            unsafe {
                apple::capture(platform.inner(), sender, scale);
            }
            #[cfg(windows)]
            unsafe {
                match mode {
                    ScreenshotMode::PanelPreview => webview2::capture_preview(platform, sender),
                    ScreenshotMode::ViewportScaled(_) => {
                        webview2::capture_scaled(platform, sender, scale, viewport)
                    }
                }
            }
            #[cfg(target_os = "linux")]
            webkitgtk::capture(platform.inner(), sender, scale, viewport);
            #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
            {
                let _ = (platform, scale, viewport);
                send_failure(&sender, unsupported_capture());
            }
        })
        .map_err(|error| Error::Other(error.to_string()))?;

    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Error::Other("screenshot channel closed".to_string())),
        Err(_) => Err(Error::Timeout),
    }
}

/// The page reports every iframe `load`, so a snapshot taken right after a
/// write does not capture the previous document. Best-effort: a widget whose
/// page never loads still gets its screenshot after the timeout.
async fn wait_loaded(app: &AppHandle, id: &str, revision: u64, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let loaded = app
            .state::<UiState>()
            .loaded
            .lock()
            .unwrap()
            .get(id)
            .copied()
            .unwrap_or(0);
        if loaded >= revision {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// Wait briefly for `window.aidget.ready()` without making it mandatory.
async fn wait_ready(app: &AppHandle, id: &str, revision: u64, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let ready = app
            .state::<UiState>()
            .ready
            .lock()
            .unwrap()
            .get(id)
            .copied()
            .unwrap_or(0);
        if ready >= revision || Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// Called by the widget page once its iframe finishes loading `revision`.
/// Windows keeps new widget windows hidden until now so the first visible frame
/// already contains the document.
pub fn mark_loaded(app: &AppHandle, window: &WebviewWindow, revision: u64) -> Result<()> {
    let id = window
        .label()
        .strip_prefix("widget-")
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))?
        .to_string();
    record_revision(
        &mut app.state::<UiState>().loaded.lock().unwrap(),
        &id,
        revision,
    );
    #[cfg(windows)]
    if !window.is_visible()? {
        crate::common::utils::reveal_window_at_bottom(window.hwnd()?)?;
    }
    Ok(())
}

/// Called when a widget document reports that its initialization is complete.
pub fn mark_ready(app: &AppHandle, window: &WebviewWindow, revision: u64) -> Result<()> {
    if revision == 0 {
        return Err(Error::InvalidParams(
            "ready revision must be greater than zero".to_string(),
        ));
    }
    let id = widget_id(window)?;
    record_revision(
        &mut app.state::<UiState>().ready.lock().unwrap(),
        &id,
        revision,
    );
    Ok(())
}

/// The error a screenshot returns on targets without a native webview capture.
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn unsupported_capture() -> Error {
    Error::Other("screenshot capture is unsupported on this platform".to_string())
}

/// Answers a snapshot request with an error.
#[cfg(not(target_os = "macos"))]
fn send_failure(sender: &SnapshotSender, error: Error) {
    if let Some(tx) = take_sender(sender) {
        let _ = tx.send(Err(error));
    }
}

/// Takes the waiting snapshot sender.
///
/// The mutex guard has to die before the caller's `Arc` does, so the take
/// happens in its own statement.
#[cfg(not(target_os = "macos"))]
fn take_sender(sender: &SnapshotSender) -> Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>> {
    sender.lock().unwrap().take()
}
