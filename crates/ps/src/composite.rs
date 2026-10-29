//! Straight-alpha source-over compositing with a blend mode — the one pixel
//! formula shared by the board compositor and the layer-style renderer,
//! replacing Canvas2D's `globalAlpha` + `globalCompositeOperation`.

use crate::blend::{blend_rgb, BlendMode};
use crate::surface::{clamp_u8, Surface};

/// Blend one straight-alpha pixel under `opacity` (0..1) with `mode`.
pub fn blend_pixel(backdrop: [u8; 4], source: [u8; 4], opacity: f64, mode: BlendMode) -> [u8; 4] {
    let as_ = f64::from(source[3]) / 255.0 * opacity.clamp(0.0, 1.0);
    if as_ <= 0.0 {
        return backdrop;
    }
    let ab = f64::from(backdrop[3]) / 255.0;
    let out_alpha = as_ + ab * (1.0 - as_);
    if out_alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    let cs = [
        f64::from(source[0]) / 255.0,
        f64::from(source[1]) / 255.0,
        f64::from(source[2]) / 255.0,
    ];
    let cb = [
        f64::from(backdrop[0]) / 255.0,
        f64::from(backdrop[1]) / 255.0,
        f64::from(backdrop[2]) / 255.0,
    ];
    let blended = blend_rgb(mode, cb, cs);
    let mut out = [0_u8; 4];
    for channel in 0..3 {
        let premultiplied = (1.0 - ab) * as_ * cs[channel]
            + (1.0 - as_) * ab * cb[channel]
            + as_ * ab * blended[channel];
        out[channel] = clamp_u8(premultiplied / out_alpha * 255.0);
    }
    out[3] = clamp_u8(out_alpha * 255.0);
    out
}

/// Composite `source` onto `dest` at the same size under `mode`/`opacity`.
pub fn draw_surface(dest: &mut Surface, source: &Surface, mode: BlendMode, opacity: f64) {
    if dest.width() != source.width() || dest.height() != source.height() {
        return;
    }
    let width = dest.width();
    for index in (0..dest.len()).step_by(4) {
        let pixel = index / 4;
        let x = pixel % width;
        let y = pixel / width;
        let backdrop = dest.get(x as i64, y as i64);
        let top = source.get(x as i64, y as i64);
        let out = blend_pixel(backdrop, top, opacity, mode);
        dest.set(x as i64, y as i64, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_source_replaces_backdrop() {
        let out = blend_pixel([10, 10, 10, 255], [200, 0, 0, 255], 1.0, BlendMode::Normal);
        assert_eq!(out, [200, 0, 0, 255]);
    }

    #[test]
    fn transparent_backdrop_keeps_source() {
        let out = blend_pixel([0, 0, 0, 0], [200, 100, 50, 128], 1.0, BlendMode::Normal);
        assert_eq!(out[0], 200);
        assert_eq!(out[3], 128);
    }

    #[test]
    fn half_opacity_mixes() {
        let out = blend_pixel([0, 0, 0, 255], [255, 255, 255, 255], 0.5, BlendMode::Normal);
        assert_eq!(out, [128, 128, 128, 255]);
    }

    #[test]
    fn multiply_blend_of_white_is_identity() {
        let out = blend_pixel(
            [100, 50, 20, 255],
            [255, 255, 255, 255],
            1.0,
            BlendMode::Multiply,
        );
        assert_eq!(out, [100, 50, 20, 255]);
    }
}
