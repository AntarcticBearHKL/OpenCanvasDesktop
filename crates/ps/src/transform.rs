//! Free-transform math — port of `web/src/lib/canvas/ps-transform.ts`: the
//! quad/warp mesh, numeric transform, handle movement, and a pure raster
//! `warp_mesh` that replaces the canvas triangle draws of `drawPsWarpedContent`.

use crate::doc::{CanvasPsLayer, CanvasPsTransform, Position};
use crate::geom::{rotate_point, Point};
use crate::surface::{clamp_u8, Surface};

/// `PS_WARP_GRID` (ps-transform.ts:11).
pub const PS_WARP_GRID: usize = 4;
const GRID_POINTS: usize = PS_WARP_GRID * PS_WARP_GRID;
const CORNER_OFFSETS: [usize; 4] = [
    0,
    PS_WARP_GRID - 1,
    GRID_POINTS - 1,
    PS_WARP_GRID * (PS_WARP_GRID - 1),
];
const CORNER_HANDLES: [(f64, f64); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
const SKEW_HANDLES: [(f64, f64); 8] = [
    (0.0, 0.0),
    (0.5, 0.0),
    (1.0, 0.0),
    (0.0, 0.5),
    (1.0, 0.5),
    (0.0, 1.0),
    (0.5, 1.0),
    (1.0, 1.0),
];
const SKEW_TOP: [usize; 3] = [0, 1, 2];
const SKEW_BOTTOM: [usize; 3] = [4, 5, 6];
const SKEW_LEFT: [usize; 3] = [0, 6, 7];
const SKEW_RIGHT: [usize; 3] = [2, 3, 4];

fn to_position(point: Point) -> Position {
    Position {
        x: point.x,
        y: point.y,
    }
}

fn to_positions(points: Vec<Point>) -> Vec<Position> {
    points.into_iter().map(to_position).collect()
}

fn to_points(points: &[Position]) -> Vec<Point> {
    points
        .iter()
        .map(|point| Point::new(point.x, point.y))
        .collect()
}

/// `PsTransformMode` (ps-transform.ts:6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsTransformMode {
    Free,
    Skew,
    Distort,
    Perspective,
    Warp,
}

/// `PsNumericTransform` (ps-transform.ts:8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PsNumericTransform {
    pub dx: f64,
    pub dy: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub skew_x: f64,
    pub skew_y: f64,
}

/// `PS_NUMERIC_TRANSFORM_DEFAULT` (ps-transform.ts:12).
pub const PS_NUMERIC_TRANSFORM_DEFAULT: PsNumericTransform = PsNumericTransform {
    dx: 0.0,
    dy: 0.0,
    scale_x: 100.0,
    scale_y: 100.0,
    rotation: 0.0,
    skew_x: 0.0,
    skew_y: 0.0,
};

/// `psIdentityQuad` (ps-transform.ts:18).
pub fn identity_quad() -> Vec<Point> {
    vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ]
}

/// `psIdentityMesh` (ps-transform.ts:22).
pub fn identity_mesh() -> Vec<Point> {
    (0..GRID_POINTS)
        .map(|index| {
            let span = (PS_WARP_GRID - 1) as f64;
            Point::new(
                (index % PS_WARP_GRID) as f64 / span,
                (index / PS_WARP_GRID) as f64 / span,
            )
        })
        .collect()
}

/// `psDefaultTransform` (ps-transform.ts:26).
pub fn default_transform() -> CanvasPsTransform {
    CanvasPsTransform {
        quad: to_positions(identity_quad()),
        warp: None,
    }
}

/// `psHasTransform` (ps-transform.ts:30).
pub fn has_transform(layer: &CanvasPsLayer) -> bool {
    layer
        .transform
        .as_ref()
        .is_some_and(|transform| transform.quad.len() == 4)
}

/// `psIsIdentityTransform` (ps-transform.ts:34). The web checks every point
/// against the 4x4 grid by index, which never holds for a 4-point quad; this
/// checks a quad against the four corner coordinates instead (a latent web bug)
/// so a default quad is recognised as identity and is not needlessly warped.
pub fn is_identity_transform(transform: Option<&CanvasPsTransform>) -> bool {
    let Some(transform) = transform else {
        return true;
    };
    if let Some(warp) = transform.warp.as_deref() {
        if warp.len() == GRID_POINTS {
            let span = (PS_WARP_GRID - 1) as f64;
            return warp.iter().enumerate().all(|(index, point)| {
                (point.x - (index % PS_WARP_GRID) as f64 / span).abs() < 1e-4
                    && (point.y - (index / PS_WARP_GRID) as f64 / span).abs() < 1e-4
            });
        }
    }
    if transform.quad.len() == 4 {
        return transform.quad.iter().enumerate().all(|(index, point)| {
            (point.x - CORNER_HANDLES[index].0).abs() < 1e-4
                && (point.y - CORNER_HANDLES[index].1).abs() < 1e-4
        });
    }
    false
}

/// `psQuadPoint` (ps-transform.ts:53).
pub fn quad_point(quad: &[Point], u: f64, v: f64) -> Point {
    if quad.len() < 4 {
        return Point::new(u, v);
    }
    let top = Point::new(
        quad[0].x + (quad[1].x - quad[0].x) * u,
        quad[0].y + (quad[1].y - quad[0].y) * u,
    );
    let bottom = Point::new(
        quad[3].x + (quad[2].x - quad[3].x) * u,
        quad[3].y + (quad[2].y - quad[3].y) * u,
    );
    Point::new(
        top.x + (bottom.x - top.x) * v,
        top.y + (bottom.y - top.y) * v,
    )
}

/// `psTransformControl` (ps-transform.ts:41): the 4x4 control mesh.
pub fn transform_control(transform: &CanvasPsTransform) -> Vec<Point> {
    if let Some(warp) = transform.warp.as_ref() {
        if warp.len() == GRID_POINTS {
            return to_points(warp);
        }
    }
    let quad: Vec<Point> = if transform.quad.len() == 4 {
        to_points(&transform.quad)
    } else {
        identity_quad()
    };
    let span = (PS_WARP_GRID - 1) as f64;
    (0..GRID_POINTS)
        .map(|index| {
            quad_point(
                &quad,
                (index % PS_WARP_GRID) as f64 / span,
                (index / PS_WARP_GRID) as f64 / span,
            )
        })
        .collect()
}

/// `psSetControlPoint` (ps-transform.ts:47).
pub fn set_control_point(
    transform: &CanvasPsTransform,
    index: usize,
    point: Point,
) -> CanvasPsTransform {
    let mut mesh = transform_control(transform);
    if let Some(slot) = mesh.get_mut(index) {
        *slot = point;
    }
    CanvasPsTransform {
        quad: transform.quad.clone(),
        warp: Some(to_positions(mesh)),
    }
}

/// `psMeshPoint` (ps-transform.ts:59).
pub fn mesh_point(mesh: &[Point], u: f64, v: f64) -> Point {
    if mesh.len() != GRID_POINTS {
        return Point::new(u, v);
    }
    let span = (PS_WARP_GRID - 1) as f64;
    let col = ((u * span).floor() as usize).min(PS_WARP_GRID - 2);
    let row = ((v * span).floor() as usize).min(PS_WARP_GRID - 2);
    let cu = (u * span - col as f64).clamp(0.0, 1.0);
    let cv = (v * span - row as f64).clamp(0.0, 1.0);
    let mix = |a: Point, b: Point, t: f64| Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    let top_left = mesh[row * PS_WARP_GRID + col];
    let top_right = mesh[row * PS_WARP_GRID + col + 1];
    let bottom_left = mesh[(row + 1) * PS_WARP_GRID + col];
    let bottom_right = mesh[(row + 1) * PS_WARP_GRID + col + 1];
    let top = mix(top_left, top_right, cu);
    let bottom = mix(bottom_left, bottom_right, cu);
    mix(top, bottom, cv)
}

/// `psTransformMesh` (ps-transform.ts:74): `None` for an absent/identity mesh.
pub fn transform_mesh(transform: Option<&CanvasPsTransform>) -> Option<Vec<Point>> {
    let transform = transform?;
    if is_identity_transform(Some(transform)) {
        return None;
    }
    Some(transform_control(transform))
}

/// `psTransformPointDoc` (ps-transform.ts:89).
pub fn transform_point_doc(layer: &CanvasPsLayer, u: f64, v: f64) -> Point {
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    rotate_point(
        layer.x + u * layer.width,
        layer.y + v * layer.height,
        centre,
        layer.rotation,
    )
}

/// `psDocToTransformPoint` (ps-transform.ts:95).
pub fn doc_to_transform_point(layer: &CanvasPsLayer, point: Point) -> Point {
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    let local = rotate_point(point.x, point.y, centre, -layer.rotation);
    Point::new(
        (local.x - layer.x) / layer.width.max(1.0),
        (local.y - layer.y) / layer.height.max(1.0),
    )
}

/// `psTransformHandlesDoc` (ps-transform.ts:102).
pub fn transform_handles_doc(layer: &CanvasPsLayer, mode: PsTransformMode) -> Vec<(usize, Point)> {
    if mode == PsTransformMode::Warp {
        let transform = layer.transform.clone().unwrap_or_else(default_transform);
        return transform_control(&transform)
            .into_iter()
            .enumerate()
            .map(|(index, point)| (index, transform_point_doc(layer, point.x, point.y)))
            .collect();
    }
    let handles: &[(f64, f64)] = if matches!(
        mode,
        PsTransformMode::Distort | PsTransformMode::Perspective
    ) {
        &CORNER_HANDLES
    } else {
        &SKEW_HANDLES
    };
    handles
        .iter()
        .enumerate()
        .map(|(index, (u, v))| (index, transform_point_doc(layer, *u, *v)))
        .collect()
}

/// `psMoveTransformHandle` (ps-transform.ts:116).
pub fn move_transform_handle(
    layer: &CanvasPsLayer,
    mode: PsTransformMode,
    handle: usize,
    point: Point,
) -> CanvasPsTransform {
    let transform = layer.transform.clone().unwrap_or_else(default_transform);
    if mode == PsTransformMode::Warp {
        return set_control_point(
            &transform,
            handle.min(15),
            doc_to_transform_point(layer, point),
        );
    }
    let control = transform_control(&transform);
    let mut quad: Vec<Point> = CORNER_OFFSETS
        .iter()
        .filter_map(|index| control.get(*index).copied())
        .collect();
    let local = doc_to_transform_point(layer, point);
    if mode == PsTransformMode::Distort {
        if let Some(slot) = quad.get_mut(handle % 4) {
            *slot = local;
        }
        return CanvasPsTransform {
            quad: to_positions(quad),
            warp: None,
        };
    }
    if mode == PsTransformMode::Perspective {
        let corner = handle % 4;
        let opposite = (corner + 2) % 4;
        if corner >= quad.len() || opposite >= quad.len() {
            return CanvasPsTransform {
                quad: to_positions(quad),
                warp: None,
            };
        }
        let delta = Point::new(local.x - quad[corner].x, local.y - quad[corner].y);
        quad[corner] = local;
        quad[opposite] = Point::new(quad[opposite].x - delta.x, quad[opposite].y - delta.y);
        return CanvasPsTransform {
            quad: to_positions(quad),
            warp: None,
        };
    }
    let (u, v) = SKEW_HANDLES[handle % 8];
    let dx = local.x - u;
    let dy = local.y - v;
    if quad.len() < 4 {
        return CanvasPsTransform {
            quad: to_positions(quad),
            warp: None,
        };
    }
    if SKEW_TOP.contains(&handle) {
        quad[0] = Point::new(quad[0].x + dx, quad[0].y);
        quad[1] = Point::new(quad[1].x + dx, quad[1].y);
    }
    if SKEW_BOTTOM.contains(&handle) {
        quad[3] = Point::new(quad[3].x + dx, quad[3].y);
        quad[2] = Point::new(quad[2].x + dx, quad[2].y);
    }
    if SKEW_LEFT.contains(&handle) {
        quad[0] = Point::new(quad[0].x, quad[0].y + dy);
        quad[3] = Point::new(quad[3].x, quad[3].y + dy);
    }
    if SKEW_RIGHT.contains(&handle) {
        quad[1] = Point::new(quad[1].x, quad[1].y + dy);
        quad[2] = Point::new(quad[2].x, quad[2].y + dy);
    }
    CanvasPsTransform {
        quad: to_positions(quad),
        warp: None,
    }
}

/// `psApplyNumericTransform` (ps-transform.ts:157).
pub fn apply_numeric_transform(
    layers: &[CanvasPsLayer],
    id: &str,
    params: PsNumericTransform,
) -> Vec<CanvasPsLayer> {
    let scale_x = (params.scale_x / 100.0).max(0.01);
    let scale_y = (params.scale_y / 100.0).max(0.01);
    let skew_x = (params.skew_x.clamp(-80.0, 80.0)).to_radians().tan();
    let skew_y = (params.skew_y.clamp(-80.0, 80.0)).to_radians().tan();
    let skewed = params.skew_x.abs() > 1e-3 || params.skew_y.abs() > 1e-3;
    let quad = if skewed {
        Some(vec![
            Point::new(skew_x, 0.0),
            Point::new(1.0 + skew_x, skew_y),
            Point::new(1.0, 1.0 + skew_y),
            Point::new(0.0, 1.0),
        ])
    } else {
        None
    };
    layers
        .iter()
        .map(|layer| {
            if layer.id != id {
                return layer.clone();
            }
            let width = (layer.width * scale_x).max(1.0);
            let height = (layer.height * scale_y).max(1.0);
            let centre_x = layer.x + layer.width / 2.0 + params.dx;
            let centre_y = layer.y + layer.height / 2.0 + params.dy;
            let mut next = layer.clone();
            next.x = centre_x - width / 2.0;
            next.y = centre_y - height / 2.0;
            next.width = width;
            next.height = height;
            next.rotation += params.rotation;
            next.font_size = layer
                .font_size
                .map(|size| (size * ((scale_x + scale_y) / 2.0)).max(1.0).round());
            next.transform = quad.clone().map(|quad| CanvasPsTransform {
                quad: to_positions(quad),
                warp: None,
            });
            next
        })
        .collect()
}

/// `psClearTransform` (ps-transform.ts:184).
pub fn clear_transform(layers: &[CanvasPsLayer], id: &str) -> Vec<CanvasPsLayer> {
    layers
        .iter()
        .map(|layer| {
            let mut next = layer.clone();
            if next.id == id {
                next.transform = None;
            }
            next
        })
        .collect()
}

/// `drawPsWarpedContent` (ps-transform.ts:218): source content warped through
/// the 4x4 mesh and painted into an `out_w x out_h` surface, one subdivided
/// triangle pair per mesh cell.
pub fn warp_mesh(src: &Surface, mesh: &[Point], out_w: usize, out_h: usize) -> Surface {
    let out_w = out_w.max(1);
    let out_h = out_h.max(1);
    let mut out = Surface::new(out_w, out_h);
    if mesh.len() != GRID_POINTS {
        return src.resample_bilinear(out_w, out_h);
    }
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let (ow, oh) = (out_w as f64, out_h as f64);
    let span = (PS_WARP_GRID - 1) as f64;
    for row in 0..PS_WARP_GRID - 1 {
        for col in 0..PS_WARP_GRID - 1 {
            let i00 = row * PS_WARP_GRID + col;
            let idx = [i00, i00 + 1, i00 + PS_WARP_GRID + 1, i00 + PS_WARP_GRID];
            let (u0, u1) = (col as f64 / span, (col + 1) as f64 / span);
            let (v0, v1) = (row as f64 / span, (row + 1) as f64 / span);
            let source = [
                Point::new(u0 * sw, v0 * sh),
                Point::new(u1 * sw, v0 * sh),
                Point::new(u1 * sw, v1 * sh),
                Point::new(u0 * sw, v1 * sh),
            ];
            let target: Vec<Point> = idx
                .iter()
                .map(|index| Point::new(mesh[*index].x * ow, mesh[*index].y * oh))
                .collect();
            raster_triangle(
                &mut out,
                src,
                &[source[0], source[1], source[2]],
                &[target[0], target[1], target[2]],
            );
            raster_triangle(
                &mut out,
                src,
                &[source[0], source[2], source[3]],
                &[target[0], target[2], target[3]],
            );
        }
    }
    out
}

fn raster_triangle(out: &mut Surface, src: &Surface, source: &[Point], target: &[Point]) {
    if source.len() < 3 || target.len() < 3 {
        return;
    }
    let (d0, d1, d2) = (target[0], target[1], target[2]);
    let area = edge(d0, d1, d2);
    if area.abs() < 1e-9 {
        return;
    }
    let min_x = d0.x.min(d1.x).min(d2.x).floor().max(0.0) as i64;
    let max_x = d0.x.max(d1.x).max(d2.x).ceil().min(out.width() as f64);
    let min_y = d0.y.min(d1.y).min(d2.y).floor().max(0.0) as i64;
    let max_y = d0.y.max(d1.y).max(d2.y).ceil().min(out.height() as f64);
    for y in min_y..(max_y as i64) {
        for x in min_x..(max_x as i64) {
            let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let w0 = edge(d1, d2, p) / area;
            let w1 = edge(d2, d0, p) / area;
            let w2 = edge(d0, d1, p) / area;
            if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
                continue;
            }
            let sx = w0 * source[0].x + w1 * source[1].x + w2 * source[2].x;
            let sy = w0 * source[0].y + w1 * source[1].y + w2 * source[2].y;
            let sample = src.sample_bilinear(sx - 0.5, sy - 0.5);
            out.set(
                x,
                y,
                [
                    clamp_u8(sample[0]),
                    clamp_u8(sample[1]),
                    clamp_u8(sample[2]),
                    clamp_u8(sample[3]),
                ],
            );
        }
    }
}

fn edge(a: Point, b: Point, p: Point) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::doc::CanvasPsLayerKind;

    fn base_layer() -> CanvasPsLayer {
        CanvasPsLayer {
            id: "l".into(),
            name: "L".into(),
            kind: CanvasPsLayerKind::Pixel,
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
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            rotation: 30.0,
            opacity: 1.0,
            blend_mode: "normal".into(),
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

    #[test]
    fn identity_detection() {
        assert!(is_identity_transform(None));
        assert!(is_identity_transform(Some(&default_transform())));
        let warped = CanvasPsTransform {
            quad: to_positions(identity_quad()),
            warp: Some(to_positions(
                (0..16)
                    .map(|i| Point::new((i % 4) as f64 / 3.0 + 0.2, (i / 4) as f64 / 3.0))
                    .collect(),
            )),
        };
        assert!(!is_identity_transform(Some(&warped)));
    }

    #[test]
    fn transform_mesh_is_none_for_identity() {
        assert!(transform_mesh(Some(&default_transform())).is_none());
        let skewed = CanvasPsTransform {
            quad: to_positions(vec![
                Point::new(0.2, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(0.0, 1.0),
            ]),
            warp: None,
        };
        let mesh = transform_mesh(Some(&skewed)).expect("mesh");
        assert_eq!(mesh.len(), 16);
    }

    #[test]
    fn mesh_point_on_identity_returns_uv() {
        let mesh = identity_mesh();
        let point = mesh_point(&mesh, 0.37, 0.62);
        assert!((point.x - 0.37).abs() < 1e-9);
        assert!((point.y - 0.62).abs() < 1e-9);
    }

    #[test]
    fn doc_to_transform_point_inverts_transform_point_doc() {
        let layer = base_layer();
        let doc = transform_point_doc(&layer, 0.25, 0.75);
        let local = doc_to_transform_point(&layer, doc);
        assert!((local.x - 0.25).abs() < 1e-9);
        assert!((local.y - 0.75).abs() < 1e-9);
    }

    #[test]
    fn numeric_transform_scales_and_clears() {
        let layer = base_layer();
        let params = PsNumericTransform {
            dx: 1.0,
            dy: 2.0,
            scale_x: 200.0,
            scale_y: 100.0,
            rotation: 15.0,
            skew_x: 0.0,
            skew_y: 0.0,
        };
        let out = apply_numeric_transform(std::slice::from_ref(&layer), "l", params);
        assert!((out[0].width - 200.0).abs() < 1e-9);
        assert!((out[0].height - 50.0).abs() < 1e-9);
        assert!((out[0].rotation - 45.0).abs() < 1e-9);
        assert!((out[0].x - (10.0 + 50.0 + 1.0 - 100.0)).abs() < 1e-9);
        let cleared = clear_transform(&out, "l");
        assert!(cleared[0].transform.is_none());
    }

    #[test]
    fn identity_warp_reproduces_the_source() {
        let mut src = Surface::new(3, 2);
        src.set(1, 0, [255, 0, 0, 255]);
        src.set(2, 1, [0, 255, 0, 255]);
        let out = warp_mesh(&src, &identity_mesh(), 3, 2);
        assert_eq!(out.get(1, 0), [255, 0, 0, 255]);
        assert_eq!(out.get(2, 1), [0, 255, 0, 255]);
        assert_eq!(out.get(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn warp_mesh_paints_content_at_the_target_mesh_position() {
        let mut src = Surface::new(4, 4);
        src.set(0, 0, [255, 0, 0, 255]);
        let shifted: Vec<Point> = identity_mesh()
            .into_iter()
            .map(|point| Point::new(point.x + 0.25, point.y))
            .collect();
        let out = warp_mesh(&src, &shifted, 4, 4);
        assert_eq!(out.get(0, 0), [0, 0, 0, 0], "the old position is empty");
        assert!(out.get(1, 0)[0] > 200, "the pixel moved one quarter right");
    }
}
