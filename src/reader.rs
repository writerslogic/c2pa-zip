use crate::error::Error;
use crate::zip::{read_zip_entry_content, ZIP_MANIFEST_PATH};

/// Read the embedded C2PA Manifest Store from a ZIP-based document.
///
/// Returns `Ok(Some(bytes))` when the archive carries a manifest at
/// [`ZIP_MANIFEST_PATH`], `Ok(None)` when it parses but has no manifest, and an
/// [`Error`] when the archive is not a parseable ZIP.
pub fn read_manifest(zip: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    Ok(read_zip_entry_content(zip, ZIP_MANIFEST_PATH)?.map(<[u8]>::to_vec))
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
}
