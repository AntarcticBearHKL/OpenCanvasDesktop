//! Screen-space overlays: the active connection while dragging, the marquee
//! border and the dash-pattern helpers. Their dash units are screen-constant
//! (the web's `strokeDasharray "5,5"` and `6/k 4/k`), which is why they are
//! built in screen coordinates even though their anchors come from world space.

use open_canvas_core::node::{CanvasNodeData, ConnectionHandleType};

use crate::coords::{Viewport, WorldPos};
use crate::doc::Drag;

/// Marquee dash pattern in world units (project.tsx:1984): `6/k 4/k`.
const MARQUEE_DASH: (f64, f64) = (6.0, 4.0);
/// Active-connection dash pattern (`strokeDasharray "5,5"`, canvas-connections.tsx:98).
const ACTIVE_WIRE_DASH: (f64, f64) = (5.0, 5.0);
/// Cubic-bezier sampling for dashed curves.
const DASH_SAMPLES: u16 = 48;
/// Segments per rounded-corner arc in the selection border.
const CORNER_SEGMENTS: f64 = 6.0;

/// The dashed bezier shown while dragging a connection
/// (canvas-connections.tsx:80-98): start/end swap with the handle side, the
/// control points use `distance * 0.5`, and the dash pattern is screen-constant.
pub fn active_wire_commands(
    drag: Option<&Drag>,
    nodes: &[CanvasNodeData],
    viewport: Viewport,
) -> String {
    let Some(Drag::Connect {
        node_id,
        handle,
        cursor,
        target,
        ..
    }) = drag
    else {
        return String::new();
    };
    let Some(node) = nodes.iter().find(|n| n.id == *node_id) else {
        return String::new();
    };
    let Some(cursor) = cursor else {
        return String::new();
    };
    let cursor = *cursor;
    let target_node = target
        .as_ref()
        .and_then(|id| nodes.iter().find(|n| n.id == *id));
    let right_mid = WorldPos::new(
        node.position.x + node.width,
        node.position.y + node.height / 2.0,
    );
    let left_mid = WorldPos::new(node.position.x, node.position.y + node.height / 2.0);
    let (mut start, mut end) = match handle {
        ConnectionHandleType::Source => (right_mid, cursor),
        ConnectionHandleType::Target => (cursor, left_mid),
    };
    // Only the dropping end snaps to a target (canvas-connections.tsx:86-92).
    if let Some(target_node) = target_node {
        let target_right = WorldPos::new(
            target_node.position.x + target_node.width,
            target_node.position.y + target_node.height / 2.0,
        );
        let target_left = WorldPos::new(
            target_node.position.x,
            target_node.position.y + target_node.height / 2.0,
        );
        match handle {
            ConnectionHandleType::Source => end = target_left,
            ConnectionHandleType::Target => start = target_right,
        }
    }
    let distance = (end.x - start.x).abs();
    let c1 = WorldPos::new(start.x + distance * 0.5, start.y);
    let c2 = WorldPos::new(end.x - distance * 0.5, end.y);
    let mut points = Vec::with_capacity(usize::from(DASH_SAMPLES) + 1);
    let step = 1.0 / f64::from(DASH_SAMPLES);
    let mut t = 0.0;
    while t <= 1.0 + f64::EPSILON {
        let point = cubic_point(start, c1, c2, end, t.min(1.0));
        let screen = viewport.world_to_screen(point);
        points.push((screen.x, screen.y));
        t += step;
    }
    let mut out = String::new();
    dashed_polyline(&points, ACTIVE_WIRE_DASH, &mut out);
    out
}

#[expect(
    clippy::many_single_char_names,
    reason = "c1/c2/t are the conventional names of bezier control points and parameters"
)]
fn cubic_point(p0: WorldPos, p1: WorldPos, p2: WorldPos, p3: WorldPos, t: f64) -> WorldPos {
    let inv = 1.0 - t;
    let a = inv * inv * inv;
    let b = 3.0 * inv * inv * t;
    let c = 3.0 * inv * t * t;
    let d = t * t * t;
    WorldPos::new(
        a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    )
}

/// Dashed rectangle border in screen coordinates (marquee, project.tsx:1984).
pub fn marquee_border_commands(rect: Option<(f64, f64, f64, f64)>, viewport: Viewport) -> String {
    let Some((world_x, world_y, world_w, world_h)) = rect else {
        return String::new();
    };
    let start = viewport.world_to_screen(WorldPos::new(world_x, world_y));
    let end = viewport.world_to_screen(WorldPos::new(world_x + world_w, world_y + world_h));
    let points = [
        (start.x, start.y),
        (end.x, start.y),
        (end.x, end.y),
        (start.x, end.y),
        (start.x, start.y),
    ];
    let mut out = String::new();
    dashed_polyline(&points, MARQUEE_DASH, &mut out);
    out
}

/// World marquee rect mapped to a screen rectangle `(x, y, w, h)`.
pub fn screen_rect(rect: (f64, f64, f64, f64), viewport: Viewport) -> (f64, f64, f64, f64) {
    let start = viewport.world_to_screen(WorldPos::new(rect.0, rect.1));
    (start.x, start.y, rect.2 * viewport.k, rect.3 * viewport.k)
}

/// The cut-tool stroke as a screen-space polyline (project.tsx:1911-1912: solid
/// `selectionStroke`, 2px screen width, round caps).
#[expect(
    clippy::format_push_string,
    reason = "writing into a String is infallible; the lint config forbids discarding fmt::Write's Result"
)]
pub fn cut_stroke_commands(points: &[WorldPos], viewport: Viewport) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut out = String::new();
    for (index, point) in points.iter().enumerate() {
        let screen = viewport.world_to_screen(*point);
        out.push_str(&format!(
            "{} {} {} ",
            if index == 0 { "M" } else { "L" },
            screen.x,
            screen.y
        ));
    }
    out
}

#[expect(
    clippy::format_push_string,
    reason = "writing into a String is infallible; fmt::Write's Result would have to be discarded, which the lint config forbids"
)]
pub fn dashed_polyline(points: &[(f64, f64)], (on, off): (f64, f64), out: &mut String) {
    let mut drawing = true;
    let mut phase = 0.0;
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        if length <= f64::EPSILON {
            continue;
        }
        let mut travelled = 0.0;
        while travelled < length {
            let remaining = if drawing { on } else { off } - phase;
            let step = remaining.min(length - travelled);
            if drawing {
                let t0 = travelled / length;
                let t1 = (travelled + step) / length;
                let (sx, sy) = (x0 + (x1 - x0) * t0, y0 + (y1 - y0) * t0);
                let (ex, ey) = (x0 + (x1 - x0) * t1, y0 + (y1 - y0) * t1);
                out.push_str(&format!("M {sx} {sy} L {ex} {ey} "));
            }
            travelled += step;
            phase += step;
            let limit = if drawing { on } else { off };
            if phase >= limit - 1e-9 {
                drawing = !drawing;
                phase = 0.0;
            }
        }
    }
}

/// Dashed rounded rectangle (canvas-selection-toolbar.tsx:78-91: rx/ry 16,
/// `strokeDasharray 7 5`). The perimeter is sampled — straight edges plus eight
/// segments per corner — and dashed with the shared walker.
pub fn rounded_border_commands(
    rect: (f64, f64, f64, f64),
    radius: f64,
    dash: (f64, f64),
    viewport: Viewport,
) -> String {
    let start = viewport.world_to_screen(WorldPos::new(rect.0, rect.1));
    let end = viewport.world_to_screen(WorldPos::new(rect.0 + rect.2, rect.1 + rect.3));
    let radius = radius
        .min((end.x - start.x) / 2.0)
        .min((end.y - start.y) / 2.0);
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(80);
    let corners = [
        (end.x - radius, start.y + radius, -90.0_f64),
        (end.x - radius, end.y - radius, 0.0),
        (start.x + radius, end.y - radius, 90.0),
        (start.x + radius, start.y + radius, 180.0),
    ];
    let straight = [
        (start.x + radius, start.y),
        (end.x - radius, start.y),
        (end.x, start.y + radius),
        (end.x, end.y - radius),
        (end.x - radius, end.y),
        (start.x + radius, end.y),
        (start.x, end.y - radius),
        (start.x, start.y + radius),
    ];
    points.push((straight[0].0, straight[0].1));
    for (index, (cx, cy, degrees)) in corners.iter().enumerate() {
        let (next_x, next_y) = straight[index * 2 + 1];
        points.push((next_x, next_y));
        let mut step = 0.0;
        while step <= CORNER_SEGMENTS {
            let angle = (degrees + 90.0 * step / CORNER_SEGMENTS).to_radians();
            points.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
            step += 1.0;
        }
        let (after_x, after_y) = straight[(index * 2 + 2) % straight.len()];
        points.push((after_x, after_y));
    }
    points.push((straight[0].0, straight[0].1));
    let mut out = String::new();
    dashed_polyline(&points, dash, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::{CanvasNodeType, Position};

    #[test]
    fn dashed_line_alternates_on_and_off_segments() {
        let mut out = String::new();
        dashed_polyline(&[(0.0, 0.0), (25.0, 0.0)], (5.0, 5.0), &mut out);
        assert_eq!(out, "M 0 0 L 5 0 M 10 0 L 15 0 M 20 0 L 25 0 ");
    }

    #[test]
    fn active_wire_starts_at_the_source_right_edge() {
        let nodes = vec![CanvasNodeData {
            id: "a".into(),
            node_type: CanvasNodeType::Text,
            title: "a".into(),
            position: Position::new(0.0, 0.0),
            width: 100.0,
            height: 50.0,
            metadata: None,
        }];
        let drag = Drag::Connect {
            node_id: "a".into(),
            handle: ConnectionHandleType::Source,
            cursor: Some(WorldPos::new(200.0, 25.0)),
            target: None,
            reject: None,
        };
        let commands = active_wire_commands(Some(&drag), &nodes, Viewport::new());
        assert!(commands.starts_with("M 100 25 "));
    }
}
