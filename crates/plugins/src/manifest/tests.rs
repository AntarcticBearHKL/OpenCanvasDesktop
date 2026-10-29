use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::*;
use crate::abi::{MetadataExtras, NodeSize};

fn node(type_id: &str, render_kind: &str) -> PluginNodeManifest {
    PluginNodeManifest {
        type_id: type_id.to_string(),
        title: "Title".to_string(),
        render_kind: render_kind.to_string(),
        icon: "📝".to_string(),
        description: None,
        default_size: None,
        default_metadata: None,
        default_extra_metadata: MetadataExtras::new(),
        minimap_color: None,
        show_in_create_menu: true,
        has_source_handle: true,
        hide_panel: false,
        transparent_background: false,
        auto_open_panel: false,
        interaction_toggle: false,
        panel: None,
        resource_kind: None,
    }
}

fn sample(id: &str, nodes: Vec<PluginNodeManifest>) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: "Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        min_app_version: None,
        nodes,
        settings: BTreeMap::new(),
        i18n: BTreeMap::new(),
    }
}

fn raw(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap_or_default()
}

#[test]
fn parses_a_minimal_manifest_and_fills_defaults() {
    let bytes = raw(json!({
        "id": "acme",
        "nodes": [{ "type": "acme:note", "title": "Note", "renderKind": "markdown", "icon": "📝" }]
    }));
    let parsed = parse_manifest(&bytes).expect("valid manifest");
    assert_eq!(parsed.name, "acme", "empty name defaults to the id");
    assert_eq!(parsed.version, "0.0.0");
    assert_eq!(parsed.nodes.len(), 1);
    assert!(parsed.nodes[0].show_in_create_menu);
    assert!(parsed.nodes[0].has_source_handle);
}

#[test]
fn missing_id_is_a_typed_error() {
    let bytes = raw(
        json!({ "nodes": [{ "type": "a:b", "title": "T", "renderKind": "text", "icon": "📝" }] }),
    );
    assert_eq!(
        parse_manifest(&bytes),
        Err(ManifestError::MissingField { field: "id" })
    );
}

#[test]
fn bad_semver_is_rejected() {
    let bytes = raw(json!({
        "id": "acme", "version": "1.0",
        "nodes": [{ "type": "acme:n", "title": "T", "renderKind": "text", "icon": "📝" }]
    }));
    assert_eq!(
        parse_manifest(&bytes),
        Err(ManifestError::InvalidVersion {
            version: "1.0".to_string()
        })
    );
}

#[test]
fn duplicate_type_ids_are_rejected() {
    let m = sample(
        "acme",
        vec![node("acme:n", "text"), node("acme:n", "markdown")],
    );
    assert_eq!(
        m.validate(),
        Err(ManifestError::DuplicateTypeId {
            type_id: "acme:n".to_string()
        })
    );
}

#[test]
fn unknown_render_kind_is_rejected() {
    let m = sample("acme", vec![node("acme:n", "acme:future")]);
    assert_eq!(
        m.validate(),
        Err(ManifestError::UnknownRenderKind {
            type_id: "acme:n".to_string(),
            render_kind: "acme:future".to_string()
        })
    );
}

#[test]
fn unknown_icon_key_is_rejected() {
    let mut n = node("acme:n", "text");
    n.icon = "definitely-not-an-icon".to_string();
    let m = sample("acme", vec![n]);
    assert_eq!(
        m.validate(),
        Err(ManifestError::UnknownIconKey {
            type_id: "acme:n".to_string(),
            icon: "definitely-not-an-icon".to_string()
        })
    );
    let mut ok = node("acme:n", "text");
    ok.icon = "file-text".to_string();
    assert!(sample("acme", vec![ok]).validate().is_ok());
}

#[test]
fn size_bounds_are_enforced() {
    let mut n = node("acme:n", "text");
    n.default_size = Some(PluginSize {
        width: 1.0,
        height: 9999.0,
    });
    let m = sample("acme", vec![n]);
    assert!(matches!(
        m.validate(),
        Err(ManifestError::SizeOutOfBounds { .. })
    ));
}

#[test]
fn invalid_minimap_colour_is_rejected() {
    let mut n = node("acme:n", "text");
    n.minimap_color = Some("red".to_string());
    let m = sample("acme", vec![n]);
    assert_eq!(
        m.validate(),
        Err(ManifestError::InvalidMinimapColor {
            type_id: "acme:n".to_string(),
            color: "red".to_string()
        })
    );
}

#[test]
fn invalid_node_type_id_is_rejected() {
    let m = sample("acme", vec![node("no-colon-here", "text")]);
    assert!(matches!(
        m.validate(),
        Err(ManifestError::InvalidNodeTypeId { .. })
    ));
}

#[test]
fn empty_plugin_declares_no_nodes() {
    let m = sample("acme", Vec::new());
    assert_eq!(m.validate(), Err(ManifestError::NoNodes));
}

#[test]
fn malformed_json_is_a_syntax_error() {
    assert!(matches!(
        parse_manifest(b"{ not json"),
        Err(ManifestError::Syntax { .. })
    ));
}

#[test]
fn select_setting_requires_options() {
    let mut m = sample("acme", vec![node("acme:n", "text")]);
    m.settings.insert(
        "size".to_string(),
        PluginSettingSpec {
            kind: PluginSettingKind::Select,
            label: "Size".to_string(),
            default: None,
            options: Vec::new(),
        },
    );
    assert!(matches!(
        m.validate(),
        Err(ManifestError::InvalidSetting { .. })
    ));
}

#[test]
fn emit_parse_round_trips_and_is_stable() {
    let mut m = sample(
        "acme",
        vec![{
            let mut n = node("acme:board", "sticky-note");
            n.minimap_color = Some("#f59e0b".to_string());
            n.default_size = Some(PluginSize {
                width: 240.0,
                height: 200.0,
            });
            n
        }],
    );
    m.description = Some("A board".to_string());
    m.i18n.insert(
        DEFAULT_LOCALE.to_string(),
        BTreeMap::from([("name".to_string(), "Board".to_string())]),
    );
    let first = emit_manifest(&m);
    let reparsed = parse_manifest(first.as_bytes()).expect("re-parse");
    assert_eq!(reparsed, m);
    assert_eq!(emit_manifest(&reparsed), first, "emit is byte-stable");
}

#[test]
fn render_kinds_map_onto_definitions() {
    let mut n = node("acme:note", "sticky-note");
    n.icon = "sticky-note".to_string();
    let definition = n.to_definition().expect("supported kind");
    assert_eq!(definition.type_id, "acme:note");
    assert_eq!(definition.default_size, NodeSize::new(240.0, 200.0));
    assert_eq!(
        definition
            .default_extra_metadata
            .get(RENDER_KIND_KEY)
            .and_then(Value::as_str),
        Some("sticky-note")
    );
    assert_eq!(definition.default_metadata.content, Some(String::new()));
}

#[test]
fn unsupported_render_kind_errors_on_conversion() {
    let n = node("acme:future", "acme:future");
    assert_eq!(
        n.to_definition().unwrap_err(),
        ManifestError::UnsupportedRenderKind {
            type_id: "acme:future".to_string(),
            render_kind: "acme:future".to_string()
        }
    );
}

#[test]
fn manifest_maps_every_node_onto_a_definition() {
    let m = sample("acme", vec![node("acme:a", "text"), node("acme:b", "svg")]);
    let plugin = m.to_plugin().expect("all kinds supported");
    assert_eq!(plugin.id, "acme");
    assert_eq!(plugin.nodes.len(), 2);
}

#[test]
fn i18n_falls_back_to_default_locale() {
    let mut m = sample("acme", vec![node("acme:n", "text")]);
    m.name = "Base Name".to_string();
    m.i18n.insert(
        DEFAULT_LOCALE.to_string(),
        BTreeMap::from([("name".to_string(), "Fallback Name".to_string())]),
    );
    assert_eq!(m.localized("zh-CN", "name"), Some("Fallback Name"));
    assert_eq!(m.name_for("zh-CN"), "Fallback Name");
    m.i18n.insert(
        "zh-CN".to_string(),
        BTreeMap::from([("name".to_string(), "本地名称".to_string())]),
    );
    assert_eq!(m.name_for("zh-CN"), "本地名称");
    assert_eq!(m.name_for("fr-FR"), "Fallback Name");
    assert_eq!(m.localized("fr-FR", "missing"), None);
}
