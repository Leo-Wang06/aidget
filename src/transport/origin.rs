//! The origin the UI documents are served from.
//!
//! Tauri serves a custom protocol over `{scheme}://localhost` on macOS and Linux,
//! but over `http(s)://{scheme}.localhost` on Windows. Every URL, CSP entry and
//! `window.aidget` guard has to agree on that difference, so it lives here.

/// Origin for the widget shell and the main window.
pub fn origin() -> &'static str {
    #[cfg(windows)]
    {
        "http://aidget.localhost"
    }
    #[cfg(not(windows))]
    {
        "aidget://localhost"
    }
}

/// Absolute URL of the per-widget window page.
pub fn widget_window_url() -> String {
    format!("{}/widget", origin())
}

/// Absolute URL of the main window page.
pub fn main_window_url() -> String {
    format!("{}/main", origin())
}

/// The host names `protocol::handle` accepts for the custom protocol.
pub fn is_scheme_host(host: &str) -> bool {
    host == "localhost" || host == "aidget" || host == "aidget.localhost"
}
