//! Pre-rotated ruler labels: text rasterised by `ab_glyph` and rotated -90° in
//! the bitmap, so Slint draws a plain upright `Image` instead of a rotated
//! `Text`.
//!
//! Slint re-samples (and therefore re-rasterises) any rotated `Text` whenever
//! its box lands on a fractional position, which is exactly what happens while
//! the canvas pans: the glyph coverage changes every frame and the numbers look
//! soft and thickened. A bitmap that is already rotated has no transform, so the
//! renderer blits it at a whole-pixel origin and the glyphs stay as crisp as the
//! rasteriser produced them.
//!
//! One [`slint::Image`] is cached per distinct label (plus ink and scale), so a
//! panning canvas only rebuilds the model rows, never the pixels.

use std::cell::RefCell;
use std::collections::HashMap;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};

/// Logical font size of a ruler label (`font-size: 10px` in the `.slint`).
pub const LABEL_PX: f32 = 10.0;
/// Width of the vertical ruler strip (`ruler-left-width` in the `.slint`).
pub const STRIP_WIDTH: f64 = 26.0;
/// Line box height as a multiple of the font size, matching the old `14px` box.
const LINE_RATIO: f32 = 1.4;
/// Horizontal breathing room around the ink, in physical pixels.
const PAD: u32 = 1;

/// The rasterised label: a bitmap already rotated counter-clockwise (the
/// numbers read bottom-to-top) plus its logical size.
#[derive(Debug, Clone)]
pub struct RasterLabel {
    pub image: Image,
    /// Logical width/height: the physical bitmap divided by the display scale.
    pub width: f32,
    pub height: f32,
}

/// The process-wide face: Segoe UI (the Windows UI face Slint's default text
/// uses) when present, else the app's embedded face. Loaded once.
fn font() -> Option<&'static FontArc> {
    static FONT: std::sync::OnceLock<Option<FontArc>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        #[cfg(windows)]
        {
            for candidate in [
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\segoeui.ttc",
            ] {
                if let Ok(bytes) = std::fs::read(candidate) {
                    if let Ok(font) = FontArc::try_from_vec(bytes) {
                        return Some(font);
                    }
                }
            }
        }
        FontArc::try_from_slice(include_bytes!("../assets/fonts/Baloo2-Variable.ttf")).ok()
    })
    .as_ref()
}

/// Rasterise `label` horizontally at `10px * scale`, then rotate it -90° into
/// the output bitmap. Pure once the font is loaded, so the tests can compare
/// two runs byte for byte.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "raster coordinates are small non-negative pixel counts, far below 2^24"
)]
pub fn rasterize(label: &str, ink: Color, scale: f32) -> (SharedPixelBuffer<Rgba8Pixel>, u32, u32) {
    let scale = if scale.is_finite() && scale > 0.1 {
        scale
    } else {
        1.0
    };
    let px = LABEL_PX * scale;
    let Some(font) = font() else {
        let mut empty = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        empty.make_mut_bytes()[3] = 0;
        return (empty, 1, 1);
    };
    let scaled = font.as_scaled(PxScale::from(px));
    let ascent = scaled.ascent();
    let descent = scaled.descent();
    let line_h = ((ascent - descent) * 1.05)
        .ceil()
        .max(LINE_RATIO * px)
        .ceil();
    let mut advance = 0.0f32;
    for ch in label.chars() {
        advance += scaled.h_advance(scaled.glyph_id(ch));
    }
    let ink_w = advance.ceil();
    let text_w = (ink_w + (PAD * 2) as f32).max(1.0) as u32;
    let text_h = (line_h + (PAD * 2) as f32).max(1.0) as u32;

    // Horizontal pass.
    let mut flat = SharedPixelBuffer::<Rgba8Pixel>::new(text_w, text_h);
    let baseline = ascent + PAD as f32;
    {
        let bytes = flat.make_mut_bytes();
        let mut pen = PAD as f32;
        let (r, g, b, a) = (ink.red(), ink.green(), ink.blue(), ink.alpha());
        for ch in label.chars() {
            let glyph = scaled
                .glyph_id(ch)
                .with_scale_and_position(PxScale::from(px), ab_glyph::point(pen, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
                outlined.draw(|x, y, coverage| {
                    let dx = x as i32 + ox;
                    let dy = y as i32 + oy;
                    if dx < 0 || dy < 0 || dx >= text_w as i32 || dy >= text_h as i32 {
                        return;
                    }
                    let index = (dy as usize * text_w as usize + dx as usize) * 4;
                    let alpha = (coverage.clamp(0.0, 1.0) * f32::from(a)).round() as u8;
                    let Some(pixel) = bytes.get_mut(index..index + 4) else {
                        return;
                    };
                    pixel[0] = r;
                    pixel[1] = g;
                    pixel[2] = b;
                    pixel[3] = alpha;
                });
            }
            pen += scaled.h_advance(scaled.glyph_id(ch));
        }
    }

    // Rotate -90° (counter-clockwise): the label starts at the bottom and reads
    // upwards, so it can sit unrotated in the 26px strip. The source's
    // left-to-right direction becomes bottom-to-top, so dst(x, y) reads
    // src(text_w - 1 - y, x).
    let dst_w = text_h;
    let dst_h = text_w;
    let mut rotated = SharedPixelBuffer::<Rgba8Pixel>::new(dst_w, dst_h);
    {
        let src = flat.as_bytes();
        let dst = rotated.make_mut_bytes();
        for y in 0..dst_h {
            for x in 0..dst_w {
                let sx = text_w - 1 - y;
                let sy = x;
                let si = (sy as usize * text_w as usize + sx as usize) * 4;
                let di = (y as usize * dst_w as usize + x as usize) * 4;
                let Some(s) = src.get(si..si + 4) else {
                    continue;
                };
                if let Some(d) = dst.get_mut(di..di + 4) {
                    d.copy_from_slice(s);
                }
            }
        }
    }
    (rotated, dst_w, dst_h)
}

/// The cached, ready-to-draw version of [`rasterize`]: one `Image` per distinct
/// label/ink/scale. Slint's `Image` is a shared handle, so clones are free.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "the scale is a small display factor and the bitmap size is small; every cast is exact"
)]
pub fn label_image(label: &str, ink: Color, scale: f32) -> RasterLabel {
    type Key = (String, u32, u8, u8, u8, u8);
    // Slint's `Image` is UI-thread bound, so the cache is thread-local: every
    // caller here is the UI thread and the values never cross a thread.
    thread_local! {
        static CACHE: RefCell<HashMap<Key, RasterLabel>> = RefCell::new(HashMap::new());
    }
    let scale = if scale.is_finite() && scale > 0.1 {
        scale
    } else {
        1.0
    };
    let key: Key = (
        label.to_string(),
        (scale * 100.0).round() as u32,
        ink.red(),
        ink.green(),
        ink.blue(),
        ink.alpha(),
    );
    if let Some(hit) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let (buffer, width, height) = rasterize(label, ink, scale);
    let raster = RasterLabel {
        image: Image::from_rgba8(buffer),
        width: width as f32 / scale,
        height: height as f32 / scale,
    };
    CACHE.with(|cache| {
        cache.borrow_mut().insert(key, raster.clone());
    });
    raster
}

/// Whole-pixel placement of a pre-rotated label: the box is centred on the
/// strip and its bottom end lands on the tick.
///
/// Rounding here is what keeps the label crisp: the bitmap itself never
/// changes, and a fractional tick only moves the box by whole pixels, so the
/// renderer never resamples it.
pub fn place_label(
    tick_pos: f64,
    strip_width: f64,
    label_width: f32,
    label_height: f32,
) -> (f64, f64) {
    let x = ((strip_width - f64::from(label_width)) / 2.0).round();
    let y = (tick_pos - f64::from(label_height)).round();
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a 10px label's bitmap is a few dozen pixels, exact in f32"
    )]
    fn labels_are_crisp_at_fractional_tick_positions() {
        let ink = Color::from_rgb_u8(120, 120, 120);
        let (first_raster, width, height) = rasterize("100", ink, 1.0);
        assert!(
            width > 1 && height > 1,
            "the label has a bitmap ({width}x{height})"
        );
        assert!(
            first_raster.as_slice().iter().any(|pixel| pixel.a > 0),
            "the glyphs put ink in the bitmap"
        );

        // The same label rasterises byte-identically, so a tick can never
        // change the glyph coverage.
        let (second_raster, _, _) = rasterize("100", ink, 1.0);
        assert_eq!(
            first_raster.as_slice(),
            second_raster.as_slice(),
            "the raster is stable"
        );

        // Fractional tick positions produce whole-pixel placements, and the box
        // size (and therefore the bitmap) is unchanged.
        let logical_w = width as f32;
        let logical_h = height as f32;
        let start = place_label(100.0, 26.0, logical_w, logical_h);
        let middle = place_label(100.37, 26.0, logical_w, logical_h);
        let end = place_label(100.94, 26.0, logical_w, logical_h);
        for (x, y) in [start, middle, end] {
            assert_eq!(x, x.round(), "x is a whole pixel");
            assert_eq!(y, y.round(), "y is a whole pixel");
        }
        assert_eq!(start.0, middle.0);
        assert_eq!(start.0, end.0);
        assert!(
            (start.1 - middle.1).abs() <= 1.0,
            "a sub-pixel tick moves at most one pixel"
        );
    }

    #[test]
    fn the_rotation_reads_bottom_to_top() {
        let ink = Color::from_rgb_u8(255, 255, 255);
        let (buffer, w, h) = rasterize("1000", ink, 1.0);
        assert!(
            h > w,
            "the rotated long label is taller than it is wide ({w}x{h})"
        );
        // Ink concentrated at the bottom half, because the single glyph starts
        // at the bottom and reads upwards.
        let mut top = 0u32;
        let mut bottom = 0u32;
        for (index, pixel) in buffer.as_slice().iter().enumerate() {
            if pixel.a == 0 {
                continue;
            }
            let y = index / w as usize;
            if y < (h / 2) as usize {
                top += 1;
            } else {
                bottom += 1;
            }
        }
        assert!(
            bottom >= top,
            "ink sits in the lower half ({top} vs {bottom})"
        );
    }

    #[test]
    fn different_labels_get_different_bitmaps() {
        let ink = Color::from_rgb_u8(10, 20, 30);
        let (a, _, _) = rasterize("100", ink, 1.0);
        let (b, _, _) = rasterize("-200", ink, 1.0);
        assert_ne!(a.as_slice(), b.as_slice());
    }
}
