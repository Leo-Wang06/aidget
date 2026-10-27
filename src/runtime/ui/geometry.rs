//! Display discovery and widget frame geometry.
use tauri::AppHandle;

use crate::common::error::Result;
use crate::widgets::model::{DisplayInfo, WidgetFrame};

/// Returns the physical position of the widget inside its display's work area.
///
/// Frames are stored relative to one display and may extend beyond its work
/// area, including into negative coordinates. That is a valid cross-display
/// position, so only a frame whose display disappeared is moved back.
pub fn physical_geometry(app: &AppHandle, frame: &WidgetFrame) -> (i32, i32, f64) {
    let monitors = app.available_monitors().unwrap_or_default();
    let primary = app.primary_monitor().ok().flatten();
    let (selected, found) = if frame.display == "primary" {
        (
            primary.clone().or_else(|| monitors.first().cloned()),
            primary.is_some() || !monitors.is_empty(),
        )
    } else {
        match monitors
            .iter()
            .enumerate()
            .find(|(index, monitor)| display_name(monitor, *index) == frame.display)
            .map(|(_, monitor)| monitor.clone())
        {
            Some(monitor) => (Some(monitor), true),
            None => (primary.clone().or_else(|| monitors.first().cloned()), false),
        }
    };

    let Some(monitor) = selected else {
        return (frame.x.round() as i32, frame.y.round() as i32, 1.0);
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (x, y) = place_frame(
        frame,
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
        scale,
        found,
    );
    (x, y, scale)
}

/// Convert a frame stored relative to `area` into a physical screen position.
/// Test seam for the display-placement arithmetic; the UI calls it directly.
pub fn place_frame(
    frame: &WidgetFrame,
    area_position: (i32, i32),
    area_size: (u32, u32),
    scale: f64,
    display_found: bool,
) -> (i32, i32) {
    let logical_width = area_size.0 as f64 / scale;
    let logical_height = area_size.1 as f64 / scale;
    let area = (logical_width, logical_height, logical_width, logical_height);
    let placed = if display_found {
        frame.fitted(area)
    } else {
        frame.clamped(area)
    };
    (
        area_position.0 + (placed.x * scale).round() as i32,
        area_position.1 + (placed.y * scale).round() as i32,
    )
}

/// List displays in the coordinate space used by widget frames.
pub fn displays(app: &AppHandle) -> Result<Vec<DisplayInfo>> {
    let monitors = app.available_monitors()?;
    let primary = app.primary_monitor().ok().flatten();
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let area = monitor.work_area();
            let scale = monitor.scale_factor();
            let name = display_name(&monitor, index);
            let primary = primary
                .as_ref()
                .is_some_and(|primary| same_monitor(primary, &monitor));
            DisplayInfo {
                primary,
                name,
                x: area.position.x as f64 / scale,
                y: area.position.y as f64 / scale,
                width: area.size.width as f64 / scale,
                height: area.size.height as f64 / scale,
                scale,
            }
        })
        .collect())
}

/// The name a frame stores for a monitor at `index`.
///
/// macOS and Windows report a device name. Wayland can report none, and an
/// unnamed monitor must not collapse into `primary`, or every multi-screen
/// frame would be restored onto one screen, so those keep the list position
/// and origin as the identifier.
pub fn display_name(monitor: &tauri::Monitor, index: usize) -> String {
    match monitor.name().filter(|name| !name.is_empty()) {
        Some(name) => name.clone(),
        None => {
            let position = monitor.position();
            format!("display-{index}@{}:{}", position.x, position.y)
        }
    }
}

/// Whether two handles describe the same physical display.
pub(super) fn same_monitor(left: &tauri::Monitor, right: &tauri::Monitor) -> bool {
    left.position() == right.position() && left.size() == right.size()
}

/// The stored name of a monitor that is not the primary display.
pub(super) fn named_display(app: &AppHandle, monitor: &tauri::Monitor) -> String {
    let index = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .position(|candidate| same_monitor(candidate, monitor))
        .unwrap_or(0);
    display_name(monitor, index)
}
