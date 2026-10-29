//! `open-canvas-plugins` — the native, UI-agnostic plugin/node ABI (P5e).
//!
//! The web plugin contract (`web/src/types/canvas-plugin.ts`) is React/DOM-bound:
//! node content and panels are React components, icons are `ReactNode`, styles go
//! into `document.head`, and plugin bundles are ESM blobs dynamically imported
//! from CDNs. None of that can run under Slint, so this crate defines the native
//! equivalent and ports only the render-free logic:
//!
//! - [`abi`] — [`abi::CanvasNodeDefinition`], [`abi::CanvasPlugin`], resource
//!   kinds, and the built-in panel config (plain data; no React).
//! - [`registry`] — [`registry::NodeRegistry`] mirroring `node-registry.ts`,
//!   including the ten built-in definitions.
//! - [`context`] — the [`context::CanvasNodeContext`] host trait.
//! - [`event_bus`] — the in-memory [`event_bus::CanvasEventBus`].
//! - [`storage`] — the host [`storage::PluginStorage`] trait plus an in-memory
//!   implementation and a per-plugin namespace wrapper.
//! - [`official`] — native ports of the pure-logic parts of the five official
//!   plugins (html / markdown / panorama / sticky-note / svg).
//! - [`view`] — the host bridge: a UI-agnostic [`view::PluginNodeView`] the Slint
//!   app renders from a node + its registry definition.
//! - [`manager`] — installed-plugin records, enable/disable/install/uninstall
//!   state, the official version check, and the declarative [`manager::InstalledRecord`]
//!   / [`manager::InstalledRegistry`] persistence seam.
//! - [`manifest`] — the declarative plugin manifest ([`manifest::PluginManifest`])
//!   that replaces a JS bundle: parse/validate/emit, render-kind mapping, settings
//!   schema, and i18n.
//! - [`render_kind`] — [`render_kind::PluginRenderKind`], the set of node bodies
//!   the native host can draw declaratively.
//!
//! ## Deliberately deferred
//!
//! Slint rendering of node content and panels, the browser-only host surfaces
//! (`theme`, `ai`, `applyOps`, CSS injection, `setup`), network fetching /
//! dynamic JavaScript import of plugin bundles, and the model-plugin
//! request-script runner are later tasks. See `docs/research/p5/ai-plugins.md` §4.

pub mod abi;
pub mod builtins;
pub mod context;
pub mod event_bus;
pub mod manager;
pub mod manifest;
pub mod official;
pub mod plugin;
pub mod registry;
pub mod render_kind;
pub mod storage;
pub mod view;

pub use abi::{
    BuiltinPanelConfig, CanvasNodeDefinition, CanvasNodeResource, MetadataExtras, NodePredicate,
    NodeSize, NodeView, ResourceResolver,
};
pub use builtins::{builtin_definitions, register_builtin_nodes};
pub use context::{CanvasNodeContext, NodePatch};
pub use event_bus::{CanvasEventBus, EventHandler, EventPayload, EventSubscription};
pub use manager::{
    compare_semver, has_upgrade, shows_upgrade_dot, upgrade_label, InstalledPlugin,
    InstalledRecord, InstalledRegistry, OfficialPluginEntry, PluginBundle, PluginManager,
};
pub use manifest::{
    emit_manifest, parse_manifest, ManifestError, PluginBuiltinPanelManifest, PluginManifest,
    PluginNodeManifest, PluginPanelManifest, PluginSettingKind, PluginSettingSpec, PluginSize,
    DEFAULT_LOCALE, RENDER_KIND_KEY,
};
pub use official::{official_plugins, register_official_plugins};
pub use plugin::{CanvasPlugin, PluginMetadataPatch};
pub use registry::{is_builtin_node_type, NodeRegistry, BUILTIN_PLUGIN_ID};
pub use render_kind::{PluginGenerationMode, PluginRenderKind, PluginResourceKind};
pub use storage::{MemoryPluginStore, NamespacedStorage, PluginError, PluginStorage};
pub use view::{plugin_node_view, BackgroundMode, PanelKind, PluginNodeView};
