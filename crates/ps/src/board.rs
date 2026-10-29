//! Board geometry, layer factories and document predicates - port of the
//! model half of `web/src/lib/canvas/smart-canvas.ts` (size/ratio helpers,
//! `psTopLayers`/`psLayerBox`/`psShapePathData`, `createPs*Layer`,
//! `arrangePsLayers`, `movePsLayer`, `psDocumentNeedsRaster`).

use crate::adjustments::ps_adjustment_defaults;
use crate::doc::{
    BoardResolution, CanvasPsAdjustmentType, CanvasPsLayer, CanvasPsLayerKind, CanvasPsShapeKind,
    IdGen, SmartCanvasBoard,
};
use crate::geom::{rotate_point, Bounds, Point};
use crate::text::{text_font_size, text_line_height, text_needs_raster};
use crate::transform::has_transform;

/// `PS_SHAPE_KINDS` (smart-canvas.ts:38).
pub const PS_SHAPE_KINDS: [CanvasPsShapeKind; 5] = [
    CanvasPsShapeKind::Rectangle,
    CanvasPsShapeKind::RoundedRectangle,
    CanvasPsShapeKind::Ellipse,
    CanvasPsShapeKind::Polygon,
    CanvasPsShapeKind::Line,
];

pub const DEFAULT_RATIO: &str = "16:9";
pub const DEFAULT_BACKGROUND: &str = "transparent";
pub const BASE_WIDTH: f64 = 640.0;
pub const BASE_HEIGHT: f64 = 360.0;
const BOARD_LAYOUT_GAP: f64 = 16.0;
const BOARD_TEXT_FONT_SIZE: f64 = 32.0;
const BOARD_TEXT_COLOR: &str = "#000000";

/// `BoardLayoutTemplate` (smart-canvas.ts:51).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardLayoutTemplate {
    Grid,
    Row,
    Column,
    Feature,
}

/// `smartCanvasRatio` (smart-canvas.ts:63).
pub fn smart_canvas_ratio(board: &SmartCanvasBoard) -> &str {
    if board.board_ratio.is_empty() {
        DEFAULT_RATIO
    } else {
        &board.board_ratio
    }
}

/// `smartCanvasBackground` (smart-canvas.ts:71).
pub fn smart_canvas_background(board: &SmartCanvasBoard) -> &str {
    if board.board_background.is_empty() {
        DEFAULT_BACKGROUND
    } else {
        &board.board_background
    }
}

const RATIOS: [&str; 9] = [
    "1:1", "2:3", "3:2", "4:3", "3:4", "16:9", "9:16", "21:9", "9:21",
];
const SIZES_1K: [(f64, f64); 9] = [
    (1024.0, 1024.0),
    (1024.0, 1536.0),
    (1536.0, 1024.0),
    (1024.0, 768.0),
    (768.0, 1024.0),
    (1536.0, 864.0),
    (864.0, 1536.0),
    (2016.0, 864.0),
    (864.0, 2016.0),
];
const SIZES_2K: [(f64, f64); 9] = [
    (2048.0, 2048.0),
    (1360.0, 2048.0),
    (2048.0, 1360.0),
    (2048.0, 1536.0),
    (1536.0, 2048.0),
    (2048.0, 1152.0),
    (1152.0, 2048.0),
    (2688.0, 1152.0),
    (1152.0, 2688.0),
];
const SIZES_4K: [(f64, f64); 9] = [
    (2880.0, 2880.0),
    (2336.0, 3520.0),
    (3520.0, 2336.0),
    (3312.0, 2480.0),
    (2480.0, 3312.0),
    (3840.0, 2160.0),
    (2160.0, 3840.0),
    (3840.0, 1648.0),
    (1648.0, 3840.0),
];

/// `readMediaDimensions` (media-size.ts:95) for the Image Studio board: the
/// export target size from a resolution tier and a ratio.
pub fn read_media_dimensions(resolution: BoardResolution, ratio: &str) -> (f64, f64) {
    let table = match resolution {
        BoardResolution::K1 => &SIZES_1K,
        BoardResolution::K2 => &SIZES_2K,
        BoardResolution::K4 => &SIZES_4K,
    };
    RATIOS
        .iter()
        .position(|item| *item == ratio)
        .map_or((0.0, 0.0), |index| table[index])
}

/// `nodeSizeFromRatio` (canvas-node-size.ts:8).
pub fn node_size_from_ratio(size: &str, base_width: f64, base_height: f64) -> Option<(f64, f64)> {
    let (head, tail) = size.split_once(['x', ':'])?;
    let width: f64 = head.parse().ok()?;
    let height: f64 = tail.parse().ok()?;
    let ratio = width / height.max(1.0);
    if !(0.25..=4.0).contains(&ratio) {
        return Some((base_width, base_height));
    }
    if ratio >= base_width / base_height {
        Some((base_width, base_width / ratio))
    } else {
        Some((base_height * ratio, base_height))
    }
}

/// `smartCanvasSizeForRatio` (smart-canvas.ts:83).
pub fn smart_canvas_size_for_ratio(ratio: &str) -> (f64, f64) {
    node_size_from_ratio(ratio, BASE_WIDTH, BASE_HEIGHT).unwrap_or((BASE_WIDTH, BASE_HEIGHT))
}

/// `psLayerChildIds` (smart-canvas.ts:96).
pub fn layer_child_ids(layers: &[CanvasPsLayer]) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    for layer in layers {
        if let Some(children) = &layer.children {
            ids.extend(children.iter().cloned());
        }
    }
    ids
}

/// `psTopLayers` (smart-canvas.ts:102).
pub fn top_layers(layers: &[CanvasPsLayer]) -> Vec<CanvasPsLayer> {
    let child_ids = layer_child_ids(layers);
    layers
        .iter()
        .filter(|layer| !child_ids.contains(&layer.id))
        .cloned()
        .collect()
}

/// `psGroupChildren` (smart-canvas.ts:107).
pub fn group_children(layers: &[CanvasPsLayer], group: &CanvasPsLayer) -> Vec<CanvasPsLayer> {
    let mut out = Vec::new();
    if let Some(children) = &group.children {
        for id in children {
            if let Some(child) = layers.iter().find(|layer| layer.id == *id) {
                out.push(child.clone());
            }
        }
    }
    out
}

fn layer_corners(layer: &CanvasPsLayer) -> Vec<Point> {
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(sx, sy)| {
            rotate_point(
                layer.x + if *sx < 0.0 { 0.0 } else { layer.width },
                layer.y + if *sy < 0.0 { 0.0 } else { layer.height },
                centre,
                layer.rotation,
            )
        })
        .collect()
}

/// `psLayerBox` (smart-canvas.ts:139): a group's box_ is derived from children.
pub fn layer_box(layers: &[CanvasPsLayer], layer: &CanvasPsLayer) -> Bounds {
    if layer.kind != CanvasPsLayerKind::Group {
        return Bounds::new(layer.x, layer.y, layer.width, layer.height);
    }
    let corners: Vec<Point> = group_children(layers, layer)
        .iter()
        .flat_map(layer_corners)
        .collect();
    match Bounds::union(
        &corners
            .iter()
            .map(|point| Bounds::new(point.x, point.y, 0.0, 0.0))
            .collect::<Vec<_>>(),
    ) {
        Some(bounds) if !corners.is_empty() => bounds,
        _ => Bounds::new(layer.x, layer.y, layer.width, layer.height),
    }
}

/// `psDocumentNeedsRaster` (smart-canvas.ts:113).
pub fn document_needs_raster(board: &SmartCanvasBoard) -> bool {
    if let Some(channels) = board.board_channel_visibility {
        if !channels.r || !channels.g || !channels.b {
            return true;
        }
    }
    board.board_layers.iter().any(|layer| {
        !layer.hidden
            && (layer.kind == CanvasPsLayerKind::Adjustment
                || has_transform(layer)
                || (layer.kind == CanvasPsLayerKind::Text && text_needs_raster(layer))
                || layer
                    .styles
                    .as_ref()
                    .is_some_and(|styles| styles.iter().any(|style| style.enabled)))
    })
}

/// `psShapePathData` (smart-canvas.ts:223).
pub fn shape_path_data(layer: &CanvasPsLayer, width: f64, height: f64) -> String {
    let w = width.max(1.0);
    let h = height.max(1.0);
    match layer.shape {
        Some(CanvasPsShapeKind::Ellipse) => format!(
            "M 0 {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 0 {} Z",
            h / 2.0,
            w / 2.0,
            h / 2.0,
            w,
            h / 2.0,
            w / 2.0,
            h / 2.0,
            h / 2.0
        ),
        Some(CanvasPsShapeKind::Line) => format!("M 0 0 L {w} {h}"),
        Some(CanvasPsShapeKind::Polygon) => {
            let sides = (layer.shape_sides.unwrap_or(6.0).round() as i64).clamp(3, 24);
            let points: Vec<String> = (0..sides)
                .map(|index| {
                    let angle = (index as f64 / sides as f64) * std::f64::consts::TAU
                        - std::f64::consts::FRAC_PI_2;
                    format!(
                        "{} {}",
                        w / 2.0 + (w / 2.0) * angle.cos(),
                        h / 2.0 + (h / 2.0) * angle.sin()
                    )
                })
                .collect();
            format!("M {} Z", points.join(" L "))
        }
        Some(CanvasPsShapeKind::RoundedRectangle) => {
            let radius = layer
                .shape_radius
                .unwrap_or(24.0)
                .max(0.0)
                .min(w / 2.0)
                .min(h / 2.0);
            if radius <= 0.0 {
                format!("M 0 0 H {w} V {h} H 0 Z")
            } else {
                format!(
                    "M {r} 0 H {a} A {r} {r} 0 0 1 {w} {r} V {b} A {r} {r} 0 0 1 {a} {h} H {r} A {r} {r} 0 0 1 0 {b} V {r} A {r} {r} 0 0 1 {r} 0 Z",
                    r = radius,
                    a = w - radius,
                    b = h - radius,
                )
            }
        }
        _ => format!("M 0 0 H {w} V {h} H 0 Z"),
    }
}

/// `psTextRenderStyle` (smart-canvas.ts:217).
#[derive(Debug, Clone, PartialEq)]
pub struct TextRenderStyle {
    pub font_size: f64,
    pub color: String,
    pub font_family: String,
    pub line_height: f64,
}

/// `psTextRenderStyle` (smart-canvas.ts:217): the line-height is a ratio.
pub fn text_render_style(layer: &CanvasPsLayer) -> TextRenderStyle {
    let font_size = text_font_size(layer);
    TextRenderStyle {
        font_size,
        color: layer
            .color
            .clone()
            .unwrap_or_else(|| BOARD_TEXT_COLOR.to_string()),
        font_family: layer
            .font_family
            .clone()
            .unwrap_or_else(|| "sans-serif".to_string()),
        line_height: text_line_height(layer) / font_size,
    }
}

/// `createPsPixelLayer` (smart-canvas.ts:148).
pub fn create_ps_pixel_layer(gen: &mut IdGen, board: (f64, f64), name: &str) -> CanvasPsLayer {
    layer_base(
        gen,
        board,
        name,
        CanvasPsLayerKind::Pixel,
        Bounds::new(0.0, 0.0, board.0.max(1.0).round(), board.1.max(1.0).round()),
    )
}

/// `createPsTextLayer` (smart-canvas.ts:152).
pub fn create_ps_text_layer(
    gen: &mut IdGen,
    board: (f64, f64),
    text: &str,
    name: &str,
) -> CanvasPsLayer {
    let width = (board.0 * 0.6).round().max(1.0);
    let height = (BOARD_TEXT_FONT_SIZE * 1.2).round().max(1.0);
    let mut layer = layer_base(
        gen,
        board,
        name,
        CanvasPsLayerKind::Text,
        Bounds::new(
            ((board.0 - width) / 2.0).round(),
            ((board.1 - height) / 2.0).round(),
            width,
            height,
        ),
    );
    layer.text = Some(text.to_string());
    layer.font_size = Some(BOARD_TEXT_FONT_SIZE);
    layer.color = Some(BOARD_TEXT_COLOR.to_string());
    layer
}

/// `createPsShapeLayer` (smart-canvas.ts:175).
pub fn create_ps_shape_layer(
    gen: &mut IdGen,
    kind: CanvasPsShapeKind,
    box_: Bounds,
    name: &str,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
) -> CanvasPsLayer {
    let board = (box_.x + box_.width, box_.y + box_.height);
    let mut layer = layer_base(gen, board, name, CanvasPsLayerKind::Shape, box_);
    layer.shape = Some(kind);
    layer.shape_radius = Some(24.0);
    layer.shape_sides = Some(6.0);
    layer.shape_fill = Some(fill.to_string());
    layer.shape_stroke = Some(stroke.to_string());
    layer.shape_stroke_width = Some(stroke_width);
    layer
}

/// `createPsAdjustmentLayer` (smart-canvas.ts:179).
pub fn create_ps_adjustment_layer(
    gen: &mut IdGen,
    board: (f64, f64),
    adjustment: CanvasPsAdjustmentType,
    name: &str,
) -> CanvasPsLayer {
    let mut layer = layer_base(
        gen,
        board,
        name,
        CanvasPsLayerKind::Adjustment,
        Bounds::new(0.0, 0.0, board.0.max(1.0).round(), board.1.max(1.0).round()),
    );
    layer.adjustment = Some(adjustment);
    layer.adjustment_params = Some(ps_adjustment_defaults(adjustment));
    layer
}

/// `createPsImageLayer` (smart-canvas.ts:183): fits the source into the board.
pub fn create_ps_image_layer(
    gen: &mut IdGen,
    board: (f64, f64),
    title: &str,
    source: (f64, f64),
) -> CanvasPsLayer {
    let (source_width, source_height) = source;
    let scale = if source_width > 0.0 && source_height > 0.0 {
        (board.0 / source_width).min(board.1 / source_height)
    } else {
        0.0
    };
    let width = if scale > 0.0 {
        (source_width * scale).round().max(1.0)
    } else {
        board.0.max(1.0)
    };
    let height = if scale > 0.0 {
        (source_height * scale).round().max(1.0)
    } else {
        board.1.max(1.0)
    };
    layer_base(
        gen,
        board,
        title,
        CanvasPsLayerKind::Image,
        Bounds::new(
            ((board.0 - width) / 2.0).round(),
            ((board.1 - height) / 2.0).round(),
            width,
            height,
        ),
    )
}

fn layer_base(
    gen: &mut IdGen,
    board: (f64, f64),
    name: &str,
    kind: CanvasPsLayerKind,
    box_: Bounds,
) -> CanvasPsLayer {
    let _ = board;
    base_layer(gen.next_id(), name, kind, box_)
}

/// A blank layer with the shared defaults (`normal`/opaque/unlocked).
pub fn base_layer(id: String, name: &str, kind: CanvasPsLayerKind, box_: Bounds) -> CanvasPsLayer {
    CanvasPsLayer {
        id,
        name: name.to_string(),
        kind,
        content: None,
        storage_key: None,
        mask_storage_key: None,
        adjustment: None,
        adjustment_params: None,
        styles: None,
        text: None,
        shape: None,
        shape_radius: None,
        shape_sides: None,
        shape_fill: None,
        shape_stroke: None,
        shape_stroke_width: None,
        x: box_.x,
        y: box_.y,
        width: box_.width,
        height: box_.height,
        rotation: 0.0,
        opacity: 1.0,
        blend_mode: "normal".to_string(),
        hidden: false,
        locked: false,
        font_size: None,
        color: None,
        font_family: None,
        tracking: None,
        leading: None,
        kerning: None,
        baseline_shift: None,
        text_scale_x: None,
        text_scale_y: None,
        faux_bold: None,
        faux_italic: None,
        underline: None,
        strikethrough: None,
        text_case: None,
        paragraph: None,
        text_path_id: None,
        text_warp: None,
        transform: None,
        children: None,
    }
}

fn board_grid_cells(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    count: usize,
    cols: usize,
) -> Vec<Bounds> {
    let cols = cols.max(1);
    let rows = count.div_ceil(cols);
    let cell_width = (width - BOARD_LAYOUT_GAP * (cols as f64 + 1.0)) / cols as f64;
    let cell_height = (height - BOARD_LAYOUT_GAP * (rows as f64 + 1.0)) / rows as f64;
    (0..count)
        .map(|index| {
            Bounds::new(
                x + BOARD_LAYOUT_GAP + (index % cols) as f64 * (cell_width + BOARD_LAYOUT_GAP),
                y + BOARD_LAYOUT_GAP + (index / cols) as f64 * (cell_height + BOARD_LAYOUT_GAP),
                cell_width,
                cell_height,
            )
        })
        .collect()
}

fn feature_board_cells(board: (f64, f64), count: usize) -> Vec<Bounds> {
    let half = (board.0 - BOARD_LAYOUT_GAP * 3.0) / 2.0;
    let first = Bounds::new(
        BOARD_LAYOUT_GAP,
        BOARD_LAYOUT_GAP,
        half,
        board.1 - BOARD_LAYOUT_GAP * 2.0,
    );
    let rest = count.saturating_sub(1);
    if rest == 0 {
        return vec![first];
    }
    let cols = (rest as f64).sqrt().ceil() as usize;
    let mut cells = vec![first];
    cells.extend(board_grid_cells(
        BOARD_LAYOUT_GAP * 2.0 + half,
        0.0,
        half,
        board.1,
        rest,
        cols,
    ));
    cells
}

fn fit_layer_cell(layer: &CanvasPsLayer, cell: Bounds) -> Bounds {
    let scale = if layer.width > 0.0 && layer.height > 0.0 {
        (cell.width / layer.width).min(cell.height / layer.height)
    } else {
        0.0
    };
    let width = if scale > 0.0 {
        (layer.width * scale).max(1.0).round()
    } else {
        cell.width.max(1.0)
    };
    let height = if scale > 0.0 {
        (layer.height * scale).max(1.0).round()
    } else {
        cell.height.max(1.0)
    };
    Bounds::new(
        (cell.x + (cell.width - width) / 2.0).round(),
        (cell.y + (cell.height - height) / 2.0).round(),
        width,
        height,
    )
}

/// `arrangePsLayers` (smart-canvas.ts:265): lays image layers out on a template.
pub fn arrange_ps_layers(
    board: (f64, f64),
    layers: &[CanvasPsLayer],
    template: BoardLayoutTemplate,
) -> Vec<CanvasPsLayer> {
    let images: Vec<CanvasPsLayer> = top_layers(layers)
        .into_iter()
        .filter(|layer| layer.kind == CanvasPsLayerKind::Image)
        .collect();
    let count = images.len();
    if count == 0 {
        return layers.to_vec();
    }
    let cols = match template {
        BoardLayoutTemplate::Row => count,
        BoardLayoutTemplate::Column => 1,
        BoardLayoutTemplate::Grid | BoardLayoutTemplate::Feature => {
            (count as f64).sqrt().ceil() as usize
        }
    };
    let cells = if template == BoardLayoutTemplate::Feature {
        feature_board_cells(board, count)
    } else {
        board_grid_cells(0.0, 0.0, board.0, board.1, count, cols)
    };
    let boxes: std::collections::HashMap<String, Bounds> = images
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| {
            cells
                .get(index)
                .map(|cell| (layer.id.clone(), fit_layer_cell(layer, *cell)))
        })
        .collect();
    layers
        .iter()
        .map(|layer| match boxes.get(&layer.id) {
            Some(box_) => {
                let mut next = layer.clone();
                next.x = box_.x;
                next.y = box_.y;
                next.width = box_.width;
                next.height = box_.height;
                next
            }
            None => layer.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::CanvasPsLayerKind;

    #[test]
    fn media_dimensions_match_the_preset_table() {
        assert_eq!(
            read_media_dimensions(BoardResolution::K2, "16:9"),
            (2048.0, 1152.0)
        );
        assert_eq!(
            read_media_dimensions(BoardResolution::K4, "1:1"),
            (2880.0, 2880.0)
        );
        assert_eq!(
            read_media_dimensions(BoardResolution::K1, "nope"),
            (0.0, 0.0)
        );
    }

    #[test]
    fn node_size_from_ratio_derives_the_base_side() {
        assert_eq!(
            node_size_from_ratio("16:9", 640.0, 360.0),
            Some((640.0, 360.0))
        );
        assert_eq!(
            node_size_from_ratio("1:1", 640.0, 360.0),
            Some((360.0, 360.0))
        );
        assert_eq!(node_size_from_ratio("bogus", 640.0, 360.0), None);
    }

    #[test]
    fn top_layers_excludes_group_children() {
        let mut gen = IdGen::new("l");
        let mut group = create_ps_pixel_layer(&mut gen, (10.0, 10.0), "g");
        let child = create_ps_pixel_layer(&mut gen, (10.0, 10.0), "c");
        group.kind = CanvasPsLayerKind::Group;
        group.children = Some(vec![child.id.clone()]);
        let layers = vec![group, child];
        let top = top_layers(&layers);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].kind, CanvasPsLayerKind::Group);
    }

    #[test]
    fn group_box_derives_from_children() {
        let mut gen = IdGen::new("l");
        let mut group = create_ps_pixel_layer(&mut gen, (10.0, 10.0), "g");
        let mut child = create_ps_pixel_layer(&mut gen, (10.0, 10.0), "c");
        child.x = 5.0;
        child.y = 6.0;
        child.width = 20.0;
        child.height = 30.0;
        group.kind = CanvasPsLayerKind::Group;
        group.x = 0.0;
        group.y = 0.0;
        group.width = 1.0;
        group.height = 1.0;
        group.children = Some(vec![child.id.clone()]);
        let layers = vec![group.clone(), child];
        let box_ = layer_box(&layers, &group);
        assert_eq!(box_, Bounds::new(5.0, 6.0, 20.0, 30.0));
    }

    #[test]
    fn shape_path_data_forms() {
        let mut gen = IdGen::new("l");
        let mut layer = create_ps_shape_layer(
            &mut gen,
            CanvasPsShapeKind::Rectangle,
            Bounds::new(0.0, 0.0, 10.0, 20.0),
            "r",
            "#fff",
            "#000",
            1.0,
        );
        assert_eq!(shape_path_data(&layer, 10.0, 20.0), "M 0 0 H 10 V 20 H 0 Z");
        layer.shape = Some(CanvasPsShapeKind::Polygon);
        let polygon = shape_path_data(&layer, 10.0, 10.0);
        assert!(polygon.starts_with("M "));
        assert!(polygon.ends_with(" Z"));
    }

    #[test]
    fn arrange_places_image_layers_on_a_grid() {
        let mut gen = IdGen::new("l");
        let mut a = create_ps_image_layer(&mut gen, (100.0, 100.0), "a", (50.0, 50.0));
        let mut b = create_ps_image_layer(&mut gen, (100.0, 100.0), "b", (50.0, 50.0));
        a.x = 0.0;
        b.x = 0.0;
        let arranged = arrange_ps_layers((100.0, 100.0), &[a, b], BoardLayoutTemplate::Row);
        assert!(arranged[0].x >= 0.0);
        assert!(arranged[1].x > arranged[0].x);
    }
}
