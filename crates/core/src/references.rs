//! Resource references of the prompt composer's `@` mentions, mirroring
//! `web/src/lib/canvas/canvas-resource-references.ts` plus `builtinResource`
//! (`web/src/components/canvas/nodes/builtin-nodes.tsx:10-16`): the referenceable
//! nodes of a node are its incoming connection sources that carry content
//! (image/video) or text (text / prompt / video-prompt), labeled per kind in
//! connection order. A node with no such inputs falls back to itself when it is
//! itself a resource node.
//!
//! Slint's `TextInput` has no inline widgets, so a chip is serialized into the
//! text as its label — the exact serialization the web's `contentEditable`
//! editor performs (`serializeEditor`) — and the span helpers here locate those
//! labels for chip rendering and atomic Backspace/Delete removal.

use crate::node::{CanvasConnection, CanvasNodeData, CanvasNodeType};

/// `CanvasResourceKind` (canvas-resource-references.ts:8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    Video,
    Text,
}

impl ResourceKind {
    /// The wire id the candidate filter and the UI models use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Text => "text",
        }
    }

    /// 1-based label of the kind (`imageReferences.label` = `Image {index}`,
    /// `configNode.videoReferences` + index, `composer.resources.text`).
    fn label(self, index: usize) -> String {
        match self {
            Self::Image => format!("Image {index}"),
            Self::Video => format!("Reference videos {index}"),
            Self::Text => format!("Text {index}"),
        }
    }
}

/// One referenceable node (`CanvasResourceReference`), with the active flag
/// dropped: every mention reference is active in the desktop app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceReference {
    /// Id of the referenced node (the web stores URLs in `metadata.references`;
    /// the desktop stores this node id — the `videoSlots` convention of storing
    /// node ids, never payloads).
    pub node_id: String,
    pub kind: ResourceKind,
    /// "Image 1" / "Reference videos 1" / "Text 1".
    pub label: String,
    /// Node title, falling back to the label (`node.title || label`).
    pub title: String,
    /// Text contents of a text-kind resource (`resourceText`).
    pub text: Option<String>,
}

impl ResourceReference {
    /// Text of the chip pill (`createReferenceChip`): a text resource shows its
    /// content, everything else its label. Whitespace is collapsed so a
    /// multi-line text cannot blow up the single-line pill.
    pub fn chip_text(&self) -> String {
        match self.kind {
            ResourceKind::Text => self.text.clone().unwrap_or_else(|| self.label.clone()),
            ResourceKind::Image | ResourceKind::Video => self.label.clone(),
        }
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
    }
}

/// The resource kind a node contributes (`resourceKind` + `builtinResource`):
/// only content-bearing image/video nodes and text-bearing text/prompt nodes
/// are referenceable.
pub fn resource_kind(node: &CanvasNodeData) -> Option<ResourceKind> {
    let meta = node.metadata.as_ref()?;
    let has = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    match node.node_type {
        CanvasNodeType::Image if has(&meta.content) => Some(ResourceKind::Image),
        CanvasNodeType::Video if has(&meta.content) => Some(ResourceKind::Video),
        CanvasNodeType::Text if has(&meta.content) || has(&meta.prompt) => Some(ResourceKind::Text),
        CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt if has(&meta.prompt) => {
            Some(ResourceKind::Text)
        }
        _ => None,
    }
}

/// Text payload of a text-kind resource (`resourceText`); `None` for
/// image/video resources (the web returns `undefined` there).
pub fn resource_text(node: &CanvasNodeData) -> Option<String> {
    let meta = node.metadata.as_ref()?;
    match node.node_type {
        CanvasNodeType::Text => meta.content.clone().or_else(|| meta.prompt.clone()),
        CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt => meta.prompt.clone(),
        _ => None,
    }
}

/// All mention references of `node_id` (`buildNodeMentionReferences`): the
/// upstream resource nodes over the incoming connections, else the node itself
/// when it is a resource node, else empty.
pub fn mention_references(
    node_id: &str,
    nodes: &[CanvasNodeData],
    connections: &[CanvasConnection],
) -> Vec<ResourceReference> {
    let own_inputs: Vec<&CanvasNodeData> = connections
        .iter()
        .filter(|connection| connection.to_node_id == node_id)
        .filter_map(|connection| nodes.iter().find(|node| node.id == connection.from_node_id))
        .filter(|node| resource_kind(node).is_some())
        .collect();
    if !own_inputs.is_empty() {
        return label_references(own_inputs);
    }
    let own: Vec<&CanvasNodeData> = nodes
        .iter()
        .filter(|node| node.id == node_id && resource_kind(node).is_some())
        .collect();
    label_references(own)
}

/// Labels nodes per kind in list order (`labelResourceNodes`).
fn label_references<'a>(
    nodes: impl IntoIterator<Item = &'a CanvasNodeData>,
) -> Vec<ResourceReference> {
    let mut counts = [0usize; 3];
    nodes
        .into_iter()
        .filter_map(|node| {
            let kind = resource_kind(node)?;
            let slot = match kind {
                ResourceKind::Image => 0,
                ResourceKind::Video => 1,
                ResourceKind::Text => 2,
            };
            counts[slot] += 1;
            let label = kind.label(counts[slot]);
            let title = if node.title.is_empty() {
                label.clone()
            } else {
                node.title.clone()
            };
            Some(ResourceReference {
                node_id: node.id.clone(),
                kind,
                label,
                title,
                text: resource_text(node),
            })
        })
        .collect()
}

/// Byte span of the `@query` typed immediately before `caret` (web `syncMention`
/// regex `@([^\s@]*)$`): `(offset of '@', query)`.
pub fn mention_query(text: &str, caret: usize) -> Option<(usize, String)> {
    let caret = boundary(text, caret);
    let before = &text[..caret];
    let at = before.rfind('@')?;
    let query = &before[at + 1..];
    if query.chars().any(|ch| ch.is_whitespace() || ch == '@') {
        return None;
    }
    Some((at, query.to_string()))
}

/// Indices (into `references`) matching a mention query; an empty query matches
/// everything (`candidates` in canvas-prompt-chip-input.tsx:57-62).
pub fn filter_candidates(references: &[ResourceReference], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    references
        .iter()
        .enumerate()
        .filter(|(_, reference)| {
            if query.is_empty() {
                return true;
            }
            let haystack = format!(
                "{} {} {} {}",
                reference.label,
                reference.title,
                reference.kind.as_str(),
                reference.text.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            haystack.contains(&query)
        })
        .map(|(index, _)| index)
        .collect()
}

/// Every chip label present in `text`, in appearance order:
/// `(start, end, reference index)`. Longer labels are tried first so `Image 11`
/// is never read as `Image 1` (`parseTokens`, sorted by descending length).
pub fn chip_spans(text: &str, references: &[ResourceReference]) -> Vec<(usize, usize, usize)> {
    let mut order: Vec<usize> = (0..references.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(references[*index].label.len()));
    let mut spans = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let matched = order
            .iter()
            .find(|index| text[at..].starts_with(references[**index].label.as_str()));
        match matched {
            Some(index) => {
                let len = references[*index].label.len();
                spans.push((at, at + len, *index));
                at += len;
            }
            None => {
                at += text[at..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    spans
}

/// Chip whose label ends exactly at `caret` (web `adjacentReferenceNode` for
/// Backspace): the atomic unit a Backspace removes.
pub fn chip_span_before(
    text: &str,
    caret: usize,
    references: &[ResourceReference],
) -> Option<(usize, usize, usize)> {
    let caret = boundary(text, caret);
    let before = &text[..caret];
    references
        .iter()
        .enumerate()
        .find(|(_, reference)| before.ends_with(reference.label.as_str()))
        .map(|(index, reference)| (caret - reference.label.len(), caret, index))
}

/// Chip whose label starts at `caret` (web `adjacentReferenceNode` for Delete).
pub fn chip_span_after(
    text: &str,
    caret: usize,
    references: &[ResourceReference],
) -> Option<(usize, usize, usize)> {
    let caret = boundary(text, caret);
    references
        .iter()
        .enumerate()
        .find(|(_, reference)| text[caret..].starts_with(reference.label.as_str()))
        .map(|(index, reference)| (caret, caret + reference.label.len(), index))
}

/// `text` with `[start, end)` replaced, offsets clamped to char boundaries.
pub fn splice(text: &str, start: usize, end: usize, replacement: &str) -> String {
    let start = boundary(text, start);
    let end = boundary(text, end).max(start);
    format!("{}{}{}", &text[..start], replacement, &text[end..])
}

/// Largest char boundary at or before `offset`; carets arriving from the UI can
/// be stale by one keystroke after a programmatic edit.
fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{CanvasNodeMetadata, Position};

    fn node(id: &str, node_type: CanvasNodeType, metadata: CanvasNodeMetadata) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type,
            title: id.to_string(),
            position: Position::new(0.0, 0.0),
            width: 100.0,
            height: 100.0,
            metadata: Some(metadata),
        }
    }

    fn image(id: &str, content: &str) -> CanvasNodeData {
        node(
            id,
            CanvasNodeType::Image,
            CanvasNodeMetadata {
                content: Some(content.to_string()),
                ..Default::default()
            },
        )
    }

    fn text_node(id: &str, content: &str) -> CanvasNodeData {
        node(
            id,
            CanvasNodeType::Text,
            CanvasNodeMetadata {
                content: Some(content.to_string()),
                ..Default::default()
            },
        )
    }

    fn prompt(id: &str, prompt_text: &str) -> CanvasNodeData {
        node(
            id,
            CanvasNodeType::Prompt,
            CanvasNodeMetadata {
                prompt: Some(prompt_text.to_string()),
                ..Default::default()
            },
        )
    }

    fn connection(from: &str, to: &str) -> CanvasConnection {
        CanvasConnection {
            id: format!("conn-{from}-{to}"),
            from_node_id: from.to_string(),
            to_node_id: to.to_string(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }
    }

    #[test]
    fn resource_kind_requires_content_or_prompt() {
        assert_eq!(
            resource_kind(&image("a", "data:x")),
            Some(ResourceKind::Image)
        );
        // Empty content is falsy in the web check, so an empty image is not a resource.
        assert_eq!(resource_kind(&image("b", "")), None);
        assert_eq!(
            resource_kind(&text_node("c", "hello")),
            Some(ResourceKind::Text)
        );
        assert_eq!(
            resource_kind(&prompt("d", "a cat")),
            Some(ResourceKind::Text)
        );
        assert_eq!(resource_kind(&prompt("e", "")), None);
        let stack = node(
            "f",
            CanvasNodeType::ImageStack,
            CanvasNodeMetadata::default(),
        );
        assert_eq!(resource_kind(&stack), None);
        let bare_image = node("g", CanvasNodeType::Image, CanvasNodeMetadata::default());
        assert_eq!(resource_kind(&bare_image), None);
    }

    #[test]
    fn mention_references_label_inputs_per_kind_in_connection_order() {
        let nodes = vec![
            image("img-1", "data:1"),
            text_node("txt-1", "a line"),
            image("img-2", "data:2"),
            prompt("prompt-1", "draw it"),
        ];
        let connections = vec![
            connection("img-1", "prompt-1"),
            connection("txt-1", "prompt-1"),
            connection("img-2", "prompt-1"),
        ];
        let references = mention_references("prompt-1", &nodes, &connections);
        let labels: Vec<&str> = references.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Image 1", "Text 1", "Image 2"]);
        assert_eq!(references[1].text.as_deref(), Some("a line"));
        assert_eq!(references[0].text, None);
        assert_eq!(references[0].node_id, "img-1");
    }

    #[test]
    fn mention_references_fall_back_to_self_for_resource_nodes() {
        let nodes = vec![prompt("prompt-1", "draw it"), image("img-1", "data:1")];
        let references = mention_references("prompt-1", &nodes, &[]);
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].label, "Text 1");
        assert_eq!(references[0].text.as_deref(), Some("draw it"));

        // A non-resource node with no inputs has no references at all.
        let empty = prompt("prompt-2", "");
        let references = mention_references("prompt-2", &[empty], &[]);
        assert!(references.is_empty());
    }

    #[test]
    fn mention_query_matches_the_trailing_at_token() {
        let text = "a cabin @im";
        assert_eq!(mention_query(text, text.len()), Some((8, "im".into())));
        let text = "@";
        assert_eq!(mention_query(text, 1), Some((0, String::new())));
        // A space between the token and the caret closes it; '@@x' matches from
        // the last '@' exactly like the web regex does.
        let text = "@im x";
        assert_eq!(mention_query(text, text.len()), None);
        let text = "@@x";
        assert_eq!(mention_query(text, text.len()), Some((1, "x".into())));
        // No '@' at all.
        assert_eq!(mention_query("plain", 5), None);
    }

    #[test]
    fn filter_candidates_matches_label_title_kind_and_text() {
        let references = vec![
            ResourceReference {
                node_id: "img-1".into(),
                kind: ResourceKind::Image,
                label: "Image 1".into(),
                title: "Sunset".into(),
                text: None,
            },
            ResourceReference {
                node_id: "txt-1".into(),
                kind: ResourceKind::Text,
                label: "Text 1".into(),
                title: "Notes".into(),
                text: Some("warm light".into()),
            },
        ];
        assert_eq!(filter_candidates(&references, ""), vec![0, 1]);
        assert_eq!(filter_candidates(&references, "im"), vec![0]);
        assert_eq!(filter_candidates(&references, "SunSet"), vec![0]);
        assert_eq!(filter_candidates(&references, "warm"), vec![1]);
        assert_eq!(filter_candidates(&references, "text"), vec![1]);
        assert!(filter_candidates(&references, "zzz").is_empty());
    }

    #[test]
    fn chip_spans_prefer_the_longest_label() {
        let references = vec![
            ResourceReference {
                node_id: "a".into(),
                kind: ResourceKind::Image,
                label: "Image 1".into(),
                title: "Image 1".into(),
                text: None,
            },
            ResourceReference {
                node_id: "b".into(),
                kind: ResourceKind::Image,
                label: "Image 11".into(),
                title: "Image 11".into(),
                text: None,
            },
        ];
        let spans = chip_spans("see Image 11 and Image 1", &references);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0], (4, 12, 1));
        assert_eq!(spans[1], (17, 24, 0));
    }

    #[test]
    fn chip_spans_and_edges_follow_the_caret() {
        let references = vec![ResourceReference {
            node_id: "img".into(),
            kind: ResourceKind::Image,
            label: "Image 1".into(),
            title: "Image 1".into(),
            text: None,
        }];
        let text = "a cabin Image 1 ";
        assert_eq!(chip_span_before(text, 15, &references), Some((8, 15, 0)));
        // The trailing space sits after the chip: backspace there is a plain delete.
        assert_eq!(chip_span_before(text, 16, &references), None);
        assert_eq!(chip_span_after(text, 0, &references), None);
        assert_eq!(
            chip_span_after("Image 1 rest", 0, &references),
            Some((0, 7, 0))
        );
    }

    #[test]
    fn splice_replaces_a_range() {
        assert_eq!(splice("a cabin Image 1 rest", 8, 15, ""), "a cabin  rest");
        assert_eq!(splice("a cabin @im", 8, 11, "Image 1 "), "a cabin Image 1 ");
    }

    #[test]
    fn chip_text_prefers_the_text_content_for_text_resources() {
        let reference = ResourceReference {
            node_id: "t".into(),
            kind: ResourceKind::Text,
            label: "Text 1".into(),
            title: "Notes".into(),
            text: Some("line one\nline two".into()),
        };
        assert_eq!(reference.chip_text(), "line one line two");
        let reference = ResourceReference {
            text: None,
            ..reference
        };
        assert_eq!(reference.chip_text(), "Text 1");
    }
}
