// Copyright 2026 WritersLogic. All rights reserved.
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option.

//! Inputs for the ZIP collection data hash.
//!
//! A ZIP asset is bound with a `c2pa.hash.collection.data` assertion: one entry
//! per archive member, plus the additional `zip_central_directory_hash` field
//! that covers the archive's own directory. Without that extra field the
//! assertion binds only the members it lists, so an entry added after signing
//! would leave the manifest valid.
//!
//! This module supplies the *byte ranges* the specification says to hash, and
//! deliberately does not hash them. Locating those ranges is ZIP parsing, which
//! is this crate's job; choosing and running a digest is the caller's, and
//! keeping it that way is what lets this crate stay dependency-free.

use crate::error::Error;
use crate::zip::{self, ZIP_MANIFEST_PATH};

/// A member of the archive, and the byte range of its stored content.
///
/// The `name` is the entry's path within the archive, which is the value the
/// `uri` field of the corresponding collection entry takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The entry's path within the archive.
    pub name: String,
    /// Byte range of the bytes the collection hash covers for this member: the
    /// local file header followed by the stored (still compressed) content.
    pub content: std::ops::Range<usize>,
}

/// Every archive member except the C2PA Manifest Store entry, in archive order.
///
/// These are the members the collection data hash covers. The manifest entry is
/// excluded because it cannot hash itself.
pub fn collection_members(zip: &[u8]) -> Result<Vec<Member>, Error> {
    Ok(zip::member_ranges(zip)?
        .into_iter()
        .filter(|(name, _)| name != ZIP_MANIFEST_PATH)
        .map(|(name, content)| Member { name, content })
        .collect())
}

/// The byte range covered by `zip_central_directory_hash`: every central
/// directory header together with the end-of-central-directory record.
///
/// The specification defines this field as a hash "of every central directory
/// header in the ZIP Central Directory as well as the end of central directory
/// record". Those are contiguous, so the coverage is a single range running
/// from the first header to the end of the archive.
pub fn central_directory_range(zip: &[u8]) -> Result<std::ops::Range<usize>, Error> {
    zip::central_directory_range(zip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::embed_manifest;
    use crate::zip::tests::build_zip;

    const MANIFEST: &[u8] = b"pretend-manifest-store";

    fn fixture() -> Vec<u8> {
        build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", b"<container/>"),
            ("OEBPS/content.opf", b"<package/>"),
        ])
    }

    #[test]
    fn members_exclude_the_manifest_entry() {
        let signed = embed_manifest(&fixture(), MANIFEST).unwrap();
        let members = collection_members(&signed).unwrap();
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            ["mimetype", "META-INF/container.xml", "OEBPS/content.opf"]
        );
        assert!(!names.contains(&ZIP_MANIFEST_PATH));
    }

    #[test]
    fn member_ranges_address_the_stored_content() {
        let zip = fixture();
        let members = collection_members(&zip).unwrap();
        let mimetype = members.iter().find(|m| m.name == "mimetype").unwrap();
        assert_eq!(&zip[mimetype.content.clone()], b"application/epub+zip");
    }

    #[test]
    fn central_directory_range_reaches_the_end_of_the_archive() {
        let zip = fixture();
        let range = central_directory_range(&zip).unwrap();
        assert_eq!(range.end, zip.len(), "the EOCD record is the last thing");
        // The range starts on a central directory header signature.
        assert_eq!(&zip[range.start..range.start + 4], b"PK\x01\x02");
        // And contains the EOCD signature.
        assert!(zip[range.clone()].windows(4).any(|w| w == b"PK\x05\x06"));
    }

    #[test]
    fn adding_an_entry_changes_the_central_directory_coverage() {
        let zip = fixture();
        let before = &zip[central_directory_range(&zip).unwrap()].to_vec();
        let signed = embed_manifest(&zip, MANIFEST).unwrap();
        let after = &signed[central_directory_range(&signed).unwrap()].to_vec();
        // This is the property the field exists for: an entry appended after
        // signing cannot leave the directory coverage unchanged.
        assert_ne!(before, after);
    }

    #[test]
    fn every_member_range_is_inside_the_archive_and_before_the_directory() {
        let signed = embed_manifest(&fixture(), MANIFEST).unwrap();
        let cd = central_directory_range(&signed).unwrap();
        for m in collection_members(&signed).unwrap() {
            assert!(m.content.end <= signed.len(), "{} past end", m.name);
            assert!(m.content.start <= m.content.end, "{} inverted", m.name);
            assert!(
                m.content.end <= cd.start,
                "{} overlaps the directory",
                m.name
            );
        }
    }

    #[test]
    fn a_truncated_archive_is_rejected_rather_than_panicking() {
        let zip = fixture();
        for cut in [0usize, 1, 8, zip.len() / 2, zip.len() - 1] {
            let truncated = &zip[..cut];
            assert!(collection_members(truncated).is_err() || truncated.is_empty());
            assert!(central_directory_range(truncated).is_err() || truncated.is_empty());
        }
    }
}
