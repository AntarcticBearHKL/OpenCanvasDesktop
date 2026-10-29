//! The pre-rendered dot-grid tile.
//!
//! The canvas background used to be one `Path` holding a near-zero-length
//! segment per world-grid intersection (thousands of `M x y l …` subpaths).
//! Slint re-tessellates that path whenever the viewbox moves, i.e. on **every**
//! pan and zoom frame, and that tessellation was the dominant cost of a canvas
//! frame. Here the lattice is rasterised **once** into a small bitmap holding
//! `dots x dots` dots, which the `.slint` side repeats with a lattice of
//! `Image` elements whose offsets are `vp-x/vp-y` modulo the tile. A pan
//! therefore only moves the lattice (pure `.slint` bindings, no Rust work, no
//! re-tessellation) and a zoom does not touch the tile at all.
//!
//! The grid is **screen-anchored**: the dot spacing is the constant
//! [`crate::paths::GRID_SCREEN_SPACING`] and the dot count per tile is
//! [`crate::paths::GRID_TILE_DOTS`], so zooming changes neither the bitmap, nor
//! the dot size, nor the distance between dots. The key deliberately has no
//! viewport in it: only a theme change (ink) or a device-scale change (DPI) can
//! produce a different bitmap.
//!
//! The dots are drawn exactly like the path drew them: a circle of
//! [`GRID_DOT_DIAMETER`] logical pixels in `Theme.canvas-dot`, at
//! [`GRID_LAYER_OPACITY`] (the `opacity: 0.65` the old path layer carried).
//! The layer opacity is baked into the bitmap alpha instead of being applied
//! to each of the dozen `Image` elements, which composites to the same pixels.

use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::paths::GridTileLayout;

/// Dot diameter in **screen** pixels (`stroke-width: 2.5px` on the old path;
/// the visible dot was the round cap of that stroke).
pub const GRID_DOT_DIAMETER: f64 = 2.5;
/// Opacity the old grid path layer drew at (`opacity: 0.65` in `app.slint`).
pub const GRID_LAYER_OPACITY: f64 = 0.65;

/// Everything a tile bitmap depends on. Two viewports that produce an equal key
/// draw from the same bitmap, and since the tile geometry is a screen-space
/// constant the key only changes with the theme or the device scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridTileKey {
    /// Dots per tile axis.
    pub dots: u32,
    /// Bitmap edge, in physical pixels.
    pub image_px: u32,
    /// The dot ink (`Theme.canvas-dot`) as straight RGBA.
    pub ink: [u8; 4],
}

impl GridTileKey {
    /// The key for a layout at a device scale and ink colour.
    pub fn of(layout: &GridTileLayout, device_scale: f64, ink: Color) -> Self {
        Self {
            dots: layout.dots,
            image_px: layout.image_px(device_scale),
            ink: [ink.red(), ink.green(), ink.blue(), ink.alpha()],
        }
    }
}

/// A rasterised tile plus the key it was built for.
#[derive(Debug, Clone)]
pub struct GridTile {
    pub key: GridTileKey,
    pub image: Image,
}

impl GridTile {
    /// Rasterises the tile. Returns `None` for a degenerate layout (no dots or
    /// a zero-sized bitmap), so a bad viewport can never panic the UI thread.
    pub fn build(layout: &GridTileLayout, device_scale: f64, ink: Color) -> Option<Self> {
        let key = GridTileKey::of(layout, device_scale, ink);
        let buffer = rasterize(layout, key)?;
        Some(Self {
            key,
            // Premultiplied: the tile is drawn at a fractional pan offset and
            // interpolating straight RGBA against the transparent gaps would
            // dim and shrink every dot (the classic dark-fringe filter bug).
            // Premultiplied sampling preserves the dot's ink.
            image: Image::from_rgba8_premultiplied(buffer),
        })
    }
}

/// One tile: `dots x dots` anti-aliased dots, one per cell, centred in its cell.
///
/// Cell `(col, row)` spans `[i * cell, (i + 1) * cell)` pixels, so the lattice
/// is seamless: the `.slint` side places the bitmap at
/// `(round(vp) - spacing / 2) mod tile`, which puts a dot's centre on the
/// canvas origin and every later dot exactly one `spacing` later.
pub fn rasterize(
    layout: &GridTileLayout,
    key: GridTileKey,
) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let edge = key.image_px;
    if edge == 0 || key.dots == 0 {
        return None;
    }
    if !layout.spacing.is_finite() || layout.spacing <= 0.0 {
        return None;
    }
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(edge, edge);
    // Pixels per logical pixel: the bitmap is `dots * spacing` logical units
    // wide and `edge` physical pixels across, i.e. the device scale (with at
    // most a rounding of the bitmap edge, well under 0.2 %).
    let px_per_unit = f64::from(edge) / (f64::from(key.dots) * layout.spacing);
    let cell = layout.spacing * px_per_unit;
    let radius = GRID_DOT_DIAMETER / 2.0 * px_per_unit;
    let alpha = f64::from(key.ink[3]) / 255.0 * GRID_LAYER_OPACITY;
    let bytes = buffer.make_mut_bytes();
    for row in 0..key.dots {
        for col in 0..key.dots {
            let cx = (f64::from(col) + 0.5) * cell;
            let cy = (f64::from(row) + 0.5) * cell;
            fill_dot(bytes, edge, cx, cy, radius, key.ink, alpha);
        }
    }
    Some(buffer)
}

/// Blends one anti-aliased dot into a **premultiplied** RGBA buffer.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the sampled box is a few pixels around a dot, clamped to the bitmap edge"
)]
fn fill_dot(bytes: &mut [u8], edge: u32, cx: f64, cy: f64, radius: f64, ink: [u8; 4], alpha: f64) {
    // One pixel of AA around the circle, clipped to the bitmap.
    let lo_x = (cx - radius - 1.0).floor().max(0.0) as u32;
    let hi_x = ((cx + radius + 1.0).ceil() as u32).min(edge.saturating_sub(1));
    let lo_y = (cy - radius - 1.0).floor().max(0.0) as u32;
    let hi_y = ((cy + radius + 1.0).ceil() as u32).min(edge.saturating_sub(1));
    for y in lo_y..=hi_y {
        for x in lo_x..=hi_x {
            let dx = f64::from(x) + 0.5 - cx;
            let dy = f64::from(y) + 0.5 - cy;
            let coverage = (radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let index = ((y * edge + x) * 4) as usize;
            let Some(pixel) = bytes.get_mut(index..index + 4) else {
                continue;
            };
            let a = (alpha * coverage * 255.0).round() as u8;
            // Premultiplied ink: sampling the tile keeps the dot's energy
            // instead of bleeding it towards the transparent gaps.
            pixel[0] = premultiply(ink[0], a);
            pixel[1] = premultiply(ink[1], a);
            pixel[2] = premultiply(ink[2], a);
            pixel[3] = a;
        }
    }
}

/// One straight-alpha channel scaled by `alpha` (premultiplication).
#[expect(
    clippy::cast_possible_truncation,
    reason = "the product is clamped to 0..=255 before the cast"
)]
fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{GRID_SCREEN_SPACING, GRID_TILE_DOTS};

    fn layout() -> GridTileLayout {
        crate::paths::grid_tile_layout(1040.0, 744.0)
    }

    /// Alpha-weighted centroid of the dot inside one cell, in pixels.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "cell coordinates come from a tiny tile bitmap and are clamped to it"
    )]
    fn centroid(
        buffer: &SharedPixelBuffer<Rgba8Pixel>,
        cell: f64,
        col: u32,
        row: u32,
    ) -> (f64, f64) {
        let edge = buffer.width();
        let (mut sum, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64);
        let lo_x = (f64::from(col) * cell).floor().max(0.0) as u32;
        let hi_x = (((f64::from(col) + 1.0) * cell).ceil() as u32).min(edge);
        let lo_y = (f64::from(row) * cell).floor().max(0.0) as u32;
        let hi_y = (((f64::from(row) + 1.0) * cell).ceil() as u32).min(edge);
        for y in lo_y..hi_y {
            for x in lo_x..hi_x {
                let a = f64::from(buffer.as_slice()[(y * edge + x) as usize].a);
                if a == 0.0 {
                    continue;
                }
                sum += a;
                sx += a * (f64::from(x) + 0.5);
                sy += a * (f64::from(y) + 0.5);
            }
        }
        if sum == 0.0 {
            return (f64::NAN, f64::NAN);
        }
        (sx / sum, sy / sum)
    }

    fn ink_alpha(buffer: &SharedPixelBuffer<Rgba8Pixel>) -> u8 {
        buffer
            .as_slice()
            .iter()
            .map(|pixel| pixel.a)
            .max()
            .unwrap_or(0)
    }

    /// Alpha-weighted area of one dot in the cell, in **pixels²** (physical):
    /// `Σ alpha / peak alpha` is the integral of the anti-aliased coverage.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "cell coordinates come from a tiny tile bitmap and are clamped to it"
    )]
    fn dot_area(buffer: &SharedPixelBuffer<Rgba8Pixel>, cell: f64, col: u32, row: u32) -> f64 {
        let edge = buffer.width();
        let lo_x = (f64::from(col) * cell).floor().max(0.0) as u32;
        let hi_x = (((f64::from(col) + 1.0) * cell).ceil() as u32).min(edge);
        let lo_y = (f64::from(row) * cell).floor().max(0.0) as u32;
        let hi_y = (((f64::from(row) + 1.0) * cell).ceil() as u32).min(edge);
        let mut sum = 0.0f64;
        let mut peak = 0u8;
        for y in lo_y..hi_y {
            for x in lo_x..hi_x {
                let a = buffer.as_slice()[(y * edge + x) as usize].a;
                sum += f64::from(a);
                peak = peak.max(a);
            }
        }
        if peak == 0 {
            return f64::NAN;
        }
        sum / f64::from(peak)
    }

    #[test]
    fn every_dot_sits_in_the_centre_of_its_cell() {
        let layout = layout();
        let ink = Color::from_argb_f32(0.4, 0.2, 0.2, 0.2);
        let key = GridTileKey::of(&layout, 1.0, ink);
        let buffer = rasterize(&layout, key).expect("tile");
        let cell = f64::from(key.image_px) / f64::from(key.dots);
        assert_eq!(cell, GRID_SCREEN_SPACING, "one cell per screen spacing");
        for row in 0..key.dots {
            for col in 0..key.dots {
                let (x, y) = centroid(&buffer, cell, col, row);
                let expected_x = (f64::from(col) + 0.5) * cell;
                let expected_y = (f64::from(row) + 0.5) * cell;
                assert!(
                    (x - expected_x).abs() < 0.4 && (y - expected_y).abs() < 0.4,
                    "dot ({col},{row}) is at ({x:.2}, {y:.2}), expected ({expected_x:.2}, {expected_y:.2})"
                );
            }
        }
    }

    #[test]
    fn the_tile_ink_matches_the_dot_colour_and_layer_opacity() {
        let layout = layout();
        // Theme.canvas-dot is rgba(58, 48, 40, 0.40) in the light theme.
        let ink = Color::from_argb_f32(0.40, 58.0 / 255.0, 48.0 / 255.0, 40.0 / 255.0);
        let key = GridTileKey::of(&layout, 1.0, ink);
        let buffer = rasterize(&layout, key).expect("tile");
        let peak = f64::from(ink_alpha(&buffer)) / 255.0;
        let expected = 0.40 * GRID_LAYER_OPACITY;
        assert!(
            (peak - expected).abs() < 0.02,
            "peak alpha {peak:.3}, expected {expected:.3} (dot ink * layer opacity)"
        );
        // The buffer is premultiplied, so the painted channels are the dot
        // colour scaled by that alpha (the renderer's interpolation relies on
        // it; see `GridTile::build`).
        let painted = buffer
            .as_slice()
            .iter()
            .max_by_key(|pixel| pixel.a)
            .expect("the tile paints ink");
        let scale = f64::from(painted.a) / 255.0;
        for (channel, ink) in [(painted.r, 58u8), (painted.g, 48), (painted.b, 40)] {
            let expected = f64::from(ink) * scale;
            assert!(
                (f64::from(channel) - expected).abs() <= 1.5,
                "channel {channel} is the dot ink {ink} premultiplied by {scale:.3} ({expected:.1})"
            );
        }
    }

    /// The whole point of a screen-anchored grid: sweeping the zoom from 100 %
    /// to 500 % never produces a second bitmap, and the layout stays the same.
    #[test]
    fn a_five_x_zoom_sweep_never_rebuilds_the_tile() {
        let ink = Color::from_argb_f32(0.4, 0.5, 0.5, 0.5);
        let base_layout = layout();
        let base = GridTileKey::of(&base_layout, 1.0, ink);
        let base_buffer = rasterize(&base_layout, base).expect("tile");
        // The key is derived from the tile geometry and the device scale only;
        // the viewport zoom is not an input. Walk 1x -> 5x in equal steps.
        for step in 0..=80 {
            let k = 5.0_f64.powf(f64::from(step) / 80.0);
            let swept = layout();
            assert_eq!(
                GridTileKey::of(&swept, 1.0, ink),
                base,
                "the tile key is unchanged at {k:.3}x"
            );
            assert_eq!(swept.spacing, GRID_SCREEN_SPACING, "spacing at {k:.3}x");
            assert_eq!(swept.dots, GRID_TILE_DOTS, "dots per tile at {k:.3}x");
        }
        let rebuilt =
            rasterize(&base_layout, GridTileKey::of(&base_layout, 1.0, ink)).expect("tile");
        assert_eq!(
            rebuilt.as_slice(),
            base_buffer.as_slice(),
            "the bitmap is byte-identical: the zoom never re-rasterises it"
        );
        // A theme change and a DPI change are the only rebuild triggers.
        assert_ne!(
            GridTileKey::of(&base_layout, 1.0, Color::from_rgb_u8(1, 2, 3)),
            base,
            "a theme change re-rasterises"
        );
        assert_ne!(
            GridTileKey::of(&base_layout, 2.0, ink),
            base,
            "a new device scale re-rasterises"
        );
    }

    /// The dot is 2.5 logical px at every device scale, in a tile whose size
    /// never depends on the canvas or the zoom.
    #[test]
    fn the_dot_size_is_a_screen_constant() {
        let ink = Color::from_rgb_u8(0, 0, 0);
        let radius = GRID_DOT_DIAMETER / 2.0;
        // Integral of the anti-aliased coverage disc: π(r - .5)² + the AA
        // ramp from r - .5 to r + .5, i.e. ~5.17 px² for r = 1.25.
        let expected = std::f64::consts::PI * (radius - 0.5).powi(2)
            + std::f64::consts::PI * ((radius + 0.5).powi(2) - (radius - 0.5).powi(2)) / 2.0;
        let mut areas = Vec::new();
        for scale in [1.0, 2.0] {
            let layout = layout();
            let key = GridTileKey::of(&layout, scale, ink);
            let buffer = rasterize(&layout, key).expect("tile");
            let cell = f64::from(key.image_px) / f64::from(key.dots);
            // Physical px² -> logical px².
            let area = dot_area(&buffer, cell, 2, 3) / (scale * scale);
            assert!(
                (area - expected).abs() < 1.0,
                "{scale}x: dot covers {area:.2} logical px², expected ~{expected:.2}"
            );
            areas.push(area);
        }
        assert!(
            (areas[0] - areas[1]).abs() < 0.4,
            "the dot is the same size at 1x and 2x ({areas:?})"
        );
        // Independent of the canvas: the bitmap geometry is a screen constant.
        let big = crate::paths::grid_tile_layout(3840.0, 2160.0);
        let small = crate::paths::grid_tile_layout(320.0, 240.0);
        assert_eq!(big.image_px(2.0), small.image_px(2.0));
        assert_eq!(big.spacing, small.spacing);
        assert_eq!(big.tile, small.tile);
    }

    /// The bitmap is small at every device scale: 520 px at the common 2x, and
    /// capped by the layout rather than by the canvas or the zoom.
    #[test]
    fn the_bitmap_is_small_at_every_device_scale() {
        let layout = layout();
        // 16 dots of 26 px at 2x.
        assert_eq!(layout.image_px(2.0), 832);
        for scale in [0.5, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0] {
            let key = GridTileKey::of(&layout, scale, Color::from_rgb_u8(0, 0, 0));
            assert!(
                (8..=1536).contains(&key.image_px),
                "{scale}x: {} px tile",
                key.image_px
            );
        }
    }

    #[test]
    fn a_degenerate_layout_never_panics() {
        let ink = Color::from_rgb_u8(0, 0, 0);
        let mut broken = layout();
        broken.dots = 0;
        assert!(rasterize(&broken, GridTileKey::of(&broken, 1.0, ink)).is_none());
        let zero = GridTileLayout {
            spacing: 0.0,
            ..GridTileLayout::default()
        };
        // A zero spacing is rejected instead of dividing by zero into a
        // NaN-sized bitmap.
        let key = GridTileKey::of(&zero, 1.0, ink);
        assert!(rasterize(&zero, key).is_none());
    }

    /// The Rust side and the `.slint` lattice must agree on the anti-shimmer
    /// rule: the phase is the pan rounded to a whole pixel, modulo the tile,
    /// and never scaled by the zoom.
    #[test]
    fn the_slint_lattice_places_the_tile_on_whole_pixels() {
        let source = include_str!("../ui/app.slint");
        for (axis, tile_axis) in [("x", "w"), ("y", "h")] {
            let prefix = format!("out property <float> grid-phase-{axis}:");
            let line = source
                .lines()
                .find(|line| line.trim_start().starts_with(&prefix))
                .unwrap_or_else(|| panic!("app.slint declares {prefix}"));
            assert!(
                line.contains("Math.round"),
                "the {axis} phase snaps to a whole pixel: {line}"
            );
            assert!(
                line.contains(&format!("root.vp-{axis} / 1px")),
                "the {axis} phase follows the pan: {line}"
            );
            assert!(
                line.contains(&format!("root.grid-tile-{tile_axis}")),
                "the {axis} phase wraps on the tile edge: {line}"
            );
            assert!(
                !line.contains("root.scale"),
                "the {axis} phase must not scale with the zoom: {line}"
            );
        }
        assert!(
            source.contains("property <float> grid-step: 26;"),
            "the grid spacing default is the fixed screen spacing"
        );
    }
}
