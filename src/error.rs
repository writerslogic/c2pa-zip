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
