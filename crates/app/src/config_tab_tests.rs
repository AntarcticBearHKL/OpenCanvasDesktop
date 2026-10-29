//! Settings-tab tests: per-tab state round trips through the real `App`, the
//! persisted files and the pushed Slint properties 鈥?providers, appearance,
//! local models, local storage, cost, logs and about.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::error::Error;
use std::rc::Rc;

use open_canvas_ai::generation_cost::GenerationCostUnit;
use slint::{ComponentHandle, Model as _};

use crate::app::App;
use crate::config_ui::ConfigTab;
use crate::store::{Store, THEME_DARK, THEME_LIGHT};
use crate::AppWindow;

use super::*;
use crate::test_support::differing_pixels;

fn tab_ids() -> [&'static str; 7] {
    [
        "providers",
        "appearance",
        "local-models",
        "local-storage",
        "cost",
        "logs",
        "about",
    ]
}

/// Renders one settings tab headlessly (software backend, no GUI): the real
/// window and app with the modal open on `tab`.
fn render_config_tab(
    store: Store,
    tab: ConfigTab,
    setup: impl FnOnce(&AppWindow, &Rc<RefCell<App>>),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn Error>> {
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = crate::app::View::Editor;
        app.sync_all(&ui);
        app.config_tab = tab;
        app.config_open = true;
        app.sync_config(&ui);
    });
    setup(&ui, &app);
    ui.set_view("editor".into());
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

fn distinct_colors(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> usize {
    let mut seen = BTreeSet::new();
    for pixel in buffer.as_slice() {
        seen.insert((pixel.r, pixel.g, pixel.b));
        if seen.len() > 64 {
            break;
        }
    }
    seen.len()
}

fn assert_tab_paints(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, what: &str) {
    assert!(buffer.width() > 0 && buffer.height() > 0, "{what} is empty");
    assert!(
        distinct_colors(buffer) > 8,
        "{what} paints its cards, not one flat surface"
    );
}

#[test]
fn every_tab_id_round_trips_through_the_enum() {
    let expected = [
        ConfigTab::Providers,
        ConfigTab::Appearance,
        ConfigTab::LocalModels,
        ConfigTab::LocalStorage,
        ConfigTab::Cost,
        ConfigTab::Logs,
        ConfigTab::About,
    ];
    for (id, tab) in tab_ids().into_iter().zip(expected) {
        assert_eq!(ConfigTab::from_id(id), tab, "{id}");
        assert_eq!(tab.as_str(), id, "{id}");
    }
    // Unknown and retired ids fall back to the first tab instead of panicking.
    for id in ["unknown", "", "models", "generation", "agent"] {
        assert_eq!(ConfigTab::from_id(id), ConfigTab::Providers, "{id}");
    }
}

#[test]
fn opening_each_tab_pushes_the_tab_id_and_rows() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    for id in tab_ids() {
        crate::app::with_app(&app, |app| {
            app.config_tab = ConfigTab::from_id(id);
            app.config_open = true;
            app.sync_config(&ui);
        });
        assert_eq!(ui.get_config_tab(), id);
        assert!(ui.get_config_open());
    }
    Ok(())
}

#[test]
fn appearance_tab_options_track_the_active_choices() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::Appearance;
        app.config_open = true;
        app.sync_config(&ui);
    });

    let theme = ui.get_appearance_theme();
    assert_eq!(theme.row_count(), 3, "light / dark / follow-system");
    let selected_light = theme.row_data(0).is_some_and(|row| row.selected);
    assert!(selected_light, "light is the default and selected");
    // There is no language option group any more: the product is en-US only.
    assert_eq!(ui.get_config_theme(), THEME_LIGHT);

    crate::app::with_app(&app, |app| app.config_option_pick(&ui, "theme", "dark"));
    assert_eq!(ui.get_config_theme(), THEME_DARK);
    assert!(ui.global::<crate::Theme>().get_dark());
    Ok(())
}

#[test]
fn local_models_tab_reports_the_real_scan_state() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::LocalModels;
        app.config_open = true;
        app.sync_config(&ui);
    });
    assert!(
        !ui.get_local_runtime_available(),
        "no inference runtime ships"
    );

    // One row per descriptor, each with the copy the row renders.
    let rows = ui.get_local_models();
    assert_eq!(
        rows.row_count(),
        crate::local_models::LOCAL_MODELS.len(),
        "one row per known model"
    );
    for index in 0..rows.row_count() {
        let row = rows.row_data(index).ok_or("local model row")?;
        assert!(!row.name.is_empty(), "a model row names the model");
        assert!(
            !row.description.is_empty(),
            "a model row explains what it does"
        );
        assert!(!row.size.is_empty(), "a model row shows its size");
    }
    assert_eq!(
        rows.iter().filter(|row| row.installed).count(),
        0,
        "empty models folder means not installed"
    );

    // The runtime card's status line comes from the two labels, and the chip
    // labels exist for the installed flag (the scan's `status` repeats them).
    let i18n = ui.global::<crate::I18n>();
    assert_eq!(i18n.get_config_local_runtime_ready(), "Runtime installed");
    assert_eq!(
        i18n.get_config_local_runtime_missing(),
        "Runtime not installed"
    );
    assert_eq!(i18n.get_config_local_installed(), "Installed");
    assert_eq!(i18n.get_config_local_missing(), "Not installed");
    let tab = include_str!("../ui/components/settings/local-models-tab.slint");
    assert!(
        tab.contains("config-local-runtime-ready") && tab.contains("config-local-runtime-missing"),
        "the runtime status is a bound label, not a bare word"
    );
    assert!(
        tab.contains("for model[index] in root.local-models"),
        "the models card repeats one row per model"
    );

    let dir_text = ui.get_local_models_dir();
    assert!(
        dir_text.contains("models"),
        "the tab shows the real models folder ({dir_text})"
    );

    // Drop the ISNet file where the manager looks, rescan, and the row flips.
    let models_dir = store.models_dir();
    std::fs::create_dir_all(&models_dir)?;
    std::fs::write(models_dir.join("isnet.onnx"), vec![0_u8; 32])?;
    crate::app::with_app(&app, |app| app.config_local_refresh(&ui));
    let rows = ui.get_local_models();
    let first = rows.row_data(0).ok_or("background-removal row")?;
    assert!(first.installed, "the file makes the model installed");
    assert_eq!(first.size, "32 B", "the row shows the real file size");
    assert_eq!(
        first.status,
        i18n.get_config_local_installed(),
        "the scan status and the chip agree, so no extra line is needed"
    );

    // The redesigned tab renders: header, runtime card, models card.
    let buffer = render_config_tab(store, ConfigTab::LocalModels, |_, _| {})?;
    assert_tab_paints(&buffer, "the Local Models tab");
    Ok(())
}

#[test]
fn local_storage_tab_aggregates_and_clears_exports() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    std::fs::write(store.index_path(), "{ }")?;
    let exports = crate::storage_scan::exports_dir(store.root());
    std::fs::create_dir_all(&exports)?;
    std::fs::write(exports.join("one.json"), vec![0_u8; 100])?;
    std::fs::write(exports.join("two.json"), vec![0_u8; 100])?;

    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::LocalStorage;
        app.config_open = true;
        app.sync_config(&ui);
    });
    let rows = ui.get_storage_rows();
    assert_eq!(rows.row_count(), 2, "library.json plus the exports folder");
    for index in 0..rows.row_count() {
        let row = rows.row_data(index).ok_or("storage kind row")?;
        assert!(
            (0.0..=1.0).contains(&row.share),
            "the bar share stays in 0..=1 ({})",
            row.share
        );
        assert!(!row.detail.is_empty(), "a kind row explains the kind");
    }
    let max_files = rows.iter().map(|row| row.files).max().unwrap_or(0);
    for row in rows.iter().filter(|row| row.files == max_files) {
        assert_eq!(row.share, 1.0, "the largest kind is a full bar");
    }
    let exports_row = rows
        .iter()
        .find(|row| row.id == "exports")
        .ok_or("exports row")?;
    assert_eq!(exports_row.files, 2);
    assert_eq!(exports_row.size, "200 B");
    assert_eq!(exports_row.detail, "Files you exported");
    assert_eq!(exports_row.share, 1.0, "two files is the widest kind here");
    // `library.json` (3 bytes) plus the two exports sum into the header line.
    assert!(
        ui.get_storage_summary().contains("203 B"),
        "the summary totals the kinds ({})",
        ui.get_storage_summary()
    );

    // The redesigned tab binds one row per kind and its proportional bar.
    let tab = include_str!("../ui/components/settings/local-storage-tab.slint");
    assert!(
        tab.contains("for row[index] in root.storage-rows"),
        "the breakdown repeats one row per storage kind"
    );
    assert!(
        tab.contains("share: row.share"),
        "each row paints the Rust-computed share"
    );
    assert!(
        tab.contains(
            "root.storage-clear-armed ? I18n.config-storage-clear-armed : I18n.config-storage-clear"
        ),
        "the danger action switches to its confirm label while armed"
    );
    let buffer = render_config_tab(store.clone(), ConfigTab::LocalStorage, |_, _| {})?;
    assert_tab_paints(&buffer, "the Local Storage tab");

    crate::app::with_app(&app, |app| app.config_storage_clear(&ui));
    assert!(ui.get_storage_clear_armed(), "the first click only arms");
    assert!(exports.join("one.json").exists());

    crate::app::with_app(&app, |app| app.config_storage_clear(&ui));
    assert!(!ui.get_storage_clear_armed());
    assert!(
        !exports.join("one.json").exists(),
        "the second click clears"
    );
    assert!(!exports.join("two.json").exists());
    Ok(())
}

/// The two-step clear is visible: arming repaints the danger button with the
/// confirm label and deletes nothing until the second click.
#[test]
fn storage_danger_label_switches_when_armed() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    std::fs::write(store.index_path(), "{ }")?;
    let exports = crate::storage_scan::exports_dir(store.root());
    std::fs::create_dir_all(&exports)?;
    std::fs::write(exports.join("one.json"), vec![0_u8; 200])?;

    let unarmed = render_config_tab(store.clone(), ConfigTab::LocalStorage, |_, _| {})?;
    let armed = render_config_tab(store, ConfigTab::LocalStorage, |ui, app| {
        crate::app::with_app(app, |app| app.config_storage_clear(ui));
        assert!(
            ui.get_storage_clear_armed(),
            "the first click only arms the action"
        );
        assert!(exports.join("one.json").exists(), "arming deletes nothing");
    })?;

    assert_tab_paints(&unarmed, "the Local Storage tab");
    assert!(
        differing_pixels(&unarmed, &armed)? > 100,
        "the armed state repaints the danger button with its confirm label"
    );
    Ok(())
}

#[test]
fn cost_tab_records_estimates_and_clears() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.costs
            .record_estimate("node-1", "gpt-image-1", GenerationCostUnit::Image, 2.0, 10);
        app.costs
            .record_estimate("node-2", "nope", GenerationCostUnit::Image, 1.0, 11);
        app.persist_costs(&ui);
        app.config_tab = ConfigTab::Cost;
        app.config_open = true;
        app.sync_config(&ui);
    });
    let rows = ui.get_cost_rows();
    // One heading per model plus the records.
    assert_eq!(rows.row_count(), 4);
    let heading = rows.row_data(0).ok_or("newest heading")?;
    assert!(heading.heading);
    assert_eq!(heading.model, "nope", "newest model first");
    assert_eq!(
        heading.detail, "1 record",
        "a one-record group reads in the singular"
    );
    let newest = rows.row_data(1).ok_or("newest row")?;
    assert!(!newest.heading);
    assert!(newest.unpriced);
    let priced = rows.row_data(3).ok_or("priced row")?;
    assert_eq!(priced.amount, "~$0.08");
    assert!(ui.get_cost_summary().contains("$0.08"));
    assert!(ui.get_cost_summary().contains("1 unpriced"));
    // The summary card counts the priced records under its headline.
    assert_eq!(ui.get_cost_priced_count(), 1);
    // A heading per model group, each carrying that group's record count.
    let headings: Vec<_> = rows.iter().filter(|row| row.heading).collect();
    assert_eq!(headings.len(), 2, "one heading per model group");
    assert!(
        headings
            .iter()
            .all(|row| row.detail.ends_with("record") || row.detail.ends_with("records")),
        "the heading says how many records the group holds"
    );
    // An unpriced record is flagged rather than rendered as a zero.
    let unpriced = rows
        .iter()
        .find(|row| !row.heading && row.unpriced)
        .ok_or("unpriced row")?;
    assert_eq!(unpriced.amount, "Unpriced");
    assert_eq!(unpriced.source, "estimate");

    // The redesigned tab binds each group header and every row's price slot,
    // and keeps the three callbacks live.
    let tab = include_str!("../ui/components/settings/cost-tab.slint");
    assert!(
        tab.contains("for row[index] in root.cost-rows"),
        "the ledger repeats the Rust-built rows"
    );
    assert!(
        tab.contains("if row.heading && index > 0 : SettingsDivider"),
        "a rule separates the model groups, never above the first"
    );
    assert!(
        tab.contains("if row.unpriced : SettingsChip"),
        "an unpriced record is flagged with a chip"
    );
    assert!(
        tab.contains("root.cost-lookup();"),
        "the lookup action calls its callback"
    );
    assert!(
        tab.contains("SettingsEmptyState"),
        "the ledger has an empty state"
    );
    assert!(
        tab.contains("root.cost-id-edited(value);") && tab.contains("root.cost-clear();"),
        "every callback stays bound"
    );

    // The records survive a reload from `costs.json`.
    let reloaded = store.load_costs()?;
    assert_eq!(reloaded.records.len(), 2);

    let populated = render_config_tab(store.clone(), ConfigTab::Cost, |_, _| {})?;
    assert_tab_paints(&populated, "the Cost tab with records");

    crate::app::with_app(&app, |app| app.config_cost_clear(&ui));
    assert_eq!(ui.get_cost_rows().row_count(), 0);
    assert_eq!(ui.get_cost_priced_count(), 0);
    assert!(
        ui.get_cost_summary().starts_with("$0.00"),
        "an empty ledger reads $0.00, never $-0.00 ({})",
        ui.get_cost_summary()
    );
    assert_eq!(store.load_costs()?.records.len(), 0);

    let empty = render_config_tab(store, ConfigTab::Cost, |_, _| {})?;
    assert_tab_paints(&empty, "the Cost tab with no records");
    Ok(())
}

#[test]
fn cost_tab_lookup_round_trips_through_the_lookup_record() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::Cost;
        app.config_open = true;
        app.sync_config(&ui);
    });
    crate::app::with_app(&app, |app| app.config_cost_id_edited(&ui, "gen-123"));
    assert_eq!(ui.get_cost_generation_id(), "gen-123");
    // An empty API key refuses the lookup with a notice instead of a request.
    crate::app::with_app(&app, |app| {
        app.config_cost_lookup(&ui);
        assert!(
            app.notice.is_some(),
            "the lookup reports why it could not run"
        );
    });

    crate::app::with_app(&app, |app| {
        app.costs.record_lookup(
            "openai/gpt-image-2.5-sunburst",
            GenerationCostUnit::Image,
            1.0,
            0.05,
            20,
        );
        app.persist_costs(&ui);
        app.sync_config(&ui);
    });
    let rows = ui.get_cost_rows();
    let row = (0..rows.row_count())
        .filter_map(|index| rows.row_data(index))
        .find(|row| !row.heading)
        .ok_or("lookup row")?;
    assert_eq!(
        row.amount, "$0.05",
        "a looked-up price has no estimate tilde"
    );
    assert_eq!(row.source, "lookup");
    assert_eq!(store.load_costs()?.records[0].source, "lookup");
    Ok(())
}

#[test]
fn logs_tab_filters_clears_and_tracks_new_entries() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let root = store.root().to_path_buf();
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::Logs;
        app.config_open = true;
        app.log(RuntimeLogLevel::Info, "generation", "started", None);
        app.log(
            RuntimeLogLevel::Error,
            "generation",
            "boom",
            Some("stack".into()),
        );
        app.log(RuntimeLogLevel::Error, "cost", "price", None);
        app.sync_config(&ui);
    });
    assert_eq!(ui.get_log_rows().row_count(), 3);
    let newest = ui.get_log_rows().row_data(0).ok_or("newest log")?;
    assert_eq!(newest.level, "error", "newest first");
    assert_eq!(newest.scope, "cost");
    assert!(!newest.expanded, "details start collapsed");
    assert!(!newest.key.is_empty(), "rows carry an expansion key");

    // Category chips: all + the distinct scopes, sorted.
    assert_eq!(ui.get_logs_categories().row_count(), 3);
    assert_eq!(ui.get_logs_categories().row_data(0), Some("all".into()));
    assert_eq!(ui.get_logs_categories().row_data(1), Some("cost".into()));

    crate::app::with_app(&app, |app| app.config_logs_filter(&ui, "info"));
    assert_eq!(ui.get_logs_filter(), "info");
    assert_eq!(ui.get_log_rows().row_count(), 1);
    assert_eq!(
        ui.get_log_rows().row_data(0).map(|row| row.message),
        Some("started".into())
    );

    // Category filter combines with the level filter.
    crate::app::with_app(&app, |app| {
        app.config_logs_filter(&ui, "error");
        app.config_logs_category(&ui, "cost");
    });
    assert_eq!(ui.get_logs_category(), "cost");
    assert_eq!(ui.get_log_rows().row_count(), 1);
    assert_eq!(
        ui.get_log_rows().row_data(0).map(|row| row.message),
        Some("price".into())
    );

    // A live entry arrives while the tab is open: the tick refreshes the rows.
    crate::app::with_app(&app, |app| {
        app.config_logs_category(&ui, "all");
        app.log(RuntimeLogLevel::Info, "generation", "later", None);
        app.poll_logs(&ui);
    });
    assert_eq!(
        ui.get_log_rows().row_count(),
        2,
        "the error filter stays on"
    );
    assert_eq!(
        ui.get_log_rows().row_data(0).map(|row| row.message),
        Some("price".into())
    );

    // Detail rows expand and collapse by key (the row that carries `data`).
    crate::app::with_app(&app, |app| {
        app.config_logs_filter(&ui, "all");
        let boom_key = ui
            .get_log_rows()
            .iter()
            .find(|row| !row.detail.is_empty())
            .map(|row| row.key)
            .unwrap_or_default();
        app.config_logs_toggle(&ui, boom_key.as_str());
    });
    let expanded_row = ui
        .get_log_rows()
        .iter()
        .find(|row| !row.detail.is_empty())
        .ok_or("a row with detail")?;
    assert!(expanded_row.expanded, "the detail row expands");

    // Download writes exactly the visible rows (level filter on).
    crate::app::with_app(&app, |app| app.config_logs_filter(&ui, "info"));
    crate::app::with_app(&app, |app| app.config_logs_download(&ui));
    let saved = std::fs::read_to_string(root.join("exports/runtime-logs.txt"))?;
    assert!(saved.contains("later"), "the visible row was written");
    assert!(!saved.contains("price"), "filtered-out rows stay out");

    crate::app::with_app(&app, |app| app.config_logs_clear(&ui));
    assert_eq!(ui.get_log_rows().row_count(), 0);
    Ok(())
}

/// The redesigned stream: one row per entry, level segments that follow
/// `logs-filter`, the category select, the expandable detail plate and both
/// empty states (unfiltered and filtered to nothing).
#[test]
fn logs_tab_renders_rows_chips_and_empty_states() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;

    let tab = include_str!("../ui/components/settings/logs-tab.slint");
    assert!(
        tab.contains("for row[index] in root.log-rows"),
        "the stream repeats one row per entry"
    );
    assert!(
        tab.contains("selected: root.logs-filter == \"error\""),
        "the level segments follow the active filter"
    );
    assert!(
        tab.contains("for category in root.categories"),
        "the category select lists the Rust scopes"
    );
    assert!(
        tab.contains("root.log-toggle(row.key)"),
        "a row toggles its detail by key"
    );
    assert!(
        tab.contains("if root.entry.expanded && root.entry.detail != \"\""),
        "the detail plate renders only for an expanded row"
    );
    for callback in [
        "root.log-filter-pick(\"all\")",
        "root.log-category-pick(category)",
        "root.logs-copy();",
        "root.logs-download();",
        "root.logs-clear();",
    ] {
        assert!(tab.contains(callback), "the tab keeps {callback} bound");
    }
    assert!(
        tab.contains("I18n.config-logs-empty-filtered")
            && tab.contains("I18n.config-logs-empty-hint"),
        "the empty state distinguishes a filtered stream from an empty one"
    );

    let populated = render_config_tab(store.clone(), ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.log(RuntimeLogLevel::Info, "generation", "started", None);
            app.log(
                RuntimeLogLevel::Error,
                "generation",
                "boom",
                Some("provider returned 500".into()),
            );
            app.sync_config(ui);
            if let Some(key) = ui
                .get_log_rows()
                .iter()
                .find(|row| !row.detail.is_empty())
                .map(|row| row.key)
            {
                app.config_logs_toggle(ui, key.as_str());
            }
            assert_eq!(ui.get_log_rows().row_count(), 2, "one row per entry");
        });
    })?;
    assert_tab_paints(&populated, "the Runtime Logs tab with entries");

    let filtered_empty = render_config_tab(store.clone(), ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_logs_filter(ui, "warn");
            assert_eq!(ui.get_log_rows().row_count(), 0, "no warnings recorded");
        });
    })?;
    assert_tab_paints(&filtered_empty, "the Runtime Logs tab filtered to nothing");

    let empty = render_config_tab(store, ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| app.config_logs_clear(ui));
    })?;
    assert_tab_paints(&empty, "the Runtime Logs tab with no entries");
    Ok(())
}

/// The level segments and the stream follow `logs-filter`, and an expanded row
/// paints its `data` plate: both are visible, not just bound.
#[test]
fn logs_tab_filter_and_expansion_repaint() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;

    let all = render_config_tab(store.clone(), ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.log(
                RuntimeLogLevel::Error,
                "generation",
                "boom",
                Some("provider returned 500".into()),
            );
            app.log(RuntimeLogLevel::Info, "cost", "priced", None);
            app.sync_config(ui);
        });
    })?;
    assert!(
        all.width() > 0 && all.height() > 0,
        "the populated Runtime Logs tab renders"
    );

    let error = render_config_tab(store.clone(), ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_logs_filter(ui, "error");
        });
    })?;
    assert!(
        differing_pixels(&all, &error)? > 100,
        "the selected level segment and the filtered stream repaint"
    );

    let collapsed = render_config_tab(store.clone(), ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_logs_filter(ui, "all");
        });
    })?;
    let expanded = render_config_tab(store, ConfigTab::Logs, |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_logs_filter(ui, "all");
            if let Some(key) = ui
                .get_log_rows()
                .iter()
                .find(|row| !row.detail.is_empty())
                .map(|row| row.key)
            {
                app.config_logs_toggle(ui, key.as_str());
            }
            assert!(
                ui.get_log_rows().iter().any(|row| row.expanded),
                "the entry with detail is open"
            );
        });
    })?;
    assert!(
        differing_pixels(&collapsed, &expanded)? > 40,
        "the expanded row paints its detail plate"
    );
    Ok(())
}

#[test]
fn runtime_logs_persist_across_restarts() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let second_store = store.clone();
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        app.log(
            RuntimeLogLevel::Warn,
            "generation",
            "kept",
            Some("detail".into()),
        );
        app.sync_config(&ui);
    });
    drop(app);
    drop(ui);

    let second = crate::app::App::new(second_store);
    assert_eq!(second.logs.len(), 1, "the persisted buffer reloads");
    let entry = second.logs.snapshot().into_iter().next().ok_or("entry")?;
    assert_eq!(entry.message, "kept");
    assert_eq!(entry.data.as_deref(), Some("detail"));
    assert_eq!(entry.level, RuntimeLogLevel::Warn);
    Ok(())
}

/// About keeps only the version (no inline keyboard-shortcuts list). The
/// standalone shortcuts modal is still reachable from the dock.
#[test]
fn about_tab_carries_only_the_version() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::About;
        app.config_open = true;
        app.sync_config(&ui);
    });
    assert_eq!(ui.get_config_app_version(), env!("CARGO_PKG_VERSION"));
    let about = include_str!("../ui/components/settings/about-tab.slint");
    assert!(
        !about.contains("ShortcutRows") && !about.contains("open-shortcuts"),
        "the About tab no longer embeds the shortcut list"
    );
    Ok(())
}

/// The settings modal is the only plugin-manager entry now that the library
/// bar and the title bar are gone: the About tab's Plugins card opens the
/// existing manager over the window, and the home screen carries no entry.
#[test]
fn the_settings_plugins_entry_opens_the_plugin_manager() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::About;
        app.config_open = true;
        app.sync_config(&ui);
    });
    assert!(ui.get_config_open(), "the settings modal is open");
    assert!(!ui.get_plugins_open(), "the manager starts closed");

    ui.invoke_open_plugins();
    assert!(ui.get_plugins_open(), "the About entry opens the manager");
    crate::app::with_app(&app, |app| {
        assert!(app.plugin.ui.manager_open, "the app model follows");
    });

    let about = include_str!("../ui/components/settings/about-tab.slint");
    assert!(
        about.contains("I18n.config-plugins-open") && about.contains("root.open-plugins()"),
        "the About tab carries the Plugins card that opens the manager"
    );
    let home = include_str!("../ui/components/library.slint");
    assert!(
        !home.contains("plugin") && !home.contains("Plugin"),
        "the home screen has no plugin-manager entry"
    );
    Ok(())
}

#[test]
fn providers_tab_yields_the_two_fixed_channels() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        app.config_tab = ConfigTab::Providers;
        app.config_open = true;
        app.sync_config(&ui);
    });
    let channels = ui.get_config_channels();
    assert_eq!(channels.row_count(), 2, "providers are locked to two cards");
    let first = channels.row_data(0).ok_or("provider row")?;
    let second = channels.row_data(1).ok_or("provider row")?;
    assert_eq!(first.id, "openrouter");
    assert_eq!(first.name, "OpenRouter");
    assert_eq!(first.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(second.id, "deepseek");
    assert_eq!(second.name, "DeepSeek");
    assert_eq!(second.base_url, "https://api.deepseek.com");
    assert_eq!(second.models.row_count(), 1, "DeepSeek is text-only");
    assert_eq!(
        second
            .models
            .row_data(0)
            .map(|model| model.name)
            .unwrap_or_default(),
        "deepseek-flash"
    );
    assert!(
        first.model_count_label.contains("model"),
        "the label is the localized count ({})",
        first.model_count_label
    );
    Ok(())
}

#[test]
fn i18n_labels_reach_the_slint_global() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let i18n = ui.global::<crate::I18n>();
    assert_eq!(i18n.get_config_tab_local_models(), "Local models");
    assert_eq!(i18n.get_config_storage_reveal(), "Reveal");
    assert_eq!(i18n.get_dock_shortcuts(), "Keyboard shortcuts");
    // The shell is English-only: no CJK reaches the global.
    assert!(
        !i18n
            .get_config_storage_reveal()
            .chars()
            .any(crate::i18n::is_cjk_or_mojibake),
        "the pushed labels are English"
    );
    Ok(())
}

#[test]
fn window_defaults_do_not_need_rust() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let ui = AppWindow::new()?;
    assert_eq!(ui.get_config_tab(), "providers");
    assert_eq!(ui.get_config_theme(), "light");
    assert_eq!(ui.get_logs_filter(), "all");
    Ok(())
}

/// The Appearance tab carries the two window sliders beside the theme group:
/// both render as `Slider`s bound to the tab's levels, and both forward through
/// the modal to the root's `glass-setting` callback. A change mutates the
/// theme and the persisted `ui.json`.
#[test]
fn appearance_tab_has_opacity_and_blur_sliders() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let tab = include_str!("../ui/components/settings/appearance-tab.slint");
    assert!(tab.contains("Slider {"), "the tab renders slider widgets");
    assert!(
        tab.contains("value: root.plate-opacity;"),
        "the opacity slider is bound to the level"
    );
    assert!(
        tab.contains("value: root.backdrop-blur;"),
        "the blur slider is bound to the level"
    );
    assert!(
        tab.contains("root.opacity-changed(value);") && tab.contains("root.blur-changed(value);"),
        "both sliders forward their change"
    );
    assert!(
        tab.contains("root.option-pick(\"theme\", value);"),
        "the theme control stays in the same tab"
    );
    let modal = include_str!("../ui/components/config-modal.slint");
    assert!(
        modal.contains("plate-opacity: root.glass-opacity;")
            && modal.contains("backdrop-blur: root.glass-blur;"),
        "the modal forwards the persisted levels into the tab"
    );
    assert!(
        modal.contains("root.glass-setting(\"opacity\", value);")
            && modal.contains("root.glass-setting(\"blur\", value);"),
        "both sliders reach the root callback"
    );

    let (_dir, store) = crate::config_test_support::temp_store()?;
    let buffer = render_config_tab(store.clone(), ConfigTab::Appearance, |ui, app| {
        crate::app::with_app(app, |app| {
            app.glass_setting(ui, "opacity", 88.0);
            app.glass_setting(ui, "blur", 65.0);
        });
    })?;
    assert!(
        distinct_colors(&buffer) > 12,
        "the appearance tab paints its controls ({} colours)",
        distinct_colors(&buffer)
    );
    assert!((ui_value(&store, "tint_opacity") - 88.0).abs() < f64::EPSILON);
    Ok(())
}

/// Reads one persisted glass level back from the store's `ui.json`.
fn ui_value(store: &Store, key: &str) -> f64 {
    let text = std::fs::read_to_string(store.ui_state_path()).unwrap_or_default();
    let wanted = format!("\"{key}\":");
    text.split(&wanted)
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or_default()
}
