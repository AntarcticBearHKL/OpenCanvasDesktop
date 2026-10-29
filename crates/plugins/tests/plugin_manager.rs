use open_canvas_plugins::manager::{compare_semver, has_upgrade};
use open_canvas_plugins::{InstalledPlugin, OfficialPluginEntry, PluginBundle, PluginManager};

fn bundle(id: &str, version: &str) -> PluginBundle {
    PluginBundle::new(
        id,
        format!("{id} name"),
        version,
        "https://cdn/plugin.js",
        "bundle source",
    )
    .with_description(format!("{id} description"))
}

#[test]
fn install_enables_prepends_and_stamps() {
    let mut manager = PluginManager::new();
    assert!(manager.is_empty());

    manager.install(bundle("a", "1.0.0"), false, "2026-01-01T00:00:00Z");
    let record = manager.get("a").expect("installed");
    assert!(record.enabled);
    assert_eq!(record.name, "a name");
    assert_eq!(record.url, "https://cdn/plugin.js");
    assert_eq!(record.source, "bundle source");
    assert_eq!(record.installed_at, "2026-01-01T00:00:00Z");
    assert!(!record.local);
    assert!(!record.official);
    assert!(manager.is_installed("a"));
    assert!(!manager.is_installed("ghost"));

    manager.install(bundle("b", "1.0.0"), true, "2026-01-02T00:00:00Z");
    let ids: Vec<&str> = manager
        .plugins()
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(ids, ["b", "a"]);
    assert!(manager.get("b").is_some_and(|item| item.official));
}

#[test]
fn install_replaces_in_place_and_clears_local() {
    let mut manager = PluginManager::new();
    manager.discover_local(bundle("a", "1.0.0"), "t1");
    manager.install(bundle("b", "1.0.0"), false, "t2");
    manager.install(bundle("a", "2.0.0"), false, "t3");

    let record = manager.get("a").expect("installed");
    assert_eq!(record.version, "2.0.0");
    assert!(record.enabled);
    assert!(!record.local);
    let ids: Vec<&str> = manager
        .plugins()
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(ids, ["b", "a"]);
}

#[test]
fn discovered_local_plugins_start_disabled_and_keep_their_state() {
    let mut manager = PluginManager::new();
    let first = manager.discover_local(bundle("local", "1.0.0"), "t1");
    assert!(!first.enabled);
    assert!(first.local);
    assert!(!first.official);

    assert!(manager.set_enabled("local", true));
    let refreshed = manager.discover_local(bundle("local", "1.1.0"), "t2");
    assert!(refreshed.enabled);
    assert_eq!(refreshed.version, "1.1.0");
    assert_eq!(refreshed.installed_at, "t2");
}

#[test]
fn set_enabled_reports_unknown_ids() {
    let mut manager = PluginManager::new();
    manager.install(bundle("a", "1.0.0"), false, "t1");
    assert!(!manager.set_enabled("ghost", true));

    assert!(manager.set_enabled("a", false));
    assert!(!manager.get("a").expect("installed").enabled);
    assert!(manager.enabled().is_empty());
    assert_eq!(manager.len(), 1);

    assert!(manager.set_enabled("a", true));
    let enabled: Vec<&str> = manager
        .enabled()
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(enabled, ["a"]);
}

#[test]
fn uninstall_removes_and_returns_the_record() {
    let mut manager = PluginManager::new();
    manager.install(bundle("a", "1.0.0"), false, "t1");
    let removed = manager.uninstall("a").expect("removed");
    assert_eq!(removed.id, "a");
    assert!(manager.is_empty());
    assert!(manager.uninstall("a").is_none());
}

#[test]
fn official_entries_flag_newer_versions_only() {
    let installed = InstalledPlugin {
        id: "html".into(),
        name: "HTML".into(),
        version: "1.2.0".into(),
        description: None,
        url: "https://cdn/html.js".into(),
        source: String::new(),
        enabled: true,
        local: false,
        official: true,
        installed_at: "t".into(),
    };
    let newer = OfficialPluginEntry {
        id: "html".into(),
        name: "HTML".into(),
        version: "1.3.0".into(),
        description: None,
        icon: None,
        url: "https://cdn/html.js".into(),
    };
    let same = OfficialPluginEntry {
        version: "1.2.0".into(),
        ..newer.clone()
    };
    let older = OfficialPluginEntry {
        version: "1.1.0".into(),
        ..newer.clone()
    };
    assert!(newer.upgrades(&installed));
    assert!(!same.upgrades(&installed));
    assert!(!older.upgrades(&installed));
}

#[test]
fn semver_comparison_is_numeric_and_lenient() {
    assert_eq!(compare_semver("1.2.0", "1.10.0"), -1);
    assert_eq!(compare_semver("1.10.0", "1.2.0"), 1);
    assert_eq!(compare_semver("2.0.0", "1.9.9"), 1);
    assert_eq!(compare_semver("1.0.0", "1.0.0"), 0);
    assert_eq!(compare_semver("1.2", "1.2.0"), 0);
    assert_eq!(compare_semver("1.2.0", "1.2.0.9"), 0);
    assert_eq!(compare_semver("1.2.x", "1.2.0"), 0);
    assert_eq!(compare_semver("1.02.0", "1.2.0"), 0);

    assert!(has_upgrade("1.0.0", "1.0.1"));
    assert!(has_upgrade("0.9.9", "1.0.0"));
    assert!(!has_upgrade("1.0.0", "1.0.0"));
    assert!(!has_upgrade("1.0.1", "1.0.0"));
    assert!(!has_upgrade("1.0.0", "1.0.0-beta"));
}
