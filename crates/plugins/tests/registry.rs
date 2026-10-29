use open_canvas_core::{
    CanvasGenerationMode, CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, ResourceKind,
};
use open_canvas_plugins::abi::{CanvasNodeDefinition, CanvasNodeResource, NodeSize, NodeView};
use open_canvas_plugins::registry::{detached_node, is_builtin_node_type, NodeRegistry};
use open_canvas_plugins::MetadataExtras;

fn view_for<'a>(node: &'a CanvasNodeData, extras: &'a MetadataExtras) -> NodeView<'a> {
    NodeView::new(node, extras)
}

fn node(node_type: CanvasNodeType, metadata: CanvasNodeMetadata) -> CanvasNodeData {
    let mut node = detached_node("n1", node_type);
    node.metadata = Some(metadata);
    node
}

fn custom(type_id: &str, title: &str) -> CanvasNodeDefinition {
    CanvasNodeDefinition::new(type_id, title, "puzzle", NodeSize::new(100.0, 50.0))
}

#[test]
fn builtin_registry_has_the_nine_core_kinds() {
    let registry = NodeRegistry::with_builtin_nodes();
    assert_eq!(registry.len(), 10);
    for node_type in CanvasNodeType::BUILTINS {
        assert!(registry.is_registered(node_type.as_str()));
        assert!(NodeRegistry::is_builtin(node_type.as_str()));
    }
}

#[test]
fn builtin_specs_reuse_the_core_defaults() {
    let registry = NodeRegistry::with_builtin_nodes();
    let spec = registry.get_spec("image");
    let core = open_canvas_core::get_node_spec(&CanvasNodeType::Image);
    assert_eq!(spec.width, core.width);
    assert_eq!(spec.height, core.height);
    assert_eq!(spec.title, core.title);
    assert_eq!(spec.metadata, core.metadata);
    assert_eq!(registry.get_node_plugin_id("image"), "builtin");
}

#[test]
fn builtin_flags_match_builtin_nodes_tsx() {
    let registry = NodeRegistry::with_builtin_nodes();
    let image_stack = registry.get("image-stack").expect("registered");
    assert!(!image_stack.has_source_handle);
    assert!(image_stack.transparent_background);
    assert_eq!(image_stack.minimap_color.as_deref(), Some("#10b981"));

    let assets = registry.get("assets").expect("registered");
    assert!(!assets.has_source_handle);
    assert!(assets.hide_panel);

    let image_generation = registry.get("image-generation").expect("registered");
    assert!(!image_generation.has_source_handle);
    assert_eq!(
        image_generation.use_builtin_panel.map(|panel| panel.mode),
        Some(CanvasGenerationMode::Image)
    );
    // Text and Prompt expose the built-in resource resolver.
    assert!(registry.get("text").expect("registered").resource.is_some());
}

#[test]
fn image_keeps_aspect_ratio_unless_free_resize() {
    let registry = NodeRegistry::with_builtin_nodes();
    let definition = registry.get("image").expect("registered");
    let extras = MetadataExtras::new();

    let locked = node(CanvasNodeType::Image, CanvasNodeMetadata::default());
    assert!(definition.keeps_aspect_ratio(&view_for(&locked, &extras)));

    let free = node(
        CanvasNodeType::Image,
        CanvasNodeMetadata {
            free_resize: Some(true),
            ..CanvasNodeMetadata::default()
        },
    );
    assert!(!definition.keeps_aspect_ratio(&view_for(&free, &extras)));

    let video = registry.get("video").expect("registered");
    let video_node = node(CanvasNodeType::Video, CanvasNodeMetadata::default());
    assert!(video.keeps_aspect_ratio(&view_for(&video_node, &extras)));
}

#[test]
fn builtin_resources_mirror_builtin_resource() {
    let registry = NodeRegistry::with_builtin_nodes();
    let image = registry.get("image").expect("registered");
    let text = registry.get("text").expect("registered");
    let prompt = registry.get("prompt").expect("registered");
    let extras = MetadataExtras::new();

    let image_node = node(
        CanvasNodeType::Image,
        CanvasNodeMetadata {
            content: Some("data:image/png;base64,AAA".into()),
            ..CanvasNodeMetadata::default()
        },
    );
    assert_eq!(
        image.resolve_resource(&view_for(&image_node, &extras)),
        Some(CanvasNodeResource::image("data:image/png;base64,AAA"))
    );

    let text_node = node(
        CanvasNodeType::Text,
        CanvasNodeMetadata {
            prompt: Some("from prompt".into()),
            ..CanvasNodeMetadata::default()
        },
    );
    let resource = text
        .resolve_resource(&view_for(&text_node, &extras))
        .expect("text resource");
    assert_eq!(resource.kind, ResourceKind::Text);
    assert_eq!(resource.text.as_deref(), Some("from prompt"));

    let empty_prompt = node(CanvasNodeType::Prompt, CanvasNodeMetadata::default());
    assert!(prompt
        .resolve_resource(&view_for(&empty_prompt, &extras))
        .is_none());
}

#[test]
fn register_and_unregister_track_the_owner_plugin() {
    let registry = NodeRegistry::new();
    let start = registry.version();
    registry.register("acme", [custom("acme:card", "Card")]);
    assert!(registry.version() > start);
    assert!(registry.is_registered("acme:card"));
    assert_eq!(registry.get_node_plugin_id("acme:card"), "acme");
    assert!(!NodeRegistry::is_builtin("acme:card"));
    assert!(is_builtin_node_type("image"));

    let removed = registry.unregister_plugin("acme");
    assert_eq!(removed, 1);
    assert!(!registry.is_registered("acme:card"));
    assert_eq!(registry.get_node_plugin_id("acme:card"), "builtin");
}

#[test]
fn re_registering_a_type_replaces_it_in_place() {
    let registry = NodeRegistry::with_builtin_nodes();
    registry.register("acme", [custom("image", "Overridden")]);
    assert_eq!(registry.list().len(), 10);
    assert_eq!(registry.get("image").expect("present").title, "Overridden");
    assert_eq!(registry.get_node_plugin_id("image"), "acme");
}

#[test]
fn unknown_types_fall_back_to_the_registry_default() {
    let registry = NodeRegistry::new();
    let spec = registry.get_spec("nope:thing");
    assert_eq!(spec.width, 340.0);
    assert_eq!(spec.height, 240.0);
    assert_eq!(spec.title, "Node");
    assert_eq!(spec.metadata, CanvasNodeMetadata::default());
    assert!(registry.get("nope:thing").is_none());
}

#[test]
fn default_extras_are_exposed_for_registered_types() {
    let registry = NodeRegistry::new();
    let mut definition = custom("acme:card", "Card");
    definition
        .default_extra_metadata
        .insert("flag".into(), serde_json::Value::Bool(true));
    registry.register("acme", [definition]);
    assert_eq!(
        registry.get_default_extras("acme:card").get("flag"),
        Some(&serde_json::Value::Bool(true))
    );
    assert!(registry.get_default_extras("missing").is_empty());
}
