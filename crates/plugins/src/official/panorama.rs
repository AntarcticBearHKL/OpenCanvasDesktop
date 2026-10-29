//! Panorama node logic, ported from `plugins/canvas/panorama/src/index.tsx`.
//!
//! Pure half: the equirectangular generation prompt, the 2:1 shape check, the
//! upstream-image fallback, and the metadata (built-in panel config + resource).
//! The three.js sphere viewer and the file picker are UI/renderer concerns.

use open_canvas_core::{CanvasGenerationMode, CanvasNodeData, CanvasNodeMetadata};

use crate::abi::{
    BuiltinPanelConfig, CanvasNodeDefinition, CanvasNodeResource, NodeSize, NodeView,
};
use crate::plugin::CanvasPlugin;

pub const NODE_TYPE: &str = "panorama:viewer";
pub const MINIMAP_COLOR: &str = "#0ea5e9";
/// Equirectangular panoramas are 2:1 (`panorama/index.tsx:16-19`).
pub const EQUIRECTANGULAR_ASPECT: f64 = 2.0;
/// Accepted relative deviation from 2:1 before a size is rejected.
pub const EQUIRECTANGULAR_TOLERANCE: f64 = 0.02;
const DEFAULT_SIZE: NodeSize = NodeSize {
    width: 480.0,
    height: 300.0,
};

/// System prompt prefix constraining AI output to a spherical equirectangular
/// panorama (`panorama/index.tsx:16-19`).
pub const PANORAMA_SYSTEM_PROMPT: &str = "A seamless 360-degree equirectangular panorama, 2:1 aspect ratio, full spherical VR photo, horizontally wrapping seamlessly at the left and right edges, no visible seam, no distortion artifacts, even horizon, no text, no watermark. Scene: ";

/// The built-in image panel prompt for a scene description (`promptPrefix` + prompt).
pub fn generation_prompt(scene: &str) -> String {
    format!("{PANORAMA_SYSTEM_PROMPT}{scene}")
}

/// `is_equirectangular`: the size is ~2:1 within [`EQUIRECTANGULAR_TOLERANCE`].
pub fn is_equirectangular(width: f64, height: f64) -> bool {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return false;
    }
    let error = (width / height - EQUIRECTANGULAR_ASPECT).abs() / EQUIRECTANGULAR_ASPECT;
    error <= EQUIRECTANGULAR_TOLERANCE
}

/// Own image, else the first non-empty upstream image content
/// (`panorama/index.tsx:250-254`).
pub fn source_from(stored: Option<&str>, upstream: &[CanvasNodeData]) -> Option<String> {
    stored
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            upstream
                .iter()
                .filter_map(|node| node.metadata.as_ref())
                .filter_map(|metadata| metadata.content.as_deref())
                .find(|content| !content.is_empty())
                .map(str::to_string)
        })
}

fn viewer_resource(view: &NodeView<'_>) -> Option<CanvasNodeResource> {
    let content = view
        .metadata()?
        .content
        .as_deref()
        .filter(|value| !value.is_empty())?;
    Some(CanvasNodeResource::image(content))
}

pub fn definition() -> CanvasNodeDefinition {
    let mut definition = CanvasNodeDefinition::new(NODE_TYPE, "3D panorama", "🌐", DEFAULT_SIZE)
        .with_description("360° panorama viewer (upload / AI generation)")
        .with_minimap_color(MINIMAP_COLOR)
        .with_metadata(CanvasNodeMetadata::default());
    definition.interaction_toggle = true;
    definition.use_builtin_panel = Some(
        BuiltinPanelConfig::new(CanvasGenerationMode::Image)
            .with_prompt_prefix(PANORAMA_SYSTEM_PROMPT)
            .writing_back_to_self(),
    );
    definition.resource = Some(std::sync::Arc::new(viewer_resource));
    definition
}

pub fn plugin() -> CanvasPlugin {
    CanvasPlugin::new("panorama", "3D panorama node", "1.1.0", vec![definition()]).with_description(
        "View a 360° equirectangular panorama: upload, AI generation, or an upstream image node",
    )
}
