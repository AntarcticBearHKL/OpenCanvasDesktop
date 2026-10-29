//! The ten built-in node definitions (`builtin-nodes.tsx:20-33`).
//!
//! Sizes, titles, and initial metadata come from the core node specs; this
//! module adds the presentation flags, minimap colors, and resource resolver.

use std::sync::Arc;

use open_canvas_core::{get_node_spec, CanvasGenerationMode, CanvasNodeType};

use crate::abi::{
    BuiltinPanelConfig, CanvasNodeDefinition, CanvasNodeResource, NodeSize, NodeView,
};
use crate::registry::{NodeRegistry, BUILTIN_PLUGIN_ID};

/// Register the ten built-in definitions (`registerBuiltinNodes`).
pub fn register_builtin_nodes(registry: &NodeRegistry) {
    registry.register(BUILTIN_PLUGIN_ID, builtin_definitions());
}

/// The ten built-in definitions in [`CanvasNodeType::BUILTINS`] order.
pub fn builtin_definitions() -> Vec<CanvasNodeDefinition> {
    CanvasNodeType::BUILTINS
        .into_iter()
        .map(builtin_definition)
        .collect()
}

fn builtin_definition(node_type: CanvasNodeType) -> CanvasNodeDefinition {
    let spec = get_node_spec(&node_type);
    let mut definition = CanvasNodeDefinition {
        type_id: node_type.as_str().to_string(),
        title: spec.title,
        icon: builtin_icon(&node_type).to_string(),
        default_size: NodeSize::new(spec.width, spec.height),
        default_metadata: spec.metadata,
        minimap_color: builtin_minimap_color(&node_type).map(str::to_string),
        ..CanvasNodeDefinition::default()
    };

    match node_type {
        CanvasNodeType::Text | CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt => {
            definition.resource = Some(Arc::new(builtin_resource));
        }
        CanvasNodeType::Video => {
            definition.resource = Some(Arc::new(builtin_resource));
            definition.keep_aspect_ratio = Some(Arc::new(|_| true));
        }
        CanvasNodeType::Image => {
            // A produced image keeps its output handle: it can feed a prompt
            // reference or another downstream consumer. Its left side never
            // accepts a wire (`plugin_node_view` withholds the target handle),
            // because an image is written by a generator, not by a connection.
            definition.resource = Some(Arc::new(builtin_resource));
            definition.keep_aspect_ratio = Some(Arc::new(|view| {
                !matches!(
                    view.node.metadata.as_ref().and_then(|m| m.free_resize),
                    Some(true)
                )
            }));
        }
        CanvasNodeType::ImageAttempt => {
            definition.resource = Some(Arc::new(builtin_resource));
            definition.keep_aspect_ratio = Some(Arc::new(|_| true));
        }
        CanvasNodeType::ImageStack => {
            definition.has_source_handle = false;
            definition.transparent_background = true;
        }
        CanvasNodeType::ImageGeneration => {
            definition.has_source_handle = false;
            definition.use_builtin_panel =
                Some(BuiltinPanelConfig::new(CanvasGenerationMode::Image));
            definition.keep_aspect_ratio = Some(Arc::new(|_| true));
        }
        CanvasNodeType::VideoGeneration => {
            definition.has_source_handle = false;
            definition.use_builtin_panel =
                Some(BuiltinPanelConfig::new(CanvasGenerationMode::Video));
            definition.keep_aspect_ratio = Some(Arc::new(|_| true));
        }
        CanvasNodeType::Assets => {
            definition.has_source_handle = false;
            definition.hide_panel = true;
        }
        CanvasNodeType::SmartCanvas => {
            definition.has_source_handle = false;
            definition.hide_panel = true;
        }
        CanvasNodeType::Plugin(_) => {}
    }

    definition
}

fn builtin_icon(node_type: &CanvasNodeType) -> &'static str {
    match node_type {
        CanvasNodeType::Text => "file-text",
        CanvasNodeType::Prompt => "message-square-text",
        CanvasNodeType::VideoPrompt => "clapperboard",
        CanvasNodeType::Image => "image",
        CanvasNodeType::ImageAttempt => "sparkles",
        CanvasNodeType::ImageStack => "images",
        CanvasNodeType::Video => "video",
        CanvasNodeType::ImageGeneration => "sparkles",
        CanvasNodeType::VideoGeneration => "video",
        CanvasNodeType::Assets => "folder-input",
        CanvasNodeType::SmartCanvas => "smart-canvas",
        CanvasNodeType::Plugin(_) => "puzzle",
    }
}

fn builtin_minimap_color(node_type: &CanvasNodeType) -> Option<&'static str> {
    match node_type {
        CanvasNodeType::Text => None,
        CanvasNodeType::Prompt => Some("#eab308"),
        CanvasNodeType::VideoPrompt => Some("#fb923c"),
        CanvasNodeType::Image | CanvasNodeType::ImageStack => Some("#10b981"),
        CanvasNodeType::ImageAttempt => Some("#f472b6"),
        CanvasNodeType::Video => Some("#f97316"),
        CanvasNodeType::ImageGeneration => Some("#f472b6"),
        CanvasNodeType::VideoGeneration => Some("#fb7185"),
        CanvasNodeType::Assets => Some("#64748b"),
        CanvasNodeType::SmartCanvas => Some("#8b5cf6"),
        CanvasNodeType::Plugin(_) => None,
    }
}

fn non_empty(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|text| !text.is_empty())
}

/// `builtinResource` (builtin-nodes.tsx:10-16).
fn builtin_resource(view: &NodeView<'_>) -> Option<CanvasNodeResource> {
    let metadata = view.node.metadata.as_ref()?;
    let content = non_empty(metadata.content.as_ref());
    let prompt = non_empty(metadata.prompt.as_ref());
    match &view.node.node_type {
        CanvasNodeType::Image => content.map(CanvasNodeResource::image),
        CanvasNodeType::Video => content.map(CanvasNodeResource::video),
        CanvasNodeType::Text => content.or(prompt).map(CanvasNodeResource::text),
        CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt => {
            prompt.map(CanvasNodeResource::text)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_cover_the_ten_core_kinds() {
        let definitions = builtin_definitions();
        assert_eq!(definitions.len(), 10);
        let types: Vec<&str> = definitions.iter().map(|d| d.type_id.as_str()).collect();
        for node_type in CanvasNodeType::BUILTINS {
            assert!(types.contains(&node_type.as_str()));
        }
    }

    /// An Image keeps its output handle (it can feed a prompt reference or a
    /// consumer) but never declares a target handle, which `plugin_node_view`
    /// withholds by kind. Image Stack and Smart Canvas offer neither.
    #[test]
    fn image_offers_an_output_but_no_input_handle() {
        let definitions = builtin_definitions();
        let definition = |kind: CanvasNodeType| {
            definitions
                .iter()
                .find(|definition| definition.type_id == kind.as_str())
                .unwrap_or_else(|| panic!("{} is built in", kind.as_str()))
        };

        assert!(
            definition(CanvasNodeType::Image).has_source_handle,
            "an image can start a wire"
        );
        for kind in [CanvasNodeType::ImageStack, CanvasNodeType::SmartCanvas] {
            assert!(
                !definition(kind.clone()).has_source_handle,
                "{} must not offer an output handle",
                kind.as_str()
            );
        }
    }
}
