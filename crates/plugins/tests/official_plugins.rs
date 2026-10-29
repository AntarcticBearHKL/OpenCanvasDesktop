use open_canvas_core::{CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, ResourceKind};
use open_canvas_plugins::abi::{CanvasNodeResource, NodeView};
use open_canvas_plugins::official::{html, markdown, panorama, sticky_note, svg};
use open_canvas_plugins::registry::{detached_node, NodeRegistry};
use open_canvas_plugins::{official_plugins, MetadataExtras};

fn node(node_type: CanvasNodeType, content: Option<&str>) -> CanvasNodeData {
    let mut node = detached_node("n", node_type);
    node.metadata = Some(CanvasNodeMetadata {
        content: content.map(str::to_string),
        ..CanvasNodeMetadata::default()
    });
    node
}

#[test]
fn the_five_official_plugins_are_exposed() {
    let plugins = official_plugins();
    let ids: Vec<&str> = plugins.iter().map(|plugin| plugin.id.as_str()).collect();
    assert_eq!(ids, ["html", "markdown", "panorama", "sticky-note", "svg"]);
    for plugin in &plugins {
        assert_eq!(plugin.nodes.len(), 1);
        assert!(!plugin.version.is_empty());
    }
}

#[test]
fn official_plugins_register_and_unregister_under_their_ids() {
    let registry = NodeRegistry::with_builtin_nodes();
    for plugin in official_plugins() {
        registry.register_plugin(&plugin);
    }
    assert_eq!(registry.len(), 15);
    assert_eq!(registry.get_node_plugin_id("markdown:doc"), "markdown");
    assert_eq!(registry.get_node_plugin_id("svg:vector"), "svg");
    assert_eq!(registry.unregister_plugin("markdown"), 1);
    assert!(!registry.is_registered("markdown:doc"));
    assert!(registry.is_registered("svg:vector"));
}

#[test]
fn html_substitutes_every_placeholder_variant() {
    assert_eq!(
        html::substitute_input("<p>{{input}}</p>", "hi"),
        "<p>hi</p>"
    );
    assert_eq!(
        html::substitute_input("<p>{{ input }}</p>", "hi"),
        "<p>hi</p>"
    );
    assert_eq!(html::substitute_input("{{  input  }}{{input}}", "x"), "xx");
    // Non-placeholders are left untouched.
    assert_eq!(
        html::substitute_input("{{inputs}} {{ input", "x"),
        "{{inputs}} {{ input"
    );
    // Non-ASCII text (accents) is preserved byte for byte.
    assert_eq!(
        html::substitute_input("café {{input}}", "naïve"),
        "café naïve"
    );
    assert_eq!(
        html::substitute_input("pre {{input}} post", "x"),
        "pre x post"
    );
}

#[test]
fn html_upstream_text_joins_non_empty_content() {
    let nodes = vec![
        node(CanvasNodeType::Text, Some("a")),
        node(CanvasNodeType::Text, Some("")),
        node(CanvasNodeType::Text, Some("b")),
    ];
    assert_eq!(html::upstream_text(&nodes), "a\nb");
    assert_eq!(
        html::render(
            &node(CanvasNodeType::Text, Some("<i>{{input}}</i>")),
            &nodes
        ),
        "<i>a\nb</i>"
    );
}

#[test]
fn html_definition_matches_the_plugin_metadata() {
    let definition = html::definition();
    assert_eq!(definition.type_id, "html:render");
    assert!(definition.hide_panel);
    assert!(definition.interaction_toggle);
    // forceInteractive only while editing (extra key).
    let mut extras = MetadataExtras::new();
    let node = node(CanvasNodeType::Plugin("html:render".into()), Some("x"));
    assert!(!definition.is_force_interactive(&NodeView::new(&node, &extras)));
    extras.insert("editing".into(), serde_json::Value::Bool(true));
    assert!(definition.is_force_interactive(&NodeView::new(&node, &extras)));
}

#[test]
fn svg_validates_normalizes_and_adopts_upstream() {
    let source = "  <?xml version=\"1.0\"?><!-- c --><svg><rect/></svg>  ";
    assert_eq!(svg::normalize(source).expect("valid"), source.trim());
    assert!(matches!(svg::validate("   "), Err(svg::SvgError::Empty)));
    assert!(matches!(
        svg::validate("<div></div>"),
        Err(svg::SvgError::MissingRoot)
    ));
    assert!(matches!(
        svg::validate("<svg>"),
        Err(svg::SvgError::UnclosedTag(_))
    ));
    assert!(matches!(
        svg::validate("<svg><g></svg>"),
        Err(svg::SvgError::MismatchedTag { .. })
    ));
    assert!(svg::validate("<svg/>").is_ok());

    let nodes = vec![
        node(CanvasNodeType::Text, Some("not svg")),
        node(CanvasNodeType::Text, Some("<svg><circle/></svg>")),
    ];
    let upstream = svg::upstream_svg(&nodes).expect("upstream svg");
    assert_eq!(upstream, "<svg><circle/></svg>");
    assert!(svg::should_adopt_upstream(None, Some(&upstream)));
    assert!(!svg::should_adopt_upstream(Some(""), Some(&upstream)));
    assert_eq!(
        svg::effective_svg(Some("  "), Some("<svg/>")),
        Some("<svg/>")
    );
    assert_eq!(svg::effective_svg(Some("<svg/>"), None), Some("<svg/>"));
    assert!(svg::definition().transparent_background);
}

#[test]
fn markdown_renders_blocks_and_inline() {
    assert_eq!(markdown::to_html("# Title"), "<h1>Title</h1>\n");
    assert_eq!(
        markdown::to_html("- a\n- b"),
        "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n"
    );
    assert_eq!(
        markdown::to_html("1. one\n2. two"),
        "<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n"
    );
    assert!(markdown::to_html("**bold** and *em*").contains("<strong>bold</strong>"));
    assert!(markdown::to_html("**bold** and *em*").contains("<em>em</em>"));
    assert!(markdown::to_html("[x](https://a.example)").contains("href=\"https://a.example\""));
    assert_eq!(
        markdown::to_html("```\n<b>\n```"),
        "<pre><code>&lt;b&gt;</code></pre>\n"
    );
    assert_eq!(
        markdown::to_html("> quoted"),
        "<blockquote>\n<p>quoted</p>\n</blockquote>\n"
    );
    assert_eq!(markdown::to_html("---"), "<hr />\n");
}

#[test]
fn markdown_escapes_html_and_rejects_unsafe_links() {
    assert!(markdown::to_html("<script>alert(1)</script>").contains("&lt;script&gt;"));
    let html = markdown::to_html("[x](javascript:alert(1))");
    assert!(!html.contains("href="));
    assert!(html.contains("x"));
}

#[test]
fn markdown_to_text_strips_syntax() {
    assert_eq!(markdown::to_text("# Title\n\n- a **b**"), "Title\n\na b");
    assert_eq!(markdown::to_text("**bold** and `code`"), "bold and code");
    assert_eq!(markdown::to_text("[label](https://a.example)"), "label");
    assert_eq!(markdown::to_text(""), "");
    assert_eq!(markdown::to_html("   \n\n"), "");
}

#[test]
fn markdown_preview_uses_the_placeholder_for_empty_documents() {
    assert_eq!(
        markdown::to_preview_html("# Title"),
        markdown::to_html("# Title")
    );
    assert!(markdown::to_preview_html("").contains("<em>Select the node"));
}

#[test]
fn sticky_note_model_resolves_color_and_content() {
    assert_eq!(
        sticky_note::normalize_color(None),
        sticky_note::DEFAULT_COLOR
    );
    assert_eq!(
        sticky_note::normalize_color(Some("   ")),
        sticky_note::DEFAULT_COLOR
    );
    assert_eq!(sticky_note::normalize_color(Some(" #abcdef ")), "#abcdef");
    assert!(sticky_note::is_preset_color(sticky_note::DEFAULT_COLOR));
    assert!(!sticky_note::is_preset_color("#123456"));

    let note = sticky_note::StickyNote::from_parts(Some("hello"), Some("#fca5a5"));
    assert_eq!(note.display_text(), "hello");
    assert_eq!(note.color, "#fca5a5");
    assert!(note.is_preset_color());

    let empty = sticky_note::StickyNote::from_parts(None, None);
    assert_eq!(empty.display_text(), sticky_note::DEFAULT_CONTENT);
    assert_eq!(empty.color, sticky_note::DEFAULT_COLOR);

    let patch = note.metadata_patch();
    assert_eq!(patch.metadata.content.as_deref(), Some("hello"));
    assert_eq!(
        patch.extra.get(sticky_note::COLOR_KEY),
        Some(&serde_json::Value::String("#fca5a5".into()))
    );
}

#[test]
fn panorama_metadata_matches_the_plugin() {
    assert_eq!(
        panorama::generation_prompt("a room"),
        format!("{}a room", panorama::PANORAMA_SYSTEM_PROMPT)
    );
    assert!(panorama::is_equirectangular(2048.0, 1024.0));
    assert!(!panorama::is_equirectangular(1000.0, 1000.0));
    assert!(!panorama::is_equirectangular(0.0, 0.0));

    let nodes = vec![node(
        CanvasNodeType::Image,
        Some("data:image/png;base64,ZZZ"),
    )];
    assert_eq!(
        panorama::source_from(None, &nodes),
        Some("data:image/png;base64,ZZZ".to_string())
    );
    assert_eq!(
        panorama::source_from(Some("own"), &nodes),
        Some("own".to_string())
    );

    let definition = panorama::definition();
    let panel = definition
        .use_builtin_panel
        .as_ref()
        .expect("builtin panel");
    assert_eq!(
        panel.prompt_prefix.as_deref(),
        Some(panorama::PANORAMA_SYSTEM_PROMPT)
    );
    assert!(panel.write_back_to_self);
    assert!(definition.interaction_toggle);

    let extras = MetadataExtras::new();
    let viewer = node(
        CanvasNodeType::Plugin("panorama:viewer".into()),
        Some("data:img"),
    );
    let resource = definition
        .resolve_resource(&NodeView::new(&viewer, &extras))
        .expect("panorama resource");
    assert_eq!(resource, CanvasNodeResource::image("data:img"));
    assert_eq!(resource.kind, ResourceKind::Image);
}
