use aidget::widgets::store::Store;
use aidget::widgets::template::{bootstrap_welcome, install, list, metadata};

use tempfile::TempDir;

#[test]
fn builtin_welcome_is_listed() {
    let templates = list().unwrap();
    assert!(templates.iter().any(|item| item.id == "welcome"));
}

#[test]
fn bundled_metadata_matches_the_reviewed_registry() {
    let registry: aidget::community::model::Registry = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/widgets/registry.json"
        ))
        .unwrap(),
    )
    .unwrap();
    for template in list().unwrap() {
        let entry = registry
            .widgets
            .iter()
            .find(|entry| entry.config.id == template.id)
            .unwrap();
        let (config, package) = metadata(&template.id).unwrap();
        assert_eq!(config, entry.config, "{} config", template.id);
        assert_eq!(package, entry.package, "{} package", template.id);
    }
}

#[test]
fn bootstrap_installs_once() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    bootstrap_welcome(&store).unwrap();
    assert!(store
        .widget_dir("welcome")
        .unwrap()
        .join(aidget::widgets::store::HTML_FILE)
        .is_file());
    bootstrap_welcome(&store).unwrap();
    assert_eq!(store.widget_ids().unwrap(), vec!["welcome".to_string()]);
}

#[test]
fn install_rejects_existing_id() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    install(&store, "welcome", "welcome").unwrap();
    assert!(install(&store, "welcome", "welcome").is_err());
}

#[test]
fn install_copies_agents_and_package() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    let meta = install(&store, "welcome", "hello").unwrap();
    assert_eq!(meta.drag_mode, aidget::widgets::model::WidgetDragMode::All);
    assert_eq!(
        store.load_package("hello").unwrap().unwrap().name,
        "@aidget-community/welcome"
    );
    assert!(store
        .widget_dir("hello")
        .unwrap()
        .join("agents.md")
        .is_file());
}
