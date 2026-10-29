//! Plugin registry persistence: the installed list is written to
//! `<data-root>/plugins/registry.json` and restored on the next start, so a
//! disabled plugin stays disabled, an uninstalled one stays gone (tombstones)
//! and a URL install survives. A broken or foreign file falls back to the
//! official defaults and reports itself.

use super::*;
use crate::app::App;
use crate::store::Store;

/// A host on a throwaway data directory plus its store.
fn temp_host() -> Option<(tempfile::TempDir, Store, PluginHost)> {
    let dir = tempfile::tempdir().ok()?;
    let store = Store::at(dir.path().to_path_buf());
    let host = PluginHost::new(store.plugins_dir());
    Some((dir, store, host))
}

fn registry_text(host: &PluginHost) -> String {
    std::fs::read_to_string(host.registry_path()).unwrap_or_default()
}

#[test]
fn disabled_plugin_stays_disabled_after_a_restart() {
    let Some((_dir, store, mut host)) = temp_host() else {
        return;
    };
    assert!(host.set_enabled("svg", false));
    assert!(host.save_registry().is_ok());

    let reloaded = PluginHost::new(store.plugins_dir());
    assert!(reloaded.registry_error.is_none());
    assert!(reloaded
        .manager
        .get("svg")
        .is_some_and(|record| !record.enabled));
    assert!(!reloaded.is_registered("svg:vector"));
    assert!(reloaded.is_registered("markdown:doc"));
    assert!(reloaded
        .manager
        .get("markdown")
        .is_some_and(|record| record.enabled));
}

#[test]
fn uninstalled_official_plugin_stays_gone_after_a_restart() {
    let Some((_dir, store, mut host)) = temp_host() else {
        return;
    };
    assert!(host.uninstall("html").is_some());
    assert!(host.save_registry().is_ok());

    let reloaded = PluginHost::new(store.plugins_dir());
    assert!(reloaded.registry_error.is_none());
    assert!(reloaded.manager.get("html").is_none());
    assert!(!reloaded.is_registered("html:render"));
    // The rest of the official set is untouched.
    assert!(reloaded
        .manager
        .get("svg")
        .is_some_and(|record| record.enabled));
    assert!(reloaded.is_registered("svg:vector"));
}

#[test]
fn url_install_survives_a_restart_with_its_source_path() {
    let Some((_dir, store, mut host)) = temp_host() else {
        return;
    };
    let installed = host
        .install_from_url("https://example.com/demo-widget.js")
        .ok();
    assert!(installed.is_some());
    assert!(host.save_registry().is_ok());

    let reloaded = PluginHost::new(store.plugins_dir());
    let record = reloaded.manager.get("demo-widget");
    assert!(record.is_some_and(|record| record.enabled && !record.official));
    assert_eq!(
        record.map(|record| record.url.as_str()),
        Some("https://example.com/demo-widget.js")
    );
    assert_eq!(
        record.map(|record| record.source.clone()),
        Some(reloaded.local_source_path("demo-widget")),
        "the bundle cache path is reconstructed from the install id"
    );
    // The restored order keeps the install in front of the officials.
    assert_eq!(
        reloaded
            .manager
            .plugins()
            .first()
            .map(|record| record.id.as_str()),
        Some("demo-widget")
    );
}

#[test]
fn registry_file_is_an_app_stamped_envelope_of_records() {
    let Some((_dir, store, host)) = temp_host() else {
        return;
    };
    assert_eq!(
        crate::store::plugin_registry_path(&store.plugins_dir()),
        host.registry_path(),
        "the store and the host agree on the registry path"
    );
    assert!(host.registry_path().ends_with("registry.json"));
    assert!(host.save_registry().is_ok());
    let text = registry_text(&host);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    assert_eq!(
        value.get("app").and_then(serde_json::Value::as_str),
        Some(PLUGIN_REGISTRY_APP)
    );
    assert_eq!(
        value.get("version").and_then(serde_json::Value::as_u64),
        Some(u64::from(PLUGIN_REGISTRY_VERSION))
    );
    let installed = value
        .get("installed")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(installed.len(), 5, "the five official plugins are recorded");
    assert!(installed.iter().any(|record| {
        record.get("id").and_then(serde_json::Value::as_str) == Some("svg")
            && record.get("enabled").and_then(serde_json::Value::as_bool) == Some(true)
            && record.get("official").and_then(serde_json::Value::as_bool) == Some(true)
            && record.get("manifest").is_some()
    }));
    assert_eq!(
        value
            .get("removed")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(0),
        "no tombstones until something is uninstalled"
    );
}

#[test]
fn broken_and_foreign_registries_fall_back_and_report() {
    let Some((_dir, store, host)) = temp_host() else {
        return;
    };
    assert!(std::fs::create_dir_all(store.plugins_dir()).is_ok());
    assert!(std::fs::write(host.registry_path(), b"{ not json").is_ok());
    let broken = PluginHost::new(store.plugins_dir());
    assert!(broken.registry_error.is_some());
    assert!(broken
        .manager
        .get("svg")
        .is_some_and(|record| record.enabled));

    let foreign = serde_json::json!({
        "app": "other-app",
        "version": 1,
        "installed": [],
    });
    assert!(std::fs::write(host.registry_path(), foreign.to_string()).is_ok());
    let reloaded = PluginHost::new(store.plugins_dir());
    assert!(reloaded
        .registry_error
        .is_some_and(|error| error.contains("unsupported")));
    assert!(reloaded.manager.get("svg").is_some());
}

#[test]
fn app_flows_write_through_and_restore_on_the_next_start() {
    let Some(dir) = tempfile::tempdir().ok() else {
        return;
    };
    let store = Store::at(dir.path().to_path_buf());
    let mut app = App::new(store.clone());
    assert!(app.apply_toggle("svg", false));
    app.apply_install("https://example.com/demo-widget.js");
    assert!(app.apply_uninstall("html"));
    assert!(
        app.plugin
            .ui
            .message
            .as_ref()
            .is_some_and(|(_, error)| !error),
        "the uninstall reported success"
    );

    let restarted = App::new(store);
    assert!(restarted
        .plugin
        .manager
        .get("svg")
        .is_some_and(|record| !record.enabled));
    assert!(restarted.plugin.manager.get("demo-widget").is_some());
    assert!(restarted.plugin.manager.get("html").is_none());
    assert!(!restarted.plugin.is_registered("svg:vector"));
    assert!(!restarted.plugin.is_registered("html:render"));
    assert!(restarted.plugin.registry_error.is_none());
}
