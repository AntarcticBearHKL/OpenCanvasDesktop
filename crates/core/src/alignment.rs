//! Node alignment and distribution, mirroring `web/src/lib/canvas/alignment.ts`.

use crate::node::{CanvasNodeData, Position};

/// `AlignAxis` (alignment.ts:3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignAxis {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
    DistributeX,
    DistributeY,
}

impl AlignAxis {
    /// The eight axes in the web's declaration order.
    pub const ALL: [AlignAxis; 8] = [
        AlignAxis::Left,
        AlignAxis::CenterX,
        AlignAxis::Right,
        AlignAxis::Top,
        AlignAxis::CenterY,
        AlignAxis::Bottom,
        AlignAxis::DistributeX,
        AlignAxis::DistributeY,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::CenterX => "center-x",
            Self::Right => "right",
            Self::Top => "top",
            Self::CenterY => "center-y",
            Self::Bottom => "bottom",
            Self::DistributeX => "distribute-x",
            Self::DistributeY => "distribute-y",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|axis| axis.as_str() == id)
    }
}

/// `selectionBounds` (alignment.ts:5-11).
struct Bounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Bounds {
    fn of(nodes: &[&CanvasNodeData]) -> Option<Self> {
        let mut bounds: Option<Self> = None;
        for node in nodes {
            let right = node.position.x + node.width;
            let bottom = node.position.y + node.height;
            bounds = Some(match bounds {
                None => Self {
                    left: node.position.x,
                    top: node.position.y,
                    right,
                    bottom,
                },
                Some(acc) => Self {
                    left: acc.left.min(node.position.x),
                    top: acc.top.min(node.position.y),
                    right: acc.right.max(right),
                    bottom: acc.bottom.max(bottom),
                },
            });
        }
        bounds
    }

    fn width(&self) -> f64 {
        self.right - self.left
    }

    fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// JavaScript `Math.round`: halves round toward +infinity (Rust's `f64::round`
/// rounds half away from zero, which differs for negative half-integers).
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `distribute` (alignment.ts:13-27): spreads the middle nodes so the gaps
/// between them are equal; fewer than three nodes is a no-op.
fn distribute(nodes: &[&CanvasNodeData], horizontal: bool) -> Vec<(String, Position)> {
    if nodes.len() < 3 {
        return Vec::new();
    }
    let mut sorted: Vec<&CanvasNodeData> = nodes.to_vec();
    sorted.sort_by(|a, b| {
        let (left, right) = if horizontal {
            (a.position.x, b.position.x)
        } else {
            (a.position.y, b.position.y)
        };
        left.total_cmp(&right)
    });
    let Some(bounds) = Bounds::of(&sorted) else {
        return Vec::new();
    };
    let span = if horizontal {
        bounds.width()
    } else {
        bounds.height()
    };
    let used: f64 = sorted
        .iter()
        .map(|node| if horizontal { node.width } else { node.height })
        .sum();
    let gap = (span - used) / (sorted.len() as f64 - 1.0);
    let mut cursor = if horizontal { bounds.left } else { bounds.top };
    let mut out = Vec::with_capacity(sorted.len());
    for node in sorted {
        let position = if horizontal {
            Position::new(js_round(cursor), node.position.y)
        } else {
            Position::new(node.position.x, js_round(cursor))
        };
        out.push((node.id.clone(), position));
        cursor += if horizontal { node.width } else { node.height } + gap;
    }
    out
}

/// `alignNodes` (alignment.ts:29-44): positions for the selected nodes only;
/// fewer than two selected is a no-op.
pub fn align_nodes(
    nodes: &[CanvasNodeData],
    selected: &[String],
    axis: AlignAxis,
) -> Vec<(String, Position)> {
    let selected_nodes: Vec<&CanvasNodeData> = nodes
        .iter()
        .filter(|node| selected.contains(&node.id))
        .collect();
    if selected_nodes.len() < 2 {
        return Vec::new();
    }
    match axis {
        AlignAxis::DistributeX => return distribute(&selected_nodes, true),
        AlignAxis::DistributeY => return distribute(&selected_nodes, false),
        AlignAxis::Left
        | AlignAxis::CenterX
        | AlignAxis::Right
        | AlignAxis::Top
        | AlignAxis::CenterY
        | AlignAxis::Bottom => {}
    }
    let Some(bounds) = Bounds::of(&selected_nodes) else {
        return Vec::new();
    };
    selected_nodes
        .into_iter()
        .map(|node| {
            let x = match axis {
                AlignAxis::Left => bounds.left,
                AlignAxis::CenterX => bounds.left + (bounds.width() - node.width) / 2.0,
                AlignAxis::Right => bounds.right - node.width,
                AlignAxis::Top
                | AlignAxis::CenterY
                | AlignAxis::Bottom
                | AlignAxis::DistributeX
                | AlignAxis::DistributeY => node.position.x,
            };
            let y = match axis {
                AlignAxis::Top => bounds.top,
                AlignAxis::CenterY => bounds.top + (bounds.height() - node.height) / 2.0,
                AlignAxis::Bottom => bounds.bottom - node.height,
                AlignAxis::Left
                | AlignAxis::CenterX
                | AlignAxis::Right
                | AlignAxis::DistributeX
                | AlignAxis::DistributeY => node.position.y,
            };
            (node.id.clone(), Position::new(js_round(x), js_round(y)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::CanvasNodeType;

    fn node(id: &str, x: f64, y: f64, w: f64, h: f64) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: CanvasNodeType::Text,
            title: id.into(),
            position: Position::new(x, y),
            width: w,
            height: h,
            metadata: None,
        }
    }

    fn selected(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn align_left_snaps_x_and_keeps_y() {
        let nodes = vec![
            node("a", 10.0, 50.0, 100.0, 40.0),
            node("b", 70.0, 5.0, 60.0, 20.0),
        ];
        let out = align_nodes(&nodes, &selected(&["a", "b"]), AlignAxis::Left);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], ("a".to_string(), Position::new(10.0, 50.0)));
        assert_eq!(out[1], ("b".to_string(), Position::new(10.0, 5.0)));
    }

    #[test]
    fn align_center_bottom_rounds_like_javascript() {
        let nodes = vec![
            node("a", 0.0, 0.0, 101.0, 50.0),
            node("b", 30.0, 20.0, 50.0, 31.0),
        ];
        let out = align_nodes(&nodes, &selected(&["a", "b"]), AlignAxis::CenterX);
        // bounds width 101: a.x = (101 - 101) / 2 = 0; b.x = (101 - 50) / 2 = 25.5 -> 26
        assert_eq!(out[0].1.x, 0.0);
        assert_eq!(out[1].1.x, 26.0);
        let bottom = align_nodes(&nodes, &selected(&["a", "b"]), AlignAxis::Bottom);
        // bottom 51: a.y = 51 - 50 = 1; b.y = 51 - 31 = 20
        assert_eq!(bottom[0].1.y, 1.0);
        assert_eq!(bottom[1].1.y, 20.0);
    }

    #[test]
    fn distribute_needs_three_and_equalizes_the_gaps() {
        let nodes = vec![
            node("a", 0.0, 0.0, 100.0, 20.0),
            node("b", 120.0, 40.0, 60.0, 20.0),
            node("c", 500.0, 80.0, 40.0, 20.0),
        ];
        let two = align_nodes(&nodes[0..2], &selected(&["a", "b"]), AlignAxis::DistributeX);
        assert!(two.is_empty());
        let out = align_nodes(&nodes, &selected(&["a", "b", "c"]), AlignAxis::DistributeX);
        // span 540, used 200, gap 170: a stays, b -> 0+100+170=270, c -> 270+60+170=500
        assert_eq!(out[1].1.x, 270.0);
        assert_eq!(out[2].1.x, 500.0);
        // The cross axis is untouched.
        assert_eq!(out[1].1.y, 40.0);
    }

    #[test]
    fn fewer_than_two_selected_is_a_no_op() {
        let nodes = vec![node("a", 1.0, 2.0, 3.0, 4.0)];
        assert!(align_nodes(&nodes, &selected(&["a"]), AlignAxis::Right).is_empty());
        assert!(align_nodes(&nodes, &selected(&[]), AlignAxis::Top).is_empty());
    }
}
