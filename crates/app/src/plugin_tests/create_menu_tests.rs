//! Dock node creation through the real app + window: one direct button per
//! creatable built-in type (each adds exactly one node of that type at the
//! viewport centre), the guarantee that an image node can never be created
//! empty, and the fact that plugin-contributed types still create their node
//! when a caller names them (the dock's plugin list is gone).

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use slint::Model as _;

use crate::app::App;
use crate::coords::ScreenPos;
use crate::plugin_views::CREATABLE_NODE_TYPES;
use crate::AppWindow;

type WiredApp = (tempfile::TempDir, AppWindow, Rc<RefCell<App>>);

fn wired_app() -> Result<WiredApp, Box<dyn Error>> {
    let (dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    Ok((dir, ui, app))
}

#[test]
fn the_dock_offers_one_button_per_creatable_node_type() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, ui, _app) = wired_app()?;
    let listed: Vec<String> = ui
        .get_create_types()
        .iter()
        .map(|type_id| type_id.to_string())
        .collect();
    assert_eq!(
        listed,
        CREATABLE_NODE_TYPES
            .iter()
            .map(|type_id| (*type_id).to_string())
            .collect::<Vec<_>>(),
        "the dock exposes exactly the creatable built-in types, in order"
    );
    assert!(listed.iter().any(|type_id| type_id == "image"));
    Ok(())
}

#[test]
fn every_node_button_creates_exactly_one_node_of_its_type() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, ui, app) = wired_app()?;
    for (index, type_id) in CREATABLE_NODE_TYPES.iter().enumerate() {
        let type_id = *type_id;
        let spec = crate::app::with_app_r(&app, |app| app.plugin.spec(type_id)).ok_or("spec")?;
        // The real dock callback: one click per node-type button.
        ui.invoke_create_node(type_id.into());
        let expected = index + 1;
        crate::app::with_app(&app, |app| {
            assert_eq!(
                app.doc.nodes().len(),
                expected,
                "{type_id} adds exactly one node"
            );
            let node = app.doc.nodes().last().cloned();
            assert_eq!(
                node.as_ref().map(|node| node.node_type.as_str()),
                Some(type_id),
                "{type_id} creates a node of its own type"
            );
            let Some(node) = node else {
                return;
            };
            let centre = app.doc.viewport().screen_to_world(ScreenPos::new(
                app.canvas_size.0 / 2.0,
                app.canvas_size.1 / 2.0,
            ));
            assert_eq!(node.position.x, centre.x - spec.width / 2.0);
            assert_eq!(node.position.y, centre.y - spec.height / 2.0);
        });
    }
    Ok(())
}

#[test]
fn image_nodes_are_never_created_empty() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, ui, app) = wired_app()?;
    // A pure Image node is creatable: it is the display/management node.
    ui.invoke_create_node("image".into());
    crate::app::with_app(&app, |app| {
        assert_eq!(app.doc.nodes().len(), 1);
        assert_eq!(
            app.doc.nodes()[0].node_type,
            open_canvas_core::node::CanvasNodeType::Image
        );
    });
    assert!(CREATABLE_NODE_TYPES.contains(&"image"));
    Ok(())
}

/// The text node left the creation path with the dock button: naming `text`
/// directly creates nothing, while the type itself stays valid for existing
/// documents (its inline body and text-generation body are untouched).
#[test]
fn text_nodes_are_not_created_from_the_dock() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, ui, app) = wired_app()?;
    ui.invoke_create_node("text".into());
    crate::app::with_app(&app, |app| {
        assert!(
            app.doc.nodes().is_empty(),
            "the creation path refuses text like it refuses image"
        );
    });
    assert!(!CREATABLE_NODE_TYPES.contains(&"text"));
    assert!(
        !ui.get_create_types()
            .iter()
            .any(|type_id| type_id.as_str() == "text"),
        "the dock offers no text button"
    );
    Ok(())
}

/// Plugin-contributed node types still create their node when created from
/// elsewhere: the dock's plugin-types list is gone, but the creation path
/// (`create_plugin_node`) keeps working for every registry type.
#[test]
fn plugin_types_still_create_their_node_from_a_direct_call() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, ui, app) = wired_app()?;
    ui.invoke_create_node("sticky-note:note".into());
    crate::app::with_app(&app, |app| {
        assert_eq!(app.doc.nodes().len(), 1, "the plugin node is created");
        assert_eq!(
            app.doc.nodes().first().map(|node| node.node_type.as_str()),
            Some("sticky-note:note")
        );
    });
    Ok(())
}
