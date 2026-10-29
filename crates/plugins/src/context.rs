//! The host surface injected while a plugin node renders, mirroring
//! `CanvasNodeContext` (canvas-plugin.ts:42) as a Rust trait.
//!
//! Deliberately not modelled yet (all are host/UI integration deferred to the
//! Slint task): `theme` (no native theme type exists yet), `ai`
//! (`CanvasPluginAi` async generation), and `applyOps` (the agent instruction
//! set). Everything that is pure graph/data access is here.

use open_canvas_core::{CanvasConnection, CanvasNodeData, CanvasNodeMetadata};

use crate::abi::MetadataExtras;
use crate::event_bus::{EventHandler, EventPayload, EventSubscription};
use crate::storage::PluginStorage;

/// `updateNode(patch)` — the node fields a plugin may patch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodePatch {
    pub title: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

impl NodePatch {
    pub fn title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::default()
        }
    }

    pub fn size(width: f64, height: f64) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            ..Self::default()
        }
    }
}

/// Context handed to a plugin node. The host implements it; plugins only read.
pub trait CanvasNodeContext {
    fn node(&self) -> &CanvasNodeData;

    /// Plugin-private metadata (custom keys beside the core schema).
    fn extras(&self) -> &MetadataExtras;

    fn scale(&self) -> f64;
    fn is_selected(&self) -> bool;

    fn update_metadata(&mut self, patch: CanvasNodeMetadata);
    fn update_extra_metadata(&mut self, key: &str, value: EventPayload);
    fn update_node(&mut self, patch: NodePatch);

    fn get_node(&self, id: &str) -> Option<CanvasNodeData>;
    fn get_nodes(&self) -> Vec<CanvasNodeData>;
    fn get_connections(&self) -> Vec<CanvasConnection>;
    fn get_upstream(&self) -> Vec<CanvasNodeData>;
    fn get_downstream(&self) -> Vec<CanvasNodeData>;

    fn emit(&self, event: &str, payload: Option<&EventPayload>);
    fn on(&self, event: &str, handler: EventHandler) -> EventSubscription;

    fn open_panel(&mut self);
    fn close_panel(&mut self);

    fn storage(&self) -> &dyn PluginStorage;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts_dyn(_context: &dyn CanvasNodeContext) {}

    #[test]
    fn context_trait_is_object_safe() {
        let _: fn(&dyn CanvasNodeContext) = accepts_dyn;
    }
}
