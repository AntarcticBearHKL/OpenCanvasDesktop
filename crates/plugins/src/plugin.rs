//! Plugin package types (`CanvasPlugin`, `CanvasPluginApp` neighbours).
//!
//! The web `setup(app)` hook injects CSS into `document.head` and hands the
//! plugin the host React runtime; both are browser integration with no native
//! equivalent, so the hook is not modelled here (see the crate docs).

use open_canvas_core::CanvasNodeMetadata;

use crate::abi::{CanvasNodeDefinition, MetadataExtras};

/// A metadata write coming from a plugin: core fields plus plugin extras.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginMetadataPatch {
    pub metadata: CanvasNodeMetadata,
    pub extra: MetadataExtras,
}

/// `CanvasPlugin` (canvas-plugin.ts:138) as plain data.
#[derive(Debug, Clone, Default)]
pub struct CanvasPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub min_app_version: Option<String>,
    pub css: Option<String>,
    pub nodes: Vec<CanvasNodeDefinition>,
}

impl CanvasPlugin {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        nodes: Vec<CanvasNodeDefinition>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: None,
            min_app_version: None,
            css: None,
            nodes,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
