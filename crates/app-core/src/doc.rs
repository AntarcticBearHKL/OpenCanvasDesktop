//! Canvas document state: the node/connection lists, selection, tool and the
//! undo history. All domain math (snapping, connection legality, history, node
//! specs) comes from `open-canvas-core`; gestures live in `nav`, `gesture`,
//! `connect` and `doc_edit` as further `impl Doc` blocks.

use std::collections::BTreeSet;

use open_canvas_core::geometry::{MovedNode, SnapGuides};
use open_canvas_core::history::{History, Snapshot};
use open_canvas_core::node::{CanvasConnection, CanvasNodeData, ConnectionHandleType};

use crate::coords::{ScreenPos, Viewport, WorldPos};
use crate::gesture::ResizeGesture;

/// In-flight pointer gesture.
#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Pan {
        start: ScreenPos,
        last: ScreenPos,
        moved: bool,
    },
    Nodes {
        start: ScreenPos,
        ids: Vec<String>,
        initial: Vec<MovedNode>,
        moved: bool,
    },
    Marquee {
        start: WorldPos,
        current: WorldPos,
        additive: bool,
        initial: BTreeSet<String>,
    },
    Connect {
        node_id: String,
        handle: ConnectionHandleType,
        cursor: Option<WorldPos>,
        target: Option<String>,
        /// A node under the cursor the pair rules reject: kept so releasing
        /// reports WHY nothing happened instead of a silent no-op.
        reject: Option<String>,
    },
    Resize(ResizeGesture),
    /// Ctrl + right-button freehand wire cut (`startCutStroke`/`finishCutStroke`,
    /// project.tsx:771-807): the sampled world-space polyline being drawn.
    Cut {
        points: Vec<WorldPos>,
    },
}

impl Drag {}

/// The live canvas document plus interaction state.
#[derive(Debug)]
pub struct Doc {
    pub nodes: Vec<CanvasNodeData>,
    pub connections: Vec<CanvasConnection>,
    pub viewport: Viewport,
    pub selection: BTreeSet<String>,
    /// Selected wire (`selectedConnectionId`, project.tsx:240); mutually
    /// exclusive with a node selection at select time.
    pub selected_connection: Option<String>,
    pub drag: Option<Drag>,
    pub snap_guides: SnapGuides,
    /// Why the last connection release did nothing (an incompatible pair was
    /// dropped): the handler shows it as a notice.
    pub connect_reject: Option<String>,
    history: History,
    pub seq: u64,
}

impl Doc {
    fn snapshot(&self) -> Snapshot {
        Snapshot::new(self.nodes.clone(), self.connections.clone())
    }

    /// Builds a document from node/connection lists and resets the history to it
    /// (web: `resetHistory` on project load).
    pub fn assemble(nodes: Vec<CanvasNodeData>, connections: Vec<CanvasConnection>) -> Self {
        let mut doc = Self {
            nodes,
            connections,
            viewport: Viewport::new(),
            selection: BTreeSet::new(),
            selected_connection: None,
            // There is no tool mode: left-drag on the background pans, a plain
            // left click selects/deselects and shift+left-drag marquee-selects.
            drag: None,
            snap_guides: SnapGuides::default(),
            connect_reject: None,
            history: History::new(),
            seq: 0,
        };
        doc.history.reset(doc.snapshot());
        doc
    }

    /// Commits the current document to the undo history (debounced by `core::History`).
    pub fn commit(&mut self, now_ms: u64) {
        let snapshot = self.snapshot();
        self.history.record(snapshot, now_ms);
    }

    pub fn nodes(&self) -> &[CanvasNodeData] {
        &self.nodes
    }

    /// Mutable access for gesture/overlay mutation; callers must `commit` and
    /// resync afterwards.
    pub fn nodes_mut(&mut self) -> &mut Vec<CanvasNodeData> {
        &mut self.nodes
    }

    pub fn connections(&self) -> &[CanvasConnection] {
        &self.connections
    }

    /// Mutable connection access for gesture code and tests; callers `commit`.
    ///
    /// Gated like `Doc::demo`: `crates/app`'s test target links this crate as a
    /// library, where `cfg(test)` is off.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn connections_mut(&mut self) -> &mut Vec<CanvasConnection> {
        &mut self.connections
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn selection(&self) -> &BTreeSet<String> {
        &self.selection
    }

    pub fn snap_guides(&self) -> &SnapGuides {
        &self.snap_guides
    }

    pub fn drag(&self) -> Option<&Drag> {
        self.drag.as_ref()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn node(&self, id: &str) -> Option<&CanvasNodeData> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Click-select (project.tsx:831-874): shift adds, otherwise a click on an
    /// unselected node replaces the selection; clicking a selected node keeps it
    /// so the whole selection can be dragged.
    pub fn select_node(&mut self, id: &str, shift: bool) {
        self.selected_connection = None;
        if shift {
            self.selection.insert(id.to_string());
        } else if !self.selection.contains(id) {
            self.selection.clear();
            self.selection.insert(id.to_string());
        }
    }

    pub fn select_all(&mut self) {
        self.selection = self.nodes.iter().map(|node| node.id.clone()).collect();
        self.selected_connection = None;
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Aborts any in-flight gesture (Escape — project.tsx:1243-1255).
    pub fn cancel_drag(&mut self) {
        self.drag = None;
        self.snap_guides = SnapGuides::default();
    }

    /// Ids currently being dragged (empty when no node drag is active).
    pub fn drag_node_ids(&self) -> Vec<String> {
        match &self.drag {
            Some(Drag::Nodes { ids, .. }) => ids.clone(),
            _ => Vec::new(),
        }
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        self.nodes = snapshot.nodes;
        self.connections = snapshot.connections;
        self.selection.clear();
        self.selected_connection = None;
        self.drag = None;
        self.snap_guides = SnapGuides::default();
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo() {
            Some(snapshot) => {
                self.apply_snapshot(snapshot);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.history.redo() {
            Some(snapshot) => {
                self.apply_snapshot(snapshot);
                true
            }
            None => false,
        }
    }

    pub fn tick_history(&mut self, now_ms: u64) -> bool {
        self.history.tick(now_ms)
    }
}

// `doc_tests.rs` moved with this module: its only non-domain dependency was
// `library::EditorMeta`, which now lives here as `doc_project::EditorMeta`.
#[cfg(test)]
#[path = "doc_tests.rs"]
mod tests;
