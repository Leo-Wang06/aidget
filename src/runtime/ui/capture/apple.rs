//! macOS WKWebView snapshot backend.
use std::sync::Arc;

use crate::common::error::{Error, Result};

use super::SnapshotSender;

pub(super) unsafe fn capture(webview: *mut std::ffi::c_void, sender: SnapshotSender, scale: f64) {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{MainThreadMarker, NSDictionary, NSNumber, NSString};
    use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};

    let webview: &WKWebView = &*(webview as *const WKWebView);
    let Some(mtm) = MainThreadMarker::new() else {
        if let Some(tx) = sender.lock().unwrap().take() {
            let _ = tx.send(Err(Error::Other(
                "screenshot must run on the main thread".to_string(),
            )));
        }
        return;
    };
    let config = WKSnapshotConfiguration::new(mtm);
    // A widget window is Retina (2x), and the default snapshot keeps that
    // density: a 380x190 card becomes 760x380 px. `snapshotWidth` is measured
    // in points, so dividing by the backing scale factor yields `scale` output
    // pixels per layout point.
    if let Some(window) = webview.window() {
        let backing = window.backingScaleFactor();
        if backing > 0.0 && scale > 0.0 {
            let width = webview.bounds().size.width * scale / backing;
            config.setSnapshotWidth(Some(&NSNumber::numberWithDouble(width)));
        }
    }
    let sender = Arc::clone(&sender);
    let handler = RcBlock::new(
        move |image: *mut NSImage, error: *mut objc2_foundation::NSError| {
            let result = (|| -> Result<Vec<u8>> {
                if !error.is_null() {
                    let error = &*error;
                    return Err(Error::Other(error.localizedDescription().to_string()));
                }
                if image.is_null() {
                    return Err(Error::Other("empty snapshot".to_string()));
                }
                let image: &NSImage = &*image;
                let tiff = image
                    .TIFFRepresentation()
                    .ok_or_else(|| Error::Other("no tiff representation".to_string()))?;
                let rep: Retained<NSBitmapImageRep> =
                    NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                        .ok_or_else(|| Error::Other("bad bitmap data".to_string()))?;
                let props = NSDictionary::<NSString, objc2::runtime::AnyObject>::new();
                let png = rep
                    .representationUsingType_properties(NSBitmapImageFileType::PNG, &props)
                    .ok_or_else(|| Error::Other("png encoding failed".to_string()))?;
                Ok(nsdata_to_vec(&png))
            })();
            if let Some(tx) = sender.lock().unwrap().take() {
                let _ = tx.send(result);
            }
        },
    );
    webview.takeSnapshotWithConfiguration_completionHandler(Some(&config), &handler);
}

fn nsdata_to_vec(data: &objc2_foundation::NSData) -> Vec<u8> {
    data.to_vec()
}
