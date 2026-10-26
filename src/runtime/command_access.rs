//! Native authorization before any command arguments or host state are read.

use super::command_names::{MAIN_COMMANDS, WIDGET_COMMANDS};
use crate::common::error::{Error, ErrorBody, Result};
use crate::widgets::model::validate_id;

/// Authorize a command using only the native window label, never caller arguments.
pub fn authorize(label: &str, command: &str) -> Result<()> {
    let allowed = if label == "main" {
        MAIN_COMMANDS.contains(&command)
    } else if let Some(id) = label.strip_prefix("widget-") {
        validate_id(id).is_ok() && WIDGET_COMMANDS.contains(&command)
    } else {
        false
    };
    if allowed {
        Ok(())
    } else {
        Err(Error::PermissionDenied(format!(
            "{command} is not allowed from window {label}"
        )))
    }
}

/// Wrap the application dispatcher at the IPC entry point.
pub fn guard<R: tauri::Runtime, F: Fn(tauri::ipc::Invoke<R>) -> bool>(
    handler: F,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool {
    move |invoke| {
        if let Err(error) = authorize(
            invoke.message.webview().window().label(),
            invoke.message.command(),
        ) {
            invoke.resolver.reject(ErrorBody::from(&error));
            return true;
        }
        handler(invoke)
    }
}
