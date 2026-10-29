//! HTML node logic, ported from `plugins/canvas/html/src/index.tsx`.
//!
//! The browser-only half is the sandboxed `iframe srcDoc` renderer; the portable
//! half is the `{{input}}` substitution: every upstream node's `metadata.content`
//! is joined with newlines and replaces `{{ input }}` in the node's own content.

use std::sync::Arc;

use open_canvas_core::{CanvasNodeData, CanvasNodeMetadata};

use crate::abi::{CanvasNodeDefinition, NodeSize, NodeView};
use crate::plugin::CanvasPlugin;

pub const NODE_TYPE: &str = "html:render";
pub const MINIMAP_COLOR: &str = "#ec4899";
const DEFAULT_SIZE: NodeSize = NodeSize {
    width: 420.0,
    height: 320.0,
};

/// Join the non-empty `content` of upstream nodes (`html/index.tsx:65-73`).
pub fn upstream_text(nodes: &[CanvasNodeData]) -> String {
    nodes
        .iter()
        .filter_map(|node| node.metadata.as_ref())
        .filter_map(|metadata| metadata.content.as_deref())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace every `{{input}}` / `{{ input }}` in `template` with `input`
/// (`value.replace(/\{\{\s*input\s*\}\}/g, upstreamText)`).
pub fn substitute_input(template: &str, input: &str) -> String {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{' && bytes.get(index + 1) == Some(&b'{') {
            if let Some(end) = placeholder_end(bytes, index + 2) {
                out.push_str(input);
                index = end;
                continue;
            }
        }
        let Some(character) = template[index..].chars().next() else {
            break;
        };
        out.push(character);
        index += character.len_utf8();
    }
    out
}

fn placeholder_end(bytes: &[u8], mut index: usize) -> Option<usize> {
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    if !bytes.get(index..)?.starts_with(b"input") {
        return None;
    }
    index += "input".len();
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    match (bytes.get(index), bytes.get(index + 1)) {
        (Some(b'}'), Some(b'}')) => Some(index + 2),
        _ => None,
    }
}

/// Render a node against its upstream nodes (`HtmlContent`).
pub fn render(node: &CanvasNodeData, upstream: &[CanvasNodeData]) -> String {
    let template = node
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.content.as_deref())
        .unwrap_or_default();
    substitute_input(template, &upstream_text(upstream))
}

/// True while the node's source editor is open (`metadata.editing`).
fn is_editing(view: &NodeView<'_>) -> bool {
    view.extra("editing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub fn definition() -> CanvasNodeDefinition {
    let mut definition = CanvasNodeDefinition::new(NODE_TYPE, "HTML", "🌐", DEFAULT_SIZE)
        .with_description("Render HTML in a sandbox")
        .with_minimap_color(MINIMAP_COLOR)
        .with_metadata(CanvasNodeMetadata {
            content: Some(String::new()),
            ..CanvasNodeMetadata::default()
        });
    definition.hide_panel = true;
    definition.interaction_toggle = true;
    definition.force_interactive = Some(Arc::new(is_editing));
    definition
}

pub fn plugin() -> CanvasPlugin {
    CanvasPlugin::new("html", "HTML node", "1.2.0", vec![definition()]).with_description(
        "Render HTML in a sandboxed iframe with {{input}} upstream text injection",
    )
}
