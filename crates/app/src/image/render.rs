//! Image Studio rendering: composites the board through the `open-canvas-ps`
//! compositor into a Slint image, builds the screen-space overlays (checker,
//! grid, guides, marching ants, draft, snap lines, path, transform handles,
//! rulers) and pushes the whole Image Studio view model — the Rust half of
//! `ps-board-view.tsx` + `ps-overlays.tsx` and `image-studio.tsx`'s render
//! effect.
//! Owns the studio view-model push (`sync_image`) and the rows derived for it.

use open_canvas_ps::doc::BoardResolution;
use open_canvas_ps::{
    layer_ops::find_ps_layer, read_media_dimensions, selection_edges, CanvasPsLayer,
    CanvasPsParamValue, ImageProject, Point, SmartCanvasBoard, Surface,
};
use slint::ComponentHandle;

use crate::app::{App, View};
use crate::image::{Gesture, ImageState, ImageTool};
use crate::AppWindow;

/// Longest display-side of the composited preview (the full board renders head
/// to a capped tier and resamples; nothing allocates an unbounded buffer).
const PREVIEW_MAX: usize = 1600;
/// `PsMarchingAnts` edge cap (ps-overlays.tsx:34).
const ANTS_LIMIT: usize = 12_000;
/// `PsGrid` spacing in document units (ps-overlays.tsx:135).
const GRID_STEP: f64 = 50.0;
/// Layers-panel thumbnail size (the web's `size-7`, 28px).
const LAYER_THUMB: usize = 28;

/// The layers panel's row order (`ps-layers-panel.tsx` `rows`): topmost layer
/// first, each group's children reversed and indented under it unless the
/// group is collapsed; one nesting level, like the web.
fn layer_row_order(image: &ImageState) -> Vec<(CanvasPsLayer, bool)> {
    let mut rows: Vec<(CanvasPsLayer, bool)> = Vec::new();
    for layer in open_canvas_ps::top_layers(&image.project.board_layers)
        .iter()
        .rev()
    {
        rows.push((layer.clone(), false));
        if layer.kind == open_canvas_ps::CanvasPsLayerKind::Group && !image.is_collapsed(&layer.id)
        {
            for child in open_canvas_ps::board::group_children(&image.project.board_layers, layer)
                .iter()
                .rev()
            {
                rows.push((child.clone(), true));
            }
        }
    }
    rows
}

/// `psColor` → Slint colour.
pub(crate) fn color_of(hex: &str) -> slint::Color {
    let [r, g, b, a] = open_canvas_ps::hex_to_rgba(hex);
    slint::Color::from_argb_u8(a, r, g, b)
}

/// A rendered `Surface` → Slint image (RGBA8, one copy).
pub(crate) fn image_of(surface: &Surface) -> slint::Image {
    let width = u32::try_from(surface.width()).unwrap_or(0);
    let height = u32::try_from(surface.height()).unwrap_or(0);
    let mut pixel_buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
    let target = pixel_buffer.make_mut_bytes();
    let source = surface.data();
    let len = target.len().min(source.len());
    target[..len].copy_from_slice(&source[..len]);
    slint::Image::from_rgba8(pixel_buffer)
}

/// The paths panel row thumbnail's square size (`ps-paths-panel.tsx` rows are
/// 36px with a small preview).
pub(crate) const PATH_THUMB_SIZE: usize = 32;

/// A small preview of one path for the paths panel row: the path's polyline
/// scaled to fit a `size`-px square, stroked in the canvas path-overlay colour.
pub(crate) fn path_thumb(path: &open_canvas_ps::CanvasPsPath, size: usize) -> Surface {
    let mut surface = Surface::new(size, size);
    let Some(bounds) = open_canvas_ps::path_bounds(path) else {
        return surface;
    };
    let points = open_canvas_ps::polyline(path, 24);
    if points.len() < 2 {
        return surface;
    }
    let padding = 3.0;
    let extent = bounds.width.max(bounds.height).max(1.0);
    let scale = (crate::vm::i2f(crate::vm::u2i(size)) - padding * 2.0) / extent;
    // `Theme.image-path-line` (`theme.slint`): the canvas path overlay colour.
    let rgba = open_canvas_ps::hex_to_rgba("#2563eb");
    let scaled: Vec<open_canvas_ps::Point> = points
        .iter()
        .map(|point| {
            open_canvas_ps::Point::new(
                padding + (point.x - bounds.x) * scale,
                padding + (point.y - bounds.y) * scale,
            )
        })
        .collect();
    for window in scaled.windows(2) {
        stamp_thumb_segment(&mut surface, window[0], window[1], rgba);
    }
    surface
}

/// One 1px-wide segment stamped into a thumbnail (2x2 blocks at the sampled
/// points, which reads as a clean 1px line at 32px).
fn stamp_thumb_segment(
    surface: &mut Surface,
    from: open_canvas_ps::Point,
    to: open_canvas_ps::Point,
    rgba: [u8; 4],
) {
    let steps = crate::vm::f2i((to.x - from.x).hypot(to.y - from.y).ceil()).max(1);
    for step in 0..=steps {
        let t = crate::vm::i2f(step) / crate::vm::i2f(steps);
        let x = crate::vm::f2i(from.x + (to.x - from.x) * t);
        let y = crate::vm::f2i(from.y + (to.y - from.y) * t);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            surface.set(x + dx, y + dy, rgba);
        }
    }
}

/// The board as a `SmartCanvasBoard` with a resolution tier that covers the
/// document, rendered through the shared compositor and resampled to the
/// document size (`renderPsDocument` at board scale).
pub(crate) fn board_surface(
    project: &ImageProject,
    bitmaps: &std::collections::HashMap<String, Surface>,
) -> Option<Surface> {
    let (width, height) = crate::image::board_dims(project)?;
    let as_f = |value: usize| crate::vm::i2f(crate::vm::u2i(value));
    let as_u = |value: f64| crate::vm::i2u(crate::vm::f2i(value.round().max(1.0)));
    let mut board = SmartCanvasBoard::from(project);
    let covers = |tier: BoardResolution| {
        let (w, h) = read_media_dimensions(tier, &board.board_ratio);
        w >= as_f(width) && h >= as_f(height)
    };
    let tier = if covers(open_canvas_ps::doc::BoardResolution::K1) {
        open_canvas_ps::doc::BoardResolution::K1
    } else if covers(open_canvas_ps::doc::BoardResolution::K2) {
        open_canvas_ps::doc::BoardResolution::K2
    } else {
        open_canvas_ps::doc::BoardResolution::K4
    };
    board.board_resolution = tier;
    let text = crate::image::text_raster::shared();
    let rendered = open_canvas_ps::render_document_with_text(
        &board,
        bitmaps,
        text.map(|rasterizer| rasterizer as &dyn open_canvas_ps::TextRasterizer),
    );
    if rendered.width() == width && rendered.height() == height {
        return Some(rendered);
    }
    let scale = (as_f(width) / as_f(rendered.width().max(1)))
        .min(as_f(height) / as_f(rendered.height().max(1)))
        .min(1.0);
    let target_w = as_u(as_f(rendered.width()) * scale).min(PREVIEW_MAX);
    let target_h = as_u(as_f(rendered.height()) * scale).min(PREVIEW_MAX);
    let resampled = rendered.resample_bilinear(target_w, target_h);
    if resampled.width() == width && resampled.height() == height {
        Some(resampled)
    } else {
        // A tier that only roughly matches (custom ratios): resample exactly.
        Some(resampled.resample_bilinear(width, height))
    }
}

/// Screen-space x of a document x under the view transform.
fn sx(view: &crate::image::ImageView, x: f64) -> f64 {
    view.x + x * view.k
}

/// Screen-space y of a document y under the view transform.
fn sy(view: &crate::image::ImageView, y: f64) -> f64 {
    view.y + y * view.k
}

/// Screen-space checker tile (logical px at 100%): a small, screen-constant
/// square under a transparent board, so the document reads as paper rather
/// than as a mesh. Zooming never scales it.
const CHECKER_TILE: f64 = 8.0;
/// Hard budget on the quads the checker may emit in one frame. Only a viewport
/// larger than roughly 2K can reach it; the tile then doubles (a coarser paper
/// texture) instead of the string growing with the window.
const CHECKER_MAX_QUADS: usize = 20_000;

/// Checkerboard under a transparent board (the web board div pattern).
///
/// The commands are **document-local** screen pixels: `0,0` is the document's
/// top-left corner, so the studio can lay them inside a frame the size of the
/// document rectangle and clip there — the checker can never paint a pixel
/// outside the document, not even an antialiased edge one. Only the visible
/// slice (the document ∩ the canvas, still in document-local units) is painted,
/// so a document larger than the viewport costs no more than the viewport. Tile
/// phase is anchored to the document's top-left corner: panning moves the
/// pattern with the document, and the 8px tile size is independent of `view.k`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    reason = "tile indices are small integers; checker quads are formatted into an infallible String write"
)]
fn checker_path(
    project: &ImageProject,
    view: &crate::image::ImageView,
    canvas: (f64, f64),
) -> String {
    let doc_w = project.width * view.k;
    let doc_h = project.height * view.k;
    if !(doc_w > 0.0 && doc_h > 0.0) {
        return String::new();
    }

    // The visible slice of the document, in document-local units: the document
    // is `0..doc_w` × `0..doc_h` and the canvas shows that box offset by the
    // view transform, so the canvas covers `-view.x .. canvas.0 - view.x`.
    let x0 = (-view.x).max(0.0);
    let y0 = (-view.y).max(0.0);
    let x1 = (canvas.0.max(0.0) - view.x).min(doc_w);
    let y1 = (canvas.1.max(0.0) - view.y).min(doc_h);
    if x1 <= x0 || y1 <= y0 {
        return String::new();
    }

    // A 4K window could otherwise ask for ~65k quads of 8px cells; double the
    // screen tile until the visible cell count is within budget.
    let mut tile = CHECKER_TILE;
    loop {
        let cols = ((x1 - x0) / tile).ceil() + 1.0;
        let rows = ((y1 - y0) / tile).ceil() + 1.0;
        if cols * rows * 0.5 <= crate::vm::i2f(crate::vm::u2i(CHECKER_MAX_QUADS)) {
            break;
        }
        tile *= 2.0;
    }

    let first_col = (x0 / tile).floor();
    let first_row = (y0 / tile).floor();
    let mut out = String::new();
    let mut ty = first_row * tile;
    let mut row = first_row as i64;
    while ty < y1 {
        let top = ty.max(y0);
        let bottom = (ty + tile).min(y1);
        let mut tx = first_col * tile;
        let mut column = first_col as i64;
        while tx < x1 {
            if (column + row) % 2 != 0 {
                let left = tx.max(x0);
                let right = (tx + tile).min(x1);
                if right > left && bottom > top {
                    out.push_str(&format!(
                        "M {left} {top} L {right} {top} L {right} {bottom} L {left} {bottom} Z "
                    ));
                }
            }
            tx += tile;
            column += 1;
        }
        ty += tile;
        row += 1;
    }
    out
}

/// `PsGrid` (ps-overlays.tsx:129): 50-unit lines across the board.
#[expect(
    clippy::format_push_string,
    reason = "grid lines are formatted into an infallible String write"
)]
fn grid_commands(
    project: &ImageProject,
    view: &crate::image::ImageView,
    canvas: (f64, f64),
) -> String {
    let mut out = String::new();
    let (x0, y0) = (sx(view, 0.0), sy(view, 0.0));
    let (x1, y1) = (sx(view, project.width), sy(view, project.height));
    let mut gx = 0.0;
    while gx <= project.width {
        let px = sx(view, gx).floor() + 0.5;
        if px >= x0.min(canvas.0) && px <= x1.max(0.0) {
            out.push_str(&format!("M {px} {y0} L {px} {y1} "));
        }
        gx += GRID_STEP;
    }
    let mut gy = 0.0;
    while gy <= project.height {
        let py = sy(view, gy).floor() + 0.5;
        if py >= y0.min(canvas.1) && py <= y1.max(0.0) {
            out.push_str(&format!("M {x0} {py} L {x1} {py} "));
        }
        gy += GRID_STEP;
    }
    out
}

/// Guide / snap lines: `(horizontal, document position)`.
#[expect(
    clippy::format_push_string,
    reason = "guide lines are formatted into an infallible String write"
)]
fn line_commands(
    project: &ImageProject,
    view: &crate::image::ImageView,
    lines: &[(bool, f64)],
) -> String {
    let mut out = String::new();
    for (horizontal, position) in lines {
        if *horizontal {
            let py = sy(view, *position).floor() + 0.5;
            out.push_str(&format!(
                "M {} {py} L {} {py} ",
                sx(view, 0.0),
                sx(view, project.width)
            ));
        } else {
            let px = sx(view, *position).floor() + 0.5;
            out.push_str(&format!(
                "M {px} {} L {px} {} ",
                sy(view, 0.0),
                sy(view, project.height)
            ));
        }
    }
    out
}

/// `PsMarchingAnts` (ps-overlays.tsx:28): the boundary between selected and
/// unselected pixels. `light` emits the alternating dash half.
#[expect(
    clippy::format_push_string,
    reason = "ant segments are formatted into an infallible String write"
)]
fn ants_commands(
    selection: &Surface,
    view: &crate::image::ImageView,
    light: bool,
    phase: i64,
) -> String {
    let (width, height) = (selection.width(), selection.height());
    let selected = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < crate::vm::u2i(width)
            && y < crate::vm::u2i(height)
            && selection.get(x, y)[3] >= 128
    };
    let mut out = String::new();
    for index in selection_edges(selection, ANTS_LIMIT) {
        let x = index % width;
        let y = index / width;
        // The light pass draws the alternating dash half over the solid dark
        // boundary; the dash phase marches 2px every 120ms (`ANT_TICK_MS`),
        // exactly like the web's canvas pass (`ps-overlays.tsx:52,60`).
        if light && !crate::image::ants_light(crate::vm::u2i(x), crate::vm::u2i(y), phase) {
            continue;
        }
        let (xi, yi) = (crate::vm::u2i(x), crate::vm::u2i(y));
        let (px, py) = (sx(view, crate::vm::i2f(xi)), sy(view, crate::vm::i2f(yi)));
        let size = view.k.max(1.0);
        if !selected(xi - 1, yi) {
            out.push_str(&format!("M {px} {py} L {px} {py2} ", py2 = py + size));
        }
        if !selected(xi + 1, yi) {
            out.push_str(&format!(
                "M {px2} {py} L {px2} {py2} ",
                px2 = px + size,
                py2 = py + size
            ));
        }
        if !selected(xi, yi - 1) {
            out.push_str(&format!("M {px} {py} L {px2} {py} ", px2 = px + size));
        }
        if !selected(xi, yi + 1) {
            out.push_str(&format!(
                "M {px} {py2} L {px2} {py2} ",
                px2 = px + size,
                py2 = py + size
            ));
        }
    }
    out
}

/// `PsDraftOverlay` (ps-overlays.tsx:173): the live gesture outline.
#[expect(
    clippy::format_push_string,
    reason = "draft outlines are formatted into an infallible String write"
)]
fn draft_commands(gesture: Option<&Gesture>, view: &crate::image::ImageView) -> String {
    let mut out = String::new();
    let point = |p: &Point| (sx(view, p.x), sy(view, p.y));
    match gesture {
        Some(Gesture::Marquee {
            tool,
            start,
            current,
            ..
        }) => {
            let (ax, ay) = point(start);
            let (bx, by) = point(current);
            let (x0, x1) = (ax.min(bx), ax.max(bx));
            let (y0, y1) = (ay.min(by), ay.max(by));
            if *tool == crate::image::ImageTool::EllipseMarquee {
                // The committed mask is an ellipse; the live preview must be
                // one too, rather than the bounding rectangle.
                let cx = (x0 + x1) / 2.0;
                let cy = (y0 + y1) / 2.0;
                let rx = ((x1 - x0) / 2.0).max(0.5);
                let ry = ((y1 - y0) / 2.0).max(0.5);
                out.push_str(&format!(
                    "M {cx} {} A {rx} {ry} 0 1 0 {cx} {} A {rx} {ry} 0 1 0 {cx} {} Z ",
                    cy - ry,
                    cy + ry,
                    cy - ry
                ));
            } else {
                out.push_str(&format!(
                    "M {x0} {y0} L {x1} {y0} L {x1} {y1} L {x0} {y1} Z "
                ));
            }
        }
        Some(Gesture::Shape { start, current }) => {
            let (ax, ay) = point(start);
            let (bx, by) = point(current);
            let (x0, x1) = (ax.min(bx), ax.max(bx));
            let (y0, y1) = (ay.min(by), ay.max(by));
            out.push_str(&format!(
                "M {x0} {y0} L {x1} {y0} L {x1} {y1} L {x0} {y1} Z "
            ));
        }
        Some(Gesture::Lasso { points }) => {
            for (index, p) in points.iter().enumerate() {
                let (px, py) = point(p);
                if index == 0 {
                    out.push_str(&format!("M {px} {py} "));
                } else {
                    out.push_str(&format!("L {px} {py} "));
                }
            }
            out.push_str("Z ");
        }
        Some(Gesture::Gradient { start, current }) => {
            let (ax, ay) = point(start);
            let (bx, by) = point(current);
            out.push_str(&format!("M {ax} {ay} L {bx} {by} "));
        }
        _ => {}
    }
    out
}

/// Polygon-lasso click points as an open outline.
#[expect(
    clippy::format_push_string,
    reason = "polygon points are formatted into an infallible String write"
)]
fn polygon_lasso_commands(points: &[Point], view: &crate::image::ImageView) -> String {
    let mut out = String::new();
    for (index, p) in points.iter().enumerate() {
        let (px, py) = (sx(view, p.x), sy(view, p.y));
        if index == 0 {
            out.push_str(&format!("M {px} {py} "));
        } else {
            out.push_str(&format!("L {px} {py} "));
        }
    }
    out
}

/// `PsPathOverlay` (ps-overlays.tsx:211): the active path outline with anchor
/// and handle squares.
#[expect(
    clippy::format_push_string,
    reason = "path geometry is formatted into an infallible String write"
)]
fn path_overlay_commands(
    project: &ImageProject,
    active_path: &str,
    view: &crate::image::ImageView,
) -> String {
    let Some(paths) = project.board_paths.as_ref() else {
        return String::new();
    };
    let mut out = String::new();
    // Every visible path stays on the canvas, not just the active one: a
    // hidden path is unselectable, while a visible path can be clicked to make
    // it active (and reveal its anchors).
    for path in paths.iter().filter(|path| path.visible) {
        let points = open_canvas_ps::polyline(path, 24);
        for (index, p) in points.iter().enumerate() {
            let (px, py) = (sx(view, p.x), sy(view, p.y));
            if index == 0 {
                out.push_str(&format!("M {px} {py} "));
            } else {
                out.push_str(&format!("L {px} {py} "));
            }
        }
        if path.closed && !points.is_empty() {
            out.push_str("Z ");
        }
    }
    let Some(path) = paths
        .iter()
        .find(|path| path.id == active_path && path.visible)
    else {
        return out;
    };
    let anchor_box = |px: f64, py: f64, out: &mut String| {
        out.push_str(&format!(
            "M {} {} L {} {} L {} {} L {} {} Z ",
            px - 3.0,
            py - 3.0,
            px + 3.0,
            py - 3.0,
            px + 3.0,
            py + 3.0,
            px - 3.0,
            py + 3.0
        ));
    };
    for anchor in &path.anchors {
        let (ax, ay) = (sx(view, anchor.x), sy(view, anchor.y));
        for handle in [anchor.handle_in, anchor.handle_out] {
            if handle.x != 0.0 || handle.y != 0.0 {
                let (hx, hy) = (sx(view, anchor.x + handle.x), sy(view, anchor.y + handle.y));
                out.push_str(&format!("M {ax} {ay} L {hx} {hy} "));
                anchor_box(hx, hy, &mut out);
            }
        }
        anchor_box(ax, ay, &mut out);
    }
    out
}

/// `PsTransformHandles` (ps-overlays.tsx:266) — only while a transform is
/// armed: the mode's handle set (8 skew, 4 distort/perspective, the 16-point
/// warp mesh), else the layer box with its four corner handles and the rotate
/// handle.
///
/// Nothing is drawn when no transform mode is armed. The always-on box used to
/// trace the active layer's bounds in selection blue; on a layer that covers the
/// document (the common case) that read as a blue outline around the board
/// itself, so the overlay is now a transform affordance only. The Hand tool's
/// corner scale hotspots stay live (`image_move_start`), they are just not
/// painted as a selection-like border.
#[expect(
    clippy::format_push_string,
    reason = "handle geometry is formatted into an infallible String write"
)]
fn transform_commands(
    project: &ImageProject,
    active_layer: &str,
    view: &crate::image::ImageView,
    mode: Option<open_canvas_ps::PsTransformMode>,
) -> String {
    let Some(mode) = mode else {
        return String::new();
    };
    let Some(layer) = find_ps_layer(&project.board_layers, active_layer) else {
        return String::new();
    };
    if layer.kind == open_canvas_ps::CanvasPsLayerKind::Adjustment {
        return String::new();
    }
    let mut out = String::new();
    let square = |px: f64, py: f64, half: f64, out: &mut String| {
        out.push_str(&format!(
            "M {} {} L {} {} L {} {} L {} {} Z ",
            px - half,
            py - half,
            px + half,
            py - half,
            px + half,
            py + half,
            px - half,
            py + half
        ));
    };
    if mode != open_canvas_ps::PsTransformMode::Free {
        for (_, point) in crate::image::transform::transform_handle_points(layer, mode) {
            let (px, py) = (sx(view, point.x), sy(view, point.y));
            square(px, py, 4.0, &mut out);
            square(px, py, 1.5, &mut out);
        }
        return out;
    }
    let (x0, y0) = (layer.x, layer.y);
    let (x1, y1) = (layer.x + layer.width, layer.y + layer.height);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    for (index, (x, y)) in corners.iter().enumerate() {
        let (px, py) = (sx(view, *x), sy(view, *y));
        if index == 0 {
            out.push_str(&format!("M {px} {py} "));
        } else {
            out.push_str(&format!("L {px} {py} "));
        }
    }
    out.push_str("Z ");
    for (x, y) in corners {
        let (px, py) = (sx(view, x), sy(view, y));
        square(px, py, 4.0, &mut out);
        square(px, py, 1.5, &mut out);
    }
    let rotate = crate::image::transform::rotate_handle_point(layer, view.k);
    let (rx, ry) = (sx(view, rotate.x), sy(view, rotate.y));
    out.push_str(&format!(
        "M {} {ry} A 4 4 0 1 1 {} {ry} Z ",
        rx - 4.0,
        rx + 4.0
    ));
    out
}

/// Ruler ticks for both axes via the shared canvas ruler helper. The vertical
/// labels are pre-rotated bitmaps, so the studio's ruler is as crisp as the
/// canvas one.
fn ruler_rows(
    view: &crate::image::ImageView,
    canvas: (f64, f64),
    ink: slint::Color,
    scale: f32,
) -> (Vec<crate::RulerTick>, Vec<crate::RulerTickImage>) {
    let viewport = crate::coords::Viewport {
        x: view.x,
        y: view.y,
        k: view.k,
    };
    let (ticks_x, ticks_y) = crate::panels::ruler_ticks(viewport, canvas.0, canvas.1);
    (
        ticks_x.into_iter().map(crate::vm::ruler_tick_row).collect(),
        ticks_y
            .into_iter()
            .map(|tick| crate::vm::ruler_label_row(tick, ink, scale))
            .collect(),
    )
}

/// Numeric-range table for the generic adjustment param editor (P5c-3 replaces
/// this with the per-type editors).
fn param_range(key: &str) -> (f64, f64) {
    match key {
        "brightness" => (-150.0, 150.0),
        "contrast" | "vibrance" | "saturation" | "lightness" | "hue" => (-100.0, 100.0),
        "amount" => (1.0, 500.0),
        "radius" | "size" => (0.0, 200.0),
        "threshold" | "level" => (0.0, 255.0),
        "exposure" => (-5.0, 5.0),
        "offset" => (-0.5, 0.5),
        "gamma" => (0.1, 9.99),
        "posterize" => (2.0, 255.0),
        _ => (0.0, 100.0),
    }
}

/// The inputs a board composite was built from, so a sync that changed none of
/// them reuses the last one: the document (mask view resolved), every bitmap's
/// content stamp, and the channel view applied on top.
#[derive(Debug, Clone, PartialEq)]
struct DisplayKey {
    project: ImageProject,
    bitmaps: u64,
    channel_view: String,
}

impl DisplayKey {
    /// The key of the board `image` would render right now.
    fn of(image: &ImageState) -> Self {
        Self {
            project: if image.mask_view {
                image.mask_view_project()
            } else {
                image.project.clone()
            },
            bitmaps: bitmaps_stamp(&image.bitmaps),
            channel_view: image.channel_view.clone(),
        }
    }
}

/// Order-independent digest of the bitmap store: each entry's key, size and
/// content stamp. The stamp changes on every write (see `Surface::stamp`), so
/// this costs one pass over the store, never over its pixels.
fn bitmaps_stamp(bitmaps: &std::collections::HashMap<String, Surface>) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    fn mix(hash: &mut u64, value: u64) {
        for byte in value.to_le_bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(PRIME);
        }
    }
    let mut keys: Vec<&String> = bitmaps.keys().collect();
    keys.sort_unstable();
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for key in keys {
        for byte in key.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        if let Some(surface) = bitmaps.get(key) {
            mix(&mut hash, surface.stamp());
            mix(&mut hash, surface.width() as u64);
            mix(&mut hash, surface.height() as u64);
        }
    }
    hash
}

/// The board composite with the channel view applied (`viewBoard`), with mask
/// preview showing the targeted mask in place of its layer. Uncached: the
/// studio's own sync goes through [`ImageState::cached_display`], which skips
/// the work when nothing that affects the pixels changed.
pub fn display_of(image: &ImageState) -> Option<Surface> {
    let project = if image.mask_view {
        image.mask_view_project()
    } else {
        image.project.clone()
    };
    let mut display = board_surface(&project, &image.bitmaps)?;
    crate::image::channels::apply_channel_view(
        &mut display,
        &image.channel_view,
        &image.bitmaps,
        image.project.board_alpha_channels.as_ref(),
    );
    Some(display)
}

/// The last composite and the inputs it was built from.
#[derive(Debug, Default)]
pub struct DisplayCache {
    key: Option<DisplayKey>,
    display: Option<Surface>,
}

impl ImageState {
    /// The displayed board and whether it differs from the last one. A pan, a
    /// zoom, a panel edit or a hover all sync the studio without touching a
    /// pixel, so the composite is only recomputed when its inputs changed.
    ///
    /// `App::sync_image` is the only caller: the `false` (unchanged) answer
    /// assumes the last `true` surface is already in the window's property.
    pub fn cached_display(&mut self) -> Option<(&Surface, bool)> {
        let key = DisplayKey::of(self);
        let hit = self.display_cache.key.as_ref() == Some(&key);
        if !hit {
            let Some(display) = display_of(self) else {
                self.display_cache = DisplayCache::default();
                return None;
            };
            self.display_cache.key = Some(key);
            self.display_cache.display = Some(display);
        }
        self.display_cache
            .display
            .as_ref()
            .map(|display| (display, !hit))
    }
}

impl App {
    /// The board composite (`renderPsDocument`), or `None` without a board.
    pub fn image_composite(&self) -> Option<Surface> {
        board_surface(&self.image.project, &self.image.bitmaps)
    }

    /// The `image-studio` render effect's view-model half.
    pub fn sync_image(&mut self, ui: &AppWindow) {
        if self.image.meta.is_none() && self.view != View::Image {
            return;
        }
        self.image_auto_fit();
        if let Some((display, changed)) = self.image.cached_display() {
            // An unchanged composite is already in the window's property: the
            // copy into a Slint image (and the repaint it would ask for) is
            // pure overhead on a pan.
            if changed {
                ui.set_image_canvas_image(image_of(display));
            }
        }
        let view = self.image.view;
        let canvas = self.image.canvas;
        let project = &self.image.project;
        ui.set_image_checker(checker_path(project, &view, canvas).into());
        let (x, y) = (view.x, view.y);
        ui.set_image_checker_x(crate::vm::len(x));
        ui.set_image_checker_y(crate::vm::len(y));
        ui.set_image_checker_w(crate::vm::len(project.width * view.k));
        ui.set_image_checker_h(crate::vm::len(project.height * view.k));
        ui.set_image_grid(grid_commands(project, &view, canvas).into());
        ui.set_image_guides(line_commands(project, &view, &self.image.guides).into());
        let draft_guides: Vec<(bool, f64)> = self
            .image
            .guide_drag
            .as_ref()
            .map(|drag| vec![(drag.horizontal, drag.value)])
            .unwrap_or_default();
        ui.set_image_guide_draft(line_commands(project, &view, &draft_guides).into());
        let (ants, ants_alt) = match &self.image.selection {
            Some(selection) => (
                ants_commands(selection, &view, false, self.image.ants_phase),
                ants_commands(selection, &view, true, self.image.ants_phase),
            ),
            None => (String::new(), String::new()),
        };
        ui.set_image_ants(ants.into());
        ui.set_image_ants_alt(ants_alt.into());
        let mut draft = draft_commands(self.image.gesture.as_ref(), &view);
        draft.push_str(&polygon_lasso_commands(&self.image.poly_points, &view));
        ui.set_image_draft(draft.into());
        ui.set_image_snap_lines(line_commands(project, &view, &self.image.snap_lines).into());
        ui.set_image_path_overlay(
            path_overlay_commands(project, &self.image.active_path, &view).into(),
        );
        ui.set_image_transform(
            transform_commands(
                project,
                &self.image.active_layer,
                &view,
                self.image.transform_mode,
            )
            .into(),
        );
        let (ticks_x, ticks_y) = ruler_rows(
            &view,
            canvas,
            ui.global::<crate::Theme>().get_node_muted(),
            ui.window().scale_factor(),
        );
        ui.set_image_ruler_x(slint::ModelRc::new(slint::VecModel::from(ticks_x)));
        ui.set_image_ruler_y(slint::ModelRc::new(slint::VecModel::from(ticks_y)));

        ui.set_image_back_to_canvas(self.image.origin_canvas.is_some());
        ui.set_image_doc_w(crate::vm::len(project.width));
        ui.set_image_doc_h(crate::vm::len(project.height));
        ui.set_image_view_k(crate::vm::len(view.k));
        ui.set_image_view_x(crate::vm::len(view.x));
        ui.set_image_view_y(crate::vm::len(view.y));
        ui.set_image_has_selection(self.image.selection.is_some());
        ui.set_image_pan_cursor(self.image.tool == ImageTool::Hand || self.image.space_held);
        ui.set_image_can_undo(self.image.history.can_undo());
        ui.set_image_can_redo(self.image.history.can_redo());
        ui.set_image_menu(self.image.menu.as_str().into());
        ui.set_image_settings_open(self.image.settings_open);
        ui.set_image_show_rulers(self.image.show_rulers);
        ui.set_image_transform_mode(transform_mode_id(self.image.transform_mode).into());
        ui.set_image_mask_view(self.image.mask_view);
        self.sync_image_options(ui);
        self.sync_image_color(ui);
        self.sync_image_rows(ui);
        self.sync_image_fields(ui);
        self.sync_image_extra(ui);
        self.sync_image_dock(ui);
    }

    /// The dock layout: read `dock.json` once (the first render is where the
    /// app's store is in hand) and push only when the layout changed, so the
    /// per-frame sync stays cheap.
    fn sync_image_dock(&mut self, ui: &AppWindow) {
        if !self.image.dock_loaded {
            self.image.dock = crate::image::dock_engine::load(
                &self.store,
                crate::image::dock_engine::STUDIO_IMAGE,
            );
            self.image.dock_loaded = true;
            self.image.dock_dirty = true;
        }
        if self.image.dock_dirty {
            self.image.dock_dirty = false;
            self.sync_dock(ui, crate::image::dock_engine::STUDIO_IMAGE);
        }
    }

    /// The P5c-3 panels' view model: channels/paths/actions/assets/adjustments/
    /// editor/gradients/patterns/brushes plus the filter dialog and the modals.
    #[expect(
        clippy::too_many_lines,
        reason = "flat view-model push for the new dock panels; splitting hides the field list"
    )]
    fn sync_image_extra(&mut self, ui: &AppWindow) {
        use crate::image::assets::{brush_preview, gradient_strip, thumbnail};
        let image = &self.image;

        let mut channels: Vec<crate::ImageChannelRow> = Vec::new();
        let composite_label = ["RGB", "Red", "Green", "Blue"];
        for (index, id) in ["rgb", "r", "g", "b"].iter().enumerate() {
            channels.push(crate::ImageChannelRow {
                id: (*id).into(),
                name: composite_label.get(index).copied().unwrap_or("").into(),
                thumb: slint::Image::default(),
                active: image.channel_view == *id,
                editing: false,
            });
        }
        for channel in image
            .project
            .board_alpha_channels
            .as_ref()
            .map_or(&[][..], Vec::as_slice)
        {
            let thumb = image
                .bitmaps
                .get(&channel.storage_key)
                .map_or_else(slint::Image::default, |surface| {
                    image_of(&thumbnail(surface, 40))
                });
            channels.push(crate::ImageChannelRow {
                id: channel.id.as_str().into(),
                name: channel.name.as_str().into(),
                thumb,
                active: image.channel_view == channel.id,
                editing: image
                    .channel_edit
                    .as_ref()
                    .is_some_and(|(id, _)| *id == channel.id),
            });
        }
        let visibility = crate::image::channels::channel_visibility(&image.project);
        ui.set_image_channels(slint::ModelRc::new(slint::VecModel::from(channels)));
        ui.set_image_channel_r(visibility.r);
        ui.set_image_channel_g(visibility.g);
        ui.set_image_channel_b(visibility.b);
        ui.set_image_channel_view(image.channel_view.as_str().into());

        let paths: Vec<crate::ImagePathRow> = image
            .project
            .board_paths
            .as_ref()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|path| crate::ImagePathRow {
                id: path.id.as_str().into(),
                name: path.name.as_str().into(),
                visible: path.visible,
                active: path.id == image.active_path,
                editing: image
                    .path_edit
                    .as_ref()
                    .is_some_and(|(id, _)| *id == path.id),
                thumb: image_of(&path_thumb(path, PATH_THUMB_SIZE)),
            })
            .collect();
        let active_path_anchors = image
            .project
            .board_paths
            .as_ref()
            .and_then(|paths| paths.iter().find(|path| path.id == image.active_path))
            .map_or(0, |path| path.anchors.len());
        ui.set_image_paths(slint::ModelRc::new(slint::VecModel::from(paths)));
        ui.set_image_path_can_paint(active_path_anchors >= 2);

        let actions: Vec<crate::ImageActionRow> = image
            .assets
            .actions
            .iter()
            .map(|action| crate::ImageActionRow {
                id: action.id.as_str().into(),
                name: action.name.as_str().into(),
                steps: i32::try_from(action.steps.len()).unwrap_or(i32::MAX),
                editing: image
                    .action_edit
                    .as_ref()
                    .is_some_and(|(id, _)| *id == action.id),
            })
            .collect();
        ui.set_image_actions(slint::ModelRc::new(slint::VecModel::from(actions)));
        ui.set_image_recording(image.recorder.recording);
        ui.set_image_recorded(i32::try_from(image.recorder.steps.len()).unwrap_or(i32::MAX));

        let search = image.asset_search.to_lowercase();
        let assets: Vec<crate::ImageAssetRow> = image
            .asset_rows
            .iter()
            .filter(|row| {
                search.is_empty()
                    || row.project_title.to_lowercase().contains(&search)
                    || row.layer_name.to_lowercase().contains(&search)
            })
            .map(|row| crate::ImageAssetRow {
                project_id: row.project_id.as_str().into(),
                layer_id: row.layer_id.as_str().into(),
                title: row.layer_name.as_str().into(),
                sub: row.project_title.as_str().into(),
                thumb: image_of(&row.thumb),
            })
            .collect();
        ui.set_image_assets(slint::ModelRc::new(slint::VecModel::from(assets)));
        ui.set_image_asset_search(image.asset_search.as_str().into());

        let adjustments: Vec<crate::ImageAdjustmentRow> = crate::image::editor::ADJUSTMENT_LABELS
            .iter()
            .map(|(id, label)| crate::ImageAdjustmentRow {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect();
        ui.set_image_adjustments(slint::ModelRc::new(slint::VecModel::from(
            adjustment_groups(adjustments),
        )));

        let active_layer = find_ps_layer(&image.project.board_layers, &image.active_layer_id());
        let editor_visible = match &image.editor_target {
            crate::image::EditorTarget::Adjustment => active_layer
                .is_some_and(|layer| layer.kind == open_canvas_ps::CanvasPsLayerKind::Adjustment),
            crate::image::EditorTarget::Style(id) => active_layer
                .and_then(|layer| layer.styles.as_ref())
                .is_some_and(|styles| styles.iter().any(|style| style.id == *id)),
        };
        ui.set_image_editor_visible(editor_visible);
        let rows = image.editor_rows();
        let title = match (&image.editor_target, active_layer) {
            (crate::image::EditorTarget::Adjustment, Some(layer)) if layer.adjustment.is_some() => {
                crate::image::editor::adjustment_label(
                    layer
                        .adjustment
                        .unwrap_or(open_canvas_ps::CanvasPsAdjustmentType::Invert),
                )
            }
            (crate::image::EditorTarget::Style(id), Some(layer)) => layer
                .styles
                .as_ref()
                .and_then(|styles| styles.iter().find(|style| style.id == *id))
                .map_or("Layer Style", |style| style_name(style.style_type)),
            _ => "Adjustment",
        };
        ui.set_image_editor_title(title.into());
        let slint_rows: Vec<crate::ImageEditorRow> = rows.iter().map(slint_editor_row).collect();
        ui.set_image_editor_rows(slint::ModelRc::new(slint::VecModel::from(slint_rows)));

        let params = image.editor_params().unwrap_or_default();
        let points = crate::image::editor::curve_points(&params);
        let has_curve = rows.iter().any(|row| row.kind == "curve");
        let curve: Vec<crate::ImageCurvePoint> = if has_curve {
            points
                .iter()
                .enumerate()
                .map(|(index, (x, y))| crate::ImageCurvePoint {
                    index: i32::try_from(index).unwrap_or(i32::MAX),
                    x: crate::vm::len(*x),
                    y: crate::vm::len(*y),
                })
                .collect()
        } else {
            Vec::new()
        };
        ui.set_image_curve_points(slint::ModelRc::new(slint::VecModel::from(curve)));
        ui.set_image_curve_path(
            if has_curve {
                crate::image::editor::curve_path(&points)
            } else {
                String::new()
            }
            .into(),
        );

        let stops = crate::image::editor::editor_stops(&params);
        let has_stops = rows.iter().any(|row| row.kind == "stops");
        let stop_rows: Vec<crate::ImageGradientStopRow> = if has_stops {
            stops
                .iter()
                .enumerate()
                .map(|(index, (color, position))| crate::ImageGradientStopRow {
                    index: i32::try_from(index).unwrap_or(i32::MAX),
                    color: color_of(color),
                    position: i32::try_from(crate::vm::f2i((position * 100.0).round()))
                        .unwrap_or(0),
                    active: index == image.editor_stop,
                })
                .collect()
        } else {
            Vec::new()
        };
        let stops_image = |stops: &[(String, f64)]| {
            let presets: Vec<crate::image::assets::PsGradientStop> = stops
                .iter()
                .map(|(color, position)| {
                    crate::image::assets::PsGradientStop::new(color, *position, 0.5)
                })
                .collect();
            image_of(&gradient_strip(&presets, 96, 18))
        };
        ui.set_image_editor_stop_selected(i32::try_from(image.editor_stop).unwrap_or(i32::MAX));
        ui.set_image_editor_stops(slint::ModelRc::new(slint::VecModel::from(stop_rows)));
        ui.set_image_editor_stops_strip(stops_image(&stops));

        let gradients: Vec<crate::ImageGradientRow> = image
            .assets
            .gradients
            .iter()
            .map(|preset| crate::ImageGradientRow {
                id: preset.id.as_str().into(),
                name: preset.name.as_str().into(),
                strip: image_of(&gradient_strip(&preset.stops, 64, 16)),
                active: image.gradient_picked_id == preset.id,
            })
            .collect();
        ui.set_image_gradients(slint::ModelRc::new(slint::VecModel::from(gradients)));
        let edit = image.gradient_edit.clone();
        ui.set_image_gradient_edit_strip(image_of(&gradient_strip(&edit.stops, 96, 18)));
        ui.set_image_gradient_name(edit.name.as_str().into());
        ui.set_image_gradient_stop_selected(i32::try_from(edit.selected).unwrap_or(i32::MAX));
        let edit_stops: Vec<crate::ImageGradientStopRow> = edit
            .stops
            .iter()
            .enumerate()
            .map(|(index, stop)| crate::ImageGradientStopRow {
                index: i32::try_from(index).unwrap_or(i32::MAX),
                color: color_of(&stop.color),
                position: i32::try_from(crate::vm::f2i(
                    (stop.position.clamp(0.0, 1.0) * 100.0).round(),
                ))
                .unwrap_or(0),
                active: index == edit.selected,
            })
            .collect();
        ui.set_image_gradient_stops(slint::ModelRc::new(slint::VecModel::from(edit_stops)));

        let patterns: Vec<crate::ImagePatternRow> = image
            .assets
            .patterns
            .iter()
            .map(|preset| crate::ImagePatternRow {
                id: preset.id.as_str().into(),
                name: preset.name.as_str().into(),
                thumb: image
                    .bitmaps
                    .get(&preset.key)
                    .map_or_else(slint::Image::default, |surface| {
                        image_of(&thumbnail(surface, 40))
                    }),
                active: image.pattern_picked == preset.id,
            })
            .collect();
        ui.set_image_patterns(slint::ModelRc::new(slint::VecModel::from(patterns)));
        ui.set_image_pattern_can_define(
            image.selection.is_some()
                || active_layer.is_some_and(|layer| {
                    matches!(
                        layer.kind,
                        open_canvas_ps::CanvasPsLayerKind::Pixel
                            | open_canvas_ps::CanvasPsLayerKind::Image
                    )
                }),
        );

        let brushes: Vec<crate::ImageBrushRow> = image
            .assets
            .brushes
            .iter()
            .map(|preset| crate::ImageBrushRow {
                id: preset.id.as_str().into(),
                name: preset.name.as_str().into(),
                thumb: image_of(&brush_preview(
                    preset.size.min(24.0),
                    preset.hardness,
                    preset.opacity,
                    crate::image::assets::BrushTool {
                        spacing: preset.spacing,
                        scatter: preset.scatter,
                        angle: preset.angle,
                        roundness: preset.roundness,
                        dynamics: preset.dynamics,
                        texture: preset.texture,
                    },
                    &image.color,
                    88,
                    18,
                )),
                active: image.brush_picked == preset.id,
            })
            .collect();
        ui.set_image_brushes(slint::ModelRc::new(slint::VecModel::from(brushes)));
        ui.set_image_brush_preview(image_of(&brush_preview(
            image.brush_size.min(30.0),
            image.brush_hardness,
            image.brush_opacity,
            image.brush_tool,
            &image.color,
            160,
            32,
        )));
        ui.set_image_brush_spacing(crate::vm::len(image.brush_tool.spacing * 100.0));
        ui.set_image_brush_scatter(crate::vm::len(image.brush_tool.scatter * 100.0));
        ui.set_image_brush_angle(crate::vm::len(image.brush_tool.angle));
        ui.set_image_brush_roundness(crate::vm::len(image.brush_tool.roundness * 100.0));
        ui.set_image_brush_dynamics(crate::vm::len(image.brush_tool.dynamics * 100.0));
        ui.set_image_brush_texture(crate::vm::len(image.brush_tool.texture * 100.0));

        crate::image::filter::sync_filter_ui(self, ui);
        crate::image::menu::sync_modal_ui(self, ui);
    }

    fn sync_image_options(&self, ui: &AppWindow) {
        let image = &self.image;
        ui.set_image_brush(crate::vm::len(image.brush_size));
        ui.set_image_hardness(crate::vm::len(image.brush_hardness * 100.0));
        ui.set_image_opacity(crate::vm::len(image.brush_opacity * 100.0));
        ui.set_image_tolerance(crate::vm::len(image.tolerance));
        ui.set_image_feather(crate::vm::len(image.feather));
        ui.set_image_contiguous(image.contiguous);
        ui.set_image_sample_all(image.sample_all);
        ui.set_image_antialias(image.antialias);
        ui.set_image_gradient_radial(image.gradient_radial);
        ui.set_image_gradient_reverse(image.gradient_reverse);
        ui.set_image_shape_kind(image.shape_kind_id().into());
        ui.set_image_shape_fill(image.shape_fill);
        ui.set_image_stroke_width(crate::vm::len(image.shape_stroke_width));
        ui.set_image_active_tool(self.image.tool.as_str().into());
    }

    fn sync_image_color(&self, ui: &AppWindow) {
        let image = &self.image;
        ui.set_image_color(color_of(&image.color));
        ui.set_image_color_hex(image.color.as_str().into());
        ui.set_image_bg_color(color_of(&image.bg_color));
        ui.set_image_bg_hex(image.bg_color.as_str().into());
        ui.set_image_slot(
            match image.color_slot {
                crate::image::ColorSlot::Foreground => "fg",
                crate::image::ColorSlot::Background => "bg",
            }
            .into(),
        );
        let active = match image.color_slot {
            crate::image::ColorSlot::Foreground => image.color.clone(),
            crate::image::ColorSlot::Background => image.bg_color.clone(),
        };
        let hsb = open_canvas_ps::color::hex_to_hsb(&active);
        let rgb = open_canvas_ps::color::hex_to_rgb_triple(&active);
        ui.set_image_hue(crate::vm::len(hsb[0]));
        ui.set_image_sat(crate::vm::len(hsb[1]));
        ui.set_image_val(crate::vm::len(hsb[2]));
        ui.set_image_red(crate::vm::len(rgb[0]));
        ui.set_image_green(crate::vm::len(rgb[1]));
        ui.set_image_blue(crate::vm::len(rgb[2]));
    }

    #[expect(
        clippy::too_many_lines,
        reason = "flat view-model push for the tool/layer/history/swatch/fx/layers rows; splitting hides the field list"
    )]
    fn sync_image_rows(&mut self, ui: &AppWindow) {
        let tool = self.image.tool;
        let tools: Vec<crate::ImageToolRow> = crate::image::TOOLS
            .iter()
            .map(|(id, label, hotkey)| crate::ImageToolRow {
                id: (*id).into(),
                label: (*label).into(),
                hotkey: (*hotkey).into(),
                active: *id == tool.as_str(),
            })
            .collect();
        self.image_tools.set_vec(tools);

        let order = layer_row_order(&self.image);
        let active_id = self.image.active_layer_id();
        let mask_target = self.image.mask_target.clone();
        let layers: Vec<crate::ImageLayerRow> = {
            let bitmaps = &self.image.bitmaps;
            let cached = &mut self.image.thumb_cache;
            let collapsed = &self.image.collapsed;
            let editing = self.image.editing_layer.as_ref();
            let mut thumb_of = |key: Option<&str>| -> slint::Image {
                let Some(surface) = key.and_then(|key| bitmaps.get(key)) else {
                    return slint::Image::default();
                };
                let key = key.unwrap_or_default();
                cached.image(key, surface, LAYER_THUMB, |surface, max| {
                    image_of(&crate::image::assets::thumbnail(surface, max))
                })
            };
            order
                .into_iter()
                .map(|(layer, child)| {
                    let thumb = thumb_of(layer.storage_key.as_deref());
                    let mask_thumb = thumb_of(layer.mask_storage_key.as_deref());
                    crate::ImageLayerRow {
                        id: layer.id.as_str().into(),
                        name: layer.name.as_str().into(),
                        kind: layer_kind_id(&layer).into(),
                        visible: !layer.hidden,
                        locked: layer.locked,
                        active: layer.id == active_id,
                        has_fx: layer
                            .styles
                            .as_ref()
                            .is_some_and(|styles| !styles.is_empty()),
                        editing: editing.is_some_and(|(id, _)| *id == layer.id),
                        child,
                        group: layer.kind == open_canvas_ps::CanvasPsLayerKind::Group,
                        collapsed: collapsed.contains(&layer.id),
                        thumb,
                        mask_thumb,
                        has_mask: layer.mask_storage_key.is_some(),
                        mask_target: mask_target == layer.id,
                    }
                })
                .collect()
        };
        self.image_layers.set_vec(layers);

        let (position, len) = self.image.history.position();
        let history: Vec<crate::ImageHistoryRow> = (0..len)
            .map(|index| crate::ImageHistoryRow {
                index: i32::try_from(index).unwrap_or(i32::MAX),
                label: self.image.history.label(index).unwrap_or_default().into(),
                active: index == position,
                future: index > position,
            })
            .collect();
        self.image_history.set_vec(history);

        let current = match self.image.color_slot {
            crate::image::ColorSlot::Foreground => {
                open_canvas_ps::color::normalize_hex(&self.image.color)
            }
            crate::image::ColorSlot::Background => {
                open_canvas_ps::color::normalize_hex(&self.image.bg_color)
            }
        };
        let swatches: Vec<crate::ImageSwatch> = self
            .image
            .swatches
            .iter()
            .enumerate()
            .map(|(index, hex)| {
                let normalized = open_canvas_ps::color::normalize_hex(hex);
                crate::ImageSwatch {
                    index: i32::try_from(index).unwrap_or(i32::MAX),
                    color: color_of(if normalized.is_empty() {
                        "#000000"
                    } else {
                        &normalized
                    }),
                    active: normalized.eq_ignore_ascii_case(&current),
                }
            })
            .collect();
        self.image_swatches
            .set_vec(group_rows(swatches, 6, |items| crate::ImageSwatchGroup {
                items: slint::ModelRc::new(slint::VecModel::from(items)),
            }));

        let styles: Vec<crate::ImageStyleRow> = find_ps_layer(
            &self.image.project.board_layers,
            &self.image.active_layer_id(),
        )
        .and_then(|layer| layer.styles.clone())
        .unwrap_or_default()
        .iter()
        .map(|style| crate::ImageStyleRow {
            id: style.id.as_str().into(),
            name: style_name(style.style_type).into(),
            enabled: style.enabled,
        })
        .collect();
        self.image_styles.set_vec(styles);

        ui.set_image_panels(crate::ImagePanels {
            layers: self.image.panels.layers,
            fx: self.image.panels.fx,
            history: self.image.panels.history,
            properties: self.image.panels.properties,
            color: self.image.panels.color,
            channels: self.image.panels.channels,
            paths: self.image.panels.paths,
            actions: self.image.panels.actions,
            assets: self.image.panels.assets,
            adjustments: self.image.panels.adjustments,
            gradients: self.image.panels.gradients,
            patterns: self.image.panels.patterns,
            brushes: self.image.panels.brushes,
        });
    }

    #[expect(
        clippy::too_many_lines,
        reason = "flat view-model field push for the properties panel; splitting hides the field list"
    )]
    fn sync_image_fields(&mut self, ui: &AppWindow) {
        let image = &self.image;
        let active = find_ps_layer(&image.project.board_layers, &image.active_layer_id()).cloned();
        let (kind, adjustment, name) = active.as_ref().map_or(("", "", ""), |layer| {
            (
                layer_kind_id(layer),
                layer.adjustment.map_or("", adjustment_id),
                layer.name.as_str(),
            )
        });
        ui.set_image_active_kind(kind.into());
        ui.set_image_adjustment(adjustment.into());
        ui.set_image_layer_name(name.into());
        let params: Vec<crate::ImageParamRow> = active
            .as_ref()
            .and_then(|layer| layer.adjustment_params.as_ref())
            .map(|params| {
                params
                    .iter()
                    .filter_map(|(key, value)| match value {
                        CanvasPsParamValue::Number(number) => {
                            let (min, max) = param_range(key);
                            Some(crate::ImageParamRow {
                                key: key.as_str().into(),
                                label: key.as_str().into(),
                                value: crate::vm::len(*number),
                                min: crate::vm::len(min),
                                max: crate::vm::len(max),
                            })
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.image_params.set_vec(params);
        ui.set_image_layer_x(
            active
                .as_ref()
                .map_or("0".into(), |layer| layer.x.round().to_string().into()),
        );
        ui.set_image_layer_y(
            active
                .as_ref()
                .map_or("0".into(), |layer| layer.y.round().to_string().into()),
        );
        ui.set_image_layer_w(
            active
                .as_ref()
                .map_or("0".into(), |layer| layer.width.round().to_string().into()),
        );
        ui.set_image_layer_h(
            active
                .as_ref()
                .map_or("0".into(), |layer| layer.height.round().to_string().into()),
        );
        ui.set_image_layer_rotation(active.as_ref().map_or("0".into(), |layer| {
            layer.rotation.round().to_string().into()
        }));
        ui.set_image_layer_opacity(crate::vm::len(
            active
                .as_ref()
                .map_or(100.0, |layer| (layer.opacity * 100.0).round())
                .max(0.0),
        ));
        ui.set_image_layer_blend(
            active
                .as_ref()
                .map_or("normal", |layer| layer.blend_mode.as_str())
                .into(),
        );
        let text = active
            .as_ref()
            .and_then(|layer| layer.text.clone())
            .unwrap_or_default();
        ui.set_image_text_content(text.as_str().into());
        ui.set_image_text_family(
            active
                .as_ref()
                .and_then(|layer| layer.font_family.clone())
                .unwrap_or_else(|| "sans-serif".to_string())
                .into(),
        );
        ui.set_image_board(self.image.project.width.round().to_string().into());
        ui.set_image_board_h(self.image.project.height.round().to_string().into());
        ui.set_image_ratio(self.image.project.board_ratio.as_str().into());
        ui.set_image_resolution(
            match self.image.project.board_resolution {
                open_canvas_ps::doc::BoardResolution::K1 => "1k",
                open_canvas_ps::doc::BoardResolution::K2 => "2k",
                open_canvas_ps::doc::BoardResolution::K4 => "4k",
            }
            .into(),
        );
        ui.set_image_background(self.image.project.board_background.as_str().into());
        ui.set_image_background_opacity(crate::vm::len(
            (self.image.project.board_background_opacity * 100.0).round(),
        ));
        let has_mask = active
            .as_ref()
            .is_some_and(|layer| layer.mask_storage_key.is_some());
        ui.set_image_layer_has_mask(has_mask);
        ui.set_image_layer_mask_target(self.image.mask_target == self.image.active_layer_id());
        ui.set_image_layer_can_add_mask(
            active
                .as_ref()
                .is_some_and(|layer| layer.kind != open_canvas_ps::CanvasPsLayerKind::Group),
        );
        ui.set_image_layer_can_apply_mask(
            has_mask
                && active
                    .as_ref()
                    .is_some_and(|layer| layer.storage_key.is_some()),
        );
        let text_rows = crate::image::text::layer_panel_rows(
            active.as_ref(),
            self.image.project.board_paths.as_deref().unwrap_or(&[]),
            self.image.text_tab,
        );
        let slint_rows: Vec<crate::ImageEditorRow> =
            text_rows.iter().map(slint_editor_row).collect();
        ui.set_image_text_rows(slint::ModelRc::new(slint::VecModel::from(slint_rows)));
        match &self.image.import {
            Some(import) => {
                ui.set_image_import_open(true);
                ui.set_image_import_path(import.path.as_str().into());
                ui.set_image_import_message(import.message.as_str().into());
                ui.set_image_import_error(import.error);
            }
            None => ui.set_image_import_open(false),
        }
        match &self.image.export {
            Some(export) => {
                ui.set_image_export_open(true);
                ui.set_image_export_path(export.path.as_str().into());
                ui.set_image_export_message(export.message.as_str().into());
                ui.set_image_export_error(export.error);
            }
            None => ui.set_image_export_open(false),
        }
        match &self.image.psd_export {
            Some(export) => {
                ui.set_image_psd_export_open(true);
                ui.set_image_psd_export_path(export.path.as_str().into());
                ui.set_image_psd_export_message(export.message.as_str().into());
                ui.set_image_psd_export_error(export.error);
            }
            None => ui.set_image_psd_export_open(false),
        }

        // In-canvas text editor overlay: visible only while a draft is open.
        let editing = self.image.text_edit.open().is_some() && self.image.meta.is_some();
        ui.set_image_text_edit_on(editing);
        let draft = self
            .image
            .text_edit
            .open()
            .map_or_else(String::new, |edit| edit.draft.clone());
        ui.set_image_text_edit_draft(draft.as_str().into());
        let (x, y, width, height) = self.image_text_edit_rect().unwrap_or((0.0, 0.0, 0.0, 0.0));
        ui.set_image_text_edit_x(crate::vm::len(x));
        ui.set_image_text_edit_y(crate::vm::len(y));
        ui.set_image_text_edit_w(crate::vm::len(width));
        ui.set_image_text_edit_h(crate::vm::len(height));
    }
}

/// Kebab id of a transform mode (the tool-options chips); "" = disarmed.
pub(crate) fn transform_mode_id(mode: Option<open_canvas_ps::PsTransformMode>) -> &'static str {
    match mode {
        Some(open_canvas_ps::PsTransformMode::Free) => "free",
        Some(open_canvas_ps::PsTransformMode::Skew) => "skew",
        Some(open_canvas_ps::PsTransformMode::Distort) => "distort",
        Some(open_canvas_ps::PsTransformMode::Perspective) => "perspective",
        Some(open_canvas_ps::PsTransformMode::Warp) => "warp",
        None => "",
    }
}

/// Kebab id of a layer kind (`layerKindId`, image-studio).
pub(crate) fn layer_kind_id(layer: &CanvasPsLayer) -> &'static str {
    match layer.kind {
        open_canvas_ps::CanvasPsLayerKind::Image => "image",
        open_canvas_ps::CanvasPsLayerKind::Text => "text",
        open_canvas_ps::CanvasPsLayerKind::Group => "group",
        open_canvas_ps::CanvasPsLayerKind::Pixel => "pixel",
        open_canvas_ps::CanvasPsLayerKind::Shape => "shape",
        open_canvas_ps::CanvasPsLayerKind::Adjustment => "adjustment",
    }
}

/// Display name of a layer style (fx panel rows).
pub(crate) fn style_name(kind: open_canvas_ps::CanvasPsLayerStyleType) -> &'static str {
    crate::image::editor::style_label(kind)
}

/// Chunks a flat row list into grid rows of `width` items (Slint grid cells
/// need static positions, so the model carries the grouping).
fn group_rows<T, G>(rows: Vec<T>, width: usize, wrap: impl Fn(Vec<T>) -> G) -> Vec<G> {
    let mut groups = Vec::new();
    let mut rows = rows;
    while !rows.is_empty() {
        let rest = rows.split_off(rows.len().min(width));
        groups.push(wrap(rows));
        rows = rest;
    }
    groups
}

/// The 15 adjustment chips as the panel's 8-column grid rows
/// (`ps-adjustments-panel.tsx` `grid-cols-8`).
pub(crate) fn adjustment_groups(
    rows: Vec<crate::ImageAdjustmentRow>,
) -> Vec<crate::ImageAdjustmentGroup> {
    group_rows(rows, 8, |items| crate::ImageAdjustmentGroup {
        items: slint::ModelRc::new(slint::VecModel::from(items)),
    })
}

/// One `EditorRow` as the panel's `ImageEditorRow`; colour rows parse their
/// hex `text` so the swatch shows the named colour instead of a fixed grey.
pub(crate) fn slint_editor_row(row: &crate::image::editor::EditorRow) -> crate::ImageEditorRow {
    let color = if row.kind == "color" {
        let hex = open_canvas_ps::color::normalize_hex(&row.text);
        color_of(if hex.is_empty() { "#000000" } else { &hex })
    } else {
        color_of("#000000")
    };
    crate::ImageEditorRow {
        key: row.key.as_str().into(),
        label: row.label.as_str().into(),
        kind: row.kind.into(),
        value: crate::vm::len(row.value),
        min: crate::vm::len(row.min),
        max: crate::vm::len(row.max),
        text: row.text.as_str().into(),
        options: slint::ModelRc::new(slint::VecModel::from(
            row.options
                .iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        )),
        checked: row.checked,
        color,
    }
}

impl ImageState {
    pub(crate) fn shape_kind_id(&self) -> &'static str {
        match self.shape_kind {
            open_canvas_ps::CanvasPsShapeKind::Rectangle => "rectangle",
            open_canvas_ps::CanvasPsShapeKind::RoundedRectangle => "rounded-rectangle",
            open_canvas_ps::CanvasPsShapeKind::Ellipse => "ellipse",
            open_canvas_ps::CanvasPsShapeKind::Polygon => "polygon",
            open_canvas_ps::CanvasPsShapeKind::Line => "line",
        }
    }
}

/// Kebab id of an adjustment kind (serde's `rename_all = "kebab-case"` names).
pub(crate) fn adjustment_id(kind: open_canvas_ps::CanvasPsAdjustmentType) -> &'static str {
    use open_canvas_ps::CanvasPsAdjustmentType as Adjustment;
    match kind {
        Adjustment::BrightnessContrast => "brightness-contrast",
        Adjustment::Levels => "levels",
        Adjustment::Curves => "curves",
        Adjustment::Exposure => "exposure",
        Adjustment::Vibrance => "vibrance",
        Adjustment::HueSaturation => "hue-saturation",
        Adjustment::ColorBalance => "color-balance",
        Adjustment::BlackWhite => "black-white",
        Adjustment::PhotoFilter => "photo-filter",
        Adjustment::ChannelMixer => "channel-mixer",
        Adjustment::GradientMap => "gradient-map",
        Adjustment::Invert => "invert",
        Adjustment::Posterize => "posterize",
        Adjustment::Threshold => "threshold",
        Adjustment::SelectiveColor => "selective-color",
    }
}

#[cfg(test)]
#[path = "render_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageView;

    fn project() -> ImageProject {
        let mut project = ImageState::default().project;
        project.width = 100.0;
        project.height = 60.0;
        project
    }

    /// Parses the painter's `M x y L x2 y L x2 y2 L x y2 Z` quads into
    /// `(x, y, width, height)` rectangles.
    fn checker_quads(commands: &str) -> Vec<(f64, f64, f64, f64)> {
        commands
            .split('Z')
            .filter_map(|quad| {
                let values: Vec<f64> = quad
                    .split_whitespace()
                    .filter_map(|token| token.parse::<f64>().ok())
                    .collect();
                if values.len() < 6 {
                    return None;
                }
                Some((
                    values[0],
                    values[1],
                    values[2] - values[0],
                    values[5] - values[1],
                ))
            })
            .collect()
    }

    #[test]
    fn checker_tiles_stay_small_and_screen_constant_across_zoom() {
        for zoom in [0.5, 1.0, 2.0] {
            let view = ImageView {
                x: 0.0,
                y: 0.0,
                k: zoom,
            };
            let quads = checker_quads(&checker_path(&project(), &view, (400.0, 300.0)));
            assert!(!quads.is_empty(), "the checker painted at {zoom}x");
            for (_, _, width, height) in &quads {
                assert!(
                    *width > 0.0 && *width <= CHECKER_TILE && *height > 0.0 && *height <= CHECKER_TILE,
                    "a tile is at most one {CHECKER_TILE}px screen cell at {zoom}x ({width}x{height})"
                );
            }
            // The 100% tile is in the small range the studio wants (6-8px), and
            // zooming never scales it: an interior tile is exactly 8px wide.
            assert!(
                quads
                    .iter()
                    .any(|(_, _, w, h)| *w == CHECKER_TILE && *h == CHECKER_TILE),
                "an interior tile is {CHECKER_TILE}px at {zoom}x"
            );
            if (zoom - 1.0).abs() < f64::EPSILON {
                let (_, _, width, _) = quads[0];
                assert!(
                    (6.0..=8.0).contains(&width),
                    "the tile edge at 100% is within 6-8px ({width})"
                );
            }
        }
    }

    #[test]
    fn checker_never_paints_outside_the_document_or_the_canvas() {
        let view = ImageView {
            x: 37.3,
            y: -20.5,
            k: 2.0,
        };
        let canvas = (300.0, 200.0);
        let commands = checker_path(&project(), &view, canvas);
        let quads = checker_quads(&commands);
        assert!(!quads.is_empty(), "a partly visible document still paints");
        // The commands are document-local: `0,0` is the document's top-left, so
        // the document is `0..width*k` × `0..height*k` and the visible canvas
        // slice is the document box offset by the view transform.
        let (doc_w, doc_h) = (200.0, 120.0);
        for (x, y, width, height) in quads {
            assert!(x >= 0.0 && x + width <= doc_w, "tile inside the document x");
            assert!(
                y >= 0.0 && y + height <= doc_h,
                "tile inside the document y"
            );
            assert!(
                x + view.x >= 0.0 && x + width + view.x <= canvas.0,
                "tile inside the canvas x"
            );
            assert!(
                y + view.y >= 0.0 && y + height + view.y <= canvas.1,
                "tile inside the canvas y"
            );
        }

        // Fully outside the viewport (to the right/below) paints nothing.
        let offscreen = ImageView {
            x: -5000.0,
            y: -5000.0,
            k: 2.0,
        };
        assert_eq!(checker_path(&project(), &offscreen, (300.0, 200.0)), "");
        // A viewport entirely left/above the document paints nothing either.
        let before = ImageView {
            x: 4000.0,
            y: 4000.0,
            k: 2.0,
        };
        assert_eq!(checker_path(&project(), &before, (300.0, 200.0)), "");
    }

    #[test]
    fn checker_quad_count_is_capped_for_a_zoomed_8k_document() {
        let mut project = project();
        project.width = 7680.0;
        project.height = 4320.0;
        // A zoomed-in slice: only the viewport is in view, but the document
        // itself spans tens of thousands of screen pixels.
        let view = ImageView {
            x: -30_000.0,
            y: -17_000.0,
            k: 8.0,
        };
        let quads = checker_quads(&checker_path(&project, &view, (3840.0, 2160.0)));
        assert!(
            !quads.is_empty(),
            "the visible slice still paints a checker"
        );
        assert!(
            quads.len() <= CHECKER_MAX_QUADS,
            "the checker stays within the quad budget ({})",
            quads.len()
        );
    }

    #[test]
    fn board_composite_renders_layers_through_the_ps_compositor(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut project = project();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            open_canvas_ps::CanvasPsLayerKind::Pixel,
            open_canvas_ps::Bounds::new(0.0, 0.0, 100.0, 60.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = std::collections::HashMap::new();
        let mut surface = Surface::new(100, 60);
        for y in 0..40 {
            for x in 0..40 {
                surface.set(x, y, [250, 10, 10, 255]);
            }
        }
        bitmaps.insert("image:l1".to_string(), surface);

        let composite = board_surface(&project, &bitmaps).ok_or("composite")?;
        assert_eq!((composite.width(), composite.height()), (100, 60));
        assert_eq!(composite.get(20, 30), [250, 10, 10, 255]);
        assert_eq!(composite.get(90, 50), [0, 0, 0, 0]);
        Ok(())
    }

    #[test]
    fn editor_color_rows_carry_the_parsed_swatch_colour() -> Result<(), Box<dyn std::error::Error>>
    {
        let params = open_canvas_ps::doc::PsParams::new();
        let rows = crate::image::editor::adjustment_rows(
            open_canvas_ps::CanvasPsAdjustmentType::PhotoFilter,
            &params,
        );
        let row = rows
            .iter()
            .find(|row| row.kind == "color")
            .ok_or("photo-filter colour row")?;
        assert_eq!(row.text, "#ec8a00", "the default filter colour");
        assert_eq!(
            slint_editor_row(row).color,
            color_of(&open_canvas_ps::color::normalize_hex("#ec8a00")),
            "the swatch mirrors the row hex"
        );
        Ok(())
    }

    #[test]
    fn a_path_row_thumbnail_renders_its_polyline() {
        let path = open_canvas_ps::create_path(
            "p-1",
            "Diagonal",
            vec![
                open_canvas_ps::path_anchor(0.0, 0.0),
                open_canvas_ps::path_anchor(40.0, 40.0),
            ],
            false,
        );
        let thumb = path_thumb(&path, PATH_THUMB_SIZE);
        assert_eq!(thumb.width(), PATH_THUMB_SIZE);
        assert_eq!(thumb.height(), PATH_THUMB_SIZE);
        let painted = thumb
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 0)
            .count();
        assert!(
            painted > 20,
            "the polyline is stroked into the thumbnail ({painted})"
        );
        // The diagonal is symmetric: the top-left pixel is painted, the
        // top-right corner is not.
        assert!(
            thumb.get(4, 4)[3] > 0,
            "the diagonal starts near the origin"
        );
        let right_edge = i64::try_from(PATH_THUMB_SIZE).unwrap_or(0) - 2;
        assert_eq!(
            thumb.get(right_edge, 2)[3],
            0,
            "the empty corner stays transparent"
        );
        // An empty path paints nothing.
        let empty = open_canvas_ps::create_path("p-2", "Empty", Vec::new(), false);
        assert_eq!(
            path_thumb(&empty, PATH_THUMB_SIZE)
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|px| px[3] > 0)
                .count(),
            0
        );
    }

    #[test]
    fn the_paths_model_carries_a_thumbnail_per_row() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let ui = crate::AppWindow::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let app = std::rc::Rc::new(std::cell::RefCell::new(App::new(store)));
        crate::events::wire(&ui, &app);
        crate::app::with_app(&app, |app| {
            app.view = crate::app::View::Image;
            app.image.project.board_paths = Some(vec![open_canvas_ps::create_path(
                "p-1",
                "Line",
                vec![
                    open_canvas_ps::path_anchor(0.0, 0.0),
                    open_canvas_ps::path_anchor(20.0, 20.0),
                ],
                false,
            )]);
            app.sync_image(&ui);
        });
        let paths = ui.get_image_paths();
        assert_eq!(slint::Model::row_count(&paths), 1);
        let row = slint::Model::row_data(&paths, 0).ok_or("path row")?;
        assert_eq!(row.name.as_str(), "Line");
        let size = row.thumb.size();
        let expected = u32::try_from(PATH_THUMB_SIZE).unwrap_or(0);
        assert_eq!(
            (size.width, size.height),
            (expected, expected),
            "the row carries a rendered thumbnail"
        );
        Ok(())
    }

    #[test]
    fn fifteen_adjustments_wrap_into_eight_column_rows() {
        let rows: Vec<crate::ImageAdjustmentRow> = crate::image::editor::ADJUSTMENT_LABELS
            .iter()
            .map(|(id, label)| crate::ImageAdjustmentRow {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect();
        let sizes: Vec<usize> = adjustment_groups(rows)
            .iter()
            .map(|group| slint::Model::row_count(&group.items))
            .collect();
        assert_eq!(sizes, vec![8, 7], "the grid is 8 columns wide");
    }

    #[test]
    fn text_layers_render_glyphs_in_the_board_composite() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut project = project();
        let mut layer = open_canvas_ps::base_layer(
            "t1".to_string(),
            "Text",
            open_canvas_ps::CanvasPsLayerKind::Text,
            open_canvas_ps::Bounds::new(10.0, 10.0, 80.0, 30.0),
        );
        layer.text = Some("Ab".to_string());
        layer.font_size = Some(24.0);
        layer.color = Some("#00ff00".to_string());
        project.board_layers = vec![layer];
        let empty = std::collections::HashMap::new();
        let composite = board_surface(&project, &empty).ok_or("composite")?;
        let ink = composite
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(ink > 10, "the text layer contributes glyph pixels");

        project.board_layers[0].hidden = true;
        let hidden = board_surface(&project, &empty).ok_or("composite")?;
        assert_eq!(
            hidden
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .count(),
            0,
            "a hidden text layer draws nothing"
        );
        Ok(())
    }

    #[test]
    fn grid_lines_are_culled_to_the_board() {
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 2.0,
        };
        let grid = grid_commands(&project(), &view, (400.0, 300.0));
        // 100x60 doc at 2x: x = 0,50,100 and y = 0,50; the x=100 line lands on
        // 200.5px and is culled by the board-edge check, so 4 lines survive.
        assert_eq!(grid.matches("M ").count(), 4);
        let grid = grid_commands(&project(), &view, (10.0, 10.0));
        assert!(grid.matches("M ").count() <= 5);
    }

    #[test]
    fn ants_follow_the_selection_boundary_with_both_dash_parities() {
        let mut selection = Surface::new(4, 4);
        for y in 0..2 {
            for x in 0..2 {
                selection.set(x, y, [255, 255, 255, 255]);
            }
        }
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 2.0,
        };
        let dark = ants_commands(&selection, &view, false, 0);
        let light = ants_commands(&selection, &view, true, 0);
        assert!(dark.matches("M ").count() > 0);
        assert!(light.matches("M ").count() > 0);
        // The 2x2 block has 4 boundary pixels; the outer square has 8 sides plus
        // interior cross edges => at least 8 segments overall.
        assert!(dark.matches("M ").count() >= 4);
        // The light half follows the marching phase.
        let advanced = ants_commands(&selection, &view, true, crate::image::ANTS_PHASE_STEP);
        assert_ne!(light, advanced, "the phase marches the light dashes");
    }

    #[test]
    fn draft_draws_the_marquee_and_gradient_gestures() {
        let view = ImageView {
            x: 10.0,
            y: 20.0,
            k: 2.0,
        };
        let marquee = Gesture::Marquee {
            tool: ImageTool::Marquee,
            start: Point::new(1.0, 1.0),
            current: Point::new(4.0, 3.0),
            shift: false,
            alt: false,
        };
        let commands = draft_commands(Some(&marquee), &view);
        assert!(commands.starts_with("M 12 22"), "unexpected {commands}");
        assert!(commands.ends_with("Z "));

        let gradient = Gesture::Gradient {
            start: Point::new(0.0, 0.0),
            current: Point::new(5.0, 5.0),
        };
        let commands = draft_commands(Some(&gradient), &view);
        assert!(
            commands.contains("M 10 20 L 20 30"),
            "unexpected {commands}"
        );
        assert_eq!(draft_commands(None, &view), "");
    }

    #[test]
    fn transform_handles_wrap_the_active_layer_box() {
        let mut project = project();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            open_canvas_ps::CanvasPsLayerKind::Pixel,
            open_canvas_ps::Bounds::new(10.0, 5.0, 30.0, 20.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        project.board_layers = vec![layer];
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        // No armed transform: no overlay at all (the old Move tool used to wear
        // a selection-blue box around the active layer, which read as a
        // document outline whenever the layer covered the board).
        assert_eq!(transform_commands(&project, "l1", &view, None), "");
        // An armed transform shows the box and its handles.
        let commands = transform_commands(
            &project,
            "l1",
            &view,
            Some(open_canvas_ps::PsTransformMode::Free),
        );
        assert!(commands.starts_with("M 10 5"), "unexpected {commands}");
        assert!(commands.contains("Z "));
        assert_eq!(
            transform_commands(
                &project,
                "missing",
                &view,
                Some(open_canvas_ps::PsTransformMode::Free)
            ),
            ""
        );
    }

    #[test]
    fn layer_rows_nest_group_children_and_collapse_hides_them() {
        let mut state = crate::image::ImageState::default();
        let mut gen = open_canvas_ps::IdGen::new("t");
        let bottom = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Bottom");
        let middle = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Middle");
        let top = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Top");
        state.project.board_layers = vec![bottom.clone(), middle.clone(), top.clone()];
        state.project.board_layers = open_canvas_ps::group_ps_layers(
            &state.project.board_layers,
            std::slice::from_ref(&middle.id),
            "g-1",
            "Group",
        );

        let rows = layer_row_order(&state);
        let labels: Vec<(&str, bool)> = rows
            .iter()
            .map(|(layer, child)| (layer.name.as_str(), *child))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("Top", false),
                ("Group", false),
                ("Middle", true),
                ("Bottom", false)
            ],
            "top layer first, the group's child indented under it"
        );

        state.toggle_collapse("g-1");
        let rows = layer_row_order(&state);
        let labels: Vec<&str> = rows.iter().map(|(layer, _)| layer.name.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Top", "Group", "Bottom"],
            "collapsed hides children"
        );
        assert!(state.is_collapsed("g-1"));
        state.toggle_collapse("g-1");
        assert!(!state.is_collapsed("g-1"));
    }

    #[test]
    fn ellipse_marquee_draft_is_an_ellipse_not_a_rectangle() {
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        let ellipse = Gesture::Marquee {
            tool: crate::image::ImageTool::EllipseMarquee,
            start: Point::new(10.0, 10.0),
            current: Point::new(40.0, 30.0),
            shift: false,
            alt: false,
        };
        let commands = draft_commands(Some(&ellipse), &view);
        assert!(
            commands.contains(" A "),
            "the ellipse preview uses arc segments: {commands}"
        );
        assert!(
            !commands.contains(" L "),
            "the ellipse preview is not the bounding rectangle: {commands}"
        );

        let rectangle = Gesture::Marquee {
            tool: crate::image::ImageTool::Marquee,
            start: Point::new(10.0, 10.0),
            current: Point::new(40.0, 30.0),
            shift: false,
            alt: false,
        };
        let commands = draft_commands(Some(&rectangle), &view);
        assert!(
            commands.contains(" L "),
            "the rectangular marquee still draws its box: {commands}"
        );
    }

    #[test]
    fn path_overlay_paints_visible_paths_and_anchors_only_for_the_active_one() {
        let mut project = project();
        let mut hidden = open_canvas_ps::create_path(
            "p-2",
            "Hidden",
            vec![
                open_canvas_ps::path_anchor(30.0, 30.0),
                open_canvas_ps::path_anchor(40.0, 30.0),
            ],
            false,
        );
        hidden.visible = false;
        project.board_paths = Some(vec![
            open_canvas_ps::create_path(
                "p-1",
                "Active",
                vec![
                    open_canvas_ps::path_anchor(10.0, 10.0),
                    open_canvas_ps::path_anchor(20.0, 10.0),
                ],
                false,
            ),
            hidden,
        ]);
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        let commands = path_overlay_commands(&project, "p-1", &view);
        assert!(
            commands.contains("M 10 10") && commands.contains("L 20 10"),
            "the active path stroke is painted: {commands}"
        );
        assert!(
            !commands.contains("M 30 30"),
            "a hidden path is neither painted nor clickable: {commands}"
        );
        project.board_paths.as_mut().expect("paths")[1].visible = true;
        let commands = path_overlay_commands(&project, "p-1", &view);
        assert!(
            commands.contains("M 30 30"),
            "every visible path stays on the canvas: {commands}"
        );
    }

    #[test]
    fn guide_drafts_and_modes_render_through_the_sync_helpers() {
        let project = project();
        let view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        let draft = line_commands(&project, &view, &[(false, 25.0)]);
        assert!(
            draft.contains("M 25.5 "),
            "the draft guide draws at its doc x: {draft}"
        );
        let guides = line_commands(&project, &view, &[(true, 10.0), (false, 30.0)]);
        assert!(
            guides.contains("M 0 10.5") && guides.contains("M 30.5 "),
            "both axes render: {guides}"
        );

        assert_eq!(transform_mode_id(None), "");
        assert_eq!(
            transform_mode_id(Some(open_canvas_ps::PsTransformMode::Warp)),
            "warp"
        );
        assert_eq!(
            transform_mode_id(Some(open_canvas_ps::PsTransformMode::Free)),
            "free"
        );
    }
}
