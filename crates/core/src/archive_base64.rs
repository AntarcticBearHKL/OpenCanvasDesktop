//! Minimal standard base64 (RFC 4648) used by data-URL assets in archives.
//!
//! Kept in-crate so `open-canvas-core` needs no extra dependency: the only
//! consumers are data URLs (`data:<mime>;base64,<payload>`) written by the web
//! export and read back on import. Both the standard (`+`/`/`) and URL-safe
//! (`-`/`_`) alphabets are accepted on decode; encoding always emits the
//! standard alphabet with padding.

use crate::archive::ArchiveError;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as padded standard base64.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(ALPHABET[((triple >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((triple >> 12) & 0x3f) as usize]));
        if chunk.len() > 1 {
            out.push(char::from(ALPHABET[((triple >> 6) & 0x3f) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(ALPHABET[(triple & 0x3f) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

/// Decode standard or URL-safe base64, tolerating ASCII whitespace and a
/// missing final padding block.
pub(crate) fn decode(text: &str) -> Result<Vec<u8>, ArchiveError> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err(ArchiveError::InvalidBase64),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    // A trailing group of 2 or 4 bits is a complete byte; 6 bits means the
    // input ended mid-quantum and cannot represent whole bytes.
    if bits >= 6 {
        return Err(ArchiveError::InvalidBase64);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_rfc4648_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decodes_standard_and_url_safe_payloads() {
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9v\nYmFy").unwrap(), b"foobar");
        assert_eq!(decode("-_8").unwrap(), vec![0xfb, 0xff]);
    }

    #[test]
    fn rejects_invalid_alphabet_and_dangling_bits() {
        assert!(matches!(decode("!!!!"), Err(ArchiveError::InvalidBase64)));
        assert!(matches!(decode("Z"), Err(ArchiveError::InvalidBase64)));
    }
}
