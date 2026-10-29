//! Content-mapping tests: one cell per plugin kind plus the SVG raster buffer.

use super::*;
use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeType, Position};

fn node(type_id: &str, content: Option<&str>) -> CanvasNodeData {
    CanvasNodeData {
        id: "n".into(),
        node_type: CanvasNodeType::from_id(type_id),
        title: "T".into(),
        position: Position::new(0.0, 0.0),
        width: 320.0,
        height: 320.0,
        metadata: Some(CanvasNodeMetadata {
            content: content.map(str::to_string),
            ..Default::default()
        }),
    }
}

#[test]
fn unregistered_plugin_nodes_resolve_to_missing() {
    let cell = plugin_cell(&node("ghost:x", None), &MetadataExtras::new(), false, &[]);
    assert_eq!(cell.kind, PluginCellKind::Missing);
}

#[test]
fn sticky_note_reads_content_and_color_extra() {
    let mut extras = MetadataExtras::new();
    extras.insert(
        sticky_note::COLOR_KEY.into(),
        serde_json::Value::String("#fca5a5".into()),
    );
    let cell = plugin_cell(
        &node(sticky_note::NODE_TYPE, Some("hello")),
        &extras,
        true,
        &[],
    );
    assert_eq!(cell.kind, PluginCellKind::Sticky);
    assert_eq!(cell.text, "hello");
    assert_eq!(cell.color.as_deref(), Some("#fca5a5"));
    assert!(!cell.empty);
}

#[test]
fn empty_sticky_note_uses_the_plugin_placeholder_and_default_color() {
    let cell = plugin_cell(
        &node(sticky_note::NODE_TYPE, Some("")),
        &MetadataExtras::new(),
        true,
        &[],
    );
    assert!(cell.empty);
    assert_eq!(cell.text, sticky_note::DEFAULT_CONTENT);
    assert_eq!(cell.color.as_deref(), Some(sticky_note::DEFAULT_COLOR));
}

#[test]
fn markdown_converts_to_text_and_empties_to_placeholder() {
    let cell = plugin_cell(
        &node(markdown::NODE_TYPE, Some("# Title\n\n- item")),
        &MetadataExtras::new(),
        true,
        &[],
    );
    assert_eq!(cell.text, "Title\n\nitem");
    let empty = plugin_cell(
        &node(markdown::NODE_TYPE, Some("")),
        &MetadataExtras::new(),
        true,
        &[],
    );
    assert!(empty.empty);
    assert!(empty.text.contains("Markdown"));
}

#[test]
fn html_substitutes_upstream_text() {
    let upstream = node("text", Some("world"));
    let cell = plugin_cell(
        &node(html::NODE_TYPE, Some("<p>{{ input }}</p>")),
        &MetadataExtras::new(),
        true,
        &[upstream],
    );
    assert_eq!(cell.text, "<p>world</p>");
}

#[test]
fn svg_adopts_upstream_source_and_keeps_own_content_priority() {
    let upstream = node("text", Some("<svg width=\"2\" height=\"2\"></svg>"));
    let own = plugin_cell(
        &node(svg::NODE_TYPE, Some("<svg width=\"4\" height=\"4\"></svg>")),
        &MetadataExtras::new(),
        true,
        std::slice::from_ref(&upstream),
    );
    assert!(own
        .svg_source
        .as_deref()
        .is_some_and(|s| s.contains("width=\"4\"")));
    let adopted = plugin_cell(
        &node(svg::NODE_TYPE, None),
        &MetadataExtras::new(),
        true,
        &[upstream],
    );
    assert!(adopted
        .svg_source
        .as_deref()
        .is_some_and(|s| s.contains("width=\"2\"")));
}

#[test]
fn invalid_svg_falls_back_to_the_placeholder() {
    let cell = plugin_cell(
        &node(svg::NODE_TYPE, Some("<div>nope</div>")),
        &MetadataExtras::new(),
        true,
        &[],
    );
    assert_eq!(cell.kind, PluginCellKind::Svg);
    assert!(cell.empty);
    assert!(cell.svg_source.is_none());
    assert!(cell.svg_image().is_none());
}

#[test]
fn svg_rasterizes_to_a_slint_image_with_the_expected_buffer() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">
        <rect width="8" height="8" fill="#ff0000"/></svg>"##;
    let image = rasterize_svg(source).unwrap_or_default();
    // 8px SVG * the 4x upscale cap = 32px raster.
    assert_eq!(image.size().width, 32);
    assert_eq!(image.size().height, 32);
    let Some(buffer) = image.to_rgba8_premultiplied() else {
        return;
    };
    let pixel = buffer.as_slice()[0];
    assert_eq!(pixel.r, 255);
    assert_eq!(pixel.g, 0);
    assert_eq!(pixel.a, 255);
}

#[test]
fn panorama_reports_source_presence_from_own_or_upstream_content() {
    let empty = plugin_cell(
        &node(panorama::NODE_TYPE, None),
        &MetadataExtras::new(),
        true,
        &[],
    );
    assert!(empty.empty);
    let upstream = node("image", Some("data:image/png;base64,AA"));
    let filled = plugin_cell(
        &node(panorama::NODE_TYPE, None),
        &MetadataExtras::new(),
        true,
        &[upstream],
    );
    assert!(!filled.empty);
}
