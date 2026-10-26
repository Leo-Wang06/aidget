use aidget::transport::origin::origin;

#[test]
fn windows_serves_the_scheme_over_an_http_host() {
    if cfg!(windows) {
        assert_eq!(origin(), "http://aidget.localhost");
    } else {
        assert_eq!(origin(), "aidget://localhost");
    }
}
