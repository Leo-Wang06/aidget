use aidget::widgets::package::{PackageManifest, WidgetConfig};

#[test]
fn initial_sizes_reject_oversized_and_nonfinite_dimensions() {
    for dimension in [8192.1, 1e308, f64::INFINITY, f64::NAN] {
        for width in [true, false] {
            let mut config = WidgetConfig::new("clock", "Clock");
            if width {
                config.size.width = dimension;
            } else {
                config.size.height = dimension;
            }
            assert!(config.validate().is_err(), "{dimension}, width={width}");
        }
    }
    let mut config = WidgetConfig::new("clock", "Clock");
    config.size.width = 8192.0;
    config.size.height = 8192.0;
    assert!(config.validate().is_ok());
}
#[test]
fn only_the_exact_community_scope_is_classified_as_community() {
    let package = |name: &str| {
        serde_json::from_value::<PackageManifest>(
            serde_json::json!({"name":name,"version":"1.0.0"}),
        )
        .unwrap()
    };
    assert!(package("@aidget-community/clock").is_community());
    assert!(!package("@aidget-community-fake/clock").is_community());
    assert!(!package("clock").is_community());
}
#[test]
fn network_declarations_are_exact_https_origins() {
    for origin in ["https://api.example.com", "https://example.com:8443"] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.network = vec![origin.into()];
        assert!(config.validate().is_ok());
    }
    for origin in [
        "https:",
        "https://*.example.com",
        "http://example.com",
        "https://example.com/api",
        "https://user@example.com",
        "https://example.com/",
        "https://example.com?x=1",
    ] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.network = vec![origin.into()];
        assert!(config.validate().is_err(), "{origin}");
    }
}
