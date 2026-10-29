//! Sticky-note node logic, ported from
//! `plugins/canvas/sticky-note/src/index.tsx`.
//!
//! Pure half: the preset palette, the default color, and the note model read
//! from `metadata.content` + the plugin-private `metadata.pluginColor` key.
//! The throttled drag-preview and the DOM palette popover are UI concerns.

use open_canvas_core::CanvasNodeMetadata;

use crate::abi::{CanvasNodeDefinition, MetadataExtras, NodeSize};
use crate::plugin::{CanvasPlugin, PluginMetadataPatch};

pub const NODE_TYPE: &str = "sticky-note:note";
pub const MINIMAP_COLOR: &str = "#f59e0b";
/// Plugin-private metadata key holding the note color.
pub const COLOR_KEY: &str = "pluginColor";
const DEFAULT_SIZE: NodeSize = NodeSize {
    width: 240.0,
    height: 200.0,
};

/// The eight preset note colors (`sticky-note/index.tsx:7`).
pub const PRESET_COLORS: [&str; 8] = [
    "#fde68a", "#fca5a5", "#fdba74", "#a7f3d0", "#bfdbfe", "#ddd6fe", "#f9a8d4", "#e7e5e4",
];
pub const DEFAULT_COLOR: &str = PRESET_COLORS[0];
/// Empty-note label (`sticky-note/index.tsx:136`).
pub const DEFAULT_CONTENT: &str = "Double-click to edit this note";

pub fn palette() -> &'static [&'static str] {
    &PRESET_COLORS
}

/// Fall back to the default color when the stored value is missing or blank.
pub fn normalize_color(value: Option<&str>) -> String {
    match value {
        Some(color) if !color.trim().is_empty() => color.trim().to_string(),
        _ => DEFAULT_COLOR.to_string(),
    }
}

pub fn is_preset_color(color: &str) -> bool {
    PRESET_COLORS
        .iter()
        .any(|preset| preset.eq_ignore_ascii_case(color))
}

/// The note model: content plus resolved color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickyNote {
    pub content: String,
    pub color: String,
}

impl StickyNote {
    pub fn new(content: impl Into<String>, color: Option<&str>) -> Self {
        Self {
            content: content.into(),
            color: normalize_color(color),
        }
    }

    /// `sticky-note/index.tsx:19-21`: content then `pluginColor` metadata.
    pub fn from_parts(content: Option<&str>, plugin_color: Option<&str>) -> Self {
        Self::new(content.unwrap_or_default(), plugin_color)
    }

    /// Label shown when the note is empty.
    pub fn display_text(&self) -> &str {
        if self.content.is_empty() {
            DEFAULT_CONTENT
        } else {
            &self.content
        }
    }

    pub fn is_preset_color(&self) -> bool {
        is_preset_color(&self.color)
    }

    /// Metadata write for the host: core `content` plus the `pluginColor` extra.
    pub fn metadata_patch(&self) -> PluginMetadataPatch {
        let mut extra = MetadataExtras::new();
        extra.insert(
            COLOR_KEY.to_string(),
            serde_json::Value::String(self.color.clone()),
        );
        PluginMetadataPatch {
            metadata: CanvasNodeMetadata {
                content: Some(self.content.clone()),
                ..CanvasNodeMetadata::default()
            },
            extra,
        }
    }
}

/// Default extras for a newly created sticky note.
pub fn default_extras() -> MetadataExtras {
    let mut extra = MetadataExtras::new();
    extra.insert(
        COLOR_KEY.to_string(),
        serde_json::Value::String(DEFAULT_COLOR.to_string()),
    );
    extra
}

pub fn definition() -> CanvasNodeDefinition {
    let mut definition = CanvasNodeDefinition::new(NODE_TYPE, "Sticky note", "📌", DEFAULT_SIZE)
        .with_description("Colorful sticky note")
        .with_minimap_color(MINIMAP_COLOR)
        .with_metadata(CanvasNodeMetadata {
            content: Some(String::new()),
            ..CanvasNodeMetadata::default()
        });
    definition.default_extra_metadata = default_extras();
    definition.hide_panel = true;
    definition
}

pub fn plugin() -> CanvasPlugin {
    CanvasPlugin::new(
        "sticky-note",
        "Sticky note node",
        "1.1.0",
        vec![definition()],
    )
    .with_description("A sticky note with preset colors, double-click editing and drag to move")
}
