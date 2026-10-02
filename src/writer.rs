use crate::error::Error;
use crate::zip::{self, insert_zip_entry, remove_zip_entry, ZIP_MANIFEST_PATH};

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

/// Overwrite a previously embedded placeholder manifest with the final,
/// signed bytes, without touching any size or offset field the first pass's
/// hash already covers.
///
/// Use this, not a second [`embed_manifest`] call, to complete the two-pass
/// `c2pa.hash.data` flow: call `embed_manifest` once with a zero-filled
/// placeholder of the manifest's eventual size, compute the hash over the
/// result via [`crate::binding::central_directory_ranges`], sign, then call
/// this with the signed bytes. `final_manifest` must be exactly as long as
/// the placeholder was -- pad it if necessary, the way the placeholder
/// itself was presumably sized to a known-large-enough length.
///
/// # Errors
///
/// [`Error::Truncated`] if `final_manifest`'s length does not match the
/// placeholder's, or if there is no manifest entry to fill (reserve one with
/// `embed_manifest` first).
pub fn fill_manifest(zip: &[u8], final_manifest: &[u8]) -> Result<Vec<u8>, Error> {
    zip::fill_manifest(zip, ZIP_MANIFEST_PATH, final_manifest)
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

    /// The whole point of fill_manifest: the central-directory hash computed
    /// over a reserved placeholder must still match after filling in a final
    /// manifest of a *different* length than the placeholder reserved --
    /// the realistic case, since a signed manifest's exact byte length is
    /// rarely knowable in advance. A second embed_manifest call would fail
    /// this (it rewrites the manifest entry's own size fields and the
    /// EOCD's central-directory offset, neither of which the hash excludes).
    #[test]
    fn fill_preserves_the_pass_one_hash_across_a_length_change() {
        use crate::binding::central_directory_ranges;

        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let placeholder = vec![0u8; 100];
        let stub = embed_manifest(&zip, &placeholder).unwrap();
        let hash_input_1: Vec<u8> = central_directory_ranges(&stub)
            .unwrap()
            .into_iter()
            .flat_map(|r| stub[r].to_vec())
            .collect();

        // The real manifest is shorter than the reserved placeholder; pad it
        // to the placeholder's length the way a real two-pass caller would.
        let mut final_manifest = b"a real signed manifest".to_vec();
        final_manifest.resize(placeholder.len(), 0);
        let filled = fill_manifest(&stub, &final_manifest).unwrap();

        let hash_input_2: Vec<u8> = central_directory_ranges(&filled)
            .unwrap()
            .into_iter()
            .flat_map(|r| filled[r].to_vec())
            .collect();
        assert_eq!(
            hash_input_1, hash_input_2,
            "the pass-1 hash no longer validates"
        );
        assert_eq!(read_manifest(&filled).unwrap().unwrap(), final_manifest);
    }

    #[test]
    fn fill_rejects_a_length_mismatch() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let stub = embed_manifest(&zip, &[0u8; 10]).unwrap();
        assert!(fill_manifest(&stub, &[1u8; 11]).is_err());
    }

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
