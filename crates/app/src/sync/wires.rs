//! Wire sync: connection paths and the selected-wire overlay.
//!
//! `ui.wires` is a single path command string, so a node drag cannot patch one
//! command in the window. [`crate::app::WireCache`] keeps every connection's
//! segment in document order; during a drag only the segments touching a
//! dragged node are regenerated (a handful) and the string is joined.

use slint::SharedString;

use open_canvas_core::node::{CanvasConnection, CanvasNodeData};

use crate::app::{App, WireCache};
use crate::coords::WorldPos;
use crate::paths;
use crate::AppWindow;

/// One `id -> node` map for a whole pass (first occurrence wins, the same rule
/// as `vm::NodeIndex` and the `iter().find` it replaces).
fn index_nodes(nodes: &[CanvasNodeData]) -> std::collections::HashMap<&str, &CanvasNodeData> {
    let mut by_id = std::collections::HashMap::with_capacity(nodes.len());
    for node in nodes {
        by_id.entry(node.id.as_str()).or_insert(node);
    }
    by_id
}

/// The path of one wire, or an empty segment when an endpoint is gone (the
/// full string skips those connections, exactly like `paths::wires_commands`).
fn segment(
    by_id: &std::collections::HashMap<&str, &CanvasNodeData>,
    connection: &CanvasConnection,
) -> String {
    let (Some(from), Some(to)) = (
        by_id.get(connection.from_node_id.as_str()),
        by_id.get(connection.to_node_id.as_str()),
    ) else {
        return String::new();
    };
    paths::wire_command(from, to)
}

/// True when the wire touches one of the dirty (dragged) node ids.
fn touches(ids: &[String], connection: &CanvasConnection) -> bool {
    ids.iter().any(|id| {
        connection.from_node_id.as_str() == id.as_str()
            || connection.to_node_id.as_str() == id.as_str()
    })
}

impl WireCache {
    /// Replaces the cache with a full build for `connections`.
    fn rebuild(&mut self, nodes: &[CanvasNodeData], connections: &[CanvasConnection]) {
        let by_id = index_nodes(nodes);
        self.connection_ids = connections
            .iter()
            .map(|connection| connection.id.clone())
            .collect();
        self.segments = connections
            .iter()
            .map(|connection| (connection.id.clone(), segment(&by_id, connection)))
            .collect();
    }

    /// Regenerates only the segments touching `dirty`, returning how many were
    /// rebuilt. A cache that does not match the current connections rebuilds
    /// fully (the first frame of a gesture after a document swap).
    fn refresh(
        &mut self,
        nodes: &[CanvasNodeData],
        connections: &[CanvasConnection],
        dirty: &[String],
    ) -> usize {
        if !self.matches(connections) {
            self.rebuild(nodes, connections);
            return connections.len();
        }
        let by_id = index_nodes(nodes);
        let mut rebuilt = 0;
        for (index, connection) in connections.iter().enumerate() {
            if !touches(dirty, connection) {
                continue;
            }
            if let Some(path) = self.segments.get_mut(index).map(|entry| &mut entry.1) {
                *path = segment(&by_id, connection);
                rebuilt += 1;
            }
        }
        rebuilt
    }

    /// The concatenated path string, byte-identical to
    /// `paths::wires_commands` (missing endpoints are skipped).
    fn commands(&self) -> String {
        let mut out = String::new();
        for (_, path) in &self.segments {
            if path.is_empty() {
                continue;
            }
            out.push_str(path);
            out.push(' ');
        }
        out
    }
}

impl App {
    /// Full wire refresh (structural changes and the release frame).
    pub fn sync_wires(&mut self, ui: &AppWindow) {
        self.wire_cache
            .rebuild(self.doc.nodes(), self.doc.connections());
        self.drag_stats.wire_paths = self
            .drag_stats
            .wire_paths
            .saturating_add(self.wire_cache.segments.len() as u64);
        self.push_wires(ui);
        self.sync_selected_wire(ui);
    }

    /// Incremental wire refresh for a live node drag: only the paths touching
    /// `ids` are regenerated.
    pub fn sync_drag_wires(&mut self, ui: &AppWindow, ids: &[String]) {
        // Starting a node drag selects the node, which clears the wire
        // selection; the selected-wire overlay must not linger on the canvas
        // until the release sync. Only the empty value can need writing here.
        if self.doc.selected_connection().is_none() && !ui.get_selected_wire().is_empty() {
            ui.set_selected_wire(SharedString::default());
        }
        let rebuilt = self
            .wire_cache
            .refresh(self.doc.nodes(), self.doc.connections(), ids);
        self.drag_stats.wire_paths = self.drag_stats.wire_paths.saturating_add(rebuilt as u64);
        if rebuilt > 0 {
            self.push_wires(ui);
        }
    }

    /// Pushes the cached wire string and counts the frame's one model write.
    fn push_wires(&mut self, ui: &AppWindow) {
        ui.set_wires(self.wire_cache.commands().into());
        self.drag_stats.wire_pushes = self.drag_stats.wire_pushes.saturating_add(1);
    }

    /// The selected wire's path (empty when nothing is selected).
    pub fn sync_selected_wire(&self, ui: &AppWindow) {
        let nodes = self.doc.nodes();
        let connections = self.doc.connections();
        let selected = self
            .doc
            .selected_connection()
            .and_then(|id| connections.iter().find(|connection| connection.id == id));
        let selected_wire = selected
            .and_then(|connection| {
                let from = nodes
                    .iter()
                    .find(|node| node.id == connection.from_node_id)?;
                let to = nodes.iter().find(|node| node.id == connection.to_node_id)?;
                Some(paths::wire_command(from, to))
            })
            .unwrap_or_default();
        ui.set_selected_wire(selected_wire.into());

        let explanation = selected.and_then(|connection| {
            let from = nodes
                .iter()
                .find(|node| node.id == connection.from_node_id)?;
            let to = nodes.iter().find(|node| node.id == connection.to_node_id)?;
            let mut parts = Vec::new();
            if let Some(reason) = connection.reason.as_deref() {
                parts.push(reason.to_string());
            } else if let Some(kind) = connection.kind.as_deref() {
                parts.push(kind.to_string());
            } else if let Some(relation) = connection.relation.as_deref() {
                parts.push(relation.to_string());
            }
            if let Some(changes) = connection.changes.as_deref() {
                parts.push(format!("· {changes}"));
            }
            if parts.is_empty() {
                return None;
            }
            let polyline = open_canvas_core::connections::connection_polyline(from, to);
            let mid = polyline
                .get(polyline.len() / 2)
                .map(|point| WorldPos::new(point.x, point.y))
                .unwrap_or_else(|| {
                    WorldPos::new(
                        (from.position.x + to.position.x) / 2.0,
                        (from.position.y + to.position.y) / 2.0,
                    )
                });
            let screen = self.doc.viewport().world_to_screen(mid);
            Some((parts.join(" "), screen.x as f32, screen.y as f32))
        });
        if let Some((text, x, y)) = explanation {
            ui.set_edge_reason_visible(true);
            ui.set_edge_reason_text(text.into());
            ui.set_edge_reason_x(x);
            ui.set_edge_reason_y(y);
        } else {
            ui.set_edge_reason_visible(false);
            ui.set_edge_reason_text("".into());
        }
    }
}
