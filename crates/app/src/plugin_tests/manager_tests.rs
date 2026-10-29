//! Manager-modal state machine exercised through the same app-level cores the
//! Slint callbacks call. The `AppWindow` itself is not instantiated (headless
//! tests): the wire handlers are thin wrappers around these cores plus the
//! property/row sync.

use super::*;
use crate::plugin_manager_model::ManagerTab;
use crate::plugin_views::manager_rows;
use crate::store::Store;
use open_canvas_core::node::CanvasNodeType;

fn test_app() -> Option<(tempfile::TempDir, App)> {
    let dir = tempfile::tempdir().ok()?;
    let app = App::new(Store::at(dir.path().to_path_buf()));
    Some((dir, app))
}

#[test]
fn manager_refresh_window_starts_and_ends_on_the_tick() {
    let Some((_dir, mut app)) = test_app() else {
        return;
    };
    assert!(!app.plugin.ui.refreshing);
    app.plugin.ui.begin_refresh(1_000);
    assert!(app.plugin.ui.refreshing);
    assert!(!app.plugin.ui.tick_refresh(1_200), "still loading");
    assert!(app.plugin.ui.refreshing);
    assert!(app.plugin.ui.tick_refresh(1_700), "the window ends");
    assert!(!app.plugin.ui.refreshing);
    assert!(!app.plugin.ui.tick_refresh(2_000), "no second flip");
}

#[test]
fn manager_install_toggle_uninstall_flows_through_the_app_cores() {
    let Some((_dir, mut app)) = test_app() else {
        return;
    };
    assert_eq!(manager_rows(&app.plugin, ManagerTab::Official).len(), 5);
    assert_eq!(manager_rows(&app.plugin, ManagerTab::ThirdParty).len(), 0);

    // Install from URL: recorded, enabled, and listed under Third-party.
    app.apply_install("https://example.com/demo-widget.js");
    let message = app.plugin.ui.message.clone();
    assert!(message.is_some_and(|(_, error)| !error));
    let third = manager_rows(&app.plugin, ManagerTab::ThirdParty);
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].id, "demo-widget");
    assert!(third[0].installed && third[0].enabled);
    // A record-only install contributes no node types to the registry.
    assert_eq!(
        app.plugin
            .registry
            .list()
            .iter()
            .filter(|definition| definition.show_in_create_menu
                && app.plugin.registry.get_node_plugin_id(&definition.type_id)
                    != open_canvas_plugins::registry::BUILTIN_PLUGIN_ID)
            .count(),
        5
    );

    // Disabling an official plugin unregisters its node type; the manager row
    // shows it disabled and the type resolves to the missing-plugin placeholder.
    assert!(app.apply_toggle("sticky-note", false));
    let sticky = manager_rows(&app.plugin, ManagerTab::Official)
        .into_iter()
        .find(|row| row.id == "sticky-note");
    assert!(sticky.is_some_and(|row| row.installed && !row.enabled));
    assert!(!app
        .plugin
        .registry
        .list()
        .iter()
        .any(|definition| definition.type_id == "sticky-note:note"));
    assert!(app.apply_toggle("sticky-note", true));
    assert!(app
        .plugin
        .registry
        .list()
        .iter()
        .any(|definition| definition.type_id == "sticky-note:note"));

    // Uninstall drops the third-party record.
    assert!(app.apply_uninstall("demo-widget"));
    assert_eq!(manager_rows(&app.plugin, ManagerTab::ThirdParty).len(), 0);
    assert!(!app.apply_uninstall("demo-widget"));
}

#[test]
fn manager_install_validation_reports_errors_in_the_ui_message() {
    let Some((_dir, mut app)) = test_app() else {
        return;
    };
    app.apply_install("ftp://example.com/x.js");
    let message = app.plugin.ui.message.clone();
    assert!(message.is_some_and(|(text, error)| error && text.contains("http")));
    assert_eq!(manager_rows(&app.plugin, ManagerTab::ThirdParty).len(), 0);
}

#[test]
fn create_menu_creates_a_plugin_node_with_registry_spec_and_default_extras() {
    let Some((_dir, mut app)) = test_app() else {
        return;
    };
    let spec = app.plugin.spec("sticky-note:note");
    assert_eq!(spec.width, 240.0);
    assert_eq!(spec.height, 200.0);
    assert_eq!(spec.title, "Sticky note");

    let extras = app.plugin.default_extras("sticky-note:note");
    assert!(extras.contains_key("pluginColor"));
    let id = app.doc.create_node_with(
        CanvasNodeType::from_id("sticky-note:note"),
        crate::coords::WorldPos::new(500.0, 400.0),
        spec,
        11,
    );
    app.plugin.set_extras(&id, extras);
    let Some(node) = app.doc.node(&id) else {
        return;
    };
    assert_eq!(node.node_type, CanvasNodeType::from_id("sticky-note:note"));
    // The registry spec centers the card on the drop point (240x200).
    assert_eq!(node.position.x, 380.0);
    assert_eq!(node.position.y, 300.0);
    assert!(app.plugin.extras_of(&id).contains_key("pluginColor"));
}
