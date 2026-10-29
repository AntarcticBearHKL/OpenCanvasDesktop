//! Path painting — port of `web/src/lib/canvas/ps/path-ops.ts`: fill or stroke a
//! board-local path into a layer bitmap, then clip by the selection.

use crate::doc::{CanvasPsLayer, CanvasPsPath};
use crate::geom::{rotate_point, Point};
use crate::path::polyline;
use crate::surface::{clamp_u8, hex_to_rgba, Surface};

/// The layer-space transform of `psApplyLayerSpace` (path-ops.ts:20): maps a
/// board point into the layer bitmap's pixel space.
fn board_to_layer(layer: &CanvasPsLayer, point: Point) -> Point {
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    let p1 = Point::new(point.x - centre.x, point.y - centre.y);
    let rotated = rotate_point(p1.x, p1.y, Point::new(0.0, 0.0), -layer.rotation);
    Point::new(
        rotated.x + centre.x - layer.x,
        rotated.y + centre.y - layer.y,
    )
}

fn distance_to_segment(point: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq <= 1e-12 {
        0.0
    } else {
        (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_sq).clamp(0.0, 1.0)
    };
    let proj = Point::new(a.x + t * dx, a.y + t * dy);
    (point.x - proj.x).hypot(point.y - proj.y)
}

fn stroke_polyline(target: &mut Surface, points: &[Point], color: [u8; 4], stroke_width: f64) {
    let half = (stroke_width / 2.0).max(0.5);
    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let min_x = (a.x.min(b.x) - half).floor().max(0.0) as i64;
        let max_x = (a.x.max(b.x) + half).ceil().min(target.width() as f64) as i64;
        let min_y = (a.y.min(b.y) - half).floor().max(0.0) as i64;
        let max_y = (a.y.max(b.y) + half).ceil().min(target.height() as f64) as i64;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                if distance_to_segment(p, a, b) <= half {
                    target.set(x, y, color);
                }
            }
        }
    }
}

/// `psPaintPath` (path-ops.ts:27): draw the path (stroke when `stroke_width > 0`,
/// else fill) onto `base`, then clip by `selection`.
pub fn paint_path(
    layer: &CanvasPsLayer,
    path: &CanvasPsPath,
    color: &str,
    stroke_width: f64,
    base: Option<&Surface>,
    selection: Option<&Surface>,
) -> Surface {
    let width = (layer.width.round() as i64).max(1) as usize;
    let height = (layer.height.round() as i64).max(1) as usize;
    let mut target = Surface::new(width, height);
    if let Some(base) = base {
        let scaled = if base.width() == width && base.height() == height {
            base.clone()
        } else {
            base.resample_bilinear(width, height)
        };
        for y in 0..height {
            for x in 0..width {
                target.set(x as i64, y as i64, scaled.get(x as i64, y as i64));
            }
        }
    }
    let rgba = hex_to_rgba(color);
    let points: Vec<Point> = polyline(path, 24)
        .iter()
        .map(|point| board_to_layer(layer, *point))
        .collect();
    if stroke_width > 0.0 {
        stroke_polyline(&mut target, &points, rgba, stroke_width);
    } else {
        let mask = crate::selection::fill_polygon_mask(width, height, &points);
        for y in 0..height {
            for x in 0..width {
                if mask.get(x as i64, y as i64)[3] > 0 {
                    target.set(x as i64, y as i64, rgba);
                }
            }
        }
    }
    if let Some(selection) = selection {
        let clip = crate::selection::selection_to_layer_space(selection, layer, width, height);
        for y in 0..height {
            for x in 0..width {
                let alpha = clip.get(x as i64, y as i64)[3];
                let pixel = target.get(x as i64, y as i64);
                target.set(
                    x as i64,
                    y as i64,
                    [
                        pixel[0],
                        pixel[1],
                        pixel[2],
                        clamp_u8(f64::from(pixel[3]) * f64::from(alpha) / 255.0),
                    ],
                );
            }
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::CanvasPsLayerKind;
    use crate::path::{create_path, path_anchor};

    #[test]
    fn fills_inside_the_path() {
        let layer = plain_layer();
        let path = create_path(
            "p",
            "box",
            vec![
                path_anchor(2.0, 2.0),
                path_anchor(6.0, 2.0),
                path_anchor(6.0, 6.0),
                path_anchor(2.0, 6.0),
            ],
            true,
        );
        let out = paint_path(&layer, &path, "#ff0000", 0.0, None, None);
        assert_eq!(out.get(4, 4), [255, 0, 0, 255]);
        assert_eq!(out.get(0, 0), [0, 0, 0, 0]);
        assert_eq!(layer.kind, CanvasPsLayerKind::Pixel);
    }

    #[test]
    fn stroke_marks_a_thick_line() {
        let layer = plain_layer();
        let path = create_path(
            "p",
            "line",
            vec![path_anchor(1.0, 5.0), path_anchor(9.0, 5.0)],
            false,
        );
        let out = paint_path(&layer, &path, "#00ff00", 4.0, None, None);
        assert_eq!(out.get(5, 5), [0, 255, 0, 255]);
        assert_eq!(out.get(5, 0), [0, 0, 0, 0]);
    }

    fn plain_layer() -> CanvasPsLayer {
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
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            rotation: 0.0,
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
}
