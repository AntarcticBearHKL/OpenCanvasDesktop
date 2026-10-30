//! Canvas node media: decode a node's stored bitmap into a `slint::Image` and
//! cache it by content hash so a row rebuild never decodes the same bitmap
//! twice.
//!
//! Provenance: `canvas-node.tsx` `CanvasImage` (thumbnail-first resolution:
//! `src = thumbnail || content`) and `ImageStackNodeContent` (`image.thumbnail ||
//! image.content`). The web stores data URLs (`data:image/png;base64,…`) —
//! Image Studio "save to canvas" writes one via
//! `renderStudioAssetOutput`, AI image generation stores the provider's data URL
//! — so the boundary here accepts anything the `image` crate's
//! `load_from_memory` can read (PNG, JPEG, BMP, WebP, GIF, TIFF, ICO, TGA).
//! Animated GIF and multi-page TIFF decode their first frame: the node stores a
//! still raster, and the data URL keeps the original container bytes.
//!
//! Decoding is main-thread CPU work, so the cache is strictly bounded: at most
//! [`MEDIA_CACHE_CAPACITY`] bitmaps and inputs up to [`MEDIA_MAX_TEXT_BYTES`] are
//! considered. Samples beyond that render the designed empty state instead of
//! stalling the UI thread.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

/// Decoded bitmaps kept per app instance (LRU-ish: insertion order, hits refresh).
pub const MEDIA_CACHE_CAPACITY: usize = 48;
/// Largest base64 text accepted for decoding (~9 MiB of binary bytes).
pub const MEDIA_MAX_TEXT_BYTES: usize = 12 * 1024 * 1024;
/// Bytes hashed from each end of the content string (identity sampling).
const HASH_SAMPLE_BYTES: usize = 64 * 1024;

/// FNV-1a over the length, the head and the tail of the content text.
///
/// Hashing a whole multi-megabyte data URL on every row rebuild would be the
/// dominant cost of `sync_rows_of` during a drag, so identity samples the two
/// ends plus the exact length: two different bitmaps agreeing on all three
/// would need a crafted collision, and a cache miss only costs one extra
/// decode — never wrong pixels.
pub fn content_hash(text: &str) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    };
    for byte in (text.len() as u64).to_le_bytes() {
        mix(byte);
    }
    let bytes = text.as_bytes();
    let head = bytes.len().min(HASH_SAMPLE_BYTES);
    for byte in &bytes[..head] {
        mix(*byte);
    }
    let tail_start = bytes.len().saturating_sub(HASH_SAMPLE_BYTES).max(head);
    for byte in &bytes[tail_start..] {
        mix(*byte);
    }
    hash
}

/// A decoded RGBA8 bitmap (straight alpha, row-major).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedPixels {
    /// The pixel at `(x, y)` as `[r, g, b, a]`; out of bounds is transparent.
    /// Pixel assertions in tests read the decoded bitmap through this.
    #[cfg(test)]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let index = (y as usize * self.width as usize + x as usize) * 4;
        match self.rgba.get(index..index + 4) {
            Some(pixel) => [pixel[0], pixel[1], pixel[2], pixel[3]],
            None => [0, 0, 0, 0],
        }
    }
}

/// Splits a stored media string into its declared image format (when the data
/// URL names one) and its base64 payload. Plain base64 has no declared format
/// and is sniffed; TGA has no magic number, so a TGA node depends on this
/// declared `image/x-tga` MIME type to decode at all.
fn base64_payload(text: &str) -> Option<(Option<image::ImageFormat>, &str)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() > MEDIA_MAX_TEXT_BYTES {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("data:") {
        let (meta, payload) = trimmed.split_once(',')?;
        if !meta.contains("base64") {
            return None;
        }
        let mime = rest.split(';').next()?;
        return Some((image::ImageFormat::from_mime_type(mime), payload));
    }
    Some((None, trimmed))
}

/// Reads only the dimensions of a stored media string (data URL or raw
/// base64), without decoding the full bitmap. Image nodes use this to keep
/// their card geometry locked to the bitmap's own aspect ratio.
pub fn decode_dimensions(text: &str) -> Option<(u32, u32)> {
    let (format, payload) = base64_payload(text)?;
    let bytes = BASE64.decode(payload).ok()?;
    let reader = match format {
        Some(format) => image::ImageReader::with_format(std::io::Cursor::new(bytes), format),
        None => image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()?,
    };
    reader.into_dimensions().ok()
}

/// Decodes a stored media string (data URL or raw base64) into RGBA pixels.
pub fn decode_pixels(text: &str) -> Option<DecodedPixels> {
    let (format, payload) = base64_payload(text)?;
    let bytes = BASE64.decode(payload).ok()?;
    let decoded = match format {
        Some(format) => image::load_from_memory_with_format(&bytes, format).ok()?,
        None => image::load_from_memory(&bytes).ok()?,
    }
    .to_rgba8();
    let (width, height) = decoded.dimensions();
    Some(DecodedPixels {
        width,
        height,
        rgba: decoded.into_raw(),
    })
}

/// Decodes stored media text into an `open-canvas-ps` surface (the image node
/// quick tools run the `ps` domain functions on it).
pub fn decode_surface(text: &str) -> Option<open_canvas_ps::Surface> {
    let pixels = decode_pixels(text)?;
    open_canvas_ps::Surface::from_rgba(
        usize::try_from(pixels.width).ok()?,
        usize::try_from(pixels.height).ok()?,
        pixels.rgba,
    )
}

/// Wraps straight-alpha RGBA8 bytes as a `slint::Image` (renderer premultiplies).
pub fn image_from_pixels(pixels: &DecodedPixels) -> Option<slint::Image> {
    let expected = pixels.width as usize * pixels.height as usize * 4;
    if expected == 0 || pixels.rgba.len() != expected {
        return None;
    }
    let mut buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(pixels.width, pixels.height);
    buffer.make_mut_bytes().copy_from_slice(&pixels.rgba);
    Some(slint::Image::from_rgba8(buffer))
}

/// Content-hash keyed decode cache: one decode per distinct bitmap per session.
#[derive(Debug, Default)]
pub struct MediaCache {
    entries: HashMap<u64, slint::Image>,
    order: VecDeque<u64>,
    decodes: usize,
}

impl MediaCache {
    /// Decodes `content` once and returns the cached image on every later call.
    /// Returns `None` for empty/invalid/oversized content.
    pub fn image(&mut self, content: &str) -> Option<slint::Image> {
        let key = content_hash(content);
        if let Some(image) = self.entries.get(&key) {
            let image = image.clone();
            self.touch(key);
            return Some(image);
        }
        let pixels = decode_pixels(content)?;
        let image = image_from_pixels(&pixels)?;
        self.decodes += 1;
        self.insert(key, image.clone());
        Some(image)
    }

    /// Decodes the thumbnail when it resolves, else the full content — the
    /// web's `thumbnail || content` order (`canvas-node.tsx:728-743`).
    pub fn image_or_thumbnail(
        &mut self,
        thumbnail: Option<&str>,
        content: &str,
    ) -> Option<slint::Image> {
        let thumb = thumbnail.filter(|text| !text.is_empty());
        if let Some(text) = thumb {
            if let Some(image) = self.image(text) {
                return Some(image);
            }
        }
        self.image(content)
    }

    /// Stores an already-decoded bitmap (produced off the UI thread) under its
    /// content hash, so the first row paint never decodes on the UI thread.
    pub fn insert_decoded(&mut self, content: &str, pixels: &DecodedPixels) {
        let Some(image) = image_from_pixels(pixels) else {
            return;
        };
        self.insert(content_hash(content), image);
    }

    /// Number of actual decodes performed (cache-effectiveness tests).
    #[cfg(test)]
    pub fn decodes(&self) -> usize {
        self.decodes
    }

    /// Number of cached bitmaps.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn touch(&mut self, key: u64) {
        if let Some(position) = self.order.iter().position(|entry| *entry == key) {
            self.order.remove(position);
        }
        self.order.push_back(key);
    }

    fn insert(&mut self, key: u64, image: slint::Image) {
        match self.entries.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.insert(image);
            }
            Entry::Vacant(entry) => {
                entry.insert(image);
            }
        }
        self.touch(key);
        while self.order.len() > MEDIA_CACHE_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.entries.remove(&evicted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// 2x2 PNG with the four pixels red/green/blue/white, encoded through the
    /// ps crate so the fixture matches the app's own "save to canvas" path.
    fn rgba_png() -> Result<String, Box<dyn std::error::Error>> {
        let mut surface = open_canvas_ps::Surface::new(2, 2);
        surface.set(0, 0, [255, 0, 0, 255]);
        surface.set(1, 0, [0, 255, 0, 255]);
        surface.set(0, 1, [0, 0, 255, 255]);
        surface.set(1, 1, [255, 255, 255, 255]);
        let png = open_canvas_ps::encode_png(&surface)?;
        Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
    }

    #[test]
    fn decodes_a_data_url_to_the_exact_pixels() -> TestResult {
        let pixels = decode_pixels(&rgba_png()?).ok_or("fixture must decode")?;
        assert_eq!((pixels.width, pixels.height), (2, 2));
        assert_eq!(pixels.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(pixels.pixel(1, 0), [0, 255, 0, 255]);
        assert_eq!(pixels.pixel(0, 1), [0, 0, 255, 255]);
        assert_eq!(pixels.pixel(1, 1), [255, 255, 255, 255]);
        assert_eq!(
            pixels.pixel(2, 2),
            [0, 0, 0, 0],
            "out of bounds is transparent"
        );
        Ok(())
    }

    #[test]
    fn decodes_raw_base64_without_the_data_prefix() -> TestResult {
        let mut surface = open_canvas_ps::Surface::new(1, 1);
        surface.set(0, 0, [12, 34, 56, 255]);
        let png = open_canvas_ps::encode_png(&surface)?;
        let pixels = decode_pixels(&BASE64.encode(&png)).ok_or("raw base64 must decode")?;
        assert_eq!(pixels.pixel(0, 0), [12, 34, 56, 255]);
        Ok(())
    }

    /// An animated GIF is a still Image node: the decoder reads frame one and
    /// ignores the rest (the same first-frame rule applies to multi-page TIFF).
    #[test]
    fn decodes_only_the_first_gif_frame() -> TestResult {
        let mut first = image::RgbaImage::new(2, 2);
        for pixel in first.pixels_mut() {
            *pixel = image::Rgba([255, 0, 0, 255]);
        }
        let mut second = image::RgbaImage::new(2, 2);
        for pixel in second.pixels_mut() {
            *pixel = image::Rgba([0, 0, 255, 255]);
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut cursor);
            encoder.encode_frames([image::Frame::new(first), image::Frame::new(second)])?;
        }
        let content = format!(
            "data:image/gif;base64,{}",
            BASE64.encode(cursor.into_inner())
        );
        let pixels = decode_pixels(&content).ok_or("animated gif must decode")?;
        assert_eq!((pixels.width, pixels.height), (2, 2));
        let [r, g, b, a] = pixels.pixel(0, 0);
        assert!(
            r > 200 && g < 60 && b < 60 && a == 255,
            "the first frame (red) is the still image, got {r},{g},{b},{a}"
        );
        Ok(())
    }

    #[test]
    fn rejects_empty_junk_and_non_base64_urls() {
        assert!(decode_pixels("").is_none());
        assert!(decode_pixels("data:image/svg+xml,<svg/>").is_none());
        assert!(decode_pixels("not base64 at all !!").is_none());
        assert!(decode_pixels("data:image/png;base64,%%%%").is_none());
    }

    #[test]
    fn cache_decodes_each_distinct_bitmap_once() -> TestResult {
        let mut cache = MediaCache::default();
        let content = rgba_png()?;
        let first = cache.image(&content).ok_or("first decode")?;
        let second = cache.image(&content).ok_or("cache hit")?;
        assert_eq!(cache.decodes(), 1, "same hash must not decode twice");
        assert_eq!(cache.len(), 1);
        assert_eq!(first.size().width, 2, "the cached handle keeps its pixels");
        assert_eq!(second.size().height, 2);

        let mut other = open_canvas_ps::Surface::new(1, 1);
        other.set(0, 0, [9, 9, 9, 255]);
        let png = open_canvas_ps::encode_png(&other)?;
        let other_content = format!("data:image/png;base64,{}", BASE64.encode(&png));
        assert!(cache.image(&other_content).is_some());
        assert_eq!(cache.decodes(), 2, "a new bitmap decodes once");
        Ok(())
    }

    #[test]
    fn cache_prefers_a_decodable_thumbnail_then_falls_back() -> TestResult {
        let mut cache = MediaCache::default();
        let content = rgba_png()?;
        let via_bad_thumbnail =
            cache.image_or_thumbnail(Some("data:image/png;base64,!!"), &content);
        assert!(
            via_bad_thumbnail.is_some(),
            "falls back to the full content"
        );
        let via_thumbnail = cache.image_or_thumbnail(Some(&content), &content);
        assert!(via_thumbnail.is_some());
        assert_eq!(
            cache.decodes(),
            1,
            "the fallback and the thumbnail share one hash"
        );
        Ok(())
    }

    #[test]
    fn cache_is_bounded_and_evicts_the_oldest_bitmap() -> TestResult {
        let mut cache = MediaCache::default();
        for index in 0..(MEDIA_CACHE_CAPACITY + 3) {
            let mut surface = open_canvas_ps::Surface::new(1, 1);
            let value = u8::try_from(index % 256).unwrap_or(0);
            surface.set(0, 0, [value, 0, 0, 255]);
            let png = open_canvas_ps::encode_png(&surface)?;
            let content = format!("data:image/png;base64,{}", BASE64.encode(&png));
            assert!(cache.image(&content).is_some());
        }
        assert_eq!(cache.len(), MEDIA_CACHE_CAPACITY);
        assert_eq!(cache.decodes(), MEDIA_CACHE_CAPACITY + 3);
        Ok(())
    }

    #[test]
    fn hash_identity_survives_head_tail_sampling() {
        let mut a = "x".repeat(HASH_SAMPLE_BYTES * 3);
        a.push_str("tail-a");
        let mut b = "x".repeat(HASH_SAMPLE_BYTES * 3);
        b.push_str("tail-b");
        assert_ne!(content_hash(&a), content_hash(&b), "tails differ");
        assert_eq!(content_hash(&a), content_hash(&a.clone()));
        assert_ne!(content_hash("a"), content_hash("aa"));
    }
}
