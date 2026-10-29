//! SVG node logic, ported from `plugins/canvas/svg/src/index.tsx`.
//!
//! Pure logic: source validation/normalization, upstream SVG adoption, and the
//! transparent-background flag. The web node renders SVG via
//! `dangerouslySetInnerHTML`; native rendering is the Slint task's job.

use open_canvas_core::CanvasNodeData;
use thiserror::Error;

use crate::abi::{CanvasNodeDefinition, NodeSize};
use crate::plugin::CanvasPlugin;

pub const NODE_TYPE: &str = "svg:vector";
pub const MINIMAP_COLOR: &str = "#14b8a6";
/// The node card is transparent so vector art blends into the canvas.
pub const TRANSPARENT_BACKGROUND: bool = true;
const DEFAULT_SIZE: NodeSize = NodeSize {
    width: 320.0,
    height: 320.0,
};

/// Well-formedness failures of an SVG source string.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SvgError {
    #[error("svg source is empty")]
    Empty,
    #[error("svg source has no <svg> root element")]
    MissingRoot,
    #[error("unclosed <{0}> element")]
    UnclosedTag(String),
    #[error("mismatched closing tag: expected </{expected}>, found </{found}>")]
    MismatchedTag { expected: String, found: String },
    #[error("unterminated comment or processing instruction")]
    Unterminated,
}

/// Validate that `source` is well-formed enough for the renderer to consume.
pub fn validate(source: &str) -> Result<(), SvgError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(SvgError::Empty);
    }
    let mut stack: Vec<String> = Vec::new();
    let mut saw_root = false;
    let mut index = 0;
    while let Some(offset) = source[index..].find('<') {
        index += offset;
        if let Some(end) = skip_non_element(source, index)? {
            index = end;
            continue;
        }
        let closing = source[index..].starts_with("</");
        let name_start = index + if closing { 2 } else { 1 };
        let name = read_name(source, name_start);
        if name.is_empty() {
            index += 1;
            continue;
        }
        let tag_end =
            find_tag_end(source, index).ok_or_else(|| SvgError::UnclosedTag(name.clone()))?;
        let self_closing = source[index..tag_end]
            .trim_end_matches('>')
            .trim_end()
            .ends_with('/');
        if closing {
            match stack.pop() {
                Some(open) if open == name => {}
                Some(open) => {
                    return Err(SvgError::MismatchedTag {
                        expected: open,
                        found: name,
                    })
                }
                None => return Err(SvgError::MissingRoot),
            }
        } else {
            if !saw_root {
                if name != "svg" {
                    return Err(SvgError::MissingRoot);
                }
                saw_root = true;
            }
            if !self_closing {
                stack.push(name);
            }
        }
        index = tag_end;
    }
    if !saw_root {
        return Err(SvgError::MissingRoot);
    }
    match stack.last() {
        Some(open) => Err(SvgError::UnclosedTag(open.clone())),
        None => Ok(()),
    }
}

/// Validate and return the trimmed source (the web `value.trim()`).
pub fn normalize(source: &str) -> Result<String, SvgError> {
    validate(source)?;
    Ok(source.trim().to_string())
}

/// First upstream node content that is an SVG document (`svg/index.tsx:10-13`).
pub fn upstream_svg(nodes: &[CanvasNodeData]) -> Option<String> {
    nodes
        .iter()
        .filter_map(|node| node.metadata.as_ref())
        .filter_map(|metadata| metadata.content.as_deref())
        .find(|content| content.trim().starts_with("<svg"))
        .map(str::to_string)
}

/// The web auto-adopts upstream only when the node has no stored content.
pub fn should_adopt_upstream(stored: Option<&str>, upstream: Option<&str>) -> bool {
    stored.is_none() && upstream.is_some()
}

/// Displayed SVG: own non-blank content, else the upstream source
/// (`value.trim() || upstream || ""`).
pub fn effective_svg<'a>(stored: Option<&'a str>, upstream: Option<&'a str>) -> Option<&'a str> {
    stored.filter(|value| !value.trim().is_empty()).or(upstream)
}

fn skip_non_element(source: &str, start: usize) -> Result<Option<usize>, SvgError> {
    let rest = &source[start..];
    let (marker, terminator) = if rest.starts_with("<!--") {
        ("<!--", "-->")
    } else if rest.starts_with("<![CDATA[") {
        ("<![CDATA[", "]]>")
    } else if rest.starts_with("<?") {
        ("<?", "?>")
    } else if rest.starts_with("<!") {
        ("<!", ">")
    } else {
        return Ok(None);
    };
    let after_marker = start + marker.len();
    match source[after_marker..].find(terminator) {
        Some(offset) => Ok(Some(after_marker + offset + terminator.len())),
        None => Err(SvgError::Unterminated),
    }
}

fn read_name(source: &str, start: usize) -> String {
    source[start..]
        .chars()
        .take_while(|character| {
            !character.is_whitespace() && *character != '>' && *character != '/'
        })
        .collect()
}

fn find_tag_end(source: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (offset, character) in source[start..].char_indices() {
        match quote {
            Some(active) if character == active => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '>' => return Some(start + offset + 1),
            None => {}
        }
    }
    None
}

pub fn definition() -> CanvasNodeDefinition {
    let mut definition = CanvasNodeDefinition::new(NODE_TYPE, "SVG", "🔷", DEFAULT_SIZE)
        .with_description("Render an SVG vector image")
        .with_minimap_color(MINIMAP_COLOR);
    definition.transparent_background = TRANSPARENT_BACKGROUND;
    definition
}

pub fn plugin() -> CanvasPlugin {
    CanvasPlugin::new("svg", "SVG node", "1.1.0", vec![definition()]).with_description(
        "Render an SVG vector image on a transparent background from an upstream text node",
    )
}
