//! Selection mask algebra — port of `web/src/lib/canvas/ps/selection.ts`. A
//! selection is a `Surface` whose alpha channel is the coverage mask; every
//! combine mode (`replace`/`add`/`subtract`/`intersect`) is a plain alpha op
//! instead of a Canvas2D `globalCompositeOperation`.

use crate::doc::CanvasPsLayer;
use crate::filters::gaussian_blur;
use crate::geom::{rotate_point, Bounds, Point};
use crate::surface::{clamp_u8, Surface};

/// `PsSelectionMode` (selection.ts:5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Subtract,
    Intersect,
}

/// `psSelectionCreate` (selection.ts:10).
pub fn selection_create(width: usize, height: usize) -> Surface {
    Surface::new(width.max(1), height.max(1))
}

/// `psSelectionAll` (selection.ts:15): the whole mask opaque white.
pub fn selection_all(selection: &mut Surface) {
    selection.fill([255, 255, 255, 255]);
}

/// `psSelectionClear` (selection.ts:24).
pub fn selection_clear(selection: &mut Surface) {
    selection.fill([0, 0, 0, 0]);
}

/// `psSelectionInvert` (selection.ts:28), expressed as an alpha inversion.
pub fn selection_invert(selection: &mut Surface) {
    for index in (0..selection.len()).step_by(4) {
        let data = selection.data_mut();
        data[index] = 255;
        data[index + 1] = 255;
        data[index + 2] = 255;
        data[index + 3] = 255 - data[index + 3];
    }
}

/// `psSelectionFeather` (selection.ts:37): gaussian blur of the mask.
pub fn selection_feather(selection: &mut Surface, px: f64) {
    if px <= 0.0 {
        return;
    }
    gaussian_blur(selection, px);
}

/// `psSelectionEdges` (selection.ts:50): boundary pixel indices (row-major).
pub fn selection_edges(selection: &Surface, limit: usize) -> Vec<usize> {
    let (width, height) = (selection.width(), selection.height());
    let selected = |x: usize, y: usize| selection.get(x as i64, y as i64)[3] >= 128;
    let mut edges = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if !selected(x, y) {
                continue;
            }
            let boundary = x == 0
                || y == 0
                || x == width - 1
                || y == height - 1
                || !selected(x - 1, y)
                || !selected(x + 1, y)
                || !selected(x, y - 1)
                || !selected(x, y + 1);
            if boundary {
                edges.push(y * width + x);
            }
            if edges.len() >= limit {
                return edges;
            }
        }
    }
    edges
}

/// `psSelectionBounds` (selection.ts:68).
pub fn selection_bounds(selection: &Surface) -> Option<Bounds> {
    let (width, height) = (selection.width(), selection.height());
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for y in 0..height {
        for x in 0..width {
            if selection.get(x as i64, y as i64)[3] < 128 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x as i64);
            max_y = max_y.max(y as i64);
        }
    }
    if max_x < 0 {
        return None;
    }
    Some(Bounds::new(
        min_x as f64,
        min_y as f64,
        (max_x - min_x as i64 + 1) as f64,
        (max_y - min_y as i64 + 1) as f64,
    ))
}

/// `psSelectionShape` (selection.ts:90): feather, then optionally harden the
/// mask to 0/255 when anti-aliasing is off.
fn finish_shape(shape: &mut Surface, feather: f64, anti_alias: bool) {
    if feather > 0.0 {
        gaussian_blur(shape, feather);
    }
    if !anti_alias {
        for index in (3..shape.len()).step_by(4) {
            let data = shape.data_mut();
            data[index] = if data[index] >= 128 { 255 } else { 0 };
        }
    }
}

/// `psSelectionCombine` (selection.ts:106): merge a shape into the selection.
pub fn selection_combine(selection: &mut Surface, shape: &Surface, mode: SelectionMode) {
    let count = selection.len().min(shape.len());
    let shape_data = shape.data().to_vec();
    let selection_data = selection.data_mut();
    for index in (0..count).step_by(4) {
        let a = f64::from(selection_data[index + 3]) / 255.0;
        let b = f64::from(shape_data[index + 3]) / 255.0;
        let merged = match mode {
            SelectionMode::Replace => b,
            SelectionMode::Add => b + a * (1.0 - b),
            SelectionMode::Subtract => a * (1.0 - b),
            SelectionMode::Intersect => a * b,
        };
        selection_data[index] = 255;
        selection_data[index + 1] = 255;
        selection_data[index + 2] = 255;
        selection_data[index + 3] = clamp_u8(merged * 255.0);
    }
}

fn fill_rect_mask(width: usize, height: usize, box_: Bounds, ellipse: bool) -> Surface {
    let mut mask = Surface::new(width, height);
    let (rx, ry) = ((box_.width / 2.0).abs(), (box_.height / 2.0).abs());
    let (cx, cy) = (box_.x + box_.width / 2.0, box_.y + box_.height / 2.0);
    for y in 0..height {
        for x in 0..width {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let inside = if ellipse {
                if rx <= 0.0 || ry <= 0.0 {
                    false
                } else {
                    ((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2) <= 1.0
                }
            } else {
                px >= box_.x
                    && px < box_.x + box_.width
                    && py >= box_.y
                    && py < box_.y + box_.height
            };
            if inside {
                mask.set(x as i64, y as i64, [255, 255, 255, 255]);
            }
        }
    }
    mask
}

/// Scanline (even-odd) polygon fill into a white alpha mask.
pub fn fill_polygon_mask(width: usize, height: usize, points: &[Point]) -> Surface {
    let mut mask = Surface::new(width, height);
    if points.len() < 3 {
        return mask;
    }
    let min_y = points
        .iter()
        .fold(f64::INFINITY, |a, p| a.min(p.y))
        .floor()
        .max(0.0) as usize;
    let max_y = (points
        .iter()
        .fold(f64::NEG_INFINITY, |a, p| a.max(p.y))
        .ceil() as usize)
        .min(height);
    for y in min_y..max_y {
        let scan = y as f64 + 0.5;
        let mut crossings: Vec<f64> = Vec::new();
        for index in 0..points.len() {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            let crosses = (a.y <= scan && b.y > scan) || (b.y <= scan && a.y > scan);
            if crosses && (b.y - a.y).abs() > 1e-12 {
                crossings.push(a.x + (scan - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut index = 0;
        while index + 1 < crossings.len() {
            let x0 = crossings[index];
            let x1 = crossings[index + 1];
            let start = (x0 - 0.5).ceil().max(0.0) as usize;
            let end = ((x1 - 0.5).floor() as i64).max(0) as usize;
            for x in start..=end.min(width.saturating_sub(1)) {
                if (x as f64 + 0.5) >= x0 && (x as f64 + 0.5) <= x1 {
                    mask.set(x as i64, y as i64, [255, 255, 255, 255]);
                }
            }
            index += 2;
        }
    }
    mask
}

/// `psSelectionRect` (selection.ts:114).
pub fn selection_rect(
    selection: &mut Surface,
    box_: Bounds,
    ellipse: bool,
    feather: f64,
    anti_alias: bool,
    mode: SelectionMode,
) {
    let mut shape = fill_rect_mask(selection.width(), selection.height(), box_, ellipse);
    finish_shape(&mut shape, feather, anti_alias);
    selection_combine(selection, &shape, mode);
}

/// `psSelectionPolygon` (selection.ts:124).
pub fn selection_polygon(
    selection: &mut Surface,
    points: &[Point],
    feather: f64,
    anti_alias: bool,
    mode: SelectionMode,
) {
    if points.len() < 3 {
        return;
    }
    let mut shape = fill_polygon_mask(selection.width(), selection.height(), points);
    finish_shape(&mut shape, feather, anti_alias);
    selection_combine(selection, &shape, mode);
}

fn color_near(
    data: &[u8],
    width: usize,
    x: usize,
    y: usize,
    origin: [u8; 4],
    tolerance: f64,
) -> bool {
    let index = (y * width + x) * 4;
    let at = |c: usize| data.get(index + c).copied().unwrap_or(0);
    if at(3) == 0 && origin[3] == 0 {
        return true;
    }
    (0..4).all(|c| (f64::from(at(c)) - f64::from(origin[c])).abs() <= tolerance)
}

/// `psSelectionWand` (selection.ts:148): flood/global near-colour mask.
pub fn selection_wand(
    image: &Surface,
    point: Point,
    tolerance: f64,
    contiguous: bool,
) -> Option<Surface> {
    let (width, height) = (image.width(), image.height());
    let start_x = point.x.floor() as i64;
    let start_y = point.y.floor() as i64;
    if start_x < 0 || start_y < 0 || start_x >= width as i64 || start_y >= height as i64 {
        return None;
    }
    let (sx, sy) = (start_x as usize, start_y as usize);
    let origin = image.get(start_x, start_y);
    let mut picked = vec![0_u8; width * height];
    let data = image.data();
    if contiguous {
        let mut stack = vec![sy * width + sx];
        while let Some(pixel) = stack.pop() {
            if picked[pixel] == 1 {
                continue;
            }
            let x = pixel % width;
            let y = pixel / width;
            if !color_near(data, width, x, y, origin, tolerance) {
                continue;
            }
            picked[pixel] = 1;
            if x > 0 {
                stack.push(pixel - 1);
            }
            if x < width - 1 {
                stack.push(pixel + 1);
            }
            if y > 0 {
                stack.push(pixel - width);
            }
            if y < height - 1 {
                stack.push(pixel + width);
            }
        }
    } else {
        for (pixel, slot) in picked.iter_mut().enumerate() {
            let x = pixel % width;
            let y = pixel / width;
            if color_near(data, width, x, y, origin, tolerance) {
                *slot = 1;
            }
        }
    }
    let mut mask = Surface::new(width, height);
    for (pixel, hit) in picked.iter().enumerate() {
        if *hit == 1 {
            let x = (pixel % width) as i64;
            let y = (pixel / width) as i64;
            mask.set(x, y, [255, 255, 255, 255]);
        }
    }
    Some(mask)
}

/// `psSelectionQuick` (selection.ts:192): round-brush grow/shrink under a
/// colour constraint.
pub fn selection_quick(
    selection: &mut Surface,
    image: Option<&Surface>,
    point: Point,
    radius: f64,
    tolerance: f64,
    sample: Option<[u8; 4]>,
    erase: bool,
) {
    let (width, height) = (selection.width(), selection.height());
    let x0 = (point.x - radius).floor().max(0.0) as usize;
    let y0 = (point.y - radius).floor().max(0.0) as usize;
    let x1 = ((point.x + radius).ceil() as usize).min(width);
    let y1 = ((point.y + radius).ceil() as usize).min(height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let squared = radius * radius;
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f64 + 0.5 - point.x;
            let dy = y as f64 + 0.5 - point.y;
            if dx * dx + dy * dy > squared {
                continue;
            }
            if erase {
                selection.set(x as i64, y as i64, [0, 0, 0, 0]);
                continue;
            }
            if let (Some(image), Some(sample)) = (image, sample) {
                if !color_near(image.data(), image.width(), x, y, sample, tolerance) {
                    continue;
                }
            }
            selection.set(x as i64, y as i64, [255, 255, 255, 255]);
        }
    }
}

/// `psImageColorAt` (selection.ts:222).
pub fn image_color_at(image: &Surface, x: i64, y: i64) -> Option<[u8; 4]> {
    if x < 0 || y < 0 || x >= image.width() as i64 || y >= image.height() as i64 {
        return None;
    }
    Some(image.get(x, y))
}

/// `psSelectionToLayerSpace` (selection.ts:230): the document-space mask mapped
/// into a layer bitmap's own pixel space (inverse of the paint transform).
pub fn selection_to_layer_space(
    selection: &Surface,
    layer: &CanvasPsLayer,
    width: usize,
    height: usize,
) -> Surface {
    let width = width.max(1);
    let height = height.max(1);
    let mut target = Surface::new(width, height);
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    for y in 0..height {
        for x in 0..width {
            let dest = Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let rel = Point::new(dest.x - centre.x + layer.x, dest.y - centre.y + layer.y);
            let rotated = rotate_point(rel.x, rel.y, Point::new(0.0, 0.0), layer.rotation);
            let source = Point::new(centre.x + rotated.x, centre.y + rotated.y);
            let sample = selection.sample_bilinear(source.x - 0.5, source.y - 0.5);
            target.set(
                x as i64,
                y as i64,
                [
                    clamp_u8(sample[0]),
                    clamp_u8(sample[1]),
                    clamp_u8(sample[2]),
                    clamp_u8(sample[3]),
                ],
            );
        }
    }
    target
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::doc::CanvasPsLayerKind;

    fn layer_at(x: f64, y: f64) -> CanvasPsLayer {
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
            x,
            y,
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

    fn mask(w: usize, h: usize, x: i64, y: i64) -> Surface {
        let mut s = Surface::new(w, h);
        s.set(x, y, [255, 255, 255, 255]);
        s
    }

    #[test]
    fn combine_modes_use_alpha_math() {
        let mut sel = mask(2, 2, 0, 0);
        selection_combine(&mut sel, &mask(2, 2, 0, 0), SelectionMode::Intersect);
        assert_eq!(sel.get(0, 0)[3], 255);
        selection_combine(&mut sel, &mask(2, 2, 1, 0), SelectionMode::Add);
        assert_eq!(sel.get(1, 0)[3], 255);
        selection_combine(&mut sel, &mask(2, 2, 0, 0), SelectionMode::Subtract);
        assert_eq!(sel.get(0, 0)[3], 0);
        selection_combine(&mut sel, &mask(2, 2, 1, 0), SelectionMode::Replace);
        assert_eq!(sel.get(1, 0)[3], 255);
        assert_eq!(sel.get(0, 0)[3], 0);
    }

    #[test]
    fn invert_flips_coverage() {
        let mut sel = mask(2, 1, 0, 0);
        selection_invert(&mut sel);
        assert_eq!(sel.get(0, 0)[3], 0);
        assert_eq!(sel.get(1, 0)[3], 255);
    }

    #[test]
    fn rect_and_ellipse_masks() {
        let mut sel = selection_create(4, 4);
        selection_rect(
            &mut sel,
            Bounds::new(0.0, 0.0, 2.0, 2.0),
            false,
            0.0,
            false,
            SelectionMode::Replace,
        );
        assert_eq!(sel.get(0, 0)[3], 255);
        assert_eq!(sel.get(2, 0)[3], 0);
        let bounds = selection_bounds(&sel).expect("bounds");
        assert_eq!(bounds, Bounds::new(0.0, 0.0, 2.0, 2.0));
        let edges = selection_edges(&sel, 12000);
        assert!(edges.contains(&0));
    }

    #[test]
    fn polygon_fill_covers_the_middle() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
        ];
        let mut sel = selection_create(4, 4);
        selection_polygon(&mut sel, &points, 0.0, false, SelectionMode::Replace);
        assert_eq!(sel.get(2, 2)[3], 255);
    }

    #[test]
    fn wand_picks_similar_colors() {
        let mut image = Surface::new(3, 1);
        image.set(0, 0, [10, 10, 10, 255]);
        image.set(1, 0, [12, 12, 12, 255]);
        image.set(2, 0, [200, 200, 200, 255]);
        let mask = selection_wand(&image, Point::new(0.0, 0.0), 5.0, true).expect("mask");
        assert_eq!(mask.get(0, 0)[3], 255);
        assert_eq!(mask.get(1, 0)[3], 255);
        assert_eq!(mask.get(2, 0)[3], 0);
    }

    #[test]
    fn layer_space_maps_identity_and_offset() {
        let layer = layer_at(0.0, 0.0);
        let mut sel = Surface::new(10, 10);
        sel.set(3, 4, [255, 255, 255, 255]);
        let mapped = selection_to_layer_space(&sel, &layer, 10, 10);
        assert_eq!(mapped.get(3, 4)[3], 255);
        let offset = layer_at(2.0, 0.0);
        let mapped = selection_to_layer_space(&sel, &offset, 10, 10);
        assert_eq!(mapped.get(1, 4)[3], 255);
    }
}
