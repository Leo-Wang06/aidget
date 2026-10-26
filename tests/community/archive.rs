use aidget::community::archive::{extract, verify_integrity};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha512};
use std::io::Write;
fn pack(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (path, bytes, kind) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(*kind);
        header.set_path(path).unwrap();
        if kind.is_symlink() || kind.is_hard_link() {
            header.set_link_name("/tmp/escape").unwrap();
        }
        header.set_cksum();
        builder.append(&header, *bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}
#[test]
fn integrity_is_checked_before_extraction() {
    let bytes = b"archive";
    let integrity = format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)));
    verify_integrity(bytes, &integrity).unwrap();
    assert!(verify_integrity(b"different", &integrity).is_err());
}
#[test]
fn rejects_links_duplicate_paths_and_platform_independent_traversal() {
    for entries in [
        vec![(
            "package/assets/link",
            b"".as_slice(),
            tar::EntryType::Symlink,
        )],
        vec![("package/assets/link", b"".as_slice(), tar::EntryType::Link)],
        vec![
            (
                "package/index.html",
                b"a".as_slice(),
                tar::EntryType::Regular,
            ),
            (
                "package/index.html",
                b"b".as_slice(),
                tar::EntryType::Regular,
            ),
        ],
        vec![(
            "package/assets/C:escape",
            b"a".as_slice(),
            tar::EntryType::Regular,
        )],
        vec![(
            "package/assets/a\\..\\escape",
            b"a".as_slice(),
            tar::EntryType::Regular,
        )],
    ] {
        let directory = tempfile::tempdir().unwrap();
        let error = extract(&pack(&entries), directory.path(), None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unsupported or duplicate") || error.contains("unsafe widget path"),
            "{error}"
        );
    }
}
#[test]
fn enforces_the_entry_limit_even_for_empty_files() {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for index in 0..1001 {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("package/assets/{index}"),
                b"".as_slice(),
            )
            .unwrap();
    }
    let bytes = builder.into_inner().unwrap().finish().unwrap();
    let directory = tempfile::tempdir().unwrap();
    assert!(extract(&bytes, directory.path(), None)
        .unwrap_err()
        .to_string()
        .contains("1000"));
}
#[test]
fn rejects_decompression_bombs_before_writing_the_oversized_file() {
    let mut header = tar::Header::new_gnu();
    header.set_path("package/assets/huge").unwrap();
    header.set_mode(0o644);
    header.set_size(101 * 1024 * 1024);
    header.set_cksum();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(header.as_bytes()).unwrap();
    gz.write_all(&[0; 1024]).unwrap();
    let bytes = gz.finish().unwrap();
    let directory = tempfile::tempdir().unwrap();
    assert!(extract(&bytes, directory.path(), None)
        .unwrap_err()
        .to_string()
        .contains("100 MiB"));
}

#[test]
fn case_aliases_cannot_overwrite_an_already_validated_file() {
    let bytes = pack(&[
        ("package/assets/name.txt", b"first", tar::EntryType::Regular),
        (
            "package/assets/NAME.txt",
            b"second",
            tar::EntryType::Regular,
        ),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let error = extract(&bytes, directory.path(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate"), "{error}");
}
