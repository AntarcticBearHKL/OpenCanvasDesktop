//! Node geometry helpers, mirroring `web/src/lib/canvas/canvas-node-geometry.ts`
//! and `web/src/lib/canvas/canvas-node-size.ts`.

use crate::node::{CanvasNodeData, ConnectionHandleType, Position};

/// Snap-to-grid step (canvas-node-geometry.ts:17).
pub const CANVAS_GRID_SIZE: f64 = 16.0;

/// Default `fit_node_size` ceiling (canvas-node-size.ts:1).
pub const FIT_NODE_MAX_SIZE: f64 = 640.0;

/// Guide-snap distance in world units at scale 1 (`6 / k` in the page).
pub const GUIDE_SNAP_THRESHOLD: f64 = 6.0;

/// Axis-aligned bounding box (`{ left, top, right, bottom }`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Bounds {
    pub fn empty() -> Self {
        Self {
            left: f64::INFINITY,
            top: f64::INFINITY,
            right: f64::NEG_INFINITY,
            bottom: f64::NEG_INFINITY,
        }
    }
}

/// Bounding box of a node list (canvas-node-geometry.ts:5-15). Empty list -> infinities.
pub fn node_bounds(nodes: &[CanvasNodeData]) -> Bounds {
    nodes.iter().fold(Bounds::empty(), |acc, node| Bounds {
        left: acc.left.min(node.position.x),
        top: acc.top.min(node.position.y),
        right: acc.right.max(node.position.x + node.width),
        bottom: acc.bottom.max(node.position.y + node.height),
    })
}

/// True when a node's center is inside `rect`'s bounds (canvas-node-geometry.ts:58-62).
pub fn node_center_inside(node: &CanvasNodeData, rect: &CanvasNodeData) -> bool {
    let center_x = node.position.x + node.width / 2.0;
    let center_y = node.position.y + node.height / 2.0;
    center_x >= rect.position.x
        && center_x <= rect.position.x + rect.width
        && center_y >= rect.position.y
        && center_y <= rect.position.y + rect.height
}

pub fn is_node_locked(node: &CanvasNodeData) -> bool {
    node.metadata
        .as_ref()
        .and_then(|m| m.locked)
        .unwrap_or(false)
}

pub fn is_node_hidden(node: &CanvasNodeData) -> bool {
    node.metadata
        .as_ref()
        .and_then(|m| m.hidden)
        .unwrap_or(false)
}

/// Connection anchor: source -> right edge, target -> left edge, both at mid-height
/// (canvas-node-geometry.ts:96-101).
pub fn connection_target_anchor(
    node: &CanvasNodeData,
    handle_type: ConnectionHandleType,
) -> Position {
    let x = match handle_type {
        ConnectionHandleType::Source => node.position.x,
        ConnectionHandleType::Target => node.position.x + node.width,
    };
    Position::new(x, node.position.y + node.height / 2.0)
}

/// Guide-snap threshold for a viewport scale: `6 / max(scale, 0.05)`.
pub fn snap_threshold(scale: f64) -> f64 {
    GUIDE_SNAP_THRESHOLD / scale.max(0.05)
}

/// A node's start position for a drag (id + pre-drag center/top-left coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct MovedNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Tuning for [`snap_drag_to_guides`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapOptions {
    /// Maximum distance for an edge to snap to a guide. The page passes `6 / k`.
    pub threshold: f64,
    /// Grid step used when no guide matches; `0` disables grid snapping.
    pub grid_size: f64,
}

impl SnapOptions {
    pub fn from_scale(scale: f64, grid_size: f64) -> Self {
        Self {
            threshold: snap_threshold(scale),
            grid_size,
        }
    }
}

/// Guide lines produced by a snap (`x` are vertical lines, `y` horizontal).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapGuides {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// Result of [`snap_drag_to_guides`].
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    pub dx: f64,
    pub dy: f64,
    pub guides: SnapGuides,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Snap {
    diff: f64,
    line: f64,
}

/// Snap a multi-node drag to other nodes' edges/centers, else to the grid
/// (canvas-node-geometry.ts:19-56). `initial` holds the pre-drag positions of the
/// moved nodes; `dx`/`dy` are the raw world-space deltas.
pub fn snap_drag_to_guides(
    initial: &[MovedNode],
    nodes: &[CanvasNodeData],
    dx: f64,
    dy: f64,
    options: SnapOptions,
) -> SnapResult {
    let moved_ids: Vec<&str> = initial.iter().map(|m| m.id.as_str()).collect();
    let moved: Vec<&CanvasNodeData> = nodes
        .iter()
        .filter(|node| moved_ids.contains(&node.id.as_str()))
        .collect();
    if moved.is_empty() {
        return SnapResult {
            dx,
            dy,
            guides: SnapGuides::default(),
        };
    }

    let mut bounds = Bounds::empty();
    for node in &moved {
        let Some(start) = initial.iter().find(|m| m.id == node.id) else {
            continue;
        };
        bounds.left = bounds.left.min(start.x + dx);
        bounds.top = bounds.top.min(start.y + dy);
        bounds.right = bounds.right.max(start.x + dx + node.width);
        bounds.bottom = bounds.bottom.max(start.y + dy + node.height);
    }

    let dragged_x = [
        bounds.left,
        (bounds.left + bounds.right) / 2.0,
        bounds.right,
    ];
    let dragged_y = [
        bounds.top,
        (bounds.top + bounds.bottom) / 2.0,
        bounds.bottom,
    ];

    let mut snap_x: Option<Snap> = None;
    let mut snap_y: Option<Snap> = None;
    for node in nodes {
        if moved_ids.contains(&node.id.as_str()) {
            continue;
        }
        for line in [
            node.position.x,
            node.position.x + node.width / 2.0,
            node.position.x + node.width,
        ] {
            for value in dragged_x {
                let diff = line - value;
                if diff.abs() <= options.threshold
                    && snap_x.is_none_or(|s| diff.abs() < s.diff.abs())
                {
                    snap_x = Some(Snap { diff, line });
                }
            }
        }
        for line in [
            node.position.y,
            node.position.y + node.height / 2.0,
            node.position.y + node.height,
        ] {
            for value in dragged_y {
                let diff = line - value;
                if diff.abs() <= options.threshold
                    && snap_y.is_none_or(|s| diff.abs() < s.diff.abs())
                {
                    snap_y = Some(Snap { diff, line });
                }
            }
        }
    }

    let apply_axis = |delta: f64, edge: f64, snap: Option<Snap>| -> f64 {
        match snap {
            Some(s) => delta + s.diff,
            None if options.grid_size > 0.0 => {
                delta + ((edge / options.grid_size).round() * options.grid_size - edge)
            }
            None => delta,
        }
    };

    SnapResult {
        dx: apply_axis(dx, bounds.left, snap_x),
        dy: apply_axis(dy, bounds.top, snap_y),
        guides: SnapGuides {
            x: snap_x.map(|s| vec![s.line]).unwrap_or_default(),
            y: snap_y.map(|s| vec![s.line]).unwrap_or_default(),
        },
    }
}

/// Scale a size down to fit within `max_width`/`max_height`, preserving ratio
/// (canvas-node-size.ts:1-6). Each input is floored at 1.
pub fn fit_node_size(width: f64, height: f64, max_width: f64, max_height: f64) -> (f64, f64) {
    let w = width.max(1.0);
    let h = height.max(1.0);
    let scale = 1.0_f64.min(max_width / w).min(max_height / h);
    (w * scale, h * scale)
}

/// Parse `"WxH"` / `"W:H"` from a size string; returns `(width, height)` or `None`
/// when the string does not start with `<digits>x|:<digits>`.
fn parse_ratio(size: &str) -> Option<(f64, f64)> {
    let bytes = size.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let width: f64 = size[..i].parse().ok()?;
    let separator = *bytes.get(i)?;
    if separator != b'x' && separator != b':' {
        return None;
    }
    let start = i + 1;
    let mut j = start;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j == start {
        return None;
    }
    let height: f64 = size[start..j].parse().ok()?;
    Some((width, height))
}

/// Derive a node size from a `"WxH"` size string (canvas-node-size.ts:8-16).
/// Returns `None` when the string is not a ratio; returns the base size when the
/// ratio is outside `0.25`–`4`.
pub fn node_size_from_ratio(size: &str, base_width: f64, base_height: f64) -> Option<(f64, f64)> {
    let (width, height) = parse_ratio(size)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{CanvasNodeData, CanvasNodeType};

    fn node(id: &str, x: f64, y: f64, w: f64, h: f64) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type: CanvasNodeType::Image,
            title: id.to_string(),
            position: Position::new(x, y),
            width: w,
            height: h,
            metadata: None,
        }
    }

    #[test]
    fn node_bounds_wraps_all_nodes() {
        let nodes = vec![
            node("a", 0.0, 0.0, 100.0, 50.0),
            node("b", 200.0, 120.0, 40.0, 60.0),
        ];
        let bounds = node_bounds(&nodes);
        assert_eq!(bounds.left, 0.0);
        assert_eq!(bounds.top, 0.0);
        assert_eq!(bounds.right, 240.0);
        assert_eq!(bounds.bottom, 180.0);
    }

    #[test]
    fn node_bounds_of_empty_list_is_infinite() {
        let bounds = node_bounds(&[]);
        assert_eq!(bounds.left, f64::INFINITY);
        assert_eq!(bounds.right, f64::NEG_INFINITY);
    }

    #[test]
    fn snap_aligns_moved_node_left_edge_to_other_node_left_edge() {
        let target = node("target", 100.0, 1000.0, 100.0, 100.0);
        let moved = node("moved", 0.0, 0.0, 50.0, 50.0);
        let nodes = vec![target, moved.clone()];
        let initial = vec![MovedNode {
            id: "moved".into(),
            x: 0.0,
            y: 0.0,
        }];
        // Drag to x=97 -> left edge 3px from target left (100), within threshold 6.
        let result = snap_drag_to_guides(
            &initial,
            &nodes,
            97.0,
            0.0,
            SnapOptions {
                threshold: 6.0,
                grid_size: 0.0,
            },
        );
        assert_eq!(result.dx, 100.0);
        assert_eq!(result.guides.x, vec![100.0]);
        assert!(result.guides.y.is_empty());
    }

    #[test]
    fn snap_falls_back_to_grid_when_no_guide_matches() {
        let moved = node("moved", 0.0, 0.0, 50.0, 50.0);
        let nodes = vec![moved];
        let initial = vec![MovedNode {
            id: "moved".into(),
            x: 0.0,
            y: 0.0,
        }];
        let result = snap_drag_to_guides(
            &initial,
            &nodes,
            5.0,
            20.0,
            SnapOptions {
                threshold: 6.0,
                grid_size: CANVAS_GRID_SIZE,
            },
        );
        // 5 -> nearest grid multiple 0 -> dx 0; 20 -> nearest 16 -> dy 16.
        assert_eq!(result.dx, 0.0);
        assert_eq!(result.dy, 16.0);
        assert!(result.guides.x.is_empty() && result.guides.y.is_empty());
    }

    #[test]
    fn snap_with_no_moved_nodes_is_identity() {
        let nodes = vec![node("a", 0.0, 0.0, 10.0, 10.0)];
        let result = snap_drag_to_guides(
            &[],
            &nodes,
            3.0,
            4.0,
            SnapOptions {
                threshold: 6.0,
                grid_size: CANVAS_GRID_SIZE,
            },
        );
        assert_eq!((result.dx, result.dy), (3.0, 4.0));
    }

    #[test]
    fn snap_threshold_scales_inversely_with_scale() {
        assert_eq!(snap_threshold(1.0), 6.0);
        assert_eq!(snap_threshold(2.0), 3.0);
        // Floor at 0.05.
        assert_eq!(snap_threshold(0.0001), 6.0 / 0.05);
    }

    #[test]
    fn fit_node_size_scales_down_preserving_ratio() {
        let (w, h) = fit_node_size(1280.0, 960.0, FIT_NODE_MAX_SIZE, FIT_NODE_MAX_SIZE);
        assert_eq!((w, h), (640.0, 480.0));
        // Already small -> unchanged.
        assert_eq!(fit_node_size(100.0, 80.0, 640.0, 640.0), (100.0, 80.0));
    }

    #[test]
    fn node_size_from_ratio_maps_to_base_axes() {
        // Square ratio (1.0) is below base 340/240 -> width = base_height, height = base_height.
        assert_eq!(
            node_size_from_ratio("1000x1000", 340.0, 240.0),
            Some((240.0, 240.0))
        );
        // Wide ratio (2:1) is above base -> base width, height = base_width / 2.
        assert_eq!(
            node_size_from_ratio("1000x500", 340.0, 240.0),
            Some((340.0, 170.0))
        );
        // Non-ratio string -> None.
        assert_eq!(node_size_from_ratio("auto", 340.0, 240.0), None);
        assert_eq!(node_size_from_ratio("", 340.0, 240.0), None);
    }

    #[test]
    fn node_size_from_ratio_rejects_ratios_outside_quarter_to_four() {
        // 10:1 is out of range -> base size.
        assert_eq!(
            node_size_from_ratio("1000x100", 340.0, 240.0),
            Some((340.0, 240.0))
        );
        // 1:10 is out of range -> base size.
        assert_eq!(
            node_size_from_ratio("100x1000", 340.0, 240.0),
            Some((340.0, 240.0))
        );
    }

    #[test]
    fn node_size_from_ratio_accepts_colon_separator() {
        // 16:9 (1.778) is above base 340/240 -> base width, height = 340 * 9 / 16.
        assert_eq!(
            node_size_from_ratio("16:9", 340.0, 240.0),
            Some((340.0, 191.25))
        );
    }

    #[test]
    fn lock_and_hidden_read_metadata_flags() {
        let mut n = node("a", 0.0, 0.0, 1.0, 1.0);
        assert!(!is_node_locked(&n) && !is_node_hidden(&n));
        n.metadata = Some(crate::node::CanvasNodeMetadata {
            locked: Some(true),
            hidden: Some(true),
            ..Default::default()
        });
        assert!(is_node_locked(&n) && is_node_hidden(&n));
    }

    #[test]
    fn connection_anchor_picks_the_requested_edge() {
        let n = node("a", 10.0, 20.0, 100.0, 40.0);
        assert_eq!(
            connection_target_anchor(&n, ConnectionHandleType::Source),
            Position::new(10.0, 40.0)
        );
        assert_eq!(
            connection_target_anchor(&n, ConnectionHandleType::Target),
            Position::new(110.0, 40.0)
        );
    }

    #[test]
    fn node_center_inside_uses_center_point() {
        let rect = node("rect", 0.0, 0.0, 100.0, 100.0);
        let inside = node("in", 40.0, 40.0, 20.0, 20.0);
        let outside = node("out", 200.0, 200.0, 20.0, 20.0);
        assert!(node_center_inside(&inside, &rect));
        assert!(!node_center_inside(&outside, &rect));
    }
}
