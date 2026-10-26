use aidget::{
    transport::server::protocol::{embedded_cache_control, serve_widget, widget_csp_for},
    widgets::{
        model::WidgetPermission,
        package::WidgetConfig,
        store::{Store, WidgetEdit},
    },
};
#[test]
fn every_resource_type_is_bounded_by_the_declared_network_list() {
    let mut config = WidgetConfig::new("note", "Note");
    let policy = widget_csp_for(&config, "http://aidget.localhost");
    assert!(policy.contains("connect-src 'none'"));
    assert!(policy.contains("script-src 'none'"));
    assert!(!policy.contains("https:"));
    config.permissions.push(WidgetPermission::Script);
    config.network.push("https://api.example.com".into());
    let policy = widget_csp_for(&config, "http://aidget.localhost");
    for directive in [
        "script-src",
        "style-src",
        "img-src",
        "font-src",
        "media-src",
        "connect-src",
    ] {
        let value = policy
            .split(';')
            .find(|s| s.trim().starts_with(directive))
            .unwrap();
        assert!(value.contains("https://api.example.com"));
    }
    assert!(policy.contains("http://aidget.localhost/content/note/"));
    assert!(!policy.contains("connect-src http:"));
}
#[test]
fn widget_files_never_expose_package_or_another_widget() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    std::fs::write(
        store.widget_dir("note").unwrap().join("assets/evil.svg"),
        b"<svg/>",
    )
    .unwrap();
    let response = serve_widget(&store, "note/assets/evil.svg");
    assert_eq!(response.status(), 200);
    assert!(response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("script-src 'none'"));
    for path in [
        "note/config.json",
        "note/package.json",
        "note/../other/index.html",
        "note/assets/%2e%2e/config.json",
    ] {
        assert_eq!(serve_widget(&store, path).status(), 404, "{path}");
    }
}
#[test]
fn embedded_html_revalidates_while_hashed_assets_are_immutable() {
    assert_eq!(embedded_cache_control("main.html"), "no-cache");
    assert_eq!(
        embedded_cache_control("assets/main-abc.js"),
        "public, max-age=31536000, immutable"
    );
}
