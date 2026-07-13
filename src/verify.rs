use crate::error::Error;
use crate::reader::read_manifest;

/// The result of a structural check of a ZIP-based document's C2PA embedding.
///
/// This is a transport-level report: it records whether the archive parses and
/// whether a manifest entry is present. It does **not** validate the manifest's
/// signature or its hard/soft binding (the collection-data-hash); use the
/// [`c2pa`](https://crates.io/crates/c2pa) SDK for cryptographic validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compliance {
    /// The archive carries a manifest at `META-INF/content_credential.c2pa`.
    pub has_manifest: bool,
    /// Length in bytes of the embedded Manifest Store, if present.
    pub manifest_len: usize,
    /// The input parses as a valid, non-ZIP64 archive.
    pub is_valid_zip: bool,
}

/// Structurally verify a ZIP-based document's C2PA embedding.
///
/// Reports whether the archive parses and whether a manifest is present. A
/// non-ZIP, truncated, or ZIP64 input is reported as `is_valid_zip: false`
/// rather than returned as an error, so callers can inspect any input uniformly.
pub fn verify(zip: &[u8]) -> Result<Compliance, Error> {
    match read_manifest(zip) {
        Ok(Some(manifest)) => Ok(Compliance {
            has_manifest: true,
            manifest_len: manifest.len(),
            is_valid_zip: true,
        }),
        Ok(None) => Ok(Compliance {
            has_manifest: false,
            manifest_len: 0,
            is_valid_zip: true,
        }),
        Err(_) => Ok(Compliance {
            has_manifest: false,
            manifest_len: 0,
            is_valid_zip: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::embed_manifest;
    use crate::zip::tests::build_zip;

    #[test]
    fn reports_embedded_manifest() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let embedded = embed_manifest(&zip, b"MANIFEST").unwrap();
        let report = verify(&embedded).unwrap();
        assert!(report.is_valid_zip);
        assert!(report.has_manifest);
        assert_eq!(report.manifest_len, 8);
    }

    #[test]
    fn reports_valid_zip_without_manifest() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let report = verify(&zip).unwrap();
        assert!(report.is_valid_zip);
        assert!(!report.has_manifest);
        assert_eq!(report.manifest_len, 0);
    }

    #[test]
    fn reports_invalid_zip() {
        let report = verify(b"not a zip file").unwrap();
        assert!(!report.is_valid_zip);
        assert!(!report.has_manifest);
    }

    #[test]
    fn reports_zip64_as_invalid() {
        let mut zip = build_zip(&[("a.txt", b"AAAA")]);
        let eocd = zip.len() - 22;
        zip[eocd + 16..eocd + 20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let report = verify(&zip).unwrap();
        assert!(!report.is_valid_zip);
    }

    #[test]
    fn reports_truncation_as_invalid() {
        let zip = build_zip(&[("content.xml", b"<doc/>")]);
        let embedded = embed_manifest(&zip, b"MANIFEST").unwrap();
        let report = verify(&embedded[..embedded.len() - 1]).unwrap();
        assert!(!report.is_valid_zip);
    }
}
