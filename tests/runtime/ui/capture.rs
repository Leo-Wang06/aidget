use aidget::runtime::ui::{screenshot_mode, ScreenshotMode};

#[test]
fn panel_previews_do_not_request_viewport_scaling() {
    assert_eq!(screenshot_mode(true, 2.0), ScreenshotMode::PanelPreview);
}

#[test]
fn explicit_screenshots_keep_the_requested_scale() {
    assert_eq!(
        screenshot_mode(false, 3.0),
        ScreenshotMode::ViewportScaled(3.0)
    );
}
