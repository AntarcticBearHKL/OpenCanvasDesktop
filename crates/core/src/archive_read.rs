//! Store-only ZIP reader.
//!
//! The central directory is authoritative: names, sizes, checksums, and local
//! header offsets all come from it, and the local header is cross-checked
//! against the central entry. Only stored (uncompressed) entries are accepted,
//! matching what the web exporter writes (`fflate` level 0).

use std::collections::BTreeSet;

use crate::archive::{ArchiveError, ArchiveLimits};
use crate::archive_wire::{
    crc32, member_path_is_safe, read_u16, read_u32, CENTRAL_HEADER_LEN, CENTRAL_SIGNATURE,
    END_RECORD_LEN, END_SIGNATURE, LOCAL_HEADER_LEN, LOCAL_SIGNATURE,
};

pub(crate) struct ZipMember {
    pub name: String,
    pub data: Vec<u8>,
}

pub(crate) fn read_zip(
    bytes: &[u8],
    limits: &ArchiveLimits,
) -> Result<Vec<ZipMember>, ArchiveError> {
    let end = find_end_record(bytes)?;

    let disk = read_u16(bytes, end + 4).ok_or(ArchiveError::Truncated)?;
    let cd_disk = read_u16(bytes, end + 6).ok_or(ArchiveError::Truncated)?;
    let entries_disk = read_u16(bytes, end + 8).ok_or(ArchiveError::Truncated)?;
    let entries_total = read_u16(bytes, end + 10).ok_or(ArchiveError::Truncated)?;
    let cd_size = read_u32(bytes, end + 12).ok_or(ArchiveError::Truncated)?;
    let cd_offset = read_u32(bytes, end + 16).ok_or(ArchiveError::Truncated)?;

    if disk != 0 || cd_disk != 0 || entries_disk != entries_total {
        return Err(ArchiveError::Unsupported("multi-disk archives"));
    }
    if entries_total == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
        return Err(ArchiveError::Unsupported("ZIP64 archives"));
    }
    let entry_count = usize::from(entries_total);
    if entry_count > limits.max_entries {
        return Err(ArchiveError::LimitExceeded {
            kind: "member count",
            limit: limits.max_entries as u64,
        });
    }

    let cd_start = usize::try_from(cd_offset).map_err(|_| ArchiveError::Truncated)?;
    let cd_bytes = usize::try_from(cd_size).map_err(|_| ArchiveError::Truncated)?;
    let cd_end = checked_add(cd_start, cd_bytes)?;
    if cd_end > end {
        return Err(ArchiveError::InvalidZip(
            "central directory overlaps the end record",
        ));
    }

    let mut members = Vec::with_capacity(entry_count.min(256));
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut total: u64 = 0;
    let mut offset = cd_start;

    for _ in 0..entry_count {
        if read_u32(bytes, offset) != Some(CENTRAL_SIGNATURE) {
            return Err(ArchiveError::InvalidZip(
                "bad central directory header signature",
            ));
        }
        let flags = read_u16(bytes, offset + 8).ok_or(ArchiveError::Truncated)?;
        let method = read_u16(bytes, offset + 10).ok_or(ArchiveError::Truncated)?;
        let checksum = read_u32(bytes, offset + 16).ok_or(ArchiveError::Truncated)?;
        let compressed = read_u32(bytes, offset + 20).ok_or(ArchiveError::Truncated)?;
        let uncompressed = read_u32(bytes, offset + 24).ok_or(ArchiveError::Truncated)?;
        let name_len = usize::from(read_u16(bytes, offset + 28).ok_or(ArchiveError::Truncated)?);
        let extra_len = usize::from(read_u16(bytes, offset + 30).ok_or(ArchiveError::Truncated)?);
        let comment_len = usize::from(read_u16(bytes, offset + 32).ok_or(ArchiveError::Truncated)?);
        let local_offset =
            usize::try_from(read_u32(bytes, offset + 42).ok_or(ArchiveError::Truncated)?)
                .map_err(|_| ArchiveError::Truncated)?;

        if flags & 1 != 0 {
            return Err(ArchiveError::Unsupported("encrypted members"));
        }
        if method != 0 {
            return Err(ArchiveError::Unsupported("compressed members"));
        }
        if compressed != uncompressed {
            return Err(ArchiveError::InvalidZip(
                "stored member reports different compressed and uncompressed sizes",
            ));
        }
        if uncompressed == u32::MAX || compressed == u32::MAX || local_offset == u32::MAX as usize {
            return Err(ArchiveError::Unsupported("ZIP64 archives"));
        }
        if local_offset >= cd_start {
            return Err(ArchiveError::InvalidZip(
                "local header lies inside the central directory",
            ));
        }

        let name_start = checked_add(offset, CENTRAL_HEADER_LEN)?;
        let name_end = checked_add(name_start, name_len)?;
        let entry_end = checked_add(checked_add(name_end, extra_len)?, comment_len)?;
        if entry_end > cd_end {
            return Err(ArchiveError::InvalidZip(
                "central directory entry is out of bounds",
            ));
        }
        let name_bytes = bytes
            .get(name_start..name_end)
            .ok_or(ArchiveError::Truncated)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| ArchiveError::InvalidZip("member name is not UTF-8"))?;
        if !member_path_is_safe(name) {
            return Err(ArchiveError::UnsafeMemberPath {
                path: name.to_string(),
            });
        }
        if !seen.insert(name.to_string()) {
            return Err(ArchiveError::DuplicateMember {
                name: name.to_string(),
            });
        }

        if !name.ends_with('/') {
            let size = u64::from(uncompressed);
            if size > limits.max_entry_bytes {
                return Err(ArchiveError::LimitExceeded {
                    kind: "entry byte",
                    limit: limits.max_entry_bytes,
                });
            }
            total = total.saturating_add(size);
            if total > limits.max_total_bytes {
                return Err(ArchiveError::LimitExceeded {
                    kind: "total byte",
                    limit: limits.max_total_bytes,
                });
            }

            let local_name_len =
                usize::from(read_u16(bytes, local_offset + 26).ok_or(ArchiveError::Truncated)?);
            let local_extra_len =
                usize::from(read_u16(bytes, local_offset + 28).ok_or(ArchiveError::Truncated)?);
            if read_u32(bytes, local_offset) != Some(LOCAL_SIGNATURE) {
                return Err(ArchiveError::InvalidZip("bad local header signature"));
            }
            let local_name_start = checked_add(local_offset, LOCAL_HEADER_LEN)?;
            let local_name_end = checked_add(local_name_start, local_name_len)?;
            if bytes.get(local_name_start..local_name_end) != Some(name_bytes) {
                return Err(ArchiveError::InvalidZip(
                    "local and central member names differ",
                ));
            }
            let data_start = checked_add(local_name_end, local_extra_len)?;
            let data_end = checked_add(
                data_start,
                usize::try_from(uncompressed).map_err(|_| ArchiveError::Truncated)?,
            )?;
            let data = bytes
                .get(data_start..data_end)
                .ok_or(ArchiveError::Truncated)?;
            let actual = crc32(data);
            if actual != checksum {
                return Err(ArchiveError::CrcMismatch {
                    name: name.to_string(),
                    expected: checksum,
                    actual,
                });
            }
            members.push(ZipMember {
                name: name.to_string(),
                data: data.to_vec(),
            });
        }

        offset = entry_end;
    }

    Ok(members)
}

fn find_end_record(bytes: &[u8]) -> Result<usize, ArchiveError> {
    if bytes.len() < END_RECORD_LEN {
        return Err(ArchiveError::Truncated);
    }
    let latest = bytes.len() - END_RECORD_LEN;
    let earliest = latest.saturating_sub(usize::from(u16::MAX));
    for offset in (earliest..=latest).rev() {
        if read_u32(bytes, offset) == Some(END_SIGNATURE) {
            let comment = usize::from(read_u16(bytes, offset + 20).unwrap_or(0));
            if offset
                .saturating_add(END_RECORD_LEN)
                .saturating_add(comment)
                == bytes.len()
            {
                return Ok(offset);
            }
        }
    }
    Err(ArchiveError::Truncated)
}

fn checked_add(left: usize, right: usize) -> Result<usize, ArchiveError> {
    left.checked_add(right).ok_or(ArchiveError::Truncated)
}
