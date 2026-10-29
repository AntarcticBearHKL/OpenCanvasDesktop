//! The native, UI-agnostic node/plugin ABI.
//!
//! Mirrors `web/src/types/canvas-plugin.ts` without React or the DOM:
//! [`CanvasNodeDefinition`] is the data half of `CanvasNodeDefinition`; the
//! render-time half (`Content`/`Panel`/`toolbar` React components) has no native
//! equivalent yet and is deliberately absent (Slint rendering is a later task).
//!
//! Node metadata reuses [`CanvasNodeMetadata`] from `open-canvas-core`. Plugin
//! nodes may need keys the fixed core schema does not model (`editing`,
//! `pluginColor`, ...); those live in [`MetadataExtras`] and are passed to the
//! definition callbacks through [`NodeView`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use open_canvas_core::{
    CanvasGenerationMode, CanvasNodeData, CanvasNodeMetadata, ResourceKind, FALLBACK_NODE_HEIGHT,
    FALLBACK_NODE_WIDTH,
};

/// Plugin-private metadata keys (`editing`, `pluginColor`, ...). The core
/// [`CanvasNodeMetadata`] schema is fixed, so custom keys are kept beside it.
pub type MetadataExtras = BTreeMap<String, serde_json::Value>;

/// A node as seen by a node definition: the core wire node plus plugin extras.
#[derive(Debug, Clone, Copy)]
pub struct NodeView<'a> {
    pub node: &'a CanvasNodeData,
    pub extras: &'a MetadataExtras,
}

impl<'a> NodeView<'a> {
    pub fn new(node: &'a CanvasNodeData, extras: &'a MetadataExtras) -> Self {
        Self { node, extras }
    }

    pub fn metadata(&self) -> Option<&CanvasNodeMetadata> {
        self.node.metadata.as_ref()
    }

    /// Plugin-private metadata value, if present.
    pub fn extra(&self, key: &str) -> Option<&serde_json::Value> {
        self.extras.get(key)
    }
}

/// `{width, height}` default size of a node kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeSize {
    pub width: f64,
    pub height: f64,
}

impl NodeSize {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

impl Default for NodeSize {
    /// Registry fallback size (`node-registry.ts:50`).
    fn default() -> Self {
        Self {
            width: FALLBACK_NODE_WIDTH,
            height: FALLBACK_NODE_HEIGHT,
        }
    }
}

/// `CanvasNodeResource` (canvas-plugin.ts:9) — emitted when this node is used as
/// an upstream input. `kind` reuses the core [`ResourceKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasNodeResource {
    pub kind: ResourceKind,
    pub text: Option<String>,
    pub url: Option<String>,
}

impl CanvasNodeResource {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            kind: ResourceKind::Text,
            text: Some(value.into()),
            url: None,
        }
    }

    pub fn image(url: impl Into<String>) -> Self {
        Self {
            kind: ResourceKind::Image,
            text: None,
            url: Some(url.into()),
        }
    }

    pub fn video(url: impl Into<String>) -> Self {
        Self {
            kind: ResourceKind::Video,
            text: None,
            url: Some(url.into()),
        }
    }
}

/// `CanvasBuiltinPanelConfig` (canvas-plugin.ts:94) — reuse the host generation
/// panel instead of a custom one. `mode` reuses the core generation mode enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinPanelConfig {
    pub mode: CanvasGenerationMode,
    pub prompt_prefix: Option<String>,
    pub write_back_to_self: bool,
}

impl BuiltinPanelConfig {
    pub fn new(mode: CanvasGenerationMode) -> Self {
        Self {
            mode,
            prompt_prefix: None,
            write_back_to_self: false,
        }
    }

    pub fn with_prompt_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prompt_prefix = Some(prefix.into());
        self
    }

    pub fn writing_back_to_self(mut self) -> Self {
        self.write_back_to_self = true;
        self
    }
}

/// Predicate over the rendered node (`forceInteractive` / `keepAspectRatio`).
pub type NodePredicate = Arc<dyn for<'a> Fn(&NodeView<'a>) -> bool + Send + Sync>;

/// Resolver for the upstream resource a node exposes (`resource`).
pub type ResourceResolver =
    Arc<dyn for<'a> Fn(&NodeView<'a>) -> Option<CanvasNodeResource> + Send + Sync>;

/// `CanvasNodeDefinition` (canvas-plugin.ts:101) as plain Rust.
///
/// Defaults mirror the web comment defaults: `showInCreateMenu` and
/// `hasSourceHandle` are `true`; every other flag is `false`/unset.
#[derive(Clone)]
pub struct CanvasNodeDefinition {
    /// Wire type id (`"image"`, `"markdown:doc"`, ...).
    pub type_id: String,
    pub title: String,
    /// Icon name or emoji. The web's `ReactNode` icons have no native form.
    pub icon: String,
    pub description: Option<String>,
    pub default_size: NodeSize,
    pub default_metadata: CanvasNodeMetadata,
    /// Plugin-private default keys, see [`MetadataExtras`].
    pub default_extra_metadata: MetadataExtras,
    pub minimap_color: Option<String>,
    pub show_in_create_menu: bool,
    pub has_source_handle: bool,
    pub hide_panel: bool,
    /// The definition supplies its own lower panel (`Panel` in the web
    /// contract). Has no UI type here; the host bridge turns it into
    /// [`crate::view::PanelKind::Plugin`].
    pub has_custom_panel: bool,
    pub transparent_background: bool,
    pub auto_open_panel: bool,
    pub use_builtin_panel: Option<BuiltinPanelConfig>,
    pub interaction_toggle: bool,
    pub force_interactive: Option<NodePredicate>,
    pub keep_aspect_ratio: Option<NodePredicate>,
    pub resource: Option<ResourceResolver>,
}

impl Default for CanvasNodeDefinition {
    fn default() -> Self {
        Self {
            type_id: String::new(),
            title: String::new(),
            icon: String::new(),
            description: None,
            default_size: NodeSize::default(),
            default_metadata: CanvasNodeMetadata::default(),
            default_extra_metadata: MetadataExtras::new(),
            minimap_color: None,
            show_in_create_menu: true,
            has_source_handle: true,
            hide_panel: false,
            has_custom_panel: false,
            transparent_background: false,
            auto_open_panel: false,
            use_builtin_panel: None,
            interaction_toggle: false,
            force_interactive: None,
            keep_aspect_ratio: None,
            resource: None,
        }
    }
}

impl CanvasNodeDefinition {
    pub fn new(
        type_id: impl Into<String>,
        title: impl Into<String>,
        icon: impl Into<String>,
        default_size: NodeSize,
    ) -> Self {
        Self {
            type_id: type_id.into(),
            title: title.into(),
            icon: icon.into(),
            default_size,
            ..Self::default()
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_minimap_color(mut self, color: impl Into<String>) -> Self {
        self.minimap_color = Some(color.into());
        self
    }

    pub fn with_metadata(mut self, metadata: CanvasNodeMetadata) -> Self {
        self.default_metadata = metadata;
        self
    }

    /// `forceInteractive(node)`, defaulting to `false` when unset.
    pub fn is_force_interactive(&self, view: &NodeView<'_>) -> bool {
        self.force_interactive
            .as_ref()
            .is_some_and(|predicate| predicate(view))
    }

    /// `keepAspectRatio(node)`, defaulting to `false` when unset.
    pub fn keeps_aspect_ratio(&self, view: &NodeView<'_>) -> bool {
        self.keep_aspect_ratio
            .as_ref()
            .is_some_and(|predicate| predicate(view))
    }

    /// `resource(node)`, or `None` when the definition does not expose one.
    pub fn resolve_resource(&self, view: &NodeView<'_>) -> Option<CanvasNodeResource> {
        self.resource.as_ref().and_then(|resolver| resolver(view))
    }
}

impl fmt::Debug for CanvasNodeDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CanvasNodeDefinition")
            .field("type_id", &self.type_id)
            .field("title", &self.title)
            .field("icon", &self.icon)
            .field("description", &self.description)
            .field("default_size", &self.default_size)
            .field("default_metadata", &self.default_metadata)
            .field("default_extra_metadata", &self.default_extra_metadata)
            .field("minimap_color", &self.minimap_color)
            .field("show_in_create_menu", &self.show_in_create_menu)
            .field("has_source_handle", &self.has_source_handle)
            .field("hide_panel", &self.hide_panel)
            .field("has_custom_panel", &self.has_custom_panel)
            .field("transparent_background", &self.transparent_background)
            .field("auto_open_panel", &self.auto_open_panel)
            .field("use_builtin_panel", &self.use_builtin_panel)
            .field("interaction_toggle", &self.interaction_toggle)
            .field("force_interactive", &self.force_interactive.is_some())
            .field("keep_aspect_ratio", &self.keep_aspect_ratio.is_some())
            .field("resource", &self.resource.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_defaults_follow_the_web_comments() {
        let definition = CanvasNodeDefinition::default();
        assert!(definition.show_in_create_menu);
        assert!(definition.has_source_handle);
        assert!(!definition.hide_panel);
        assert!(!definition.transparent_background);
        assert!(!definition.auto_open_panel);
        assert!(!definition.interaction_toggle);
        assert_eq!(definition.default_size, NodeSize::new(340.0, 240.0));
    }

    #[test]
    fn resources_carry_their_kind() {
        assert_eq!(CanvasNodeResource::text("hi").kind, ResourceKind::Text);
        assert_eq!(CanvasNodeResource::image("u").kind, ResourceKind::Image);
        assert_eq!(CanvasNodeResource::video("u").kind, ResourceKind::Video);
    }

    #[test]
    fn unset_callbacks_have_false_defaults() {
        let node = open_canvas_core::CanvasNodeData {
            id: "n".into(),
            node_type: open_canvas_core::CanvasNodeType::Text,
            title: "n".into(),
            position: open_canvas_core::Position::new(0.0, 0.0),
            width: 1.0,
            height: 1.0,
            metadata: None,
        };
        let extras = MetadataExtras::new();
        let view = NodeView::new(&node, &extras);
        let definition = CanvasNodeDefinition::default();
        assert!(!definition.is_force_interactive(&view));
        assert!(!definition.keeps_aspect_ratio(&view));
        assert!(definition.resolve_resource(&view).is_none());
    }
}
