//! Manifest data types: the declarative mirror of `CanvasPlugin` /
//! `CanvasNodeDefinition` (canvas-plugin.ts:101-149) without React members.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use open_canvas_core::CanvasNodeMetadata;

use crate::abi::MetadataExtras;
use crate::render_kind::{PluginGenerationMode, PluginRenderKind, PluginResourceKind};

/// Fallback locale for [`PluginManifest::localized`].
pub const DEFAULT_LOCALE: &str = "en-US";
/// Extra-metadata key holding the resolved render kind, so the host renderer
/// knows which declarative body to draw for a plugin node.
pub const RENDER_KIND_KEY: &str = "pluginRenderKind";
/// Smallest accepted node side, in canvas units.
pub const MIN_NODE_SIDE: f64 = 40.0;
/// Largest accepted node side, in canvas units.
pub const MAX_NODE_SIDE: f64 = 8000.0;
/// Icon keys the native host can resolve to a vector glyph. Any other icon must
/// be a non-ASCII emoji/glyph (plugin nodes use emoji, built-ins use these).
pub const KNOWN_ICON_KEYS: [&str; 13] = [
    "file-text",
    "message-square-text",
    "clapperboard",
    "image",
    "images",
    "video",
    "sparkles",
    "folder-input",
    "puzzle",
    "globe",
    "sticky-note",
    "code",
    "type",
];

/// `{width, height}` default size as declared in a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSize {
    pub width: f64,
    pub height: f64,
}

/// `useBuiltinPanel` (canvas-plugin.ts:89): reuse the host generation panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginBuiltinPanelManifest {
    pub mode: PluginGenerationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub write_back_to_self: bool,
}

/// The lower panel a node declares: a host built-in generation panel or a
/// custom panel supplied by the host's plugin body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPanelManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<PluginBuiltinPanelManifest>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub custom: bool,
}

/// One declarative node definition, mirroring `CanvasNodeDefinition`
/// (canvas-plugin.ts:101) without its React members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginNodeManifest {
    /// Wire type id (`"<pluginId>:<name>"`). Serialised as `"type"` to match the
    /// web contract; `"typeId"` is accepted as an alias.
    #[serde(rename = "type", alias = "typeId", default)]
    pub type_id: String,
    #[serde(default)]
    pub title: String,
    /// Declared body key, resolved against [`crate::render_kind::PluginRenderKind`].
    #[serde(default)]
    pub render_kind: String,
    /// Icon key (lucide name) or emoji.
    #[serde(default)]
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<PluginSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_metadata: Option<CanvasNodeMetadata>,
    /// Plugin-private default keys, see [`MetadataExtras`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_extra_metadata: MetadataExtras,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimap_color: Option<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub show_in_create_menu: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub has_source_handle: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_panel: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub transparent_background: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_open_panel: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interaction_toggle: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PluginPanelManifest>,
    /// Resource emitted upstream; the render kind supplies a default when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<PluginResourceKind>,
}

/// The kind of value a settings field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSettingKind {
    String,
    Number,
    Boolean,
    Select,
}

/// One entry of a plugin's settings schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingSpec {
    pub kind: PluginSettingKind,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// A declarative plugin manifest. Mirrors `CanvasPlugin` (canvas-plugin.ts:138)
/// minus the browser-only members (`css`, `setup`, React components).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub nodes: Vec<PluginNodeManifest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, PluginSettingSpec>,
    /// `i18n[locale][key]` strings, e.g. `i18n["zh-CN"]["name"]`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub i18n: BTreeMap<String, BTreeMap<String, String>>,
}

impl PluginManifest {
    /// Fill in the web-compatible fallbacks and canonicalise declared keys, so
    /// a parsed manifest is self-consistent and re-emittable byte-stably.
    pub(crate) fn normalize(&mut self) {
        if self.name.trim().is_empty() {
            self.name = self.id.clone();
        }
        if self.version.trim().is_empty() {
            self.version = "0.0.0".to_string();
        }
        for node in &mut self.nodes {
            if let Some(kind) = PluginRenderKind::from_key(&node.render_kind) {
                node.render_kind = kind.key().to_string();
            }
        }
    }

    /// `i18n[locale][key]`, falling back to [`DEFAULT_LOCALE`].
    pub fn localized(&self, locale: &str, key: &str) -> Option<&str> {
        self.i18n
            .get(locale)
            .and_then(|table| table.get(key))
            .or_else(|| {
                self.i18n
                    .get(DEFAULT_LOCALE)
                    .and_then(|table| table.get(key))
            })
            .map(String::as_str)
    }

    /// Localised plugin name, falling back to the base name.
    pub fn name_for(&self, locale: &str) -> &str {
        self.localized(locale, "name").unwrap_or(&self.name)
    }

    /// Localised description, falling back to the base description.
    pub fn description_for(&self, locale: &str) -> Option<&str> {
        self.localized(locale, "description")
            .or(self.description.as_deref())
    }
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}
