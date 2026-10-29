//! Document image ops — port of `web/src/lib/canvas/ps/image-ops.ts`: layer
//! geometry scaling/offset/rotation, trim box, canvas anchors and bitmap
//! resampling. The web's blob round-trip is replaced by in-memory surfaces.

use std::collections::HashMap;

use crate::doc::{CanvasPsLayer, CanvasPsLayerKind};
use crate::geom::{rotate_point, Bounds, Point};
use crate::surface::{px_size, Surface};

/// `PsDocBox` (image-ops.ts:7).
pub type PsDocBox = Bounds;

/// `PsCanvasAnchor` (image-ops.ts:8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsCanvasAnchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// `psScaleLayers` (image-ops.ts:10).
pub fn ps_scale_layers(layers: &[CanvasPsLayer], sx: f64, sy: f64) -> Vec<CanvasPsLayer> {
    let text_scale = (sx + sy) / 2.0;
    layers
        .iter()
        .map(|layer| {
            let mut next = layer.clone();
            next.x = layer.x * sx;
            next.y = layer.y * sy;
            next.width = (layer.width * sx).max(1.0);
            next.height = (layer.height * sy).max(1.0);
            next.font_size = layer
                .font_size
                .map(|size| (size * text_scale).max(1.0).round());
            next
        })
        .collect()
}

/// `psOffsetLayers` (image-ops.ts:22).
pub fn ps_offset_layers(layers: &[CanvasPsLayer], dx: f64, dy: f64) -> Vec<CanvasPsLayer> {
    layers
        .iter()
        .map(|layer| {
            let mut next = layer.clone();
            next.x = layer.x + dx;
            next.y = layer.y + dy;
            next
        })
        .collect()
}

/// `psRotateLayers` (image-ops.ts:27): rotate around the document centre and
/// grow the canvas to the rotated document's bounding box.
pub fn ps_rotate_layers(
    layers: &[CanvasPsLayer],
    degrees: f64,
    width: f64,
    height: f64,
) -> (Vec<CanvasPsLayer>, f64, f64) {
    let centre = Point::new(width / 2.0, height / 2.0);
    let corners: Vec<Bounds> = [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)]
        .iter()
        .map(|(x, y)| {
            let point = rotate_point(*x, *y, centre, degrees);
            Bounds::new(point.x, point.y, 0.0, 0.0)
        })
        .collect();
    let Some(box_) = Bounds::union(&corners) else {
        return (layers.to_vec(), width, height);
    };
    let rotated = layers
        .iter()
        .map(|layer| {
            let next = rotate_point(
                layer.x + layer.width / 2.0,
                layer.y + layer.height / 2.0,
                centre,
                degrees,
            );
            let mut out = layer.clone();
            out.x = next.x - layer.width / 2.0 - box_.x;
            out.y = next.y - layer.height / 2.0 - box_.y;
            out.rotation += degrees;
            out
        })
        .collect();
    (
        rotated,
        box_.width.max(1.0).round(),
        box_.height.max(1.0).round(),
    )
}

/// `psTrimBox` (image-ops.ts:39): the union of the visible layers' boxes,
/// clamped to the document; adjustment layers have no pixels of their own.
pub fn ps_trim_box(layers: &[CanvasPsLayer], width: f64, height: f64) -> PsDocBox {
    let boxes: Vec<Bounds> = layers
        .iter()
        .filter(|layer| !layer.hidden && layer.kind != CanvasPsLayerKind::Adjustment)
        .map(|layer| Bounds::new(layer.x, layer.y, layer.width, layer.height))
        .collect();
    let Some(box_) = Bounds::union(&boxes) else {
        return Bounds::new(0.0, 0.0, width, height);
    };
    let x = box_.x.floor().max(0.0);
    let y = box_.y.floor().max(0.0);
    let right = (box_.x + box_.width).ceil().min(width);
    let bottom = (box_.y + box_.height).ceil().min(height);
    if right <= x || bottom <= y {
        return Bounds::new(0.0, 0.0, width, height);
    }
    Bounds::new(x, y, right - x, bottom - y)
}

/// `psAnchorOffset` (image-ops.ts:49).
pub fn ps_anchor_offset(
    anchor: PsCanvasAnchor,
    width: f64,
    height: f64,
    next_width: f64,
    next_height: f64,
) -> (f64, f64) {
    let dx = width - next_width;
    let dy = height - next_height;
    let left = matches!(
        anchor,
        PsCanvasAnchor::TopLeft | PsCanvasAnchor::Left | PsCanvasAnchor::BottomLeft
    );
    let right = matches!(
        anchor,
        PsCanvasAnchor::TopRight | PsCanvasAnchor::Right | PsCanvasAnchor::BottomRight
    );
    let top = matches!(
        anchor,
        PsCanvasAnchor::TopLeft | PsCanvasAnchor::Top | PsCanvasAnchor::TopRight
    );
    let bottom = matches!(
        anchor,
        PsCanvasAnchor::BottomLeft | PsCanvasAnchor::Bottom | PsCanvasAnchor::BottomRight
    );
    let offset_x = if left {
        0.0
    } else if right {
        dx
    } else {
        dx / 2.0
    };
    let offset_y = if top {
        0.0
    } else if bottom {
        dy
    } else {
        dy / 2.0
    };
    (offset_x, offset_y)
}

/// `psResampleLayerBitmaps` (image-ops.ts:58): re-encode each layer's bitmap and
/// mask at the layer's new pixel size.
pub fn ps_resample_layer_bitmaps(
    layers: &[CanvasPsLayer],
    bitmaps: &HashMap<String, Surface>,
) -> HashMap<String, Surface> {
    let mut out = HashMap::new();
    for layer in layers {
        let width = px_size(layer.width);
        let height = px_size(layer.height);
        for key in [layer.storage_key.as_ref(), layer.mask_storage_key.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(surface) = bitmaps.get(key) {
                out.insert(key.clone(), surface.resample_bilinear(width, height));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::board::create_ps_pixel_layer;
    use crate::doc::IdGen;

    fn layer() -> CanvasPsLayer {
        let mut gen = IdGen::new("l");
        let mut layer = create_ps_pixel_layer(&mut gen, (100.0, 100.0), "a");
        layer.x = 10.0;
        layer.y = 20.0;
        layer.width = 40.0;
        layer.height = 30.0;
        layer
    }

    #[test]
    fn scale_and_offset_layers() {
        let scaled = ps_scale_layers(&[layer()], 2.0, 2.0);
        assert!((scaled[0].x - 20.0).abs() < 1e-9);
        assert!((scaled[0].width - 80.0).abs() < 1e-9);
        let offset = ps_offset_layers(&[layer()], 5.0, -5.0);
        assert!((offset[0].x - 15.0).abs() < 1e-9);
        assert!((offset[0].y - 15.0).abs() < 1e-9);
    }

    #[test]
    fn rotate_grows_the_canvas() {
        let (rotated, width, height) = ps_rotate_layers(&[layer()], 90.0, 100.0, 50.0);
        assert_eq!(rotated.len(), 1);
        assert!((width - 50.0).abs() < 1e-6);
        assert!((height - 100.0).abs() < 1e-6);
    }

    #[test]
    fn trim_box_clamps_to_document() {
        let box_ = ps_trim_box(&[layer()], 100.0, 100.0);
        assert_eq!(box_, Bounds::new(10.0, 20.0, 40.0, 30.0));
        let empty = ps_trim_box(&[], 100.0, 100.0);
        assert_eq!(empty, Bounds::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn anchor_offsets() {
        assert_eq!(
            ps_anchor_offset(PsCanvasAnchor::TopLeft, 10.0, 10.0, 8.0, 6.0),
            (0.0, 0.0)
        );
        assert_eq!(
            ps_anchor_offset(PsCanvasAnchor::BottomRight, 10.0, 10.0, 8.0, 6.0),
            (2.0, 4.0)
        );
        assert_eq!(
            ps_anchor_offset(PsCanvasAnchor::Center, 10.0, 10.0, 8.0, 6.0),
            (1.0, 2.0)
        );
    }

    #[test]
    fn resample_uses_layer_size() {
        let mut layer = layer();
        layer.storage_key = Some("image:x".to_string());
        let mut bitmaps = HashMap::new();
        bitmaps.insert("image:x".to_string(), Surface::new(4, 4));
        let resampled = ps_resample_layer_bitmaps(&[layer], &bitmaps);
        let surface = resampled.get("image:x").expect("surface");
        assert_eq!(surface.width(), 40);
        assert_eq!(surface.height(), 30);
    }
}
