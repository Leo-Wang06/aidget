//! Serializable widget payloads and content/surface metadata.
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::widgets::model::{WidgetDragMode, WidgetPermission, WidgetSurface};
use crate::widgets::store::{Store, CONFIG_FILE, CSS_FILE, HTML_FILE};

/// Serialized state handed to a widget page after creation or content changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub id: String,
    pub revision: u64,
    pub token: String,
    /// Region that can initiate native window dragging.
    pub drag_mode: WidgetDragMode,
    pub permissions: Vec<WidgetPermission>,
    pub host_surface: bool,
    pub surface: WidgetSurface,
    pub title: String,
}

pub(super) fn signature(store: &Store, id: &str, revision: u64) -> String {
    let dir = store.widgets_dir().join(id);
    let mtime = |name: &str| {
        std::fs::metadata(dir.join(name))
            .and_then(|meta| meta.modified())
            .map(|time| {
                time.duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };
    format!(
        "{revision}:{}:{}:{}",
        mtime(HTML_FILE),
        mtime(CSS_FILE),
        mtime(CONFIG_FILE)
    )
}

/// Test seam for the content hash that busts the widget iframe cache.
pub fn content_token(signature: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in signature.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub(super) fn widget_surface(store: &Store, id: &str) -> WidgetSurface {
    store
        .widget_dir(id)
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(HTML_FILE)).ok())
        .map_or(WidgetSurface::Host, |html| html_widget_surface(&html))
}

/// Test seam for the `<meta name="aidget-surface">` opt-out check.
pub fn html_paints_own_surface(html: &str) -> bool {
    html_widget_surface(html) != WidgetSurface::Host
}

/// Read a widget's host-surface declaration from its HTML metadata.
pub fn html_widget_surface(html: &str) -> WidgetSurface {
    let lower = html.to_ascii_lowercase();
    lower
        .split('<')
        .filter(|tag| tag.starts_with("meta"))
        .map(|tag| tag.split('>').next().unwrap_or(tag))
        .find(|tag| {
            tag.contains("name=\"aidget-surface\"") || tag.contains("name='aidget-surface'")
        })
        .map_or(WidgetSurface::Host, |tag| {
            if tag.contains("content=\"glass\"") || tag.contains("content='glass'") {
                WidgetSurface::Glass
            } else if tag.contains("content=\"none\"") || tag.contains("content='none'") {
                WidgetSurface::None
            } else {
                WidgetSurface::Host
            }
        })
}
