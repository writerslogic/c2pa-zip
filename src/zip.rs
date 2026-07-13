//! ZIP transport primitives for C2PA manifest embedding.
//!
//! Parses, rebuilds, and edits ZIP archives at the byte level to insert, read,
//! and remove a stored (uncompressed) manifest entry. All parsing is bounds-
//! checked against untrusted input; ZIP64 archives are rejected (fail-closed)
//! rather than mis-parsed.

use crate::error::Error;

/// ZIP signatures (little-endian on disk).
const EOCD_SIG: u32 = 0x0605_4b50;
const CD_HEADER_SIG: u32 = 0x0201_4b50;
const LOCAL_HEADER_SIG: u32 = 0x0403_4b50;
/// Minimum end-of-central-directory record length (no comment).
const EOCD_MIN_LEN: usize = 22;
/// Fixed central-directory-header length (before variable name/extra/comment).
const CD_HEADER_FIXED_LEN: usize = 46;
/// Fixed local-file-header length (before variable name/extra).
const LOCAL_HEADER_FIXED_LEN: usize = 30;
/// Sentinel values indicating a ZIP64 field is in use.
const ZIP64_SENTINEL_U32: u32 = 0xFFFF_FFFF;
const ZIP64_SENTINEL_U16: u16 = 0xFFFF;

/// Spec-mandated location + filename of the manifest store in a ZIP container.
pub const ZIP_MANIFEST_PATH: &str = "META-INF/content_credential.c2pa";

fn read_u16(buf: &[u8], at: usize) -> Result<u16, Error> {
    buf.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Truncated)
}

fn read_u32(buf: &[u8], at: usize) -> Result<u32, Error> {
    buf.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated)
}

/// A parsed central-directory entry: its name, local-header offset, and the byte
/// range of its own central-directory header.
struct CdEntry {
    name: String,
    local_header_offset: usize,
    cd_header_offset: usize,
    cd_header_len: usize,
}

/// Parsed ZIP layout: entries (sorted by local-header offset) and the offsets of
/// the central directory and the end-of-central-directory record.
struct ZipLayout {
    entries: Vec<CdEntry>,
    cd_start: usize,
    /// Offset of the end-of-central-directory record (end of the CD headers).
    eocd_offset: usize,
}

/// Locate the EOCD by scanning backwards for its signature, tolerating a trailing
/// ZIP comment of up to 65535 bytes.
fn find_eocd(bytes: &[u8]) -> Result<usize, Error> {
    if bytes.len() < EOCD_MIN_LEN {
        return Err(Error::NoEocd);
    }
    let max_comment = 0xFFFF;
    let scan_start = bytes.len().saturating_sub(EOCD_MIN_LEN + max_comment);
    // Search from the latest possible EOCD position backwards.
    for pos in (scan_start..=bytes.len() - EOCD_MIN_LEN).rev() {
        if read_u32(bytes, pos)? == EOCD_SIG {
            // Validate the comment length matches the remaining bytes so we don't
            // latch onto a false signature inside compressed data.
            let comment_len = read_u16(bytes, pos + 20)? as usize;
            if pos + EOCD_MIN_LEN + comment_len == bytes.len() {
                return Ok(pos);
            }
        }
    }
    Err(Error::NoEocd)
}

fn parse_layout(bytes: &[u8]) -> Result<ZipLayout, Error> {
    let eocd = find_eocd(bytes)?;

    let total_entries = read_u16(bytes, eocd + 10)?;
    let cd_size = read_u32(bytes, eocd + 12)?;
    let cd_offset = read_u32(bytes, eocd + 16)?;
    let comment_len = read_u16(bytes, eocd + 20)? as usize;

    if total_entries == ZIP64_SENTINEL_U16
        || cd_size == ZIP64_SENTINEL_U32
        || cd_offset == ZIP64_SENTINEL_U32
    {
        return Err(Error::Zip64Unsupported);
    }

    let cd_start = cd_offset as usize;
    let cd_through_eocd_end = eocd
        .checked_add(EOCD_MIN_LEN)
        .and_then(|e| e.checked_add(comment_len))
        .ok_or(Error::Truncated)?;
    if cd_start > eocd || cd_through_eocd_end > bytes.len() {
        return Err(Error::BadOffset);
    }

    // Walk the central directory headers.
    let mut entries = Vec::with_capacity(total_entries as usize);
    let mut cursor = cd_start;
    for _ in 0..total_entries {
        if read_u32(bytes, cursor)? != CD_HEADER_SIG {
            return Err(Error::Truncated);
        }
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment = read_u16(bytes, cursor + 32)? as usize;
        let local_off = read_u32(bytes, cursor + 42)?;
        if local_off == ZIP64_SENTINEL_U32 {
            return Err(Error::Zip64Unsupported);
        }
        let name_start = cursor
            .checked_add(CD_HEADER_FIXED_LEN)
            .ok_or(Error::Truncated)?;
        let name_end = name_start.checked_add(name_len).ok_or(Error::Truncated)?;
        let name_bytes = bytes.get(name_start..name_end).ok_or(Error::Truncated)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| Error::NonUtf8Name)?
            .to_string();

        let local_off = local_off as usize;
        if local_off >= cd_start {
            return Err(Error::BadOffset);
        }
        let next = name_end
            .checked_add(extra_len)
            .and_then(|c| c.checked_add(comment))
            .ok_or(Error::Truncated)?;
        if next > eocd {
            return Err(Error::Truncated);
        }
        entries.push(CdEntry {
            name,
            local_header_offset: local_off,
            cd_header_offset: cursor,
            cd_header_len: next - cursor,
        });

        cursor = next;
    }

    // Sort by local-header offset so each entry's byte range ends at the next
    // entry's local header (entries are stored contiguously before the CD).
    entries.sort_by_key(|e| e.local_header_offset);

    Ok(ZipLayout {
        entries,
        cd_start,
        eocd_offset: eocd,
    })
}

/// CRC-32 (IEEE, as used by ZIP) of a byte slice.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Insert a stored (uncompressed) ZIP entry `name` with `content`, returning the
/// new archive. The entry is appended after existing file data (before the
/// central directory), so existing entries keep their byte offsets; the central
/// directory and EOCD are rebuilt to include the new entry.
///
/// The caller is responsible for ensuring no entry named `name` already exists;
/// [`remove_zip_entry`] is used first when replacing.
pub(crate) fn insert_zip_entry(bytes: &[u8], name: &str, content: &[u8]) -> Result<Vec<u8>, Error> {
    let layout = parse_layout(bytes)?;
    let name_b = name.as_bytes();
    let name_len = u16::try_from(name_b.len()).map_err(|_| Error::Truncated)?;
    let size = u32::try_from(content.len()).map_err(|_| Error::Truncated)?;
    let crc = crc32(content);

    // File data of existing entries is unchanged; the new entry goes at cd_start.
    let manifest_local_offset =
        u32::try_from(layout.cd_start).map_err(|_| Error::Zip64Unsupported)?;
    let mut out = Vec::with_capacity(bytes.len() + content.len() + 128);
    out.extend_from_slice(&bytes[..layout.cd_start]);

    // Local file header for the stored entry (general purpose flag 0, method 0).
    out.extend_from_slice(&LOCAL_HEADER_SIG.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method = stored
    out.extend_from_slice(&0u16.to_le_bytes()); // mod time
    out.extend_from_slice(&0u16.to_le_bytes()); // mod date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes()); // compressed
    out.extend_from_slice(&size.to_le_bytes()); // uncompressed
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra len
    out.extend_from_slice(name_b);
    out.extend_from_slice(content);

    let new_cd_start = u32::try_from(out.len()).map_err(|_| Error::Zip64Unsupported)?;
    // Copy the original central directory headers verbatim (offsets still valid).
    out.extend_from_slice(&bytes[layout.cd_start..layout.eocd_offset]);

    // New central directory header for the inserted entry.
    out.extend_from_slice(&CD_HEADER_SIG.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version made by
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method
    out.extend_from_slice(&0u16.to_le_bytes()); // time
    out.extend_from_slice(&0u16.to_le_bytes()); // date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes()); // compressed
    out.extend_from_slice(&size.to_le_bytes()); // uncompressed
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra len
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number start
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    out.extend_from_slice(&manifest_local_offset.to_le_bytes());
    out.extend_from_slice(name_b);

    let new_cd_size =
        u32::try_from(out.len() - new_cd_start as usize).map_err(|_| Error::Truncated)?;
    let total_entries =
        u16::try_from(layout.entries.len() + 1).map_err(|_| Error::Zip64Unsupported)?;

    // End of central directory record (no comment).
    out.extend_from_slice(&EOCD_SIG.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&total_entries.to_le_bytes()); // entries this disk
    out.extend_from_slice(&total_entries.to_le_bytes()); // total entries
    out.extend_from_slice(&new_cd_size.to_le_bytes());
    out.extend_from_slice(&new_cd_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len

    Ok(out)
}

/// Read the stored content of a named entry (e.g. the manifest) from a ZIP.
/// Returns `Ok(None)` if the entry is absent.
pub(crate) fn read_zip_entry_content<'a>(
    bytes: &'a [u8],
    name: &str,
) -> Result<Option<&'a [u8]>, Error> {
    let layout = parse_layout(bytes)?;
    for (i, entry) in layout.entries.iter().enumerate() {
        if entry.name != name {
            continue;
        }
        let lh = entry.local_header_offset;
        if read_u32(bytes, lh)? != LOCAL_HEADER_SIG {
            return Err(Error::Truncated);
        }
        let comp_size = read_u32(bytes, lh + 18)? as usize;
        let name_len = read_u16(bytes, lh + 26)? as usize;
        let extra_len = read_u16(bytes, lh + 28)? as usize;
        let content_start = lh
            .checked_add(LOCAL_HEADER_FIXED_LEN)
            .and_then(|c| c.checked_add(name_len))
            .and_then(|c| c.checked_add(extra_len))
            .ok_or(Error::Truncated)?;
        let content_end = content_start
            .checked_add(comp_size)
            .ok_or(Error::Truncated)?;
        let entry_end = layout
            .entries
            .get(i + 1)
            .map(|n| n.local_header_offset)
            .unwrap_or(layout.cd_start);
        if content_end > entry_end {
            return Err(Error::BadOffset);
        }
        return bytes
            .get(content_start..content_end)
            .map(Some)
            .ok_or(Error::Truncated);
    }
    Ok(None)
}

/// Remove the entry named `name`, rebuilding the archive (file data + central
/// directory + EOCD). Entries retain their relative order; their local-header
/// offsets are recomputed. Returns a byte-identical copy when `name` is absent.
pub(crate) fn remove_zip_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>, Error> {
    let layout = parse_layout(bytes)?;
    if !layout.entries.iter().any(|e| e.name == name) {
        return Ok(bytes.to_vec());
    }

    let mut out = Vec::with_capacity(bytes.len());
    // Retained entries paired with their new local-header offsets, in offset order.
    let mut retained: Vec<(&CdEntry, u32)> = Vec::with_capacity(layout.entries.len());
    for (i, entry) in layout.entries.iter().enumerate() {
        let end = layout
            .entries
            .get(i + 1)
            .map(|n| n.local_header_offset)
            .unwrap_or(layout.cd_start);
        if entry.local_header_offset > end || end > layout.cd_start {
            return Err(Error::BadOffset);
        }
        if entry.name == name {
            continue;
        }
        let data = bytes
            .get(entry.local_header_offset..end)
            .ok_or(Error::Truncated)?;
        let new_off = u32::try_from(out.len()).map_err(|_| Error::Zip64Unsupported)?;
        out.extend_from_slice(data);
        retained.push((entry, new_off));
    }

    let new_cd_start = u32::try_from(out.len()).map_err(|_| Error::Zip64Unsupported)?;
    for (entry, new_off) in &retained {
        let hdr = bytes
            .get(entry.cd_header_offset..entry.cd_header_offset + entry.cd_header_len)
            .ok_or(Error::Truncated)?;
        let start = out.len();
        out.extend_from_slice(hdr);
        // Patch the local-header-offset field (CD header offset 42) to the new value.
        out[start + 42..start + 46].copy_from_slice(&new_off.to_le_bytes());
    }

    let cd_size = u32::try_from(out.len() - new_cd_start as usize).map_err(|_| Error::Truncated)?;
    let total_entries = u16::try_from(retained.len()).map_err(|_| Error::Zip64Unsupported)?;

    out.extend_from_slice(&EOCD_SIG.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&total_entries.to_le_bytes()); // entries this disk
    out.extend_from_slice(&total_entries.to_le_bytes()); // total entries
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&new_cd_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len

    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a minimal, valid ZIP with stored (uncompressed) entries so the byte
    /// layout is deterministic and the parser can be checked against known ranges.
    pub(crate) fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        let mut offsets = Vec::new();

        for (name, data) in files {
            let local_off = out.len() as u32;
            offsets.push(local_off);
            let name_b = name.as_bytes();
            out.extend_from_slice(&LOCAL_HEADER_SIG.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method = stored
            out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            out.extend_from_slice(&0u16.to_le_bytes()); // mod date
            out.extend_from_slice(&crc32(data).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // comp size
            out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncomp size
            out.extend_from_slice(&(name_b.len() as u16).to_le_bytes()); // name len
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_b);
            out.extend_from_slice(data);
        }

        let cd_start = out.len() as u32;
        for (i, (name, data)) in files.iter().enumerate() {
            let name_b = name.as_bytes();
            cd.extend_from_slice(&CD_HEADER_SIG.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
            cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
            cd.extend_from_slice(&0u16.to_le_bytes()); // flags
            cd.extend_from_slice(&0u16.to_le_bytes()); // method
            cd.extend_from_slice(&0u16.to_le_bytes()); // time
            cd.extend_from_slice(&0u16.to_le_bytes()); // date
            cd.extend_from_slice(&crc32(data).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes()); // comp
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncomp
            cd.extend_from_slice(&(name_b.len() as u16).to_le_bytes()); // name len
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk start
            cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            cd.extend_from_slice(&offsets[i].to_le_bytes()); // local header offset
            cd.extend_from_slice(name_b);
        }
        let cd_size = cd.len() as u32;
        out.extend_from_slice(&cd);

        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&(files.len() as u16).to_le_bytes()); // entries this disk
        out.extend_from_slice(&(files.len() as u16).to_le_bytes()); // total entries
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    #[test]
    fn parses_entries_in_offset_order() {
        let zip = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("content.xml", b"<doc>hello</doc>"),
        ]);
        let layout = parse_layout(&zip).unwrap();
        assert_eq!(layout.entries.len(), 2);
        assert_eq!(layout.entries[0].name, "mimetype");
        assert_eq!(layout.entries[1].name, "content.xml");
    }

    #[test]
    fn tolerates_trailing_comment() {
        let mut zip = build_zip(&[("a.txt", b"AAAA")]);
        // Rewrite the EOCD comment length and append the comment bytes.
        let eocd = zip.len() - EOCD_MIN_LEN;
        let comment = b"a trailing zip comment";
        let clen = comment.len() as u16;
        zip[eocd + 20..eocd + 22].copy_from_slice(&clen.to_le_bytes());
        zip.extend_from_slice(comment);
        let layout = parse_layout(&zip).unwrap();
        assert_eq!(layout.entries.len(), 1);
    }

    #[test]
    fn insert_appends_and_keeps_existing_bytes_stable() {
        let zip = build_zip(&[("content.xml", b"<doc/>"), ("styles.xml", b"body{}")]);
        let cd_start = parse_layout(&zip).unwrap().cd_start;
        let out = insert_zip_entry(&zip, ZIP_MANIFEST_PATH, b"MANIFEST").unwrap();
        // Every byte before the original central directory is unchanged.
        assert_eq!(&out[..cd_start], &zip[..cd_start]);
        let content = read_zip_entry_content(&out, ZIP_MANIFEST_PATH)
            .unwrap()
            .unwrap();
        assert_eq!(content, b"MANIFEST");
    }

    #[test]
    fn insert_sets_valid_crc() {
        let zip = build_zip(&[("a.txt", b"AAAA")]);
        let out = insert_zip_entry(&zip, ZIP_MANIFEST_PATH, b"hello world").unwrap();
        let layout = parse_layout(&out).unwrap();
        let m = layout
            .entries
            .iter()
            .find(|e| e.name == ZIP_MANIFEST_PATH)
            .unwrap();
        let local_crc = read_u32(&out, m.local_header_offset + 14).unwrap();
        let cd_crc = read_u32(&out, m.cd_header_offset + 16).unwrap();
        assert_eq!(local_crc, crc32(b"hello world"));
        assert_eq!(cd_crc, crc32(b"hello world"));
    }

    #[test]
    fn read_absent_entry_is_none() {
        let zip = build_zip(&[("a.txt", b"AAAA")]);
        assert!(read_zip_entry_content(&zip, ZIP_MANIFEST_PATH)
            .unwrap()
            .is_none());
    }

    #[test]
    fn remove_absent_entry_is_byte_identical() {
        let zip = build_zip(&[("a.txt", b"AAAA")]);
        let out = remove_zip_entry(&zip, ZIP_MANIFEST_PATH).unwrap();
        assert_eq!(out, zip);
    }

    #[test]
    fn remove_middle_entry_rebuilds_valid_archive() {
        let zip = build_zip(&[("a.txt", b"AAAA"), ("b.txt", b"BBBB"), ("c.txt", b"CCCC")]);
        let out = remove_zip_entry(&zip, "b.txt").unwrap();
        let layout = parse_layout(&out).unwrap();
        assert_eq!(layout.entries.len(), 2);
        assert_eq!(
            read_zip_entry_content(&out, "a.txt").unwrap().unwrap(),
            b"AAAA"
        );
        assert_eq!(
            read_zip_entry_content(&out, "c.txt").unwrap().unwrap(),
            b"CCCC"
        );
        assert!(read_zip_entry_content(&out, "b.txt").unwrap().is_none());
    }

    #[test]
    fn rejects_non_zip() {
        assert!(matches!(
            parse_layout(b"not a zip file at all"),
            Err(Error::NoEocd)
        ));
        assert!(matches!(parse_layout(&[]), Err(Error::NoEocd)));
    }

    #[test]
    fn rejects_zip64_sentinels() {
        let mut zip = build_zip(&[("a.txt", b"AAAA")]);
        let eocd = zip.len() - EOCD_MIN_LEN;
        // Set the CD offset field to the ZIP64 sentinel.
        zip[eocd + 16..eocd + 20].copy_from_slice(&ZIP64_SENTINEL_U32.to_le_bytes());
        assert!(matches!(parse_layout(&zip), Err(Error::Zip64Unsupported)));
    }

    #[test]
    fn rejects_truncation() {
        let zip = build_zip(&[("a.txt", b"AAAA")]);
        // Drop the last byte so the EOCD comment length no longer matches.
        let truncated = &zip[..zip.len() - 1];
        assert!(parse_layout(truncated).is_err());
    }

    #[test]
    fn rejects_out_of_range_cd_offset() {
        let mut zip = build_zip(&[("a.txt", b"AAAA")]);
        let eocd = zip.len() - EOCD_MIN_LEN;
        // Point the central directory past the EOCD record.
        let bad = (eocd + 1) as u32;
        zip[eocd + 16..eocd + 20].copy_from_slice(&bad.to_le_bytes());
        assert!(matches!(parse_layout(&zip), Err(Error::BadOffset)));
    }
}
