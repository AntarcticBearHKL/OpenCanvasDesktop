//! Plugin manager model: installed-plugin records and their mutations,
//! mirroring `use-plugin-store.ts`, the state half of `plugin-loader.ts`, and
//! the official-registry version check in `plugin-registry.ts`.
//!
//! Data/state only: fetching a plugin bundle, caching it, and evaluating /
//! importing it are host concerns and are deliberately not modelled here (no
//! network, no dynamic JavaScript import). The host supplies the evaluated
//! [`PluginBundle`] and the install timestamp, so this module stays pure and
//! testable.
//!
//! For declarative plugins there is no JavaScript bundle at all: the install
//! state is a [`InstalledRecord`] carrying the parsed
//! [`crate::manifest::PluginManifest`], which round-trips through serde.
//! [`InstalledRegistry`] rebuilds a [`crate::registry::NodeRegistry`] from those
//! records (registering only the enabled plugins) and is the seam the app
//! persists and restores.

/// `InstalledPlugin` (use-plugin-store.ts:6-17) as plain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    /// Installation source used for updates.
    pub url: String,
    /// Cached plugin source for offline use and pinned versions.
    pub source: String,
    pub enabled: bool,
    /// Discovered in the local plugin directory; disabled by default.
    pub local: bool,
    /// Installed from the official registry.
    pub official: bool,
    /// ISO-8601 install timestamp supplied by the host.
    pub installed_at: String,
}

/// A plugin bundle the host has already fetched and evaluated, before it is
/// recorded as an [`InstalledPlugin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginBundle {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub url: String,
    pub source: String,
}

impl PluginBundle {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        url: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: None,
            url: url.into(),
            source: source.into(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A resolved official-registry entry (`plugin-registry.ts:4-11`). Relative
/// entries are resolved to absolute URLs by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialPluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub url: String,
}

impl OfficialPluginEntry {
    /// Whether this entry is a newer version than the installed record
    /// (`canvas-plugin-manager-modal.tsx:172`: `hasUpgrade(record.version, entry.version)`).
    pub fn upgrades(&self, record: &InstalledPlugin) -> bool {
        has_upgrade(&record.version, &self.version)
    }

    /// The `old → new` label the manager modal shows beside an upgradeable row.
    pub fn upgrade_label(&self, record: &InstalledPlugin) -> String {
        upgrade_label(&record.version, &self.version)
    }

    /// Whether the row shows its upgrade dot (`upgrades`, as a display flag).
    pub fn shows_upgrade_dot(&self, record: &InstalledPlugin) -> bool {
        self.upgrades(record)
    }
}

/// `updateAvailable` display helpers: the `old → new` version string and the
/// boolean the manager modal renders as an upgrade dot
/// (`canvas-plugin-manager-modal.tsx`, `hasUpgrade`).
pub fn upgrade_label(installed_version: &str, remote_version: &str) -> String {
    format!("{installed_version} \u{2192} {remote_version}")
}

/// Whether an upgrade dot should be shown for `installed_version`.
pub fn shows_upgrade_dot(installed_version: &str, remote_version: &str) -> bool {
    has_upgrade(installed_version, remote_version)
}

mod install;

pub use install::{InstalledRecord, InstalledRegistry};

/// The installed-plugin list and its state machine
/// (`use-plugin-store.ts` + the state half of `plugin-loader.ts`).
#[derive(Debug, Clone, Default)]
pub struct PluginManager {
    plugins: Vec<InstalledPlugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plugins(&self) -> &[InstalledPlugin] {
        &self.plugins
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.iter().find(|record| record.id == id)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Every enabled record, in list order (the startup activation set,
    /// `plugin-loader.ts:100`).
    pub fn enabled(&self) -> Vec<&InstalledPlugin> {
        self.plugins
            .iter()
            .filter(|record| record.enabled)
            .collect()
    }

    /// `installPluginFromUrl`'s state half (`plugin-loader.ts:61-68`): replace the
    /// previous version in place, or prepend a new record. Always enabled; a URL
    /// install clears the `local` flag and sets `official` from the caller.
    pub fn install(
        &mut self,
        bundle: PluginBundle,
        official: bool,
        installed_at: impl Into<String>,
    ) -> &InstalledPlugin {
        let record = InstalledPlugin {
            id: bundle.id,
            name: bundle.name,
            version: bundle.version,
            description: bundle.description,
            url: bundle.url,
            source: bundle.source,
            enabled: true,
            local: false,
            official,
            installed_at: installed_at.into(),
        };
        self.upsert(record)
    }

    /// `loadLocalPlugins`'s state half (`plugin-loader.ts:128-143`): refresh a
    /// discovered local plugin's metadata and source while preserving its
    /// `enabled` flag; a new discovery starts disabled.
    pub fn discover_local(
        &mut self,
        bundle: PluginBundle,
        installed_at: impl Into<String>,
    ) -> &InstalledPlugin {
        let enabled = self.get(&bundle.id).is_some_and(|record| record.enabled);
        let record = InstalledPlugin {
            id: bundle.id,
            name: bundle.name,
            version: bundle.version,
            description: bundle.description,
            url: bundle.url,
            source: bundle.source,
            enabled,
            local: true,
            official: false,
            installed_at: installed_at.into(),
        };
        self.upsert(record)
    }

    /// `setEnabled` (use-plugin-store.ts:37). Returns `false` when the id is
    /// unknown and therefore nothing changed.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        match self.plugins.iter_mut().find(|record| record.id == id) {
            Some(record) => {
                record.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// `remove` + uninstall (`use-plugin-store.ts:38`, `plugin-loader.ts:87-90`).
    pub fn uninstall(&mut self, id: &str) -> Option<InstalledPlugin> {
        let index = self.plugins.iter().position(|record| record.id == id)?;
        Some(self.plugins.remove(index))
    }

    fn upsert(&mut self, record: InstalledPlugin) -> &InstalledPlugin {
        match self.plugins.iter().position(|item| item.id == record.id) {
            Some(index) => {
                self.plugins[index] = record;
                &self.plugins[index]
            }
            None => {
                self.plugins.insert(0, record);
                &self.plugins[0]
            }
        }
    }
}

/// `compareSemver` (plugin-registry.ts:36-44): compare the numeric
/// major.minor.patch components only (non-numeric parts, such as prerelease
/// labels, parse as `0`). Positive means `a` is newer than `b`.
pub fn compare_semver(a: &str, b: &str) -> i32 {
    let left = parse_semver(a);
    let right = parse_semver(b);
    for (component, other) in left.iter().zip(right.iter()) {
        let diff = component - other;
        if diff != 0 {
            return diff.signum() as i32;
        }
    }
    0
}

/// `hasUpgrade` (plugin-registry.ts:47-49): is `remote_version` newer than
/// `installed_version`.
pub fn has_upgrade(installed_version: &str, remote_version: &str) -> bool {
    compare_semver(remote_version, installed_version) > 0
}

fn parse_semver(version: &str) -> [i64; 3] {
    let mut components = [0_i64; 3];
    for (index, part) in version.split('.').take(3).enumerate() {
        components[index] = parse_int_prefix(part);
    }
    components
}

/// `parseInt(part, 10) || 0`: leading whitespace and sign, then the leading run
/// of ASCII digits; anything else (including an empty or non-numeric part) is 0.
fn parse_int_prefix(part: &str) -> i64 {
    let trimmed = part.trim_start();
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1_i64, rest),
        None => (1_i64, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let value = digits
        .chars()
        .take_while(char::is_ascii_digit)
        .fold(0_i64, |acc, digit| {
            acc.saturating_mul(10)
                .saturating_add(i64::from(digit as u8 - b'0'))
        });
    sign * value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestError;

    fn bundle(id: &str, version: &str) -> PluginBundle {
        PluginBundle::new(id, id, version, "https://x/plugin.js", "source")
    }

    #[test]
    fn install_prepends_and_is_enabled() {
        let mut manager = PluginManager::new();
        manager.install(bundle("a", "1.0.0"), false, "t1");
        manager.install(bundle("b", "1.0.0"), false, "t2");
        let ids: Vec<&str> = manager.plugins().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert!(manager.get("a").is_some_and(|r| r.enabled));
    }

    #[test]
    fn install_same_id_replaces_in_place() {
        let mut manager = PluginManager::new();
        manager.install(bundle("a", "1.0.0"), false, "t1");
        manager.install(bundle("b", "1.0.0"), false, "t2");
        let updated = manager.install(bundle("a", "2.0.0"), true, "t3");
        assert_eq!(updated.version, "2.0.0");
        assert!(updated.official);
        let ids: Vec<&str> = manager.plugins().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
    }

    #[test]
    fn discover_local_defaults_disabled_and_preserves_enabled() {
        let mut manager = PluginManager::new();
        let first = manager.discover_local(bundle("local", "1.0.0"), "t1");
        assert!(!first.enabled);
        assert!(first.local);
        assert!(manager.set_enabled("local", true));
        let refreshed = manager.discover_local(bundle("local", "1.1.0"), "t2");
        assert!(refreshed.enabled);
        assert_eq!(refreshed.version, "1.1.0");
        assert!(!manager.set_enabled("ghost", true));
    }

    #[test]
    fn uninstall_removes_the_record() {
        let mut manager = PluginManager::new();
        manager.install(bundle("a", "1.0.0"), false, "t1");
        assert!(manager.uninstall("a").is_some());
        assert!(manager.is_empty());
        assert!(manager.uninstall("a").is_none());
    }

    #[test]
    fn semver_compares_numeric_components_only() {
        assert_eq!(compare_semver("1.2.0", "1.10.0"), -1);
        assert_eq!(compare_semver("2.0.0", "1.9.9"), 1);
        assert_eq!(compare_semver("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_semver("1.0.x", "1.0.0"), 0);
        assert_eq!(compare_semver("1.0.0", "1.0"), 0);
    }

    #[test]
    fn has_upgrade_only_when_remote_is_newer() {
        assert!(has_upgrade("1.0.0", "1.0.1"));
        assert!(!has_upgrade("1.0.1", "1.0.0"));
        assert!(!has_upgrade("1.0.0", "1.0.0"));
        assert!(!has_upgrade("1.0.0", "1.0.0-beta"));
    }

    fn record(id: &str, version: &str, enabled: bool) -> InstalledRecord {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "id": id,
            "version": version,
            "nodes": [{
                "type": format!("{id}:note"),
                "title": "Note",
                "renderKind": "markdown",
                "icon": "file-text"
            }]
        }))
        .unwrap_or_default();
        let manifest = crate::manifest::parse_manifest(&bytes).expect("valid manifest");
        InstalledRecord::new(manifest, "https://x/plugin.json")
            .with_installed_at("t1")
            .with_enabled(enabled)
    }

    #[test]
    fn installed_record_round_trips_through_serde() {
        let original = record("acme", "1.2.3", true);
        let json = serde_json::to_string(&original).expect("serialise");
        let restored: InstalledRecord = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(restored, original);
        assert_eq!(restored.manifest.id, "acme");
        assert_eq!(restored.manifest.nodes.len(), 1);
    }

    #[test]
    fn registry_rebuilds_from_records_and_registers_enabled_only() {
        let records = vec![record("on", "1.0.0", true), record("off", "1.0.0", false)];
        let rebuilt = InstalledRegistry::from_records(records.clone()).expect("rebuild");
        assert_eq!(rebuilt.len(), 2);
        assert_eq!(rebuilt.enabled().count(), 1);
        assert!(rebuilt.registry().is_registered("on:note"));
        assert!(!rebuilt.registry().is_registered("off:note"));
        assert_eq!(rebuilt.to_records(), records, "to_records is the inverse");
    }

    #[test]
    fn registry_round_trips_through_json() {
        let rebuilt =
            InstalledRegistry::from_records(vec![record("acme", "2.0.0", true)]).expect("rebuild");
        let json = rebuilt.to_json();
        let restored = InstalledRegistry::from_json(json.as_bytes()).expect("restore");
        assert_eq!(restored.to_records(), rebuilt.to_records());
        assert!(restored.registry().is_registered("acme:note"));
    }

    #[test]
    fn rebuild_fails_on_an_unsupported_render_kind() {
        let mut bad = record("acme", "1.0.0", true);
        bad.manifest.nodes[0].render_kind = "acme:future".to_string();
        assert!(matches!(
            InstalledRegistry::from_records(vec![bad]),
            Err(ManifestError::UnsupportedRenderKind { .. })
        ));
    }

    #[test]
    fn upgrade_helpers_describe_the_transition() {
        let installed = InstalledPlugin {
            id: "a".to_string(),
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            url: String::new(),
            source: String::new(),
            enabled: true,
            local: false,
            official: true,
            installed_at: String::new(),
        };
        let entry = OfficialPluginEntry {
            id: "a".to_string(),
            name: "a".to_string(),
            version: "1.1.0".to_string(),
            description: None,
            icon: None,
            url: String::new(),
        };
        assert_eq!(upgrade_label("1.0.0", "1.1.0"), "1.0.0 \u{2192} 1.1.0");
        assert!(shows_upgrade_dot("1.0.0", "1.1.0"));
        assert!(!shows_upgrade_dot("1.1.0", "1.0.0"));
        assert_eq!(entry.upgrade_label(&installed), "1.0.0 \u{2192} 1.1.0");
        assert!(entry.shows_upgrade_dot(&installed));
        assert!(entry.upgrades(&installed));
    }

    #[test]
    fn installed_record_projects_onto_installed_plugin() {
        let record = record("acme", "3.0.0", true);
        let installed = record.to_installed();
        assert_eq!(installed.id, "acme");
        assert_eq!(installed.version, "3.0.0");
        assert!(installed.enabled);
        assert!(installed.source.is_empty());
    }
}
