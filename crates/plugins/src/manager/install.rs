//! Declarative-plugin install state: the persisted [`InstalledRecord`] and the
//! [`InstalledRegistry`] rebuilt from it. Kept apart from the JS-bundle
//! [`super::PluginManager`] because it owns no `source` blob.

use serde::{Deserialize, Serialize};

use crate::manifest::{ManifestError, PluginManifest};
use crate::registry::NodeRegistry;

use super::InstalledPlugin;

/// One persisted install record for a declarative plugin: the identity/state
/// fields of `InstalledPlugin` plus the parsed [`PluginManifest`]. Serde
/// round-trips it so the app can save and restore the installed list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Installation source used for updates. Empty for a purely local manifest.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub enabled: bool,
    /// Installed from the official registry.
    #[serde(default)]
    pub official: bool,
    /// ISO-8601 install timestamp supplied by the host.
    #[serde(default)]
    pub installed_at: String,
    pub manifest: PluginManifest,
}

impl InstalledRecord {
    /// Build a record from a validated manifest; new installs start enabled.
    pub fn new(manifest: PluginManifest, url: impl Into<String>) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            url: url.into(),
            enabled: true,
            official: false,
            installed_at: String::new(),
            manifest,
        }
    }

    /// Mark the record as installed from the official registry.
    pub fn with_official(mut self, official: bool) -> Self {
        self.official = official;
        self
    }

    /// Stamp the host-supplied ISO-8601 install time.
    pub fn with_installed_at(mut self, installed_at: impl Into<String>) -> Self {
        self.installed_at = installed_at.into();
        self
    }

    /// Set the enabled flag (new installs are enabled by default).
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Description from the manifest, if any.
    pub fn description(&self) -> Option<&str> {
        self.manifest.description.as_deref()
    }

    /// Project onto the JS-bundle install record. Declarative plugins have no
    /// cached source, so `source` is empty.
    pub fn to_installed(&self) -> InstalledPlugin {
        InstalledPlugin {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.manifest.description.clone(),
            url: self.url.clone(),
            source: String::new(),
            enabled: self.enabled,
            local: false,
            official: self.official,
            installed_at: self.installed_at.clone(),
        }
    }
}

/// The installed declarative-plugin list plus the [`NodeRegistry`] rebuilt from
/// it. Only enabled records are registered, matching `plugin-loader.ts`'s
/// activate/deactivate split.
pub struct InstalledRegistry {
    registry: NodeRegistry,
    records: Vec<InstalledRecord>,
}

impl std::fmt::Debug for InstalledRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InstalledRegistry")
            .field("records", &self.records)
            .finish_non_exhaustive()
    }
}

impl InstalledRegistry {
    /// Rebuild from records using a fresh registry.
    pub fn from_records(records: Vec<InstalledRecord>) -> Result<Self, ManifestError> {
        Self::with_registry(NodeRegistry::new(), records)
    }

    /// Rebuild onto an existing registry (e.g. one that already owns the nine
    /// built-in definitions).
    pub fn with_registry(
        registry: NodeRegistry,
        records: Vec<InstalledRecord>,
    ) -> Result<Self, ManifestError> {
        for record in records.iter().filter(|record| record.enabled) {
            let plugin = record.manifest.to_plugin()?;
            registry.register_plugin(&plugin);
        }
        Ok(Self { registry, records })
    }

    /// The inverse of [`InstalledRegistry::from_records`].
    pub fn to_records(&self) -> Vec<InstalledRecord> {
        self.records.clone()
    }

    pub fn records(&self) -> &[InstalledRecord] {
        &self.records
    }

    pub fn registry(&self) -> &NodeRegistry {
        &self.registry
    }

    pub fn get(&self, id: &str) -> Option<&InstalledRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    pub fn enabled(&self) -> impl Iterator<Item = &InstalledRecord> {
        self.records.iter().filter(|record| record.enabled)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Canonical JSON for the whole list.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.records).unwrap_or_default()
    }

    /// Restore the list from JSON and rebuild the registry.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ManifestError> {
        Self::from_json_with_registry(NodeRegistry::new(), bytes)
    }

    /// Restore onto an existing registry from JSON.
    pub fn from_json_with_registry(
        registry: NodeRegistry,
        bytes: &[u8],
    ) -> Result<Self, ManifestError> {
        let records: Vec<InstalledRecord> =
            serde_json::from_slice(bytes).map_err(|err| ManifestError::Syntax {
                message: err.to_string(),
            })?;
        Self::with_registry(registry, records)
    }
}
