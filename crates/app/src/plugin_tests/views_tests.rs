//! Manager-row / toolbar-extra tests for the plugin views.

use super::*;
use crate::plugin_tests_support::{plugin_host, plugin_node, sticky_node};

#[test]
fn creatable_types_include_the_pure_image_node() {
    assert!(is_creatable_node_type("image"));
    for type_id in CREATABLE_NODE_TYPES {
        assert!(
            is_creatable_node_type(type_id),
            "{type_id} has a dock button"
        );
    }
}

/// Text nodes are no longer created from the dock (the type still renders
/// existing documents). The creation path refuses them exactly like `image`.
#[test]
fn creatable_types_exclude_the_text_node() {
    assert!(
        !is_creatable_node_type("text"),
        "the dock no longer creates text nodes"
    );
    assert!(
        !CREATABLE_NODE_TYPES.contains(&"text"),
        "no text button in the creatable list"
    );
}

#[test]
fn manager_rows_follow_the_manager_state() {
    let mut host = plugin_host();
    let official = manager_rows(&host, ManagerTab::Official);
    assert_eq!(official.len(), 5);
    assert!(official
        .iter()
        .all(|row| row.installed && row.enabled && row.badge == "official"));

    let installed = host.install_from_url("https://example.com/third.js");
    assert!(installed.is_ok());
    let third = manager_rows(&host, ManagerTab::ThirdParty);
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].badge, "third-party");
    assert_eq!(third[0].name, "third");

    assert!(host.set_enabled("third", false));
    let third = manager_rows(&host, ManagerTab::ThirdParty);
    assert!(third[0].installed);
    assert!(!third[0].enabled);
    assert_eq!(manager_rows(&host, ManagerTab::Local).len(), 0);
}

#[test]
fn toolbar_extras_declare_the_official_toolbar_contracts() {
    let mut host = plugin_host();
    let panorama = plugin_node("panorama:viewer", Some("data:image/png;base64,AA"));
    let ids = action_ids(toolbar_extras(&host, &panorama));
    assert!(ids.contains(&"plugin-generate".to_string()));
    assert!(ids.contains(&"plugin-reset".to_string()));
    assert!(ids.contains(&"plugin-interact".to_string()));

    let empty_panorama = plugin_node("panorama:viewer", None);
    let ids = action_ids(toolbar_extras(&host, &empty_panorama));
    assert!(ids.contains(&"plugin-generate".to_string()));
    assert!(!ids.contains(&"plugin-reset".to_string()));
    assert!(!ids.contains(&"plugin-interact".to_string()));

    let sticky = sticky_node(Some("note"));
    let ids = action_ids(toolbar_extras(&host, &sticky));
    assert!(ids.contains(&"plugin-palette".to_string()));
    assert!(!ids.contains(&"plugin-generate".to_string()));

    host.palette_node = Some(sticky.id.clone());
    assert!(toolbar_extras(&host, &sticky)
        .iter()
        .any(|action| action.id.as_str() == "plugin-palette" && action.active));

    let markdown = plugin_node("markdown:doc", Some("# hi"));
    let ids = action_ids(toolbar_extras(&host, &markdown));
    assert!(ids.contains(&"plugin-interact".to_string()));
}

fn action_ids(actions: Vec<HoverAction>) -> Vec<String> {
    actions
        .into_iter()
        .map(|action| action.id.to_string())
        .collect()
}
