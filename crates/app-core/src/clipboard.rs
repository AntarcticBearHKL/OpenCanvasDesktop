//! Node copy/paste (`copySelectedNodes`/`pasteCopiedNodes`,
//! use-canvas-document.ts:116-198) plus the desktop-only system-clipboard
//! envelope so a copy can survive a restart or reach another window.
//!
//! The internal clipboard holds the selected nodes and the connections whose
//! *both* endpoints are in the selection; pasting re-mints every node id, adds a
//! `PASTE_OFFSET` world-space displacement, appends `" Copy"` to the titles and
//! remaps the copied connections onto the new ids. The paste commits to the
//! shared undo history as one entry.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use open_canvas_core::document::{from_json, to_json, DocumentError};
use open_canvas_core::node::{CanvasConnection, CanvasNodeData, Position};

use crate::doc::Doc;

/// World-space displacement of a paste (web pastes at the canvas center; the
/// desktop keeps the gesture-relative offset small and predictable).
pub const PASTE_OFFSET: f64 = 24.0;
/// Discriminator of the JSON envelope written to the system clipboard.
pub const CLIPBOARD_APP: &str = "open-canvas-desktop-clipboard";
/// Schema version of the JSON envelope.
pub const CLIPBOARD_VERSION: u32 = 1;

/// The copied payload: the selected nodes plus their internal connections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeClipboard {
    pub nodes: Vec<CanvasNodeData>,
    pub connections: Vec<CanvasConnection>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardEnvelope {
    app: String,
    version: u32,
    nodes: Vec<CanvasNodeData>,
    connections: Vec<CanvasConnection>,
}

/// The web's title suffix rule (`use-canvas-document.ts:161`).
pub fn copy_title(title: &str) -> String {
    if title.ends_with(" Copy") {
        title.to_string()
    } else {
        format!("{title} Copy")
    }
}

/// Serializes a payload for the system clipboard.
pub fn to_clipboard_text(clipboard: &NodeClipboard) -> Result<String, DocumentError> {
    to_json(&ClipboardEnvelope {
        app: CLIPBOARD_APP.to_string(),
        version: CLIPBOARD_VERSION,
        nodes: clipboard.nodes.clone(),
        connections: clipboard.connections.clone(),
    })
}

/// Parses system clipboard text; anything that is not a valid envelope (or that
/// carries no nodes) yields `None` — arbitrary clipboard text never panics.
pub fn from_clipboard_text(text: &str) -> Option<NodeClipboard> {
    let envelope: ClipboardEnvelope = from_json(text).ok()?;
    if envelope.app != CLIPBOARD_APP || envelope.version != CLIPBOARD_VERSION {
        return None;
    }
    if envelope.nodes.is_empty() {
        return None;
    }
    Some(NodeClipboard {
        nodes: envelope.nodes,
        connections: envelope.connections,
    })
}

impl Doc {
    /// Captures the selected nodes and their internal connections; `None` when
    /// nothing is selected.
    pub fn copy_selection(&self) -> Option<NodeClipboard> {
        if self.selection.is_empty() {
            return None;
        }
        let nodes: Vec<CanvasNodeData> = self
            .nodes
            .iter()
            .filter(|node| self.selection.contains(&node.id))
            .cloned()
            .collect();
        if nodes.is_empty() {
            return None;
        }
        let ids: BTreeSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
        let connections = self
            .connections
            .iter()
            .filter(|connection| {
                ids.contains(connection.from_node_id.as_str())
                    && ids.contains(connection.to_node_id.as_str())
            })
            .cloned()
            .collect();
        Some(NodeClipboard { nodes, connections })
    }

    /// Pastes a payload: ids and connection ids are re-minted, titles get the
    /// `" Copy"` suffix, every node moves by [`PASTE_OFFSET`], and the pasted
    /// set becomes the selection. One history entry. Returns the new node ids
    /// in payload order.
    pub fn paste_nodes(&mut self, clipboard: &NodeClipboard, now_ms: u64) -> Vec<String> {
        if clipboard.nodes.is_empty() {
            return Vec::new();
        }
        let mut id_map: HashMap<String, String> = HashMap::with_capacity(clipboard.nodes.len());
        let mut pasted = Vec::with_capacity(clipboard.nodes.len());
        for node in &clipboard.nodes {
            self.seq = self.seq.wrapping_add(1);
            let id = format!("{}-{now_ms}-{}", node.node_type.as_str(), self.seq);
            id_map.insert(node.id.clone(), id.clone());
            let mut copy = node.clone();
            copy.id.clone_from(&id);
            copy.title = copy_title(&node.title);
            copy.position = Position::new(
                node.position.x + PASTE_OFFSET,
                node.position.y + PASTE_OFFSET,
            );
            pasted.push(id);
            self.nodes.push(copy);
        }
        for (index, connection) in clipboard.connections.iter().enumerate() {
            let (Some(from), Some(to)) = (
                id_map.get(&connection.from_node_id),
                id_map.get(&connection.to_node_id),
            ) else {
                continue;
            };
            self.connections.push(CanvasConnection {
                id: format!("conn-{now_ms}-{index}"),
                from_node_id: from.clone(),
                to_node_id: to.clone(),
                relation: connection.relation.clone(),
                kind: connection.kind.clone(),
                reason: connection.reason.clone(),
                changes: connection.changes.clone(),
                agent_run_id: connection.agent_run_id.clone(),
            });
        }
        self.selection = pasted.iter().cloned().collect();
        self.selected_connection = None;
        self.commit(now_ms);
        pasted
    }

    /// The system-clipboard path: parses the envelope and pastes it; `None` when
    /// the text is not a clipboard payload.
    pub fn paste_clipboard_text(&mut self, text: &str, now_ms: u64) -> Option<Vec<String>> {
        let clipboard = from_clipboard_text(text)?;
        Some(self.paste_nodes(&clipboard, now_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::CanvasNodeType;

    fn doc() -> Doc {
        Doc::demo()
    }

    #[test]
    fn copy_collects_only_internal_connections() {
        let mut doc = doc();
        // prompt -> image-generation and prompt -> text both start at `prompt`.
        doc.select_node("prompt", false);
        let clipboard = doc.copy_selection().unwrap_or_default();
        assert_eq!(clipboard.nodes.len(), 1);
        assert!(
            clipboard.connections.is_empty(),
            "no internal connection when only the source node is copied"
        );

        doc.select_node("image", true);
        doc.select_node("text", true);
        let clipboard = doc.copy_selection().unwrap_or_default();
        assert_eq!(clipboard.nodes.len(), 3);
        // The demo fixture wires prompt -> text and image -> text through the
        // selection, so exactly those two connections are internal.
        assert_eq!(clipboard.connections.len(), 2);
        assert!(clipboard.connections.iter().any(
            |connection| connection.from_node_id == "image" && connection.to_node_id == "text"
        ));
        assert!(!clipboard
            .connections
            .iter()
            .any(|connection| connection.from_node_id == "video-prompt"));
    }

    #[test]
    fn copy_without_selection_returns_none() {
        let doc = doc();
        assert_eq!(doc.copy_selection(), None);
    }

    #[test]
    fn paste_remaps_ids_connections_and_offsets_positions() {
        let mut doc = doc();
        doc.select_node("image", false);
        doc.select_node("text", true);
        let clipboard = doc.copy_selection().unwrap_or_default();
        let origin = clipboard
            .nodes
            .iter()
            .find(|node| node.id == "image")
            .map(|node| (node.position.x, node.position.y))
            .unwrap_or_default();

        let pasted = doc.paste_nodes(&clipboard, 1_000);
        assert_eq!(pasted.len(), 2);
        for id in &pasted {
            assert!(
                doc.node(id).is_some(),
                "every pasted id exists in the document"
            );
            assert!(
                id.contains("-1000-"),
                "ids are re-minted with the paste stamp"
            );
        }
        // Nodes keep array order: image first, then text.
        let pasted_image = doc
            .node(&pasted[0])
            .map(|node| (node.position.x, node.position.y, node.title.clone()))
            .unwrap_or_default();
        assert_eq!(pasted_image.0, origin.0 + PASTE_OFFSET);
        assert_eq!(pasted_image.1, origin.1 + PASTE_OFFSET);
        assert_eq!(pasted_image.2, "Image Copy");
        let pasted_is_image = doc
            .node(&pasted[0])
            .is_some_and(|node| node.node_type == CanvasNodeType::Image);
        assert!(pasted_is_image, "the pasted node keeps its type");

        // The connection was remapped onto the new endpoints.
        let remapped = doc
            .connections()
            .iter()
            .find(|connection| connection.from_node_id == pasted[0])
            .map(|connection| (connection.to_node_id.clone(), connection.id.clone()));
        let (to_node_id, connection_id) = remapped.unwrap_or_default();
        assert_eq!(to_node_id, pasted[1]);
        assert!(connection_id.starts_with("conn-1000-"));
        assert_eq!(doc.selection().len(), 2);
        assert!(doc.selection().contains(&pasted[0]));
    }

    #[test]
    fn paste_commits_one_undoable_history_entry() {
        let mut doc = doc();
        doc.select_node("prompt", false);
        let clipboard = doc.copy_selection().unwrap_or_default();
        let before = doc.nodes().len();
        doc.paste_nodes(&clipboard, 2_000);
        assert_eq!(doc.nodes().len(), before + 1);
        // The commit lands once the 180ms debounce window elapses.
        assert!(doc.tick_history(2_000 + open_canvas_core::history::HISTORY_DEBOUNCE_MS));
        assert!(doc.can_undo());
        assert!(doc.undo(), "paste is undoable");
        assert_eq!(doc.nodes().len(), before);
    }

    #[test]
    fn copy_title_never_doubles_the_suffix() {
        assert_eq!(copy_title("Image"), "Image Copy");
        assert_eq!(copy_title("Image Copy"), "Image Copy");
    }

    #[test]
    fn envelope_round_trips_through_text() {
        let mut doc = doc();
        doc.select_node("video-prompt", false);
        let clipboard = doc.copy_selection().unwrap_or_default();
        let text = to_clipboard_text(&clipboard).unwrap_or_default();
        assert!(text.contains("open-canvas-desktop-clipboard"));
        let back = from_clipboard_text(&text);
        assert_eq!(back, Some(clipboard));
    }

    #[test]
    fn foreign_and_malformed_clipboard_text_is_rejected() {
        assert_eq!(from_clipboard_text("hello world"), None);
        assert_eq!(from_clipboard_text(""), None);
        assert_eq!(
            from_clipboard_text("{\"app\":\"other\",\"version\":1}"),
            None
        );
        let wrong_version =
            r#"{"app":"open-canvas-desktop-clipboard","version":9,"nodes":[],"connections":[]}"#;
        assert_eq!(from_clipboard_text(wrong_version), None);
    }

    #[test]
    fn paste_text_path_pastes_and_preserves_relations() {
        let mut doc = doc();
        doc.select_node("prompt", false);
        doc.select_node("image-generation", true);
        let clipboard = doc.copy_selection().unwrap_or_default();
        assert_eq!(clipboard.connections.len(), 1, "prompt -> image-generation");
        let text = to_clipboard_text(&clipboard).unwrap_or_default();

        let pasted = doc.paste_clipboard_text(&text, 3_000);
        let pasted = pasted.unwrap_or_default();
        assert_eq!(pasted.len(), 2);
        let connection = doc
            .connections()
            .iter()
            .find(|connection| connection.from_node_id == pasted[0])
            .map(|connection| (connection.to_node_id.clone(), connection.relation.clone()));
        let (to_node_id, relation) = connection.unwrap_or_default();
        assert_eq!(to_node_id, pasted[1]);
        assert_eq!(relation, clipboard.connections[0].relation);
    }
}
