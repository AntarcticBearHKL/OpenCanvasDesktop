//! Host bridge: resolve a node plus its registered definition into a renderable,
//! UI-agnostic spec.
//!
//! This is the seam the Slint app consumes. It takes the two inputs the app
//! already has — the wire [`CanvasNodeData`] (type id + `CanvasNodeMetadata`) and
//! the [`CanvasNodeDefinition`] from the [`crate::registry::NodeRegistry`] — and
//! produces a [`PluginNodeView`] with no UI types. The mapping mirrors the
//! render decisions in `canvas-node.tsx` (background, handles, interaction) and
//! `project.tsx` (panel kind); the host's runtime-only exclusions (an open
//! dialog, a live selection) stay in the UI layer.

use open_canvas_core::{CanvasGenerationMode, CanvasNodeData, CanvasNodeType, ResourceKind};

use crate::abi::{CanvasNodeDefinition, MetadataExtras, NodeSize, NodeView};

/// Title resolved for an unregistered (missing-plugin) node.
pub const MISSING_PLUGIN_TITLE: &str = "Node";
/// Icon key resolved for an unregistered node.
pub const MISSING_PLUGIN_ICON: &str = "puzzle";

/// Node-card background treatment (`canvas-node.tsx:176-177`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundMode {
    /// Frosted card; the default for every node kind.
    Normal,
    /// Transparent so vector / blended content shows through to the canvas.
    Transparent,
}

/// The lower panel a definition declares (`project.tsx:1807-1830`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// No lower panel (`hidePanel`, or a definition that declares none).
    None,
    /// Host built-in generation panel in image mode.
    BuiltinImage,
    /// Host built-in generation panel in video mode.
    BuiltinVideo,
    /// Host built-in generation panel in text mode.
    BuiltinText,
    /// A panel supplied by the plugin definition (web `definition.Panel`).
    Plugin,
}

impl PanelKind {
    fn builtin(mode: CanvasGenerationMode) -> Self {
        match mode {
            CanvasGenerationMode::Text => Self::BuiltinText,
            CanvasGenerationMode::Image => Self::BuiltinImage,
            CanvasGenerationMode::Video => Self::BuiltinVideo,
        }
    }
}

/// A fully resolved, render-only description of one node.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginNodeView {
    /// Wire type id (`"image"`, `"markdown:doc"`, ...).
    pub type_id: String,
    /// Display title of the node kind (the create-menu label).
    pub title: String,
    /// Icon key: a lucide name for built-ins, an emoji for plugin nodes.
    pub icon: String,
    pub minimap_color: Option<String>,
    pub background: BackgroundMode,
    pub panel: PanelKind,
    pub show_in_create_menu: bool,
    /// Right-side output handle.
    pub has_source_handle: bool,
    /// Left-side input handle (`false` for image/image-stack, see
    /// `canvas-node.tsx:483`).
    pub has_target_handle: bool,
    pub keep_aspect_ratio: bool,
    /// The definition offers the interaction/move toolbar toggle.
    pub interaction_toggle: bool,
    /// Whether content receives pointer events right now
    /// (`canvas-node.tsx:171-174`).
    pub content_interactive: bool,
    /// Kind of resource the node emits upstream, if any.
    pub resource_kind: Option<ResourceKind>,
    /// Default size for this node kind.
    pub default_size: NodeSize,
    /// `false` when no definition is registered (missing-plugin placeholder).
    pub registered: bool,
}

/// Resolve the render spec for `node` from `definition`.
///
/// Pass `None` for an unregistered type: the result carries the missing-plugin
/// placeholder (title `"Node"`, puzzle icon, no resource, both handles present).
pub fn plugin_node_view(
    node: &CanvasNodeData,
    extras: &MetadataExtras,
    definition: Option<&CanvasNodeDefinition>,
) -> PluginNodeView {
    let type_id = node.node_type.as_str().to_string();
    let has_target_handle = !matches!(
        node.node_type,
        CanvasNodeType::Image | CanvasNodeType::ImageStack | CanvasNodeType::SmartCanvas
    );

    let Some(definition) = definition else {
        return PluginNodeView {
            type_id,
            title: MISSING_PLUGIN_TITLE.to_string(),
            icon: MISSING_PLUGIN_ICON.to_string(),
            minimap_color: None,
            background: BackgroundMode::Normal,
            panel: PanelKind::None,
            show_in_create_menu: false,
            has_source_handle: true,
            has_target_handle,
            keep_aspect_ratio: false,
            interaction_toggle: false,
            content_interactive: content_interactive(node, false, false),
            resource_kind: None,
            default_size: NodeSize::default(),
            registered: false,
        };
    };

    let view = NodeView::new(node, extras);
    let force_interactive = definition.is_force_interactive(&view);
    PluginNodeView {
        type_id,
        title: definition.title.clone(),
        icon: definition.icon.clone(),
        minimap_color: definition.minimap_color.clone(),
        background: if definition.transparent_background {
            BackgroundMode::Transparent
        } else {
            BackgroundMode::Normal
        },
        panel: panel_kind(definition),
        show_in_create_menu: definition.show_in_create_menu,
        has_source_handle: definition.has_source_handle,
        has_target_handle,
        keep_aspect_ratio: definition.keeps_aspect_ratio(&view),
        interaction_toggle: definition.interaction_toggle,
        content_interactive: content_interactive(
            node,
            definition.interaction_toggle,
            force_interactive,
        ),
        resource_kind: definition
            .resolve_resource(&view)
            .map(|resource| resource.kind),
        default_size: definition.default_size,
        registered: true,
    }
}

fn panel_kind(definition: &CanvasNodeDefinition) -> PanelKind {
    if definition.hide_panel {
        return PanelKind::None;
    }
    if let Some(config) = &definition.use_builtin_panel {
        return PanelKind::builtin(config.mode);
    }
    if definition.has_custom_panel {
        return PanelKind::Plugin;
    }
    PanelKind::None
}

fn content_interactive(
    node: &CanvasNodeData,
    interaction_toggle: bool,
    force_interactive: bool,
) -> bool {
    let metadata = node.metadata.as_ref();
    if metadata.and_then(|meta| meta.locked).unwrap_or(false) {
        return false;
    }
    if !interaction_toggle || force_interactive {
        return true;
    }
    let content_empty = metadata
        .and_then(|meta| meta.content.as_deref())
        .is_none_or(|content| content.is_empty());
    if content_empty {
        return true;
    }
    metadata.and_then(|meta| meta.interactive).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::CanvasNodeDefinition;

    fn node(type_id: &str) -> CanvasNodeData {
        CanvasNodeData {
            id: "n".to_string(),
            node_type: CanvasNodeType::from_id(type_id),
            title: "T".to_string(),
            position: open_canvas_core::Position::new(0.0, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: None,
        }
    }

    #[test]
    fn unregistered_types_get_the_missing_plugin_placeholder() {
        let extras = MetadataExtras::new();
        let view = plugin_node_view(&node("ghost:node"), &extras, None);
        assert!(!view.registered);
        assert_eq!(view.title, MISSING_PLUGIN_TITLE);
        assert_eq!(view.icon, MISSING_PLUGIN_ICON);
        assert!(view.has_source_handle);
        assert!(view.has_target_handle);
        assert_eq!(view.panel, PanelKind::None);
        assert_eq!(view.default_size, NodeSize::default());
    }

    #[test]
    fn registered_definitions_keep_a_custom_panel() {
        let extras = MetadataExtras::new();
        let mut definition =
            CanvasNodeDefinition::new("acme:x", "X", "x", NodeSize::new(10.0, 10.0));
        definition.has_custom_panel = true;
        let view = plugin_node_view(&node("acme:x"), &extras, Some(&definition));
        assert_eq!(view.panel, PanelKind::Plugin);
        assert!(view.registered);
    }

    /// The image asymmetry at the view seam: the definition keeps its output
    /// handle, while the target handle is withheld by node kind.
    #[test]
    fn image_view_keeps_the_source_handle_and_withholds_the_target() {
        let extras = MetadataExtras::new();
        let definition =
            CanvasNodeDefinition::new("image", "Image", "image", NodeSize::new(340.0, 240.0));
        let view = plugin_node_view(&node("image"), &extras, Some(&definition));
        assert!(view.has_source_handle, "an image can start a wire");
        assert!(!view.has_target_handle, "an image never receives one");
    }

    #[test]
    fn panel_kind_prefers_hide_over_builtin() {
        let builtin = CanvasNodeDefinition {
            use_builtin_panel: Some(crate::abi::BuiltinPanelConfig::new(
                CanvasGenerationMode::Image,
            )),
            ..CanvasNodeDefinition::default()
        };
        assert_eq!(panel_kind(&builtin), PanelKind::BuiltinImage);

        let hidden = CanvasNodeDefinition {
            hide_panel: true,
            ..builtin
        };
        assert_eq!(panel_kind(&hidden), PanelKind::None);
    }
}
