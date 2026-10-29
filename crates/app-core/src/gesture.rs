//! Node resize geometry and the node drag / resize gestures.

use open_canvas_core::geometry::{
    is_node_hidden, is_node_locked, snap_drag_to_guides, MovedNode, SnapGuides, SnapOptions,
};
use open_canvas_core::node::{CanvasNodeType, Position};

use crate::coords::ScreenPos;
use crate::doc::{Doc, Drag};

/// Corner resize handle (`ResizeCorner` in canvas-node.tsx:816-825).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    fn anchors_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    fn anchors_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
}

/// Minimum node size on resize (canvas-node.tsx:260-261).
pub const MIN_NODE_WIDTH: f64 = 220.0;
pub const MIN_NODE_HEIGHT: f64 = 160.0;

/// A corner-resize gesture snapshot; [`ResizeGesture::bounds_at`] reproduces
/// canvas-node.tsx:255-290 (min size + optional ratio lock, opposite corner fixed).
#[derive(Debug, Clone, PartialEq)]
pub struct ResizeGesture {
    pub node_id: String,
    pub corner: Corner,
    pub start: ScreenPos,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub keep_ratio: bool,
    pub ratio: f64,
}

impl ResizeGesture {
    pub fn bounds_at(&self, at: ScreenPos, k: f64) -> (f64, f64, f64, f64) {
        let dx = (at.x - self.start.x) / k;
        let dy = (at.y - self.start.y) / k;
        let start_right = self.x + self.width;
        let start_bottom = self.y + self.height;
        let mut width =
            MIN_NODE_WIDTH.max(self.width + if self.corner.anchors_left() { -dx } else { dx });
        let mut height =
            MIN_NODE_HEIGHT.max(self.height + if self.corner.anchors_top() { -dy } else { dy });
        if self.keep_ratio {
            if dx.abs() >= dy.abs() {
                height = width / self.ratio;
            } else {
                width = height * self.ratio;
            }
            if height < MIN_NODE_HEIGHT {
                height = MIN_NODE_HEIGHT;
                width = height * self.ratio;
            }
            if width < MIN_NODE_WIDTH {
                width = MIN_NODE_WIDTH;
                height = width / self.ratio;
            }
        }
        let x = if self.corner.anchors_left() {
            start_right - width
        } else {
            self.x
        };
        let y = if self.corner.anchors_top() {
            start_bottom - height
        } else {
            self.y
        };
        (x, y, width, height)
    }
}

impl Doc {
    /// Starts dragging `id` with the current selection; locked nodes refuse.
    pub fn begin_node_drag(&mut self, id: &str, at: ScreenPos) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        if is_node_locked(node) {
            return false;
        }
        let ids: Vec<String> = if self.selection.contains(id) {
            self.selection
                .iter()
                .filter(|candidate| {
                    self.node(candidate)
                        .is_some_and(|n| !is_node_locked(n) && !is_node_hidden(n))
                })
                .cloned()
                .collect()
        } else {
            vec![id.to_string()]
        };
        let initial: Vec<MovedNode> = ids
            .iter()
            .filter_map(|candidate| self.node(candidate))
            .map(|n| MovedNode {
                id: n.id.clone(),
                x: n.position.x,
                y: n.position.y,
            })
            .collect();
        self.drag = Some(Drag::Nodes {
            start: at,
            ids,
            initial,
            moved: false,
        });
        true
    }

    /// Applies a drag sample: raw delta from the gesture start, snapped only
    /// when an alignment guide is within `6/k`. Grid quantization is disabled:
    /// a 16px grid made a slow drag move in visible jumps, which read as
    /// stutter even when every frame rendered on time.
    pub fn node_drag_to(&mut self, at: ScreenPos) -> bool {
        let Some(Drag::Nodes {
            start,
            ids,
            initial,
            moved,
        }) = &mut self.drag
        else {
            return false;
        };
        let raw_dx = (at.x - start.x) / self.viewport.k;
        let world_dy = (at.y - start.y) / self.viewport.k;
        *moved = *moved || (at.x - start.x).abs() > 3.0 || (at.y - start.y).abs() > 3.0;
        let snapped = snap_drag_to_guides(
            initial,
            &self.nodes,
            raw_dx,
            world_dy,
            SnapOptions::from_scale(self.viewport.k, 0.0),
        );
        for node in &mut self.nodes {
            let Some(origin) = initial.iter().find(|m| m.id == node.id) else {
                continue;
            };
            if !ids.contains(&node.id) {
                continue;
            }
            node.position = Position::new(origin.x + snapped.dx, origin.y + snapped.dy);
        }
        self.snap_guides = snapped.guides;
        true
    }

    /// Ends a node drag; returns whether the document changed (moved).
    pub fn end_node_drag(&mut self) -> bool {
        let Some(Drag::Nodes { moved, .. }) = self.drag.take() else {
            return false;
        };
        self.snap_guides = SnapGuides::default();
        moved
    }

    pub fn begin_resize(&mut self, id: &str, corner: Corner, at: ScreenPos) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        if is_node_locked(node) {
            return false;
        }
        let meta = node.metadata.as_ref();
        let natural = meta.and_then(|m| m.natural_width.zip(m.natural_height));
        let ratio = natural.map_or(node.width / node.height.max(1.0), |(w, h)| w / h.max(1.0));
        let keep_ratio = match node.node_type {
            // builtin-nodes.tsx:20-33 - image unless `freeResize`, video, image-generation.
            CanvasNodeType::Image => !meta.and_then(|m| m.free_resize).unwrap_or(false),
            CanvasNodeType::Video | CanvasNodeType::ImageGeneration => true,
            _ => false,
        };
        self.drag = Some(Drag::Resize(ResizeGesture {
            node_id: id.to_string(),
            corner,
            start: at,
            x: node.position.x,
            y: node.position.y,
            width: node.width,
            height: node.height,
            keep_ratio,
            ratio,
        }));
        true
    }

    pub fn resize_to(&mut self, at: ScreenPos) -> bool {
        let Some(Drag::Resize(gesture)) = &self.drag else {
            return false;
        };
        let (x, y, width, height) = gesture.bounds_at(at, self.viewport.k);
        let Some(node) = self.nodes.iter_mut().find(|n| n.id == gesture.node_id) else {
            return false;
        };
        node.position = Position::new(x, y);
        node.width = width;
        node.height = height;
        true
    }

    pub fn end_resize(&mut self) -> bool {
        matches!(self.drag.take(), Some(Drag::Resize(_)))
    }
}
