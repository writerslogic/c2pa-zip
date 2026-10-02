use crate::error::Error;
use crate::zip::{read_manifest_entry, ZIP_MANIFEST_PATH};

/// Read the embedded C2PA Manifest Store from a ZIP-based document.
///
/// Returns `Ok(Some(bytes))` when the archive carries a manifest at
/// [`ZIP_MANIFEST_PATH`], `Ok(None)` when it parses but has no manifest, and an
/// [`Error`] when the archive is not a parseable ZIP -- including when the
/// entry at that path exists but is compressed or encrypted, which the
/// specification does not permit for the manifest entry specifically.
pub fn read_manifest(zip: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    Ok(read_manifest_entry(zip, ZIP_MANIFEST_PATH)?.map(<[u8]>::to_vec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::embed_manifest;
    use crate::zip::tests::build_zip;

    #[test]
    fn reads_embedded_manifest() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let embedded = embed_manifest(&zip, b"\x00\x01\x02").unwrap();
        assert_eq!(
            read_manifest(&embedded).unwrap().as_deref(),
            Some(&b"\x00\x01\x02"[..])
        );
    }

    #[test]
    fn missing_manifest_is_none() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        assert_eq!(read_manifest(&zip).unwrap(), None);
    }

    #[test]
    fn non_zip_is_error() {
        assert!(read_manifest(b"not a zip").is_err());
    }

    /// The manifest entry must be stored and unencrypted; if a third-party
    /// archive has a compressed or encrypted entry at the manifest path,
    /// read_manifest must reject it rather than hand back the raw
    /// deflated/encrypted bytes as if they were the manifest.
    #[test]
    fn a_compressed_or_encrypted_manifest_entry_is_rejected() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let embedded = embed_manifest(&zip, b"\x00\x01\x02").unwrap();

        // Find the manifest entry's local header by scanning for its name
        // right after the fixed 30-byte local-header prefix, and its CD
        // header the same way after the fixed 46-byte CD prefix.
        let name = crate::zip::ZIP_MANIFEST_PATH.as_bytes();
        let mut tampered = embedded.clone();
        let mut i = 0;
        while i + 30 <= tampered.len() {
            if tampered[i..i + 4] == [0x50, 0x4B, 0x03, 0x04]
                && tampered.get(i + 30..i + 30 + name.len()) == Some(name)
            {
                tampered[i + 6] = 8; // method = deflate
            }
            if tampered[i..i + 4] == [0x50, 0x4B, 0x01, 0x02]
                && tampered.get(i + 46..i + 46 + name.len()) == Some(name)
            {
                tampered[i + 10] = 8; // method = deflate
            }
            i += 1;
        }
        assert!(
            tampered != embedded,
            "the manifest entry was not found to tamper"
        );
        assert!(matches!(
            read_manifest(&tampered),
            Err(Error::ManifestEntryNotStoredOrEncrypted)
        ));
    }
}
