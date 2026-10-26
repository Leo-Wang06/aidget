use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[cfg(target_os = "macos")]
use objc2_app_kit::NSWindow;
use serde::{Deserialize, Serialize};
use tauri::{
    webview::PageLoadEvent, AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::common::error::{Error, Result};
use crate::runtime::{main_startup::MainStartup, main_window_state};
use crate::settings::{
    self, GithubToken, Language, MainPage, Theme, TrayClickAction, UiPreferences,
};
use crate::transport::origin;
use crate::widgets::model::{
    validate_id, Board, DeletedWidgetSummary, DisplayInfo, WidgetDragMode, WidgetFrame,
    WidgetPermission, WidgetSurface,
};
use crate::widgets::store::{Store, CONFIG_FILE, CSS_FILE, HTML_FILE};

pub const MAIN_LABEL: &str = "main";
// Keep card previews sharp when the image is scaled down into the grid.
const MAIN_PREVIEW_SCALE: f64 = 2.0;
#[cfg(not(target_os = "macos"))]
const MAIN_STARTUP_POSITION: f64 = -10_000.0;

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

#[cfg(target_os = "macos")]
#[path = "native_surface.rs"]
mod native_surface;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// Metadata needed to render one static preview card in the main window.
pub struct WidgetSummary {
    pub id: String,
    pub title: String,
    /// Current npm identity, used only to reconcile the community install state.
    pub package_name: Option<String>,
    pub revision: u64,
    pub token: String,
    pub width: f64,
    pub height: f64,
    pub enabled: bool,
    pub created_at: u64,
}

/// Non-secret state used to identify the configured GitHub token in the main window.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GithubTokenSummary {
    pub masked: String,
}

impl From<&GithubToken> for GithubTokenSummary {
    fn from(token: &GithubToken) -> Self {
        Self {
            masked: token.masked(),
        }
    }
}

/// Current widget inventory, application version, and native preferences shown
/// by the main window.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MainState {
    pub widgets: Vec<WidgetSummary>,
    pub deleted_widgets: Vec<DeletedWidgetSummary>,
    pub broken: Vec<crate::widgets::model::BrokenWidget>,
    pub version: String,
    pub theme: Theme,
    pub language: Language,
    pub tray_click_action: TrayClickAction,
    pub last_page: MainPage,
    pub github_token: Option<GithubTokenSummary>,
}

#[derive(Default)]
pub struct UiState {
    pub windows: Mutex<HashMap<String, ()>>,
    pub signatures: Mutex<HashMap<String, String>>,
    /// Last revision whose iframe reported `load`, so a screenshot can wait
    /// for the document the caller just wrote instead of the previous one.
    pub loaded: Mutex<HashMap<String, u64>>,
    /// Last revision whose iframe called `window.aidget.ready()`.
    pub ready: Mutex<HashMap<String, u64>>,
    pub applying: Mutex<bool>,
    pub dragging: Mutex<Option<Instant>>,
    pub topology: Mutex<String>,
    pub board_revision: Mutex<u64>,
    pub root: Mutex<std::path::PathBuf>,
    pub storage_lock: Mutex<()>,
    pub startup: MainStartup,
}

fn label(id: &str) -> String {
    format!("widget-{id}")
}

/// The window label is the only authority on which widget is asking, so a page
/// cannot borrow another widget's permissions.
pub fn widget_id(window: &WebviewWindow) -> Result<String> {
    window
        .label()
        .strip_prefix("widget-")
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))
}

/// Reject commands that are only valid for the main page.
pub fn require_main_window_label(label: &str) -> Result<()> {
    if label == MAIN_LABEL {
        Ok(())
    } else {
        Err(Error::InvalidParams("main window only".to_string()))
    }
}

pub fn start(app: &AppHandle, store: Store) -> Result<()> {
    let state = UiState::default();
    *state.root.lock().unwrap() = store.root().to_path_buf();
    app.manage(state);
    if let Some(encoded) = displays(app)
        .ok()
        .and_then(|list| serde_json::to_string(&list).ok())
    {
        *app.state::<UiState>().topology.lock().unwrap() = encoded;
    }
    watch(app.clone(), store.clone());
    watch_topology(app.clone(), store);
    Ok(())
}

pub fn show_main(app: &AppHandle) -> Result<()> {
    // The bridge calls this from a worker thread; window creation must run on
    // the main thread. Queue it instead of blocking the request.
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = show_main_now(&handle) {
            eprintln!("aidget: cannot open the main window: {error}");
        }
    })
    .map_err(|error| Error::Other(error.to_string()))?;
    Ok(())
}

/// Show the main window. Must be called on the main thread.
pub fn show_main_now(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        if app.state::<UiState>().startup.is_revealed() {
            reveal_main(app, &window);
        }
        return Ok(());
    }

    app.state::<UiState>().startup.reset_reveal();
    let handle_for_load = app.clone();
    let builder = WebviewWindowBuilder::new(
        app,
        MAIN_LABEL,
        WebviewUrl::External(
            origin::main_window_url()
                .parse()
                .map_err(|error| Error::Other(format!("invalid main url: {error}")))?,
        ),
    )
    .title("aidget")
    .inner_size(1080.0, 720.0)
    .min_inner_size(860.0, 560.0)
    .center()
    .visible(false)
    .focused(false)
    .on_page_load(move |window, payload| {
        if matches!(payload.event(), PageLoadEvent::Finished) {
            handle_for_load
                .state::<UiState>()
                .startup
                .mark_page_loaded();
            if let Err(error) = prime_main(&window) {
                eprintln!("aidget: cannot prime the main window: {error}");
            }
            reveal_main_if_ready(&handle_for_load, &window);
        }
    });
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .traffic_light_position(tauri::LogicalPosition::new(24.0, 26.0))
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build()?;
    prepare_main(&window)?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = handle.get_webview_window(MAIN_LABEL) {
                hide_main(&handle, &window);
            }
        }
    });
    Ok(())
}

fn reveal_main(app: &AppHandle, window: &WebviewWindow) {
    // Widget-only starts stay out of the Dock; opening the main window makes
    // aidget behave like a regular app so the window can take focus.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    emit_main(app);
}

#[cfg(target_os = "macos")]
fn set_main_alpha(window: &WebviewWindow, alpha: f64) -> Result<()> {
    window
        .with_webview(move |webview| unsafe {
            let window: &NSWindow = &*webview.ns_window().cast();
            window.setAlphaValue(alpha);
        })
        .map_err(|error| Error::Other(format!("cannot set main window opacity: {error}")))
}

fn prepare_main(window: &WebviewWindow) -> Result<()> {
    // A hidden WebView does not run animation frames. Prime it without visible
    // content: transparent in place on macOS, offscreen where opacity is absent.
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| Error::Other(format!("cannot ignore main window cursor: {error}")))?;
    #[cfg(target_os = "macos")]
    set_main_alpha(window, 0.0)?;
    #[cfg(not(target_os = "macos"))]
    window
        .set_position(tauri::LogicalPosition::new(
            MAIN_STARTUP_POSITION,
            MAIN_STARTUP_POSITION,
        ))
        .map_err(|error| Error::Other(format!("cannot stage main window: {error}")))?;
    Ok(())
}

fn prime_main(window: &WebviewWindow) -> Result<()> {
    window
        .show()
        .map_err(|error| Error::Other(format!("cannot prime main window: {error}")))
}

fn reveal_main_if_ready(app: &AppHandle, window: &WebviewWindow) {
    if app.state::<UiState>().startup.claim_ready_reveal() {
        #[cfg(target_os = "macos")]
        let _ = set_main_alpha(window, 1.0);
        #[cfg(not(target_os = "macos"))]
        let _ = window.center();
        reveal_main(app, window);
    }
}

/// Reveal the main window only after the page has loaded and painted its first frame.
pub fn mark_main_painted(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    require_main_window_label(window.label())?;
    app.state::<UiState>().startup.mark_main_painted();
    reveal_main_if_ready(app, window);
    Ok(())
}

fn hide_main(app: &AppHandle, window: &WebviewWindow) {
    let _ = window.hide();
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub fn sync_widgets(app: &AppHandle) {
    if !app.state::<UiState>().startup.widgets_started() {
        return;
    }
    if let Err(error) = schedule_sync_widgets(app) {
        eprintln!("aidget: sync failed: {error}");
    }
}

fn schedule_sync_widgets(app: &AppHandle) -> Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = sync_widgets_inner(&handle) {
            eprintln!("aidget: sync failed: {error}");
        }
    })
    .map_err(|error| Error::Other(error.to_string()))
}

/// Start desktop widget creation once, either after the main window paints or
/// immediately for a background launch.
pub fn start_initial_widgets(app: &AppHandle) -> Result<bool> {
    let state = app.state::<UiState>();
    if !state.startup.claim_widgets_start() {
        return Ok(false);
    }
    if let Err(error) = schedule_sync_widgets(app) {
        state.startup.reset_widgets_start();
        return Err(error);
    }
    Ok(true)
}

fn sync_widgets_inner(app: &AppHandle) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    let board = store.load_board()?;
    let archived = main_window_state::archived_widgets(store.root(), &store.widget_ids()?)?;
    let ids = enabled_widget_ids(&store, &board, &archived);
    let active: HashSet<_> = ids.iter().cloned().collect();
    retain_widget_revisions(
        &mut state.loaded.lock().unwrap(),
        &mut state.ready.lock().unwrap(),
        &active,
    );
    let mut windows = state.windows.lock().unwrap();
    let mut signatures = state.signatures.lock().unwrap();
    let mut board_revision = state.board_revision.lock().unwrap();

    let existing: Vec<String> = windows.keys().cloned().collect();
    for id in existing {
        if !ids.contains(&id) {
            if let Some(window) = app.get_webview_window(&label(&id)) {
                if let Err(error) = window.close() {
                    eprintln!("aidget: cannot close {id}: {error}");
                }
            }
            windows.remove(&id);
            signatures.remove(&id);
        }
    }

    for id in ids {
        let Ok(meta) = store.load_meta(&id) else {
            continue;
        };
        let signature = signature(&store, &id, meta.revision);
        let surface = widget_surface(&store, &id);
        let widget_state = WidgetState {
            id: id.clone(),
            revision: meta.revision,
            token: content_token(&signature),
            drag_mode: meta.drag_mode,
            permissions: meta.permissions.clone(),
            host_surface: surface == WidgetSurface::Host
                || (surface == WidgetSurface::Glass && !cfg!(target_os = "macos")),
            surface,
            title: meta.title.clone(),
        };

        if let Some(window) = app.get_webview_window(&label(&id)) {
            // A previously deleted widget may have left an offscreen window
            // addressable by label; show it again when the widget returns.
            // Keep an already visible widget untouched: `show()` re-orders the
            // window as key, which makes macOS re-evaluate transparency while
            // the user is dragging.
            if !window.is_visible().unwrap_or(false) {
                let _ = window.show();
            }
            let changed = signatures.get(&id) != Some(&signature);
            if changed {
                #[cfg(target_os = "macos")]
                native_surface::apply(&window, widget_state.surface)?;

                let _ = app.emit_to(label(&id), "aidget://widget", &widget_state);
                let _ = window.eval(format!(
                    "window.__aidgetApply && window.__aidgetApply({});",
                    serde_json::to_string(&widget_state)
                        .map_err(|error| Error::Other(error.to_string()))?
                ));
                signatures.insert(id.clone(), signature);
            }
            if let Some(frame) = board.widgets.get(&id) {
                if board.revision > *board_revision && !dragging_active(&state) {
                    apply_frame(app, &window, frame)?;
                }
            }
            continue;
        }

        let Some(frame) = board.widgets.get(&id).cloned() else {
            continue;
        };
        let window = build_window(app, &id, &frame, &widget_state)?;
        apply_frame(app, &window, &frame)?;
        windows.insert(id.clone(), ());
        signatures.insert(id, signature);
    }
    *board_revision = (*board_revision).max(board.revision);
    emit_main(app);
    if state.startup.claim_widgets_announcement() {
        let _ = app.emit_to(MAIN_LABEL, "aidget://widgets-ready", ());
    }
    Ok(())
}

fn enabled_widget_ids(store: &Store, board: &Board, archived: &HashSet<String>) -> Vec<String> {
    board
        .widgets
        .keys()
        .filter(|id| !archived.contains(*id) && store.load_meta(id).is_ok())
        .cloned()
        .collect()
}

/// List every valid widget, including widgets that are currently archived.
pub fn main_widgets(store: &Store) -> Result<MainState> {
    let settings = settings::load(store.root())?;
    let preferences = settings.preferences();
    let github_token = settings.github_token.as_ref().map(GithubTokenSummary::from);
    let board = store.load_board()?;
    let archived = main_window_state::archived_widgets(store.root(), &store.widget_ids()?)?;
    let mut widgets = Vec::new();
    for (index, id) in store.widget_ids()?.into_iter().enumerate() {
        let Ok(meta) = store.load_meta(&id) else {
            continue;
        };
        let frame = board
            .widgets
            .get(&id)
            .cloned()
            .unwrap_or_else(|| WidgetFrame::default_for(index));
        let signature = signature(store, &id, meta.revision);
        widgets.push(WidgetSummary {
            package_name: store.load_package(&id)?.map(|package| package.name),
            id: meta.id.clone(),
            title: meta.title.clone(),
            revision: meta.revision,
            token: content_token(&signature),
            width: frame.w,
            height: frame.h,
            enabled: board.widgets.contains_key(&id) && !archived.contains(&id),
            created_at: store.widget_created_at_ms(&id)?,
        });
    }
    Ok(MainState {
        widgets,
        deleted_widgets: store.deleted_widgets()?,
        broken: {
            let mut broken = store.list()?.broken;
            broken.extend(crate::widgets::migration::trash_errors(store)?);
            broken
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme: preferences.theme,
        language: preferences.language,
        tray_click_action: preferences.tray_click_action.effective(),
        last_page: preferences.last_page,
        github_token,
    })
}

pub fn main_payload(app: &AppHandle) -> Result<MainState> {
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    main_widgets(&store)
}

/// Persist UI preferences and apply the primary-click tray behavior without
/// restarting. Linux rejects attempts to choose a non-menu tray action.
pub fn set_preferences(app: &AppHandle, preferences: UiPreferences) -> Result<()> {
    if cfg!(target_os = "linux") && preferences.tray_click_action != TrayClickAction::Menu {
        return Err(Error::InvalidParams(
            "changing the tray click action is unsupported on Linux".to_string(),
        ));
    }
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let previous = settings::load(store.root())?.preferences();
    let tray = app
        .tray_by_id("aidget")
        .ok_or_else(|| Error::Other("tray icon is not available".to_string()))?;
    let previous_tray = previous.tray_click_action.effective();
    let next_tray = preferences.tray_click_action.effective();
    if previous_tray != next_tray {
        tray.set_show_menu_on_left_click(next_tray == TrayClickAction::Menu)
            .map_err(|error| Error::Other(format!("cannot update tray click action: {error}")))?;
    }
    if let Err(error) = settings::save_preferences(store.root(), preferences) {
        if previous_tray != next_tray {
            let _ = tray.set_show_menu_on_left_click(previous_tray == TrayClickAction::Menu);
        }
        return Err(error);
    }
    Ok(())
}

/// Verify and persist a newly entered GitHub token, or remove the existing one.
pub fn set_github_token(
    root: &std::path::Path,
    token: Option<String>,
) -> Result<Option<GithubTokenSummary>> {
    let token = token.map(GithubToken::parse).transpose()?;
    if let Some(token) = &token {
        crate::community::service::Community::with_endpoints_and_token(
            crate::community::service::Endpoints::default(),
            Some(token.clone()),
        )?
        .verify_github_access()?;
    }
    settings::save_github_token(root, token.as_ref())?;
    Ok(token.as_ref().map(GithubTokenSummary::from))
}

/// Enable or disable a desktop widget without changing its saved position.
pub fn set_widget_enabled(app: &AppHandle, id: &str, enabled: bool) -> Result<()> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.load_meta(id)?;
    if enabled {
        main_window_state::set_archived(store.root(), &store.widget_ids()?, id, false)?;
        let board = store.load_board()?;
        if !board.widgets.contains_key(id) {
            store.move_widget(
                id,
                board.revision,
                WidgetFrame::default_for(board.widgets.len()),
            )?;
        }
    } else {
        main_window_state::set_archived(store.root(), &store.widget_ids()?, id, true)?;
    }
    sync_widgets(app);
    Ok(())
}

/// Move a widget into trash and remove its main-window preview state.
pub fn delete_widget(app: &AppHandle, id: &str) -> Result<()> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.delete(id)?;
    main_window_state::forget_widget(store.root(), &store.widget_ids()?, id)?;
    sync_widgets(app);
    Ok(())
}

/// Restore one trashed widget and recreate its desktop window.
pub fn restore_widget(app: &AppHandle, trash_name: &str) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.restore(trash_name)?;
    sync_widgets(app);
    Ok(())
}

/// Return the preview saved with a trashed widget.
pub fn deleted_widget_preview(app: &AppHandle, trash_name: &str) -> Result<Option<String>> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    let Some(png) = store.deleted_preview(trash_name)? else {
        return Ok(None);
    };
    Ok(Some(preview_data_url(png)))
}

/// Permanently remove a trashed widget.
pub fn purge_deleted_widget(app: &AppHandle, trash_name: &str) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.purge(trash_name)?;
    sync_widgets(app);
    Ok(())
}

/// Return a PNG data URL, refreshing enabled widgets and reusing archived cache.
pub async fn main_widget_preview(app: &AppHandle, id: &str) -> Result<Option<String>> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.load_meta(id)?;
    let board = store.load_board()?;
    let archived = main_window_state::archived_widgets(store.root(), &store.widget_ids()?)?;
    let png = if board.widgets.contains_key(id) && !archived.contains(id) {
        let png = screenshot(app, id, MAIN_PREVIEW_SCALE).await?;
        main_window_state::save_preview(store.root(), id, &png)?;
        png
    } else {
        let Some(png) = main_window_state::load_preview(store.root(), id)? else {
            return Ok(None);
        };
        png
    };
    Ok(Some(preview_data_url(png)))
}

/// Return the last persisted preview without capturing the widget WebView.
pub fn cached_preview(store: &Store, id: &str) -> Result<Option<String>> {
    store.load_meta(id)?;
    Ok(main_window_state::load_preview(store.root(), id)?.map(preview_data_url))
}

fn preview_data_url(png: Vec<u8>) -> String {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
    format!("data:image/png;base64,{encoded}")
}

fn emit_main(app: &AppHandle) {
    if app.get_webview_window(MAIN_LABEL).is_none() {
        return;
    }
    if let Ok(state) = main_payload(app) {
        let _ = app.emit_to(MAIN_LABEL, "aidget://main", &state);
    }
}

fn build_window(
    app: &AppHandle,
    id: &str,
    frame: &WidgetFrame,
    state: &WidgetState,
) -> Result<WebviewWindow> {
    let (x, y, scale) = physical_geometry(app, frame);
    // The page reads its state from `widget_state`, never from the URL.
    let url = origin::widget_window_url();
    let window = WebviewWindowBuilder::new(
        app,
        label(id),
        WebviewUrl::External(
            url.parse()
                .map_err(|error| Error::Other(format!("invalid widget url: {error}")))?,
        ),
    )
    .title(&state.title)
    .inner_size(frame.w, frame.h)
    .position(x as f64 / scale, y as f64 / scale)
    // Installed in every frame: the widget document itself runs inside the
    // shell's iframe. The script ignores everything outside `/content/`.
    .initialization_script_for_all_frames(crate::runtime::bridge_script::WIDGET_API)
    .initialization_script_for_all_frames(crate::runtime::bridge_script::WIDGET_DRAG)
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .resizable(false)
    // Open without taking key focus; clicking a widget still works and its
    // window stays behind normal windows.
    .focused(false)
    .always_on_bottom(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .accept_first_mouse(true)
    .build()?;

    #[cfg(target_os = "macos")]
    native_surface::apply(&window, state.surface)?;

    Ok(window)
}

pub fn apply_board(app: &AppHandle, board: &crate::widgets::model::Board) {
    apply_board_inner(app, board, false);
}

/// Re-apply every frame even when the board revision did not change.
fn apply_board_forced(app: &AppHandle, board: &crate::widgets::model::Board) {
    apply_board_inner(app, board, true);
}

fn apply_board_inner(app: &AppHandle, board: &crate::widgets::model::Board, force: bool) {
    let handle = app.clone();
    let widgets = board.widgets.clone();
    let revision = board.revision;
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<UiState>();
        // Never re-place a window while the user is dragging it. Board writes
        // triggered by that same drag would otherwise snap it back to an older
        // frame for one or two frames.
        if dragging_active(&state) {
            return;
        }
        let mut board_revision = state.board_revision.lock().unwrap();
        // A watcher may read an older board while a drag is saving. Never move
        // a window back to a snapshot we have already superseded.
        if !force && revision <= *board_revision {
            return;
        }
        let mut applying = state.applying.lock().unwrap();
        *applying = true;
        drop(applying);
        for (id, frame) in widgets {
            if let Some(window) = handle.get_webview_window(&label(&id)) {
                if let Err(error) = apply_frame(&handle, &window, &frame) {
                    eprintln!("aidget: cannot place {id}: {error}");
                }
            }
        }
        *state.applying.lock().unwrap() = false;
        *board_revision = revision;
        emit_main(&handle);
    });
}

fn apply_frame(app: &AppHandle, window: &WebviewWindow, frame: &WidgetFrame) -> Result<()> {
    let (x, y, scale) = physical_geometry(app, frame);
    let position = window.outer_position().ok();
    let size = window.inner_size().ok();
    let moved = position
        .map(|current| (current.x - x).abs() > 1 || (current.y - y).abs() > 1)
        .unwrap_or(true);
    let resized = size
        .map(|current| {
            (current.width as f64 - frame.w * scale).abs() > 1.0
                || (current.height as f64 - frame.h * scale).abs() > 1.0
        })
        .unwrap_or(true);
    if moved {
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    if scale > 0.0 && resized {
        window.set_size(PhysicalSize::new(
            (frame.w * scale).round() as u32,
            (frame.h * scale).round() as u32,
        ))?;
    }
    Ok(())
}

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
fn same_monitor(left: &tauri::Monitor, right: &tauri::Monitor) -> bool {
    left.position() == right.position() && left.size() == right.size()
}

/// The stored name of a monitor that is not the primary display.
fn named_display(app: &AppHandle, monitor: &tauri::Monitor) -> String {
    let index = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .position(|candidate| same_monitor(candidate, monitor))
        .unwrap_or(0);
    display_name(monitor, index)
}

fn signature(store: &Store, id: &str, revision: u64) -> String {
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

fn watch(app: AppHandle, store: Store) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            eprintln!("aidget: watcher unavailable: {error}");
            return;
        }
    };
    use notify::Watcher;
    if let Err(error) = watcher.watch(store.root(), notify::RecursiveMode::Recursive) {
        eprintln!("aidget: cannot watch {}: {error}", store.root().display());
        return;
    }

    std::thread::spawn(move || {
        let _keep = watcher;
        loop {
            let Ok(event) = rx.recv() else {
                return;
            };
            if !is_relevant(&event) {
                continue;
            }
            std::thread::sleep(Duration::from_millis(80));
            while let Ok(pending) = rx.try_recv() {
                let _ = pending;
            }
            let state = app.state::<UiState>();
            let applying = *state.applying.lock().unwrap();
            if applying || !state.startup.widgets_started() {
                continue;
            }
            sync_widgets(&app);
            if let Ok(board) = store.load_board() {
                apply_board(&app, &board);
            }
        }
    });
}

/// Test seam for the watcher filter that ignores screenshot and socket churn.
pub fn is_relevant(event: &notify::Result<notify::Event>) -> bool {
    use notify::EventKind;
    let Ok(event) = event else {
        return false;
    };
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        let text = path.to_string_lossy().replace('\\', "/");
        !(text.contains("/state/") || text.contains("/run/") || text.contains("/trash/"))
            && !text.ends_with(".sock")
    })
}

/// Re-apply board positions when displays are added, removed or rearranged.
/// An 8s poll is cheap and also catches most sleep/wake transitions.
fn watch_topology(app: AppHandle, store: Store) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(8));
        let handle = app.clone();
        let store = store.clone();
        let _ = app.run_on_main_thread(move || {
            let Ok(list) = displays(&handle) else {
                return;
            };
            let Ok(encoded) = serde_json::to_string(&list) else {
                return;
            };
            let state = handle.state::<UiState>();
            {
                let mut topology = state.topology.lock().unwrap();
                if *topology == encoded {
                    return;
                }
                *topology = encoded;
            }
            if let Ok(board) = store.load_board() {
                apply_board_forced(&handle, &board);
            }
        });
    });
}

pub fn widget_payload(app: &AppHandle, window: &WebviewWindow) -> Option<WidgetState> {
    let label = window.label();
    let id = label.strip_prefix("widget-")?;
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let meta = store.load_meta(id).ok()?;
    let _ = app;
    let surface = widget_surface(&store, id);
    Some(WidgetState {
        id: id.to_string(),
        revision: meta.revision,
        token: content_token(&signature(&store, id, meta.revision)),
        drag_mode: meta.drag_mode,
        permissions: meta.permissions.clone(),
        host_surface: surface == WidgetSurface::Host
            || (surface == WidgetSurface::Glass && !cfg!(target_os = "macos")),
        surface,
        title: meta.title.clone(),
    })
}

fn widget_surface(store: &Store, id: &str) -> WidgetSurface {
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

pub fn persist_position(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let state = app.state::<UiState>();
    if *state.applying.lock().unwrap() {
        return Ok(());
    }
    let id = window
        .label()
        .strip_prefix("widget-")
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))?
        .to_string();
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let _lock = store.write_lock()?;
    let mut board = store.load_board()?;
    let Some(current) = board.widgets.get(&id).cloned() else {
        return Ok(());
    };
    let position = window.outer_position()?;
    let size = window.inner_size()?;
    // Let the toolkit pick the screen that holds most of the window. Computing
    // an overlap ourselves would duplicate `NSWindow.screen` and can disagree
    // while the window is straddling the seam.
    let monitor = window
        .current_monitor()?
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let on_primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .is_some_and(|primary| same_monitor(&primary, &monitor));
    let display = if on_primary {
        "primary".to_string()
    } else {
        named_display(app, &monitor)
    };
    let frame = WidgetFrame {
        display,
        x: (position.x - area.position.x) as f64 / scale,
        y: (position.y - area.position.y) as f64 / scale,
        w: size.width as f64 / scale,
        h: size.height as f64 / scale,
    };

    if (frame.x - current.x).abs() < 1.0
        && (frame.y - current.y).abs() < 1.0
        && (frame.w - current.w).abs() < 1.0
        && (frame.h - current.h).abs() < 1.0
        && frame.display == current.display
    {
        return Ok(());
    }
    board.widgets.insert(id.clone(), frame);
    board.revision += 1;
    store.save_board(&board)?;
    *app.state::<UiState>().board_revision.lock().unwrap() = board.revision;
    emit_main(app);
    Ok(())
}

pub fn begin_drag(app: &AppHandle) {
    *app.state::<UiState>().dragging.lock().unwrap() = Some(Instant::now());
}

pub fn end_drag(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    *app.state::<UiState>().dragging.lock().unwrap() = None;
    persist_position(app, window)
}

/// Push or pop the native grabbing cursor around a macOS window drag.
#[cfg(target_os = "macos")]
pub fn set_dragging_cursor(app: &AppHandle, grabbing: bool) -> Result<()> {
    run_on_main_thread(app, move || {
        if grabbing {
            objc2_app_kit::NSCursor::closedHandCursor().push();
        } else {
            objc2_app_kit::NSCursor::pop_class();
        }
    })
}

/// Wait until the user releases the mouse button that started a native drag.
#[cfg(target_os = "macos")]
pub async fn wait_for_drag_release(app: &AppHandle) -> Result<()> {
    loop {
        let pressed =
            run_on_main_thread(app, || objc2_app_kit::NSEvent::pressedMouseButtons() != 0)?;
        if !pressed {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}

#[cfg(target_os = "macos")]
fn run_on_main_thread<T: Send + 'static>(
    app: &AppHandle,
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(task());
    })?;
    receiver
        .recv()
        .map_err(|_| Error::Other("main thread task was dropped".to_string()))
}

/// A drag event can be lost when the drag ends over another app. Expire the
/// freeze so geometry never stays locked.
fn dragging_active(state: &UiState) -> bool {
    state
        .dragging
        .lock()
        .unwrap()
        .is_some_and(|started| started.elapsed() < Duration::from_secs(30))
}

/// Drop load and ready receipts for closed widgets so a recreated window cannot
/// satisfy the next screenshot wait with state from its predecessor.
pub fn retain_widget_revisions(
    loaded: &mut HashMap<String, u64>,
    ready: &mut HashMap<String, u64>,
    active: &HashSet<String>,
) {
    loaded.retain(|id, _| active.contains(id));
    ready.retain(|id, _| active.contains(id));
}

/// Raise the receipt for `id` to `revision`, never letting it move backwards.
pub fn record_revision(revisions: &mut HashMap<String, u64>, id: &str, revision: u64) {
    let current = revisions.entry(id.to_string()).or_default();
    *current = (*current).max(revision);
}

pub async fn screenshot(app: &AppHandle, id: &str, scale: f64) -> Result<Vec<u8>> {
    let expected = Store::new(app.state::<UiState>().root.lock().unwrap().clone())
        .load_meta(id)?
        .revision;
    if wait_loaded(app, id, expected, Duration::from_secs(3)).await {
        wait_ready(app, id, expected, Duration::from_millis(500)).await;
    }
    let window = app
        .get_webview_window(&label(id))
        .ok_or_else(|| Error::NotFound(id.to_string()))?;
    // The webview runs on the main thread, so the viewport is read here and
    // handed to the capture as a size in layout points.
    let viewport = window
        .inner_size()
        .and_then(|size| window.scale_factor().map(|factor| (size, factor)))
        .map(|(size, factor)| {
            (
                f64::from(size.width) / factor,
                f64::from(size.height) / factor,
            )
        })
        .unwrap_or((0.0, 0.0));
    #[cfg(target_os = "macos")]
    let _ = viewport;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(tx)));
    let sender_for_cb = Arc::clone(&sender);
    window
        .with_webview(move |platform| {
            let sender = Arc::clone(&sender_for_cb);
            #[cfg(target_os = "macos")]
            unsafe {
                capture_wkwebview(platform.inner(), sender, scale);
            }
            #[cfg(windows)]
            unsafe {
                capture_webview2(platform, sender, scale, viewport);
            }
            #[cfg(target_os = "linux")]
            capture_webkitgtk(platform.inner(), sender, scale, viewport);
            #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
            {
                let _ = (platform, scale, viewport);
                send_failure(&sender, unsupported_capture());
            }
        })
        .map_err(|error| Error::Other(error.to_string()))?;

    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Error::Other("screenshot channel closed".to_string())),
        Err(_) => Err(Error::Timeout),
    }
}

/// The page reports every iframe `load`, so a snapshot taken right after a
/// write does not capture the previous document. Best-effort: a widget whose
/// page never loads still gets its screenshot after the timeout.
async fn wait_loaded(app: &AppHandle, id: &str, revision: u64, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let loaded = app
            .state::<UiState>()
            .loaded
            .lock()
            .unwrap()
            .get(id)
            .copied()
            .unwrap_or(0);
        if loaded >= revision {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// Wait briefly for `window.aidget.ready()` without making it mandatory.
async fn wait_ready(app: &AppHandle, id: &str, revision: u64, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let ready = app
            .state::<UiState>()
            .ready
            .lock()
            .unwrap()
            .get(id)
            .copied()
            .unwrap_or(0);
        if ready >= revision || Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// Called by the widget page once its iframe finishes loading `revision`.
pub fn mark_loaded(app: &AppHandle, window: &WebviewWindow, revision: u64) -> Result<()> {
    let id = window
        .label()
        .strip_prefix("widget-")
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))?
        .to_string();
    record_revision(
        &mut app.state::<UiState>().loaded.lock().unwrap(),
        &id,
        revision,
    );
    Ok(())
}

/// Called when a widget document reports that its initialization is complete.
pub fn mark_ready(app: &AppHandle, window: &WebviewWindow, revision: u64) -> Result<()> {
    if revision == 0 {
        return Err(Error::InvalidParams(
            "ready revision must be greater than zero".to_string(),
        ));
    }
    let id = widget_id(window)?;
    record_revision(
        &mut app.state::<UiState>().ready.lock().unwrap(),
        &id,
        revision,
    );
    Ok(())
}

/// The error a screenshot returns on targets without a native webview capture.
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn unsupported_capture() -> Error {
    Error::Other("screenshot capture is unsupported on this platform".to_string())
}

/// Answers a snapshot request with an error.
#[cfg(not(target_os = "macos"))]
fn send_failure(sender: &SnapshotSender, error: Error) {
    if let Some(tx) = take_sender(sender) {
        let _ = tx.send(Err(error));
    }
}

/// Takes the waiting snapshot sender.
///
/// The mutex guard has to die before the caller's `Arc` does, so the take
/// happens in its own statement.
#[cfg(not(target_os = "macos"))]
fn take_sender(sender: &SnapshotSender) -> Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>> {
    sender.lock().unwrap().take()
}

type SnapshotSender = Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<Vec<u8>>>>>>;

/// PNG of the WebView2 surface at `scale` output pixels per layout point.
///
/// `CapturePreview` copies the current viewport at its native density and takes
/// no clip, so it cannot honour `scale`. DevTools `Page.captureScreenshot` does:
/// its clip is in CSS pixels and `clip.scale` is pixels per CSS pixel, which is
/// exactly the meaning aidget gives `scale`.
#[cfg(windows)]
unsafe fn capture_webview2(
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
#[cfg(windows)]
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

/// Captures the visible WebKitGTK viewport and encodes it at the requested scale.
#[cfg(target_os = "linux")]
fn capture_webkitgtk(
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

#[cfg(target_os = "macos")]
unsafe fn capture_wkwebview(webview: *mut std::ffi::c_void, sender: SnapshotSender, scale: f64) {
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

#[cfg(target_os = "macos")]
fn nsdata_to_vec(data: &objc2_foundation::NSData) -> Vec<u8> {
    data.to_vec()
}
