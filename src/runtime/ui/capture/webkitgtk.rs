//! Linux WebKitGTK snapshot backend.
use crate::common::error::{Error, Result};

use super::{send_failure, take_sender, SnapshotSender};

/// Captures the visible WebKitGTK viewport and encodes it at the requested scale.
pub(super) fn capture(
    webview: webkit2gtk::WebView,
    sender: SnapshotSender,
    scale: f64,
    viewport: (f64, f64),
) {
    use webkit2gtk::WebViewExt;

    if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        send_failure(
            &sender,
            Error::Other("widget window has no size".to_string()),
        );
        return;
    }
    if !scale.is_finite() || scale <= 0.0 {
        send_failure(
            &sender,
            Error::InvalidParams("screenshot scale must be a finite positive number".to_string()),
        );
        return;
    }
    webkit2gtk::glib::MainContext::default().spawn_local(async move {
        let result = async {
            use webkit2gtk::{SnapshotOptions, SnapshotRegion};

            let captured = webview
                .snapshot_future(SnapshotRegion::Visible, SnapshotOptions::empty())
                .await
                .map_err(|error| Error::Other(format!("webkit snapshot: {error}")))?;
            // SAFETY: WebKitGTK owns a live cairo_surface_t. Both binding versions
            // use the same Cairo C ABI; from_raw_none retains an independent reference.
            let captured = unsafe { cairo::Surface::from_raw_none(captured.to_raw_none().cast()) };
            let captured = cairo::ImageSurface::try_from(captured)
                .map_err(|_| Error::Other("webkit snapshot is not an image surface".to_string()))?;
            if captured.width() <= 0 || captured.height() <= 0 {
                return Err(Error::Other("webkit snapshot is empty".to_string()));
            }
            let width = (viewport.0 * scale).round() as i32;
            let height = (viewport.1 * scale).round() as i32;
            if width <= 0 || height <= 0 {
                return Err(Error::InvalidParams(
                    "screenshot scale produces an empty image".to_string(),
                ));
            }
            let image = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
                .map_err(|error| Error::Other(format!("screenshot surface: {error}")))?;
            let context = cairo::Context::new(&image)
                .map_err(|error| Error::Other(format!("screenshot context: {error}")))?;
            context.scale(
                f64::from(width) / f64::from(captured.width()),
                f64::from(height) / f64::from(captured.height()),
            );
            context
                .set_source_surface(&captured, 0.0, 0.0)
                .map_err(|error| Error::Other(format!("screenshot source: {error}")))?;
            context
                .paint()
                .map_err(|error| Error::Other(format!("screenshot paint: {error}")))?;
            let mut png = Vec::new();
            image
                .write_to_png(&mut png)
                .map_err(|error| Error::Other(format!("screenshot png: {error}")))?;
            Ok(png)
        }
        .await;
        if let Some(tx) = take_sender(&sender) {
            let _ = tx.send(result);
        }
    });
}
