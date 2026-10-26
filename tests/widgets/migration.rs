use aidget::widgets::{migration, model::WidgetPermission, store::Store};
#[test]
fn migration_backs_up_preserves_revision_and_does_not_invent_a_package() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    let widget = store.widget_dir("old").unwrap();
    std::fs::create_dir_all(&widget).unwrap();
    let bytes = br#"{"id":"old","title":"Old","version":"0.1.0","allowScripts":true,"permissions":["storage"],"revision":7,"updatedAt":"2026-10-25T00:00:00Z"}"#;
    std::fs::write(widget.join("config.json"), bytes).unwrap();
    assert!(migration::run(&store).unwrap().is_empty());
    let meta = store.load_meta("old").unwrap();
    assert_eq!(meta.revision, 7);
    assert!(meta.grants(WidgetPermission::Script));
    assert!(meta.grants(WidgetPermission::Storage));
    assert!(meta.network.is_empty());
    assert!(store.load_package("old").unwrap().is_none());
    assert_eq!(
        std::fs::read(
            root.path()
                .join("backups/widget-metadata/widgets/old/config.json")
        )
        .unwrap(),
        bytes
    );
    assert!(migration::run(&store).unwrap().is_empty());
}
#[test]
fn one_broken_migration_does_not_prevent_other_widgets_from_loading() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    store.create("good", "Good", Default::default()).unwrap();
    std::fs::create_dir_all(store.widget_dir("bad").unwrap()).unwrap();
    std::fs::write(store.widget_dir("bad").unwrap().join("config.json"), b"{").unwrap();
    let failures = migration::run(&store).unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].id, "bad");
    assert!(store.load_meta("good").is_ok());
}
