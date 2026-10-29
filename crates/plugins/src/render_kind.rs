//! Declarative node bodies the native host can already draw.
//!
//! A third-party plugin has no JS engine, so a node cannot ship React
//! `Content`/`Panel` components. Instead a manifest node **declares** one of a
//! fixed set of render kinds; the host maps it onto the built-in renderer for
//! that body. The set mirrors the pure-logic ports under [`crate::official`]
//! (markdown / svg / html / sticky-note / panorama) plus the built-in `image`
//! and `text` kinds.
//!
//! [`PluginRenderKind`] is the *resolved* enum; a [`crate::manifest::PluginNodeManifest`]
//! stores the declared key as a string and resolves it here, so an unknown key
//! is a typed [`crate::manifest::ManifestError`] rather than a silent fallback.

use serde::{Deserialize, Serialize};

use open_canvas_core::{CanvasGenerationMode, CanvasNodeMetadata, ResourceKind};

use crate::abi::NodeSize;

/// `CanvasResourceKind` (canvas-plugin.ts:9) as a serialisable manifest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginResourceKind {
    Image,
    Video,
    Text,
}

impl PluginResourceKind {
    /// The core resource kind this maps onto.
    pub fn to_core(self) -> ResourceKind {
        match self {
            Self::Image => ResourceKind::Image,
            Self::Video => ResourceKind::Video,
            Self::Text => ResourceKind::Text,
        }
    }

    /// The manifest value for a core resource kind.
    pub fn from_core(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Image => Self::Image,
            ResourceKind::Video => Self::Video,
            ResourceKind::Text => Self::Text,
        }
    }
}

/// `CanvasBuiltinPanelConfig.mode` (canvas-plugin.ts:89) as a manifest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginGenerationMode {
    Text,
    Image,
    Video,
}

impl PluginGenerationMode {
    /// The core generation mode this maps onto.
    pub fn to_core(self) -> CanvasGenerationMode {
        match self {
            Self::Text => CanvasGenerationMode::Text,
            Self::Image => CanvasGenerationMode::Image,
            Self::Video => CanvasGenerationMode::Video,
        }
    }
}

/// A declarative node body the native host can draw.
///
/// The keys are the wire values a manifest declares, e.g. `"markdown"` or
/// `"html-sandbox-text"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginRenderKind {
    /// Rendered Markdown document (`official::markdown`).
    Markdown,
    /// SVG vector source (`official::svg`), transparent background.
    Svg,
    /// Sandboxed HTML rendered from text (`official::html`).
    HtmlSandboxText,
    /// Colour sticky note (`official::sticky_note`).
    StickyNote,
    /// 360° panorama viewer placeholder (`official::panorama`).
    PanoramaPlaceholder,
    /// Still image from a URL/data URI (built-in `image`).
    Image,
    /// Plain text body (built-in `text`).
    Text,
}

impl PluginRenderKind {
    /// Every supported kind, in canonical order.
    pub const ALL: [PluginRenderKind; 7] = [
        Self::Markdown,
        Self::Svg,
        Self::HtmlSandboxText,
        Self::StickyNote,
        Self::PanoramaPlaceholder,
        Self::Image,
        Self::Text,
    ];

    /// The canonical wire key (`"markdown"`, `"html-sandbox-text"`, ...).
    pub fn key(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Svg => "svg",
            Self::HtmlSandboxText => "html-sandbox-text",
            Self::StickyNote => "sticky-note",
            Self::PanoramaPlaceholder => "panorama-placeholder",
            Self::Image => "image",
            Self::Text => "text",
        }
    }

    /// Resolve a declared key; `None` when the host cannot draw it.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Default node size, matching the official plugin of the same body
    /// (`plugins/canvas/*/src/index.tsx`).
    pub fn default_size(self) -> NodeSize {
        match self {
            Self::Markdown => NodeSize::new(360.0, 300.0),
            Self::Svg => NodeSize::new(320.0, 320.0),
            Self::HtmlSandboxText => NodeSize::new(420.0, 320.0),
            Self::StickyNote => NodeSize::new(240.0, 200.0),
            Self::PanoramaPlaceholder => NodeSize::new(480.0, 300.0),
            Self::Image | Self::Text => NodeSize::default(),
        }
    }

    /// Default metadata for a freshly created node of this body. Text-like
    /// bodies start with empty `content`; image/panorama/svg start unset, as
    /// their official plugins do.
    pub fn default_metadata(self) -> CanvasNodeMetadata {
        match self {
            Self::Markdown | Self::HtmlSandboxText | Self::StickyNote | Self::Text => {
                CanvasNodeMetadata {
                    content: Some(String::new()),
                    ..CanvasNodeMetadata::default()
                }
            }
            Self::Svg | Self::PanoramaPlaceholder | Self::Image => CanvasNodeMetadata::default(),
        }
    }

    /// Resource kind this body emits upstream when the manifest declares none.
    pub fn default_resource_kind(self) -> Option<ResourceKind> {
        match self {
            Self::Markdown | Self::HtmlSandboxText | Self::Text => Some(ResourceKind::Text),
            Self::Image | Self::PanoramaPlaceholder => Some(ResourceKind::Image),
            Self::Svg | Self::StickyNote => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_through_the_enum() {
        for kind in PluginRenderKind::ALL {
            assert_eq!(PluginRenderKind::from_key(kind.key()), Some(kind));
        }
        assert_eq!(PluginRenderKind::from_key("acme:future"), None);
    }

    #[test]
    fn resource_and_generation_modes_map_to_core() {
        assert_eq!(PluginResourceKind::Text.to_core(), ResourceKind::Text);
        assert_eq!(
            PluginResourceKind::from_core(ResourceKind::Video),
            PluginResourceKind::Video
        );
        assert_eq!(
            PluginGenerationMode::Video.to_core(),
            CanvasGenerationMode::Video
        );
    }

    #[test]
    fn kinds_carry_their_own_defaults() {
        assert_eq!(
            PluginRenderKind::StickyNote.default_size(),
            NodeSize::new(240.0, 200.0)
        );
        assert_eq!(
            PluginRenderKind::Markdown.default_resource_kind(),
            Some(ResourceKind::Text)
        );
        assert_eq!(PluginRenderKind::Svg.default_resource_kind(), None);
        assert_eq!(
            PluginRenderKind::Markdown.default_metadata().content,
            Some(String::new())
        );
        assert_eq!(PluginRenderKind::Svg.default_metadata().content, None);
    }
}
