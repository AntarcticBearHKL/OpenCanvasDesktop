//! Connection helpers, mirroring `web/src/lib/canvas/canvas-connections.ts` and the
//! connection part of `canvas-node-geometry.ts`.

use crate::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeType, ConnectionHandleType, Position,
};

/// Number of cubic-bezier samples used by the edge-cut hit test
/// (canvas-connections.ts:5).
pub const EDGE_SAMPLE_SEGMENTS: usize = 24;

/// Relation key inferred from the source node type
/// (`SOURCE_RELATIONS`, canvas-connections.ts:7-13).
pub fn source_relation(node_type: &CanvasNodeType) -> Option<&'static str> {
    match node_type {
        CanvasNodeType::Text | CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt => {
            Some("prompt")
        }
        CanvasNodeType::Video => Some("video-reference"),
        CanvasNodeType::Image => Some("reference"),
        _ => None,
    }
}

/// `inferConnectionRelation` (canvas-connections.ts:21-23) returns `undefined`, so
/// stored relations are always derived rather than inferred from node types.
pub fn infer_connection_relation(_from: &CanvasNodeData, _to: &CanvasNodeData) -> Option<String> {
    None
}

/// Resolve the relation key used for a connection label: the stored relation if set,
/// else the source-type relation, else `"linked"`.
pub fn connection_relation(connection: &CanvasConnection, from: &CanvasNodeData) -> String {
    connection
        .relation
        .clone()
        .or_else(|| source_relation(&from.node_type).map(str::to_string))
        .unwrap_or_else(|| "linked".to_string())
}

/// Cubic-bezier edge geometry from `from`'s right-mid to `to`'s left-mid.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionGeometry {
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub curvature: f64,
    pub path_d: String,
}

/// `connectionGeometry` (canvas-connections.ts:25-32): curvature `max(|dx| * 0.5, 50)`.
pub fn connection_geometry(from: &CanvasNodeData, to: &CanvasNodeData) -> ConnectionGeometry {
    let start_x = from.position.x + from.width;
    let start_y = from.position.y + from.height / 2.0;
    let end_x = to.position.x;
    let end_y = to.position.y + to.height / 2.0;
    let curvature = ((end_x - start_x).abs() * 0.5).max(50.0);
    let path_d = format!(
        "M {start_x} {start_y} C {} {start_y}, {} {end_y}, {end_x} {end_y}",
        start_x + curvature,
        end_x - curvature
    );
    ConnectionGeometry {
        start_x,
        start_y,
        end_x,
        end_y,
        curvature,
        path_d,
    }
}

/// The sampled polyline of an edge: `EDGE_SAMPLE_SEGMENTS + 1` points
/// (canvas-connections.ts:45-61).
pub fn connection_polyline(from: &CanvasNodeData, to: &CanvasNodeData) -> Vec<Position> {
    let geometry = connection_geometry(from, to);
    (0..=EDGE_SAMPLE_SEGMENTS)
        .map(|i| {
            let t = i as f64 / EDGE_SAMPLE_SEGMENTS as f64;
            let inv = 1.0 - t;
            let a = inv * inv * inv;
            let b = 3.0 * inv * inv * t;
            let c = 3.0 * inv * t * t;
            let d = t * t * t;
            Position::new(
                a * geometry.start_x
                    + b * (geometry.start_x + geometry.curvature)
                    + c * (geometry.end_x - geometry.curvature)
                    + d * geometry.end_x,
                a * geometry.start_y
                    + b * geometry.start_y
                    + c * geometry.end_y
                    + d * geometry.end_y,
            )
        })
        .collect()
}

/// True when the edge crosses any segment of the freehand stroke
/// (canvas-connections.ts:34-43).
pub fn connection_crosses_stroke(
    from: &CanvasNodeData,
    to: &CanvasNodeData,
    stroke: &[Position],
) -> bool {
    if stroke.len() < 2 {
        return false;
    }
    let edge = connection_polyline(from, to);
    for i in 1..stroke.len() {
        for j in 1..edge.len() {
            if segments_cross(stroke[i - 1], stroke[i], edge[j - 1], edge[j]) {
                return true;
            }
        }
    }
    false
}

fn segments_cross(a: Position, b: Position, c: Position, d: Position) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    if o1 != o2 && o3 != o4 {
        return true;
    }
    (o1 == 0 && on_segment(a, b, c))
        || (o2 == 0 && on_segment(a, b, d))
        || (o3 == 0 && on_segment(c, d, a))
        || (o4 == 0 && on_segment(c, d, b))
}

fn orientation(a: Position, b: Position, c: Position) -> i8 {
    let value = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if value.abs() < 1e-9 {
        return 0;
    }
    if value > 0.0 {
        1
    } else {
        -1
    }
}

fn on_segment(a: Position, b: Position, p: Position) -> bool {
    p.x >= a.x.min(b.x) - 1e-9
        && p.x <= a.x.max(b.x) + 1e-9
        && p.y >= a.y.min(b.y) - 1e-9
        && p.y <= a.y.max(b.y) + 1e-9
}

fn prompt_target(node_type: &CanvasNodeType) -> Option<CanvasNodeType> {
    match node_type {
        CanvasNodeType::Prompt => Some(CanvasNodeType::ImageGeneration),
        CanvasNodeType::VideoPrompt => Some(CanvasNodeType::VideoGeneration),
        _ => None,
    }
}

fn generator_prompt(node_type: &CanvasNodeType) -> Option<CanvasNodeType> {
    match node_type {
        CanvasNodeType::ImageGeneration => Some(CanvasNodeType::Prompt),
        CanvasNodeType::VideoGeneration => Some(CanvasNodeType::VideoPrompt),
        _ => None,
    }
}

/// `isPromptConnectionAllowed` (canvas-node-geometry.ts:113-119): prompt nodes only
/// connect to their matching generator, and a generator only to its matching prompt.
pub fn is_prompt_connection_allowed(from: &CanvasNodeType, to: &CanvasNodeType) -> bool {
    if prompt_target(from).is_some() {
        return match generator_prompt(to) {
            Some(expected_prompt) => expected_prompt == *from,
            None => prompt_target(to).is_none(),
        };
    }
    if prompt_target(to).is_some() {
        return match generator_prompt(from) {
            Some(expected_prompt) => expected_prompt == *to,
            None => true,
        };
    }
    true
}

/// A normalized (oriented) connection before an id is assigned.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedConnection {
    pub from_node_id: String,
    pub to_node_id: String,
    pub relation: Option<String>,
}

/// True when `node_type` may be the source (right handle) of a new connection.
///
/// An Image is a display-only card that can still feed a consumer: its right
/// handle may start a wire to a prompt reference or another downstream node.
/// An Image Stack and a Smart Canvas own their content and start no wire.
pub fn accepts_source_connections(node_type: &CanvasNodeType) -> bool {
    !matches!(
        node_type,
        CanvasNodeType::ImageStack | CanvasNodeType::SmartCanvas | CanvasNodeType::ImageAttempt
    )
}

/// True when `node_type` may be the target (left handle) of a new connection.
///
/// Display-only media never accept an incoming wire: an Image is produced by a
/// generator, an Image Stack collects images, and a Smart Canvas owns a
/// project, so none of them renders a target handle.
pub fn accepts_target_connections(node_type: &CanvasNodeType) -> bool {
    !matches!(
        node_type,
        CanvasNodeType::Image | CanvasNodeType::ImageStack | CanvasNodeType::SmartCanvas
    )
}

/// True when a drag may start on `handle` of `node_type`.
pub fn accepts_handle(node_type: &CanvasNodeType, handle: ConnectionHandleType) -> bool {
    match handle {
        ConnectionHandleType::Source => accepts_source_connections(node_type),
        ConnectionHandleType::Target => accepts_target_connections(node_type),
    }
}

/// `normalizeConnection` (canvas-node-geometry.ts:121-134): rejects
/// self-connections, generation-to-generation, a wire whose target end cannot
/// receive one, and any pair the prompt-pairing rule refuses; orients
/// generation nodes as sinks.
///
/// The two ends are asymmetric: an Image may start a wire but never receive
/// one, while an Image Stack / Smart Canvas participates on neither side.
pub fn normalize_connection(
    first_node_id: &str,
    second_node_id: &str,
    nodes: &[CanvasNodeData],
    first_handle_type: ConnectionHandleType,
) -> Option<NormalizedConnection> {
    let first = nodes.iter().find(|node| node.id == first_node_id)?;
    let second = nodes.iter().find(|node| node.id == second_node_id)?;
    if first.id == second.id {
        return None;
    }
    let attempt_pair = first.node_type == CanvasNodeType::ImageAttempt
        && second.node_type == CanvasNodeType::ImageAttempt;
    if first.node_type.is_generation() && second.node_type.is_generation() && !attempt_pair {
        return None;
    }
    // The handle the drag started from fixes that node's role: a target-handle
    // drag from a node that cannot receive is refused even when the
    // orientation below would make it the source instead.
    if !accepts_handle(&first.node_type, first_handle_type) {
        return None;
    }
    let to_second = second.node_type.is_generation()
        || !first.node_type.is_generation()
        || first_handle_type == ConnectionHandleType::Source;
    let (from_node, to_node) = if to_second {
        (first, second)
    } else {
        (second, first)
    };
    if !accepts_source_connections(&from_node.node_type)
        || !accepts_target_connections(&to_node.node_type)
    {
        return None;
    }
    if !is_prompt_connection_allowed(&from_node.node_type, &to_node.node_type) {
        return None;
    }
    Some(NormalizedConnection {
        from_node_id: from_node.id.clone(),
        to_node_id: to_node.id.clone(),
        relation: infer_connection_relation(from_node, to_node),
    })
}

/// True when a directed connection already exists (`connectNodes` de-dup,
/// project.tsx:487-503).
pub fn has_connection(
    connections: &[CanvasConnection],
    from_node_id: &str,
    to_node_id: &str,
) -> bool {
    connections
        .iter()
        .any(|c| c.from_node_id == from_node_id && c.to_node_id == to_node_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::CanvasNodeData;

    fn node(id: &str, node_type: CanvasNodeType, x: f64, y: f64, w: f64, h: f64) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type,
            title: id.to_string(),
            position: Position::new(x, y),
            width: w,
            height: h,
            metadata: None,
        }
    }

    #[test]
    fn geometry_goes_right_mid_to_left_mid_with_min_curvature() {
        let from = node("a", CanvasNodeType::Prompt, 0.0, 0.0, 100.0, 50.0);
        let to = node(
            "b",
            CanvasNodeType::ImageGeneration,
            300.0,
            100.0,
            80.0,
            40.0,
        );
        let geometry = connection_geometry(&from, &to);
        assert_eq!(geometry.start_x, 100.0);
        assert_eq!(geometry.start_y, 25.0);
        assert_eq!(geometry.end_x, 300.0);
        assert_eq!(geometry.end_y, 120.0);
        assert_eq!(geometry.curvature, 100.0);
        assert_eq!(geometry.path_d, "M 100 25 C 200 25, 200 120, 300 120");
    }

    #[test]
    fn curvature_floors_at_fifty_for_close_nodes() {
        let from = node("a", CanvasNodeType::Prompt, 0.0, 0.0, 100.0, 50.0);
        let to = node("b", CanvasNodeType::ImageGeneration, 110.0, 0.0, 80.0, 40.0);
        // |dx| = |110 - 100| = 10 -> 10 * 0.5 = 5 -> floor 50.
        assert_eq!(connection_geometry(&from, &to).curvature, 50.0);
    }

    #[test]
    fn polyline_has_twenty_five_samples() {
        let from = node("a", CanvasNodeType::Prompt, 0.0, 0.0, 100.0, 50.0);
        let to = node(
            "b",
            CanvasNodeType::ImageGeneration,
            300.0,
            100.0,
            80.0,
            40.0,
        );
        assert_eq!(connection_polyline(&from, &to).len(), 25);
    }

    #[test]
    fn crossing_detects_a_stroke_through_the_edge() {
        let from = node("a", CanvasNodeType::Prompt, 0.0, 0.0, 100.0, 50.0);
        let to = node(
            "b",
            CanvasNodeType::ImageGeneration,
            300.0,
            100.0,
            80.0,
            40.0,
        );
        // The drawn edge passes the midpoint (200, 72.5); this vertical stroke cuts it.
        let stroke = vec![Position::new(200.0, -50.0), Position::new(200.0, 300.0)];
        assert!(connection_crosses_stroke(&from, &to, &stroke));
    }

    #[test]
    fn crossing_ignores_a_stroke_that_misses() {
        let from = node("a", CanvasNodeType::Prompt, 0.0, 0.0, 100.0, 50.0);
        let to = node(
            "b",
            CanvasNodeType::ImageGeneration,
            300.0,
            100.0,
            80.0,
            40.0,
        );
        let stroke = vec![Position::new(1000.0, 0.0), Position::new(1000.0, 500.0)];
        assert!(!connection_crosses_stroke(&from, &to, &stroke));
        assert!(!connection_crosses_stroke(
            &from,
            &to,
            &[Position::new(1.0, 1.0)]
        ));
    }

    #[test]
    fn relation_prefers_stored_then_source_type() {
        let text = node("t", CanvasNodeType::Text, 0.0, 0.0, 10.0, 10.0);
        let stored = CanvasConnection {
            id: "c".into(),
            from_node_id: "t".into(),
            to_node_id: "g".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        };
        assert_eq!(connection_relation(&stored, &text), "prompt");

        let video = node("v", CanvasNodeType::Video, 0.0, 0.0, 10.0, 10.0);
        assert_eq!(connection_relation(&stored, &video), "video-reference");

        let assets = node("s", CanvasNodeType::Assets, 0.0, 0.0, 10.0, 10.0);
        assert_eq!(connection_relation(&stored, &assets), "linked");

        let explicit = CanvasConnection {
            relation: Some("custom".into()),
            ..stored.clone()
        };
        assert_eq!(connection_relation(&explicit, &assets), "custom");
    }

    fn sample_nodes() -> Vec<CanvasNodeData> {
        vec![
            node("p", CanvasNodeType::Prompt, 0.0, 0.0, 340.0, 240.0),
            node(
                "g",
                CanvasNodeType::ImageGeneration,
                500.0,
                0.0,
                412.0,
                576.0,
            ),
            node("vp", CanvasNodeType::VideoPrompt, 0.0, 500.0, 340.0, 300.0),
            node(
                "vg",
                CanvasNodeType::VideoGeneration,
                500.0,
                500.0,
                412.0,
                576.0,
            ),
            node("img", CanvasNodeType::Image, 1000.0, 0.0, 340.0, 240.0),
            node("txt", CanvasNodeType::Text, 1000.0, 500.0, 340.0, 240.0),
        ]
    }

    #[test]
    fn normalize_rejects_self_and_generation_to_generation() {
        let nodes = sample_nodes();
        assert_eq!(
            normalize_connection("p", "p", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("g", "vg", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("g", "vg", &nodes, ConnectionHandleType::Target),
            None
        );
    }

    /// A wire into an image is refused whatever starts it: its left side is
    /// never a drop target.
    #[test]
    fn normalize_rejects_anything_targeting_an_image() {
        let nodes = sample_nodes();
        assert_eq!(
            normalize_connection("p", "img", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("txt", "img", &nodes, ConnectionHandleType::Source),
            None
        );
        // A target-handle drag that lands on an image is the same rejection.
        assert_eq!(
            normalize_connection("img", "p", &nodes, ConnectionHandleType::Target),
            None
        );
    }

    /// The image rules are asymmetric: an image may start a wire (a prompt
    /// reference or a downstream consumer) but never receive one.
    #[test]
    fn image_sources_to_a_prompt_and_to_a_generator() {
        let nodes = sample_nodes();
        assert!(accepts_source_connections(&CanvasNodeType::Image));
        assert!(!accepts_target_connections(&CanvasNodeType::Image));
        assert!(accepts_handle(
            &CanvasNodeType::Image,
            ConnectionHandleType::Source
        ));
        assert!(!accepts_handle(
            &CanvasNodeType::Image,
            ConnectionHandleType::Target
        ));

        let to_prompt =
            normalize_connection("img", "p", &nodes, ConnectionHandleType::Source).unwrap();
        assert_eq!(to_prompt.from_node_id, "img");
        assert_eq!(to_prompt.to_node_id, "p");

        let to_generation =
            normalize_connection("img", "g", &nodes, ConnectionHandleType::Source).unwrap();
        assert_eq!(to_generation.from_node_id, "img");
        assert_eq!(to_generation.to_node_id, "g");
    }

    /// An Image Stack owns its images: it neither sources nor receives a wire.
    #[test]
    fn image_stack_nodes_refuse_connections_in_both_directions() {
        let mut nodes = sample_nodes();
        nodes.push(node(
            "stack",
            CanvasNodeType::ImageStack,
            1400.0,
            0.0,
            420.0,
            320.0,
        ));
        assert!(!accepts_source_connections(&CanvasNodeType::ImageStack));
        assert!(!accepts_target_connections(&CanvasNodeType::ImageStack));
        assert!(accepts_source_connections(&CanvasNodeType::Text));

        assert_eq!(
            normalize_connection("stack", "txt", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("p", "stack", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("stack", "p", &nodes, ConnectionHandleType::Target),
            None
        );
    }

    /// A Smart Canvas card owns a project and is display-only, so it rejects a
    /// wire on either side.
    #[test]
    fn smart_canvas_nodes_refuse_connections_in_both_directions() {
        let mut nodes = sample_nodes();
        nodes.push(node(
            "smart",
            CanvasNodeType::SmartCanvas,
            1400.0,
            0.0,
            420.0,
            300.0,
        ));
        assert!(!accepts_source_connections(&CanvasNodeType::SmartCanvas));
        assert!(!accepts_target_connections(&CanvasNodeType::SmartCanvas));
        assert_eq!(
            normalize_connection("smart", "txt", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("smart", "p", &nodes, ConnectionHandleType::Target),
            None
        );
        assert_eq!(
            normalize_connection("txt", "smart", &nodes, ConnectionHandleType::Source),
            None
        );
        assert_eq!(
            normalize_connection("p", "smart", &nodes, ConnectionHandleType::Target),
            None
        );
    }

    #[test]
    fn normalize_accepts_prompt_to_its_generator() {
        let nodes = sample_nodes();
        let result = normalize_connection("p", "g", &nodes, ConnectionHandleType::Source).unwrap();
        assert_eq!(result.from_node_id, "p");
        assert_eq!(result.to_node_id, "g");
        assert_eq!(result.relation, None);
    }

    #[test]
    fn normalize_orients_generation_nodes_as_sinks() {
        let nodes = sample_nodes();
        // Dragging a target handle onto a generation node still produces prompt -> generation.
        let result = normalize_connection("g", "p", &nodes, ConnectionHandleType::Target).unwrap();
        assert_eq!(result.from_node_id, "p");
        assert_eq!(result.to_node_id, "g");
    }

    #[test]
    fn normalize_rejects_mismatched_prompt_generator_pairs() {
        let nodes = sample_nodes();
        // A Prompt (image) cannot target a video generator.
        assert_eq!(
            normalize_connection("p", "vg", &nodes, ConnectionHandleType::Source),
            None
        );
        // A Video Prompt cannot target an image generator.
        assert_eq!(
            normalize_connection("vp", "g", &nodes, ConnectionHandleType::Source),
            None
        );
    }

    #[test]
    fn has_connection_matches_direction() {
        let connections = vec![CanvasConnection {
            id: "c1".into(),
            from_node_id: "p".into(),
            to_node_id: "g".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }];
        assert!(has_connection(&connections, "p", "g"));
        assert!(!has_connection(&connections, "g", "p"));
    }
}
