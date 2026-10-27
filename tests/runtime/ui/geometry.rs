use aidget::runtime::ui::place_frame;
use aidget::widgets::model::WidgetFrame;

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
