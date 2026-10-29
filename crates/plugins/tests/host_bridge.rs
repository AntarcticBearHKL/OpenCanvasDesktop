use open_canvas_core::{CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, ResourceKind};
use open_canvas_plugins::registry::{detached_node, NodeRegistry};
use open_canvas_plugins::view::{plugin_node_view, BackgroundMode, PanelKind, PluginNodeView};
use open_canvas_plugins::{register_official_plugins, MetadataExtras};

fn node(type_id: &str, metadata: CanvasNodeMetadata) -> CanvasNodeData {
    let mut node = detached_node("n", CanvasNodeType::from_id(type_id));
    node.metadata = Some(metadata);
    node
}

fn empty(type_id: &str) -> CanvasNodeData {
    node(type_id, CanvasNodeMetadata::default())
}

fn content(type_id: &str, value: &str) -> CanvasNodeData {
    node(
        type_id,
        CanvasNodeMetadata {
            content: Some(value.to_string()),
            ..CanvasNodeMetadata::default()
        },
    )
}

fn prompt_for(type_id: &str) -> CanvasNodeData {
    node(
        type_id,
        CanvasNodeMetadata {
            prompt: Some("a clip".to_string()),
            ..CanvasNodeMetadata::default()
        },
    )
}

fn resolved(
    registry: &NodeRegistry,
    node: &CanvasNodeData,
    extras: &MetadataExtras,
) -> PluginNodeView {
    registry.node_view(node, extras)
}

#[test]
fn builtin_text_prompt_and_video_prompt_emit_text_resources() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();

    let text = resolved(&registry, &content("text", "hello"), &extras);
    assert_eq!(text.title, "Text");
    assert_eq!(text.icon, "file-text");
    assert_eq!(text.minimap_color, None);
    assert_eq!(text.panel, PanelKind::None);
    assert_eq!(text.resource_kind, Some(ResourceKind::Text));
    assert!(text.has_source_handle && text.has_target_handle);
    assert!(text.registered);

    let prompt = resolved(&registry, &prompt_for("prompt"), &extras);
    assert_eq!(prompt.resource_kind, Some(ResourceKind::Text));
    assert_eq!(prompt.minimap_color.as_deref(), Some("#eab308"));

    let video_prompt = resolved(&registry, &prompt_for("video-prompt"), &extras);
    assert_eq!(video_prompt.resource_kind, Some(ResourceKind::Text));
    assert_eq!(video_prompt.minimap_color.as_deref(), Some("#fb923c"));
}

#[test]
fn builtin_image_and_video_keep_aspect_ratio_and_emit_media() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();

    let image = resolved(
        &registry,
        &content("image", "data:image/png;base64,AAA"),
        &extras,
    );
    assert_eq!(image.resource_kind, Some(ResourceKind::Image));
    assert!(image.keep_aspect_ratio);
    assert!(
        image.has_source_handle,
        "an image can feed a prompt reference or another consumer"
    );
    assert!(
        !image.has_target_handle,
        "an image is produced, never wired into"
    );
    assert_eq!(image.background, BackgroundMode::Normal);
    assert_eq!(image.minimap_color.as_deref(), Some("#10b981"));

    let free_image = node(
        "image",
        CanvasNodeMetadata {
            content: Some("data:image/png;base64,AAA".to_string()),
            free_resize: Some(true),
            ..CanvasNodeMetadata::default()
        },
    );
    assert!(!resolved(&registry, &free_image, &extras).keep_aspect_ratio);

    let video = resolved(
        &registry,
        &content("video", "data:video/mp4;base64,BBB"),
        &extras,
    );
    assert_eq!(video.resource_kind, Some(ResourceKind::Video));
    assert!(video.keep_aspect_ratio);
    assert_eq!(video.minimap_color.as_deref(), Some("#f97316"));
}

#[test]
fn builtin_image_stack_is_transparent_and_has_no_handles() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();
    let view = resolved(&registry, &empty("image-stack"), &extras);
    assert_eq!(view.background, BackgroundMode::Transparent);
    assert!(!view.has_source_handle);
    assert!(!view.has_target_handle);
    assert_eq!(view.panel, PanelKind::None);
}

#[test]
fn builtin_generators_use_the_builtin_panel() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();

    let image_generation = resolved(&registry, &empty("image-generation"), &extras);
    assert_eq!(image_generation.panel, PanelKind::BuiltinImage);
    assert!(!image_generation.has_source_handle);
    assert!(image_generation.keep_aspect_ratio);
    assert_eq!(image_generation.minimap_color.as_deref(), Some("#f472b6"));

    let video_generation = resolved(&registry, &empty("video-generation"), &extras);
    assert_eq!(video_generation.panel, PanelKind::BuiltinVideo);
    assert!(!video_generation.has_source_handle);
    assert_eq!(video_generation.minimap_color.as_deref(), Some("#fb7185"));
    assert_eq!(video_generation.title, "Video Generation");
}

#[test]
fn builtin_assets_hides_its_panel_and_source_handle() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();
    let view = resolved(&registry, &empty("assets"), &extras);
    assert!(!view.has_source_handle);
    assert_eq!(view.panel, PanelKind::None);
    assert_eq!(view.minimap_color.as_deref(), Some("#64748b"));
}

#[test]
fn every_builtin_resolves_to_a_registered_view() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();
    for node_type in CanvasNodeType::BUILTINS {
        let view = resolved(&registry, &empty(node_type.as_str()), &extras);
        assert!(
            view.registered,
            "{} should be registered",
            node_type.as_str()
        );
        assert_eq!(view.type_id, node_type.as_str());
        assert!(!view.title.is_empty());
        assert!(!view.icon.is_empty());
    }
}

#[test]
fn official_plugins_resolve_to_expected_views() {
    let registry = NodeRegistry::with_builtin_nodes();
    assert_eq!(register_official_plugins(&registry), 5);
    assert_eq!(registry.len(), 15);
    let extras = MetadataExtras::new();

    let html = resolved(&registry, &content("html:render", "<b>x</b>"), &extras);
    assert_eq!(html.title, "HTML");
    assert_eq!(html.icon, "🌐");
    assert_eq!(html.minimap_color.as_deref(), Some("#ec4899"));
    assert_eq!(html.panel, PanelKind::None);
    assert!(html.interaction_toggle);
    assert_eq!(html.resource_kind, None);
    assert_eq!(html.default_size.width, 420.0);
    assert!(!html.content_interactive);

    let markdown = resolved(&registry, &content("markdown:doc", "# Title"), &extras);
    assert_eq!(markdown.minimap_color.as_deref(), Some("#6366f1"));
    assert_eq!(markdown.panel, PanelKind::None);
    assert!(markdown.interaction_toggle);
    assert_eq!(markdown.resource_kind, Some(ResourceKind::Text));

    let svg = resolved(&registry, &content("svg:vector", "<svg/>"), &extras);
    assert_eq!(svg.background, BackgroundMode::Transparent);
    assert_eq!(svg.panel, PanelKind::None);
    assert_eq!(svg.resource_kind, None);
    assert_eq!(svg.default_size.width, 320.0);

    let note = resolved(&registry, &content("sticky-note:note", "hi"), &extras);
    assert_eq!(note.minimap_color.as_deref(), Some("#f59e0b"));
    assert_eq!(note.panel, PanelKind::None);
    assert_eq!(note.resource_kind, None);
    assert!(!note.interaction_toggle);

    let panorama = resolved(
        &registry,
        &content("panorama:viewer", "data:image/png;base64,PPP"),
        &extras,
    );
    assert_eq!(panorama.minimap_color.as_deref(), Some("#0ea5e9"));
    assert_eq!(panorama.panel, PanelKind::BuiltinImage);
    assert_eq!(panorama.resource_kind, Some(ResourceKind::Image));
    assert!(panorama.interaction_toggle);
}

#[test]
fn interaction_toggle_nodes_force_interactive_while_editing() {
    let registry = NodeRegistry::with_builtin_nodes();
    register_official_plugins(&registry);

    let empty_html = empty("html:render");
    let mut extras = MetadataExtras::new();
    let interactive = resolved(&registry, &empty_html, &extras);
    assert!(interactive.interaction_toggle);
    assert!(interactive.content_interactive);

    let filled = content("html:render", "<b>x</b>");
    assert!(!resolved(&registry, &filled, &extras).content_interactive);

    extras.insert("editing".into(), serde_json::Value::Bool(true));
    assert!(resolved(&registry, &filled, &extras).content_interactive);
}

#[test]
fn unregistered_types_fall_back_to_the_missing_plugin_placeholder() {
    let registry = NodeRegistry::with_builtin_nodes();
    let extras = MetadataExtras::new();
    let view = plugin_node_view(&empty("ghost:node"), &extras, None);
    assert!(!view.registered);
    assert_eq!(view.title, "Node");
    assert_eq!(view.icon, "puzzle");
    assert_eq!(view.panel, PanelKind::None);
    assert!(view.has_source_handle);
    assert!(view.has_target_handle);
    assert_eq!(view.resource_kind, None);

    let via_registry = resolved(&registry, &empty("ghost:node"), &extras);
    assert_eq!(via_registry, view);
}
