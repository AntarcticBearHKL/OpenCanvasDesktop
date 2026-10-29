//! The settings tabs beyond Providers: Appearance, Local models, Local storage,
//! Cost, Runtime logs and About. Pure builders (rows + summaries, all localized)
//! live at the top; the `App` methods perform the mutations;
//! `wire_config_tabs` registers the callbacks.
//!
//! Everything here is the desktop's real state: appearance/locale come from
//! `ui.json`, local models from a filesystem scan of `<data-root>\models`,
//! storage from a walk of the data directory, cost records from `costs.json`
//! (capped at `GENERATION_COST_RECORD_LIMIT`), and logs from `crates/ai`'s
//! `RuntimeLogBuffer`.

use std::cell::RefCell;
use std::rc::Rc;

use open_canvas_ai::config::{resolve_model_for_capability, resolve_model_request_config};
use open_canvas_ai::generation_cost::format_usd;
use open_canvas_ai::runtime_log::{RuntimeLogBuffer, RuntimeLogLevel};
use slint::ComponentHandle;

use crate::app::{with_app, App};
use crate::appearance::ThemeLook;
use crate::cost::CostLog;
use crate::i18n::{self, Locale};
use crate::local_models::{self, LocalModelsView};
use crate::log_view::{self, LogLine};
use crate::storage_scan::{self, StorageUsage};
use crate::{AppWindow, ConfigOptionRow, CostRecordRow, LocalModelRow, LogRow, StorageKindRow};

/// `~$0.04` for an estimate, `$0.04` for a looked-up price, `Unpriced` when the
/// model has no price row (`config-generation-cost.tsx` amount cell).
fn cost_amount(locale: Locale, record: &crate::cost::CostRecord) -> String {
    if !record.priced {
        return i18n::t(locale, "config.cost.unpriced").to_string();
    }
    if record.source == crate::cost::COST_SOURCE_ESTIMATE {
        format!("~{}", format_usd(record.usd))
    } else {
        format_usd(record.usd)
    }
}

fn cost_quantity(quantity: f64) -> String {
    if (quantity.fract()).abs() < f64::EPSILON {
        format!("{quantity:.0}")
    } else {
        format!("{quantity:.1}")
    }
}

/// A record count for the Cost tab's integer properties; saturates instead of
/// wrapping on a log far beyond `i32::MAX` entries.
fn count_i32(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Appearance tab: theme rows (light / dark / follow-system).
pub fn theme_options(locale: Locale, current: ThemeLook) -> Vec<ConfigOptionRow> {
    [ThemeLook::Light, ThemeLook::Dark, ThemeLook::FollowSystem]
        .into_iter()
        .map(|entry| ConfigOptionRow {
            label: match entry {
                ThemeLook::Light => i18n::t(locale, "config.theme.light"),
                ThemeLook::Dark => i18n::t(locale, "config.theme.dark"),
                ThemeLook::FollowSystem => i18n::t(locale, "config.theme.system"),
            }
            .into(),
            selected: entry == current,
            value: entry.as_str().into(),
        })
        .collect()
}

/// Local models tab: one row per descriptor with its real file state.
pub fn local_model_rows(locale: Locale, view: &LocalModelsView) -> Vec<LocalModelRow> {
    local_models::LOCAL_MODELS
        .iter()
        .map(|spec| {
            let status = view.models.iter().find(|model| model.id == spec.id);
            let installed = status.is_some_and(|model| model.installed);
            let size = status.filter(|model| model.installed).map_or_else(
                || i18n::t(locale, "config.local.size").to_string(),
                |model| storage_scan::human_bytes(model.bytes),
            );
            LocalModelRow {
                id: spec.id.into(),
                name: i18n::t(locale, spec.name_key).into(),
                description: i18n::t(locale, spec.desc_key).into(),
                size: size.into(),
                status: if installed {
                    i18n::t(locale, "config.local.installed").into()
                } else {
                    i18n::t(locale, "config.local.missing").into()
                },
                installed,
            }
        })
        .collect()
}

/// Local storage tab: one row per non-empty kind. `share` is the kind's file
/// count over the largest kind's, so the widest bar is always a full row and an
/// empty scan leaves every bar hidden.
#[expect(
    clippy::cast_precision_loss,
    reason = "display-only bar share; precision loss above 2^24 files is irrelevant"
)]
pub fn storage_rows(locale: Locale, usage: &StorageUsage) -> Vec<StorageKindRow> {
    let max_files = usage.kinds.iter().map(|kind| kind.files).max().unwrap_or(0);
    usage
        .kinds
        .iter()
        .map(|kind| StorageKindRow {
            id: kind.id.into(),
            label: i18n::t(locale, storage_kind_key(kind.id)).into(),
            files: i32::try_from(kind.files).unwrap_or(i32::MAX),
            detail: i18n::t(locale, storage_kind_detail_key(kind.id)).into(),
            size: storage_scan::human_bytes(kind.bytes).into(),
            share: if max_files == 0 {
                0.0
            } else {
                (kind.files as f32 / max_files as f32).clamp(0.0, 1.0)
            },
        })
        .collect()
}

/// `config.storage.kind.*` key for a scan bucket id.
pub fn storage_kind_key(id: &str) -> &'static str {
    match id {
        "library" => "config.storage.kind.library",
        "config" => "config.storage.kind.config",
        "costs" => "config.storage.kind.costs",
        "projects" => "config.storage.kind.projects",
        "images" => "config.storage.kind.images",
        "exports" => "config.storage.kind.exports",
        "plugins" => "config.storage.kind.plugins",
        "models" => "config.storage.kind.models",
        _ => "config.storage.kind.other",
    }
}

/// `config.storage.kind.*.detail` key: the muted one-line explanation under a
/// kind's label (the file count is its own column, so it does not repeat here).
pub fn storage_kind_detail_key(id: &str) -> &'static str {
    match id {
        "library" => "config.storage.kind.library.detail",
        "config" => "config.storage.kind.config.detail",
        "costs" => "config.storage.kind.costs.detail",
        "projects" => "config.storage.kind.projects.detail",
        "images" => "config.storage.kind.images.detail",
        "exports" => "config.storage.kind.exports.detail",
        "plugins" => "config.storage.kind.plugins.detail",
        "models" => "config.storage.kind.models.detail",
        _ => "config.storage.kind.other.detail",
    }
}

pub fn storage_summary(locale: Locale, usage: &StorageUsage) -> String {
    i18n::tf(
        locale,
        "config.storage.summary",
        &[
            ("total", &storage_scan::human_bytes(usage.total_bytes)),
            ("count", &usage.total_files.to_string()),
        ],
    )
}

/// Cost tab: newest-first records with the localized unit + amount.
/// Cost tab: the newest [`COST_TAB_ROWS`] records grouped by model (a header
/// row per model, first-seen order), newest-first inside each group.
pub const COST_TAB_ROWS: usize = 20;

pub fn cost_rows(locale: Locale, log: &CostLog) -> Vec<CostRecordRow> {
    let mut groups: Vec<(&str, Vec<&crate::cost::CostRecord>)> = Vec::new();
    for record in log.records.iter().take(COST_TAB_ROWS) {
        match groups.iter_mut().find(|(model, _)| *model == record.model) {
            Some((_, records)) => records.push(record),
            None => groups.push((record.model.as_str(), vec![record])),
        }
    }
    let mut rows = Vec::new();
    for (model, records) in groups {
        let count = records.len();
        let total = format_usd(
            records
                .iter()
                .filter(|record| record.priced)
                .map(|record| record.usd)
                .sum(),
        );
        let detail = i18n::tf(
            locale,
            if count == 1 {
                "config.cost.record"
            } else {
                "config.cost.records"
            },
            &[("count", &count.to_string())],
        );
        rows.push(CostRecordRow {
            model: model.into(),
            detail: detail.into(),
            amount: total.into(),
            source: "".into(),
            unpriced: false,
            heading: true,
        });
        for record in records {
            rows.push(CostRecordRow {
                model: record.model.as_str().into(),
                detail: format!(
                    "{} {} · {}",
                    cost_quantity(record.quantity),
                    i18n::cost_unit_label(locale, &record.unit),
                    log_view::format_time(record.at)
                )
                .into(),
                amount: cost_amount(locale, record).into(),
                source: record.source.as_str().into(),
                unpriced: !record.priced,
                heading: false,
            });
        }
    }
    rows
}

pub fn cost_summary(locale: Locale, log: &CostLog) -> String {
    // `sum()` over an empty priced set yields -0.0, which formats as $-0.00;
    // the card reads $0.00 instead.
    let total = log.total_usd();
    let total = format_usd(if total == 0.0 { 0.0 } else { total });
    let records = i18n::tf(
        locale,
        "config.cost.records",
        &[("count", &log.records.len().to_string())],
    );
    let unpriced = log.unpriced_count();
    if unpriced == 0 {
        format!("{total} · {records}")
    } else {
        let note = i18n::tf(
            locale,
            "config.cost.unpriced-note",
            &[("count", &unpriced.to_string())],
        );
        format!("{total} · {records} · {note}")
    }
}

/// Runtime logs tab: the filtered newest-first window.
pub fn log_rows_of(
    buffer: &RuntimeLogBuffer,
    filter: &str,
    category: &str,
    expanded: &[String],
) -> Vec<LogRow> {
    log_view::lines(buffer, filter, category)
        .into_iter()
        .map(|line| log_row(line, expanded))
        .collect()
}

fn log_row(line: LogLine, expanded: &[String]) -> LogRow {
    let is_expanded = expanded.iter().any(|key| key == &line.key);
    LogRow {
        key: line.key.into(),
        level: line.level.into(),
        scope: line.scope.into(),
        message: line.message.into(),
        time: line.time.into(),
        detail: line.detail.into(),
        expanded: is_expanded,
    }
}

/// The visible rows as plain text (the Copy / Download payload).
fn log_visible_text(app: &App) -> String {
    log_view::visible_text(&log_view::lines(
        &app.logs,
        app.logs_filter.as_str(),
        app.logs_category.as_str(),
    ))
}

/// Reveals `path` in the OS file manager. Windows-only (`explorer`); elsewhere
/// this reports unsupported so the notice is honest.
pub fn reveal_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::fs::create_dir_all(path)?;
        let child = std::process::Command::new("explorer")
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        child.map(|_child| ())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no file manager integration on this platform",
        ))
    }
}

impl App {
    /// Pushes every new tab's scalars and row models (called from
    /// `refresh_config_rows`, so it runs on open and on each mutation).
    pub(crate) fn sync_config_tabs(&mut self, ui: &AppWindow) {
        let locale = self.locale;
        self.appearance_theme
            .set_vec(theme_options(locale, self.theme_look));
        match self.config_tab {
            crate::config_ui::ConfigTab::LocalModels => {
                self.local_models = local_models::scan(&self.store.models_dir());
            }
            crate::config_ui::ConfigTab::LocalStorage => {
                self.storage = Some(storage_scan::scan(self.store.root()));
            }
            _ => {}
        }
        self.local_model_rows
            .set_vec(local_model_rows(locale, &self.local_models));
        self.storage_rows.set_vec(
            self.storage
                .as_ref()
                .map_or_else(Vec::new, |usage| storage_rows(locale, usage)),
        );
        self.cost_rows.set_vec(cost_rows(locale, &self.costs));
        self.log_rows.set_vec(log_rows_of(
            &self.logs,
            self.logs_filter.as_str(),
            self.logs_category.as_str(),
            &self.logs_expanded,
        ));
        self.logs_categories.set_vec(
            log_view::categories(&self.logs)
                .into_iter()
                .map(Into::into)
                .collect::<Vec<slint::SharedString>>(),
        );

        ui.set_config_theme(self.theme_look.as_str().into());
        ui.set_config_app_version(env!("CARGO_PKG_VERSION").into());
        ui.set_local_runtime_available(self.local_models.runtime_available);
        ui.set_local_models_dir(self.local_models.dir.display().to_string().into());
        ui.set_storage_summary(
            self.storage
                .as_ref()
                .map_or_else(String::new, |usage| storage_summary(locale, usage))
                .into(),
        );
        ui.set_storage_clear_armed(self.storage_clear_armed);
        ui.set_cost_summary(cost_summary(locale, &self.costs).into());
        ui.set_cost_generation_id(self.cost_generation_id.as_str().into());
        ui.set_cost_priced_count(count_i32(
            self.costs
                .records
                .len()
                .saturating_sub(self.costs.unpriced_count()),
        ));
        ui.set_logs_filter(self.logs_filter.as_str().into());
        ui.set_logs_category(self.logs_category.as_str().into());
    }

    /// Appearance tab: one callback for the option groups (theme).
    /// There is no language group: the product ships en-US only.
    pub fn config_option_pick(&mut self, ui: &AppWindow, kind: &str, value: &str) {
        if kind == "theme" {
            self.theme_pick(ui, value);
        }
    }

    pub fn config_local_refresh(&mut self, ui: &AppWindow) {
        self.local_models = local_models::scan(&self.store.models_dir());
        self.sync_config(ui);
    }

    pub fn config_local_reveal(&mut self, ui: &AppWindow) {
        let dir = self.store.models_dir();
        if let Err(error) = reveal_in_file_manager(&dir) {
            let message = format!("Could not open {}: {error}", dir.display());
            self.log(RuntimeLogLevel::Warn, "settings", message.clone(), None);
            self.set_notice(ui, message, true);
        }
    }

    pub fn config_storage_refresh(&mut self, ui: &AppWindow) {
        self.storage = Some(storage_scan::scan(self.store.root()));
        self.sync_config(ui);
    }

    pub fn config_storage_reveal(&mut self, ui: &AppWindow) {
        let root = self.store.root().to_path_buf();
        if let Err(error) = reveal_in_file_manager(&root) {
            let message = format!("Could not open {}: {error}", root.display());
            self.log(RuntimeLogLevel::Warn, "settings", message.clone(), None);
            self.set_notice(ui, message, true);
        }
    }

    /// Two-step clear: the first click arms, the second removes the export
    /// envelopes (never projects or settings).
    pub fn config_storage_clear(&mut self, ui: &AppWindow) {
        if !self.storage_clear_armed {
            self.storage_clear_armed = true;
            self.sync_config(ui);
            return;
        }
        self.storage_clear_armed = false;
        match storage_scan::clear_exports(self.store.root()) {
            Ok(count) => {
                let message = i18n::tf(
                    self.locale,
                    "config.storage.cleared",
                    &[("count", &count.to_string())],
                );
                self.log(RuntimeLogLevel::Info, "settings", message.clone(), None);
                self.set_notice(ui, message, false);
            }
            Err(error) => {
                let message = format!("Could not clear exports: {error}");
                self.log(RuntimeLogLevel::Error, "settings", message.clone(), None);
                self.set_notice(ui, message, true);
            }
        }
        self.storage = Some(storage_scan::scan(self.store.root()));
        self.sync_config(ui);
    }

    pub fn config_cost_clear(&mut self, ui: &AppWindow) {
        self.costs.clear();
        self.persist_costs(ui);
        self.sync_config(ui);
    }

    /// Keeps the Cost tab's lookup draft in the view; it needs no persistence.
    pub fn config_cost_id_edited(&mut self, ui: &AppWindow, value: &str) {
        self.cost_generation_id = value.to_string();
        ui.set_cost_generation_id(self.cost_generation_id.as_str().into());
    }

    /// Cost tab: asks the provider for a generation id's billed amount through
    /// `crates/ai`'s `fetch_generation_cost`; the result lands in `poll_ai`.
    pub fn config_cost_lookup(&mut self, ui: &AppWindow) {
        let generation_id = self.cost_generation_id.trim().to_string();
        if generation_id.is_empty() {
            self.set_notice(
                ui,
                "Paste a generation id before looking it up".to_string(),
                true,
            );
            return;
        }
        let Some(service) = self.ai.as_ref() else {
            self.set_notice(ui, "The AI runtime could not start".to_string(), true);
            return;
        };
        let capability = open_canvas_ai::config::ModelCapability::Image;
        let resolved = resolve_model_for_capability(&self.config, None, capability);
        let req = resolve_model_request_config(&self.config, &resolved);
        if req.api_key.trim().is_empty() {
            self.set_notice(
                ui,
                "Set a provider API key in Settings before generating".to_string(),
                true,
            );
            return;
        }
        service.spawn_cost_lookup(req, generation_id, capability);
        self.log(
            RuntimeLogLevel::Info,
            "cost",
            "Looking up the generation price",
            None,
        );
    }

    pub fn config_logs_filter(&mut self, ui: &AppWindow, filter: &str) {
        let filter = match filter {
            "info" => "info",
            "warn" => "warn",
            "error" => "error",
            _ => log_view::FILTER_ALL,
        };
        if self.logs_filter == filter {
            return;
        }
        self.logs_filter = filter.to_string();
        self.sync_config(ui);
    }

    /// Logs tab: the scope/category filter (the entry's `scope`; `all` clears).
    pub fn config_logs_category(&mut self, ui: &AppWindow, category: &str) {
        let category = if category.is_empty() {
            log_view::CATEGORY_ALL
        } else {
            category
        };
        if self.logs_category == category {
            return;
        }
        self.logs_category = category.to_string();
        self.sync_config(ui);
    }

    /// Logs tab: expands/collapses one row's `data` detail.
    pub fn config_logs_toggle(&mut self, ui: &AppWindow, key: &str) {
        if key.is_empty() {
            return;
        }
        if let Some(index) = self.logs_expanded.iter().position(|entry| entry == key) {
            self.logs_expanded.remove(index);
        } else {
            self.logs_expanded.push(key.to_string());
        }
        self.sync_config(ui);
    }

    /// Logs tab: copies the visible rows to the system clipboard.
    pub fn config_logs_copy(&mut self, ui: &AppWindow) {
        let text = log_visible_text(self);
        if text.is_empty() {
            self.set_notice(ui, "No visible log entries to copy".to_string(), true);
            return;
        }
        let copied = crate::system_clipboard::write_text(&text);
        let message = if copied {
            format!("Copied {} log entries", text.lines().count())
        } else {
            "Could not access the system clipboard".to_string()
        };
        self.set_notice(ui, message, !copied);
    }

    /// Logs tab: writes the visible rows to `<data-root>\exports\runtime-logs.txt`.
    pub fn config_logs_download(&mut self, ui: &AppWindow) {
        let text = log_visible_text(self);
        if text.is_empty() {
            self.set_notice(ui, "No visible log entries to download".to_string(), true);
            return;
        }
        let path = self.store.logs_export_path();
        let written = path
            .parent()
            .ok_or_else(|| "no export directory".to_string())
            .and_then(|parent| {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create {}: {error}", parent.display()))
            })
            .and_then(|()| {
                std::fs::write(&path, text.as_bytes())
                    .map_err(|error| format!("Could not write {}: {error}", path.display()))
            });
        match written {
            Ok(()) => self.set_notice(
                ui,
                format!("Saved runtime logs to {}", path.display()),
                false,
            ),
            Err(message) => self.set_notice(ui, message, true),
        }
    }

    pub fn config_logs_clear(&mut self, ui: &AppWindow) {
        self.logs.clear();
        self.logs_expanded.clear();
        self.logs_version = self.logs_version.wrapping_add(1);
        drop(self.store.save_logs(&self.logs));
        self.sync_config(ui);
    }

    /// Rebuilds the log rows when new entries arrived while the modal shows the
    /// Runtime logs tab (called from the 60 ms tick; the buffer never persists).
    pub fn poll_logs(&mut self, ui: &AppWindow) {
        if self.logs_version == self.logs_shown_version {
            return;
        }
        if !(self.config_open && self.config_tab == crate::config_ui::ConfigTab::Logs) {
            return;
        }
        self.logs_shown_version = self.logs_version;
        self.log_rows.set_vec(log_rows_of(
            &self.logs,
            self.logs_filter.as_str(),
            self.logs_category.as_str(),
            &self.logs_expanded,
        ));
        self.logs_categories.set_vec(
            log_view::categories(&self.logs)
                .into_iter()
                .map(Into::into)
                .collect::<Vec<slint::SharedString>>(),
        );
        ui.set_logs_filter(self.logs_filter.as_str().into());
        ui.set_logs_category(self.logs_category.as_str().into());
    }

    /// Writes `costs.json`; a failure only means the record history lags.
    pub(crate) fn persist_costs(&mut self, ui: &AppWindow) {
        if let Err(error) = self.store.save_costs(&self.costs) {
            let message = format!("Could not save cost records: {error}");
            self.log(RuntimeLogLevel::Warn, "cost", message.clone(), None);
            self.set_notice(ui, message, true);
        }
    }
}

/// Registers the settings-tab callbacks (appearance, local models, local
/// storage, cost, logs).
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the wave's callbacks; splitting adds indirection, not clarity"
)]
pub fn wire_config_tabs(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_local_refresh(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_local_refresh(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_local_reveal(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_local_reveal(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_storage_refresh(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_storage_refresh(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_storage_reveal(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_storage_reveal(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_storage_clear(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_storage_clear(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_cost_clear(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_cost_clear(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_cost_id_edited(move |value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_cost_id_edited(&ui, value.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_cost_lookup(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_cost_lookup(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_filter(move |filter| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_logs_filter(&ui, filter.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_category(move |category| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.config_logs_category(&ui, category.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_toggle(move |key| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_logs_toggle(&ui, key.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_copy(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_logs_copy(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_download(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_logs_download(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_logs_clear(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.config_logs_clear(&ui));
        });
    }
}

#[cfg(test)]
#[path = "config_tab_tests.rs"]
mod tests;
