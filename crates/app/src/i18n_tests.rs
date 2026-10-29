//! i18n table tests: key uniqueness, the en-US fallback chain, the fixed
//! locale and the `{name}` interpolation helper. Pure table checks — no window
//! involved. The product is en-US only, so there is exactly one table and no
//! CJK copy.

use super::*;

fn keys(table: &[(&'static str, &'static str)]) -> Vec<&'static str> {
    table.iter().map(|(key, _)| *key).collect()
}

#[test]
fn tables_have_no_duplicate_keys() {
    let mut en = keys(EN);
    en.sort_unstable();
    let count = en.len();
    en.dedup();
    assert_eq!(count, en.len(), "en-US table has a duplicate key");
}

#[test]
fn t_resolves_keys_and_echoes_a_gap() {
    assert_eq!(t(Locale::EnUs, "config.title"), "Settings");
    assert_eq!(t(Locale::EnUs, "config.tabs.local-models"), "Local models");
    // A missing key comes back verbatim, so a gap is visible instead of
    // silently blank.
    assert_eq!(
        t(Locale::EnUs, "config.nonexistent.key"),
        "config.nonexistent.key"
    );
}

#[test]
fn lookup_prefers_the_active_table_then_en_us() {
    assert_eq!(lookup(&[], "config.title"), "Settings");
    // A table with its own entry wins over the fallback.
    assert_eq!(
        lookup(&[("config.title", "Title")], "config.title"),
        "Title"
    );
    // Missing in the active table AND in en-US: the key echoes back.
    assert_eq!(lookup(&[], "missing.key"), "missing.key");
    assert_eq!(t(Locale::EnUs, "shortcut.wheel"), "Mouse wheel");
}

#[test]
fn the_single_locale_is_en_us() {
    assert_eq!(Locale::id(), "en-US");
    assert_eq!(Locale::default(), Locale::EnUs);
    // There is no parser: a stale `zh-CN` in `ui.json` is never read back into
    // the app (see `appearance_tests::app_starts_..._with_en_us_copy`).
}

#[test]
fn fill_substitutes_every_named_marker() {
    let text = fill("Priced at {usd}", &[("usd", "$0.04")]);
    assert_eq!(text, "Priced at $0.04");
    let text = fill("{a} and {b}", &[("a", "1"), ("b", "2")]);
    assert_eq!(text, "1 and 2");
    assert_eq!(fill("no markers", &[("a", "1")]), "no markers");
}

#[test]
fn derived_labels_are_english() {
    assert_eq!(model_count_label(Locale::EnUs, 2), "2 models");
    assert_eq!(cost_unit_label(Locale::EnUs, "video-second"), "second(s)");
    assert_eq!(cost_unit_label(Locale::EnUs, "unknown-unit"), "image(s)");
}

#[test]
fn the_table_carries_no_cjk_and_no_replacement_character() {
    for (key, value) in EN {
        for text in [*key, *value] {
            assert!(
                !text.chars().any(is_cjk_or_mojibake),
                "the en-US table is English only: {key} = {value:?}"
            );
        }
    }
}

#[test]
fn every_ui_label_key_exists_in_the_table() {
    // Keys the Slint global pushes plus the Rust-only labels the tabs build.
    let ui_keys = [
        "library.eyebrow",
        "library.projects-label",
        "library.new-project",
        "library.new-canvas",
        "library.no-project",
        "library.column.name",
        "library.column.updated",
        "library.column.actions",
        "library.empty.project",
        "library.empty.project-hint",
        "library.empty.no-projects",
        "library.empty.no-projects-hint",
        "dock.shortcuts",
        "config.title",
        "config.tabs.providers",
        "config.tabs.appearance",
        "config.tabs.local-models",
        "config.tabs.local-storage",
        "config.tabs.cost",
        "config.tabs.logs",
        "config.tabs.about",
        "config.plugins.title",
        "config.plugins.hint",
        "config.plugins.open",
        "config.providers.model-count",
        "config.storage.kind.library",
        "config.storage.kind.config",
        "config.storage.kind.costs",
        "config.storage.kind.projects",
        "config.storage.kind.images",
        "config.storage.kind.exports",
        "config.storage.kind.plugins",
        "config.storage.kind.models",
        "config.storage.kind.other",
        "config.storage.summary",
        "config.storage.cleared",
        "config.local.bg-name",
        "config.local.bg-desc",
        "config.local.sam-name",
        "config.local.sam-desc",
        "config.local.size",
        "config.local.installed",
        "config.local.missing",
        "config.cost.records",
        "config.cost.unpriced-note",
        "config.cost.lookup-ok",
        "config.cost.lookup-failed",
        "config.cost.unit.image",
        "config.cost.unit.video-second",
        "config.cost.unit.call",
    ];
    for key in ui_keys {
        assert!(
            EN.iter().any(|(name, _)| *name == key),
            "en-US misses {key}"
        );
    }
}
