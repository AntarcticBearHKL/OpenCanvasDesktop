//! Node card content view data: the media info bar line, the video duration
//! label, the image-stack contact sheet, the assets list and the related/focus
//! highlight set.
//!
//! Provenance: `canvas-node.tsx` `MediaInfoBar` (lines 759-791: `title · WxH ·
//! bytes · cost`, cost `~$` when estimated), `ImageStackNodeContent` (cover +
//! backing thumbs, count chip, `view all`), `assets-node-content.tsx` (rows with
//! name + kind, count footer, empty state), `VideoNodeContent` (poster or the
//! empty video placeholder), and `project.tsx` `relatedHighlight` (lines 569-588:
//! the active node plus every direct connection neighbour).

use std::collections::BTreeSet;

use open_canvas_ai::generation_cost::{estimate_generation_cost, format_usd, GenerationCostUnit};
use open_canvas_core::document::LibraryIndex;
use open_canvas_core::media_size::clamp_video_seconds;
use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType,
};

use crate::media::MediaCache;
use crate::{AssetItem, StackImage};

/// `formatBytes` (image-utils.ts:4-16): "" for zero/absent sizes, one decimal
/// below 10 within a unit.
#[expect(
    clippy::cast_precision_loss,
    reason = "byte counts stay far below 2^53, so the f64 conversion is exact for every size the UI can show"
)]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if bytes == 0 {
        return String::new();
    }
    let mut value = bytes as f64;
    let mut unit = 0_usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let text = if value >= 10.0 || unit == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    format!("{} {}", text, UNITS[unit])
}

/// Object-fit of an image node body. Image nodes are always `contain`: their
/// card geometry is locked to the bitmap's aspect ratio, so `contain` fills the
/// card exactly and never crops or stretches the picture. Other media keep the
/// legacy free-resize flag for backward-compatible documents.
pub fn media_fit(node: &CanvasNodeData) -> &'static str {
    if node.node_type == CanvasNodeType::Image {
        return "contain";
    }
    let free = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.free_resize)
        .unwrap_or(false);
    if free {
        "fill"
    } else {
        "contain"
    }
}

/// Slint-side status tag: only `loading`/`error` render a state surface; idle
/// and success show the content (`canvas-node.tsx:496-497`).
pub fn status_id(status: Option<CanvasNodeStatus>) -> &'static str {
    match status {
        Some(CanvasNodeStatus::Loading) => "loading",
        Some(CanvasNodeStatus::Error) => "error",
        _ => "",
    }
}

/// Human status word for the media strips: idle renders nothing (the web's
/// records only exist once a generation ran).
pub fn status_label(status: Option<CanvasNodeStatus>) -> &'static str {
    match status {
        Some(CanvasNodeStatus::Loading) => "Generating",
        Some(CanvasNodeStatus::Success) => "Ready",
        Some(CanvasNodeStatus::Error) => "Failed",
        _ => "",
    }
}

/// Human duration of the video metadata (`"0:06"`, `"1:32"`); "" when absent
/// or non-positive.
pub fn duration_label(duration_ms: Option<f64>) -> String {
    let Some(ms) = duration_ms.filter(|value| value.is_finite() && *value > 0.0) else {
        return String::new();
    };
    let total = (ms / 1000.0).round().max(0.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a clamped, finite, non-negative second count; i32 covers any plausible media duration"
    )]
    let seconds = total as u32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The `(model, unit, quantity)` a node's generation is priced by: shared by
/// the info bar's estimate text and the cost-record hook on completion.
pub fn node_cost_inputs(node: &CanvasNodeData) -> Option<(String, GenerationCostUnit, f64)> {
    let meta = node.metadata.as_ref()?;
    let model = meta
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())?;
    let (unit, quantity) = match node.node_type {
        CanvasNodeType::ImageGeneration | CanvasNodeType::Image | CanvasNodeType::ImageStack => (
            GenerationCostUnit::Image,
            meta.count.map_or(1.0, |count| {
                f64::from(i32::try_from(count).unwrap_or(1).max(1))
            }),
        ),
        CanvasNodeType::VideoGeneration | CanvasNodeType::Video => {
            let seconds = clamp_video_seconds(meta.seconds.as_deref().unwrap_or("6"));
            (
                GenerationCostUnit::VideoSecond,
                f64::from(i32::try_from(seconds).unwrap_or(6)),
            )
        }
        _ => return None,
    };
    Some((model, unit, quantity))
}

/// Estimated generation cost of a priced model: cinema "~$0.04" like the web's
/// estimate source; "" when the node carries no model or the model is unpriced
/// (`MediaInfoBar`: `costText` is empty unless the record is priced).
pub fn node_cost_text(node: &CanvasNodeData) -> String {
    let Some((model, unit, quantity)) = node_cost_inputs(node) else {
        return String::new();
    };
    let estimate = estimate_generation_cost(&model, unit, quantity);
    if !estimate.priced {
        return String::new();
    }
    format!("~{}", format_usd(estimate.usd))
}

/// The `MediaInfoBar` line: `title · W x H · bytes · cost`, dropping empty
/// parts (`canvas-node.tsx:762-768`). The desktop has no provider cost records,
/// so the cost part is always the client-side estimate.
pub fn media_info_line(node: &CanvasNodeData) -> String {
    let meta = node.metadata.as_ref();
    let width = meta
        .and_then(|m| m.natural_width)
        .unwrap_or(node.width)
        .round()
        .max(0.0);
    let height = meta
        .and_then(|m| m.natural_height)
        .unwrap_or(node.height)
        .round()
        .max(0.0);
    let info = if width > 0.0 && height > 0.0 {
        format!("{width:.0} x {height:.0}")
    } else {
        String::new()
    };
    let size = format_bytes(meta.and_then(|m| m.bytes).unwrap_or(0));
    let cost = node_cost_text(node);
    [node.title.trim().to_string(), info, size, cost]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<String>>()
        .join(" · ")
}

/// Active-node related set (`project.tsx:570-588`): the focus node itself plus
/// both endpoints of every connection touching it.
pub fn related_ids(active: Option<&str>, connections: &[CanvasConnection]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Some(active) = active else {
        return ids;
    };
    ids.insert(active.to_string());
    for connection in connections {
        if connection.from_node_id != active && connection.to_node_id != active {
            continue;
        }
        ids.insert(connection.from_node_id.clone());
        ids.insert(connection.to_node_id.clone());
    }
    ids
}

/// The image preview modal's content: the decoded bitmap, the media info line
/// and (for a stack) the gallery rows (`image-stack-preview.tsx`).
#[derive(Default)]
pub struct PreviewContent {
    pub image: slint::Image,
    pub info: String,
    pub rows: Vec<crate::StackGalleryRow>,
}

/// Builds the preview content of an image or image-stack node.
pub fn preview_content(node: &CanvasNodeData, cache: &mut MediaCache) -> PreviewContent {
    if node.node_type == CanvasNodeType::ImageStack {
        let sheet = stack_sheet(node, cache);
        let images = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.images.as_ref())
            .map_or(&[][..], Vec::as_slice);
        let cover_id = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.primary_image_id.as_deref())
            .unwrap_or_default();
        let rows = images
            .iter()
            .map(|image| crate::StackGalleryRow {
                id: image.id.as_str().into(),
                thumb: cache
                    .image_or_thumbnail(image.thumbnail.as_deref(), image.content.as_str())
                    .unwrap_or_default(),
                cover: image.id == cover_id,
                status: match image.status {
                    CanvasNodeStatus::Error => "error".into(),
                    CanvasNodeStatus::Loading => "loading".into(),
                    _ => slint::SharedString::default(),
                },
                error: image_error(image).into(),
            })
            .collect();
        return PreviewContent {
            image: sheet.cover,
            info: media_info_line(node),
            rows,
        };
    }
    let meta = node.metadata.as_ref();
    let thumbnail = meta.and_then(|meta| meta.thumbnail.as_deref());
    let content = meta
        .and_then(|meta| meta.content.as_deref())
        .unwrap_or_default();
    PreviewContent {
        image: cache
            .image_or_thumbnail(thumbnail, content)
            .unwrap_or_default(),
        info: media_info_line(node),
        rows: Vec::new(),
    }
}

/// The image-stack contact sheet: the decoded cover (`ImageStackNodeContent`
/// renders the cover on top, at slot 0), the cover's error line, and the
/// backing rows with their precomputed heap offsets.
#[derive(Default)]
pub struct StackSheet {
    pub cover: slint::Image,
    pub cover_error: String,
    pub rows: Vec<StackImage>,
    pub count: usize,
}

/// Builds the contact sheet of an image stack: the cover is the stored
/// `primaryImageId` (else the first entry); the backing rows follow in order,
/// capped at [`crate::vm::STACK_THUMBS`] total cards.
pub fn stack_sheet(node: &CanvasNodeData, cache: &mut MediaCache) -> StackSheet {
    let images = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.images.as_ref())
        .map_or(&[][..], Vec::as_slice);
    if images.is_empty() {
        return StackSheet::default();
    }
    let cover_id = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.primary_image_id.as_deref())
        .filter(|id| images.iter().any(|image| image.id == *id))
        .unwrap_or(images[0].id.as_str());
    let mut ordered: Vec<&open_canvas_core::node::CanvasNodeImage> =
        Vec::with_capacity(images.len());
    if let Some(cover) = images.iter().find(|image| image.id == cover_id) {
        ordered.push(cover);
    }
    ordered.extend(images.iter().filter(|image| image.id != cover_id));
    ordered.truncate(crate::vm::STACK_THUMBS);

    let cover = ordered.first().map(|image| {
        (
            cache
                .image_or_thumbnail(image.thumbnail.as_deref(), image.content.as_str())
                .unwrap_or_default(),
            image_error(image),
        )
    });
    let (cover_image, cover_error) = cover.unwrap_or_default();
    let rows = ordered
        .iter()
        .skip(1)
        .enumerate()
        .map(|(slot, image)| {
            let (dx, dy, angle) = stack_offset(slot + 1);
            StackImage {
                id: image.id.as_str().into(),
                thumb: cache
                    .image_or_thumbnail(image.thumbnail.as_deref(), image.content.as_str())
                    .unwrap_or_default(),
                dx,
                dy,
                angle,
            }
        })
        .collect();
    StackSheet {
        cover: cover_image,
        cover_error,
        rows,
        count: images.len(),
    }
}

/// `Generation failed` for an errored cover image, else "".
fn image_error(image: &open_canvas_core::node::CanvasNodeImage) -> String {
    if image.status == CanvasNodeStatus::Error {
        image
            .error_details
            .clone()
            .unwrap_or_else(|| "Generation failed".to_string())
    } else {
        String::new()
    }
}

/// Heap offset of one contact-sheet slot (`ImageStackNodeContent` line 42):
/// the cover sits at 0; backing card `i` drifts x by `±(i+1)*7`, y by `(i+1)*5`
/// and rotates by `±(2+i*1.4)` degrees, alternating sides.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "slot indices and their offsets are tiny; f32 is exact for every card the sheet renders"
)]
pub fn stack_offset(slot: usize) -> (f32, f32, f32) {
    if slot == 0 {
        return (0.0, 0.0, 0.0);
    }
    let index = (slot - 1) as f64;
    let sign = if (slot - 1).is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    (
        (((index + 1.0) * 7.0 * sign) as f32),
        (((index + 1.0) * 5.0) as f32),
        (((2.0 + index * 1.4) * sign) as f32),
    )
}

/// The bound-folder view of an assets node: the tree rows, whether a folder is
/// bound, the folder path (for the header) and the error/status line.
///
/// The folder is project data (`metadata.assetFolder`); the scan is cached by
/// `folders`. The tree is rooted at that folder and cannot navigate upward —
/// only expansion states travel, and they are relative names validated by
/// `asset_folder::resolve_in_root`.
pub fn asset_folder_view(
    node: &CanvasNodeData,
    folders: &mut crate::asset_folder::FolderCache,
    expanded: Option<&BTreeSet<String>>,
) -> (Vec<crate::AssetTreeRow>, bool, String, String, usize) {
    let Some(folder) = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.asset_folder.clone())
        .filter(|folder| !folder.trim().is_empty())
    else {
        return (Vec::new(), false, String::new(), String::new(), 0);
    };
    let empty = BTreeSet::new();
    let scan = folders.scan(&folder, expanded.unwrap_or(&empty));
    let rows = scan
        .rows
        .iter()
        .map(|row| crate::AssetTreeRow {
            path: row.path.as_str().into(),
            label: row.label.as_str().into(),
            depth: row.depth,
            kind: row.kind.into(),
            expanded: row.expanded,
        })
        .collect();
    (
        rows,
        true,
        folder,
        scan.status().unwrap_or_default(),
        scan.files,
    )
}

/// Assets rows for the desktop: the node's studio selection resolved against
/// the app library (the desktop's project stores are the "studio" source). An
/// id without a live project renders as a missing row instead of disappearing,
/// so a stale selection stays visible.
pub fn asset_rows(node: &CanvasNodeData, library: &LibraryIndex) -> Vec<AssetItem> {
    let Some(selection) = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.asset_studio_projects.as_ref())
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Some(ids) = selection.image.as_ref() {
        for id in ids {
            let project = library.projects.iter().find(|project| project.id == *id);
            let (name, sub) = match project {
                Some(project) => (
                    project.title.clone(),
                    group_name(library, project.group_id.as_deref()).unwrap_or_default(),
                ),
                None => (id.clone(), "Missing project".to_string()),
            };
            rows.push(AssetItem {
                id: id.as_str().into(),
                label: name.as_str().into(),
                sub: sub.as_str().into(),
                kind: "image".into(),
                thumb: slint::Image::default(),
                ready: project.is_some(),
            });
        }
    }
    rows
}

fn group_name(library: &LibraryIndex, group_id: Option<&str>) -> Option<String> {
    let group_id = group_id?;
    library
        .groups
        .iter()
        .find(|group| group.id == group_id)
        .map(|group| group.name.clone())
}

/// The assets footer count line (`assets-node-content.tsx:433-436`).
pub fn asset_count_label(count: usize) -> String {
    crate::vm::count_label(count, "item")
}

/// The assets source tag: `studio` when a studio selection exists, else the
/// stored source, else `folder` (the web default, line 53).
pub fn asset_source_id(node: &CanvasNodeData) -> &'static str {
    match node.metadata.as_ref().and_then(|meta| meta.asset_source) {
        Some(open_canvas_core::node::CanvasAssetSource::Cache) => "cache",
        Some(open_canvas_core::node::CanvasAssetSource::Studio) => "studio",
        _ => "folder",
    }
}

/// Text body a Text node displays: the primary batch text when present, else
/// the plain `content` (`canvas-node.tsx:569-571`).
pub fn text_body(meta: Option<&CanvasNodeMetadata>) -> String {
    let Some(meta) = meta else {
        return String::new();
    };
    let texts = meta.texts.as_ref();
    if let Some(texts) = texts {
        if !texts.is_empty() {
            let primary_id = meta
                .primary_text_id
                .as_deref()
                .unwrap_or(texts[0].id.as_str());
            if let Some(text) = texts.iter().find(|text| text.id == primary_id) {
                if !text.content.is_empty() {
                    return text.content.clone();
                }
            }
        }
    }
    meta.content.clone().unwrap_or_default()
}

/// Removes one reference id from `metadata.references` (the card's chip delete
/// affordance). Returns `false` when the id was not referenced.
pub fn remove_reference(node: &mut CanvasNodeData, reference_id: &str) -> bool {
    let Some(meta) = node.metadata.as_mut() else {
        return false;
    };
    let Some(references) = meta.references.as_mut() else {
        return false;
    };
    let before = references.len();
    references.retain(|id| id != reference_id);
    references.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::{
        CanvasAssetStudioProjects, CanvasNodeImage, CanvasNodeText, Position,
    };

    fn text_node(content: &str) -> CanvasNodeData {
        CanvasNodeData {
            id: "t".into(),
            node_type: CanvasNodeType::Text,
            title: "Text".into(),
            position: Position::new(0.0, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: Some(CanvasNodeMetadata {
                content: Some(content.to_string()),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn bytes_follow_the_web_formatting() {
        assert_eq!(format_bytes(0), "");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(20 * 1024), "20 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn info_line_joins_only_the_available_parts() {
        let mut node = text_node("");
        node.node_type = CanvasNodeType::Image;
        node.title = "Cover".into();
        node.metadata = Some(CanvasNodeMetadata {
            natural_width: Some(1024.0),
            natural_height: Some(768.0),
            bytes: Some(2 * 1024 * 1024),
            ..Default::default()
        });
        assert_eq!(media_info_line(&node), "Cover · 1024 x 768 · 2.0 MB");

        node.title = "  ".into();
        assert_eq!(media_info_line(&node), "1024 x 768 · 2.0 MB");

        node.metadata = Some(CanvasNodeMetadata::default());
        node.title = "Cover".into();
        assert_eq!(
            media_info_line(&node),
            "Cover · 340 x 240",
            "missing natural size falls back to the card size"
        );
    }

    #[test]
    fn cost_is_estimated_only_for_priced_models() {
        let mut node = text_node("");
        node.node_type = CanvasNodeType::ImageGeneration;
        assert_eq!(node_cost_text(&node), "", "no model, no cost");

        if let Some(meta) = node.metadata.as_mut() {
            meta.model = Some("gpt-image-1".into());
            meta.count = Some(2);
        }
        assert_eq!(
            node_cost_text(&node),
            "~$0.08",
            "two images at the client-side price estimate"
        );

        if let Some(meta) = node.metadata.as_mut() {
            meta.model = Some("unknown-model".into());
        }
        assert_eq!(node_cost_text(&node), "", "unpriced models stay empty");
    }

    #[test]
    fn duration_label_formats_minutes_and_seconds() {
        assert_eq!(duration_label(Some(6000.0)), "0:06");
        assert_eq!(duration_label(Some(92_000.0)), "1:32");
        assert_eq!(duration_label(Some(0.0)), "");
        assert_eq!(duration_label(None), "");
    }

    #[test]
    fn related_set_holds_the_focus_node_and_its_neighbours() {
        let connections = vec![
            CanvasConnection {
                id: "c1".into(),
                from_node_id: "prompt".into(),
                to_node_id: "image".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
            CanvasConnection {
                id: "c2".into(),
                from_node_id: "other".into(),
                to_node_id: "unrelated".into(),
                relation: None,
                kind: None,
                reason: None,
                changes: None,
                agent_run_id: None,
            },
        ];
        let related = related_ids(Some("image"), &connections);
        assert!(related.contains("image"));
        assert!(related.contains("prompt"));
        assert!(!related.contains("other"));
        assert!(related_ids(None, &connections).is_empty());
    }

    #[test]
    fn text_body_prefers_the_primary_batch_text() {
        let mut node = text_node("plain");
        assert_eq!(text_body(node.metadata.as_ref()), "plain");
        if let Some(meta) = node.metadata.as_mut() {
            meta.texts = Some(vec![
                CanvasNodeText {
                    id: "a".into(),
                    status: CanvasNodeStatus::Success,
                    error_details: None,
                    content: "first".into(),
                },
                CanvasNodeText {
                    id: "b".into(),
                    status: CanvasNodeStatus::Success,
                    error_details: None,
                    content: "second".into(),
                },
            ]);
            meta.primary_text_id = Some("b".into());
        }
        assert_eq!(text_body(node.metadata.as_ref()), "second");
        assert_eq!(text_body(None), "");
    }

    #[test]
    fn stack_sheet_puts_the_cover_first_and_caps_the_cards() {
        let mut node = text_node("");
        node.node_type = CanvasNodeType::ImageStack;
        let image = |id: &str| CanvasNodeImage {
            id: id.into(),
            status: CanvasNodeStatus::Success,
            error_details: None,
            content: String::new(),
            storage_key: None,
            thumbnail: None,
            thumbnail_key: None,
            natural_width: 4.0,
            natural_height: 4.0,
            bytes: 0,
            mime_type: "image/png".into(),
        };
        if let Some(meta) = node.metadata.as_mut() {
            meta.images = Some(vec![image("a"), image("b"), image("c"), image("d")]);
            meta.primary_image_id = Some("c".into());
        }
        let mut cache = MediaCache::default();
        let sheet = stack_sheet(&node, &mut cache);
        assert_eq!(sheet.count, 4);
        assert_eq!(sheet.rows.len(), crate::vm::STACK_THUMBS - 1);
        assert_eq!(
            sheet.rows[0].id, "a",
            "the cover leads, the rest follow in order"
        );
        assert_eq!(sheet.rows[0].dx, 7.0, "backing card 0 drifts right");

        let empty = stack_sheet(&text_node(""), &mut cache);
        assert_eq!(empty.count, 0);
        assert_eq!(empty.rows.len(), 0);
    }

    #[test]
    fn stack_heap_offsets_alternate_sides() {
        assert_eq!(stack_offset(0), (0.0, 0.0, 0.0));
        assert_eq!(stack_offset(1), (7.0, 5.0, 2.0));
        let (dx, dy, angle) = stack_offset(2);
        assert_eq!(dx, -14.0);
        assert_eq!(dy, 10.0);
        assert!(angle < 0.0);
    }

    #[test]
    fn asset_rows_resolve_library_projects_and_keep_missing_ones() {
        let mut library = LibraryIndex::new();
        let group = open_canvas_core::document::CanvasGroup {
            id: "g1".into(),
            name: "Sketches".into(),
            created_at: String::new(),
        };
        library.groups.push(group);
        library
            .projects
            .push(open_canvas_core::document::ProjectIndexEntry {
                id: "p1".into(),
                title: "Hero".into(),
                created_at: String::new(),
                updated_at: String::new(),
                group_id: Some("g1".into()),
                kind: "image".into(),
            });
        let mut node = text_node("");
        node.node_type = CanvasNodeType::Assets;
        if let Some(meta) = node.metadata.as_mut() {
            meta.asset_studio_projects = Some(CanvasAssetStudioProjects {
                image: Some(vec!["p1".into(), "gone".into()]),
                pixel: None,
            });
        }
        let rows = asset_rows(&node, &library);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label.as_str(), "Hero");
        assert_eq!(rows[0].sub.as_str(), "Sketches");
        assert!(rows[0].ready);
        assert_eq!(rows[1].label.as_str(), "gone");
        assert!(!rows[1].ready, "a stale selection stays visible as missing");
        assert_eq!(asset_count_label(rows.len()), "2 items");
        assert_eq!(asset_count_label(1), "1 item");
    }

    #[test]
    fn reference_removal_is_scoped_to_the_referenced_id() {
        let mut node = text_node("");
        if let Some(meta) = node.metadata.as_mut() {
            meta.references = Some(vec!["a".into(), "b".into()]);
        }
        assert!(remove_reference(&mut node, "a"));
        let left = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.references.clone())
            .unwrap_or_default();
        assert_eq!(left, vec!["b".to_string()]);
        assert!(!remove_reference(&mut node, "a"));
    }
}
