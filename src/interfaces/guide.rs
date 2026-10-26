//! `aidget guide`: the machine-readable schema and conventions an agent needs
//! before its first write. Kept out of the CLI so that file stays about
//! argument parsing.

use serde_json::{json, Value};

use crate::common::error::Result;

/// The canonical machine-readable authoring contract shared by the CLI and MCP.
pub fn payload() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "root": "the OS home directory/.aidget (Windows: C:\\Users\\<user>\\.aidget); AIDGET_HOME overrides it",
        "layout": {
            "board": "board.json",
            "settings": "settings.toml",
            "config": "widgets/<id>/config.json",
            "html": "widgets/<id>/index.html",
            "css": "widgets/<id>/style.css",
            "agents": "widgets/<id>/agents.md",
            "assets": "widgets/<id>/assets/"
        },
        "configJson": {
            "id": "lowercase a-z0-9 and dashes, 1-64 chars",
            "title": "string",
            "dragMode": "all",
            "permissions": [],
            "network": [],
            "logo": null,
            "screenshots": [],
            "platforms": ["macos", "windows", "linux"],
            "size": {"width": 360, "height": 220},
            "revision": 1,
            "updatedAt": "RFC3339"
        },
        "regions": "Wrap replaceable areas with data-aidget-region=\"name\"; names must be unique per widget.",
        "visuals": "The window is a transparent host. The widget page paints a static fallback surface unless it declares <meta name=\"aidget-surface\" content=\"none\">. On macOS, content=\"glass\" places an AppKit visual effect behind the widget; keep the page background transparent. Avoid CSS backdrop-filter because desktop blur can flash transparent while dragging.",
        "scripts": "permissions containing script enables widget JavaScript in an opaque iframe without same-origin access. Only package-local scripts and inline script blocks already present in the served HTML are allowed; inline blocks use content hashes. Remote scripts, eval, new Function, inline event attributes and javascript: URLs are blocked; use addEventListener. With no script grant, CSP blocks widget scripts. Nested frames, objects and workers are blocked. style.css and assets/ remain local. External images, styles, fonts, media and fetch require an exact HTTPS origin in network; a network grant never authorizes remote JavaScript. HTTP and wildcard domains are refused. Call window.aidget.ready() after async initialization for screenshot settling.",
        "packages": "package.json is optional for local widgets and contains npm name/version/description/author/repository. A malformed package file is an error. No source field is stored. Community packages use @aidget-community/<id>. Local cards never display provenance badges. config.json owns portable aidget metadata; revision and updatedAt exist only on installed copies.",
        "dragging": "dragMode defaults to all: ordinary text and background move the native window after 4px of mouse movement. Controls remain interactive. Mark selectable text or custom interactions with data-aidget-no-drag, including their descendants. Use handle for the top 28px drag region. CLI create/update accepts --drag-mode all|handle; omitted updates preserve the current mode.",
        "permissions": "config.json permissions lists host capabilities the widget may use; supported values are \"script\", \"location\" and \"storage\"; every config has an explicit declaration. A declared capability reaches the widget as window.aidget. await window.aidget.requestLocation() resolves to {latitude, longitude, accuracy, place} after the operating system's location prompt. Storage is per widget: await window.aidget.storage.getItem(key), setItem(key, value), removeItem(key), and clear(); keys and values are strings, and the serialized namespace is capped at 256 KB. Missing declarations return permission_denied. Location errors also use denied, unavailable, or timeout (20s without a fix). aidget never falls back to IP geolocation.",
        "agents": "agents.md is never rendered and aidget_list does not return it. Read the agents field from aidget_get before editing, and follow it: it records which data-aidget-region holds which value, layout constraints, and refresh rules. Keep it current with aidget_update agents.",
        "time": "aidget does not recompute widget data. Use a script-enabled widget or let the agent update content; the main window recaptures enabled previews every 30 seconds while the local-widgets page is visible.",
        "position": "Widget position is user-owned. The user can drag or resize a widget at any time, which bumps boardRevision without any tool call. aidget_update and aidget_patch never change position and return no frame; do not treat a changed frame as a side effect of an edit and do not move a widget back unless the user asks.",
        "limits": { "maxWidgets": 12, "maxPatchBytes": 262144 },
        "retention": {
            "settings": "settings.toml",
            "defaultScreenshotRetention": 30,
            "defaultTrashRetention": 20
        },
        "preferences": {
            "settings": "settings.toml",
            "defaults": {
                "theme": "system",
                "language": "system",
                "tray_click_action": "window",
                "last_page": "integrations",
                "github_token": "optional; configured in General > Network, stored in plain text in settings.toml, and sent only to api.github.com"
            }
        },
        "screenshot": "scale is output pixels per layout point, so it does not depend on the display's pixel density; the default 1 renders one pixel per point. Pass 2 only when detail matters. Capture waits for the current iframe load and then up to 500ms for window.aidget.ready(). macOS captures through WKWebView, Windows through the WebView2 devtools protocol, and Linux through WebKitGTK.",
        "errors": [
            "ui_not_running", "not_found", "invalid_id", "invalid_params",
            "revision_conflict", "region_not_found", "region_ambiguous",
            "io_error", "timeout"
        ]
    })
}

pub fn print(json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload())?);
        return Ok(());
    }
    println!("aidget guide");
    println!("root      <os home>/.aidget (override with AIDGET_HOME)");
    println!("widget    widgets/<id>/{{config.json,index.html,style.css,agents.md,assets/}}");
    println!("regions   data-aidget-region=\"name\" marks replaceable inner HTML");
    println!(
        "visuals   host fallback surface by default; meta aidget-surface=none paints all pixels"
    );
    println!(
        "scripts   permissions=[script] enables JavaScript; network lists exact HTTPS origins"
    );
    println!(
        "ready     window.aidget.ready() after async initialization; screenshots wait up to 500ms"
    );
    println!("dragging  dragMode=all (default) or handle; data-aidget-no-drag preserves custom interactions");
    println!("perms     permissions=[\"location\",\"storage\"] unlocks location and storage APIs");
    println!("time      aidget never rewrites data; previews refresh every 30s while visible");
    println!("position  user-owned; drags bump boardRevision, content writes never move");
    println!("limits    12 widgets, 256 KB per patch");
    println!("retention settings.toml defaults to 30 screenshots and 20 trash directories");
    println!("prefs    settings.toml stores theme, language, tray-click behavior, last page, and an optional GitHub token");
    println!("errors    ui_not_running not_found invalid_id invalid_params");
    println!("          revision_conflict region_not_found region_ambiguous io_error timeout");
    println!("\nrun `aidget guide --json` for the machine-readable version");
    Ok(())
}
