use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The end-of-central-directory record was not found.
    NoEocd,
    /// A structure extended past the buffer or a length was inconsistent.
    Truncated,
    /// A ZIP64 archive; not supported (rejected fail-closed).
    Zip64Unsupported,
    /// A central directory entry pointed outside the archive.
    BadOffset,
    /// A ZIP entry name was not valid UTF-8.
    NonUtf8Name,
}

impl Error {
    /// The registered C2PA validation status code for this error, or `None`
    /// when the condition carries no status code.
    ///
    /// Always `None` here, and deliberately so. Every variant is an *archive
    /// parsing* failure — the bytes are not a ZIP this crate can read — which
    /// happens before any manifest is located and so is not a validation
    /// outcome.
    ///
    /// A ZIP-based asset is bound with a [collection data
    /// hash](https://crates.io/crates/c2pa-zip), whose failures
    /// (`assertion.collectionHash.mismatch` and friends) belong to whoever
    /// computes the hash. This crate reports the byte ranges to hash and does
    /// not hash them, so those codes are not its to emit.
    ///
    /// Every crate in this family exposes this method, so a dispatcher handling
    /// several embedding methods can ask the same question of any of them.
    pub fn code(&self) -> Option<&'static str> {
        None
    }

    /// Whether this error means the asset carries no provenance at all.
    ///
    /// Always `false`: an unreadable archive is not the same as a readable one
    /// carrying no manifest, which [`crate::read_manifest`] reports as
    /// `Ok(None)` rather than as an error.
    pub fn is_no_manifest_located(&self) -> bool {
        false
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NoEocd => "ZIP end-of-central-directory record not found",
            Self::Truncated => "ZIP archive is truncated or malformed",
            Self::Zip64Unsupported => "ZIP64 archives are not supported",
            Self::BadOffset => "ZIP central directory offset out of range",
            Self::NonUtf8Name => "ZIP entry name is not valid UTF-8",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant is an archive-parsing failure, so none may claim a
    /// validation status code. Guards against a later edit inventing one.
    #[test]
    fn no_variant_claims_a_status_code() {
        for e in [
            Error::NoEocd,
            Error::Truncated,
            Error::Zip64Unsupported,
            Error::BadOffset,
            Error::NonUtf8Name,
        ] {
            assert_eq!(e.code(), None, "{e:?} claimed a status code");
            assert!(
                !e.is_no_manifest_located(),
                "{e:?} is not an absent manifest"
            );
        }
    }

    /// An unreadable archive is an error; an archive with no manifest is
    /// `Ok(None)`. Those two outcomes must not collapse into one, or a caller
    /// cannot tell "corrupt file" from "unsigned file".
    #[test]
    fn an_unreadable_archive_is_distinct_from_an_unsigned_one() {
        let err = crate::read_manifest(b"not a zip at all").unwrap_err();
        assert!(
            !err.is_no_manifest_located(),
            "{err:?} misreported as unsigned"
        );
        assert_eq!(err.code(), None);
    }
}
