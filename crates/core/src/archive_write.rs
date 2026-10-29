//! Store-only ZIP writer.
//!
//! The web exporter calls fflate with `{ level: 0 }`, so every member is
//! uncompressed; writing stored entries keeps this crate dependency-free and
//! produces archives the web reader accepts. Timestamps are fixed so the same
//! input always yields the same bytes.

use std::collections::BTreeSet;

use crate::archive::{ArchiveError, ArchiveLimits};
use crate::archive_wire::{
    crc32, member_path_is_safe, push_u16, push_u32, CENTRAL_SIGNATURE, DOS_DATE, DOS_TIME,
    END_SIGNATURE, FLAG_UTF8, LOCAL_SIGNATURE, VERSION,
};

pub(crate) fn write_zip(
    entries: &[(String, &[u8])],
    limits: &ArchiveLimits,
) -> Result<Vec<u8>, ArchiveError> {
    if entries.len() > limits.max_entries {
        return Err(ArchiveError::LimitExceeded {
            kind: "member count",
            limit: limits.max_entries as u64,
        });
    }
    if entries.len() > usize::from(u16::MAX) {
        return Err(ArchiveError::Unsupported("too many archive members"));
    }

    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut total: u64 = 0;

    for (name, data) in entries {
        if !member_path_is_safe(name) {
            return Err(ArchiveError::UnsafeMemberPath { path: name.clone() });
        }
        if !seen.insert(name.as_str()) {
            return Err(ArchiveError::DuplicateMember { name: name.clone() });
        }
        let size = data.len() as u64;
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

        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| ArchiveError::Unsupported("archive member name is too long"))?;
        let offset =
            u32::try_from(out.len()).map_err(|_| ArchiveError::Unsupported("archive too large"))?;
        let size32 = u32::try_from(data.len())
            .map_err(|_| ArchiveError::Unsupported("archive too large"))?;
        let checksum = crc32(data);

        push_u32(&mut out, LOCAL_SIGNATURE);
        push_u16(&mut out, VERSION);
        push_u16(&mut out, FLAG_UTF8);
        push_u16(&mut out, 0); // stored
        push_u16(&mut out, DOS_TIME);
        push_u16(&mut out, DOS_DATE);
        push_u32(&mut out, checksum);
        push_u32(&mut out, size32);
        push_u32(&mut out, size32);
        push_u16(&mut out, name_len);
        push_u16(&mut out, 0); // extra field length
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        push_u32(&mut central, CENTRAL_SIGNATURE);
        push_u16(&mut central, VERSION); // version made by
        push_u16(&mut central, VERSION); // version needed
        push_u16(&mut central, FLAG_UTF8);
        push_u16(&mut central, 0); // stored
        push_u16(&mut central, DOS_TIME);
        push_u16(&mut central, DOS_DATE);
        push_u32(&mut central, checksum);
        push_u32(&mut central, size32);
        push_u32(&mut central, size32);
        push_u16(&mut central, name_len);
        push_u16(&mut central, 0); // extra field length
        push_u16(&mut central, 0); // comment length
        push_u16(&mut central, 0); // disk number start
        push_u16(&mut central, 0); // internal attributes
        push_u32(&mut central, 0); // external attributes
        push_u32(&mut central, offset);
        central.extend_from_slice(name_bytes);
    }

    let cd_offset =
        u32::try_from(out.len()).map_err(|_| ArchiveError::Unsupported("archive too large"))?;
    let cd_size =
        u32::try_from(central.len()).map_err(|_| ArchiveError::Unsupported("archive too large"))?;
    out.extend_from_slice(&central);

    let count = u16::try_from(entries.len())
        .map_err(|_| ArchiveError::Unsupported("too many archive members"))?;
    push_u32(&mut out, END_SIGNATURE);
    push_u16(&mut out, 0); // this disk
    push_u16(&mut out, 0); // central directory disk
    push_u16(&mut out, count);
    push_u16(&mut out, count);
    push_u32(&mut out, cd_size);
    push_u32(&mut out, cd_offset);
    push_u16(&mut out, 0); // comment length
    Ok(out)
}
