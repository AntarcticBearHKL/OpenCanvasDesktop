//! Render-ready content of a plugin node, resolved natively (no React/DOM).
//!
//! The five official plugins render through React components in the web; the
//! native equivalent is a plain data cell the Slint card consumes. Only the
//! render-free logic of `crates/plugins::official` is reused:
//!
//! - `sticky-note:note` —colored note (preset color from the `pluginColor`
//!   extra, empty note shows the plugin's own placeholder label).
//! - `markdown:doc` —the documented markdown subset converted to plain text by
//!   [`markdown::to_text`] (ATX headings, fences, quotes, lists, inline marks;
//!   no tables).
//! - `svg:vector` —the effective source (own content, else an upstream SVG) is
//!   rasterized headlessly with `resvg`; sources that fail the plugins-crate
//!   validator (or rasterization) fall back to a text placeholder.
//! - `html:render` —no webview: the `{{input}}`-substituted text is shown in a
//!   sandbox placeholder styled by the card.
//! - `panorama:viewer` —no 3D renderer: an equirectangular placeholder plus
//!   the image-gen panel hook (the panel itself comes from the bridge).
//!
//! This is fully code-verified: [`PluginCell::svg_image`] produces a Slint
//! `Image` whose buffer can be asserted on in tests.

use open_canvas_core::node::CanvasNodeData;
use open_canvas_plugins::abi::MetadataExtras;
use open_canvas_plugins::official::{html, markdown, panorama, sticky_note, svg};

/// Longest raster edge for an SVG node (`resvg` output cap).
pub const SVG_MAX_EDGE: u32 = 720;

/// Which native renderer draws the node content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCellKind {
    /// Not a plugin type (the card's built-in body path handles it).
    None,
    /// Plugin type with no registered definition (missing-plugin placeholder).
    Missing,
    Sticky,
    Markdown,
    Svg,
    Html,
    Panorama,
}

/// Resolved content of one plugin node.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginCell {
    pub kind: PluginCellKind,
    /// Body text (note text, markdown text, html text, placeholder label).
    pub text: String,
    /// The raw `metadata.content` the plugin editor writes back (editing mode).
    pub raw: String,
    /// The text is a placeholder (empty document / missing source).
    pub empty: bool,
    /// The plugin's edit mode is on (`editing` extra).
    pub editing: bool,
    /// Sticky-note background (preset hex), already normalized.
    pub color: Option<String>,
    /// Effective SVG source (validated) when `kind == Svg`.
    pub svg_source: Option<String>,
}

impl PluginCell {
    fn plain(kind: PluginCellKind) -> Self {
        Self {
            kind,
            text: String::new(),
            raw: String::new(),
            empty: false,
            editing: false,
            color: None,
            svg_source: None,
        }
    }

    fn with_text(kind: PluginCellKind, text: String, empty: bool, editing: bool) -> Self {
        Self {
            text,
            empty,
            editing,
            ..Self::plain(kind)
        }
    }

    /// Rasterize the cell's SVG source at its native size, capped to
    /// [`SVG_MAX_EDGE`]. `None` for non-SVG cells or invalid sources; the card
    /// then shows the text placeholder instead.
    pub fn svg_image(&self) -> Option<slint::Image> {
        if self.kind != PluginCellKind::Svg {
            return None;
        }
        rasterize_svg(self.svg_source.as_deref()?)
    }
}

/// Resolve the content cell of `node`. `registered` is the bridge's answer for
/// whether a definition exists; `upstream` are the nodes feeding this one
/// (web `ctx.getUpstream()`), used by the html/svg/panorama ports.
pub fn plugin_cell(
    node: &CanvasNodeData,
    extras: &MetadataExtras,
    registered: bool,
    upstream: &[CanvasNodeData],
) -> PluginCell {
    if node.node_type.is_builtin() {
        return PluginCell::plain(PluginCellKind::None);
    }
    if !registered {
        return PluginCell::plain(PluginCellKind::Missing);
    }
    let editing = is_editing(extras);
    let raw = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.as_deref())
        .unwrap_or_default()
        .to_string();
    let content = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.as_deref());
    let mut cell = match node.node_type.as_str() {
        sticky_note::NODE_TYPE => {
            let color = extras
                .get(sticky_note::COLOR_KEY)
                .and_then(serde_json::Value::as_str);
            let sticky = sticky_note::StickyNote::from_parts(content, color);
            let mut cell = PluginCell::with_text(
                PluginCellKind::Sticky,
                sticky.display_text().to_string(),
                sticky.content.is_empty(),
                editing,
            );
            cell.color = Some(sticky.color);
            cell
        }
        markdown::NODE_TYPE => {
            let source = content.unwrap_or_default();
            let text = if source.is_empty() {
                markdown::EMPTY_PLACEHOLDER.to_string()
            } else {
                markdown::to_text(source)
            };
            PluginCell::with_text(PluginCellKind::Markdown, text, source.is_empty(), editing)
        }
        svg::NODE_TYPE => {
            let upstream_svg = svg::upstream_svg(upstream);
            let source = svg::effective_svg(content, upstream_svg.as_deref())
                .and_then(|value| svg::normalize(value).ok());
            let mut cell = PluginCell::with_text(
                PluginCellKind::Svg,
                String::new(),
                source.is_none(),
                editing,
            );
            cell.svg_source = source;
            cell
        }
        html::NODE_TYPE => {
            let text = html::render(node, upstream);
            PluginCell::with_text(PluginCellKind::Html, text.clone(), text.is_empty(), editing)
        }
        panorama::NODE_TYPE => {
            let source = panorama::source_from(content, upstream);
            PluginCell::with_text(
                PluginCellKind::Panorama,
                String::new(),
                source.is_none(),
                editing,
            )
        }
        _ => PluginCell::plain(PluginCellKind::Missing),
    };
    cell.raw = raw;
    cell
}

fn is_editing(extras: &MetadataExtras) -> bool {
    extras
        .get("editing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Headless SVG rasterization (no DOM, no webview). tiny-skia output is
/// premultiplied RGBA, which is exactly what Slint's
/// `Image::from_rgba8_premultiplied` expects, so the buffer copies in directly.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "SVG sizes are small positive f32 values; the u32 conversions are bounded by SVG_MAX_EDGE"
)]
pub fn rasterize_svg(source: &str) -> Option<slint::Image> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(source, &options).ok()?;
    let size = tree.size();
    let scale = (SVG_MAX_EDGE as f32 / size.width().max(1.0))
        .min(SVG_MAX_EDGE as f32 / size.height().max(1.0))
        .min(4.0);
    let width = (size.width() * scale).round().max(1.0);
    let height = (size.height() * scale).round().max(1.0);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width as u32, height as u32)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width as u32, height as u32);
    buffer.make_mut_bytes().copy_from_slice(pixmap.data());
    Some(slint::Image::from_rgba8_premultiplied(buffer))
}

#[cfg(test)]
#[path = "plugin_tests/content_tests.rs"]
mod tests;
