//! Board compositor — the raster port of `renderPsDocument`/`drawPs*` from
//! `web/src/lib/canvas/smart-canvas.ts`. It draws pixel/image layers (bitmaps
//! supplied headlessly through a storage-key map), groups, adjustment layers,
//! masks, shapes, per-layer rotation and opacity, and the 16 blend modes.
//!
//! Text layers get their glyphs from a host-supplied [`TextRasterizer`] (a font
//! rasteriser is an environment concern; the host owns the font files). Without
//! one they stay transparent, and `crate::text` still provides the pure
//! layout/warp math the host builds its rasteriser from.

use std::collections::HashMap;

use crate::adjustments::apply_ps_adjustment;
use crate::blend::{clamp_layer_opacity, resolve_blend_mode, BlendMode};
use crate::board::{
    base_layer, group_children, read_media_dimensions, smart_canvas_background, smart_canvas_ratio,
    top_layers,
};
use crate::composite::blend_pixel;
use crate::doc::{
    CanvasPsLayer, CanvasPsLayerKind, CanvasPsPath, CanvasPsShapeKind, SmartCanvasBoard,
};
use crate::geom::{rotate_point, Point};
use crate::selection::fill_polygon_mask;
use crate::surface::{clamp_u8, hex_to_rgba, px_size, Surface};
use crate::transform::{transform_mesh, warp_mesh};

/// Host glyph rasteriser for text layers (`drawPsTextLayer`). The callback
/// receives the layer, the board's paths (for text-on-path), the layer box in
/// pixels and the vertical scale factor the web passes as `scale`
/// (`smart-canvas.ts` `drawPsLayerContent`), and returns the layer-local
/// RGBA surface. `None` keeps the layer transparent.
pub trait TextRasterizer {
    fn rasterize(
        &self,
        layer: &CanvasPsLayer,
        paths: &[CanvasPsPath],
        width: usize,
        height: usize,
        scale: f64,
    ) -> Option<Surface>;
}

/// `renderPsDocument` (smart-canvas.ts:309): the whole board at export size.
/// Text layers stay transparent; hosts with a font rasteriser call
/// [`render_document_with_text`].
pub fn render_document(board: &SmartCanvasBoard, bitmaps: &HashMap<String, Surface>) -> Surface {
    render_document_with_text(board, bitmaps, None)
}

/// [`render_document`] with a host glyph rasteriser for text layers.
pub fn render_document_with_text(
    board: &SmartCanvasBoard,
    bitmaps: &HashMap<String, Surface>,
    text: Option<&dyn TextRasterizer>,
) -> Surface {
    let (target_w, target_h) =
        read_media_dimensions(board.board_resolution, smart_canvas_ratio(board));
    let width = px_size(target_w);
    let height = px_size(target_h);
    let mut out = Surface::new(width, height);

    let background = smart_canvas_background(board);
    if background != "transparent" {
        let mut rgba = hex_to_rgba(background);
        let opacity = clamp_layer_opacity(Some(board.board_background_opacity));
        rgba[3] = clamp_u8(f64::from(rgba[3]) * opacity);
        out.fill(rgba);
    }

    let scale_x = width as f64 / board.width.max(1.0);
    let scale_y = height as f64 / board.height.max(1.0);
    let paths = board.board_paths.as_slice();
    for layer in top_layers(&board.board_layers) {
        if layer.hidden {
            continue;
        }
        if layer.kind == CanvasPsLayerKind::Group {
            draw_group(
                &mut out, board, &layer, scale_x, scale_y, bitmaps, paths, text,
            );
        } else {
            draw_layer(&mut out, &layer, scale_x, scale_y, bitmaps, paths, text);
        }
    }
    apply_channel_visibility(&mut out, board);
    out
}

/// `psApplyChannelVisibility` (smart-canvas.ts:348).
fn apply_channel_visibility(out: &mut Surface, board: &SmartCanvasBoard) {
    let Some(channels) = board.board_channel_visibility else {
        return;
    };
    if channels.r && channels.g && channels.b {
        return;
    }
    let data = out.data_mut();
    for index in (0..data.len()).step_by(4) {
        if !channels.r {
            data[index] = 0;
        }
        if !channels.g {
            data[index + 1] = 0;
        }
        if !channels.b {
            data[index + 2] = 0;
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "drawPs* mirror: the compositor threads output, layer, scales, bitmaps, paths and the text hook like the web's closures"
)]
fn draw_group(
    out: &mut Surface,
    board: &SmartCanvasBoard,
    group: &CanvasPsLayer,
    scale_x: f64,
    scale_y: f64,
    bitmaps: &HashMap<String, Surface>,
    paths: &[CanvasPsPath],
    text: Option<&dyn TextRasterizer>,
) {
    let mut isolated = Surface::new(out.width(), out.height());
    for child in group_children(&board.board_layers, group) {
        if child.hidden {
            continue;
        }
        draw_layer(
            &mut isolated,
            &child,
            scale_x,
            scale_y,
            bitmaps,
            paths,
            text,
        );
    }
    if let Some(mask) = layer_mask(group, out.width(), out.height(), scale_x, scale_y, bitmaps) {
        multiply_alpha(&mut isolated, &mask);
    }
    let mode = resolve_blend_mode(&group.blend_mode);
    let opacity = clamp_layer_opacity(Some(group.opacity));
    blend_full(out, &isolated, mode, opacity);
}

fn draw_layer(
    out: &mut Surface,
    layer: &CanvasPsLayer,
    scale_x: f64,
    scale_y: f64,
    bitmaps: &HashMap<String, Surface>,
    paths: &[CanvasPsPath],
    text: Option<&dyn TextRasterizer>,
) {
    if layer.kind == CanvasPsLayerKind::Adjustment {
        draw_adjustment(out, layer, bitmaps, scale_x, scale_y);
        return;
    }
    let layer_w = px_size(layer.width * scale_x);
    let layer_h = px_size(layer.height * scale_y);
    let mut content = layer_content(layer, layer_w, layer_h, bitmaps, paths, text, scale_y);
    if let Some(mask) = layer_mask(layer, layer_w, layer_h, scale_x, scale_y, bitmaps) {
        multiply_alpha(&mut content, &mask);
    }
    if let Some(mesh) = transform_mesh(layer.transform.as_ref()) {
        content = warp_mesh(&content, &mesh, content.width(), content.height());
    }
    let x = layer.x * scale_x;
    let y = layer.y * scale_y;
    composite_content(
        out,
        &content,
        x,
        y,
        layer.rotation,
        clamp_layer_opacity(Some(layer.opacity)),
        resolve_blend_mode(&layer.blend_mode),
    );
}

/// `drawPsAdjustment` (smart-canvas.ts:422): adjust everything below, draw on top.
fn draw_adjustment(
    out: &mut Surface,
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    scale_x: f64,
    scale_y: f64,
) {
    let Some(kind) = layer.adjustment else {
        return;
    };
    let params = layer.adjustment_params.clone().unwrap_or_default();
    let mut adjusted = out.clone();
    apply_ps_adjustment(&mut adjusted, kind, &params);
    if let Some(mask) = layer_mask(layer, out.width(), out.height(), scale_x, scale_y, bitmaps) {
        multiply_alpha(&mut adjusted, &mask);
    }
    blend_full(
        out,
        &adjusted,
        resolve_blend_mode(&layer.blend_mode),
        clamp_layer_opacity(Some(layer.opacity)),
    );
}

fn layer_content(
    layer: &CanvasPsLayer,
    width: usize,
    height: usize,
    bitmaps: &HashMap<String, Surface>,
    paths: &[CanvasPsPath],
    text: Option<&dyn TextRasterizer>,
    scale: f64,
) -> Surface {
    if let Some(key) = layer.storage_key.as_deref() {
        if let Some(bitmap) = bitmaps.get(key) {
            if bitmap.width() == width && bitmap.height() == height {
                return bitmap.clone();
            }
            return bitmap.resample_bilinear(width, height);
        }
    }
    if layer.kind == CanvasPsLayerKind::Shape {
        return rasterize_shape(layer, width, height);
    }
    if layer.kind == CanvasPsLayerKind::Text {
        if let Some(rasterized) =
            text.and_then(|text| text.rasterize(layer, paths, width, height, scale))
        {
            return rasterized;
        }
    }
    Surface::new(width, height)
}

fn layer_mask(
    layer: &CanvasPsLayer,
    width: usize,
    height: usize,
    _scale_x: f64,
    _scale_y: f64,
    bitmaps: &HashMap<String, Surface>,
) -> Option<Surface> {
    let key = layer.mask_storage_key.as_deref()?;
    let mask = bitmaps.get(key)?;
    if mask.width() == width && mask.height() == height {
        Some(mask.clone())
    } else {
        Some(mask.resample_bilinear(width, height))
    }
}

pub(crate) fn multiply_alpha(surface: &mut Surface, mask: &Surface) {
    if surface.width() != mask.width() || surface.height() != mask.height() {
        return;
    }
    let mask_data = mask.data().to_vec();
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let factor = f64::from(mask_data.get(index + 3).copied().unwrap_or(0)) / 255.0;
        data[index + 3] = clamp_u8(f64::from(data[index + 3]) * factor);
    }
}

fn blend_full(dest: &mut Surface, source: &Surface, mode: BlendMode, opacity: f64) {
    if dest.width() != source.width() || dest.height() != source.height() {
        return;
    }
    let width = dest.width();
    for index in (0..dest.len()).step_by(4) {
        let pixel = index / 4;
        let x = pixel % width;
        let y = pixel / width;
        let out = blend_pixel(
            dest.get(x as i64, y as i64),
            source.get(x as i64, y as i64),
            opacity,
            mode,
        );
        dest.set(x as i64, y as i64, out);
    }
}

fn composite_content(
    out: &mut Surface,
    content: &Surface,
    x: f64,
    y: f64,
    rotation: f64,
    opacity: f64,
    mode: BlendMode,
) {
    let (cw, ch) = (content.width() as f64, content.height() as f64);
    let centre = Point::new(x + cw / 2.0, y + ch / 2.0);
    let corners: Vec<Point> = [(0.0, 0.0), (cw, 0.0), (cw, ch), (0.0, ch)]
        .iter()
        .map(|(cx, cy)| rotate_point(x + cx, y + cy, centre, rotation))
        .collect();
    let min_x = corners
        .iter()
        .fold(f64::INFINITY, |a, p| a.min(p.x))
        .floor()
        .max(0.0) as i64;
    let max_x = corners
        .iter()
        .fold(f64::NEG_INFINITY, |a, p| a.max(p.x))
        .ceil()
        .min(out.width() as f64) as i64;
    let min_y = corners
        .iter()
        .fold(f64::INFINITY, |a, p| a.min(p.y))
        .floor()
        .max(0.0) as i64;
    let max_y = corners
        .iter()
        .fold(f64::NEG_INFINITY, |a, p| a.max(p.y))
        .ceil()
        .min(out.height() as f64) as i64;
    let angle = -rotation.to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    for py in min_y..max_y {
        for px in min_x..max_x {
            let dx = px as f64 + 0.5 - centre.x;
            let dy = py as f64 + 0.5 - centre.y;
            let lx = dx * cos - dy * sin;
            let ly = dx * sin + dy * cos;
            let sx = lx + cw / 2.0 - 0.5;
            let sy = ly + ch / 2.0 - 0.5;
            if sx < 0.0 || sy < 0.0 || sx >= cw || sy >= ch {
                continue;
            }
            let sample = content.sample_bilinear(sx, sy);
            let source = [
                clamp_u8(sample[0]),
                clamp_u8(sample[1]),
                clamp_u8(sample[2]),
                clamp_u8(sample[3]),
            ];
            if source[3] == 0 {
                continue;
            }
            let backdrop = out.get(px, py);
            out.set(px, py, blend_pixel(backdrop, source, opacity, mode));
        }
    }
}

/// `drawPsShapeLayer` (smart-canvas.ts:465): a minimal pure rasteriser for the
/// five shape kinds (fill + optional stroke).
fn rasterize_shape(layer: &CanvasPsLayer, width: usize, height: usize) -> Surface {
    let mut surface = Surface::new(width, height);
    let (w, h) = (width as f64, height as f64);
    let fill = layer.shape_fill.clone().filter(|value| !value.is_empty());
    let stroke = layer.shape_stroke.clone().filter(|value| !value.is_empty());
    let stroke_width = layer.shape_stroke_width.unwrap_or(0.0).max(0.0);
    let stroke_color = stroke.clone().or_else(|| fill.clone());
    match layer.shape {
        Some(CanvasPsShapeKind::Line) => {
            if let Some(color) = stroke_color {
                draw_line(
                    &mut surface,
                    Point::new(0.0, 0.0),
                    Point::new(w, h),
                    stroke_width.max(1.0),
                    &color,
                );
            }
        }
        Some(CanvasPsShapeKind::Ellipse) => {
            fill_region(
                &mut surface,
                |px, py| {
                    let (rx, ry) = (w / 2.0, h / 2.0);
                    if rx <= 0.0 || ry <= 0.0 {
                        return false;
                    }
                    ((px - rx) / rx).powi(2) + ((py - ry) / ry).powi(2) <= 1.0
                },
                fill.as_deref(),
            );
            if let Some(color) = stroke {
                if stroke_width > 0.0 {
                    outline_ellipse(&mut surface, w, h, stroke_width, &color);
                }
            }
        }
        Some(CanvasPsShapeKind::Polygon) => {
            let sides = layer.shape_sides.unwrap_or(6.0).round().clamp(3.0, 24.0) as usize;
            let points: Vec<Point> = (0..sides)
                .map(|index| {
                    let angle = (index as f64 / sides as f64) * std::f64::consts::TAU
                        - std::f64::consts::FRAC_PI_2;
                    Point::new(
                        w / 2.0 + (w / 2.0) * angle.cos(),
                        h / 2.0 + (h / 2.0) * angle.sin(),
                    )
                })
                .collect();
            if let Some(color) = fill {
                let rgba = hex_to_rgba(&color);
                let mask = fill_polygon_mask(width, height, &points);
                for y in 0..height {
                    for x in 0..width {
                        if mask.get(x as i64, y as i64)[3] > 0 {
                            surface.set(x as i64, y as i64, rgba);
                        }
                    }
                }
            }
            if let (Some(color), true) = (stroke_color, stroke_width > 0.0) {
                for window in points
                    .iter()
                    .copied()
                    .zip(points.iter().copied().cycle().skip(1))
                    .take(sides)
                {
                    draw_line(&mut surface, window.0, window.1, stroke_width, &color);
                }
            }
        }
        Some(CanvasPsShapeKind::RoundedRectangle) => {
            let radius = layer
                .shape_radius
                .unwrap_or(24.0)
                .max(0.0)
                .min(w / 2.0)
                .min(h / 2.0);
            fill_region(
                &mut surface,
                |px, py| in_rounded_rect(px, py, w, h, radius),
                fill.as_deref(),
            );
            if let Some(color) = stroke {
                if stroke_width > 0.0 {
                    outline_rect(&mut surface, w, h, stroke_width, &color);
                }
            }
        }
        _ => {
            fill_region(
                &mut surface,
                |px, py| px >= 0.0 && px < w && py >= 0.0 && py < h,
                fill.as_deref(),
            );
            if let Some(color) = stroke {
                if stroke_width > 0.0 {
                    outline_rect(&mut surface, w, h, stroke_width, &color);
                }
            }
        }
    }
    surface
}

fn fill_region(surface: &mut Surface, inside: impl Fn(f64, f64) -> bool, color: Option<&str>) {
    let Some(color) = color else {
        return;
    };
    let rgba = hex_to_rgba(color);
    for y in 0..surface.height() {
        for x in 0..surface.width() {
            if inside(x as f64 + 0.5, y as f64 + 0.5) {
                surface.set(x as i64, y as i64, rgba);
            }
        }
    }
}

fn in_rounded_rect(px: f64, py: f64, w: f64, h: f64, radius: f64) -> bool {
    if px < 0.0 || py < 0.0 || px > w || py > h {
        return false;
    }
    let r = radius.max(0.0);
    let corner_x = px < r || px > w - r;
    let corner_y = py < r || py > h - r;
    if !(corner_x && corner_y) {
        return true;
    }
    let cx = if px < r { r } else { w - r };
    let cy = if py < r { r } else { h - r };
    (px - cx).hypot(py - cy) <= r
}

fn outline_rect(surface: &mut Surface, w: f64, h: f64, stroke_width: f64, color: &str) {
    let rgba = hex_to_rgba(color);
    let half = stroke_width / 2.0;
    for y in 0..surface.height() {
        for x in 0..surface.width() {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let edge = (px - 0.0).abs() <= half
                || (px - w).abs() <= half
                || (py - 0.0).abs() <= half
                || (py - h).abs() <= half;
            if edge && px >= -half && px <= w + half && py >= -half && py <= h + half {
                surface.set(x as i64, y as i64, rgba);
            }
        }
    }
}

fn outline_ellipse(surface: &mut Surface, w: f64, h: f64, stroke_width: f64, color: &str) {
    let rgba = hex_to_rgba(color);
    let (rx, ry) = (w / 2.0, h / 2.0);
    if rx <= 0.0 || ry <= 0.0 {
        return;
    }
    let half = stroke_width / 2.0;
    for y in 0..surface.height() {
        for x in 0..surface.width() {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let d = (px - rx).powi(2) / (rx * rx) + (py - ry).powi(2) / (ry * ry);
            let inner = ((rx - half) / rx).powi(2) + ((ry - half) / ry).powi(2);
            if d <= 1.0 && d >= inner.min(1.0) {
                surface.set(x as i64, y as i64, rgba);
            }
        }
    }
}

fn draw_line(surface: &mut Surface, a: Point, b: Point, width: f64, color: &str) {
    let rgba = hex_to_rgba(color);
    let half = (width / 2.0).max(0.5);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = (dx * dx + dy * dy).max(1e-9);
    let min_x = (a.x.min(b.x) - half).floor().max(0.0) as i64;
    let max_x = (a.x.max(b.x) + half).ceil().min(surface.width() as f64) as i64;
    let min_y = (a.y.min(b.y) - half).floor().max(0.0) as i64;
    let max_y = (a.y.max(b.y) + half).ceil().min(surface.height() as f64) as i64;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let t = (((px - a.x) * dx + (py - a.y) * dy) / length_sq).clamp(0.0, 1.0);
            let proj = Point::new(a.x + t * dx, a.y + t * dy);
            if (px - proj.x).hypot(py - proj.y) <= half {
                surface.set(x, y, rgba);
            }
        }
    }
}

/// A blank layer helper re-exported for tests and callers without a `board`.
pub fn blank_layer(
    id: &str,
    name: &str,
    kind: CanvasPsLayerKind,
    box_: crate::geom::Bounds,
) -> CanvasPsLayer {
    base_layer(id.to_string(), name, kind, box_)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{CanvasPsAdjustmentType, PsParams};

    #[test]
    fn empty_board_renders_background_at_export_size() {
        let board = SmartCanvasBoard {
            id: "b".into(),
            title: "B".into(),
            width: 16.0,
            height: 9.0,
            board_ratio: "16:9".into(),
            board_resolution: crate::doc::BoardResolution::K1,
            board_background: "#ffffff".into(),
            board_background_opacity: 1.0,
            board_layers: vec![],
            board_paths: vec![],
            board_alpha_channels: vec![],
            board_channel_visibility: None,
        };
        let out = render_document(&board, &HashMap::new());
        assert_eq!(out.width(), 1536);
        assert_eq!(out.height(), 864);
        assert_eq!(out.get(0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn layers_composite_with_blend_and_opacity() {
        let mut board = base_board(1.0, 1.0);
        let mut bottom = blank_layer(
            "bottom",
            "bottom",
            CanvasPsLayerKind::Pixel,
            crate::geom::Bounds::new(0.0, 0.0, 1.0, 1.0),
        );
        bottom.blend_mode = "normal".into();
        bottom.storage_key = Some("image:bottom".into());
        let mut top = bottom.clone();
        top.id = "top".into();
        top.opacity = 0.5;
        top.storage_key = Some("image:top".into());
        board.board_layers = vec![bottom, top];

        let mut bitmaps = HashMap::new();
        let mut red = Surface::new(1, 1);
        red.fill([255, 0, 0, 255]);
        let mut white = Surface::new(1, 1);
        white.fill([255, 255, 255, 255]);
        bitmaps.insert("image:bottom".into(), red);
        bitmaps.insert("image:top".into(), white);

        let out = render_document(&board, &bitmaps);
        // 50% white over red -> (255, 128, 128).
        let pixel = out.get(0, 0);
        assert_eq!(pixel[0], 255);
        assert!((i32::from(pixel[1]) - 128).abs() <= 1);
        assert!((i32::from(pixel[2]) - 128).abs() <= 1);
    }

    #[test]
    fn adjustment_layer_modifies_lower_layers() {
        let mut board = base_board(1.0, 1.0);
        let mut bottom = blank_layer(
            "b",
            "b",
            CanvasPsLayerKind::Pixel,
            crate::geom::Bounds::new(0.0, 0.0, 1.0, 1.0),
        );
        bottom.storage_key = Some("image:b".into());
        let mut adjust = blank_layer(
            "a",
            "a",
            CanvasPsLayerKind::Adjustment,
            crate::geom::Bounds::new(0.0, 0.0, 1.0, 1.0),
        );
        adjust.adjustment = Some(CanvasPsAdjustmentType::Invert);
        adjust.adjustment_params = Some(PsParams::new());
        board.board_layers = vec![bottom, adjust];

        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(1, 1);
        surface.fill([10, 20, 30, 255]);
        bitmaps.insert("image:b".into(), surface);

        let out = render_document(&board, &bitmaps);
        assert_eq!(out.get(0, 0), [245, 235, 225, 255]);
    }

    #[test]
    fn hidden_layers_are_skipped() {
        let mut board = base_board(1.0, 1.0);
        let mut layer = blank_layer(
            "b",
            "b",
            CanvasPsLayerKind::Pixel,
            crate::geom::Bounds::new(0.0, 0.0, 1.0, 1.0),
        );
        layer.hidden = true;
        layer.storage_key = Some("image:b".into());
        board.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(1, 1);
        surface.fill([255, 0, 0, 255]);
        bitmaps.insert("image:b".into(), surface);
        let out = render_document(&board, &bitmaps);
        assert_eq!(out.get(0, 0), [0, 0, 0, 0]);
    }

    struct SolidText;

    impl TextRasterizer for SolidText {
        fn rasterize(
            &self,
            layer: &CanvasPsLayer,
            _paths: &[CanvasPsPath],
            width: usize,
            height: usize,
            scale: f64,
        ) -> Option<Surface> {
            let mut surface = Surface::new(width, height);
            let hex = layer.color.clone().unwrap_or_else(|| "#000000".to_string());
            surface.fill(hex_to_rgba(&hex));
            (scale > 0.0).then_some(surface)
        }
    }

    #[test]
    fn text_layers_rasterize_through_the_host_hook() {
        let mut board = base_board(1.0, 1.0);
        let mut layer = blank_layer(
            "t",
            "t",
            CanvasPsLayerKind::Text,
            crate::geom::Bounds::new(0.0, 0.0, 1.0, 1.0),
        );
        layer.text = Some("hi".into());
        layer.color = Some("#ff0000".into());
        board.board_layers = vec![layer];
        let bitmaps = HashMap::new();
        assert_eq!(render_document(&board, &bitmaps).get(0, 0), [0, 0, 0, 0]);
        let out = render_document_with_text(&board, &bitmaps, Some(&SolidText));
        assert_eq!(out.get(0, 0), [255, 0, 0, 255]);
    }

    fn base_board(width: f64, height: f64) -> SmartCanvasBoard {
        SmartCanvasBoard {
            id: "b".into(),
            title: "B".into(),
            width,
            height,
            board_ratio: "1:1".into(),
            board_resolution: crate::doc::BoardResolution::K1,
            board_background: "transparent".into(),
            board_background_opacity: 1.0,
            board_layers: vec![],
            board_paths: vec![],
            board_alpha_channels: vec![],
            board_channel_visibility: None,
        }
    }
}
