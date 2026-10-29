//! Slint-facing rows and decisions derived from the plugin host: the manager
//! modal's tab lists and the plugin-contributed hover-toolbar items.
//!
//! The hover items mirror `buildNodeToolbarItems` (`use-plugin-host.tsx:128`)
//! and each official plugin's `toolbar` declaration, mapped onto what the
//! native ABI declares:
//!
//! - `interactionToggle` + content + not forced-interactive -> the host
//!   interaction toggle (flips `metadata.interactive`; active when set).
//! - `useBuiltinPanel` (panorama) -> "Generate with AI" (`plugin-generate`, opens
//!   the panel) and, once the node has its own content, "Reset content"
//!   (`plugin-reset`, clears the content).
//! - a definition declaring a color extra (`pluginColor`, sticky-note) -> the
//!   palette item (`plugin-palette`).

use open_canvas_core::node::CanvasNodeData;
use open_canvas_plugins::official::sticky_note;

use crate::plugin_host::PluginHost;
use crate::plugin_manager_model::ManagerTab;
use crate::{HoverAction, PluginManagerRow};

/// The built-in node types the dock's node buttons create, in dock order.
/// Text and video are gone; a prompt is now part of an Image Attempt, and an
/// Image node is a real empty display/management node that accepts a drop.
pub const CREATABLE_NODE_TYPES: [&str; 5] = [
    "image-attempt",
    "image",
    "image-stack",
    "assets",
    "smart-canvas",
];

/// Whether `type_id` is one of the dock's creatable built-in node types.
pub fn is_creatable_node_type(type_id: &str) -> bool {
    CREATABLE_NODE_TYPES.contains(&type_id)
}

/// Manager-modal rows for a tab.
pub fn manager_rows(host: &PluginHost, tab: ManagerTab) -> Vec<PluginManagerRow> {
    match tab {
        ManagerTab::Official => PluginHost::official_entries()
            .into_iter()
            .map(|entry| {
                let installed = host.manager.get(&entry.id);
                PluginManagerRow {
                    upgradable: installed.is_some_and(|record| entry.upgrades(record)),
                    enabled: installed.is_some_and(|record| record.enabled),
                    installed: installed.is_some(),
                    id: entry.id.into(),
                    name: entry.name.into(),
                    version: entry.version.clone().into(),
                    description: entry.description.unwrap_or_default().into(),
                    url: entry.url.into(),
                    source: "builtin".into(),
                    badge: "official".into(),
                    update_version: entry.version.into(),
                }
            })
            .collect(),
        ManagerTab::Local => host
            .manager
            .plugins()
            .iter()
            .filter(|record| record.local)
            .map(|record| row_from_record(record, "local"))
            .collect(),
        ManagerTab::ThirdParty => host
            .manager
            .plugins()
            .iter()
            .filter(|record| !record.local && !record.official)
            .map(|record| row_from_record(record, "third-party"))
            .collect(),
    }
}

fn row_from_record(
    record: &open_canvas_plugins::manager::InstalledPlugin,
    badge: &str,
) -> PluginManagerRow {
    PluginManagerRow {
        id: record.id.clone().into(),
        name: record.name.clone().into(),
        version: record.version.clone().into(),
        description: record.description.clone().unwrap_or_default().into(),
        url: record.url.clone().into(),
        source: record.source.clone().into(),
        badge: badge.into(),
        enabled: record.enabled,
        installed: true,
        upgradable: false,
        update_version: record.version.clone().into(),
    }
}

/// Plugin-contributed hover-toolbar items for `node`, in the web's order
/// (host toggle first, then the plugin's own items).
pub fn toolbar_extras(host: &PluginHost, node: &CanvasNodeData) -> Vec<HoverAction> {
    let Some(definition) = host.registry.get(node.node_type.as_str()) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    let content = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.clone())
        .unwrap_or_default();
    let extras = host.extras_of(&node.id);
    let view = open_canvas_plugins::abi::NodeView::new(node, &extras);
    if definition.interaction_toggle
        && !content.is_empty()
        && !definition.is_force_interactive(&view)
    {
        actions.push(HoverAction {
            id: "plugin-interact".into(),
            danger: false,
            active: node
                .metadata
                .as_ref()
                .and_then(|meta| meta.interactive)
                .unwrap_or(false),
        });
    }
    if definition.use_builtin_panel.is_some() {
        actions.push(HoverAction {
            id: "plugin-generate".into(),
            danger: false,
            active: false,
        });
        if !content.is_empty() {
            actions.push(HoverAction {
                id: "plugin-reset".into(),
                danger: false,
                active: false,
            });
        }
    }
    if definition
        .default_extra_metadata
        .contains_key(sticky_note::COLOR_KEY)
    {
        actions.push(HoverAction {
            id: "plugin-palette".into(),
            danger: false,
            active: host.palette_node.as_deref() == Some(node.id.as_str()),
        });
    }
    actions
}

/// `#rgb` / `#rrggbb` / `#rrggbbaa` (a plugin-declared metadata color, e.g.
/// sticky-note `pluginColor`) -> Slint color. `None` for anything else.
pub fn parse_hex_color(hex: &str) -> Option<slint::Color> {
    let hex = hex.trim().trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        3 => (
            nibble(hex, 0)? * 17,
            nibble(hex, 1)? * 17,
            nibble(hex, 2)? * 17,
            255,
        ),
        6 => (pair(hex, 0)?, pair(hex, 2)?, pair(hex, 4)?, 255),
        8 => (pair(hex, 0)?, pair(hex, 2)?, pair(hex, 4)?, pair(hex, 6)?),
        _ => return None,
    };
    Some(slint::Color::from_argb_u8(a, r, g, b))
}

/// The sticky-note preset palette as Slint colors (the plugin's declared
/// `PRESET_COLORS`, in order (the card's palette row).
pub fn palette_colors() -> Vec<slint::Color> {
    sticky_note::palette()
        .iter()
        .filter_map(|hex| parse_hex_color(hex))
        .collect()
}

fn pair(hex: &str, index: usize) -> Option<u8> {
    u8::from_str_radix(hex.get(index..index + 2)?, 16).ok()
}

fn nibble(hex: &str, index: usize) -> Option<u8> {
    u8::from_str_radix(hex.get(index..index + 1)?, 16).ok()
}

#[cfg(test)]
#[path = "plugin_tests/views_tests.rs"]
mod tests;
