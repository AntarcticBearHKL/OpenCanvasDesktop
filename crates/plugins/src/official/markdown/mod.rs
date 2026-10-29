//! Markdown node logic, ported from `plugins/canvas/markdown/src/index.tsx`.
//!
//! The web node renders with `marked` loaded from a CDN. This port is a
//! dependency-free subset converter with documented limits: ATX headings, fenced
//! code, blockquotes, unordered/ordered lists, thematic breaks, paragraphs, and
//! the inline syntax handled by [`inline`]. No tables, nested lists, or
//! reference links.

mod inline;

use open_canvas_core::CanvasNodeMetadata;

use crate::abi::{CanvasNodeDefinition, CanvasNodeResource, NodeSize, NodeView};
use crate::plugin::CanvasPlugin;

pub use inline::{escape_html, inline_html, inline_text};

pub const NODE_TYPE: &str = "markdown:doc";
pub const MINIMAP_COLOR: &str = "#6366f1";
/// Preview placeholder for an empty document (`markdown/index.tsx:22`).
pub const EMPTY_PLACEHOLDER: &str =
    "*Select the node, then click ✎ in the toolbar to edit Markdown*";
const DEFAULT_SIZE: NodeSize = NodeSize {
    width: 360.0,
    height: 300.0,
};
const FENCE_MIN: usize = 3;

/// Convert the supported Markdown subset to HTML.
pub fn to_html(source: &str) -> String {
    let normalized = normalize_newlines(source);
    let lines: Vec<&str> = normalized.split('\n').collect();
    block_html(&lines)
}

/// Convert the supported Markdown subset to plain text.
pub fn to_text(source: &str) -> String {
    let normalized = normalize_newlines(source);
    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;
    for line in normalized.split('\n') {
        let trimmed = line.trim();
        if fence_marker(trimmed).is_some() {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(line.trim_end().to_string());
            continue;
        }
        if trimmed.is_empty() {
            out.push(String::new());
            continue;
        }
        if is_thematic_break(trimmed) {
            continue;
        }
        let mut text = trimmed;
        if let Some((_, heading_text)) = heading(text) {
            text = heading_text;
        }
        text = strip_quote(text);
        if list_marker(text).is_some() {
            text = strip_list_marker(text);
        }
        out.push(inline_text(text));
    }
    collapse_blank_lines(out)
}

/// Preview HTML for the node body, rendering [`EMPTY_PLACEHOLDER`] when the
/// document is empty (`markdown/index.tsx:27-36`, `const key = source || PLACEHOLDER`).
pub fn to_preview_html(source: &str) -> String {
    if source.is_empty() {
        to_html(EMPTY_PLACEHOLDER)
    } else {
        to_html(source)
    }
}

pub fn definition() -> CanvasNodeDefinition {
    let mut definition = CanvasNodeDefinition::new(NODE_TYPE, "Markdown", "📝", DEFAULT_SIZE)
        .with_description("Edit and render Markdown")
        .with_minimap_color(MINIMAP_COLOR)
        .with_metadata(CanvasNodeMetadata {
            content: Some(String::new()),
            ..CanvasNodeMetadata::default()
        });
    definition.hide_panel = true;
    definition.interaction_toggle = true;
    definition.force_interactive = Some(std::sync::Arc::new(is_editing));
    definition.resource = Some(std::sync::Arc::new(document_resource));
    definition
}

pub fn plugin() -> CanvasPlugin {
    CanvasPlugin::new("markdown", "Markdown node", "1.1.0", vec![definition()])
        .with_description("Edit and render Markdown on the canvas")
}

fn is_editing(view: &NodeView<'_>) -> bool {
    view.extra("editing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn document_resource(view: &NodeView<'_>) -> Option<CanvasNodeResource> {
    let content = view.metadata()?.content.clone()?;
    Some(CanvasNodeResource::text(content))
}

fn normalize_newlines(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

fn block_html(lines: &[&str]) -> String {
    let mut html = String::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if let Some(marker) = fence_marker(trimmed) {
            let mut code = String::new();
            let mut cursor = index + 1;
            while cursor < lines.len() && !is_closing_fence(lines[cursor].trim(), marker) {
                code.push_str(lines[cursor].trim_end());
                code.push('\n');
                cursor += 1;
            }
            if cursor < lines.len() {
                cursor += 1;
            }
            html.push_str("<pre><code>");
            html.push_str(&escape_html(code.trim_end_matches('\n')));
            html.push_str("</code></pre>\n");
            index = cursor;
            continue;
        }
        if let Some((level, text)) = heading(trimmed) {
            html.push_str(&format!("<h{level}>{}</h{level}>\n", inline_html(text)));
            index += 1;
            continue;
        }
        if is_thematic_break(trimmed) {
            html.push_str("<hr />\n");
            index += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            let mut inner_lines: Vec<&str> = Vec::new();
            let mut cursor = index;
            while cursor < lines.len() && lines[cursor].trim_start().starts_with('>') {
                inner_lines.push(strip_quote(lines[cursor]));
                cursor += 1;
            }
            html.push_str("<blockquote>\n");
            html.push_str(&block_html(&inner_lines));
            html.push_str("</blockquote>\n");
            index = cursor;
            continue;
        }
        if let Some(ordered) = list_marker(trimmed) {
            let tag = if ordered { "ol" } else { "ul" };
            html.push_str(&format!("<{tag}>\n"));
            while index < lines.len() {
                let candidate = lines[index].trim();
                match list_marker(candidate) {
                    Some(kind) if kind == ordered => {
                        let item = strip_list_marker(candidate);
                        html.push_str(&format!("<li>{}</li>\n", inline_html(item)));
                        index += 1;
                    }
                    _ => break,
                }
            }
            html.push_str(&format!("</{tag}>\n"));
            continue;
        }
        let mut paragraph: Vec<&str> = Vec::new();
        while index < lines.len() {
            let candidate = lines[index].trim();
            if candidate.is_empty() || starts_block(candidate) {
                break;
            }
            paragraph.push(candidate);
            index += 1;
        }
        html.push_str(&format!("<p>{}</p>\n", inline_html(&paragraph.join("\n"))));
    }
    html
}

fn starts_block(line: &str) -> bool {
    fence_marker(line).is_some()
        || heading(line).is_some()
        || is_thematic_break(line)
        || line.starts_with('>')
        || list_marker(line).is_some()
}

/// ATX heading: 1-6 `#` followed by a space (`# Title`).
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if level == 0 || level > 6 {
        return None;
    }
    let text = line.get(level..)?.strip_prefix(' ')?;
    Some((level, text.trim_end()))
}

fn is_thematic_break(line: &str) -> bool {
    let marker = match line.chars().next() {
        Some(character @ ('-' | '*' | '_')) => character,
        _ => return false,
    };
    let mut count = 0;
    for character in line.chars() {
        if character == marker {
            count += 1;
        } else if character != ' ' {
            return false;
        }
    }
    count >= FENCE_MIN
}

/// `Some(ordered)` when the line begins a list item (`- `, `* `, `+ `, `1. `).
fn list_marker(line: &str) -> Option<bool> {
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*' | b'+')) && bytes.get(1) == Some(&b' ') {
        return Some(false);
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits > 0 && bytes.get(digits) == Some(&b'.') && bytes.get(digits + 1) == Some(&b' ') {
        return Some(true);
    }
    None
}

fn strip_list_marker(line: &str) -> &str {
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*' | b'+')) {
        return line.get(2..).unwrap_or_default();
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    line.get(digits + 2..).unwrap_or_default()
}

fn strip_quote(line: &str) -> &str {
    let rest = line
        .trim_start()
        .strip_prefix('>')
        .unwrap_or(line.trim_start());
    rest.strip_prefix(' ').unwrap_or(rest)
}

fn fence_marker(line: &str) -> Option<char> {
    let marker = match line.chars().next() {
        Some(character @ ('`' | '~')) => character,
        _ => return None,
    };
    let count = line
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (count >= FENCE_MIN).then_some(marker)
}

fn is_closing_fence(line: &str, marker: char) -> bool {
    let count = line
        .chars()
        .take_while(|character| *character == marker)
        .count();
    count >= FENCE_MIN && line.chars().skip(count).all(|character| character == ' ')
}

fn collapse_blank_lines(lines: Vec<String>) -> String {
    let mut result: Vec<&str> = Vec::new();
    for line in &lines {
        let skip = match result.last() {
            None => true,
            Some(last) => last.is_empty(),
        };
        if line.is_empty() && skip {
            continue;
        }
        result.push(line);
    }
    while matches!(result.last(), Some(line) if line.is_empty()) {
        result.pop();
    }
    result.join("\n")
}
