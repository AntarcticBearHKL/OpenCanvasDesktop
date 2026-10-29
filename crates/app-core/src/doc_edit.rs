//! Document edits: the demo scene, node creation and deletion.

use std::collections::BTreeSet;

use open_canvas_core::constants::{get_node_spec, NodeSpec};
use open_canvas_core::geometry::is_node_locked;
use open_canvas_core::node::{CanvasConnection, CanvasNodeData, CanvasNodeType, Position};
// The demo fixture's metadata patch closure is test-only (or the
// `test-fixtures` feature that `crates/app`'s test target enables).
#[cfg(any(test, feature = "test-fixtures"))]
use open_canvas_core::node::CanvasNodeMetadata;

use crate::coords::WorldPos;
use crate::doc::Doc;

/// True when `a` and `b` overlap once both are grown by `margin` (so two
/// rectangles have to stay at least `margin` apart).
fn rects_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), margin: f64) -> bool {
    a.0 < b.0 + b.2 + margin
        && b.0 < a.0 + a.2 + margin
        && a.1 < b.1 + b.3 + margin
        && b.1 < a.1 + a.3 + margin
}

/// First free `(x, y)` for a `width` x `height` rectangle around `(base_x,
/// base_y)`: the columns march right one card-gap at a time, then the same
/// columns are retried above and below the default row (`step` 1, 2, ...).
pub fn free_output_spot(
    nodes: &[CanvasNodeData],
    base_x: f64,
    base_y: f64,
    width: f64,
    height: f64,
    gap: f64,
    margin: f64,
) -> (f64, f64) {
    const COLUMNS: usize = 48;
    const ROWS: usize = 24;
    let blocked = |x: f64, y: f64| {
        nodes.iter().any(|node| {
            rects_overlap(
                (x, y, width, height),
                (node.position.x, node.position.y, node.width, node.height),
                margin,
            )
        })
    };
    let column_x = |column: usize| {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small search index, exact in f64"
        )]
        let index = column as f64;
        base_x + index * (width + gap)
    };
    for column in 0..COLUMNS {
        let x = column_x(column);
        if !blocked(x, base_y) {
            return (x, base_y);
        }
    }
    for step in 1..ROWS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small search index, exact in f64"
        )]
        let dy = step as f64 * (height + gap);
        for column in 0..8 {
            let x = column_x(column);
            if !blocked(x, base_y - dy) {
                return (x, base_y - dy);
            }
            if !blocked(x, base_y + dy) {
                return (x, base_y + dy);
            }
        }
    }
    (column_x(COLUMNS), base_y - 24.0 * (height + gap))
}

impl Doc {
    /// P1's static scene, now real `CanvasNodeData` with legal connections
    /// (test fixture since P4: the app starts on the library instead).
    ///
    /// `test-fixtures` and not `cfg(test)`: `crates/app`'s test target links this
    /// crate as a library, where `cfg(test)` is off for the library build.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn demo() -> Self {
        use open_canvas_core::node::CanvasConnection;

        let mut nodes = Vec::new();
        let mut push =
            |kind: CanvasNodeType, x: f64, y: f64, patch: &dyn Fn(&mut CanvasNodeMetadata)| {
                let spec = get_node_spec(&kind);
                let mut metadata = spec.metadata;
                patch(&mut metadata);
                nodes.push(CanvasNodeData {
                    id: kind.as_str().to_string(),
                    node_type: kind,
                    title: spec.title,
                    position: Position::new(x, y),
                    width: spec.width,
                    height: spec.height,
                    metadata: Some(metadata),
                });
            };
        push(CanvasNodeType::Prompt, 64.0, 96.0, &|m| {
            m.prompt = Some("A cozy mountain cabin at dusk, warm window light".into());
        });
        push(CanvasNodeType::Image, 492.0, 48.0, &|_| {});
        push(CanvasNodeType::Text, 492.0, 372.0, &|m| {
            m.content = Some(
                "The quick brown fox jumps over the lazy dog.\nPack my box with five dozen liquor jugs."
                    .into(),
            );
        });
        push(CanvasNodeType::VideoPrompt, 64.0, 364.0, &|m| {
            m.prompt = Some("Slow dolly-in over the product, warm rim light".into());
        });
        // The two generation config nodes live off the initial view (P3b demo of
        // the 600px config panel; pan right/down to reach them).
        push(CanvasNodeType::ImageGeneration, 940.0, 96.0, &|_| {});
        push(CanvasNodeType::VideoGeneration, 940.0, 696.0, &|_| {});
        let connections = vec![
            CanvasConnection {
                id: "conn-prompt-text".into(),
                from_node_id: "prompt".into(),
                to_node_id: "text".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
            CanvasConnection {
                id: "conn-image-text".into(),
                from_node_id: "image".into(),
                to_node_id: "text".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
            CanvasConnection {
                id: "conn-video-prompt-text".into(),
                from_node_id: "video-prompt".into(),
                to_node_id: "text".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
            CanvasConnection {
                id: "conn-prompt-image-generation".into(),
                from_node_id: "prompt".into(),
                to_node_id: "image-generation".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
            CanvasConnection {
                id: "conn-video-prompt-video-generation".into(),
                from_node_id: "video-prompt".into(),
                to_node_id: "video-generation".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
        ];
        Doc::assemble(nodes, connections)
    }

    /// Creates a node centered on `at` (canvas-node-factory.ts:9-25), selects it
    /// and starts a history entry. Returns the new id.
    pub fn create_node(&mut self, kind: CanvasNodeType, at: WorldPos, now_ms: u64) -> String {
        let spec = get_node_spec(&kind);
        self.create_node_with(kind, at, spec, now_ms)
    }

    /// `create_node` with an explicit spec: the plugin host resolves registry
    /// specs (plugin sizes/titles/metadata) the core constants cannot.
    pub fn create_node_with(
        &mut self,
        kind: CanvasNodeType,
        at: WorldPos,
        mut spec: NodeSpec,
        now_ms: u64,
    ) -> String {
        if kind == CanvasNodeType::ImageAttempt {
            let next = self
                .nodes
                .iter()
                .filter_map(|node| node.metadata.as_ref().and_then(|meta| meta.attempt_version))
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            spec.metadata.attempt_version = Some(next);
            spec.metadata.attempt_title = Some("new attempt".to_string());
            spec.title = format!("v{next} · new attempt");
        }
        self.seq = self.seq.wrapping_add(1);
        let id = format!("{}-{now_ms}-{}", kind.as_str(), self.seq);
        self.nodes.push(CanvasNodeData {
            id: id.clone(),
            node_type: kind,
            title: spec.title,
            position: Position::new(at.x - spec.width / 2.0, at.y - spec.height / 2.0),
            width: spec.width,
            height: spec.height,
            metadata: Some(spec.metadata),
        });
        self.selection.clear();
        self.selection.insert(id.clone());
        self.commit(now_ms);
        id
    }

    /// Duplicates a node 36px down-right with a `Copy` title suffix and selects
    /// the copy (`duplicateNode`, use-canvas-document.ts:99-112); the unique id
    /// suffix uses the document sequence counter instead of `Math.random()`.
    pub fn duplicate_node(&mut self, id: &str, now_ms: u64) -> Option<String> {
        let source = self.nodes().iter().find(|node| node.id == id)?.clone();
        self.seq = self.seq.wrapping_add(1);
        let next_id = format!("{}-{now_ms}-{}", source.node_type.as_str(), self.seq);
        self.nodes.push(CanvasNodeData {
            id: next_id.clone(),
            title: format!("{} Copy", source.title),
            position: Position::new(source.position.x + 36.0, source.position.y + 36.0),
            ..source
        });
        self.selection.clear();
        self.selection.insert(next_id.clone());
        self.commit(now_ms);
        Some(next_id)
    }

    /// Horizontal gap between a generation card and the result it produces.
    pub const OUTPUT_GAP: f64 = 48.0;

    /// Empty space kept around a placed result so cards never touch.
    pub const OUTPUT_MARGIN: f64 = 16.0;

    /// Places a generated result in the nearest free space to the right of
    /// `source_id`, vertically centred on it, without creating a connection:
    /// the generation card keeps its prompt/settings, the media is a sibling on
    /// the canvas (the web's generated child node, minus its output wire).
    ///
    /// The default spot is `OUTPUT_GAP` to the right. Existing nodes of any
    /// kind block it; the search then walks further right, and finally above
    /// and below the default row, until a rectangle with `OUTPUT_MARGIN`
    /// clearance is free.
    pub fn place_output_node(
        &mut self,
        source_id: &str,
        kind: CanvasNodeType,
        now_ms: u64,
    ) -> Option<String> {
        let source = self.nodes.iter().find(|node| node.id == source_id)?.clone();
        let spec = get_node_spec(&kind);
        let base_x = source.position.x + source.width + Self::OUTPUT_GAP;
        let base_y = source.position.y + source.height / 2.0 - spec.height / 2.0;
        let (x, y) = free_output_spot(
            &self.nodes,
            base_x,
            base_y,
            spec.width,
            spec.height,
            Self::OUTPUT_GAP,
            Self::OUTPUT_MARGIN,
        );
        self.seq = self.seq.wrapping_add(1);
        let id = format!("{}-{now_ms}-{}", kind.as_str(), self.seq);
        self.nodes.push(CanvasNodeData {
            id: id.clone(),
            node_type: kind,
            title: spec.title,
            position: Position::new(x, y),
            width: spec.width,
            height: spec.height,
            metadata: Some(spec.metadata),
        });
        self.commit(now_ms);
        Some(id)
    }

    /// Removes one node and every wire touching it (used when a generation
    /// placeholder has to change kind between a retry). Returns whether the
    /// node existed.
    pub fn remove_node(&mut self, id: &str, now_ms: u64) -> bool {
        let before = self.nodes.len();
        self.nodes.retain(|node| node.id != id);
        if self.nodes.len() == before {
            return false;
        }
        self.connections
            .retain(|conn| conn.from_node_id != id && conn.to_node_id != id);
        self.selection.remove(id);
        if self
            .selected_connection
            .as_ref()
            .is_some_and(|selected| !self.connections.iter().any(|conn| conn.id == *selected))
        {
            self.selected_connection = None;
        }
        self.commit(now_ms);
        true
    }

    /// Creates a downstream output node to the right of `source_id` and wires
    /// `source_id -> output` in the same history entry. `stack` shifts the new
    /// node down so a multi-image result never piles up on one spot.
    ///
    /// Video output keeps this wired placement; an image result uses
    /// [`Doc::place_output_node`] instead (an Image node accepts no wires).
    pub fn create_output_node(
        &mut self,
        source_id: &str,
        kind: CanvasNodeType,
        stack: usize,
        now_ms: u64,
    ) -> Option<String> {
        let source = self.nodes.iter().find(|node| node.id == source_id)?.clone();
        let spec = get_node_spec(&kind);
        let gap = 48.0;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small output index, exact in f64"
        )]
        let row = stack as f64;
        let at = WorldPos::new(
            source.position.x + source.width + gap + spec.width / 2.0,
            source.position.y + spec.height / 2.0 + row * (spec.height + 24.0),
        );
        let id = self.create_node(kind, at, now_ms);
        self.connections.push(CanvasConnection {
            id: format!("conn-{source_id}-{id}"),
            from_node_id: source_id.to_string(),
            to_node_id: id.clone(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        });
        self.commit(now_ms);
        Some(id)
    }

    pub fn delete_selection(&mut self, now_ms: u64) -> bool {
        let removable: BTreeSet<String> = self
            .selection
            .iter()
            .filter(|id| self.node(id).is_some_and(|node| !is_node_locked(node)))
            .cloned()
            .collect();
        if removable.is_empty() {
            return false;
        }
        self.nodes.retain(|node| !removable.contains(&node.id));
        self.connections.retain(|conn| {
            !removable.contains(&conn.from_node_id) && !removable.contains(&conn.to_node_id)
        });
        if self
            .selected_connection
            .as_ref()
            .is_some_and(|id| !self.connections.iter().any(|conn| conn.id == *id))
        {
            self.selected_connection = None;
        }
        self.selection.clear();
        self.commit(now_ms);
        true
    }

    /// Moves a node one step inside the array whose order is the z-order
    /// (canvas-node-layer-popover.tsx:170-173): `up` moves the row toward the
    /// array head, `down` toward the tail; the list ends are rejected.
    pub fn move_node(&mut self, id: &str, up: bool, now_ms: u64) -> bool {
        let Some(index) = self.nodes.iter().position(|node| node.id == id) else {
            return false;
        };
        let target = if up {
            let Some(previous) = index.checked_sub(1) else {
                return false;
            };
            previous
        } else {
            index + 1
        };
        if target >= self.nodes.len() {
            return false;
        }
        self.nodes.swap(index, target);
        self.commit(now_ms);
        true
    }
}
