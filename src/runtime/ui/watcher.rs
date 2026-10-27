//! Widget file and display-topology watchers.
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::widgets::store::Store;

use super::geometry::displays;
use super::state::UiState;
use super::widget_windows::{apply_board, apply_board_forced, sync_widgets};

pub(super) fn watch(app: AppHandle, store: Store) {
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
pub(super) fn watch_topology(app: AppHandle, store: Store) {
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
