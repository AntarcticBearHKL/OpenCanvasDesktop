//! Node layer popover behaviour: the layer count of one node and its own
//! z-order moves (`canvas-node-layer-popover.tsx`).
//!
//! The node itself is the subject here — this is NOT the removed global node
//! list (its type filter, checkboxes, bulk rename and layer-count header are
//! gone). Selection still follows the canvas, and the popover moves only the
//! node whose hover bar opened it.

use open_canvas_core::node::CanvasNodeData;

use crate::app::App;
use crate::{vm, AppWindow};

/// Layer position of one node in the z-order stack: `(current, total)` with
/// `current` counted from the top, exactly like the web's
/// `stack.length - stackIndex` (`canvas-node-layer-popover.tsx:49`).
pub fn layer_position(nodes: &[CanvasNodeData], id: &str) -> Option<(usize, usize)> {
    let index = nodes.iter().position(|node| node.id == id)?;
    Some((nodes.len() - index, nodes.len()))
}

/// Header line of the node-level popover (`layerCount`): "Layer 2 / 5".
pub fn layer_header(current: usize, total: usize) -> String {
    format!("Layer {current} / {total}")
}

/// Can the node move toward the array head / tail?
pub fn layer_bounds(nodes: &[CanvasNodeData], id: &str) -> (bool, bool) {
    let Some(index) = nodes.iter().position(|node| node.id == id) else {
        return (false, false);
    };
    (index > 0, index + 1 < nodes.len())
}

/// Height of the hover bar and the gap between the card and the bar, in screen
/// pixels (the bar is a screen-space overlay; it does not scale with zoom).
pub const HOVER_BAR_HEIGHT: f32 = 44.0;
pub const HOVER_BAR_GAP: f32 = 8.0;
/// Gap between the bar and the layer popover hanging under it.
pub const LAYER_POPOVER_GAP: f32 = 8.0;

/// Screen anchor of the node layer popover: centred on the node's hover bar and
/// its top edge 8px under the bar's bottom edge. The bar sits 8px under the
/// card, so the popover follows every node move/canvas pan/zoom.
pub fn layer_popover_anchor(
    node: &CanvasNodeData,
    viewport: crate::coords::Viewport,
) -> (f32, f32) {
    let x = viewport.x + (node.position.x + node.width / 2.0) * viewport.k;
    let y = viewport.y
        + (node.position.y + node.height) * viewport.k
        + f64::from(HOVER_BAR_GAP)
        + f64::from(HOVER_BAR_HEIGHT)
        + f64::from(LAYER_POPOVER_GAP);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates are far below f32's exact integer range"
    )]
    {
        (x as f32, y as f32)
    }
}

impl App {
    /// Hover-bar `layers` action: opens (or closes) the layer popover for one
    /// node.
    pub fn toggle_node_layers(&mut self, ui: &AppWindow, id: &str) {
        self.node_layer = if self.node_layer.as_deref() == Some(id) {
            None
        } else {
            Some(id.to_string())
        };
        self.sync_node_layer(ui);
    }

    /// Brings the popover node forward (`up`) or backward (`down`) one step.
    pub fn node_layer_move(&mut self, ui: &AppWindow, direction: &str) {
        let Some(id) = self.node_layer.clone() else {
            return;
        };
        let now = self.now_ms();
        let changed = match direction {
            "up" => self.doc.move_node(&id, true, now),
            "down" => self.doc.move_node(&id, false, now),
            _ => false,
        };
        if changed {
            self.note_doc_change();
            self.sync_all(ui);
        } else {
            self.sync_node_layer(ui);
        }
    }

    /// Pushes the popover's visibility, header and anchor (under the node's
    /// hover bar, exactly where the web renders it inside the bar).
    pub fn sync_node_layer(&mut self, ui: &AppWindow) {
        let Some(id) = self.node_layer.clone() else {
            ui.set_node_layer_open(false);
            return;
        };
        let Some(node) = self.doc.node(&id).cloned() else {
            self.node_layer = None;
            ui.set_node_layer_open(false);
            return;
        };
        let Some((current, total)) = layer_position(self.doc.nodes(), &id) else {
            self.node_layer = None;
            ui.set_node_layer_open(false);
            return;
        };
        let (can_up, can_down) = layer_bounds(self.doc.nodes(), &id);
        let viewport = self.doc.viewport();
        let (x, y) = layer_popover_anchor(&node, viewport);
        ui.set_node_layer_header(layer_header(current, total).into());
        ui.set_node_layer_can_up(can_up);
        ui.set_node_layer_can_down(can_down);
        ui.set_node_layer_x(vm::len(f64::from(x)));
        ui.set_node_layer_y(vm::len(f64::from(y)));
        ui.set_node_layer_open(true);
    }

    /// Closes the popover (its × button, Escape or a workspace switch).
    pub fn close_node_layer(&mut self, ui: &AppWindow) {
        if self.node_layer.take().is_some() {
            ui.set_node_layer_open(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeType, Position};

    fn node(id: &str, kind: CanvasNodeType) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(0.0, 0.0),
            width: 100.0,
            height: 100.0,
            metadata: Some(CanvasNodeMetadata::default()),
        }
    }

    #[test]
    fn layer_position_counts_from_the_top_and_bounds_the_ends() {
        let nodes = vec![
            node("bottom", CanvasNodeType::Image),
            node("middle", CanvasNodeType::Image),
            node("top", CanvasNodeType::Image),
        ];
        // Array order is the z-order: the last entry is the top layer.
        assert_eq!(layer_position(&nodes, "top"), Some((1, 3)));
        assert_eq!(layer_position(&nodes, "middle"), Some((2, 3)));
        assert_eq!(layer_position(&nodes, "bottom"), Some((3, 3)));
        assert_eq!(layer_position(&nodes, "missing"), None);
        assert_eq!(layer_header(2, 5), "Layer 2 / 5");
        assert_eq!(layer_bounds(&nodes, "top"), (true, false));
        assert_eq!(layer_bounds(&nodes, "middle"), (true, true));
        assert_eq!(layer_bounds(&nodes, "bottom"), (false, true));
        assert_eq!(layer_bounds(&nodes, "missing"), (false, false));
    }

    /// The layer popover hangs under the hover bar: bar top = card bottom + 8,
    /// popover top = bar top + 44 + 8; it follows pans, drags and zoom.
    #[test]
    fn layer_popover_anchors_under_the_hover_bar() {
        use crate::coords::Viewport;

        let mut card = node("n", CanvasNodeType::Image);
        card.position = Position::new(100.0, 200.0);
        card.width = 340.0;
        card.height = 240.0;

        let identity = Viewport::new();
        let (bar_x, bar_y) = crate::bars::hover_anchor(&card, identity);
        let (x, y) = layer_popover_anchor(&card, identity);
        assert!((x - bar_x).abs() < 0.01, "centred with the bar: {x}");
        assert!(
            (y - (bar_y + 44.0 + 8.0)).abs() < 0.01,
            "popover top 8px under the 44px bar: {y}"
        );
        assert!(y > 448.0, "the popover is under the card, not over it");

        // The same world rect at 2x: the popover follows the zoomed card and bar.
        let zoomed = Viewport {
            x: 10.0,
            y: 20.0,
            k: 2.0,
        };
        let (zx, zy) = layer_popover_anchor(&card, zoomed);
        let expected_y = 20.0 + (200.0 + 240.0) * 2.0 + 8.0 + 44.0 + 8.0;
        assert!((zx - (10.0 + 270.0 * 2.0)).abs() < 0.01);
        assert!((zy - expected_y).abs() < 0.01);
    }

    #[test]
    fn move_node_reorders_the_shared_z_stack() {
        let mut doc = crate::doc::Doc::demo();
        let before: Vec<String> = doc.nodes().iter().map(|node| node.id.clone()).collect();
        let target = before[1].clone();
        assert!(doc.move_node(&target, true, 0));
        assert_eq!(doc.nodes()[0].id, target);
        assert!(doc.move_node(&target, false, 0));
        assert_eq!(doc.nodes()[1].id, target);
        // The array ends reject the move instead of wrapping around.
        let first = doc.nodes()[0].id.clone();
        let last = doc
            .nodes()
            .last()
            .map(|node| node.id.clone())
            .unwrap_or_default();
        assert!(!doc.move_node(&first, true, 0));
        assert!(!doc.move_node(&last, false, 0));
        assert!(!doc.move_node("missing", true, 0));
    }
}
