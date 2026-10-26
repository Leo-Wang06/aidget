use aidget::{
    transport::csp::for_document,
    widgets::{model::WidgetPermission, package::WidgetConfig},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};

#[test]
fn hashes_preserve_script_text_and_normalize_html_line_endings_and_nuls() {
    let mut config = WidgetConfig::new("note", "Note");
    config.permissions = vec![WidgetPermission::Script];
    let script = "\nwindow.a = '&amp; 文';\n// comment\n\u{fffd}";
    let input = format!(
        "<script>{}</script>",
        script.replace('\n', "\r\n").replace('\u{fffd}', "\0")
    );
    let policy = for_document(&config, "http://aidget.localhost", Some(input.as_bytes())).unwrap();
    let expected = STANDARD.encode(Sha256::digest(script.as_bytes()));
    assert!(policy.contains(&format!("'sha256-{expected}'")));
    let input = format!("<script>{}</script>", script.replace('\n', "\r"));
    assert_eq!(
        policy,
        for_document(&config, "http://aidget.localhost", Some(input.as_bytes())).unwrap()
    );
}

#[test]
fn external_scripts_data_blocks_and_parser_chunking_cannot_merge_script_grants() {
    let mut config = WidgetConfig::new("note", "Note");
    config.permissions = vec![WidgetPermission::Script];
    let script = format!("window.data = '{}';", "中文<&".repeat(5000));
    let html = format!("<!-- <script>unparsed()</script> --><script>{script}</script><script src='assets/app.js'>ignored()</script><script>second()</script><script>second()</script>");
    let policy = for_document(&config, "aidget://localhost", Some(html.as_bytes())).unwrap();
    for script in [&script, "second()"] {
        assert!(policy.contains(&STANDARD.encode(Sha256::digest(script.as_bytes()))));
    }
    for script in ["unparsed()", "ignored()"] {
        assert!(!policy.contains(&STANDARD.encode(Sha256::digest(script.as_bytes()))));
    }
    assert_eq!(policy.matches("'sha256-").count(), 2);
}

#[test]
fn static_widgets_and_non_html_documents_never_receive_inline_script_grants() {
    let mut config = WidgetConfig::new("note", "Note");
    let policy = for_document(
        &config,
        "aidget://localhost",
        Some(b"<script>bad()</script>"),
    )
    .unwrap();
    assert!(policy.contains("script-src 'none'"));
    assert!(!policy.contains("sha256-"));
    config.permissions = vec![WidgetPermission::Script];
    let policy = for_document(&config, "aidget://localhost", None).unwrap();
    assert!(!policy.contains("sha256-"));
    assert!(for_document(&config, "aidget://localhost", Some(&[0xff])).is_err());
}
