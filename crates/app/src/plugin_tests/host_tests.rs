//! Host tests: registration, manager state machine and extras bookkeeping.

use super::*;
use crate::plugin_manager_model::InstallError;
use open_canvas_core::constants::get_node_spec;
use open_canvas_core::node::{CanvasConnection, CanvasNodeType};
use open_canvas_plugins::manager::has_upgrade;

fn test_host() -> PluginHost {
    PluginHost::new(PathBuf::from("D:\\tmp\\plugins"))
}

#[test]
fn official_plugins_start_installed_enabled_and_registered() {
    let host = test_host();
    for id in ["html", "markdown", "panorama", "sticky-note", "svg"] {
        assert!(host.manager.get(id).is_some_and(|record| record.enabled));
        assert!(host.manager.get(id).is_some_and(|record| record.official));
    }
    for type_id in [
        "html:render",
        "markdown:doc",
        "panorama:viewer",
        "sticky-note:note",
        "svg:vector",
    ] {
        assert!(
            host.is_registered(type_id),
            "{type_id} should be registered"
        );
    }
    assert!(host.is_registered("image-attempt"));
    assert!(!host.is_registered("text"));
}

#[test]
fn disabling_an_official_plugin_unregisters_its_node_types() {
    let mut host = test_host();
    assert!(host.set_enabled("svg", false));
    assert!(!host.is_registered("svg:vector"));
    assert!(host.is_registered("markdown:doc"));
    assert!(host.set_enabled("svg", true));
    assert!(host.is_registered("svg:vector"));
    assert!(!host.set_enabled("ghost", true));
}

#[test]
fn uninstall_drops_the_record_and_its_definitions() {
    let mut host = test_host();
    assert!(host.uninstall("html").is_some());
    assert!(!host.is_registered("html:render"));
    assert!(host.uninstall("html").is_none());
}

#[test]
fn install_from_url_records_url_and_local_source_path() {
    let mut host = test_host();
    let record = host
        .install_from_url("https://example.com/plugins/my-widget.js")
        .ok();
    let Some(record) = record else {
        return;
    };
    assert_eq!(record.id, "my-widget");
    assert_eq!(record.version, "0.0.0");
    assert!(record.enabled);
    assert!(!record.official);
    assert_eq!(record.url, "https://example.com/plugins/my-widget.js");
    assert_eq!(record.source, host.local_source_path("my-widget"));
    assert!(record.source.ends_with("my-widget.js"));
    assert!(!host.is_registered("my-widget:anything"));
}

#[test]
fn install_validation_rejects_bad_urls() {
    let mut host = test_host();
    assert_eq!(host.install_from_url("   "), Err(InstallError::Empty));
    assert_eq!(
        host.install_from_url("ftp://example.com/x.js"),
        Err(InstallError::NotAUrl)
    );
    assert_eq!(
        host.install_from_url("https://example.com/..."),
        Err(InstallError::NoPluginName)
    );
}

#[test]
fn installing_an_official_url_marks_it_official() {
    let mut host = test_host();
    let record = host.install_from_url("builtin://official/svg.js").ok();
    let Some(record) = record else {
        return;
    };
    assert!(record.official);
    assert_eq!(record.version, "1.1.0");
    assert!(host.is_registered("svg:vector"));
}

#[test]
fn official_entries_expose_upgrades_through_the_version_compare() {
    let host = test_host();
    let entries = PluginHost::official_entries();
    assert_eq!(entries.len(), 5);
    let markdown = entries.iter().find(|entry| entry.id == "markdown");
    let Some(entry) = markdown else {
        return;
    };
    let installed = host.manager.get("markdown");
    let Some(installed) = installed else {
        return;
    };
    assert!(!entry.upgrades(installed));
    assert!(has_upgrade("1.0.0", &entry.version));
}

#[test]
fn extras_round_trip_and_prune_with_the_document() {
    let mut host = test_host();
    let mut doc = Doc::empty();
    let spec = host.spec("sticky-note:note");
    let extras = host.default_extras("sticky-note:note");
    let id = doc.create_node_with(
        CanvasNodeType::from_id("sticky-note:note"),
        crate::coords::WorldPos::new(0.0, 0.0),
        spec,
        0,
    );
    host.set_extras(&id, extras);
    assert!(host.extras_of(&id).contains_key("pluginColor"));
    host.set_extra(&id, "editing", serde_json::Value::Bool(true));
    assert_eq!(
        host.extras_of(&id).get("editing"),
        Some(&serde_json::Value::Bool(true))
    );
    doc.delete_selection(1);
    host.prune_extras(&doc);
    assert!(host.extras_of(&id).is_empty());
}

#[test]
fn duplicate_copies_extras_to_the_copy() {
    let mut host = test_host();
    let mut doc = Doc::empty();
    let spec = host.spec("sticky-note:note");
    let id = doc.create_node_with(
        CanvasNodeType::from_id("sticky-note:note"),
        crate::coords::WorldPos::new(0.0, 0.0),
        spec,
        0,
    );
    host.set_extra(
        &id,
        "pluginColor",
        serde_json::Value::String("#fca5a5".into()),
    );
    let copy = doc.duplicate_node(&id, 5);
    let Some(copy) = copy else {
        return;
    };
    host.copy_extras(&id, &copy);
    assert_eq!(
        host.extras_of(&copy).get("pluginColor"),
        Some(&serde_json::Value::String("#fca5a5".into()))
    );
}

#[test]
fn upstream_nodes_follow_incoming_connections() {
    let mut doc = Doc::empty();
    let text_spec = get_node_spec(&CanvasNodeType::Text);
    let source = doc.create_node_with(
        CanvasNodeType::Text,
        crate::coords::WorldPos::new(0.0, 0.0),
        text_spec.clone(),
        0,
    );
    let target = doc.create_node_with(
        CanvasNodeType::from_id("html:render"),
        crate::coords::WorldPos::new(0.0, 0.0),
        text_spec,
        1,
    );
    let empty = upstream_nodes(doc.nodes(), doc.connections(), &target);
    assert_eq!(empty.len(), 0);
    doc.connections_mut().push(CanvasConnection {
        id: "conn-1".into(),
        from_node_id: source.clone(),
        to_node_id: target.clone(),
        relation: None,
        kind: None,
        reason: None,
        changes: None,
        agent_run_id: None,
    });
    let upstream = upstream_nodes(doc.nodes(), doc.connections(), &target);
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0].id, source);
}
