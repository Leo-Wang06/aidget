use aidget::runtime::ui::{content_token, html_widget_surface};
use aidget::widgets::model::WidgetSurface;

#[test]
fn content_token_tracks_signature_changes() {
    assert_ne!(content_token("1:100"), content_token("1:101"));
    assert_eq!(content_token("1:100"), content_token("1:100"));
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
