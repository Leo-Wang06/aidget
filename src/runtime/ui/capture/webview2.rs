//! Windows WebView2 snapshot backends.
use std::sync::Arc;

use crate::common::error::{Error, Result};

use super::{send_failure, take_sender, SnapshotSender};

/// Capture panel thumbnails without temporarily changing the WebView viewport.
pub(super) unsafe fn capture_preview(
    platform: tauri::webview::PlatformWebview,
    sender: SnapshotSender,
) {
    use webview2_com::CapturePreviewCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};

    let result = (|| -> windows::core::Result<()> {
        let webview = unsafe { platform.controller().CoreWebView2()? };
        let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true)? };
        let output = stream.clone();
        let sender_for_handler = Arc::clone(&sender);
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |status| {
            let result = (|| -> Result<Vec<u8>> {
                status.map_err(|error| Error::Other(format!("preview capture: {error}")))?;
                let mut stat = STATSTG::default();
                unsafe {
                    output
                        .Stat(&mut stat, STATFLAG_NONAME)
                        .map_err(|error| Error::Other(format!("preview stream: {error}")))?;
                    output
                        .Seek(0, STREAM_SEEK_SET, None)
                        .map_err(|error| Error::Other(format!("preview stream: {error}")))?;
                }
                let length = u32::try_from(stat.cbSize)
                    .map_err(|_| Error::Other("preview stream is too large".to_string()))?;
                let mut png = vec![0; length as usize];
                let mut read = 0;
                unsafe { output.Read(png.as_mut_ptr().cast(), length, Some(&mut read)) }
                    .ok()
                    .map_err(|error| Error::Other(format!("preview stream: {error}")))?;
                if read != length {
                    return Err(Error::Other("preview stream is incomplete".to_string()));
                }
                Ok(png)
            })();
            if let Some(tx) = take_sender(&sender_for_handler) {
                let _ = tx.send(result);
            }
            Ok(())
        }));
        unsafe {
            webview.CapturePreview(
                COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                &stream,
                &handler,
            )
        }
    })();
    if let Err(error) = result {
        send_failure(&sender, Error::Other(format!("preview capture: {error}")));
    }
}

/// PNG of the WebView2 surface at `scale` output pixels per layout point.
///
/// `CapturePreview` copies the current viewport at its native density and takes
/// no clip, so it cannot honour `scale`. DevTools `Page.captureScreenshot` does:
/// its clip is in CSS pixels and `clip.scale` is pixels per CSS pixel, which is
/// exactly the meaning aidget gives `scale`.
pub(super) unsafe fn capture_scaled(
    platform: tauri::webview::PlatformWebview,
    sender: SnapshotSender,
    scale: f64,
    viewport: (f64, f64),
) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use windows::core::Interface;

    let fail = |error: Error| {
        send_failure(&sender, error);
    };
    if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return fail(Error::Other("widget window has no size".to_string()));
    }

    let controller = platform.controller();
    let webview = match unsafe { controller.CoreWebView2() }
        .and_then(|webview| webview.cast::<ICoreWebView2_2>())
    {
        Ok(webview) => webview,
        Err(error) => return fail(Error::Other(format!("webview2 handle: {error}"))),
    };

    let params = serde_json::json!({
        "format": "png",
        "captureBeyondViewport": true,
        "clip": {
            "x": 0.0,
            "y": 0.0,
            "width": viewport.0,
            "height": viewport.1,
            "scale": scale,
        },
    });
    let method = windows::core::HSTRING::from("Page.captureScreenshot");
    let json = windows::core::HSTRING::from(params.to_string());
    let sender_for_handler = Arc::clone(&sender);
    let handler =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |status, answer| {
            let result = status
                .map_err(|error| Error::Other(format!("devtools capture failed: {error}")))
                .and_then(|_| decode_devtools_png(&answer));
            if let Some(tx) = take_sender(&sender_for_handler) {
                let _ = tx.send(result);
            }
            Ok(())
        }));
    if let Err(error) = unsafe { webview.CallDevToolsProtocolMethod(&method, &json, &handler) } {
        fail(Error::Other(format!("devtools capture: {error}")));
    }
}

/// Pull the base64 PNG out of the `Page.captureScreenshot` reply.
fn decode_devtools_png(answer: &str) -> Result<Vec<u8>> {
    use base64::Engine;

    let value: serde_json::Value = serde_json::from_str(answer)
        .map_err(|error| Error::Other(format!("devtools reply: {error}")))?;
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Error::Other("devtools reply has no data".to_string()))?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| Error::Other(format!("devtools png: {error}")))
}
