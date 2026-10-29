//! Local, AI-free image analysis — port of `web/src/lib/image/image-algorithms.ts`
//! and `canvas-image-analysis-dialog.tsx`: palette extraction, pixel-derived
//! metadata, perceptual hash and a deterministic smart-crop suggestion.

use crate::surface::{rgba_to_hex, Rgba, Surface};

/// `extractImagePalette` default `count` (image-algorithms.ts:19).
pub const DEFAULT_PALETTE_COUNT: usize = 6;
/// Quantisation levels per channel (4-bit) used by the palette histogram.
pub const PALETTE_LEVELS: usize = 16;

/// One palette entry: the bucket's representative colour and its pixel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub count: usize,
}

impl PaletteColor {
    pub fn hex(&self) -> String {
        rgba_to_hex([self.r, self.g, self.b, 255])
    }
}

/// `extractImagePalette` (image-algorithms.ts:19): the `count` most common
/// opaque colours. Fully transparent pixels are skipped; ties break by bucket
/// order, so the result is deterministic.
pub fn extract_palette(surface: &Surface, count: usize) -> Vec<PaletteColor> {
    if count == 0 || surface.is_empty() {
        return Vec::new();
    }
    let levels = PALETTE_LEVELS;
    let step = 256 / levels;
    let mut buckets = vec![0usize; levels * levels * levels];
    for y in 0..surface.height() {
        for x in 0..surface.width() {
            let [r, g, b, a] = surface.get(x as i64, y as i64);
            if a == 0 {
                continue;
            }
            let index = (r as usize / step) * levels * levels
                + (g as usize / step) * levels
                + (b as usize / step);
            buckets[index] += 1;
        }
    }
    let mut ranked: Vec<(usize, usize)> = buckets
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, hits)| *hits > 0)
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(count);
    ranked
        .into_iter()
        .map(|(index, hits)| {
            let (r, g, b) = decode_bucket(index);
            PaletteColor {
                r,
                g,
                b,
                count: hits,
            }
        })
        .collect()
}

fn decode_bucket(index: usize) -> (u8, u8, u8) {
    let level = |value: usize| -> u8 { (value * 255 / (PALETTE_LEVELS - 1)) as u8 };
    let r = level(index / (PALETTE_LEVELS * PALETTE_LEVELS));
    let g = level((index / PALETTE_LEVELS) % PALETTE_LEVELS);
    let b = level(index % PALETTE_LEVELS);
    (r, g, b)
}

/// Pixel-derived metadata (the web's EXIF read needs the encoded file; from
/// decoded pixels this is everything honestly available).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageMetadata {
    pub width: usize,
    pub height: usize,
    pub has_alpha: bool,
    pub transparent_pixels: usize,
    pub opaque_ratio: f64,
    pub average: Rgba,
    pub megapixels: f64,
}

/// `readImageMeta` + the transparency/average the analysis dialog surfaces.
pub fn read_image_metadata(surface: &Surface) -> ImageMetadata {
    let (width, height) = (surface.width(), surface.height());
    let total = width.saturating_mul(height);
    let mut sums = [0_u64; 4];
    let mut transparent = 0_usize;
    let mut has_alpha = false;
    for y in 0..height {
        for x in 0..width {
            let pixel = surface.get(x as i64, y as i64);
            for (channel, sum) in sums.iter_mut().enumerate() {
                *sum += u64::from(pixel[channel]);
            }
            if pixel[3] < 255 {
                has_alpha = true;
            }
            if pixel[3] == 0 {
                transparent += 1;
            }
        }
    }
    let divisor = total.max(1) as u64;
    let average = [
        (sums[0] / divisor) as u8,
        (sums[1] / divisor) as u8,
        (sums[2] / divisor) as u8,
        (sums[3] / divisor) as u8,
    ];
    let opaque_ratio = if total == 0 {
        0.0
    } else {
        (total - transparent) as f64 / total as f64
    };
    ImageMetadata {
        width,
        height,
        has_alpha,
        transparent_pixels: transparent,
        opaque_ratio,
        average,
        megapixels: total as f64 / 1_000_000.0,
    }
}

/// The key/value rows `canvas-image-analysis-dialog.tsx:94` renders.
pub fn metadata_rows(metadata: &ImageMetadata) -> Vec<(String, String)> {
    vec![
        (
            "Dimensions".to_string(),
            format!("{} x {} px", metadata.width, metadata.height),
        ),
        (
            "Megapixels".to_string(),
            format!("{:.2}", metadata.megapixels),
        ),
        (
            "Transparency".to_string(),
            if metadata.has_alpha { "Yes" } else { "No" }.to_string(),
        ),
        (
            "Opaque ratio".to_string(),
            format!("{:.0}%", metadata.opaque_ratio * 100.0),
        ),
        ("Average colour".to_string(), rgba_to_hex(metadata.average)),
    ]
}

/// `imghash.hashRaw(data, 8)` (image-algorithms.ts:44): the side of the aHash grid.
pub const PERCEPTUAL_HASH_SIZE: usize = 8;

/// `computePerceptualHash` (image-algorithms.ts:38): 8x8 average hash as 16 hex
/// characters. A uniform image hashes to all zero bits.
pub fn perceptual_hash(surface: &Surface) -> String {
    let grays = gray_grid(surface, PERCEPTUAL_HASH_SIZE, PERCEPTUAL_HASH_SIZE);
    let count = grays.len().max(1);
    let mean = grays.iter().sum::<f64>() / count as f64;
    let mut bits = 0_u64;
    for (index, value) in grays.iter().enumerate() {
        if *value > mean {
            bits |= 1_u64 << (63 - index);
        }
    }
    format!("{bits:016x}")
}

fn gray_grid(surface: &Surface, width: usize, height: usize) -> Vec<f64> {
    let resized = surface.resample_bilinear(width, height);
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b, a] = resized.get(x as i64, y as i64);
            out.push(luma(r, g, b) * (f64::from(a) / 255.0));
        }
    }
    out
}

fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)
}

/// A pixel crop rectangle (`SmartCropArea`, image-algorithms.ts:3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// `ratios` (canvas-image-analysis-dialog.tsx:11).
pub const DEFAULT_CROP_RATIOS: [&str; 5] = ["1:1", "4:3", "3:4", "16:9", "9:16"];

/// `parseAspect` (canvas-image-analysis-dialog.tsx:13): `"W:H"` ratio, else 1.
pub fn parse_aspect(value: &str) -> f64 {
    let Some((width, height)) = value.split_once(':') else {
        return 1.0;
    };
    let (Ok(width), Ok(height)) = (width.trim().parse::<f64>(), height.trim().parse::<f64>())
    else {
        return 1.0;
    };
    if width > 0.0 && height > 0.0 {
        width / height
    } else {
        1.0
    }
}

/// `cropBoxForAspect` (image-algorithms.ts:58): the largest in-bounds box of a ratio.
pub fn crop_box_for_aspect(width: i64, height: i64, aspect: f64) -> (i64, i64) {
    if !aspect.is_finite() || aspect <= 0.0 {
        return (width, height);
    }
    let by_width_height = (width as f64 / aspect).round() as i64;
    let by_height_width = (height as f64 * aspect).round() as i64;
    let (box_width, box_height) = if by_width_height <= height {
        (width, by_width_height)
    } else {
        (by_height_width, height)
    };
    (box_width.min(width).max(1), box_height.min(height).max(1))
}

/// `resolveSmartCropArea` (image-algorithms.ts:5): clamp a suggested area into
/// the image and round it.
pub fn resolve_smart_crop_area(
    width: i64,
    height: i64,
    aspect: f64,
    top_crop: CropRect,
) -> CropRect {
    let full_width = width.max(1);
    let full_height = height.max(1);
    if !aspect.is_finite() || aspect <= 0.0 {
        return CropRect {
            x: 0,
            y: 0,
            width: full_width,
            height: full_height,
        };
    }
    let x = top_crop.x.clamp(0, full_width - 1);
    let y = top_crop.y.clamp(0, full_height - 1);
    CropRect {
        x,
        y,
        width: top_crop.width.clamp(1, full_width - x),
        height: top_crop.height.clamp(1, full_height - y),
    }
}

/// Longest-edge budget for the saliency grid used by [`suggest_smart_crop`].
const CROP_GRID_MAX: usize = 64;

/// `suggestSmartCrop` (image-algorithms.ts:48): pick the highest-detail window of
/// the target ratio. The web's `smartcrop` saliency is replaced by a
/// deterministic gradient-energy search over a downscaled grid.
pub fn suggest_smart_crop(surface: &Surface, aspect: f64) -> CropRect {
    let width = surface.width() as i64;
    let height = surface.height() as i64;
    if !aspect.is_finite() || aspect <= 0.0 || width <= 0 || height <= 0 {
        return CropRect {
            x: 0,
            y: 0,
            width: width.max(1),
            height: height.max(1),
        };
    }
    let (crop_width, crop_height) = crop_box_for_aspect(width, height, aspect);
    let top_crop = best_crop_window(surface, crop_width, crop_height);
    resolve_smart_crop_area(width, height, aspect, top_crop)
}

fn best_crop_window(surface: &Surface, crop_width: i64, crop_height: i64) -> CropRect {
    let (width, height) = (surface.width(), surface.height());
    if width == 0 || height == 0 {
        return CropRect {
            x: 0,
            y: 0,
            width: crop_width,
            height: crop_height,
        };
    }
    let factor = width.max(height).div_ceil(CROP_GRID_MAX).max(1);
    let grid_w = width.div_ceil(factor);
    let grid_h = height.div_ceil(factor);
    let grid = surface.resample_bilinear(grid_w, grid_h);
    let stride = grid_w + 1;
    let mut integral = vec![0.0_f64; stride * (grid_h + 1)];
    for y in 0..grid_h {
        for x in 0..grid_w {
            let [r, g, b, _] = grid.get(x as i64, y as i64);
            let mut magnitude = 0.0;
            if x + 1 < grid_w {
                let [r2, g2, b2, _] = grid.get(x as i64 + 1, y as i64);
                magnitude += (luma(r, g, b) - luma(r2, g2, b2)).abs();
            }
            if y + 1 < grid_h {
                let [r2, g2, b2, _] = grid.get(x as i64, y as i64 + 1);
                magnitude += (luma(r, g, b) - luma(r2, g2, b2)).abs();
            }
            integral[(y + 1) * stride + (x + 1)] =
                magnitude + integral[y * stride + (x + 1)] + integral[(y + 1) * stride + x]
                    - integral[y * stride + x];
        }
    }
    let window_w = ((crop_width as usize).div_ceil(factor)).clamp(1, grid_w);
    let window_h = ((crop_height as usize).div_ceil(factor)).clamp(1, grid_h);
    let max_x = grid_w - window_w;
    let max_y = grid_h - window_h;
    let mut best = (0_usize, 0_usize);
    let mut best_score = f64::NEG_INFINITY;
    for y in 0..=max_y {
        for x in 0..=max_x {
            let sum = integral[(y + window_h) * stride + (x + window_w)]
                - integral[y * stride + (x + window_w)]
                - integral[(y + window_h) * stride + x]
                + integral[y * stride + x];
            if sum > best_score {
                best_score = sum;
                best = (x, y);
            }
        }
    }
    CropRect {
        x: ((best.0 * factor) as i64).min((width as i64 - crop_width).max(0)),
        y: ((best.1 * factor) as i64).min((height as i64 - crop_height).max(0)),
        width: crop_width,
        height: crop_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, rgba: Rgba) -> Surface {
        let mut surface = Surface::new(width, height);
        surface.fill(rgba);
        surface
    }

    #[test]
    fn palette_ranks_colours_by_count() {
        let mut surface = Surface::new(2, 2);
        surface.set(0, 0, [255, 0, 0, 255]);
        surface.set(1, 0, [255, 0, 0, 255]);
        surface.set(0, 1, [0, 0, 255, 255]);
        surface.set(1, 1, [0, 0, 0, 0]);
        let palette = extract_palette(&surface, DEFAULT_PALETTE_COUNT);
        assert_eq!(palette.len(), 2);
        assert_eq!(palette[0].hex(), "#ff0000");
        assert_eq!(palette[0].count, 2);
        assert_eq!(palette[1].hex(), "#0000ff");
    }

    #[test]
    fn palette_is_deterministic_and_handles_edges() {
        let surface = solid(2, 2, [255, 255, 255, 255]);
        assert_eq!(extract_palette(&surface, 6), extract_palette(&surface, 6));
        assert_eq!(extract_palette(&surface, 6)[0].hex(), "#ffffff");
        assert!(extract_palette(&surface, 0).is_empty());
        assert!(extract_palette(&Surface::new(0, 0), 6).is_empty());

        let mut sampled = solid(4, 4, [10, 20, 30, 255]);
        sampled.fill([0, 0, 0, 0]);
        assert!(extract_palette(&sampled, 6).is_empty());
    }

    #[test]
    fn palette_one_by_one() {
        let surface = solid(1, 1, [12, 34, 56, 255]);
        let palette = extract_palette(&surface, 6);
        assert_eq!(palette.len(), 1);
        assert_eq!(palette[0].count, 1);
        assert_eq!(palette[0].hex(), "#002233");
    }

    #[test]
    fn metadata_counts_transparency() {
        let mut surface = Surface::new(2, 2);
        surface.set(0, 0, [100, 100, 100, 255]);
        surface.set(1, 0, [100, 100, 100, 255]);
        surface.set(0, 1, [0, 0, 0, 0]);
        surface.set(1, 1, [0, 0, 0, 0]);
        let metadata = read_image_metadata(&surface);
        assert_eq!(metadata.width, 2);
        assert_eq!(metadata.height, 2);
        assert!(metadata.has_alpha);
        assert_eq!(metadata.transparent_pixels, 2);
        assert!((metadata.opaque_ratio - 0.5).abs() < 1e-9);
        assert_eq!(metadata.average, [50, 50, 50, 127]);
        assert_eq!(metadata_rows(&metadata).len(), 5);
    }

    #[test]
    fn metadata_empty_surface_is_all_zero() {
        let metadata = read_image_metadata(&Surface::new(0, 0));
        assert_eq!(metadata.width, 0);
        assert!(!metadata.has_alpha);
        assert_eq!(metadata.opaque_ratio, 0.0);
        assert_eq!(metadata.average, [0, 0, 0, 0]);
    }

    #[test]
    fn hash_is_stable_and_uniform_images_are_zero() {
        let transparent = perceptual_hash(&Surface::new(4, 4));
        assert_eq!(transparent, "0000000000000000");
        let white = perceptual_hash(&solid(3, 3, [255, 255, 255, 255]));
        assert_eq!(white, "0000000000000000");
        let split = {
            let mut surface = Surface::new(4, 4);
            surface.fill([0, 0, 0, 255]);
            for y in 0..4 {
                for x in 2..4 {
                    surface.set(x, y, [255, 255, 255, 255]);
                }
            }
            surface
        };
        let first = perceptual_hash(&split);
        assert_eq!(first.len(), 16);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, "0000000000000000");
        assert_eq!(first, perceptual_hash(&split));
    }

    #[test]
    fn parse_aspect_reads_ratios() {
        assert!((parse_aspect("16:9") - 16.0 / 9.0).abs() < 1e-9);
        assert_eq!(parse_aspect("1:1"), 1.0);
        assert_eq!(parse_aspect("junk"), 1.0);
        assert_eq!(parse_aspect("0:0"), 1.0);
    }

    #[test]
    fn crop_box_picks_the_in_bounds_dimension() {
        assert_eq!(crop_box_for_aspect(100, 50, 1.0), (50, 50));
        assert_eq!(crop_box_for_aspect(50, 100, 1.0), (50, 50));
        assert_eq!(crop_box_for_aspect(100, 100, 0.0), (100, 100));
    }

    #[test]
    fn resolve_area_clamps_into_bounds() {
        let resolved = resolve_smart_crop_area(
            100,
            50,
            1.0,
            CropRect {
                x: 200,
                y: -5,
                width: 500,
                height: 500,
            },
        );
        assert_eq!(resolved.y, 0);
        assert_eq!(resolved.x, 99);
        assert_eq!(resolved.width, 1);
        assert_eq!(resolved.height, 50);
    }

    #[test]
    fn smart_crop_is_deterministic_and_bounded() {
        let mut surface = solid(100, 50, [10, 10, 10, 255]);
        for y in 10..20 {
            for x in 40..60 {
                surface.set(x, y, [255, 0, 0, 255]);
            }
        }
        let crop = suggest_smart_crop(&surface, 1.0);
        assert_eq!(crop, suggest_smart_crop(&surface, 1.0));
        assert_eq!(crop.width, 50);
        assert_eq!(crop.height, 50);
        assert_eq!(crop.y, 0);
        assert!((0..=50).contains(&crop.x));
    }

    #[test]
    fn smart_crop_edge_cases() {
        let one = suggest_smart_crop(&solid(1, 1, [5, 5, 5, 255]), 1.0);
        assert_eq!(
            one,
            CropRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            }
        );
        let full = suggest_smart_crop(&Surface::new(8, 8), 0.0);
        assert_eq!(
            full,
            CropRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8
            }
        );
        let transparent = suggest_smart_crop(&Surface::new(100, 50), 1.0);
        assert_eq!(
            transparent,
            CropRect {
                x: 0,
                y: 0,
                width: 50,
                height: 50
            }
        );
    }
}
