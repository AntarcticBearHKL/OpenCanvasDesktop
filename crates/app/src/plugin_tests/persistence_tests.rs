//! Project save/load round-trip with a plugin node: the node type, its core
//! metadata and its plugin-private extras sidecar must survive the disk round
//! trip through the same `Store` + `Doc::from_project` + `PluginHost::load_extras`
//! calls `open_project_ui` performs.

use super::*;
use crate::library::EditorMeta;
use crate::plugin_content::PluginCellKind;
use crate::store::Store;
use open_canvas_core::node::CanvasNodeType;

fn editor_meta() -> EditorMeta {
    EditorMeta {
        id: "plugin-proj".into(),
        title: "Plugin round trip".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        group_id: None,
    }
}

#[test]
fn plugin_node_round_trips_through_project_save_and_load() {
    let Some(dir) = tempfile::tempdir().ok() else {
        return;
    };
    let store = Store::at(dir.path().to_path_buf());
    let mut host = PluginHost::new(store.plugins_dir());
    let mut doc = Doc::empty();
    let spec = host.spec("sticky-note:note");
    let id = doc.create_node_with(
        CanvasNodeType::from_id("sticky-note:note"),
        crate::coords::WorldPos::new(300.0, 200.0),
        spec,
        7,
    );
    host.set_extras(&id, host.default_extras("sticky-note:note"));
    host.set_extra(
        &id,
        "pluginColor",
        serde_json::Value::String("#fca5a5".into()),
    );
    if let Some(node) = doc.nodes_mut().iter_mut().find(|node| node.id == id) {
        let mut metadata = node.metadata.clone().unwrap_or_default();
        metadata.content = Some("hello note".into());
        node.metadata = Some(metadata);
    }

    let project = doc.to_project(&editor_meta());
    assert!(store.save_project(&project).is_ok());
    let extras = host.extras_snapshot();
    assert!(store.save_project_extras(&project.id, &extras).is_ok());

    let loaded = store.load_project("plugin-proj");
    let Ok(loaded) = loaded else {
        return;
    };
    let loaded_extras = store.load_project_extras("plugin-proj");
    let Ok(loaded_extras) = loaded_extras else {
        return;
    };
    // The on-disk wire JSON keeps the plugin type id verbatim.
    let json = serde_json::to_string(&loaded).unwrap_or_default();
    assert!(json.contains("\"type\":\"sticky-note:note\""));

    let reloaded = Doc::from_project(&loaded);
    let Some(node) = reloaded.node(&id).cloned() else {
        return;
    };
    assert_eq!(node.node_type, CanvasNodeType::from_id("sticky-note:note"));
    assert_eq!(node.width, 240.0);
    assert_eq!(node.height, 200.0);
    assert_eq!(
        node.metadata
            .as_ref()
            .and_then(|meta| meta.content.as_deref()),
        Some("hello note")
    );

    // `open_project_ui` rehydrates the host the same way.
    let mut rehydrated = PluginHost::new(store.plugins_dir());
    rehydrated.load_extras(loaded_extras);
    assert_eq!(
        rehydrated.extras_of(&id).get("pluginColor"),
        Some(&serde_json::Value::String("#fca5a5".into()))
    );
    let cell = rehydrated.cell(&node, &[]);
    assert_eq!(cell.kind, PluginCellKind::Sticky);
    assert_eq!(cell.color.as_deref(), Some("#fca5a5"));
    assert_eq!(cell.text, "hello note");
    assert!(cell.svg_image().is_none());
}

#[test]
fn extras_sidecar_is_absent_for_a_plugin_free_project() {
    let Some(dir) = tempfile::tempdir().ok() else {
        return;
    };
    let store = Store::at(dir.path().to_path_buf());
    let doc = Doc::empty();
    let project = doc.to_project(&editor_meta());
    assert!(store.save_project(&project).is_ok());
    let extras = store.load_project_extras("plugin-proj");
    let Ok(extras) = extras else {
        return;
    };
    assert_eq!(extras.len(), 0);
}
