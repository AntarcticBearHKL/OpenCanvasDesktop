//! Small ZIP wire helpers shared by the writer and reader: signatures, CRC-32,
//! little-endian encoding, and archive member-path validation.

/// Local file header signature (`PK\x03\x04`).
pub(crate) const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// Central directory header signature (`PK\x01\x02`).
pub(crate) const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// End of central directory signature (`PK\x05\x06`).
pub(crate) const END_SIGNATURE: u32 = 0x0605_4b50;

/// ZIP version 2.0 — the minimum for the stored (uncompressed) entries we write.
pub(crate) const VERSION: u16 = 20;
/// "UTF-8 file name" general-purpose flag.
pub(crate) const FLAG_UTF8: u16 = 0x0800;
/// 1980-01-01, the DOS epoch: fixed so packed archives are byte-deterministic.
pub(crate) const DOS_DATE: u16 = 0x0021;
pub(crate) const DOS_TIME: u16 = 0x0000;

/// Length of a fixed local file header (without name/extra).
pub(crate) const LOCAL_HEADER_LEN: usize = 30;
/// Length of a fixed central directory header (without name/extra/comment).
pub(crate) const CENTRAL_HEADER_LEN: usize = 46;
/// Length of the end-of-central-directory record (without comment).
pub(crate) const END_RECORD_LEN: usize = 22;

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

/// CRC-32 (IEEE 802.3), the checksum ZIP members carry.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        let slot = ((crc ^ u32::from(*byte)) & 0xff) as usize;
        crc = CRC_TABLE[slot] ^ (crc >> 8);
    }
    crc ^ 0xffff_ffff
}

pub(crate) fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// True when `name` is a relative path with no `.`/`..`/empty components, no
/// absolute or Windows-drive prefix, and no backslashes or NUL bytes.
pub(crate) fn member_path_is_safe(name: &str) -> bool {
    if name.is_empty() || name.len() > 1024 {
        return false;
    }
    if name.contains('\0') || name.contains('\\') || name.starts_with('/') {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    let body = name.strip_suffix('/').unwrap_or(name);
    if body.is_empty() {
        return false;
    }
    body.split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_known_vectors() {
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b"hello"), 0x3610_a686);
    }

    #[test]
    fn member_paths_reject_traversal_and_absolute_forms() {
        for safe in [
            "projects.json",
            "projects/p1/files/a.png",
            "projects/p1/files/a..b.png",
            "dir/",
        ] {
            assert!(member_path_is_safe(safe), "{safe:?} should be safe");
        }
        for unsafe_name in [
            "",
            "/etc/passwd",
            "../evil",
            "a/../../b",
            "a\\b",
            "C:/windows/x",
            "a//b",
            "./a",
            "a/./b",
            "a\0b",
        ] {
            assert!(
                !member_path_is_safe(unsafe_name),
                "{unsafe_name:?} should be rejected"
            );
        }
    }
}
