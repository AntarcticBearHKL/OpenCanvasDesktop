//! Default sizes and initial metadata per built-in node type, mirroring
//! `web/src/constant/canvas.ts` (`NODE_DEFAULT_SIZE` + `NODE_SPECS`).
//!
//! Titles in the web app come from `i18n` at runtime; the UI-agnostic core stores a
//! stable English label that the UI layer can localize later.

use crate::node::{
    CanvasGenerationMode, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType, CanvasVideoMode,
};

/// First OpenRouter video model id, mirrored from `openRouterVideoModels[0].value`
/// (web/src/lib/video-generation.ts) and used as the video-generation default model.
pub const DEFAULT_VIDEO_GENERATION_MODEL: &str = "minimax/hailuo-3-max";

/// Registry fallback size for an unregistered/plugin type (node-registry.ts:50).
pub const FALLBACK_NODE_WIDTH: f64 = 340.0;
pub const FALLBACK_NODE_HEIGHT: f64 = 240.0;

/// Resolved node spec: default size, title, and initial metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub width: f64,
    pub height: f64,
    pub title: String,
    pub metadata: CanvasNodeMetadata,
}

fn meta(build: impl FnOnce(&mut CanvasNodeMetadata)) -> CanvasNodeMetadata {
    let mut metadata = CanvasNodeMetadata::default();
    build(&mut metadata);
    metadata
}

fn spec(width: f64, height: f64, title: &str, metadata: CanvasNodeMetadata) -> NodeSpec {
    NodeSpec {
        width,
        height,
        title: title.to_string(),
        metadata,
    }
}

/// Resolve the spec for a node type, mirroring `getNodeSpec` (constant/canvas.ts:46-50)
/// with the registry fallback for plugin / unknown types.
pub fn get_node_spec(node_type: &CanvasNodeType) -> NodeSpec {
    match node_type {
        CanvasNodeType::Image => spec(
            340.0,
            240.0,
            "Image",
            meta(|m| {
                m.content = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
            }),
        ),
        CanvasNodeType::ImageAttempt => spec(
            380.0,
            460.0,
            "Image Attempt",
            meta(|m| {
                m.prompt = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
                m.generation_mode = Some(CanvasGenerationMode::Image);
                m.attempt_version = Some(1);
                m.attempt_frozen = Some(false);
                m.attempt_runs = Some(Vec::new());
            }),
        ),
        CanvasNodeType::ImageStack => spec(
            420.0,
            320.0,
            "Image Stack",
            meta(|m| {
                m.status = Some(CanvasNodeStatus::Idle);
                m.images = Some(Vec::new());
            }),
        ),
        CanvasNodeType::Text => spec(
            340.0,
            360.0,
            "Text",
            meta(|m| {
                m.content = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
                m.font_size = Some(14.0);
            }),
        ),
        CanvasNodeType::Prompt => spec(
            340.0,
            240.0,
            "Image Prompt",
            meta(|m| {
                m.prompt = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
            }),
        ),
        CanvasNodeType::VideoPrompt => spec(
            340.0,
            360.0,
            "Video Prompt",
            meta(|m| {
                m.prompt = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
                m.video_mode = Some(CanvasVideoMode::Frames);
            }),
        ),
        CanvasNodeType::ImageGeneration => spec(
            320.0,
            360.0,
            "Image Generation",
            meta(|m| {
                m.status = Some(CanvasNodeStatus::Idle);
                m.generation_mode = Some(CanvasGenerationMode::Image);
            }),
        ),
        CanvasNodeType::VideoGeneration => spec(
            320.0,
            320.0,
            "Video Generation",
            meta(|m| {
                m.status = Some(CanvasNodeStatus::Idle);
                m.generation_mode = Some(CanvasGenerationMode::Video);
                m.model = Some(DEFAULT_VIDEO_GENERATION_MODEL.to_string());
            }),
        ),
        CanvasNodeType::Video => spec(
            420.0,
            236.0,
            "Video",
            meta(|m| {
                m.content = Some(String::new());
                m.status = Some(CanvasNodeStatus::Idle);
            }),
        ),
        CanvasNodeType::Assets => spec(360.0, 320.0, "Assets", CanvasNodeMetadata::default()),
        CanvasNodeType::SmartCanvas => {
            spec(420.0, 300.0, "Smart Canvas", CanvasNodeMetadata::default())
        }
        CanvasNodeType::Plugin(_) => NodeSpec {
            width: FALLBACK_NODE_WIDTH,
            height: FALLBACK_NODE_HEIGHT,
            title: "Node".to_string(),
            metadata: CanvasNodeMetadata::default(),
        },
    }
}

/// The ten built-in specs (the `NODE_SPECS` table), in `CanvasNodeType::BUILTINS` order.
pub fn node_specs() -> Vec<(CanvasNodeType, NodeSpec)> {
    CanvasNodeType::BUILTINS
        .into_iter()
        .map(|node_type| {
            let spec = get_node_spec(&node_type);
            (node_type, spec)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size_of(node_type: &CanvasNodeType) -> (f64, f64) {
        let spec = get_node_spec(node_type);
        (spec.width, spec.height)
    }

    #[test]
    fn node_specs_have_the_expected_default_sizes() {
        assert_eq!(size_of(&CanvasNodeType::Image), (340.0, 240.0));
        assert_eq!(size_of(&CanvasNodeType::ImageStack), (420.0, 320.0));
        // Text grew from 340x240 so a text-generation card (inline model +
        // reasoning + count rows, the action and the text body) fits at 100%.
        assert_eq!(size_of(&CanvasNodeType::Text), (340.0, 360.0));
        assert_eq!(size_of(&CanvasNodeType::Prompt), (340.0, 240.0));
        // Video-prompt grew from 340x300 so the prompt editor, reference chips
        // and strip, mode chips and three 96px frame slots fit at 100%.
        assert_eq!(size_of(&CanvasNodeType::VideoPrompt), (340.0, 360.0));
        assert_eq!(size_of(&CanvasNodeType::ImageGeneration), (320.0, 360.0));
        assert_eq!(size_of(&CanvasNodeType::VideoGeneration), (320.0, 320.0));
        assert_eq!(size_of(&CanvasNodeType::Video), (420.0, 236.0));
        assert_eq!(size_of(&CanvasNodeType::Assets), (360.0, 320.0));
        assert_eq!(size_of(&CanvasNodeType::SmartCanvas), (420.0, 300.0));
    }

    #[test]
    fn node_specs_cover_every_builtin_once() {
        let specs = node_specs();
        assert_eq!(specs.len(), 10);
        let types: Vec<&CanvasNodeType> = specs.iter().map(|(t, _)| t).collect();
        for builtin in &CanvasNodeType::BUILTINS {
            assert!(types.contains(&builtin));
        }
    }

    #[test]
    fn initial_metadata_matches_the_web_specs() {
        let text = get_node_spec(&CanvasNodeType::Text).metadata;
        assert_eq!(text.status, Some(CanvasNodeStatus::Idle));
        assert_eq!(text.font_size, Some(14.0));
        assert_eq!(text.content.as_deref(), Some(""));

        let video_prompt = get_node_spec(&CanvasNodeType::VideoPrompt).metadata;
        assert_eq!(video_prompt.video_mode, Some(CanvasVideoMode::Frames));

        let video_generation = get_node_spec(&CanvasNodeType::VideoGeneration).metadata;
        assert_eq!(
            video_generation.generation_mode,
            Some(CanvasGenerationMode::Video)
        );
        assert_eq!(
            video_generation.model.as_deref(),
            Some(DEFAULT_VIDEO_GENERATION_MODEL)
        );

        let images = get_node_spec(&CanvasNodeType::ImageStack).metadata;
        assert_eq!(images.images, Some(Vec::new()));

        let assets = get_node_spec(&CanvasNodeType::Assets).metadata;
        assert_eq!(assets, CanvasNodeMetadata::default());
    }

    #[test]
    fn plugin_types_fall_back_to_the_registry_default() {
        let spec = get_node_spec(&CanvasNodeType::Plugin("acme:x".into()));
        assert_eq!(spec.width, FALLBACK_NODE_WIDTH);
        assert_eq!(spec.height, FALLBACK_NODE_HEIGHT);
    }
}
