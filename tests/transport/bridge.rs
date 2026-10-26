use aidget::common::error::{ConflictCurrent, Error};
use aidget::transport::bridge::Response;

#[test]
fn round_trips_error_with_current_widget() {
    let meta = aidget::widgets::model::WidgetMeta::new("note", "Note");
    let error = Error::RevisionConflict {
        current: Box::new(ConflictCurrent::Widget(meta)),
    };
    let response = Response::err("r1", &error);
    let encoded = serde_json::to_string(&response).unwrap();
    let decoded: Response = serde_json::from_str(&encoded).unwrap();
    let back = decoded.into_result().unwrap_err();
    assert_eq!(back.code(), "revision_conflict");
}

#[test]
fn encodes_success() {
    let response = Response::ok("r2", serde_json::json!({ "ok": true }));
    let decoded: Response =
        serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
    assert_eq!(decoded.into_result().unwrap()["ok"], true);
}

#[test]
fn rebuilt_errors_do_not_repeat_the_display_prefix() {
    let response = Response::err("r3", &Error::NotFound("welcome".to_string()));
    let back = response.into_result().unwrap_err();
    assert_eq!(back.to_string(), "widget not found: welcome");

    let response = Response::err("r4", &Error::InvalidParams("bad input".to_string()));
    assert_eq!(
        response.into_result().unwrap_err().to_string(),
        "invalid params: bad input"
    );
}
