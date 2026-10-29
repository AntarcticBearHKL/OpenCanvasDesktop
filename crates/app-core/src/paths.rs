//! World-space path builders: the dot grid, committed wires and snap guides.
//!
//! These carry **world** coordinates and the `.slint` side maps them with a
//! shared viewbox bound to the viewport, so stroke widths stay constant in
//! screen pixels (femtovg strokes after fitting) and pan/zoom never rebuilds
//! the command strings.

use open_canvas_core::connections::connection_geometry;
use open_canvas_core::geometry::{node_bounds, SnapGuides};
use open_canvas_core::node::{CanvasConnection, CanvasNodeData};

use crate::coords::{Viewport, WorldPos};

/// World grid step (`GRID_SIZE`, open-canvas.tsx:9). Only the legacy
/// world-space `grid_patch` fallback (`OC_LEGACY_GRID=1`) uses it; the shipped
/// tile grid is screen-anchored.
pub const GRID_STEP: f64 = 48.0;
/// Extra world margin around the viewport when generating grid dots.
const GRID_MARGIN_SCREENS: f64 = 0.5;
/// Grid dots may not be denser than this on screen; below it the stride doubles.
const GRID_MIN_SCREEN_STEP: f64 = 12.0;
/// Hard cap on generated dots (safety net for the stride loop).
const GRID_DOT_CAP: f64 = 8000.0;

/// Spacing of the dot grid **on screen**, in logical pixels.
///
/// This is a screen-space constant: the tiled background is anchored to the
/// screen, so zooming never changes the spacing, the dot size or the tile
/// bitmap. Panning only slides the lattice (`grid_phase`).
pub const GRID_SCREEN_SPACING: f64 = 26.0;
/// Dots per tile axis. With [`GRID_SCREEN_SPACING`] this fixes the tile bitmap
/// for the whole run: only a theme (ink) or `HiDPI` (device scale) change can
/// rebuild it.
pub const GRID_TILE_DOTS: u32 = 16;
/// Length of a dot's segment stub in **screen** pixels. The visible dot is the
/// round cap of the 2.5px stroke; the stub only has to keep the segment from
/// collapsing into a degenerate (unpainted) subpath.
const DOT_STUB_PX: f64 = 0.25;
/// Snap guides extend the node bounds by 400 world units (project.tsx:1872).
const GUIDE_PADDING: f64 = 400.0;

/// Concatenated world-space cubic beziers for every connection
/// (`connections::connection_geometry` per wire, canvas-connections.tsx:25-32).
pub fn wires_commands(nodes: &[CanvasNodeData], connections: &[CanvasConnection]) -> String {
    // One id -> node index for the whole pass: each wire used to scan the node
    // list twice, which is quadratic on a busy board.
    let mut by_id: std::collections::HashMap<&str, &CanvasNodeData> =
        std::collections::HashMap::with_capacity(nodes.len());
    for node in nodes {
        // First occurrence wins, like the `iter().find()` this replaces.
        by_id.entry(node.id.as_str()).or_insert(node);
    }
    let mut out = String::new();
    for connection in connections {
        let (Some(from), Some(to)) = (
            by_id.get(connection.from_node_id.as_str()),
            by_id.get(connection.to_node_id.as_str()),
        ) else {
            continue;
        };
        out.push_str(&connection_geometry(from, to).path_d);
        out.push(' ');
    }
    out
}

/// Bezier path of a single wire, for the selected-wire overlay.
pub fn wire_command(from: &CanvasNodeData, to: &CanvasNodeData) -> String {
    connection_geometry(from, to).path_d
}

/// A generated grid patch: the world rectangle it covers and its commands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GridPatch {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub commands: String,
}

impl GridPatch {
    pub fn covers(&self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> bool {
        min_x >= self.min_x && min_y >= self.min_y && max_x <= self.max_x && max_y <= self.max_y
    }
}

/// World-space grid stride for a zoom: the base [`GRID_STEP`] multiplied by the
/// smallest integer that keeps the on-screen spacing at or above
/// [`GRID_MIN_SCREEN_STEP`]. Only the legacy path fallback grows its stride;
/// the shipped tile grid is screen-anchored and constant.
pub fn grid_stride(viewport: Viewport) -> f64 {
    (GRID_MIN_SCREEN_STEP / (GRID_STEP * viewport.k))
        .ceil()
        .max(1.0)
}

/// Geometry of the pre-rendered dot-grid tile: one square bitmap holding
/// `dots x dots` dots, drawn `cols x rows` times over the canvas.
///
/// The grid is **screen-anchored**: `spacing` is [`GRID_SCREEN_SPACING`] and
/// `dots` is [`GRID_TILE_DOTS`], both independent of the viewport. Zooming
/// therefore never changes the lattice, the tile size or the dot size, and the
/// bitmap only depends on the theme ink and the device scale. All positions are
/// logical screen pixels; only the tile *count* follows the canvas size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridTileLayout {
    /// On-screen spacing between dots, in logical pixels.
    pub spacing: f64,
    /// Dots per tile axis.
    pub dots: u32,
    /// Displayed tile edge, in logical pixels (`dots * spacing`).
    pub tile: f64,
    /// Tile lattice size that covers the viewport at any offset.
    pub cols: u32,
    pub rows: u32,
}

impl Default for GridTileLayout {
    fn default() -> Self {
        Self {
            spacing: GRID_SCREEN_SPACING,
            dots: GRID_TILE_DOTS,
            tile: f64::from(GRID_TILE_DOTS) * GRID_SCREEN_SPACING,
            cols: 1,
            rows: 1,
        }
    }
}

impl GridTileLayout {
    /// Number of `Image` elements the lattice needs (`cols * rows`).
    pub fn tiles(&self) -> u32 {
        self.cols.max(1).saturating_mul(self.rows.max(1))
    }

    /// Edge of the tile bitmap at `device_scale`, in physical pixels.
    ///
    /// The bitmap is rasterised for the fixed logical tile and then displayed
    /// at `dots * spacing` logical pixels, so the visible dot is always the
    /// same size, independent of the zoom and the canvas size.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the pixel edge is rounded and clamped to 8..=1536 before the cast"
    )]
    pub fn image_px(&self, device_scale: f64) -> u32 {
        let scale = if device_scale.is_finite() && device_scale > 0.0 {
            device_scale
        } else {
            1.0
        };
        let px = f64::from(self.dots) * self.spacing * scale;
        // A 1536 px cap keeps the bitmap under ~9 MB while covering displays
        // up to ~5.9x; the common 2x case is 832 px.
        px.round().clamp(8.0, 1536.0) as u32
    }
}

/// The tile lattice that covers `width x height`.
///
/// The spacing and the dots per tile are constants (see
/// [`GRID_SCREEN_SPACING`] / [`GRID_TILE_DOTS`]); only the number of tiles
/// needed to cover the canvas depends on its size.
///
/// The lattice starts up to one whole tile before the canvas and runs one tile
/// past its far edge, because the scroll phase is `Math.mod(vp, tile)` in `[0,
/// tile]`: `cols = ceil(width / tile) + 1` always covers the canvas.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tile counts are viewport-sized integers far below u32; sizes are clamped positive first"
)]
pub fn grid_tile_layout(width: f64, height: f64) -> GridTileLayout {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let spacing = GRID_SCREEN_SPACING;
    let dots = GRID_TILE_DOTS;
    let tile = f64::from(dots) * spacing;
    // One tile of margin: the phase is in [0, tile], so the lattice starts up
    // to one tile before the canvas and the last tile starts at `w - tile`.
    let cols = ((width / tile).ceil().max(1.0) as u32).saturating_add(1);
    let rows = ((height / tile).ceil().max(1.0) as u32).saturating_add(1);
    GridTileLayout {
        spacing,
        dots,
        tile,
        cols,
        rows,
    }
}

/// Screen phase of the tile lattice along one axis, in logical pixels.
///
/// The lattice is placed from `phase - tile` (one whole tile before the
/// canvas), so the phase wraps to `[0, tile]`. The offset is rounded to a whole
/// pixel *before* the modulo, because the tile bitmap is only ever sampled
/// crisply at integral positions: a fractional sample would resample the 2.5 px
/// dot and make the grid shimmer while panning. With `spacing` an even integer
/// the phase is an integer too, exactly like the `Math.round` binding in
/// `app.slint`.
pub fn grid_phase(offset: f64, layout: &GridTileLayout) -> f64 {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    ((offset - layout.spacing * 0.5).rem_euclid(layout.tile)).round()
}

/// Dot grid over the visible world rect (legacy path; the shipped canvas
/// paints the tiled image instead). Web: `background-size: 48k`, dots at
/// intersections, screen dots 3px. Slint has no tileable background, so dots
/// are emitted as near-zero-length segments with a round cap; the stride grows
/// while the on-screen spacing would fall under [`GRID_MIN_SCREEN_STEP`].
///
/// The stub is a fixed **screen** length ([`DOT_STUB_PX`]), not a world one:
/// a stroker may collapse a segment shorter than its own rounding into a
/// degenerate subpath and paint nothing at all (which is exactly what a 0.01
/// world stub did in the software renderer). A constant 0.25px stub keeps the
/// segment alive at every zoom while the 2.5px round cap stays the visible dot.
#[expect(
    clippy::format_push_string,
    reason = "each dot is a formatted subpath pushed into an infallible String write"
)]
pub fn grid_patch(viewport: Viewport, width: f64, height: f64) -> GridPatch {
    let margin_x = width * GRID_MARGIN_SCREENS / viewport.k;
    let margin_y = height * GRID_MARGIN_SCREENS / viewport.k;
    let min_x = -viewport.x / viewport.k - margin_x;
    let min_y = -viewport.y / viewport.k - margin_y;
    let max_x = (width - viewport.x) / viewport.k + margin_x;
    let max_y = (height - viewport.y) / viewport.k + margin_y;

    let stub = DOT_STUB_PX / viewport.k;
    let mut stride = grid_stride(viewport);
    let mut commands = String::new();
    loop {
        let step = GRID_STEP * stride;
        let start_x = (min_x / step).floor() * step;
        let start_y = (min_y / step).floor() * step;
        let cols = ((max_x - start_x) / step).floor() + 1.0;
        let rows = ((max_y - start_y) / step).floor() + 1.0;
        if cols * rows <= GRID_DOT_CAP {
            commands.clear();
            let mut x = start_x;
            while x <= max_x {
                let mut y = start_y;
                while y <= max_y {
                    commands.push_str(&format!("M {x} {y} l {stub:.3} 0 "));
                    y += step;
                }
                x += step;
            }
            return GridPatch {
                min_x: start_x,
                min_y: start_y,
                max_x,
                max_y,
                commands,
            };
        }
        stride *= 2.0;
    }
}

/// Snap-guide lines in screen coordinates (project.tsx:1872-1967): the node
/// bounds padded by 400 world units, 1px wide on screen.
#[expect(
    clippy::format_push_string,
    reason = "guide lines are formatted into an infallible String write"
)]
pub fn guides_commands(
    guides: &SnapGuides,
    nodes: &[CanvasNodeData],
    viewport: Viewport,
) -> String {
    if guides.x.is_empty() && guides.y.is_empty() {
        return String::new();
    }
    let bounds = node_bounds(nodes);
    let top = viewport
        .world_to_screen(WorldPos::new(0.0, bounds.top - GUIDE_PADDING))
        .y;
    let bottom = viewport
        .world_to_screen(WorldPos::new(0.0, bounds.bottom + GUIDE_PADDING))
        .y;
    let left = viewport
        .world_to_screen(WorldPos::new(bounds.left - GUIDE_PADDING, 0.0))
        .x;
    let right = viewport
        .world_to_screen(WorldPos::new(bounds.right + GUIDE_PADDING, 0.0))
        .x;
    let mut out = String::new();
    for x in &guides.x {
        let sx = viewport.world_to_screen(WorldPos::new(*x, 0.0)).x;
        out.push_str(&format!("M {sx} {top} L {sx} {bottom} "));
    }
    for y in &guides.y {
        let sy = viewport.world_to_screen(WorldPos::new(0.0, *y)).y;
        out.push_str(&format!("M {left} {sy} L {right} {sy} "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::{CanvasNodeType, Position};

    fn node(id: &str, x: f64, y: f64, w: f64, h: f64) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: CanvasNodeType::Text,
            title: id.into(),
            position: Position::new(x, y),
            width: w,
            height: h,
            metadata: None,
        }
    }

    #[test]
    fn wires_use_the_core_geometry_paths() {
        let from = node("a", 0.0, 0.0, 100.0, 50.0);
        let to = node("b", 300.0, 100.0, 80.0, 40.0);
        let connections = vec![CanvasConnection {
            id: "c".into(),
            from_node_id: "a".into(),
            to_node_id: "b".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }];
        let commands = wires_commands(&[from.clone(), to.clone()], &connections);
        assert_eq!(commands.trim(), connection_geometry(&from, &to).path_d);
    }

    #[test]
    fn grid_patch_covers_the_view_and_bounds_the_dot_count() {
        let viewport = Viewport::new();
        let patch = grid_patch(viewport, 1040.0, 744.0);
        assert!(patch.covers(
            -viewport.x / viewport.k,
            -viewport.y / viewport.k,
            1040.0,
            744.0
        ));
        // k = 1: dot spacing is exactly 48 world units from the first intersection.
        assert!(patch.commands.starts_with("M -528 -384 "));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "GRID_DOT_CAP is 8000.0, an exact whole number well inside usize range"
        )]
        let cap = GRID_DOT_CAP as usize;
        let count = patch.commands.matches("M ").count();
        assert!(count > 1 && count <= cap);
        // k = 0.05 would need 170k dots at the raw spacing; the stride keeps it bounded.
        let zoomed_out = grid_patch(
            Viewport {
                x: 0.0,
                y: 0.0,
                k: 0.05,
            },
            1040.0,
            744.0,
        );
        assert!(zoomed_out.commands.matches("M ").count() <= cap);
    }

    /// The tile lattice covers the canvas at *any* scroll phase, including the
    /// worst one (`Math.mod` returns just under one tile, so the lattice starts
    /// almost two tiles early), whatever the pan offset.
    #[test]
    fn the_tile_lattice_covers_every_pan_offset() {
        let (width, height) = (1040.0, 744.0);
        for size in [(width, height), (320.0, 240.0), (3840.0, 2160.0)] {
            let layout = grid_tile_layout(size.0, size.1);
            for offset in [-1_000_017.3, -260.4, -0.5, 0.0, 17.37, 259.9, 1e6] {
                let phase_x = grid_phase(offset, &layout);
                let phase_y = grid_phase(-offset / 2.0, &layout);
                assert!(
                    (0.0..=layout.tile).contains(&phase_x)
                        && (0.0..=layout.tile).contains(&phase_y),
                    "{size:?} offset={offset}: phase in [0, tile] ({phase_x}, {phase_y})"
                );
                // Leftmost/topmost image origin: one tile before the phase.
                let left = phase_x - layout.tile;
                let top = phase_y - layout.tile;
                assert!(
                    left <= 0.0 && top <= 0.0,
                    "{size:?} offset={offset}: the lattice starts before the canvas ({left}, {top})"
                );
                let (cols, rows) = (f64::from(layout.cols), f64::from(layout.rows));
                assert!(
                    left + (cols - 1.0) * layout.tile + layout.tile >= size.0,
                    "{size:?} offset={offset}: {cols} tiles cover the width"
                );
                assert!(
                    top + (rows - 1.0) * layout.tile + layout.tile >= size.1,
                    "{size:?} offset={offset}: {rows} tiles cover the height"
                );
            }
        }
    }

    /// The lattice is a screen-space constant: spacing, dots per tile and tile
    /// edge do not depend on the viewport at all, the element count stays
    /// small, and a dot sits on the canvas origin.
    #[test]
    fn the_tile_lattice_is_small_and_screen_anchored() {
        for size in [(1040.0, 744.0), (320.0, 240.0), (3840.0, 2160.0)] {
            let layout = grid_tile_layout(size.0, size.1);
            assert_eq!(
                layout.spacing, GRID_SCREEN_SPACING,
                "{size:?}: the on-screen spacing is the constant"
            );
            assert_eq!(layout.dots, GRID_TILE_DOTS);
            assert_eq!(layout.tile, f64::from(layout.dots) * layout.spacing);
            assert!(
                layout.tiles() <= 100,
                "{size:?}: {} images ({layout:?})",
                layout.tiles()
            );
        }
        // A dot at the world origin's screen position: with an integral pan the
        // lattice phase is the origin minus half a cell, so the first dot's
        // centre is exactly on the origin.
        let layout = grid_tile_layout(1040.0, 744.0);
        let phase = grid_phase(0.0, &layout);
        assert_eq!(phase, layout.tile - layout.spacing * 0.5);
        // The dot centres of the tile placed at `phase - tile` then land on
        // `0, spacing, 2 * spacing, ...`.
        let first_centre = phase - layout.tile + layout.spacing * 0.5;
        assert!(first_centre.abs() < f64::EPSILON);
    }

    /// Panning only slides the lattice: the phase follows the pan one-to-one,
    /// rounded to whole pixels so the bitmap is never resampled.
    #[test]
    fn the_phase_tracks_the_pan_in_whole_pixels() {
        let layout = grid_tile_layout(1040.0, 744.0);
        let base = grid_phase(0.0, &layout);
        assert!(
            base.fract() == 0.0,
            "whole pixels from an integral pan: {base}"
        );
        // 17.37 px of pan rounds to 17 px of lattice movement.
        let panned = grid_phase(17.37, &layout);
        let moved = (panned - base).rem_euclid(layout.tile);
        assert_eq!(moved, 17.0, "the lattice advances by the rounded pan");
        // Sub-pixel movement never reaches the bitmap (that is the anti-
        // shimmer rule): +-0.4 px of pan does not move the lattice.
        assert_eq!(grid_phase(0.4, &layout), base);
        assert_eq!(grid_phase(-0.4, &layout), base);
        assert_eq!(grid_phase(17.4, &layout), panned);
        // A full tile of pan is the identity, and the phase is always integral.
        assert_eq!(grid_phase(layout.tile, &layout), base);
        for offset in [-3.7, 0.0, 0.6, 13.0, 259.4, 260.2, 10_000.9] {
            let phase = grid_phase(offset, &layout);
            assert!(
                phase.fract() == 0.0 && (0.0..=layout.tile).contains(&phase),
                "offset {offset}: {phase}"
            );
        }
        // Degenerate input is safe.
        assert_eq!(grid_phase(f64::NAN, &layout), base);
        assert_eq!(grid_phase(f64::INFINITY, &layout), base);
    }

    /// The bitmap size follows the device scale, so a `HiDPI` display gets a
    /// crisp tile, and it is bounded.
    #[test]
    fn the_tile_bitmap_is_bounded_and_scales_with_the_display() {
        let layout = grid_tile_layout(1040.0, 744.0);
        let one_x = layout.image_px(1.0);
        let two_x = layout.image_px(2.0);
        assert!(
            two_x > one_x,
            "2x gets a larger bitmap ({one_x} -> {two_x})"
        );
        for scale in [0.0, -1.0, f64::NAN, 8.0] {
            let px = layout.image_px(scale);
            assert!((8..=1536).contains(&px), "scale {scale} -> {px} px");
        }
        // One dot cell per `spacing` logical px.
        let px_per_dot = f64::from(one_x) / f64::from(layout.dots);
        assert!(
            (px_per_dot - layout.spacing).abs() < 1.0,
            "one dot cell per {px_per_dot} px, expected {}",
            layout.spacing
        );
    }
}
