//! Connection drags and the drop-target hit test.

use open_canvas_core::connections::{accepts_handle, has_connection, normalize_connection};
use open_canvas_core::geometry::is_node_hidden;
use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeType, ConnectionHandleType,
};

use crate::coords::WorldPos;
use crate::doc::{Doc, Drag};
impl Doc {
    pub fn begin_connect(&mut self, node_id: &str, handle: ConnectionHandleType) {
        self.connect_reject = None;
        // Handles are asymmetric: an Image starts a drag from its output
        // handle but has no target handle, and an Image Stack / Smart Canvas
        // has neither. A stray pointer event from an old render must not start
        // a drag either.
        if !self
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .is_some_and(|node| accepts_handle(&node.node_type, handle))
        {
            self.connect_reject = Some("Image nodes cannot be connected".to_string());
            return;
        }
        self.drag = Some(Drag::Connect {
            node_id: node_id.to_string(),
            handle,
            cursor: None,
            target: None,
            reject: None,
        });
    }

    /// Tracks the cursor during a connection drag and hit-tests the drop target
    /// (project.tsx:505-540: handle radius 40/k, node padding 32/k, inside wins).
    ///
    /// The legal target drives the highlight; a node the pair rules reject is
    /// remembered separately, so the release can explain the rejection instead
    /// of silently doing nothing.
    pub fn connect_to(&mut self, at: WorldPos) -> bool {
        let Some(Drag::Connect {
            node_id,
            handle,
            cursor,
            target,
            reject,
        }) = &mut self.drag
        else {
            return false;
        };
        *cursor = Some(at);
        let next = drop_target(&self.nodes, node_id, *handle, at, self.viewport.k);
        let blocked = if next.is_none() {
            hit_node(&self.nodes, node_id, *handle, at, self.viewport.k)
        } else {
            None
        };
        let changed = *target != next || *reject != blocked;
        *target = next;
        *reject = blocked;
        changed
    }

    /// Finishes a connection drag; creates the connection when legal and new.
    /// A rejected release records the reason in [`Doc::connect_reject`] so the
    /// caller can show it.
    pub fn end_connect(&mut self, now_ms: u64) -> bool {
        let Some(Drag::Connect {
            node_id,
            handle,
            target,
            reject,
            ..
        }) = self.drag.take()
        else {
            return false;
        };
        self.connect_reject = None;
        let Some(target_id) = target else {
            self.connect_reject =
                reject.and_then(|target_id| self.rejection_message(&node_id, &target_id));
            return false;
        };
        let Some(normalized) = normalize_connection(&node_id, &target_id, &self.nodes, handle)
        else {
            self.connect_reject = self.rejection_message(&node_id, &target_id);
            return false;
        };
        if has_connection(
            &self.connections,
            &normalized.from_node_id,
            &normalized.to_node_id,
        ) {
            return false;
        }
        // One derive parent per attempt; children are unlimited.
        let target_is_attempt = self
            .nodes
            .iter()
            .find(|node| node.id == normalized.to_node_id)
            .is_some_and(|node| node.node_type == CanvasNodeType::ImageAttempt);
        if target_is_attempt
            && self.connections.iter().any(|connection| {
                connection.to_node_id == normalized.to_node_id
                    && connection
                        .kind
                        .as_deref()
                        .is_some_and(|kind| kind == "derive")
            })
        {
            self.connect_reject = Some("An attempt can have only one parent version".into());
            return false;
        }
        let both_attempts = self
            .nodes
            .iter()
            .find(|node| node.id == normalized.from_node_id)
            .is_some_and(|node| node.node_type == CanvasNodeType::ImageAttempt)
            && self
                .nodes
                .iter()
                .find(|node| node.id == normalized.to_node_id)
                .is_some_and(|node| node.node_type == CanvasNodeType::ImageAttempt);
        self.connections.push(CanvasConnection {
            id: format!("conn-{now_ms}"),
            from_node_id: normalized.from_node_id,
            to_node_id: normalized.to_node_id,
            relation: normalized.relation,
            kind: both_attempts.then(|| "derive".to_string()),
            reason: both_attempts.then(|| "Derived version".to_string()),
            changes: both_attempts
                .then(|| "prompt/config carried into the next attempt".to_string()),
            agent_run_id: None,
        });
        true
    }

    /// The reason the last connection release was refused, if an incompatible
    /// pair was dropped. Taken so a notice is shown once.
    pub fn take_connect_rejection(&mut self) -> Option<String> {
        self.connect_reject.take()
    }

    fn rejection_message(&self, from_id: &str, to_id: &str) -> Option<String> {
        let from = self.nodes.iter().find(|node| node.id == from_id)?;
        let to = self.nodes.iter().find(|node| node.id == to_id)?;
        Some(rejection_message(&from.node_type, &to.node_type))
    }

    /// The node id a connection drag would drop on, for the target highlight.
    pub fn connect_target(&self) -> Option<&str> {
        match &self.drag {
            Some(Drag::Connect { target, .. }) => target.as_deref(),
            _ => None,
        }
    }
}

/// The user-facing reason a pair is refused: generation nodes accept only their
/// matching prompt, and a prompt reaches only its matching generator.
pub fn rejection_message(from: &CanvasNodeType, to: &CanvasNodeType) -> String {
    match (from, to) {
        // An Image has an output handle but no input handle, so a refused drop
        // is one landing on its left side. An Image Stack never participates.
        (_, CanvasNodeType::Image | CanvasNodeType::ImageStack)
        | (CanvasNodeType::ImageStack, _) => "Image nodes cannot be connected".to_string(),
        (CanvasNodeType::Prompt, CanvasNodeType::VideoGeneration) => {
            "An Image Prompt connects only to Image Generation".to_string()
        }
        (CanvasNodeType::VideoPrompt, CanvasNodeType::ImageGeneration) => {
            "A Video Prompt connects only to Video Generation".to_string()
        }
        (CanvasNodeType::ImageGeneration, CanvasNodeType::VideoPrompt)
        | (_, CanvasNodeType::ImageGeneration) => {
            "Image Generation connects only from an Image Prompt".to_string()
        }
        (CanvasNodeType::VideoGeneration, CanvasNodeType::Prompt)
        | (_, CanvasNodeType::VideoGeneration) => {
            "Video Generation connects only from a Video Prompt".to_string()
        }
        (CanvasNodeType::Prompt, _) => {
            "An Image Prompt connects only to Image Generation".to_string()
        }
        (CanvasNodeType::VideoPrompt, _) => {
            "A Video Prompt connects only to Video Generation".to_string()
        }
        _ => "That connection is not allowed".to_string(),
    }
}

/// Drop-target hit test for connection drags (project.tsx:505-540): highest
/// priority is "inside the node", then the target handle circle, then the padded
/// box; legality goes through `normalize_connection`.
fn drop_target(
    nodes: &[CanvasNodeData],
    from_id: &str,
    handle: ConnectionHandleType,
    at: WorldPos,
    k: f64,
) -> Option<String> {
    hit_node_with(nodes, handle, at, k, |node| {
        node.id != from_id && normalize_connection(from_id, &node.id, nodes, handle).is_some()
    })
}

/// The nearest node the cursor is over, ignoring the legality rules: used only
/// to explain a refused drop.
fn hit_node(
    nodes: &[CanvasNodeData],
    from_id: &str,
    handle: ConnectionHandleType,
    at: WorldPos,
    k: f64,
) -> Option<String> {
    hit_node_with(nodes, handle, at, k, |node| node.id != from_id)
}

fn hit_node_with(
    nodes: &[CanvasNodeData],
    handle: ConnectionHandleType,
    at: WorldPos,
    k: f64,
    accept: impl Fn(&CanvasNodeData) -> bool,
) -> Option<String> {
    let scale = k.max(0.05);
    let padding = 32.0 / scale;
    let radius = 40.0 / scale;
    let mut best: Option<(u8, String)> = None;
    for node in nodes.iter().rev() {
        if is_node_hidden(node) {
            continue;
        }
        let anchor = open_canvas_core::geometry::connection_target_anchor(node, handle);
        let dx = at.x - anchor.x;
        let dy = at.y - anchor.y;
        let hits_handle = dx * dx + dy * dy <= radius * radius;
        let hits_inside = at.x >= node.position.x
            && at.x <= node.position.x + node.width
            && at.y >= node.position.y
            && at.y <= node.position.y + node.height;
        let hits_expanded = at.x >= node.position.x - padding
            && at.x <= node.position.x + node.width + padding
            && at.y >= node.position.y - padding
            && at.y <= node.position.y + node.height + padding;
        if !hits_handle && !hits_inside && !hits_expanded {
            continue;
        }
        if !accept(node) {
            continue;
        }
        let priority = if hits_inside {
            0
        } else if hits_handle {
            1
        } else {
            2
        };
        if best.as_ref().is_none_or(|(p, _)| priority < *p) {
            best = Some((priority, node.id.clone()));
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::connections::is_prompt_connection_allowed;
    use open_canvas_core::node::Position;

    fn types() -> (
        CanvasNodeType,
        CanvasNodeType,
        CanvasNodeType,
        CanvasNodeType,
    ) {
        (
            CanvasNodeType::Prompt,
            CanvasNodeType::ImageGeneration,
            CanvasNodeType::VideoPrompt,
            CanvasNodeType::VideoGeneration,
        )
    }

    /// The two legality rules, from both ends: an image prompt reaches only the
    /// image generator and a video prompt only the video generator.
    #[test]
    fn prompt_generator_pairing_holds_in_both_directions() {
        let (image_prompt, image_generation, video_prompt, video_generation) = types();
        assert!(is_prompt_connection_allowed(
            &image_prompt,
            &image_generation
        ));
        assert!(is_prompt_connection_allowed(
            &image_generation,
            &image_prompt
        ));
        assert!(!is_prompt_connection_allowed(
            &image_prompt,
            &video_generation
        ));
        assert!(!is_prompt_connection_allowed(
            &video_generation,
            &image_prompt
        ));

        assert!(is_prompt_connection_allowed(
            &video_prompt,
            &video_generation
        ));
        assert!(is_prompt_connection_allowed(
            &video_generation,
            &video_prompt
        ));
        assert!(!is_prompt_connection_allowed(
            &video_prompt,
            &image_generation
        ));
        assert!(!is_prompt_connection_allowed(
            &image_generation,
            &video_prompt
        ));
    }

    /// An Image starts a connect drag from its output handle — feeding a prompt
    /// reference or another consumer — while its left side is never a target
    /// and refuses a drop with the existing media notice.
    #[test]
    fn image_starts_a_connect_but_cannot_be_targeted() {
        let mut doc = Doc::demo();
        doc.begin_connect("image", ConnectionHandleType::Source);
        assert!(
            matches!(
                doc.drag(),
                Some(Drag::Connect { node_id, handle, .. })
                    if node_id == "image" && *handle == ConnectionHandleType::Source
            ),
            "an image starts a drag from its output handle"
        );
        assert!(doc.take_connect_rejection().is_none());

        // Image -> prompt is legal and does not exist in the fixture yet.
        let prompt_at = doc
            .nodes()
            .iter()
            .find(|node| node.id == "prompt")
            .map(|node| Position::new(node.position.x + 20.0, node.position.y + 20.0))
            .expect("the fixture has the image prompt");
        doc.connect_to(WorldPos::new(prompt_at.x, prompt_at.y));
        assert_eq!(doc.connect_target(), Some("prompt"));
        assert!(doc.end_connect(10));
        assert!(
            doc.connections()
                .iter()
                .any(|c| c.from_node_id == "image" && c.to_node_id == "prompt"),
            "the image sources a wire into the prompt"
        );

        // A release over the image's left side is refused with the notice.
        doc.begin_connect("text", ConnectionHandleType::Source);
        let image_at = doc
            .nodes()
            .iter()
            .find(|node| node.id == "image")
            .map(|node| Position::new(node.position.x + 20.0, node.position.y + 20.0))
            .expect("the fixture has an image");
        doc.connect_to(WorldPos::new(image_at.x, image_at.y));
        assert_eq!(doc.connect_target(), None, "an image is not a target");
        assert!(!doc.end_connect(20));
        assert_eq!(
            doc.take_connect_rejection().as_deref(),
            Some("Image nodes cannot be connected")
        );

        // A target-handle drag from an image is a drop on that same left side.
        doc.begin_connect("image", ConnectionHandleType::Target);
        assert!(doc.drag().is_none(), "an image has no target handle");
        assert_eq!(
            doc.take_connect_rejection().as_deref(),
            Some("Image nodes cannot be connected")
        );
    }

    #[test]
    fn rejected_drops_explain_the_rule() {
        let (image_prompt, image_generation, video_prompt, video_generation) = types();
        assert_eq!(
            rejection_message(&image_prompt, &video_generation),
            "An Image Prompt connects only to Image Generation"
        );
        assert_eq!(
            rejection_message(&video_prompt, &image_generation),
            "A Video Prompt connects only to Video Generation"
        );
        assert_eq!(
            rejection_message(&image_generation, &video_prompt),
            "Image Generation connects only from an Image Prompt"
        );
        assert_eq!(
            rejection_message(&video_generation, &image_prompt),
            "Video Generation connects only from a Video Prompt"
        );
    }

    /// Releasing over the wrong prompt records the reason (the handler turns it
    /// into a notice) instead of silently doing nothing.
    #[test]
    fn end_connect_reports_an_incompatible_drop() {
        let mut doc = Doc::demo();
        // Drag from the video prompt's source handle onto the image generator.
        doc.begin_connect("video-prompt", ConnectionHandleType::Source);
        let target = doc
            .nodes()
            .iter()
            .find(|node| node.id == "image-generation")
            .map(|node| Position::new(node.position.x + 10.0, node.position.y + 10.0));
        assert!(target.is_some(), "the fixture has the image generator");
        if let Some(target) = target {
            doc.connect_to(WorldPos::new(target.x, target.y));
            assert!(!doc.end_connect(10), "the pair is illegal");
            assert_eq!(
                doc.take_connect_rejection().as_deref(),
                Some("A Video Prompt connects only to Video Generation")
            );
            assert!(doc.take_connect_rejection().is_none(), "reported once");
        }
        // The legal pair still connects (drop the demo wire first, so this is a
        // fresh connection rather than the existing duplicate).
        doc.delete_connection("conn-prompt-image-generation", 0);
        doc.begin_connect("prompt", ConnectionHandleType::Source);
        let target = doc
            .nodes()
            .iter()
            .find(|node| node.id == "image-generation")
            .map(|node| Position::new(node.position.x + 10.0, node.position.y + 10.0));
        if let Some(target) = target {
            doc.connect_to(WorldPos::new(target.x, target.y));
            assert!(doc.end_connect(20));
        }
        // No drag left behind and no stray rejection state.
        assert!(doc.drag().is_none());
        assert!(doc.take_connect_rejection().is_none());
    }
}
