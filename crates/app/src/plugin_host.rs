//! The canvas-side plugin host: the `crates/plugins` node registry, the
//! installed-plugin manager model, per-node plugin-private metadata and the
//! resolved content cache.
//!
//! The five official plugins ship compiled in, so they start installed and
//! enabled; disabling one unregisters its node definitions (the type then
//! resolves to the missing-plugin placeholder). Install-from-URL is a **record
//! only** flow: no network fetch and no dynamic JavaScript import happen — the
//! URL plus the would-be local source path are stored on the record (see
//! [`PluginHost::install_from_url`], defined with the rest of the manager model
//! in `plugin_manager_model.rs`).
//!
//! The installed list is persisted as `crates/plugins`' [`InstalledRecord`]s in
//! `<data-root>/plugins/registry.json`: [`PluginHost::new`] restores it and the
//! manager flows write it back through [`PluginHost::save_registry`]. The file
//! is an app-stamped envelope (`app`/`version`, like `library.json`) holding the
//! records plus the ids the user uninstalled, so an official plugin this binary
//! ships cannot reappear after a restart. The records carry the plugin identity
//! plus a manifest; the official plugins are native
//! (`crates/plugins/src/official`), so their records are identity-only and
//! their node definitions stay owned by [`PluginHost::sync_registry`].

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use open_canvas_core::constants::NodeSpec;
use open_canvas_core::node::CanvasNodeData;
use open_canvas_plugins::abi::MetadataExtras;
use open_canvas_plugins::manager::{
    InstalledPlugin, InstalledRecord, InstalledRegistry, PluginBundle, PluginManager,
};
use open_canvas_plugins::manifest::PluginManifest;
use open_canvas_plugins::official::official_plugins;
use open_canvas_plugins::registry::NodeRegistry;
use open_canvas_plugins::view::PluginNodeView;

use crate::doc::Doc;
use crate::plugin_content::{plugin_cell, PluginCell};
use crate::plugin_manager_model::{official_url, PluginUi};

/// Discriminator of the persisted plugin registry (`plugins/registry.json`).
pub const PLUGIN_REGISTRY_APP: &str = "open-canvas-desktop-plugins";
/// Schema version of the plugin registry file.
pub const PLUGIN_REGISTRY_VERSION: u32 = 1;

/// The on-disk plugin registry: the installed records plus the ids the user
/// uninstalled (tombstones, so the compiled-in official defaults stay gone).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    app: String,
    version: u32,
    installed: Vec<InstalledRecord>,
    #[serde(default)]
    removed: Vec<String>,
}

impl RegistryFile {
    /// Parses and validates the file discriminator; a foreign or future file is
    /// a typed error the host reports instead of applying half of it.
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let file: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if file.app != PLUGIN_REGISTRY_APP {
            return Err(format!("unsupported plugin registry app {:?}", file.app));
        }
        if file.version != PLUGIN_REGISTRY_VERSION {
            return Err(format!(
                "unsupported plugin registry version {}",
                file.version
            ));
        }
        Ok(file)
    }
}

/// The plugin host: registry + manager + plugin-private node metadata.
pub struct PluginHost {
    pub registry: NodeRegistry,
    pub manager: PluginManager,
    pub ui: PluginUi,
    /// Per-node plugin-private metadata (keys the fixed core schema cannot
    /// model), persisted in the project's extras sidecar.
    extras: BTreeMap<String, MetadataExtras>,
    /// `<data-root>/plugins`: where an installed bundle *would* be cached.
    pub(crate) plugins_dir: PathBuf,
    /// `<plugins-dir>/registry.json`: the persisted install registry.
    registry_path: PathBuf,
    /// Ids the user uninstalled; they suppress the official default install on
    /// the next start (an id installed again drops its tombstone on save).
    removed: BTreeSet<String>,
    /// A non-fatal startup problem with the persisted registry; the manager
    /// modal shows it once (see `open_plugin_manager`).
    pub registry_error: Option<String>,
    /// The node whose sticky-note palette row is open.
    pub palette_node: Option<String>,
    /// node id -> (svg source, raster); rebuilt only when the source changes.
    svg_cache: BTreeMap<String, (String, Option<slint::Image>)>,
}

impl PluginHost {
    /// Builds the host with all ten built-in definitions plus every enabled
    /// official plugin registered, the official plugins installed by default
    /// (they are compiled into this binary), and the persisted install registry
    /// applied over those defaults.
    pub fn new(plugins_dir: PathBuf) -> Self {
        let registry_path = crate::store::plugin_registry_path(&plugins_dir);
        let mut host = Self {
            registry: NodeRegistry::new(),
            manager: PluginManager::new(),
            ui: PluginUi::default(),
            extras: BTreeMap::new(),
            plugins_dir,
            registry_path,
            removed: BTreeSet::new(),
            registry_error: None,
            palette_node: None,
            svg_cache: BTreeMap::new(),
        };
        host.registry.register(
            open_canvas_plugins::registry::BUILTIN_PLUGIN_ID,
            open_canvas_plugins::builtins::builtin_definitions(),
        );
        let installed_at = crate::store::now_iso();
        for plugin in official_plugins() {
            let bundle = PluginBundle::new(
                plugin.id.clone(),
                plugin.name.clone(),
                plugin.version.clone(),
                official_url(&plugin.id),
                "builtin".to_string(),
            );
            let bundle = match plugin.description.clone() {
                Some(text) => bundle.with_description(text),
                None => bundle,
            };
            host.manager.install(bundle, true, installed_at.clone());
        }
        host.sync_registry();
        host.restore_registry();
        host
    }

    /// Re-registers every enabled plugin's node definitions and unregisters the
    /// disabled ones (registry registration is keyed by owner id).
    pub fn sync_registry(&mut self) {
        for plugin in official_plugins() {
            let enabled = self
                .manager
                .get(&plugin.id)
                .is_some_and(|record| record.enabled);
            if enabled {
                self.registry.register_plugin(&plugin);
            } else {
                self.registry.unregister_plugin(&plugin.id);
            }
        }
    }

    /// The persisted registry file (`<data-root>/plugins/registry.json`).
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    /// Writes the registry file: every installed plugin as a `crates/plugins`
    /// [`InstalledRecord`] (validated through [`InstalledRegistry`]) plus the
    /// uninstall tombstones that are still uninstalled.
    pub fn save_registry(&self) -> Result<(), String> {
        let registry = InstalledRegistry::from_records(self.installed_records())
            .map_err(|error| format!("plugin registry: {error}"))?;
        let installed = registry.to_records();
        let live: BTreeSet<&str> = installed.iter().map(|record| record.id.as_str()).collect();
        let removed: Vec<String> = self
            .removed
            .iter()
            .filter(|id| !live.contains(id.as_str()))
            .cloned()
            .collect();
        let file = RegistryFile {
            app: PLUGIN_REGISTRY_APP.to_string(),
            version: PLUGIN_REGISTRY_VERSION,
            installed,
            removed,
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|error| format!("plugin registry: {error}"))?;
        crate::store::write_bytes(self.registry_path(), json.as_bytes())
            .map_err(|error| format!("plugin registry: {error}"))
    }

    /// The manager list as persisted records, in manager order.
    fn installed_records(&self) -> Vec<InstalledRecord> {
        self.manager
            .plugins()
            .iter()
            .map(installed_record)
            .collect()
    }

    /// Applies `<plugins-dir>/registry.json` over the official defaults: a
    /// missing file keeps them (first run), a broken one is reported through
    /// [`PluginHost::registry_error`] and keeps them too.
    fn restore_registry(&mut self) {
        let bytes = match std::fs::read(self.registry_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                self.registry_error = Some(format!(
                    "The installed-plugin registry could not be read ({error}); official defaults apply"
                ));
                return;
            }
        };
        let file = match RegistryFile::parse(&bytes) {
            Ok(file) => file,
            Err(error) => {
                self.registry_error = Some(format!(
                    "The installed-plugin registry could not be restored ({error}); official defaults apply"
                ));
                return;
            }
        };
        // `InstalledRegistry` validates every record and converts the enabled
        // declarative ones; the official plugins are native, so their
        // identity-only records contribute no definitions here.
        match InstalledRegistry::with_registry(NodeRegistry::new(), file.installed.clone()) {
            Ok(registry) => {
                for record in registry.enabled() {
                    match record.manifest.to_plugin() {
                        Ok(plugin) => self.registry.register_plugin(&plugin),
                        Err(error) => {
                            self.registry_error = Some(format!("Plugin {}: {error}", record.id));
                        }
                    }
                }
                self.apply_records(registry.records(), &file.removed);
            }
            Err(error) => {
                self.registry_error = Some(format!(
                    "The installed-plugin registry could not be restored ({error}); official defaults apply"
                ));
            }
        }
    }

    /// Applies the persisted records over the official defaults: the tombstones
    /// first (an uninstalled official plugin must not come back), then every
    /// record in file order, then one registry sync.
    fn apply_records(&mut self, records: &[InstalledRecord], removed: &[String]) {
        for id in removed {
            self.manager.uninstall(id);
        }
        self.removed = removed.iter().cloned().collect();
        // Records are saved newest first and `install` prepends, so rebuilding
        // them in reverse restores the manager order.
        for record in records.iter().rev() {
            let installed = record.to_installed();
            let source = self.installed_source(&installed);
            let mut bundle = PluginBundle::new(
                installed.id.clone(),
                installed.name,
                installed.version,
                installed.url,
                source,
            );
            if let Some(description) = installed.description {
                bundle = bundle.with_description(description);
            }
            let id = bundle.id.clone();
            let enabled = installed.enabled;
            self.manager
                .install(bundle, installed.official, installed.installed_at);
            self.manager.set_enabled(&id, enabled);
        }
        self.sync_registry();
    }

    /// The `source` a restored record shows: the official pseudo-path for a
    /// compiled-in plugin, else the bundle cache path the install recorded.
    fn installed_source(&self, plugin: &InstalledPlugin) -> String {
        if plugin.official {
            "builtin".to_string()
        } else {
            self.local_source_path(&plugin.id)
        }
    }

    /// `setEnabled` + registry sync. Returns `false` for an unknown id.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if !self.manager.set_enabled(id, enabled) {
            return false;
        }
        self.sync_registry();
        true
    }

    /// Uninstalls a record and unregisters its definitions (if it had any). The
    /// id is tombstoned so a compiled-in official plugin stays uninstalled on
    /// the next start.
    pub fn uninstall(&mut self, id: &str) -> Option<InstalledPlugin> {
        let removed = self.manager.uninstall(id);
        if removed.is_some() {
            self.registry.unregister_plugin(id);
            self.removed.insert(id.to_string());
        }
        removed
    }

    pub fn is_registered(&self, type_id: &str) -> bool {
        self.registry.is_registered(type_id)
    }

    /// Registry spec (size/title/default metadata) for a node type; the web
    /// registry fallback applies to unknown types.
    pub fn spec(&self, type_id: &str) -> NodeSpec {
        self.registry.get_spec(type_id)
    }

    /// Default plugin-private metadata a freshly created node of this type
    /// starts with.
    pub fn default_extras(&self, type_id: &str) -> MetadataExtras {
        self.registry.get_default_extras(type_id)
    }

    /// The definition's built-in-panel prompt prefix, if it declares one
    /// (panorama's equirectangular constraint).
    pub fn prompt_prefix(&self, node: &CanvasNodeData) -> Option<String> {
        self.registry
            .get(node.node_type.as_str())?
            .use_builtin_panel?
            .prompt_prefix
    }

    /// Bridge view (background, handles, panel kind, interactivity) for a node.
    pub fn view_for(&self, node: &CanvasNodeData) -> PluginNodeView {
        let extras = self.extras_of(&node.id);
        self.registry.node_view(node, &extras)
    }

    pub fn extras_of(&self, id: &str) -> MetadataExtras {
        self.extras.get(id).cloned().unwrap_or_default()
    }

    pub fn set_extras(&mut self, id: &str, extras: MetadataExtras) {
        if extras.is_empty() {
            self.extras.remove(id);
        } else {
            self.extras.insert(id.to_string(), extras);
        }
    }

    pub fn set_extra(&mut self, id: &str, key: &str, value: serde_json::Value) {
        self.extras
            .entry(id.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// Copies a node's extras (node duplicate).
    pub fn copy_extras(&mut self, from: &str, to: &str) {
        if let Some(extras) = self.extras.get(from).cloned() {
            self.extras.insert(to.to_string(), extras);
        }
    }

    /// Drops extras of nodes that no longer exist in the document (delete/undo).
    pub fn prune_extras(&mut self, doc: &Doc) {
        self.extras.retain(|id, _| doc.node(id).is_some());
        self.svg_cache.retain(|id, _| doc.node(id).is_some());
    }

    /// Resolves the native content cell of `node` (and refreshes the SVG
    /// raster when its source changed).
    pub fn cell(&mut self, node: &CanvasNodeData, upstream: &[CanvasNodeData]) -> PluginCell {
        let extras = self.extras_of(&node.id);
        let cell = plugin_cell(
            node,
            &extras,
            self.is_registered(node.node_type.as_str()),
            upstream,
        );
        self.refresh_svg_cache(&node.id, &cell);
        cell
    }

    /// The cached raster of an SVG node, if its source is valid.
    pub fn svg_image(&self, id: &str) -> Option<slint::Image> {
        self.svg_cache.get(id)?.1.clone()
    }

    fn refresh_svg_cache(&mut self, node_id: &str, cell: &PluginCell) {
        let Some(source) = cell.svg_source.as_deref() else {
            self.svg_cache.remove(node_id);
            return;
        };
        if self
            .svg_cache
            .get(node_id)
            .is_some_and(|(cached, _)| cached == source)
        {
            return;
        }
        let image = cell.svg_image();
        self.svg_cache
            .insert(node_id.to_string(), (source.to_string(), image));
    }

    /// Replaces the persisted extras map (project load).
    pub fn load_extras(&mut self, extras: BTreeMap<String, MetadataExtras>) {
        self.extras = extras;
        self.svg_cache.clear();
    }

    /// The extras map persisted for the open project.
    pub fn extras_snapshot(&self) -> BTreeMap<String, MetadataExtras> {
        self.extras.clone()
    }
}

/// The persisted record of one installed plugin: its identity plus a manifest.
/// The five official plugins are compiled in as native `CanvasPlugin`s, so the
/// manifest carries identity/description only (their node definitions live in
/// `crates/plugins/src/official`); a URL install declares no nodes at all.
fn installed_record(plugin: &InstalledPlugin) -> InstalledRecord {
    let manifest = PluginManifest {
        id: plugin.id.clone(),
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        description: plugin.description.clone(),
        min_app_version: None,
        nodes: Vec::new(),
        settings: BTreeMap::new(),
        i18n: BTreeMap::new(),
    };
    InstalledRecord::new(manifest, plugin.url.clone())
        .with_official(plugin.official)
        .with_enabled(plugin.enabled)
        .with_installed_at(plugin.installed_at.clone())
}

/// Direct upstream nodes of `node` (web `ctx.getUpstream()`): the sources of
/// every connection targeting it, in connection order.
pub fn upstream_nodes(
    nodes: &[CanvasNodeData],
    connections: &[open_canvas_core::node::CanvasConnection],
    node_id: &str,
) -> Vec<CanvasNodeData> {
    connections
        .iter()
        .filter(|connection| connection.to_node_id == node_id)
        .filter_map(|connection| {
            nodes
                .iter()
                .find(|node| node.id == connection.from_node_id)
                .cloned()
        })
        .collect()
}

#[cfg(test)]
#[path = "plugin_tests/host_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "plugin_tests/persistence_tests.rs"]
mod persistence_tests;

#[cfg(test)]
#[path = "plugin_tests/registry_tests.rs"]
mod registry_tests;
