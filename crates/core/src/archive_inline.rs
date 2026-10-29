//! Inline `data:` URL handling: decode a base64 data URL into a typed asset and
//! derive a deterministic storage key for assets that had none.

use crate::archive::ArchiveError;
use crate::archive_base64 as base64;

/// One decoded inline asset (mime type plus raw bytes).
pub(crate) struct InlineAsset {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Decode `value` when it is a base64 `data:` URL.
///
/// Returns `Ok(None)` for anything that is not a base64 data URL (plain URLs,
/// storage keys, `archive:` references, `data:` URLs without a base64 marker),
/// so callers can leave such values untouched.
pub(crate) fn parse_data_url(value: &str) -> Result<Option<InlineAsset>, ArchiveError> {
    let Some(rest) = value.strip_prefix("data:") else {
        return Ok(None);
    };
    let Some((header, payload)) = rest.split_once(',') else {
        return Err(ArchiveError::InvalidDataUrl("missing comma separator"));
    };
    if !header
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        return Ok(None);
    }
    let declared = header.split(';').next().unwrap_or("");
    let mime_type = if declared.is_empty() {
        "application/octet-stream".to_string()
    } else {
        declared.to_ascii_lowercase()
    };
    let data = base64::decode(payload)?;
    Ok(Some(InlineAsset { mime_type, data }))
}

/// A stable `image:`/`media:` key derived from the payload hash.
pub(crate) fn derived_key(mime_type: &str, data: &[u8]) -> String {
    let prefix = if mime_type.starts_with("image/") {
        "image"
    } else {
        "media"
    };
    format!("{prefix}:{:016x}", fnv1a(data))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64_data_urls() {
        let asset = parse_data_url("data:image/png;base64,iVBORw0KGgo=")
            .unwrap()
            .unwrap();
        assert_eq!(asset.mime_type, "image/png");
        assert_eq!(asset.data, b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn leaves_non_assets_untouched() {
        assert!(parse_data_url("https://example.test/a.png")
            .unwrap()
            .is_none());
        assert!(parse_data_url("image:abc").unwrap().is_none());
        assert!(parse_data_url("archive:projects/p1/files/a.png")
            .unwrap()
            .is_none());
        assert!(parse_data_url("data:text/plain,hello").unwrap().is_none());
        // An inline text node may legitimately start with "data:" and no comma.
        assert!(matches!(
            parse_data_url("data: not really"),
            Err(ArchiveError::InvalidDataUrl(_))
        ));
    }

    #[test]
    fn default_mime_and_derived_keys() {
        let asset = parse_data_url("data:;base64,Zm9v").unwrap().unwrap();
        assert_eq!(asset.mime_type, "application/octet-stream");
        let image = derived_key("image/png", b"abc");
        assert!(image.starts_with("image:"));
        assert_eq!(image.len(), "image:".len() + 16);
        let media = derived_key("video/mp4", b"abc");
        assert!(media.starts_with("media:"));
        // Deterministic for the same payload.
        assert_eq!(derived_key("image/png", b"abc"), image);
    }
}
