//! The plugin-manager model as applied to the host: official-registry entries,
//! the record-only install flow, enable/uninstall and the URL parsing.
//!
//! The five official plugins ship compiled in, so they start installed and
//! enabled; disabling one unregisters its node definitions (the type then
//! resolves to the missing-plugin placeholder). Install-from-URL is a **record
//! only** flow: no network fetch and no dynamic JavaScript import happen — the
//! URL plus the would-be local source path are stored on the record.

use open_canvas_plugins::manager::{InstalledPlugin, OfficialPluginEntry, PluginBundle};
use open_canvas_plugins::official::official_plugins;

use crate::plugin_host::PluginHost;

/// Sub-tabs of the plugin manager modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManagerTab {
    #[default]
    Official,
    Local,
    ThirdParty,
}

impl ManagerTab {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "official" => Some(Self::Official),
            "local" => Some(Self::Local),
            "third-party" => Some(Self::ThirdParty),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Local => "local",
            Self::ThirdParty => "third-party",
        }
    }
}

/// Why an install-from-URL request was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallError {
    Empty,
    NotAUrl,
    NoPluginName,
}

impl InstallError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "Enter a plugin URL first",
            Self::NotAUrl => "Plugin URL must start with http:// or https://",
            Self::NoPluginName => "Could not derive a plugin name from the URL",
        }
    }
}

/// Open/tab/URL/feedback state of the plugin UI (the manager modal).
#[derive(Debug, Default)]
pub struct PluginUi {
    pub manager_open: bool,
    pub tab: ManagerTab,
    pub url: String,
    /// Feedback line after an install/uninstall attempt: `(text, is_error)`.
    pub message: Option<(String, bool)>,
    /// Official-tab refresh: `true` until the short loading window elapses (the
    /// entries are compiled in, but the UI keeps the web's refresh affordance).
    pub refreshing: bool,
    /// Deadline of the loading window in app milliseconds.
    pub refresh_until: u64,
}

impl PluginUi {
    /// Starts the official-tab loading window (700ms).
    pub fn begin_refresh(&mut self, now: u64) {
        self.refreshing = true;
        self.refresh_until = now.saturating_add(700);
    }

    /// Ends the loading window once the deadline passed; `true` when the state
    /// flipped and the modal should be re-synced.
    pub fn tick_refresh(&mut self, now: u64) -> bool {
        if self.refreshing && now >= self.refresh_until {
            self.refreshing = false;
            return true;
        }
        false
    }
}

/// `builtin://` URL recorded for the compiled-in official plugins.
pub(crate) fn official_url(id: &str) -> String {
    format!("builtin://official/{id}.js")
}

impl PluginHost {
    /// Official-registry entries (ids/versions/descriptions of the plugins that
    /// ship with this build). The URL is the `builtin://` pseudo-URL recorded on
    /// the install.
    pub fn official_entries() -> Vec<OfficialPluginEntry> {
        official_plugins()
            .into_iter()
            .map(|plugin| OfficialPluginEntry {
                url: official_url(&plugin.id),
                id: plugin.id,
                name: plugin.name,
                version: plugin.version,
                description: plugin.description,
                icon: None,
            })
            .collect()
    }

    /// Record-only install: stores the entered URL and the local source path the
    /// bundle would be cached at (`<data-root>/plugins/<id>.js`). No fetch and
    /// no dynamic import happen — the plugin contributes no node definitions, so
    /// it is listed and toggleable but inert until a later native loader exists.
    /// Installing a URL that matches an official entry records it as official.
    pub fn install_from_url(&mut self, url: &str) -> Result<InstalledPlugin, InstallError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(InstallError::Empty);
        }
        let official = Self::official_entries()
            .into_iter()
            .find(|entry| entry.url == url);
        if official.is_none() && !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(InstallError::NotAUrl);
        }
        let id = plugin_id_from_url(url).ok_or(InstallError::NoPluginName)?;
        let source = self.local_source_path(&id);
        let (name, version, description, is_official) = match official {
            Some(entry) => (entry.name, entry.version, entry.description, true),
            None => (id.clone(), "0.0.0".to_string(), None, false),
        };
        let bundle = PluginBundle {
            id,
            name,
            version,
            description,
            url: url.to_string(),
            source,
        };
        let record = self
            .manager
            .install(bundle, is_official, crate::store::now_iso())
            .clone();
        if is_official {
            self.sync_registry();
        }
        Ok(record)
    }

    /// The would-be on-disk cache path of a plugin bundle (`plugins\<id>.js`).
    pub fn local_source_path(&self, id: &str) -> String {
        self.plugins_dir
            .join(format!("{id}.js"))
            .to_string_lossy()
            .into_owned()
    }
}

/// `node-registry`-style plugin id from a URL: the file stem of the last path
/// segment, lowercased with anything outside `[a-z0-9-]` collapsed to `-`.
pub fn plugin_id_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', '\\']).next()?;
    let stem = last.split(['.', '?', '#']).next().unwrap_or_default();
    let mut id = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
