//! Shared fixtures for the plugin integration tests (test builds only).

use std::path::PathBuf;

use open_canvas_core::node::{CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position};

use crate::plugin_host::PluginHost;

/// A host rooted at a fixed fake directory (no disk access happens).
pub fn plugin_host() -> PluginHost {
    PluginHost::new(PathBuf::from("D:\\tmp\\plugins"))
}

/// A detached plugin node with `metadata.content` set.
pub fn plugin_node(type_id: &str, content: Option<&str>) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("node-{type_id}"),
        node_type: CanvasNodeType::from_id(type_id),
        title: type_id.to_string(),
        position: Position::new(0.0, 0.0),
        width: 240.0,
        height: 200.0,
        metadata: Some(CanvasNodeMetadata {
            content: content.map(str::to_string),
            ..CanvasNodeMetadata::default()
        }),
    }
}

/// A detached sticky note; `content: Some("")` is the empty-note case.
pub fn sticky_node(content: Option<&str>) -> CanvasNodeData {
    let mut node = plugin_node(
        open_canvas_plugins::official::sticky_note::NODE_TYPE,
        content,
    );
    node.id = "sticky-1".to_string();
    node
}
