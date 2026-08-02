//! End-to-end use of the public API, as a consumer sees it.
//!
//! The archive is built here from the ZIP specification rather than reusing the
//! crate's private test fixture, so the parser is checked against bytes it did
//! not produce. A mistake in this builder makes the crate reject the archive and
//! the test fail loudly — it cannot silently pass.

use c2pa_zip::{
    central_directory_range, collection_members, embed_manifest, read_manifest, remove_manifest,
    verify, Error, ZIP_MANIFEST_PATH,
};

/// CRC-32 (IEEE 802.3, reflected), as ZIP requires for each entry.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// A minimal ZIP with stored (uncompressed) entries.
fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut cd: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();

    for (name, data) in files {
        offsets.push(out.len() as u32);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc32(data).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
    }

    for ((name, data), &offset) in files.iter().zip(&offsets) {
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
        cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
        cd.extend_from_slice(&0u16.to_le_bytes()); // flags
        cd.extend_from_slice(&0u16.to_le_bytes()); // method
        cd.extend_from_slice(&0u16.to_le_bytes()); // mod time
        cd.extend_from_slice(&0u16.to_le_bytes()); // mod date
        cd.extend_from_slice(&crc32(data).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
        cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
        cd.extend_from_slice(&0u16.to_le_bytes()); // disk start
        cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        cd.extend_from_slice(&offset.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
    }

    let cd_offset = out.len() as u32;
    let cd_size = cd.len() as u32;
    out.extend_from_slice(&cd);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out
}

fn epub() -> Vec<u8> {
    build_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", b"<container/>"),
        ("OEBPS/ch1.xhtml", b"<html><body>one</body></html>"),
    ])
}

const STORE: &[u8] = b"\x00\x01\x02manifest-store\xFF";

#[test]
fn the_fixture_is_a_readable_archive() {
    // If this fails, every other test here is meaningless.
    let report = verify(&epub()).unwrap();
    assert!(
        report.is_valid_zip,
        "the hand-built fixture is not a valid ZIP"
    );
    assert!(!report.has_manifest);
}

#[test]
fn embed_read_and_remove_round_trip() {
    let original = epub();
    let embedded = embed_manifest(&original, STORE).unwrap();
    assert_eq!(read_manifest(&embedded).unwrap().as_deref(), Some(STORE));
    assert_eq!(remove_manifest(&embedded).unwrap(), original);
}

#[test]
fn the_manifest_lives_at_the_specified_path() {
    assert_eq!(ZIP_MANIFEST_PATH, "META-INF/content_credential.c2pa");
    let embedded = embed_manifest(&epub(), STORE).unwrap();
    assert!(embedded
        .windows(ZIP_MANIFEST_PATH.len())
        .any(|w| w == ZIP_MANIFEST_PATH.as_bytes()));
}

#[test]
fn embedding_twice_replaces_rather_than_accumulates() {
    let once = embed_manifest(&epub(), b"first").unwrap();
    let twice = embed_manifest(&once, b"second").unwrap();
    assert_eq!(
        read_manifest(&twice).unwrap().as_deref(),
        Some(&b"second"[..])
    );
}

#[test]
fn the_collection_covers_every_member_except_the_manifest() {
    let embedded = embed_manifest(&epub(), STORE).unwrap();
    let members = collection_members(&embedded).unwrap();

    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"mimetype"));
    assert!(names.contains(&"OEBPS/ch1.xhtml"));
    assert!(
        !names.contains(&ZIP_MANIFEST_PATH),
        "the manifest must not be inside its own collection hash"
    );

    // Every reported range must be in bounds.
    for m in &members {
        assert!(m.content.end <= embedded.len(), "{} out of bounds", m.name);
    }
}

#[test]
fn the_central_directory_range_is_in_bounds_and_starts_at_a_header() {
    let embedded = embed_manifest(&epub(), STORE).unwrap();
    let range = central_directory_range(&embedded).unwrap();
    assert!(range.end <= embedded.len());
    assert_eq!(
        &embedded[range.start..range.start + 4],
        b"PK\x01\x02",
        "the range should begin at a central directory header"
    );
}

#[test]
fn an_archive_without_a_manifest_reads_as_none_not_an_error() {
    // "Unsigned" and "unreadable" must stay distinguishable.
    assert_eq!(read_manifest(&epub()).unwrap(), None);
}

#[test]
fn a_non_archive_is_an_error_carrying_no_status_code() {
    let err = read_manifest(b"this is not a zip file").unwrap_err();
    assert_eq!(err.code(), None);
    assert!(!err.is_no_manifest_located());
    assert!(matches!(err, Error::NoEocd | Error::Truncated));
}
