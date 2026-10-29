//! RGBA8 surface buffer + hex conversions + bilinear sampling — the Rust match
//! for the web's `HTMLCanvasElement` + `ImageData` pair used by every PS module.
//!
//! A `Surface` is straight-alpha (non-premultiplied) RGBA8 (`w * h * 4` bytes),
//! exactly like a Canvas2D `ImageData`: `get`/`set` are bounds-checked so an
//! out-of-canvas stamp simply vanishes (mirrors `crates/pixel/src/raster.rs`).

/// `Rgba` (raster.ts:3): one straight-alpha pixel.
pub type Rgba = [u8; 4];

/// Next stamp handed to a brand-new or freshly mutated surface. Process-wide,
/// so two surfaces never share one — a clone keeps its source's stamp because
/// its pixels are identical.
static SURFACE_STAMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_stamp() -> u64 {
    SURFACE_STAMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A width x height RGBA8 bitmap.
#[derive(Debug, Clone)]
pub struct Surface {
    width: usize,
    height: usize,
    px: Vec<u8>,
    /// Content stamp, replaced on every mutation. A consumer that has to know
    /// "are these the same pixels as last time" (the board composite cache)
    /// reads it instead of comparing or hashing the buffer.
    stamp: u64,
}

impl PartialEq for Surface {
    /// Pixels only: the stamp is bookkeeping, not content.
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.px == other.px
    }
}

impl Eq for Surface {}

impl Surface {
    /// A surface's current content stamp; it changes whenever the pixels may
    /// have changed, and never for a read.
    pub const fn stamp(&self) -> u64 {
        self.stamp
    }

    /// Zeroed (fully transparent) `width * height * 4` bytes.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            px: vec![0; width.saturating_mul(height).saturating_mul(4)],
            stamp: next_stamp(),
        }
    }

    /// Wraps decoded RGBA bytes; `None` when the length is not `w * h * 4`.
    pub fn from_rgba(width: usize, height: usize, px: Vec<u8>) -> Option<Self> {
        (px.len() == width.checked_mul(height)?.checked_mul(4)?).then(|| Self {
            width,
            height,
            px,
            stamp: next_stamp(),
        })
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn len(&self) -> usize {
        self.px.len()
    }

    pub fn is_empty(&self) -> bool {
        self.px.is_empty()
    }

    pub fn data(&self) -> &[u8] {
        &self.px
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.stamp = next_stamp();
        &mut self.px
    }

    /// Out-of-bounds reads are transparent black.
    pub fn get(&self, x: i64, y: i64) -> Rgba {
        let Some(index) = self.index(x, y) else {
            return [0, 0, 0, 0];
        };
        [
            self.px[index],
            self.px[index + 1],
            self.px[index + 2],
            self.px[index + 3],
        ]
    }

    /// Out-of-bounds writes are dropped.
    pub fn set(&mut self, x: i64, y: i64, rgba: Rgba) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        self.stamp = next_stamp();
        self.px[index] = rgba[0];
        self.px[index + 1] = rgba[1];
        self.px[index + 2] = rgba[2];
        self.px[index + 3] = rgba[3];
    }

    /// Fills every pixel.
    pub fn fill(&mut self, rgba: Rgba) {
        self.stamp = next_stamp();
        for index in (0..self.px.len()).step_by(4) {
            self.px[index..index + 4].copy_from_slice(&rgba);
        }
    }

    /// Byte index of `(x, y)` when inside the buffer.
    pub(crate) fn index(&self, x: i64, y: i64) -> Option<usize> {
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        if x >= self.width || y >= self.height {
            return None;
        }
        (y * self.width + x).checked_mul(4)
    }

    /// Bilinear sample at a fractional pixel centre, clamped to the edges.
    /// Matches `psSampleBilinear` (filters.ts:181).
    pub fn sample_bilinear(&self, x: f64, y: f64) -> [f64; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let cx = x.clamp(0.0, self.width as f64 - 1.0);
        let cy = y.clamp(0.0, self.height as f64 - 1.0);
        let x0 = cx.floor();
        let y0 = cy.floor();
        let x1 = (x0 + 1.0).min(self.width as f64 - 1.0);
        let y1 = (y0 + 1.0).min(self.height as f64 - 1.0);
        let fx = cx - x0;
        let fy = cy - y0;
        let at = |px: f64, py: f64, c: usize| -> f64 {
            let ix = px as usize;
            let iy = py as usize;
            let index = (iy * self.width + ix) * 4 + c;
            self.px.get(index).copied().map_or(0.0, f64::from)
        };
        let mut out = [0.0; 4];
        for (c, slot) in out.iter_mut().enumerate() {
            *slot = at(x0, y0, c) * (1.0 - fx) * (1.0 - fy)
                + at(x1, y0, c) * fx * (1.0 - fy)
                + at(x0, y1, c) * (1.0 - fx) * fy
                + at(x1, y1, c) * fx * fy;
        }
        out
    }

    /// Bilinear resize to a new pixel size (`psResampleLayerBitmaps`'s drawImage).
    pub fn resample_bilinear(&self, width: usize, height: usize) -> Self {
        let mut out = Self::new(width.max(1), height.max(1));
        let (fw, fh) = (out.width as f64, out.height as f64);
        let (sw, sh) = (self.width as f64, self.height as f64);
        for y in 0..out.height {
            for x in 0..out.width {
                let sx = if fw > 0.0 {
                    (x as f64 + 0.5) / fw * sw - 0.5
                } else {
                    0.0
                };
                let sy = if fh > 0.0 {
                    (y as f64 + 0.5) / fh * sh - 0.5
                } else {
                    0.0
                };
                let sample = self.sample_bilinear(sx, sy);
                if let Some(index) = (y * out.width + x).checked_mul(4) {
                    if index + 3 < out.px.len() {
                        out.px[index] = clamp_u8(sample[0]);
                        out.px[index + 1] = clamp_u8(sample[1]);
                        out.px[index + 2] = clamp_u8(sample[2]);
                        out.px[index + 3] = clamp_u8(sample[3]);
                    }
                }
            }
        }
        out
    }
}

/// `Uint8ClampedArray`-style write: round, then clamp to `0..=255`.
pub fn clamp_u8(value: f64) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is rounded and clamped to an integral 0..=255 range just above"
    )]
    let out = value.round().clamp(0.0, 255.0) as u8;
    out
}

/// `hexToRgba` (raster.ts:7): `transparent` -> `[0,0,0,0]`, `#rgb`/`#rgba`/
/// `#rrggbb`/`#rrggbbaa`; anything unparsable falls back to opaque black.
pub fn hex_to_rgba(hex: &str) -> Rgba {
    let value = hex.trim().to_ascii_lowercase();
    if value == "transparent" {
        return [0, 0, 0, 0];
    }
    let body = value.strip_prefix('#').unwrap_or(value.as_str());
    if body.len() == 3 || body.len() == 4 {
        let mut out = [0_u8; 4];
        for (index, ch) in body.chars().enumerate() {
            let Some(byte) = pair(ch, ch) else {
                return [0, 0, 0, 255];
            };
            out[index] = byte;
        }
        if body.len() == 3 {
            out[3] = 255;
        }
        return out;
    }
    if body.len() == 6 || body.len() == 8 {
        let bytes = body.as_bytes();
        let mut out = [0_u8; 4];
        for (index, slot) in out.iter_mut().enumerate() {
            let hi = bytes.get(index * 2).copied().unwrap_or(b'0') as char;
            let lo = bytes.get(index * 2 + 1).copied().unwrap_or(b'0') as char;
            let Some(byte) = pair(hi, lo) else {
                return [0, 0, 0, 255];
            };
            *slot = byte;
        }
        if body.len() == 6 {
            out[3] = 255;
        }
        return out;
    }
    [0, 0, 0, 255]
}

fn pair(hi: char, lo: char) -> Option<u8> {
    let text: String = [hi, lo].iter().collect();
    u8::from_str_radix(&text, 16).ok()
}

/// `rgbaToHex` (raster.ts:24): `#rrggbb`, plus `aa` when the alpha is below 255.
pub fn rgba_to_hex(rgba: Rgba) -> String {
    let [r, g, b, a] = rgba;
    if a < 255 {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

/// `psBitmapSize` (paint.ts:36): `max(1, round(value))` as a pixel count.
pub fn px_size(value: f64) -> usize {
    if value < 1.0 || !value.is_finite() {
        1
    } else {
        value.round() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms_round_trip() {
        assert_eq!(hex_to_rgba("#000000"), [0, 0, 0, 255]);
        assert_eq!(hex_to_rgba("#fff"), [255, 255, 255, 255]);
        assert_eq!(hex_to_rgba("#00ff0080"), [0, 255, 0, 128]);
        assert_eq!(hex_to_rgba("transparent"), [0, 0, 0, 0]);
        assert_eq!(hex_to_rgba("nonsense"), [0, 0, 0, 255]);
        assert_eq!(rgba_to_hex([0xed, 0x1c, 0x24, 255]), "#ed1c24");
        assert_eq!(rgba_to_hex([0xed, 0x1c, 0x24, 0x80]), "#ed1c2480");
    }

    #[test]
    fn surface_reads_and_writes_are_bounds_checked() {
        let mut surface = Surface::new(2, 2);
        assert_eq!(surface.get(0, 0), [0, 0, 0, 0]);
        surface.set(1, 1, [1, 2, 3, 4]);
        assert_eq!(surface.get(1, 1), [1, 2, 3, 4]);
        surface.set(-1, 0, [9, 9, 9, 9]);
        surface.set(0, 2, [9, 9, 9, 9]);
        assert_eq!(surface.get(-1, 0), [0, 0, 0, 0]);
        assert_eq!(surface.len(), 16);
        assert!(Surface::from_rgba(2, 2, vec![0; 16]).is_some());
        assert!(Surface::from_rgba(2, 2, vec![0; 15]).is_none());
    }

    #[test]
    fn the_stamp_changes_on_a_write_and_not_on_a_read_or_a_clone() {
        let mut surface = Surface::new(4, 4);
        let start = surface.stamp();
        // A read never invalidates a consumer's cached derivative.
        assert_eq!(surface.get(1, 1), [0, 0, 0, 0]);
        assert_eq!(surface.stamp(), start);
        surface.set(1, 1, [1, 2, 3, 4]);
        let after_set = surface.stamp();
        assert_ne!(after_set, start);
        surface.data_mut()[0] = 9;
        assert_ne!(surface.stamp(), after_set);
        // A clone is the same pixels, so it keeps the source's stamp; an
        // independent surface never shares one.
        let copy = surface.clone();
        assert_eq!(copy.stamp(), surface.stamp());
        assert_eq!(copy, surface);
        assert_ne!(Surface::new(4, 4).stamp(), Surface::new(4, 4).stamp());
        // Equality is about pixels only: two surfaces written the same way
        // are equal even though their stamps differ.
        let mut other = Surface::new(4, 4);
        other.set(1, 1, [1, 2, 3, 4]);
        other.data_mut()[0] = 9;
        assert_eq!(other, surface);
        assert_ne!(other.stamp(), surface.stamp());
    }

    #[test]
    fn resample_scales_corners() {
        let mut src = Surface::new(2, 2);
        src.fill([10, 20, 30, 255]);
        let out = src.resample_bilinear(4, 4);
        assert_eq!(out.width(), 4);
        assert_eq!(out.get(0, 0), [10, 20, 30, 255]);
        assert_eq!(out.get(3, 3), [10, 20, 30, 255]);
    }
}
