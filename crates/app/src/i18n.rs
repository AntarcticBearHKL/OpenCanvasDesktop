//! Runtime localization: a [`Locale`] type, the en-US string table for the
//! shell chrome (top bar + canvas dock) and the settings modal, and a `t(key)`
//! accessor.
//!
//! The product ships **en-US only**. One table means there is nothing to fall
//! back from and nothing to keep in sync; a persisted language choice from an
//! older build is ignored at startup. The Rust-only keys at the end of the
//! table cover strings this port owns (storage-kind labels, cost units, local
//! model descriptors).

use crate::AppWindow;
use slint::ComponentHandle;

/// `AppLocale`. The product ships **en-US only**: one variant, so a persisted
/// `zh-CN` (or any other id) cannot select another table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    EnUs,
}

impl Locale {
    /// The only shipped locale id, written into `ui.json`.
    pub const fn id() -> &'static str {
        "en-US"
    }
}

/// Replaces every `{name}` marker in `template` (the web's `{{name}}`
/// interpolation, one brace here to keep Slint/Rust literals unambiguous).
pub fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut text = template.to_string();
    for (name, value) in pairs {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// The en-US table. Also the fallback table: a key missing from another locale
/// resolves here before it degrades to the key itself.
const EN: &[(&str, &str)] = &[
    // Home screen (library.slint): project sidebar + canvas table copy. The
    // model is Projects (level 1) holding Canvases.
    ("library.eyebrow", "LIBRARY"),
    ("library.projects-label", "PROJECTS"),
    ("library.new-project", "New Project"),
    ("library.new-canvas", "New Canvas"),
    ("library.no-project", "No project"),
    ("library.column.name", "Name"),
    ("library.column.updated", "Last edited"),
    ("library.column.actions", "Actions"),
    ("library.empty.project", "No canvases yet"),
    (
        "library.empty.project-hint",
        "Create a canvas and it opens in the editor.",
    ),
    ("library.empty.no-projects", "No projects yet"),
    (
        "library.empty.no-projects-hint",
        "Create a project to hold your canvases.",
    ),
    // Canvas dock + zoom popover.
    ("dock.shortcuts", "Keyboard shortcuts"),
    // Shortcut reference values (keys stay literal).
    ("shortcut.toggle-tool", "Temporarily switch Select / Move"),
    ("shortcut.wheel", "Mouse wheel"),
    ("shortcut.zoom", "Zoom canvas"),
    ("shortcut.zoom-slider", "Zoom slider"),
    ("shortcut.precise-zoom", "Adjust zoom precisely"),
    ("shortcut.box-select", "Box-select nodes"),
    ("shortcut.add-selection", "Add nodes to the selection"),
    ("shortcut.select-all", "Select all nodes"),
    ("shortcut.copy", "Copy selected nodes"),
    ("shortcut.cut", "Cut selected nodes"),
    ("shortcut.paste", "Paste nodes or clipboard payload"),
    ("shortcut.undo", "Undo"),
    ("shortcut.redo", "Redo"),
    ("shortcut.delete", "Delete selection or the selected wire"),
    ("shortcut.cut-wires", "Cut wires crossed by the stroke"),
    ("shortcut.reset-view", "Reset view"),
    ("shortcut.fit", "Fit to content"),
    ("shortcut.escape", "Clear selection and close overlays"),
    // Settings modal shell.
    ("config.title", "Settings"),
    ("config.tabs.providers", "Providers"),
    ("config.tabs.appearance", "Appearance"),
    ("config.tabs.local-models", "Local models"),
    ("config.tabs.local-storage", "Local storage"),
    ("config.tabs.cost", "Cost"),
    ("config.tabs.logs", "Runtime logs"),
    ("config.tabs.about", "About"),
    // Plugins (About tab entry into the plugin manager).
    ("config.plugins.title", "Plugins"),
    (
        "config.plugins.hint",
        "Install, enable and remove node plugins. The plugin manager opens over \
         this window.",
    ),
    ("config.plugins.open", "Open plugin manager"),
    // Providers tab.
    ("config.providers.title", "Providers"),
    ("config.providers.name", "Name"),
    ("config.providers.base-url", "Base URL"),
    ("config.providers.api-key", "API key"),
    ("config.providers.no-models", "No models yet"),
    ("config.providers.model-count", "{count} models"),
    // Appearance.
    ("config.appearance.theme", "Theme"),
    ("config.appearance.window", "Window"),
    ("config.appearance.opacity", "Opacity"),
    (
        "config.appearance.opacity-hint",
        "100 is nearly opaque; 0 is almost clear. The canvas and panels stay \
         see-through at every level.",
    ),
    ("config.appearance.blur", "Blur"),
    ("config.appearance.blur-off", "Off"),
    (
        "config.appearance.blur-hint",
        "0 turns the desktop backdrop off and paints the window opaque. 1-34 \
         uses Mica, 35-69 Mica Alt, 70-100 acrylic (the strongest frost). \
         Windows 10 always uses the acrylic fallback.",
    ),
    ("config.theme.light", "Light"),
    ("config.theme.dark", "Dark"),
    ("config.theme.system", "Follow system"),
    // Local models.
    ("config.local.title", "Local Models"),
    (
        "config.local.page-hint",
        "Optional models that run on this device instead of in the cloud.",
    ),
    ("config.local.eyebrow-runtime", "RUNTIME"),
    ("config.local.runtime", "On-device runtime"),
    ("config.local.runtime-ready", "Runtime installed"),
    ("config.local.runtime-missing", "Runtime not installed"),
    (
        "config.local.runtime-hint",
        "Runs background removal and segmentation locally. Model files are never \
         downloaded for you.",
    ),
    ("config.local.refresh", "Refresh"),
    ("config.local.reveal", "Reveal"),
    ("config.local.dir", "Models folder"),
    ("config.local.eyebrow-models", "MODELS"),
    ("config.local.models-title", "Available models"),
    ("config.local.empty", "No local models"),
    (
        "config.local.empty-hint",
        "Add model files to the models folder, then refresh.",
    ),
    ("config.local.bg-name", "Background removal (ISNet)"),
    (
        "config.local.bg-desc",
        "Removes an image background locally with ISNet segmentation.",
    ),
    ("config.local.sam-name", "Segmentation (MobileSAM)"),
    (
        "config.local.sam-desc",
        "Segments an image locally from prompt points with MobileSAM.",
    ),
    ("config.local.size", "About 42 MB"),
    ("config.local.installed", "Installed"),
    ("config.local.missing", "Not installed"),
    // Local storage.
    ("config.storage.title", "Local Storage"),
    (
        "config.storage.hint",
        "What this app keeps on this device, and how much room it takes.",
    ),
    ("config.storage.eyebrow-summary", "TOTAL"),
    ("config.storage.total", "App data"),
    ("config.storage.eyebrow-breakdown", "BREAKDOWN"),
    ("config.storage.by-kind", "By kind"),
    ("config.storage.eyebrow-danger", "DANGER"),
    ("config.storage.clear-title", "Clear local data"),
    (
        "config.storage.clear-hint",
        "Deletes the exported files in the app data folder. Projects, models and \
         settings are kept.",
    ),
    ("config.storage.refresh", "Refresh"),
    ("config.storage.reveal", "Reveal"),
    ("config.storage.clear", "Clear exports"),
    ("config.storage.clear-armed", "Confirm clear exports?"),
    ("config.storage.empty", "No data directory yet"),
    (
        "config.storage.empty-hint",
        "Storage appears here after the app writes its first file.",
    ),
    ("config.storage.kind", "kind"),
    ("config.storage.kinds", "kinds"),
    ("config.storage.file", "file"),
    ("config.storage.files", "files"),
    ("config.storage.summary", "{total} in {count} files"),
    ("config.storage.cleared", "Cleared {count} export files"),
    ("config.storage.kind.library", "Library index"),
    ("config.storage.kind.library.detail", "Project index"),
    ("config.storage.kind.config", "App settings"),
    (
        "config.storage.kind.config.detail",
        "Preferences and UI state",
    ),
    ("config.storage.kind.costs", "Cost records"),
    (
        "config.storage.kind.costs.detail",
        "Generation cost history",
    ),
    ("config.storage.kind.projects", "Canvas projects"),
    (
        "config.storage.kind.projects.detail",
        "Saved canvas documents",
    ),
    ("config.storage.kind.images", "Image projects"),
    ("config.storage.kind.images.detail", "Saved image documents"),
    ("config.storage.kind.exports", "Exports"),
    ("config.storage.kind.exports.detail", "Files you exported"),
    ("config.storage.kind.plugins", "Plugins"),
    (
        "config.storage.kind.plugins.detail",
        "Installed plugin files",
    ),
    ("config.storage.kind.models", "Local models"),
    ("config.storage.kind.models.detail", "On-device model files"),
    ("config.storage.kind.other", "Other files"),
    (
        "config.storage.kind.other.detail",
        "Unrecognized data files",
    ),
    // Cost.
    ("config.cost.title", "Cost"),
    (
        "config.cost.page-hint",
        "Every generation this app has priced, recorded on this device.",
    ),
    ("config.cost.eyebrow-total", "TOTAL"),
    ("config.cost.total", "Total spend"),
    ("config.cost.priced-record", "priced record"),
    ("config.cost.priced-records", "priced records"),
    ("config.cost.record", "{count} record"),
    ("config.cost.records", "{count} records"),
    ("config.cost.unpriced-note", "{count} unpriced"),
    ("config.cost.eyebrow-ledger", "LEDGER"),
    ("config.cost.recent", "Recorded generations"),
    ("config.cost.empty", "No cost records yet"),
    (
        "config.cost.empty-hint",
        "Generate something, or look up a generation id, to record its cost here.",
    ),
    ("config.cost.clear", "Clear history"),
    ("config.cost.unpriced", "Unpriced"),
    ("config.cost.eyebrow-lookup", "LOOKUP"),
    ("config.cost.lookup", "Look up a generation id"),
    (
        "config.cost.lookup-hint",
        "Resolves a generation id to the cost the provider recorded for it. The result \
         is appended to the ledger.",
    ),
    ("config.cost.lookup-label", "Generation id"),
    ("config.cost.lookup-field", "gen_..."),
    ("config.cost.lookup-action", "Look up"),
    ("config.cost.lookup-ok", "Priced at {usd}"),
    ("config.cost.lookup-failed", "Price lookup failed: {error}"),
    ("config.cost.unit.image", "image(s)"),
    ("config.cost.unit.video-second", "second(s)"),
    ("config.cost.unit.call", "call(s)"),
    // Runtime logs.
    ("config.logs.title", "Runtime Logs"),
    (
        "config.logs.page-hint",
        "What this app has done this session, newest first.",
    ),
    ("config.logs.eyebrow-controls", "FILTERS"),
    ("config.logs.controls-title", "Filter and export"),
    ("config.logs.level", "Level"),
    ("config.logs.category", "Category"),
    ("config.logs.eyebrow-stream", "STREAM"),
    ("config.logs.stream-title", "Log stream"),
    ("config.logs.empty", "No runtime logs yet"),
    (
        "config.logs.empty-hint",
        "Runtime activity appears here as it happens.",
    ),
    ("config.logs.empty-filtered", "No logs match this filter"),
    (
        "config.logs.empty-filtered-hint",
        "Change the level or category to see more entries.",
    ),
    ("config.logs.clear", "Clear"),
    ("config.logs.all", "All"),
    ("config.logs.all-categories", "All categories"),
    ("config.logs.copy", "Copy"),
    ("config.logs.download", "Download"),
    ("config.logs.info", "Info"),
    ("config.logs.warn", "Warning"),
    ("config.logs.error", "Error"),
    // About.
    ("config.about.version", "Version"),
];

/// True for a character that must never appear in shipped copy: a CJK
/// ideograph (a stray Chinese literal) or the U+FFFD replacement character (a
/// mojibake decode failure). Shared by the table test and the copy scan.
#[cfg(test)]
pub fn is_cjk_or_mojibake(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{FF00}'..='\u{FFEF}'
            | '\u{FFFD}'
    )
}

/// The translated string for `key`. There is one table: the product ships
/// en-US only, so the `Locale` argument is accepted for the call sites but can
/// never select another language.
pub fn t(_locale: Locale, key: &'static str) -> &'static str {
    lookup(EN, key)
}

/// The fallback chain of [`t`]: active table, then en-US, then the key.
pub(crate) fn lookup(active: &[(&'static str, &'static str)], key: &'static str) -> &'static str {
    if let Some((_, value)) = active.iter().find(|(name, _)| *name == key) {
        return value;
    }
    if let Some((_, value)) = EN.iter().find(|(name, _)| *name == key) {
        return value;
    }
    key
}

/// `t` plus `{name}` substitution (see [`fill`]).
pub fn tf(locale: Locale, key: &'static str, pairs: &[(&str, &str)]) -> String {
    fill(t(locale, key), pairs)
}

/// `"{count} models"` for a provider card header.
pub fn model_count_label(locale: Locale, count: usize) -> String {
    tf(
        locale,
        "config.providers.model-count",
        &[("count", &count.to_string())],
    )
}

/// Unit word of a cost record (`image(s)`, `second(s)`, ...).
pub fn cost_unit_label(locale: Locale, unit: &str) -> &'static str {
    match unit {
        "video-second" => t(locale, "config.cost.unit.video-second"),
        "call" => t(locale, "config.cost.unit.call"),
        _ => t(locale, "config.cost.unit.image"),
    }
}

/// Pushes the active locale's shell + settings strings into the Slint global.
pub fn push_locale(ui: &AppWindow, locale: Locale) {
    let i = ui.global::<crate::I18n>();
    i.set_locale(Locale::id().into());
    push_chrome(&i, locale);
    push_config(&i, locale);
}

fn push_chrome(i: &crate::I18n, locale: Locale) {
    i.set_library_eyebrow(t(locale, "library.eyebrow").into());
    i.set_library_projects_label(t(locale, "library.projects-label").into());
    i.set_library_new_project(t(locale, "library.new-project").into());
    i.set_library_new_canvas(t(locale, "library.new-canvas").into());
    i.set_library_no_project(t(locale, "library.no-project").into());
    i.set_library_name_column(t(locale, "library.column.name").into());
    i.set_library_updated_column(t(locale, "library.column.updated").into());
    i.set_library_actions_column(t(locale, "library.column.actions").into());
    i.set_library_empty_project(t(locale, "library.empty.project").into());
    i.set_library_empty_project_hint(t(locale, "library.empty.project-hint").into());
    i.set_library_empty_no_projects(t(locale, "library.empty.no-projects").into());
    i.set_library_empty_no_projects_hint(t(locale, "library.empty.no-projects-hint").into());
    i.set_dock_shortcuts(t(locale, "dock.shortcuts").into());
    i.set_shortcut_toggle_tool(t(locale, "shortcut.toggle-tool").into());
    i.set_shortcut_wheel(t(locale, "shortcut.wheel").into());
    i.set_shortcut_zoom_slider(t(locale, "shortcut.zoom-slider").into());
    i.set_shortcut_box_select(t(locale, "shortcut.box-select").into());
    i.set_shortcut_add_selection(t(locale, "shortcut.add-selection").into());
    i.set_shortcut_select_all(t(locale, "shortcut.select-all").into());
    i.set_shortcut_copy(t(locale, "shortcut.copy").into());
    i.set_shortcut_cut(t(locale, "shortcut.cut").into());
    i.set_shortcut_paste(t(locale, "shortcut.paste").into());
    i.set_shortcut_undo(t(locale, "shortcut.undo").into());
    i.set_shortcut_redo(t(locale, "shortcut.redo").into());
    i.set_shortcut_delete(t(locale, "shortcut.delete").into());
    i.set_shortcut_cut_wires(t(locale, "shortcut.cut-wires").into());
    i.set_shortcut_reset_view(t(locale, "shortcut.reset-view").into());
    i.set_shortcut_fit(t(locale, "shortcut.fit").into());
    i.set_shortcut_escape(t(locale, "shortcut.escape").into());
}

fn push_config(i: &crate::I18n, locale: Locale) {
    i.set_config_title(t(locale, "config.title").into());
    i.set_config_tab_providers(t(locale, "config.tabs.providers").into());
    i.set_config_tab_appearance(t(locale, "config.tabs.appearance").into());
    i.set_config_tab_local_models(t(locale, "config.tabs.local-models").into());
    i.set_config_tab_local_storage(t(locale, "config.tabs.local-storage").into());
    i.set_config_tab_cost(t(locale, "config.tabs.cost").into());
    i.set_config_tab_logs(t(locale, "config.tabs.logs").into());
    i.set_config_tab_about(t(locale, "config.tabs.about").into());
    i.set_config_plugins_title(t(locale, "config.plugins.title").into());
    i.set_config_plugins_hint(t(locale, "config.plugins.hint").into());
    i.set_config_plugins_open(t(locale, "config.plugins.open").into());
    i.set_config_providers_title(t(locale, "config.providers.title").into());
    i.set_config_providers_name(t(locale, "config.providers.name").into());
    i.set_config_providers_base_url(t(locale, "config.providers.base-url").into());
    i.set_config_providers_api_key(t(locale, "config.providers.api-key").into());
    i.set_config_providers_no_models(t(locale, "config.providers.no-models").into());
    i.set_config_appearance_theme(t(locale, "config.appearance.theme").into());
    i.set_config_appearance_window(t(locale, "config.appearance.window").into());
    i.set_config_appearance_opacity(t(locale, "config.appearance.opacity").into());
    i.set_config_appearance_opacity_hint(t(locale, "config.appearance.opacity-hint").into());
    i.set_config_appearance_blur(t(locale, "config.appearance.blur").into());
    i.set_config_appearance_blur_off(t(locale, "config.appearance.blur-off").into());
    i.set_config_appearance_blur_hint(t(locale, "config.appearance.blur-hint").into());
    i.set_config_theme_light(t(locale, "config.theme.light").into());
    i.set_config_theme_dark(t(locale, "config.theme.dark").into());
    i.set_config_theme_system(t(locale, "config.theme.system").into());
    i.set_config_local_title(t(locale, "config.local.title").into());
    i.set_config_local_page_hint(t(locale, "config.local.page-hint").into());
    i.set_config_local_eyebrow_runtime(t(locale, "config.local.eyebrow-runtime").into());
    i.set_config_local_runtime(t(locale, "config.local.runtime").into());
    i.set_config_local_runtime_ready(t(locale, "config.local.runtime-ready").into());
    i.set_config_local_runtime_missing(t(locale, "config.local.runtime-missing").into());
    i.set_config_local_runtime_hint(t(locale, "config.local.runtime-hint").into());
    i.set_config_local_refresh(t(locale, "config.local.refresh").into());
    i.set_config_local_reveal(t(locale, "config.local.reveal").into());
    i.set_config_local_dir(t(locale, "config.local.dir").into());
    i.set_config_local_eyebrow_models(t(locale, "config.local.eyebrow-models").into());
    i.set_config_local_models_title(t(locale, "config.local.models-title").into());
    i.set_config_local_empty(t(locale, "config.local.empty").into());
    i.set_config_local_empty_hint(t(locale, "config.local.empty-hint").into());
    i.set_config_local_installed(t(locale, "config.local.installed").into());
    i.set_config_local_missing(t(locale, "config.local.missing").into());
    i.set_config_storage_title(t(locale, "config.storage.title").into());
    i.set_config_storage_hint(t(locale, "config.storage.hint").into());
    i.set_config_storage_eyebrow_summary(t(locale, "config.storage.eyebrow-summary").into());
    i.set_config_storage_total(t(locale, "config.storage.total").into());
    i.set_config_storage_eyebrow_breakdown(t(locale, "config.storage.eyebrow-breakdown").into());
    i.set_config_storage_by_kind(t(locale, "config.storage.by-kind").into());
    i.set_config_storage_eyebrow_danger(t(locale, "config.storage.eyebrow-danger").into());
    i.set_config_storage_clear_title(t(locale, "config.storage.clear-title").into());
    i.set_config_storage_clear_hint(t(locale, "config.storage.clear-hint").into());
    i.set_config_storage_refresh(t(locale, "config.storage.refresh").into());
    i.set_config_storage_reveal(t(locale, "config.storage.reveal").into());
    i.set_config_storage_clear(t(locale, "config.storage.clear").into());
    i.set_config_storage_clear_armed(t(locale, "config.storage.clear-armed").into());
    i.set_config_storage_empty(t(locale, "config.storage.empty").into());
    i.set_config_storage_empty_hint(t(locale, "config.storage.empty-hint").into());
    i.set_config_storage_kind(t(locale, "config.storage.kind").into());
    i.set_config_storage_kinds(t(locale, "config.storage.kinds").into());
    i.set_config_storage_file(t(locale, "config.storage.file").into());
    i.set_config_storage_files(t(locale, "config.storage.files").into());
    i.set_config_cost_title(t(locale, "config.cost.title").into());
    i.set_config_cost_page_hint(t(locale, "config.cost.page-hint").into());
    i.set_config_cost_eyebrow_total(t(locale, "config.cost.eyebrow-total").into());
    i.set_config_cost_total(t(locale, "config.cost.total").into());
    i.set_config_cost_priced_record(t(locale, "config.cost.priced-record").into());
    i.set_config_cost_priced_records(t(locale, "config.cost.priced-records").into());
    i.set_config_cost_eyebrow_ledger(t(locale, "config.cost.eyebrow-ledger").into());
    i.set_config_cost_recent(t(locale, "config.cost.recent").into());
    i.set_config_cost_empty(t(locale, "config.cost.empty").into());
    i.set_config_cost_empty_hint(t(locale, "config.cost.empty-hint").into());
    i.set_config_cost_clear(t(locale, "config.cost.clear").into());
    i.set_config_cost_unpriced(t(locale, "config.cost.unpriced").into());
    i.set_config_cost_eyebrow_lookup(t(locale, "config.cost.eyebrow-lookup").into());
    i.set_config_cost_lookup(t(locale, "config.cost.lookup").into());
    i.set_config_cost_lookup_hint(t(locale, "config.cost.lookup-hint").into());
    i.set_config_cost_lookup_label(t(locale, "config.cost.lookup-label").into());
    i.set_config_cost_lookup_field(t(locale, "config.cost.lookup-field").into());
    i.set_config_cost_lookup_action(t(locale, "config.cost.lookup-action").into());
    i.set_config_logs_title(t(locale, "config.logs.title").into());
    i.set_config_logs_page_hint(t(locale, "config.logs.page-hint").into());
    i.set_config_logs_eyebrow_controls(t(locale, "config.logs.eyebrow-controls").into());
    i.set_config_logs_controls_title(t(locale, "config.logs.controls-title").into());
    i.set_config_logs_level(t(locale, "config.logs.level").into());
    i.set_config_logs_category(t(locale, "config.logs.category").into());
    i.set_config_logs_eyebrow_stream(t(locale, "config.logs.eyebrow-stream").into());
    i.set_config_logs_stream_title(t(locale, "config.logs.stream-title").into());
    i.set_config_logs_empty(t(locale, "config.logs.empty").into());
    i.set_config_logs_empty_hint(t(locale, "config.logs.empty-hint").into());
    i.set_config_logs_empty_filtered(t(locale, "config.logs.empty-filtered").into());
    i.set_config_logs_empty_filtered_hint(t(locale, "config.logs.empty-filtered-hint").into());
    i.set_config_logs_clear(t(locale, "config.logs.clear").into());
    i.set_config_logs_all(t(locale, "config.logs.all").into());
    i.set_config_logs_all_categories(t(locale, "config.logs.all-categories").into());
    i.set_config_logs_copy(t(locale, "config.logs.copy").into());
    i.set_config_logs_download(t(locale, "config.logs.download").into());
    i.set_config_logs_info(t(locale, "config.logs.info").into());
    i.set_config_logs_warn(t(locale, "config.logs.warn").into());
    i.set_config_logs_error(t(locale, "config.logs.error").into());
    i.set_config_about_version(t(locale, "config.about.version").into());
}

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;
