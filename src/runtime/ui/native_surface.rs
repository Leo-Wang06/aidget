use std::ptr::NonNull;

use objc2_app_kit::NSView;
use tauri::WebviewWindow;
use window_vibrancy::{
    apply_liquid_glass, apply_vibrancy, clear_liquid_glass, clear_vibrancy, LiquidGlassOptions,
    NSGlassEffectViewStyle, NSVisualEffectMaterial, NSVisualEffectState,
};

use crate::common::error::{Error, Result};
use crate::widgets::model::WidgetSurface;

const CORNER_RADIUS: f64 = 28.0;

/// Put the strongest native glass material available behind a glass widget.
pub fn apply(window: &WebviewWindow, surface: WidgetSurface) -> Result<()> {
    let window = window.clone();
    let window_for_apply = window.clone();
    window
        .with_webview(move |webview| {
            let raw_webview = webview.inner().cast::<NSView>();
            if let Err(error) = apply_inner(&window_for_apply, raw_webview, surface) {
                eprintln!("aidget: native surface failed: {error}");
            }
        })
        .map_err(|error| Error::Other(format!("cannot access widget webview: {error}")))
}

fn apply_inner(
    window: &WebviewWindow,
    raw_webview: *mut NSView,
    surface: WidgetSurface,
) -> Result<()> {
    let webview = NonNull::new(raw_webview)
        .ok_or_else(|| Error::Other("widget WKWebView is null".to_string()))?;
    // `with_webview` invokes this closure on the AppKit main thread.
    let webview = unsafe { webview.as_ref() };

    let _ = clear_liquid_glass(window);
    let _ = clear_vibrancy(window);

    if surface != WidgetSurface::Glass {
        return Ok(());
    }

    let options = LiquidGlassOptions::new(NSGlassEffectViewStyle::Clear)
        .radius(CORNER_RADIUS)
        .opaque(false)
        .interactive(false)
        .content_view(webview);

    match apply_liquid_glass(window, options) {
        Ok(()) => Ok(()),
        Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => apply_vibrancy(
            window,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(CORNER_RADIUS),
        )
        .map_err(|error| Error::Other(format!("cannot apply fallback vibrancy: {error}"))),
        Err(error) => Err(Error::Other(format!("cannot apply liquid glass: {error}"))),
    }
}
