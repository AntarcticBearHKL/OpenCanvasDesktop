//! PNG decode/encode at the persistence boundary — the only fallible part of
//! the crate. Everything else is pure raster math.

use std::io::Cursor;

use crate::error::PsError;
use crate::surface::Surface;

/// Decode any supported image into a straight-alpha `Surface` (resized to RGBA8).
pub fn decode_rgba(bytes: &[u8]) -> Result<Surface, PsError> {
    let image =
        image::load_from_memory(bytes).map_err(|error| PsError::Decode(error.to_string()))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let raw = rgba.into_raw();
    let found = raw.len();
    Surface::from_rgba(width as usize, height as usize, raw).ok_or(PsError::Dimension {
        expected: (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4),
        found,
    })
}

/// Encode a `Surface` as PNG bytes.
pub fn encode_png(surface: &Surface) -> Result<Vec<u8>, PsError> {
    let width =
        u32::try_from(surface.width()).map_err(|_| PsError::Encode("width too large".into()))?;
    let height =
        u32::try_from(surface.height()).map_err(|_| PsError::Encode("height too large".into()))?;
    let buffer = image::RgbaImage::from_raw(width, height, surface.data().to_vec()).ok_or(
        PsError::Dimension {
            expected: (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4),
            found: surface.len(),
        },
    )?;
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| PsError::Encode(error.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn png_round_trips_a_surface() {
        let mut surface = Surface::new(2, 2);
        surface.set(0, 0, [255, 0, 0, 255]);
        surface.set(1, 1, [0, 255, 0, 128]);
        let bytes = encode_png(&surface).expect("encode");
        let decoded = decode_rgba(&bytes).expect("decode");
        assert_eq!(decoded, surface);
    }

    #[test]
    fn junk_bytes_are_rejected() {
        assert!(decode_rgba(&[0, 1, 2, 3]).is_err());
    }
}
