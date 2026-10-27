use std::collections::{HashMap, HashSet};

use aidget::runtime::ui::{record_revision, require_main_window_label, retain_widget_revisions};

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
fn main_only_commands_reject_widget_windows() {
    assert!(require_main_window_label("main").is_ok());
    assert!(require_main_window_label("widget-weather").is_err());
}
