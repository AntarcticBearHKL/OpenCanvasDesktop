//! Connection editing: wire selection, the click hit test, deletion and the
//! Ctrl + right-button cut stroke.
//!
//! Provenance: `canvas-connections.tsx` (hit path `strokeWidth 16`, selected
//! stroke `node.activeStroke` at width 3) and project.tsx:771-807
//! (`startCutStroke` / `moveCutStroke` / `finishCutStroke`).

use open_canvas_core::connections::{connection_crosses_stroke, connection_polyline};
use open_canvas_core::node::Position;

use crate::coords::WorldPos;
use crate::doc::{Doc, Drag};

/// Screen-space hit radius of a wire click (half of the web's 16px hit stroke).
pub const WIRE_HIT_RADIUS: f64 = 8.0;
/// Minimum world-space movement between sampled cut points.
const CUT_MIN_STEP: f64 = 0.5;

/// Squared distance from `point` to the segment `a`-`b` (core `Position` space).
fn segment_distance_sq(point: Position, a: Position, b: Position) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        let px = point.x - a.x;
        let py = point.y - a.y;
        return px * px + py * py;
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_sq).clamp(0.0, 1.0);
    let px = point.x - (a.x + dx * t);
    let py = point.y - (a.y + dy * t);
    px * px + py * py
}

impl Doc {
    /// Selects a wire (`selectedConnectionId`); `None` clears it.
    pub fn select_connection(&mut self, id: Option<&str>) {
        self.selected_connection = id.map(str::to_string);
    }

    pub fn selected_connection(&self) -> Option<&str> {
        self.selected_connection.as_deref()
    }

    /// Topmost wire within [`WIRE_HIT_RADIUS`] screen pixels of `at` (world).
    pub fn wire_at(&self, at: WorldPos) -> Option<String> {
        let tolerance = WIRE_HIT_RADIUS / self.viewport.k.max(0.05);
        let tolerance_sq = tolerance * tolerance;
        for connection in self.connections.iter().rev() {
            let (Some(from), Some(to)) = (
                self.node(&connection.from_node_id),
                self.node(&connection.to_node_id),
            ) else {
                continue;
            };
            let polyline = connection_polyline(from, to);
            for pair in polyline.windows(2) {
                if segment_distance_sq(Position::new(at.x, at.y), pair[0], pair[1]) <= tolerance_sq
                {
                    return Some(connection.id.clone());
                }
            }
        }
        None
    }

    /// Removes one wire and clears its selection when it held it.
    pub fn delete_connection(&mut self, id: &str, now_ms: u64) -> bool {
        let before = self.connections.len();
        self.connections.retain(|connection| connection.id != id);
        if self.connections.len() == before {
            return false;
        }
        if self.selected_connection.as_deref() == Some(id) {
            self.selected_connection = None;
        }
        self.commit(now_ms);
        true
    }

    /// Deletes every wire the stroke crosses (`connectionCrossesStroke`); used
    /// by the cut tool and returns the number of removed wires.
    pub fn cut_connections(&mut self, stroke: &[WorldPos], now_ms: u64) -> usize {
        if stroke.len() < 2 {
            return 0;
        }
        let stroke: Vec<Position> = stroke
            .iter()
            .map(|point| Position::new(point.x, point.y))
            .collect();
        let doomed: Vec<String> = self
            .connections
            .iter()
            .filter(|connection| {
                let (Some(from), Some(to)) = (
                    self.node(&connection.from_node_id),
                    self.node(&connection.to_node_id),
                ) else {
                    return false;
                };
                connection_crosses_stroke(from, to, &stroke)
            })
            .map(|connection| connection.id.clone())
            .collect();
        if doomed.is_empty() {
            return 0;
        }
        self.connections
            .retain(|connection| !doomed.contains(&connection.id));
        if self
            .selected_connection
            .as_ref()
            .is_some_and(|id| doomed.contains(id))
        {
            self.selected_connection = None;
        }
        self.commit(now_ms);
        doomed.len()
    }

    /// Starts a cut stroke at `at` (Ctrl + right-button down).
    pub fn begin_cut(&mut self, at: WorldPos) {
        self.drag = Some(Drag::Cut { points: vec![at] });
    }

    /// Appends a sampled point; reports whether the stroke grew. Points closer
    /// than [`CUT_MIN_STEP`] are dropped so the polyline stays compact.
    pub fn cut_to(&mut self, at: WorldPos) -> bool {
        let Some(Drag::Cut { points }) = &mut self.drag else {
            return false;
        };
        let last = points.last().copied().unwrap_or(at);
        let dx = at.x - last.x;
        let dy = at.y - last.y;
        if dx * dx + dy * dy < CUT_MIN_STEP * CUT_MIN_STEP {
            return false;
        }
        points.push(at);
        true
    }

    /// The in-flight cut polyline (empty while no cut runs).
    pub fn cut_points(&self) -> &[WorldPos] {
        match &self.drag {
            Some(Drag::Cut { points }) => points,
            _ => &[],
        }
    }

    /// Finishes the cut: crossed wires are deleted and the gesture clears.
    pub fn end_cut(&mut self, now_ms: u64) -> usize {
        let points = match self.drag.take() {
            Some(Drag::Cut { points }) => points,
            other => {
                self.drag = other;
                return 0;
            }
        };
        self.cut_connections(&points, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Viewport;
    use open_canvas_core::node::{
        CanvasConnection, CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position,
    };

    fn node(
        id: &str,
        kind: CanvasNodeType,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(x, y),
            width,
            height,
            metadata: Some(CanvasNodeMetadata::default()),
        }
    }

    fn wire_doc() -> Doc {
        let nodes = vec![
            node("prompt", CanvasNodeType::Prompt, 0.0, 0.0, 340.0, 240.0),
            node("text", CanvasNodeType::Text, 600.0, 0.0, 340.0, 240.0),
        ];
        let connections = vec![CanvasConnection {
            id: "conn-1".into(),
            from_node_id: "prompt".into(),
            to_node_id: "text".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }];
        Doc::assemble(nodes, connections)
    }

    #[test]
    fn wire_hit_test_finds_the_curve_and_misses_far_points() {
        let doc = wire_doc();
        // The edge leaves the prompt's right-mid (340, 120) and enters the text's
        // left-mid (600, 120); its midpoint is (470, 120).
        assert_eq!(
            doc.wire_at(WorldPos::new(470.0, 120.0)).as_deref(),
            Some("conn-1")
        );
        assert_eq!(doc.wire_at(WorldPos::new(470.0, 300.0)), None);
        assert_eq!(doc.wire_at(WorldPos::new(50.0, 50.0)), None);
    }

    #[test]
    fn hit_tolerance_scales_with_zoom() {
        let mut doc = wire_doc();
        doc.viewport = Viewport {
            x: 0.0,
            y: 0.0,
            k: 0.25,
        };
        // At k = 0.25 the world tolerance is 32; a point 20 world units away hits.
        assert_eq!(
            doc.wire_at(WorldPos::new(470.0, 140.0)).as_deref(),
            Some("conn-1")
        );
        assert_eq!(doc.wire_at(WorldPos::new(470.0, 180.0)), None);
    }

    #[test]
    fn select_and_delete_removes_only_the_target_wire() {
        let mut doc = wire_doc();
        doc.select_connection(Some("conn-1"));
        assert_eq!(doc.selected_connection(), Some("conn-1"));
        assert!(doc.delete_connection("conn-1", 0));
        assert_eq!(doc.connections().len(), 0);
        assert_eq!(
            doc.selected_connection(),
            None,
            "selection is cleared with the wire"
        );
        assert!(
            !doc.delete_connection("conn-1", 0),
            "second delete is a no-op"
        );
    }

    #[test]
    fn cut_stroke_deletes_crossed_wires_only() {
        let mut doc = wire_doc();
        let stroke = vec![WorldPos::new(470.0, -100.0), WorldPos::new(470.0, 400.0)];
        assert_eq!(doc.cut_connections(&stroke, 0), 1);
        assert_eq!(doc.connections().len(), 0);

        let mut doc = wire_doc();
        let miss = vec![WorldPos::new(2000.0, 0.0), WorldPos::new(2000.0, 500.0)];
        assert_eq!(doc.cut_connections(&miss, 0), 0);
        assert_eq!(doc.connections().len(), 1);
        assert_eq!(doc.cut_connections(&[], 0), 0, "a single point never cuts");
    }

    #[test]
    fn cut_gesture_samples_points_and_reports_removals() {
        let mut doc = wire_doc();
        doc.begin_cut(WorldPos::new(470.0, -100.0));
        assert!(
            !doc.cut_to(WorldPos::new(470.0, -100.1)),
            "sub-step moves are dropped"
        );
        assert!(doc.cut_to(WorldPos::new(470.0, 400.0)));
        assert_eq!(doc.cut_points().len(), 2);
        assert_eq!(doc.end_cut(0), 1);
        assert_eq!(doc.cut_points().len(), 0);
        assert_eq!(doc.connections().len(), 0);
    }
}
