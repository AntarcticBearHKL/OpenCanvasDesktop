//! Manifest → runtime ABI conversion: resolve each declared node onto a
//! [`PluginRenderKind`] body and build the [`CanvasNodeDefinition`] the registry
//! and host renderer consume.

use std::sync::Arc;

use serde_json::Value;

use open_canvas_core::ResourceKind;

use super::error::ManifestError;
use super::model::{PluginManifest, PluginNodeManifest, RENDER_KIND_KEY};
use crate::abi::{
    BuiltinPanelConfig, CanvasNodeDefinition, CanvasNodeResource, MetadataExtras, NodeSize,
    NodeView, ResourceResolver,
};
use crate::plugin::CanvasPlugin;
use crate::render_kind::{PluginRenderKind, PluginResourceKind};

impl PluginManifest {
    /// Convert the whole manifest into the runtime [`CanvasPlugin`] shape the
    /// registry consumes. Errors on a node whose render kind is unsupported.
    pub fn to_plugin(&self) -> Result<CanvasPlugin, ManifestError> {
        let nodes = self
            .nodes
            .iter()
            .map(PluginNodeManifest::to_definition)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CanvasPlugin {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            min_app_version: self.min_app_version.clone(),
            css: None,
            nodes,
        })
    }
}

impl PluginNodeManifest {
    /// Resolve the declared render kind, or [`ManifestError::UnsupportedRenderKind`].
    pub fn render_kind(&self) -> Result<PluginRenderKind, ManifestError> {
        PluginRenderKind::from_key(&self.render_kind).ok_or_else(|| {
            ManifestError::UnsupportedRenderKind {
                type_id: self.type_id.clone(),
                render_kind: self.render_kind.clone(),
            }
        })
    }

    /// The definition the registry and host renderer consume. The manifest's
    /// explicit values win; a render kind supplies the rest (size, metadata,
    /// resource) and records itself in [`RENDER_KIND_KEY`].
    pub fn to_definition(&self) -> Result<CanvasNodeDefinition, ManifestError> {
        let kind = self.render_kind()?;
        let size = self.default_size.map_or_else(
            || kind.default_size(),
            |size| NodeSize::new(size.width, size.height),
        );
        let mut definition = CanvasNodeDefinition::new(
            self.type_id.clone(),
            self.title.clone(),
            self.icon.clone(),
            size,
        );
        definition.description = self.description.clone();
        definition.default_metadata = self
            .default_metadata
            .clone()
            .unwrap_or_else(|| kind.default_metadata());
        definition.default_extra_metadata = self.extra_metadata(kind);
        definition.minimap_color = self.minimap_color.clone();
        definition.show_in_create_menu = self.show_in_create_menu;
        definition.has_source_handle = self.has_source_handle;
        definition.hide_panel = self.hide_panel;
        definition.transparent_background = self.transparent_background;
        definition.auto_open_panel = self.auto_open_panel;
        definition.interaction_toggle = self.interaction_toggle;
        if let Some(panel) = &self.panel {
            if let Some(builtin) = &panel.builtin {
                definition.use_builtin_panel = Some(BuiltinPanelConfig {
                    mode: builtin.mode.to_core(),
                    prompt_prefix: builtin.prompt_prefix.clone(),
                    write_back_to_self: builtin.write_back_to_self,
                });
            }
            definition.has_custom_panel = panel.custom;
        }
        definition.resource = self.resource_resolver(kind);
        Ok(definition)
    }

    fn extra_metadata(&self, kind: PluginRenderKind) -> MetadataExtras {
        let mut extras = self.default_extra_metadata.clone();
        extras.insert(
            RENDER_KIND_KEY.to_string(),
            Value::String(kind.key().to_string()),
        );
        extras
    }

    fn resource_resolver(&self, kind: PluginRenderKind) -> Option<ResourceResolver> {
        let resource_kind = self
            .resource_kind
            .map(PluginResourceKind::to_core)
            .or_else(|| kind.default_resource_kind())?;
        Some(Arc::new(move |view: &NodeView<'_>| {
            let content = view
                .metadata()?
                .content
                .as_deref()
                .filter(|text| !text.is_empty())?;
            Some(match resource_kind {
                ResourceKind::Image => CanvasNodeResource::image(content),
                ResourceKind::Video => CanvasNodeResource::video(content),
                ResourceKind::Text => CanvasNodeResource::text(content),
            })
        }))
    }
}
