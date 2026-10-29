//! Disk persistence for the project library: one `library.json` index (groups +
//! per-project rows) next to one full `CanvasProject` JSON per project, plus the
//! JSON export/import helpers built on the `open-canvas` v3 envelope.
//!
//! Data directory (documented choice): `%APPDATA%\open-canvas-desktop\` — the
//! roaming app-data root Windows owns for per-user application state, so the
//! library survives moving/reinstalling the binary and needs no admin rights.
//! An explicit `OPEN_CANVAS_DATA_DIR` environment variable overrides it (used by
//! the screenshot script and tests to keep runs hermetic). Layout:
//!
//! ```text
//! %APPDATA%\open-canvas-desktop\
//!   library.json            # LibraryIndex: groups + project rows
//!   projects\<id>.json      # one CanvasProject per file (web-compatible JSON)
//!   exports\<title>.zip     # packed CanvasExportFile archives written by Export
//!   plugins\registry.json   # installed-plugin registry (see `plugin_host`)
//! ```
//!
//! Every write goes through a temp file + rename so a crash mid-write cannot
//! leave a truncated document, and every fallible step returns a typed
//! [`StoreError`] — the UI reports it; nothing here panics.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use open_canvas_ai::config::AiConfig;
use open_canvas_core::document::{
    deserialize_library, to_json, CanvasProject, DocumentError, LibraryIndex,
};
// The plain-JSON envelope helpers below are test-only (the export flow packs a
// ZIP instead), so their imports follow the same `cfg(test)` gate.
#[cfg(test)]
use open_canvas_core::document::{deserialize_export, CanvasExportFile};

/// Overrides the data directory (screenshot automation / tests).
pub const DATA_DIR_ENV: &str = "OPEN_CANVAS_DATA_DIR";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error on {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("document error: {0}")]
    Document(#[from] DocumentError),
    #[error("config error: {0}")]
    Config(String),
    #[error("image project error: {0}")]
    Image(String),
    #[error("plugin extras error: {0}")]
    Extras(String),
    #[error("invalid project id {0:?} (must be a plain file name)")]
    InvalidId(String),
    #[error("path {0} has no parent directory")]
    NoParent(PathBuf),
}

/// Discriminator of the persisted runtime-log file.
pub const LOGS_FILE_APP: &str = "open-canvas-desktop-logs";
/// Schema version of the runtime-log file.
pub const LOGS_FILE_VERSION: u32 = 1;

/// On-disk envelope of the runtime log (newest first).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct LogFile {
    app: String,
    version: u32,
    entries: Vec<LogEntryFile>,
}

/// One serialized runtime-log entry; the level stays a plain string so an
/// unknown level from a newer build degrades to `info` instead of failing.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct LogEntryFile {
    level: String,
    scope: String,
    message: String,
    timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<String>,
}

impl LogEntryFile {
    fn from_entry(entry: open_canvas_ai::runtime_log::RuntimeLogEntry) -> Self {
        Self {
            level: entry.level.as_str().to_string(),
            scope: entry.scope,
            message: entry.message,
            timestamp: entry.timestamp,
            data: entry.data,
        }
    }

    fn into_entry(self) -> open_canvas_ai::runtime_log::RuntimeLogEntry {
        use open_canvas_ai::runtime_log::{RuntimeLogEntry, RuntimeLogLevel};
        let level = match self.level.as_str() {
            "warn" => RuntimeLogLevel::Warn,
            "error" => RuntimeLogLevel::Error,
            _ => RuntimeLogLevel::Info,
        };
        RuntimeLogEntry {
            level,
            scope: self.scope,
            message: self.message,
            timestamp: self.timestamp,
            data: self.data,
        }
    }
}

/// A JSON store rooted at one directory (see the module docs).
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Resolves the data directory: `OPEN_CANVAS_DATA_DIR`, else
    /// `%APPDATA%\open-canvas-desktop`, else `%USERPROFILE%\.open-canvas-desktop`.
    pub fn resolve() -> Self {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
            if !dir.is_empty() {
                return Self::at(PathBuf::from(dir));
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            if !appdata.is_empty() {
                return Self::at(PathBuf::from(appdata).join("open-canvas-desktop"));
            }
        }
        let home = std::env::var_os("USERPROFILE").map_or_else(std::env::temp_dir, PathBuf::from);
        Self::at(home.join(".open-canvas-desktop"))
    }

    /// A store at an explicit root (tests, and [`Store::resolve`]).
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<data-root>\plugins`: where a plugin bundle would be cached (the
    /// install-from-URL flow records the path but never fetches or writes it).
    pub fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join("library.json")
    }

    /// `config.json` next to `library.json`: the AI provider/model config,
    /// including the API keys (the file is local to the user's data directory).
    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    /// Reads the AI config; a missing file yields the preset defaults. The loaded
    /// value is normalized so a partial or older file still resolves models.
    pub fn load_ai_config(&self) -> Result<AiConfig, StoreError> {
        let path = self.config_path();
        match read_text(&path) {
            Ok(Some(text)) => serde_json::from_str::<AiConfig>(&text)
                .map(AiConfig::normalize)
                .map_err(|error| StoreError::Config(error.to_string())),
            Ok(None) => Ok(AiConfig::default()),
            Err(source) => Err(StoreError::Io { path, source }),
        }
    }

    pub fn save_ai_config(&self, config: &AiConfig) -> Result<(), StoreError> {
        write_json(&self.config_path(), config)
    }

    /// `projects\<id>.json`; ids are validated so an imported document cannot
    /// escape the projects directory through `..` or a separator.
    pub fn project_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !is_safe_id(id) {
            return Err(StoreError::InvalidId(id.to_string()));
        }
        Ok(self.root.join("projects").join(format!("{id}.json")))
    }

    /// `exports\<title>.json`; the canvas export flow appends `.zip` to it, the
    /// studio image flows `.png`.
    pub fn export_path_for(&self, title: &str) -> PathBuf {
        self.root
            .join("exports")
            .join(format!("{}.json", safe_stem(title)))
    }

    /// Reads `library.json`; a missing file is an empty (valid) index.
    pub fn load_index(&self) -> Result<LibraryIndex, StoreError> {
        let path = self.index_path();
        let text = match read_text(&path) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(LibraryIndex::new()),
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        Ok(deserialize_library(&text)?)
    }

    pub fn save_index(&self, index: &LibraryIndex) -> Result<(), StoreError> {
        write_json(&self.index_path(), index)
    }

    pub fn load_project(&self, id: &str) -> Result<CanvasProject, StoreError> {
        let path = self.project_path(id)?;
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Err(StoreError::Io {
                path,
                source: io::Error::new(io::ErrorKind::NotFound, "project file is missing"),
            });
        };
        Ok(open_canvas_core::document::from_json(&text)?)
    }

    pub fn save_project(&self, project: &CanvasProject) -> Result<(), StoreError> {
        write_json(&self.project_path(&project.id)?, project)
    }

    /// `projects\<id>.extras.json`: plugin-private node metadata the fixed core
    /// `CanvasNodeMetadata` schema cannot carry (`pluginColor`, `editing`, ...),
    /// keyed by node id. It is a sidecar so the project JSON stays exactly the
    /// web-compatible `CanvasProject`; a missing file is an empty map.
    pub fn project_extras_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !is_safe_id(id) {
            return Err(StoreError::InvalidId(id.to_string()));
        }
        Ok(self.root.join("projects").join(format!("{id}.extras.json")))
    }

    pub fn load_project_extras(
        &self,
        id: &str,
    ) -> Result<BTreeMap<String, open_canvas_plugins::abi::MetadataExtras>, StoreError> {
        let path = self.project_extras_path(id)?;
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Ok(BTreeMap::new());
        };
        serde_json::from_str(&text).map_err(|error| StoreError::Extras(error.to_string()))
    }

    pub fn save_project_extras(
        &self,
        id: &str,
        extras: &BTreeMap<String, open_canvas_plugins::abi::MetadataExtras>,
    ) -> Result<(), StoreError> {
        write_json(&self.project_extras_path(id)?, extras)
    }

    /// Removes the extras sidecar; a missing file is not an error.
    pub fn delete_project_extras(&self, id: &str) -> Result<(), StoreError> {
        let path = self.project_extras_path(id)?;
        remove_quiet(&path)
    }

    /// Removes a project document; a missing file is not an error (the index is
    /// the authority, and a half-deleted project must still leave the library).
    pub fn delete_project(&self, id: &str) -> Result<(), StoreError> {
        let path = self.project_path(id)?;
        remove_quiet(&path)
    }

    /// `images\<id>.json`; image projects live beside the canvas projects so
    /// both kinds keep the same id-safety rules.
    pub fn image_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !is_safe_id(id) {
            return Err(StoreError::InvalidId(id.to_string()));
        }
        Ok(self.root.join("images").join(format!("{id}.json")))
    }

    /// Reads an image project file (metadata + board document + embedded
    /// base64 PNG bitmaps).
    pub fn load_image(&self, id: &str) -> Result<crate::image::ImageFile, StoreError> {
        let path = self.image_path(id)?;
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Err(StoreError::Io {
                path,
                source: io::Error::new(io::ErrorKind::NotFound, "image project file is missing"),
            });
        };
        serde_json::from_str::<crate::image::ImageFile>(&text)
            .map_err(|error| StoreError::Image(error.to_string()))
    }

    pub fn save_image(&self, file: &crate::image::ImageFile) -> Result<(), StoreError> {
        write_json(&self.image_path(&file.project.id)?, file)
    }

    /// Removes an image project file; a missing file is not an error.
    pub fn delete_image(&self, id: &str) -> Result<(), StoreError> {
        let path = self.image_path(id)?;
        remove_quiet(&path)
    }

    /// `ui.json`: app-level view preferences (the web keeps these in
    /// localStorage — `canvas-side-panel-width` / `canvas-side-panel-open`).
    pub fn ui_state_path(&self) -> PathBuf {
        self.root.join("ui.json")
    }

    /// Reads the UI preferences; a missing or unreadable file yields the
    /// defaults so a broken state file can never block startup.
    pub fn load_ui_state(&self) -> Result<UiState, StoreError> {
        let path = self.ui_state_path();
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Ok(UiState::default());
        };
        Ok(UiState::parse(&text))
    }

    pub fn save_ui_state(&self, state: &UiState) -> Result<(), StoreError> {
        write_json(&self.ui_state_path(), state)
    }

    /// `<data-root>\dock.json`: the studio dock layouts (see
    /// [`crate::image::dock_engine_engine`]). Same data directory and the same
    /// app/version-discriminated JSON style as `ui.json`; the web keeps the
    /// equivalent payload in `localStorage` under
    /// `open-canvas:studio-dock:v4:<studio>`.
    pub fn dock_path(&self) -> PathBuf {
        self.root.join("dock.json")
    }

    /// Reads the dock preferences; a missing file is the default (empty) one,
    /// and a foreign or malformed file degrades to the defaults so a broken
    /// state file can never block a studio.
    pub fn load_dock_file(&self) -> Result<crate::image::dock_engine::DockFile, StoreError> {
        let path = self.dock_path();
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Ok(crate::image::dock_engine::DockFile::default());
        };
        let Ok(file) = serde_json::from_str::<crate::image::dock_engine::DockFile>(&text) else {
            return Ok(crate::image::dock_engine::DockFile::default());
        };
        if file.app != crate::image::dock_engine::DOCK_FILE_APP
            || file.version != crate::image::dock_engine::DOCK_FILE_VERSION
        {
            return Ok(crate::image::dock_engine::DockFile::default());
        }
        Ok(file)
    }

    pub fn save_dock_file(
        &self,
        file: &crate::image::dock_engine::DockFile,
    ) -> Result<(), StoreError> {
        write_json(&self.dock_path(), file)
    }

    /// `<data-root>\costs.json`: the generation-cost record list (capped by
    /// `GENERATION_COST_RECORD_LIMIT`, newest first). The web keeps the same
    /// list in localforage (`open-canvas:generation_cost_store`).
    pub fn costs_path(&self) -> PathBuf {
        self.root.join("costs.json")
    }

    /// Reads the cost records; a missing or unreadable file yields an empty log
    /// so a broken sidecar can never block the settings modal.
    pub fn load_costs(&self) -> Result<crate::cost::CostLog, StoreError> {
        let path = self.costs_path();
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Ok(crate::cost::CostLog::default());
        };
        crate::cost::CostLog::parse(&text)
            .map_err(|error| StoreError::Config(format!("cost records: {error}")))
    }

    pub fn save_costs(&self, log: &crate::cost::CostLog) -> Result<(), StoreError> {
        write_json(&self.costs_path(), log)
    }

    /// `<data-root>\logs.json`: the runtime-log ring buffer, so the settings
    /// modal still shows what happened before a restart (the web persists the
    /// same list in localforage).
    pub fn logs_path(&self) -> PathBuf {
        self.root.join("logs.json")
    }

    /// `<data-root>\exports\runtime-logs.txt`: the logs tab's Download target.
    pub fn logs_export_path(&self) -> PathBuf {
        self.root.join("exports").join("runtime-logs.txt")
    }

    /// Reads the persisted runtime log; a missing or malformed file yields an
    /// empty list so a broken sidecar can never block startup.
    pub fn load_logs(
        &self,
    ) -> Result<Vec<open_canvas_ai::runtime_log::RuntimeLogEntry>, StoreError> {
        let path = self.logs_path();
        let Some(text) = read_text(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?
        else {
            return Ok(Vec::new());
        };
        let file: LogFile = serde_json::from_str(&text)
            .map_err(|error| StoreError::Config(format!("runtime logs: {error}")))?;
        if file.app != LOGS_FILE_APP || file.version != LOGS_FILE_VERSION {
            return Ok(Vec::new());
        }
        Ok(file
            .entries
            .into_iter()
            .map(LogEntryFile::into_entry)
            .collect())
    }

    pub fn save_logs(
        &self,
        buffer: &open_canvas_ai::runtime_log::RuntimeLogBuffer,
    ) -> Result<(), StoreError> {
        let file = LogFile {
            app: LOGS_FILE_APP.to_string(),
            version: LOGS_FILE_VERSION,
            entries: buffer
                .snapshot()
                .into_iter()
                .map(LogEntryFile::from_entry)
                .collect(),
        };
        write_json(&self.logs_path(), &file)
    }

    /// `<data-root>\models`: where the local model manager looks for model
    /// files. The desktop ships no local inference runtime, so this is only a
    /// real location the user can drop files into (and the tab reflects it).
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    /// Deletes by the `ProjectIndexEntry.kind` tag: image projects have their
    /// own directory; everything else is a canvas project.
    pub fn delete_by_kind(&self, id: &str, kind: &str) -> Result<(), StoreError> {
        if kind == crate::image::PROJECT_KIND_IMAGE {
            return self.delete_image(id);
        }
        self.delete_project(id)?;
        self.delete_project_extras(id)
    }
}

/// Discriminator of the desktop UI-preferences file.
pub const UI_STATE_APP: &str = "open-canvas-desktop-ui";
/// Schema version of the UI-preferences file.
pub const UI_STATE_VERSION: u32 = 1;
/// Side-panel width bounds (`CANVAS_SIDE_PANEL_MIN_WIDTH/_MAX_WIDTH`).
pub const SIDE_PANEL_MIN_WIDTH: f64 = 220.0;
pub const SIDE_PANEL_MAX_WIDTH: f64 = 480.0;
/// Web default width (`CANVAS_SIDE_PANEL_DEFAULT_WIDTH`).
pub const SIDE_PANEL_DEFAULT_WIDTH: f64 = 280.0;
/// The shipped window-plate opacity (0–100): the white frost over the blurred
/// desktop. 80 is the asset-library default (its "20% transparency" tint),
/// which reads as a light frosted window instead of dark glass.
pub const WINDOW_TINT_DEFAULT: f64 = 80.0;
/// The shipped in-app Gaussian blur (0–100). 100 is the strongest frost, the
/// asset-library default.
pub const WINDOW_BLUR_DEFAULT: f64 = 100.0;
/// Persisted theme ids (`use-theme-store.ts` plus the follow-system option the
/// desktop adds; `follow-system` resolves against the OS at runtime).
pub const THEME_LIGHT: &str = "light";
pub const THEME_DARK: &str = "dark";
pub const THEME_FOLLOW_SYSTEM: &str = "follow-system";
/// Persisted locale ids (`web/src/i18n/index.ts` `AppLocale`).
pub const LOCALE_EN_US: &str = "en-US";
pub const LOCALE_ZH_CN: &str = "zh-CN";

/// Clamps any width (including NaN/infinite) into the persisted range.
pub fn clamp_side_panel_width(width: f64) -> f64 {
    if width.is_finite() {
        width.clamp(SIDE_PANEL_MIN_WIDTH, SIDE_PANEL_MAX_WIDTH)
    } else {
        SIDE_PANEL_DEFAULT_WIDTH
    }
}

/// Clamps a 0–100 setting (including NaN/infinite) into the slider's range: a
/// hand-edited `ui.json` can never ask for a value the UI cannot show.
pub fn clamp_percent(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

/// App-level view preferences persisted beside `library.json`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub app: String,
    pub version: u32,
    pub side_panel_width: f64,
    pub side_panel_open: bool,
    /// `light` | `dark` | `follow-system` (default `light`, the web default).
    #[serde(default = "default_theme_id")]
    pub theme: String,
    /// `en-US` | `zh-CN` (default `en-US`, the web default).
    #[serde(default = "default_locale_id")]
    pub locale: String,
    /// Window plate opacity, 0–100 (`100` is nearly opaque, `0` almost clear).
    #[serde(default = "default_tint_opacity", rename = "tint_opacity")]
    pub tint_opacity: f64,
    /// In-app Gaussian blur, 0–100 (`0` shows the live desktop; above 0 the
    /// whole-desktop capture is blurred by `glass_capture.rs`).
    #[serde(default = "default_blur")]
    pub blur: f64,
    /// True once the capture-based glass engine owns the backdrop. Files from
    /// the OS-accent era omit it (serde false), which triggers the one-time
    /// migration to the capture defaults in `App::new`.
    #[serde(default)]
    pub glass_capture: bool,
}

fn default_tint_opacity() -> f64 {
    WINDOW_TINT_DEFAULT
}

fn default_blur() -> f64 {
    WINDOW_BLUR_DEFAULT
}

fn default_theme_id() -> String {
    THEME_LIGHT.to_string()
}

fn default_locale_id() -> String {
    LOCALE_EN_US.to_string()
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            app: UI_STATE_APP.to_string(),
            version: UI_STATE_VERSION,
            side_panel_width: SIDE_PANEL_DEFAULT_WIDTH,
            side_panel_open: true,
            theme: default_theme_id(),
            locale: default_locale_id(),
            tint_opacity: default_tint_opacity(),
            blur: default_blur(),
            glass_capture: true,
        }
    }
}

impl UiState {
    /// Parses persisted text; foreign or malformed files fall back to defaults,
    /// the width is always clamped into range, and unknown theme/locale ids
    /// degrade to the defaults so a hand-edited file cannot wedge startup.
    pub fn parse(text: &str) -> Self {
        let Ok(mut state) = serde_json::from_str::<Self>(text) else {
            return Self::default();
        };
        if state.app != UI_STATE_APP || state.version != UI_STATE_VERSION {
            return Self::default();
        }
        state.side_panel_width = clamp_side_panel_width(state.side_panel_width);
        if !matches!(
            state.theme.as_str(),
            THEME_LIGHT | THEME_DARK | THEME_FOLLOW_SYSTEM
        ) {
            state.theme = default_theme_id();
        }
        if !matches!(state.locale.as_str(), LOCALE_EN_US | LOCALE_ZH_CN) {
            state.locale = default_locale_id();
        }
        state.tint_opacity = clamp_percent(state.tint_opacity);
        state.blur = clamp_percent(state.blur);
        state
    }
}

/// Best-effort file removal where "already gone" is success.
fn remove_quiet(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// `<plugins-dir>\registry.json`: the installed-plugin registry (derived here
/// so the plugin host and the store agree on one path).
pub fn plugin_registry_path(plugins_dir: &Path) -> PathBuf {
    plugins_dir.join("registry.json")
}

/// Writes a `CanvasExportFile` envelope as plain JSON: the pre-archive format,
/// kept for the store's own envelope round-trip tests. The library export flow
/// packs the envelope into a ZIP instead (`library_export.rs`).
#[cfg(test)]
pub fn write_export(path: &Path, export: &CanvasExportFile) -> Result<(), StoreError> {
    write_json(path, export)
}

/// Reads and validates a plain-JSON `CanvasExportFile` (`app == "open-canvas"`,
/// `version == 3`); the import flow reads the packed archive instead.
#[cfg(test)]
pub fn read_export(path: &Path) -> Result<CanvasExportFile, StoreError> {
    let text = read_text(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let Some(text) = text else {
        return Err(StoreError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::NotFound, "export file is missing"),
        });
    };
    Ok(deserialize_export(&text)?)
}

/// The current UTC time as `2026-01-02T03:04:05Z` (the web store's ISO shape).
pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Writes raw bytes anywhere on disk through a temp file + rename, so a crash
/// mid-write cannot leave a truncated archive or registry behind. Used by the
/// export archives (`.zip`) and the plugin registry.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let Some(parent) = path.parent() else {
        return Err(StoreError::NoParent(path.to_path_buf()));
    };
    fs::create_dir_all(parent).map_err(|source| StoreError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let tmp = temp_path(path);
    fs::write(&tmp, bytes).map_err(|source| StoreError::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, path).map_err(|source| {
        let leftover = fs::remove_file(&tmp);
        if let Err(cleanup) = leftover {
            eprintln!("store: could not remove {}: {cleanup}", tmp.display());
        }
        StoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Reads a file's bytes; a missing file is an `Io(NotFound)` error.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, StoreError> {
    fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// `archive.zip` → `archive.zip.tmp` (never collides with the target name).
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    name.push_str(".tmp");
    path.with_file_name(name)
}

/// `2026-01-02T03:04:05Z` → `01-02 03:04` for the library card footer. Anything
/// unexpected is echoed as-is (no panics on malformed input).
pub fn display_time(iso: &str) -> String {
    match (iso.get(5..10), iso.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time}"),
        _ => iso.to_string(),
    }
}

/// Lowercase ASCII/hyphen/underscore project ids only; everything else is
/// rejected before it can become a file name.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// Title → file stem: alphanumerics kept, runs of anything else collapsed to a
/// single hyphen, trimmed, and never empty.
fn safe_stem(title: &str) -> String {
    let mut stem = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            stem.push(ch);
        } else if !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        "canvas".to_string()
    } else {
        stem
    }
}

/// True when `id` / `title` failure modes stay inside the store; used by tests
/// and the import path (`library.rs`) to decide whether to mint a fresh id.
pub fn project_id_is_safe(id: &str) -> bool {
    is_safe_id(id)
}

fn read_text(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Serializes, ensures the parent directory, writes `path.tmp`, renames over
/// `path` (crash-safe on Windows: `rename` replaces atomically).
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let json = to_json(value)?;
    let Some(parent) = path.parent() else {
        return Err(StoreError::NoParent(path.to_path_buf()));
    };
    fs::create_dir_all(parent).map_err(|source| StoreError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|source| StoreError::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, path).map_err(|source| {
        // Best-effort cleanup; the original error is what the caller needs.
        let leftover = fs::remove_file(&tmp);
        if let Err(cleanup) = leftover {
            eprintln!("store: could not remove {}: {cleanup}", tmp.display());
        }
        StoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
