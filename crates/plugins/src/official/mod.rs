//! Native ports of the pure-logic parts of the official plugins
//! (`plugins/canvas/*/src/index.tsx`).
//!
//! Only the render-free logic is ported: template/substitution, text transforms,
//! validation, and the metadata models the Slint renderer will consume later.
//! `Content`/`Panel` React components and the CDN `import()`s (three, marked)
//! have no native equivalent and are out of scope (they need actual renderers).

pub mod html;
pub mod markdown;
pub mod panorama;
pub mod sticky_note;
pub mod svg;

use crate::plugin::CanvasPlugin;
use crate::registry::NodeRegistry;

/// The five official plugins ported here, in registry build order
/// (`plugins/canvas/registry/package.json`).
pub fn official_plugins() -> Vec<CanvasPlugin> {
    vec![
        html::plugin(),
        markdown::plugin(),
        panorama::plugin(),
        sticky_note::plugin(),
        svg::plugin(),
    ]
}

/// Register every official plugin's node definitions under its id, returning
/// the number of definitions registered.
pub fn register_official_plugins(registry: &NodeRegistry) -> usize {
    let plugins = official_plugins();
    let count = plugins.iter().map(|plugin| plugin.nodes.len()).sum();
    for plugin in &plugins {
        registry.register_plugin(plugin);
    }
    count
}
