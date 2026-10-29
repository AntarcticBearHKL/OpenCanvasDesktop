//! Node drop bindings and application, mirroring
//! `web/src/lib/canvas/canvas-drop-bindings.ts`, `canvas-video-slots.ts`,
//! `canvas-node-geometry.ts` (`findAssetsDropTarget` / `findImageStackDropTarget`)
//! and the `finishNodeDrag` branches of project.tsx:900-1000.
//!
//! Desktop notes: "collect" onto an assets node records the source node in the
//! target's `metadata.references` (the desktop assets node reads the app
//! library, so there is no asset folder to write into), and an image-stack drop
//! moves the image payload into the stack's `metadata.images` and consumes the
//! source node, exactly like the web.

use open_canvas_core::geometry::{is_node_hidden, MovedNode};
use open_canvas_core::node::{
    CanvasNodeData, CanvasNodeImage, CanvasNodeStatus, CanvasNodeType, CanvasVideoSlots, Position,
};

use crate::coords::WorldPos;
use crate::doc::Doc;

/// Drop rule kinds (`canvas-drop-bindings.ts:4`); the desktop rules table only
/// resolves `collect` and `slot`, mirroring the web's `CANVAS_DROP_RULES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropBinding {
    Collect,
    Slot,
}

/// Video keyframe/reference slot (`CanvasVideoSlot`, canvas.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoSlot {
    FirstFrame,
    LastFrame,
    Reference,
}

impl VideoSlot {
    pub fn id(self) -> &'static str {
        match self {
            Self::FirstFrame => "firstFrame",
            Self::LastFrame => "lastFrame",
            Self::Reference => "reference",
        }
    }

    #[cfg(test)]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "firstFrame" => Some(Self::FirstFrame),
            "lastFrame" => Some(Self::LastFrame),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }
}

/// Height of the keyframe slot band at the bottom of a video-prompt card (the
/// two 96px `FrameSlot`s plus their spacing).
pub const SLOT_BAND_HEIGHT: f64 = 112.0;
/// Per-kind reference limits (`VIDEO_REFERENCE_LIMITS`, video-generation.ts:32).
pub fn reference_limit(kind: &str) -> usize {
    match kind {
        "video" | "audio" => 3,
        _ => 9,
    }
}
/// Mixed reference limit (`VIDEO_REFERENCE_TOTAL_LIMIT`).
pub const REFERENCE_TOTAL_LIMIT: usize = 12;

/// The web's `CANVAS_DROP_RULES` table.
pub fn resolve_drop_binding(
    source: &CanvasNodeType,
    target: &CanvasNodeType,
) -> Option<DropBinding> {
    match (source, target) {
        (CanvasNodeType::Image, CanvasNodeType::Assets | CanvasNodeType::ImageStack) => {
            Some(DropBinding::Collect)
        }
        (CanvasNodeType::Image | CanvasNodeType::Video, CanvasNodeType::VideoPrompt) => {
            Some(DropBinding::Slot)
        }
        _ => None,
    }
}

/// Node types that can be dragged with a drop ghost
/// (`isCanvasDropSourceType`, canvas-drop-bindings.ts:20-22).
pub fn is_drop_source_type(node_type: &CanvasNodeType) -> bool {
    matches!(node_type, CanvasNodeType::Image | CanvasNodeType::Video)
}

/// Kind of a node attached to a video slot (`videoReferenceKind`).
pub fn video_reference_kind(node_type: &CanvasNodeType) -> Option<&'static str> {
    match node_type {
        CanvasNodeType::Image => Some("image"),
        CanvasNodeType::Video => Some("video"),
        _ => None,
    }
}

/// The resolved drop destination.
#[derive(Debug, Clone, PartialEq)]
pub struct DropTarget {
    pub node_id: String,
    pub binding: DropBinding,
    pub slot: Option<VideoSlot>,
}

/// Slot zone under the pointer: the bottom band of the card splits left =
/// first frame, right = last frame; everything else is the reference area.
pub fn video_slot_zone(point: WorldPos, node: &CanvasNodeData) -> VideoSlot {
    let band_top = node.position.y + node.height - SLOT_BAND_HEIGHT;
    if point.y >= band_top {
        if point.x < node.position.x + node.width / 2.0 {
            VideoSlot::FirstFrame
        } else {
            VideoSlot::LastFrame
        }
    } else {
        VideoSlot::Reference
    }
}

fn contains(node: &CanvasNodeData, point: WorldPos) -> bool {
    point.x >= node.position.x
        && point.x <= node.position.x + node.width
        && point.y >= node.position.y
        && point.y <= node.position.y + node.height
}

fn center_inside(dragged: &CanvasNodeData, target: &CanvasNodeData) -> bool {
    open_canvas_core::geometry::node_center_inside(dragged, target)
}

fn has_content(node: &CanvasNodeData) -> bool {
    node.metadata
        .as_ref()
        .and_then(|meta| meta.content.as_deref())
        .is_some_and(|content| !content.is_empty())
}

/// Resolves the drop destination for a drag: the video slot under the pointer
/// wins, then the topmost assets target, then the topmost image-stack target
/// (project.tsx:1074-1076).
pub fn resolve_drop_target(
    moved_ids: &[String],
    nodes: &[CanvasNodeData],
    pointer: WorldPos,
) -> Option<DropTarget> {
    let dragged: Vec<&CanvasNodeData> = nodes
        .iter()
        .filter(|node| moved_ids.iter().any(|id| id == &node.id))
        .collect();
    if dragged.is_empty() {
        return None;
    }
    let slot_source = dragged
        .iter()
        .find(|node| is_drop_source_type(&node.node_type) && has_content(node));
    if let Some(source) = slot_source {
        let source_kind = video_reference_kind(&source.node_type).unwrap_or("");
        for node in nodes.iter().rev() {
            if node.node_type != CanvasNodeType::VideoPrompt
                || moved_ids.iter().any(|id| id == &node.id)
                || is_node_hidden(node)
                || !contains(node, pointer)
            {
                continue;
            }
            let zone = video_slot_zone(pointer, node);
            let kind_ok = match zone {
                VideoSlot::FirstFrame | VideoSlot::LastFrame => source_kind == "image",
                VideoSlot::Reference => true,
            };
            if kind_ok {
                return Some(DropTarget {
                    node_id: node.id.clone(),
                    binding: DropBinding::Slot,
                    slot: Some(zone),
                });
            }
        }
    }
    let collect_target = |target_type: CanvasNodeType| -> Option<DropTarget> {
        nodes
            .iter()
            .rev()
            .find(|node| {
                if node.node_type != target_type
                    || moved_ids.iter().any(|id| id == &node.id)
                    || is_node_hidden(node)
                {
                    return false;
                }
                dragged.iter().any(|source| {
                    resolve_drop_binding(&source.node_type, &node.node_type)
                        == Some(DropBinding::Collect)
                        && center_inside(source, node)
                })
            })
            .map(|node| DropTarget {
                node_id: node.id.clone(),
                binding: DropBinding::Collect,
                slot: None,
            })
    };
    collect_target(CanvasNodeType::Assets).or_else(|| collect_target(CanvasNodeType::ImageStack))
}

/// Frame-image limit of the model behind a video prompt
/// (`videoPromptFrameLimit` + `videoFrameImageLimit`): the two `MiniMax` models
/// accept a single keyframe, every other/unknown model falls back to two.
pub fn video_frame_limit(model: Option<&str>) -> usize {
    match model {
        Some("minimax/hailuo-3-max" | "minimax/hailuo-3") | None => 1,
        Some(_) => 2,
    }
}

impl Doc {
    /// The frame-image limit through the prompt's outgoing video-generation
    /// connection (project.tsx:124-127): unknown wiring uses the default model.
    pub fn prompt_frame_limit(&self, prompt_id: &str) -> usize {
        let generation = self.connections.iter().find_map(|connection| {
            if connection.from_node_id != prompt_id {
                return None;
            }
            self.nodes.iter().find(|node| {
                node.id == connection.to_node_id
                    && node.node_type == CanvasNodeType::VideoGeneration
            })
        });
        let model = generation
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.model.as_deref());
        video_frame_limit(model)
    }

    /// `bindVideoSlot` (canvas-video-slots.ts:19-40): keyframe slots take images
    /// only and respect the single-keyframe model rule; references append within
    /// the per-kind and total limits. `None` when the binding is illegal.
    pub fn bind_video_slot(
        &self,
        target_id: &str,
        slot: VideoSlot,
        source_id: &str,
    ) -> Option<CanvasVideoSlots> {
        let target = self
            .nodes
            .iter()
            .find(|node| node.id == target_id && node.node_type == CanvasNodeType::VideoPrompt)?;
        let source = self.nodes.iter().find(|node| node.id == source_id)?;
        if target.id == source.id {
            return None;
        }
        let kind = video_reference_kind(&source.node_type)?;
        let current = target
            .metadata
            .as_ref()
            .and_then(|meta| meta.video_slots.clone())
            .unwrap_or_default();
        let single_keyframe = self.prompt_frame_limit(target_id) <= 1;
        match slot {
            VideoSlot::FirstFrame => (kind == "image").then(|| CanvasVideoSlots {
                first_frame: Some(source_id.to_string()),
                last_frame: if single_keyframe {
                    None
                } else {
                    current.last_frame.clone()
                },
                references: current.references.clone(),
            }),
            VideoSlot::LastFrame => (kind == "image").then(|| CanvasVideoSlots {
                first_frame: if single_keyframe {
                    None
                } else {
                    current.first_frame.clone()
                },
                last_frame: Some(source_id.to_string()),
                references: current.references.clone(),
            }),
            VideoSlot::Reference => {
                let references = current.references.clone().unwrap_or_default();
                if references.iter().any(|id| id == source_id)
                    || references.len() >= REFERENCE_TOTAL_LIMIT
                {
                    return None;
                }
                let same_kind = references
                    .iter()
                    .filter(|id| {
                        self.nodes
                            .iter()
                            .find(|node| node.id == **id)
                            .and_then(|node| video_reference_kind(&node.node_type))
                            == Some(kind)
                    })
                    .count();
                if same_kind >= reference_limit(kind) {
                    return None;
                }
                let mut references = references;
                references.push(source_id.to_string());
                Some(CanvasVideoSlots {
                    first_frame: current.first_frame.clone(),
                    last_frame: current.last_frame.clone(),
                    references: Some(references),
                })
            }
        }
    }

    /// Restores dragged nodes to their pre-drag origins (`Drag::Nodes.initial`).
    fn restore_moved(&mut self, moved: &[MovedNode]) {
        for node in &mut self.nodes {
            if let Some(origin) = moved.iter().find(|origin| origin.id == node.id) {
                node.position = Position::new(origin.x, origin.y);
            }
        }
    }

    /// Applies a resolved drop (project.tsx `finishNodeDrag` branches). Dragged
    /// nodes spring back to their original positions; a stack drop consumes the
    /// image nodes it absorbed. Returns whether the document changed.
    pub fn apply_drop(&mut self, target: &DropTarget, moved: &[MovedNode], now_ms: u64) -> bool {
        let moved_ids: Vec<String> = moved.iter().map(|node| node.id.clone()).collect();
        match target.binding {
            DropBinding::Collect
                if target_is(&target.node_id, &self.nodes, &CanvasNodeType::Assets) =>
            {
                let additions: Vec<String> = moved
                    .iter()
                    .filter(|origin| {
                        self.nodes.iter().any(|node| {
                            node.id == origin.id && node.node_type == CanvasNodeType::Image
                        })
                    })
                    .map(|origin| origin.id.clone())
                    .collect();
                self.restore_moved(moved);
                self.append_references(&target.node_id, &additions)
            }
            DropBinding::Collect => self.stack_absorb(&target.node_id, &moved_ids, now_ms),
            DropBinding::Slot => {
                let Some(slot) = target.slot else {
                    return false;
                };
                let source = self.nodes.iter().find(|node| {
                    moved_ids.iter().any(|id| id == &node.id)
                        && is_drop_source_type(&node.node_type)
                        && has_content(node)
                });
                let Some(source_id) = source.map(|node| node.id.clone()) else {
                    return false;
                };
                let Some(slots) = self.bind_video_slot(&target.node_id, slot, &source_id) else {
                    return false;
                };
                if let Some(node) = self.nodes.iter_mut().find(|node| node.id == target.node_id) {
                    let metadata = node
                        .metadata
                        .get_or_insert_with(open_canvas_core::node::CanvasNodeMetadata::default);
                    metadata.video_slots = Some(slots);
                }
                self.restore_moved(moved);
                self.commit(now_ms);
                true
            }
        }
    }

    /// Adds source ids to an assets node's `metadata.references`, de-duplicating.
    fn append_references(&mut self, target_id: &str, additions: &[String]) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == target_id) else {
            return false;
        };
        let metadata = node
            .metadata
            .get_or_insert_with(open_canvas_core::node::CanvasNodeMetadata::default);
        let references = metadata.references.get_or_insert_with(Vec::new);
        let before = references.len();
        for id in additions {
            if !references.iter().any(|existing| existing == id) {
                references.push(id.clone());
            }
        }
        references.len() != before
    }

    /// Image-stack consumption: appends the dragged images' payloads, re-points
    /// `primaryImageId`/`count`, deletes the consumed nodes and their wires and
    /// selects the stack.
    fn stack_absorb(&mut self, target_id: &str, moved_ids: &[String], now_ms: u64) -> bool {
        let target_is_stack = self
            .nodes
            .iter()
            .any(|node| node.id == target_id && node.node_type == CanvasNodeType::ImageStack);
        if !target_is_stack {
            return false;
        }
        let mut appended: Vec<CanvasNodeImage> = Vec::new();
        let mut consumed: Vec<String> = Vec::new();
        for id in moved_ids {
            let Some(node) = self.nodes.iter().find(|node| node.id == *id) else {
                continue;
            };
            if node.node_type != CanvasNodeType::Image {
                continue;
            }
            let Some(meta) = node.metadata.as_ref() else {
                continue;
            };
            let Some(content) = meta.content.clone().filter(|content| !content.is_empty()) else {
                continue;
            };
            self.seq = self.seq.wrapping_add(1);
            appended.push(CanvasNodeImage {
                id: format!("img-{now_ms}-{}", self.seq),
                status: CanvasNodeStatus::Success,
                error_details: None,
                content,
                storage_key: meta.storage_key.clone(),
                thumbnail: meta.thumbnail.clone(),
                thumbnail_key: meta.thumbnail_key.clone(),
                natural_width: meta.natural_width.unwrap_or(node.width),
                natural_height: meta.natural_height.unwrap_or(node.height),
                bytes: meta.bytes.unwrap_or(0),
                mime_type: meta
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| "image/png".to_string()),
            });
            consumed.push(node.id.clone());
        }
        if appended.is_empty() {
            return false;
        }
        let first_new_id = appended
            .first()
            .map(|image| image.id.clone())
            .unwrap_or_default();
        if let Some(stack) = self.nodes.iter_mut().find(|node| node.id == target_id) {
            let metadata = stack
                .metadata
                .get_or_insert_with(open_canvas_core::node::CanvasNodeMetadata::default);
            let images = metadata.images.get_or_insert_with(Vec::new);
            images.extend(appended);
            metadata.count = Some(i64::try_from(images.len()).unwrap_or(i64::MAX));
            if metadata.primary_image_id.is_none() {
                metadata.primary_image_id = Some(first_new_id);
            }
        }
        self.nodes.retain(|node| !consumed.contains(&node.id));
        self.connections.retain(|connection| {
            !consumed.contains(&connection.from_node_id)
                && !consumed.contains(&connection.to_node_id)
        });
        self.selection.clear();
        self.selection.insert(target_id.to_string());
        self.selected_connection = None;
        self.commit(now_ms);
        true
    }
}

fn target_is(id: &str, nodes: &[CanvasNodeData], node_type: &CanvasNodeType) -> bool {
    nodes
        .iter()
        .any(|node| node.id == id && &node.node_type == node_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::CanvasNodeMetadata;

    fn node(
        id: &str,
        kind: CanvasNodeType,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        patch: &dyn Fn(&mut CanvasNodeMetadata),
    ) -> CanvasNodeData {
        let mut metadata = CanvasNodeMetadata::default();
        patch(&mut metadata);
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(x, y),
            width,
            height,
            metadata: Some(metadata),
        }
    }

    fn image(id: &str, x: f64, y: f64) -> CanvasNodeData {
        node(id, CanvasNodeType::Image, x, y, 200.0, 200.0, &|meta| {
            meta.content = Some("data:image/png;base64,AAAA".into());
            meta.natural_width = Some(512.0);
            meta.natural_height = Some(512.0);
        })
    }

    fn video(id: &str, x: f64, y: f64) -> CanvasNodeData {
        node(id, CanvasNodeType::Video, x, y, 200.0, 200.0, &|meta| {
            meta.content = Some("data:video/mp4;base64,AAAA".into());
        })
    }

    fn empty(id: &str, kind: CanvasNodeType, x: f64, y: f64, w: f64, h: f64) -> CanvasNodeData {
        node(id, kind, x, y, w, h, &|_| {})
    }

    fn moved(ids: &[&str], nodes: &[CanvasNodeData]) -> Vec<MovedNode> {
        ids.iter()
            .map(|id| {
                let position = nodes
                    .iter()
                    .find(|node| node.id == *id)
                    .map_or(Position::new(0.0, 0.0), |node| node.position);
                MovedNode {
                    id: (*id).to_string(),
                    x: position.x,
                    y: position.y,
                }
            })
            .collect()
    }

    #[test]
    fn binding_table_matches_the_web_rules() {
        use CanvasNodeType as T;
        assert_eq!(
            resolve_drop_binding(&T::Image, &T::Assets),
            Some(DropBinding::Collect)
        );
        assert_eq!(
            resolve_drop_binding(&T::Image, &T::ImageStack),
            Some(DropBinding::Collect)
        );
        assert_eq!(
            resolve_drop_binding(&T::Image, &T::VideoPrompt),
            Some(DropBinding::Slot)
        );
        assert_eq!(
            resolve_drop_binding(&T::Video, &T::VideoPrompt),
            Some(DropBinding::Slot)
        );
        assert_eq!(resolve_drop_binding(&T::Prompt, &T::Assets), None);
        assert_eq!(resolve_drop_binding(&T::Image, &T::Text), None);
        assert!(is_drop_source_type(&T::Image));
        assert!(!is_drop_source_type(&T::Prompt));
        assert_eq!(video_reference_kind(&T::Video), Some("video"));
        assert_eq!(video_reference_kind(&T::Text), None);
    }

    #[test]
    fn slot_zones_split_the_bottom_band() {
        let prompt = empty("vp", CanvasNodeType::VideoPrompt, 0.0, 0.0, 412.0, 576.0);
        // Band top = 576 - 112 = 464.
        assert_eq!(
            video_slot_zone(WorldPos::new(100.0, 500.0), &prompt),
            VideoSlot::FirstFrame
        );
        assert_eq!(
            video_slot_zone(WorldPos::new(300.0, 500.0), &prompt),
            VideoSlot::LastFrame
        );
        assert_eq!(
            video_slot_zone(WorldPos::new(200.0, 200.0), &prompt),
            VideoSlot::Reference
        );
        assert_eq!(VideoSlot::FirstFrame.id(), "firstFrame");
        assert_eq!(VideoSlot::from_id("lastFrame"), Some(VideoSlot::LastFrame));
        assert_eq!(VideoSlot::from_id("nope"), None);
    }

    #[test]
    fn drop_target_resolution_prefers_slots_then_assets_then_stacks() {
        let nodes = vec![
            image("img", 0.0, 0.0),
            empty("vp", CanvasNodeType::VideoPrompt, 300.0, 0.0, 412.0, 576.0),
            empty("assets", CanvasNodeType::Assets, 0.0, 400.0, 300.0, 300.0),
            empty(
                "stack",
                CanvasNodeType::ImageStack,
                400.0,
                700.0,
                300.0,
                300.0,
            ),
        ];
        let moved_ids = vec!["img".to_string()];

        // Pointer inside the video-prompt reference area.
        let target = resolve_drop_target(&moved_ids, &nodes, WorldPos::new(500.0, 200.0));
        assert_eq!(
            target,
            Some(DropTarget {
                node_id: "vp".into(),
                binding: DropBinding::Slot,
                slot: Some(VideoSlot::Reference),
            })
        );

        // Drag the image over the assets node: its center (100,100 after move) is
        // inside only when we pretend the pointer drop position; use a moved node
        // positioned over the assets node.
        let mut over_assets = nodes.clone();
        if let Some(image) = over_assets.iter_mut().find(|node| node.id == "img") {
            image.position = Position::new(150.0, 500.0);
        }
        let target = resolve_drop_target(&moved_ids, &over_assets, WorldPos::new(150.0, 500.0));
        assert_eq!(
            target,
            Some(DropTarget {
                node_id: "assets".into(),
                binding: DropBinding::Collect,
                slot: None,
            })
        );

        // Over the stack.
        let mut over_stack = nodes.clone();
        if let Some(image) = over_stack.iter_mut().find(|node| node.id == "img") {
            image.position = Position::new(500.0, 800.0);
        }
        let target = resolve_drop_target(&moved_ids, &over_stack, WorldPos::new(500.0, 800.0));
        assert_eq!(
            target,
            Some(DropTarget {
                node_id: "stack".into(),
                binding: DropBinding::Collect,
                slot: None,
            })
        );

        // Nothing under the pointer.
        assert_eq!(
            resolve_drop_target(&moved_ids, &nodes, WorldPos::new(5000.0, 5000.0)),
            None
        );
    }

    #[test]
    fn frame_zones_reject_video_sources_but_accept_reference_area() {
        let nodes = vec![
            video("vid", 0.0, 0.0),
            empty("vp", CanvasNodeType::VideoPrompt, 300.0, 0.0, 412.0, 576.0),
        ];
        let moved_ids = vec!["vid".to_string()];
        // Bottom band with a video source: no frame slot matches.
        assert_eq!(
            resolve_drop_target(&moved_ids, &nodes, WorldPos::new(400.0, 500.0)),
            None
        );
        // Reference area accepts the video.
        let target = resolve_drop_target(&moved_ids, &nodes, WorldPos::new(400.0, 200.0));
        assert_eq!(
            target.and_then(|target| target.slot),
            Some(VideoSlot::Reference)
        );
    }

    #[test]
    fn frame_limit_follows_the_model() {
        assert_eq!(video_frame_limit(None), 1);
        assert_eq!(video_frame_limit(Some("minimax/hailuo-3-max")), 1);
        assert_eq!(video_frame_limit(Some("minimax/hailuo-3")), 1);
        assert_eq!(video_frame_limit(Some("plugin/model-x")), 2);
    }

    #[test]
    fn single_keyframe_model_clears_the_paired_frame() {
        let nodes = vec![
            image("img-a", 0.0, 0.0),
            image("img-b", 0.0, 300.0),
            empty("vp", CanvasNodeType::VideoPrompt, 400.0, 0.0, 412.0, 576.0),
        ];
        let doc = Doc::assemble(nodes, Vec::new());
        let slots = doc.bind_video_slot("vp", VideoSlot::FirstFrame, "img-a");
        assert_eq!(
            slots.and_then(|slots| slots.first_frame),
            Some("img-a".into())
        );

        // Setting the last frame afterwards replaces the single keyframe pair.
        let mut doc = doc;
        if let Some(node) = doc.nodes.iter_mut().find(|node| node.id == "vp") {
            if let Some(meta) = node.metadata.as_mut() {
                meta.video_slots = Some(CanvasVideoSlots {
                    first_frame: Some("img-a".into()),
                    last_frame: None,
                    references: None,
                });
            }
        }
        let slots = doc
            .bind_video_slot("vp", VideoSlot::LastFrame, "img-b")
            .unwrap_or_default();
        assert_eq!(slots.last_frame, Some("img-b".into()));
        assert_eq!(slots.first_frame, None, "the paired keyframe is cleared");
    }

    #[test]
    fn reference_slots_enforce_kind_and_total_limits() {
        let mut nodes = vec![empty(
            "vp",
            CanvasNodeType::VideoPrompt,
            0.0,
            0.0,
            412.0,
            576.0,
        )];
        for index in 0..10 {
            nodes.push(image(
                &format!("img-{index}"),
                f64::from(index) * 250.0,
                -300.0,
            ));
        }
        nodes.push(video("vid", 0.0, -300.0));
        let mut doc = Doc::assemble(nodes, Vec::new());

        // Nine images bind; the tenth is rejected by the per-kind limit.
        for index in 0..9 {
            let slots = doc.bind_video_slot("vp", VideoSlot::Reference, &format!("img-{index}"));
            assert!(slots.is_some(), "image {index} binds");
            if let Some(slots) = slots {
                if let Some(node) = doc.nodes.iter_mut().find(|node| node.id == "vp") {
                    if let Some(meta) = node.metadata.as_mut() {
                        meta.video_slots = Some(slots);
                    }
                }
            }
        }
        assert_eq!(
            doc.bind_video_slot("vp", VideoSlot::Reference, "img-9"),
            None
        );
        // A video still fits within its own limit and the total.
        assert!(doc
            .bind_video_slot("vp", VideoSlot::Reference, "vid")
            .is_some());
        // Duplicate rejected.
        assert_eq!(
            doc.bind_video_slot("vp", VideoSlot::Reference, "img-0"),
            None
        );
        // The same node cannot bind to itself.
        assert_eq!(doc.bind_video_slot("vp", VideoSlot::Reference, "vp"), None);
        assert_eq!(reference_limit("image"), 9);
        assert_eq!(reference_limit("video"), 3);
        assert_eq!(REFERENCE_TOTAL_LIMIT, 12);
    }

    #[test]
    fn collect_onto_assets_records_references_and_restores_positions() {
        let nodes = vec![
            image("img", 900.0, 900.0),
            empty("assets", CanvasNodeType::Assets, 0.0, 0.0, 300.0, 300.0),
        ];
        let initial = moved(&["img"], &nodes);
        let mut doc = Doc::assemble(
            {
                let mut nodes = nodes.clone();
                if let Some(image) = nodes.iter_mut().find(|node| node.id == "img") {
                    image.position = Position::new(100.0, 100.0);
                }
                nodes
            },
            Vec::new(),
        );
        let target = DropTarget {
            node_id: "assets".into(),
            binding: DropBinding::Collect,
            slot: None,
        };
        assert!(doc.apply_drop(&target, &initial, 0));
        let references = doc
            .nodes
            .iter()
            .find(|node| node.id == "assets")
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.references.clone())
            .unwrap_or_default();
        assert_eq!(references, vec!["img".to_string()]);
        let position = doc
            .nodes
            .iter()
            .find(|node| node.id == "img")
            .map(|node| node.position.x);
        assert_eq!(position, Some(900.0), "the collected node springs back");
        // Second collect is a no-op (already referenced).
        assert!(!doc.apply_drop(&target, &initial, 0));
    }

    #[test]
    fn stack_drop_appends_images_and_consumes_the_sources() {
        let nodes = vec![
            image("img-a", 500.0, 500.0),
            image("img-b", 800.0, 500.0),
            empty("stack", CanvasNodeType::ImageStack, 0.0, 0.0, 300.0, 300.0),
        ];
        let initial = moved(&["img-a", "img-b"], &nodes);
        let mut doc = Doc::assemble(nodes.clone(), Vec::new());
        let target = DropTarget {
            node_id: "stack".into(),
            binding: DropBinding::Collect,
            slot: None,
        };
        assert!(doc.apply_drop(&target, &initial, 5_000));
        assert!(doc.node("img-a").is_none(), "consumed source is removed");
        assert!(doc.node("img-b").is_none());
        let stack = doc.node("stack").and_then(|node| node.metadata.as_ref());
        let images = stack
            .and_then(|meta| meta.images.clone())
            .unwrap_or_default();
        assert_eq!(images.len(), 2);
        assert_eq!(stack.and_then(|meta| meta.count), Some(2));
        assert_eq!(
            stack.and_then(|meta| meta.primary_image_id.clone()),
            Some(images[0].id.clone())
        );
        assert_eq!(images[0].natural_width, 512.0);
        assert_eq!(images[0].mime_type, "image/png");
        assert!(doc.selection().contains("stack"));
    }

    #[test]
    fn slot_drop_writes_slots_and_returns_the_node() {
        let nodes = vec![
            image("img", 100.0, 100.0),
            empty("vp", CanvasNodeType::VideoPrompt, 600.0, 0.0, 412.0, 576.0),
        ];
        let initial = moved(&["img"], &nodes);
        let mut doc = Doc::assemble(
            {
                let mut nodes = nodes.clone();
                if let Some(image) = nodes.iter_mut().find(|node| node.id == "img") {
                    image.position = Position::new(700.0, 300.0);
                }
                nodes
            },
            Vec::new(),
        );
        let target = DropTarget {
            node_id: "vp".into(),
            binding: DropBinding::Slot,
            slot: Some(VideoSlot::FirstFrame),
        };
        assert!(doc.apply_drop(&target, &initial, 6_000));
        let slots = doc
            .node("vp")
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.video_slots.clone())
            .unwrap_or_default();
        assert_eq!(slots.first_frame.as_deref(), Some("img"));
        assert_eq!(
            doc.node("img").map(|node| node.position.x),
            Some(100.0),
            "the bound node returns to its origin"
        );
        assert!(
            doc.node("img").is_some(),
            "slot binding never consumes nodes"
        );
    }

    #[test]
    fn slot_binding_requires_a_content_bearing_source() {
        let nodes = vec![
            node(
                "img",
                CanvasNodeType::Image,
                100.0,
                100.0,
                200.0,
                200.0,
                &|_| {},
            ),
            empty("vp", CanvasNodeType::VideoPrompt, 600.0, 0.0, 412.0, 576.0),
        ];
        let moved_ids = vec!["img".to_string()];
        // An empty image cannot arm the slot path (web `draggedVideoReferenceNode`).
        assert_eq!(
            resolve_drop_target(&moved_ids, &nodes, WorldPos::new(700.0, 300.0)),
            None
        );
    }
}
