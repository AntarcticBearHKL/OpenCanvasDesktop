//! Node definition registry, mirroring `web/src/lib/canvas/node-registry.ts`.
//!
//! Same semantics as the web module:
//! - registering a type again replaces it in place (JS `Map.set` keeps position),
//! - every registration/removal bumps a version counter the UI can watch,
//! - `get_spec` falls back to 340x240 / "Node" / empty metadata,
//! - ownership is tracked per type so a plugin can be unregistered wholesale,
//! - `is_builtin` is a property of the type id, not of the registry.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use open_canvas_core::{
    CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, NodeSpec, Position, FALLBACK_NODE_HEIGHT,
    FALLBACK_NODE_WIDTH,
};

use crate::abi::{CanvasNodeDefinition, MetadataExtras};
use crate::view::PluginNodeView;

/// Owner id of the ten built-in node kinds.
pub const BUILTIN_PLUGIN_ID: &str = "builtin";

struct RegistryEntry {
    plugin_id: String,
    definition: CanvasNodeDefinition,
}

#[derive(Default)]
struct RegistryState {
    entries: Vec<RegistryEntry>,
    version: u64,
}

/// Thread-safe definition store shared by the canvas host and plugins.
#[derive(Default)]
pub struct NodeRegistry {
    state: RwLock<RegistryState>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry that already owns the ten built-in definitions.
    pub fn with_builtin_nodes() -> Self {
        let registry = Self::new();
        crate::builtins::register_builtin_nodes(&registry);
        registry
    }

    fn read(&self) -> RwLockReadGuard<'_, RegistryState> {
        match self.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write(&self) -> RwLockWriteGuard<'_, RegistryState> {
        match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Register definitions owned by `plugin_id` (`registerNodeDefinitions`).
    /// Re-registering a type replaces it and keeps list position.
    pub fn register(
        &self,
        plugin_id: &str,
        definitions: impl IntoIterator<Item = CanvasNodeDefinition>,
    ) {
        let mut state = self.write();
        for definition in definitions {
            let entry = RegistryEntry {
                plugin_id: plugin_id.to_string(),
                definition,
            };
            match state
                .entries
                .iter_mut()
                .find(|current| current.definition.type_id == entry.definition.type_id)
            {
                Some(current) => *current = entry,
                None => state.entries.push(entry),
            }
        }
        state.version += 1;
    }

    /// Register a plugin's node definitions under its id.
    pub fn register_plugin(&self, plugin: &crate::plugin::CanvasPlugin) {
        self.register(&plugin.id, plugin.nodes.iter().cloned());
    }

    /// Remove every definition owned by `plugin_id` (`unregisterPluginNodes`).
    /// Returns the number of removed definitions.
    pub fn unregister_plugin(&self, plugin_id: &str) -> usize {
        let mut state = self.write();
        let before = state.entries.len();
        state.entries.retain(|entry| entry.plugin_id != plugin_id);
        let removed = before - state.entries.len();
        state.version += 1;
        removed
    }

    pub fn get(&self, type_id: &str) -> Option<CanvasNodeDefinition> {
        self.read()
            .entries
            .iter()
            .find(|entry| entry.definition.type_id == type_id)
            .map(|entry| entry.definition.clone())
    }

    pub fn list(&self) -> Vec<CanvasNodeDefinition> {
        self.read()
            .entries
            .iter()
            .map(|entry| entry.definition.clone())
            .collect()
    }

    /// `getNodeSpec(type)`: a registered definition's size/title/metadata, or
    /// the web registry fallback for unknown types.
    pub fn get_spec(&self, type_id: &str) -> NodeSpec {
        match self.get(type_id) {
            Some(definition) => NodeSpec {
                width: definition.default_size.width,
                height: definition.default_size.height,
                title: definition.title,
                metadata: definition.default_metadata,
            },
            None => NodeSpec {
                width: FALLBACK_NODE_WIDTH,
                height: FALLBACK_NODE_HEIGHT,
                title: "Node".to_string(),
                metadata: CanvasNodeMetadata::default(),
            },
        }
    }

    /// Plugin-private default metadata for a registered definition.
    pub fn get_default_extras(&self, type_id: &str) -> MetadataExtras {
        self.get(type_id)
            .map_or_else(MetadataExtras::new, |definition| {
                definition.default_extra_metadata
            })
    }

    /// Resolve the renderable host-bridge spec for `node` using its registered
    /// definition (or the missing-plugin placeholder when the type is unknown).
    pub fn node_view(&self, node: &CanvasNodeData, extras: &MetadataExtras) -> PluginNodeView {
        let definition = self.get(node.node_type.as_str());
        crate::view::plugin_node_view(node, extras, definition.as_ref())
    }

    pub fn is_registered(&self, type_id: &str) -> bool {
        self.read()
            .entries
            .iter()
            .any(|entry| entry.definition.type_id == type_id)
    }

    /// Owner plugin id, defaulting to [`BUILTIN_PLUGIN_ID`]
    /// (`getNodePluginId`, node-registry.ts:38).
    pub fn get_node_plugin_id(&self, type_id: &str) -> String {
        self.read()
            .entries
            .iter()
            .find(|entry| entry.definition.type_id == type_id)
            .map_or_else(
                || BUILTIN_PLUGIN_ID.to_string(),
                |entry| entry.plugin_id.clone(),
            )
    }

    /// `isBuiltinNodeType`: the ten core kinds, independent of registration.
    pub fn is_builtin(type_id: &str) -> bool {
        is_builtin_node_type(type_id)
    }

    pub fn version(&self) -> u64 {
        self.read().version
    }

    pub fn len(&self) -> usize {
        self.read().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.read().entries.is_empty()
    }
}

/// `isBuiltinNodeType` (node-registry.ts:59): the ten core kinds, independent
/// of whether their definitions have been registered.
pub fn is_builtin_node_type(type_id: &str) -> bool {
    CanvasNodeType::from_id(type_id).is_builtin()
}

/// A detached node for registry tests and host defaults.
pub fn detached_node(id: &str, node_type: CanvasNodeType) -> CanvasNodeData {
    CanvasNodeData {
        id: id.to_string(),
        node_type,
        title: id.to_string(),
        position: Position::new(0.0, 0.0),
        width: FALLBACK_NODE_WIDTH,
        height: FALLBACK_NODE_HEIGHT,
        metadata: Some(CanvasNodeMetadata::default()),
    }
}
