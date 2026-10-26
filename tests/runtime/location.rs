use aidget::runtime::location::{authorize, LocationError};
use aidget::widgets::model::WidgetPermission;
use aidget::widgets::store::{Store, WidgetEdit};

use tempfile::TempDir;

fn store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    (dir, store)
}

#[test]
fn authorize_reads_the_widget_config() {
    let (_dir, store) = store();
    store
        .create(
            "weather",
            "Weather",
            WidgetEdit {
                html: Some("<p></p>"),
                css: Some(""),
                permissions: Some(&[aidget::widgets::model::WidgetPermission::Script]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        authorize(&store, "weather").unwrap_err().code(),
        "permission_denied"
    );

    store
        .update(
            "weather",
            1,
            WidgetEdit {
                permissions: Some(&[WidgetPermission::Location]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert!(authorize(&store, "weather").is_ok());

    // Clearing the declaration closes the gate again on the next request.
    store
        .update(
            "weather",
            2,
            WidgetEdit {
                permissions: Some(&[]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        authorize(&store, "weather").unwrap_err().code(),
        "permission_denied"
    );
}

#[test]
fn error_codes_are_stable_for_the_widget_api() {
    assert_eq!(LocationError::PermissionDenied.code(), "permission_denied");
    assert_eq!(LocationError::Denied.code(), "denied");
    assert_eq!(LocationError::Unavailable.code(), "unavailable");
    assert_eq!(LocationError::Timeout.code(), "timeout");
}

#[test]
fn unknown_widget_cannot_borrow_a_permission() {
    let (_dir, store) = store();
    assert_eq!(
        authorize(&store, "weather").unwrap_err().code(),
        "not_found"
    );
}
