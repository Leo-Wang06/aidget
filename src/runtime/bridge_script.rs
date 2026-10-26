//! The `window.aidget` API injected into every widget document.
//!
//! Installed in all frames because the widget document lives inside the
//! shell's iframe; the script's own `/content/` guard keeps it out of the
//! shell page and the main-window preview. Requests reach the shell through
//! `postMessage` and are answered by the shell over Tauri IPC. The Windows
//! subframe-initialization limitation is documented in SECURITY.md.

/// Injected with `initialization_script_for_all_frames`.
pub const WIDGET_API: &str = include_str!("../../ui/src/widget/api.js");

/// Host-owned pointer bridge, also installed when a widget's own scripts are disabled.
pub const WIDGET_DRAG: &str = include_str!("../../ui/src/widget/drag.js");
