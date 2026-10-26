use std::collections::{HashMap, HashSet};

use aidget::runtime::main_window_state;
use aidget::runtime::ui::{
    cached_preview, content_token, html_widget_surface, is_relevant, main_widgets, place_frame,
    record_revision, require_main_window_label, retain_widget_revisions, MainState, WidgetSummary,
};
use aidget::settings::{
    save_preferences, Language, MainPage, Theme, TrayClickAction, UiPreferences,
};
use aidget::widgets::model::{DeletedWidgetSummary, WidgetFrame, WidgetSurface};
use aidget::widgets::store::{Store, WidgetEdit, CONFIG_FILE};
use tempfile::TempDir;

#[test]
fn content_token_tracks_signature_changes() {
    assert_ne!(content_token("1:100"), content_token("1:101"));
    assert_eq!(content_token("1:100"), content_token("1:100"));
}

#[test]
fn ignores_state_and_run_events() {
    use notify::event::{AccessKind, AccessMode, CreateKind, Event, EventKind};
    let state = Event::new(EventKind::Create(CreateKind::File)).add_path(std::path::PathBuf::from(
        "/tmp/aidget/state/screenshots/a.png",
    ));
    assert!(!is_relevant(&Ok(state)));

    let widget = Event::new(EventKind::Create(CreateKind::File))
        .add_path(std::path::PathBuf::from("/tmp/aidget/widgets/a/index.html"));
    assert!(is_relevant(&Ok(widget)));

    let access = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
        .add_path(std::path::PathBuf::from("/tmp/aidget/widgets/a/index.html"));
    assert!(!is_relevant(&Ok(access)));
}

#[test]
fn main_window_lists_all_widgets_and_marks_enabled_state() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "zeta",
            "Zeta",
            WidgetEdit {
                html: Some("<p>z</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "alpha",
            "Alpha",
            WidgetEdit {
                html: Some("<p>a</p>"),
                css: Some(""),
                permissions: Some(&[aidget::widgets::model::WidgetPermission::Script]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "hidden",
            "Hidden",
            WidgetEdit {
                html: Some("<p>h</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let mut board = store.load_board().unwrap();
    board.widgets.remove("hidden");
    board.widgets.insert(
        "alpha".to_string(),
        WidgetFrame {
            display: "primary".to_string(),
            x: 10.0,
            y: 20.0,
            w: 420.0,
            h: 260.0,
        },
    );
    store.save_board(&board).unwrap();

    let state = main_widgets(&store).unwrap();
    let ids: Vec<_> = state
        .widgets
        .iter()
        .map(|widget| widget.id.as_str())
        .collect();
    assert_eq!(ids, vec!["alpha", "hidden", "zeta"]);
    assert_eq!(state.widgets[0].title, "Alpha");
    assert_eq!(state.widgets[0].width, 420.0);
    assert_eq!(state.widgets[0].height, 260.0);
    assert!(!state.widgets[0].token.is_empty());
    assert!(state.widgets[0].enabled);
    assert!(!state.widgets[1].enabled);
    assert_eq!(state.widgets[1].width, 360.0);
    assert_eq!(state.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn main_state_restores_last_page_from_settings() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();

    save_preferences(
        store.root(),
        UiPreferences {
            last_page: MainPage::About,
            ..UiPreferences::default()
        },
    )
    .unwrap();

    assert_eq!(main_widgets(&store).unwrap().last_page, MainPage::About);
}

#[test]
fn archiving_a_widget_keeps_its_board_position_for_restore() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "weather",
            "Weather",
            WidgetEdit {
                html: Some("<p>sunny</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let original = store.load_board().unwrap().widgets["weather"].clone();

    let ids = store.widget_ids().unwrap();
    main_window_state::set_archived(store.root(), &ids, "weather", true).unwrap();
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["weather"]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(!main_widgets(&store).unwrap().widgets[0].enabled);

    main_window_state::set_archived(store.root(), &ids, "weather", false).unwrap();
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["weather"]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(main_widgets(&store).unwrap().widgets[0].enabled);
}

#[test]
fn main_window_skips_widgets_with_broken_config() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "good",
            "Good",
            WidgetEdit {
                html: Some("<p>ok</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "broken",
            "Broken",
            WidgetEdit {
                html: Some("<p>bad</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    std::fs::remove_file(store.widget_dir("broken").unwrap().join(CONFIG_FILE)).unwrap();

    let state = main_widgets(&store).unwrap();
    assert_eq!(state.widgets.len(), 1);
    assert_eq!(state.widgets[0].id, "good");
}

#[test]
fn cross_display_frame_is_not_pulled_back() {
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: 0.0,
        y: -100.0,
        w: 360.0,
        h: 220.0,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, true);
    assert_eq!((x, y), (0, -200));
}

#[test]
fn negative_offsets_are_kept_on_a_live_display() {
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: -1200.0,
        y: -3000.0,
        w: 360.0,
        h: 220.0,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, true);
    assert_eq!((x, y), (-2400, -6000));
}

#[test]
fn missing_display_frame_is_clamped_into_fallback() {
    let frame = WidgetFrame {
        display: "Monitor #999".to_string(),
        x: 5000.0,
        y: 80.0,
        w: 360.0,
        h: 220.0,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, false);
    assert_eq!((x, y), ((3024.0 - 720.0) as i32, 160));
}

#[test]
fn widget_declares_its_card_surface() {
    assert_eq!(
        html_widget_surface(r#"<head><meta name="aidget-surface" content="none" /></head>"#),
        WidgetSurface::None
    );
    assert_eq!(
        html_widget_surface(r#"<HEAD><META CONTENT="none" NAME="aidget-surface"></HEAD>"#),
        WidgetSurface::None
    );
    assert_eq!(
        html_widget_surface(r#"<head><meta name="aidget-surface" content="glass" /></head>"#),
        WidgetSurface::Glass
    );
    assert_eq!(
        html_widget_surface(r#"<meta name="aidget-surface" content="host" />"#),
        WidgetSurface::Host
    );
    assert_eq!(
        html_widget_surface("<head><title>plain</title></head>"),
        WidgetSurface::Host
    );
}

#[test]
fn stale_revisions_are_removed_after_a_widget_is_archived() {
    let mut loaded = HashMap::from([("weather".to_string(), 3)]);
    let mut ready = HashMap::from([("weather".to_string(), 3)]);

    retain_widget_revisions(&mut loaded, &mut ready, &HashSet::new());

    assert!(!loaded.contains_key("weather"));
    assert!(!ready.contains_key("weather"));
}

#[test]
fn revision_receipts_never_move_backwards() {
    let mut ready = HashMap::new();
    record_revision(&mut ready, "weather", 3);
    record_revision(&mut ready, "weather", 5);
    record_revision(&mut ready, "weather", 4);

    assert_eq!(ready["weather"], 5);
}

#[test]
fn main_state_exposes_widget_metadata_in_camel_case() {
    let value = serde_json::to_value(MainState {
        widgets: vec![WidgetSummary {
            package_name: None,
            id: "weather".to_string(),
            title: "Weather".to_string(),
            revision: 2,
            token: "token".to_string(),
            width: 360.0,
            height: 220.0,
            enabled: true,
            created_at: 1_761_158_300_000,
        }],
        broken: vec![],
        deleted_widgets: vec![DeletedWidgetSummary {
            trash_name: "note-20261022T000000".to_string(),
            id: "note".to_string(),
            title: "Note".to_string(),
            width: 360.0,
            height: 220.0,
            has_preview: true,
            created_at: 1_761_158_400_000,
        }],
        version: "0.1.0".to_string(),
        theme: Theme::Dark,
        language: Language::ZhCn,
        tray_click_action: TrayClickAction::Window,
        last_page: MainPage::Integrations,
    })
    .unwrap();

    let deleted = value.get("deletedWidgets").unwrap();
    assert_eq!(value["widgets"][0]["createdAt"], 1_761_158_300_000_u64);
    assert_eq!(deleted[0]["trashName"], "note-20261022T000000");
    assert_eq!(deleted[0]["hasPreview"], true);
    assert_eq!(deleted[0]["createdAt"], 1_761_158_400_000_u64);
    assert!(deleted[0].get("trash_name").is_none());
    assert!(value.get("deleted_widgets").is_none());
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["language"], "zh-CN");
    assert_eq!(value["trayClickAction"], "window");
    assert_eq!(value["lastPage"], "integrations");
}

#[test]
fn cached_preview_reads_the_saved_png_without_a_webview() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();

    assert_eq!(cached_preview(&store, "note").unwrap(), None);

    main_window_state::save_preview(store.root(), "note", b"png bytes").unwrap();
    assert_eq!(
        cached_preview(&store, "note").unwrap().unwrap(),
        "data:image/png;base64,cG5nIGJ5dGVz"
    );
}

#[test]
fn cached_preview_rejects_unknown_and_invalid_widgets() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();

    assert!(cached_preview(&store, "missing").is_err());
    assert!(cached_preview(&store, "../outside").is_err());
}

#[test]
fn main_only_commands_reject_widget_windows() {
    assert!(require_main_window_label("main").is_ok());
    assert!(require_main_window_label("widget-weather").is_err());
}

#[test]
fn main_state_tracks_current_package_identity_without_a_source_badge() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    assert_eq!(main_widgets(&store).unwrap().widgets[0].package_name, None);
    std::fs::write(
        store.widget_dir("note").unwrap().join("package.json"),
        br#"{"name":"@someone/note","version":"1.0.0"}"#,
    )
    .unwrap();
    let state = main_widgets(&store).unwrap();
    assert_eq!(
        state.widgets[0].package_name.as_deref(),
        Some("@someone/note")
    );
    let value = serde_json::to_value(state).unwrap();
    assert!(value["widgets"][0].get("source").is_none());
}
