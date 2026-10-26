use aidget::community::{model::FileDigest, snapshot::validate_files};

#[test]
fn unsafe_snapshot_manifests_fail_before_any_download() {
    let file = FileDigest {
        path: "index.html".into(),
        size: 1,
        sha256: "a".repeat(64),
    };
    assert!(validate_files(std::slice::from_ref(&file)).is_ok());
    let mut alias = file.clone();
    alias.path = "INDEX.html".into();
    assert!(validate_files(&[file.clone(), alias]).is_err());
    let mut traversal = file.clone();
    traversal.path = "../escape".into();
    assert!(validate_files(&[traversal]).is_err());
    let mut bomb = file.clone();
    bomb.size = 100 * 1024 * 1024 + 1;
    assert!(validate_files(&[bomb]).is_err());
    assert!(validate_files(&vec![file; 1001]).is_err());
}
