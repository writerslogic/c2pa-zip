use crate::error::Error;
use crate::zip::{insert_zip_entry, remove_zip_entry, ZIP_MANIFEST_PATH};

/// Embed a C2PA Manifest Store into a ZIP-based document.
///
/// Inserts (or replaces) a stored, uncompressed entry at [`ZIP_MANIFEST_PATH`]
/// and rebuilds the central directory and EOCD. An existing manifest entry is
/// removed first, so calling this repeatedly leaves exactly one manifest. When
/// no manifest is present, existing entries keep their byte offsets (the new
/// entry is appended before the central directory).
pub fn embed_manifest(zip: &[u8], manifest_store: &[u8]) -> Result<Vec<u8>, Error> {
    let base = remove_zip_entry(zip, ZIP_MANIFEST_PATH)?;
    insert_zip_entry(&base, ZIP_MANIFEST_PATH, manifest_store)
}

/// Remove the C2PA Manifest Store from a ZIP-based document, if present,
/// returning the rebuilt archive. A document without a manifest is returned
/// unchanged.
pub fn remove_manifest(zip: &[u8]) -> Result<Vec<u8>, Error> {
    remove_zip_entry(zip, ZIP_MANIFEST_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read_manifest;
    use crate::zip::tests::build_zip;

    #[test]
    fn embed_then_read_round_trips() {
        let zip = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("content.xml", b"<doc/>"),
        ]);
        let embedded = embed_manifest(&zip, b"MANIFESTBYTES").unwrap();
        assert_eq!(read_manifest(&embedded).unwrap().unwrap(), b"MANIFESTBYTES");
    }

    #[test]
    fn embed_preserves_other_entries() {
        let zip = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("content.xml", b"<doc/>"),
        ]);
        let embedded = embed_manifest(&zip, b"m").unwrap();
        assert_eq!(read_manifest(&embedded).unwrap().unwrap(), b"m");
        // The original entries survive intact.
        assert_eq!(
            crate::zip::read_zip_entry_content(&embedded, "mimetype")
                .unwrap()
                .unwrap(),
            b"application/epub+zip"
        );
        assert_eq!(
            crate::zip::read_zip_entry_content(&embedded, "content.xml")
                .unwrap()
                .unwrap(),
            b"<doc/>"
        );
    }

    #[test]
    fn embed_replaces_existing_manifest() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let first = embed_manifest(&zip, b"old").unwrap();
        let second = embed_manifest(&first, b"newer-manifest").unwrap();
        assert_eq!(read_manifest(&second).unwrap().unwrap(), b"newer-manifest");
        // Exactly one manifest entry remains.
        let layout = crate::zip::read_zip_entry_content(&second, ZIP_MANIFEST_PATH).unwrap();
        assert!(layout.is_some());
    }

    #[test]
    fn remove_strips_manifest() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let embedded = embed_manifest(&zip, b"m").unwrap();
        let removed = remove_manifest(&embedded).unwrap();
        assert_eq!(read_manifest(&removed).unwrap(), None);
        assert_eq!(
            crate::zip::read_zip_entry_content(&removed, "content.xml")
                .unwrap()
                .unwrap(),
            b"<doc/>"
        );
    }

    #[test]
    fn remove_without_manifest_is_noop() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let out = remove_manifest(&zip).unwrap();
        assert_eq!(out, zip);
    }

    #[test]
    fn embed_rejects_zip64() {
        let mut zip = build_zip(&[("a.txt", b"AAAA")]);
        let eocd = zip.len() - 22;
        zip[eocd + 16..eocd + 20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(matches!(
            embed_manifest(&zip, b"m"),
            Err(Error::Zip64Unsupported)
        ));
    }

    #[test]
    fn embed_rejects_non_zip() {
        assert!(embed_manifest(b"not a zip", b"m").is_err());
    }
}
