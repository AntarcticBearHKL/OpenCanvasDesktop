//! Appearance tests: theme resolution + follow-system, id round trips, image
//! count normalization, system-theme parsing, and `ui.json` persistence of the
//! theme/locale pair (including legacy files without those fields).

use std::error::Error;

use crate::i18n::Locale;
use crate::store::{self, UiState, LOCALE_ZH_CN, THEME_DARK, THEME_FOLLOW_SYSTEM, THEME_LIGHT};

use super::*;

#[test]
fn resolve_dark_covers_the_three_looks() {
    assert!(!resolve_dark(ThemeLook::Light, true));
    assert!(!resolve_dark(ThemeLook::Light, false));
    assert!(resolve_dark(ThemeLook::Dark, false));
    assert!(resolve_dark(ThemeLook::Dark, true));
    assert!(resolve_dark(ThemeLook::FollowSystem, true));
    assert!(!resolve_dark(ThemeLook::FollowSystem, false));
}

#[test]
fn theme_ids_round_trip_and_unknown_values_degrade_to_light() {
    for look in [ThemeLook::Light, ThemeLook::Dark, ThemeLook::FollowSystem] {
        assert_eq!(ThemeLook::from_id(look.as_str()), look);
    }
    assert_eq!(ThemeLook::from_id("sepia"), ThemeLook::Light);
    assert_eq!(ThemeLook::from_id(""), ThemeLook::Light);
    assert_eq!(ThemeLook::Light.as_str(), THEME_LIGHT);
    assert_eq!(ThemeLook::Dark.as_str(), THEME_DARK);
    assert_eq!(ThemeLook::FollowSystem.as_str(), THEME_FOLLOW_SYSTEM);
}

#[test]
fn system_theme_parsing_reads_the_reg_query_output() {
    let light = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\r\n    AppsUseLightTheme    REG_DWORD    0x1\r\n\r\n";
    assert_eq!(parse_system_theme(light), Some(false));
    let dark = "    AppsUseLightTheme    REG_DWORD    0x0\r\n";
    assert_eq!(parse_system_theme(dark), Some(true));
    assert_eq!(parse_system_theme("no such value here"), None);
    assert_eq!(
        parse_system_theme("    AppsUseLightTheme    REG_DWORD    zzz"),
        None
    );
}

#[test]
fn ui_state_round_trips_theme_and_locale() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let state = UiState {
        theme: THEME_DARK.to_string(),
        locale: LOCALE_ZH_CN.to_string(),
        ..UiState::default()
    };
    store.save_ui_state(&state)?;
    let loaded = store.load_ui_state()?;
    assert_eq!(loaded, state);
    Ok(())
}

#[test]
fn legacy_ui_state_without_appearance_gets_the_web_defaults() {
    let raw = r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":300.0,"sidePanelOpen":false}"#;
    let state = UiState::parse(raw);
    assert_eq!(state.theme, THEME_LIGHT);
    assert_eq!(state.locale, store::LOCALE_EN_US);
    assert!((state.side_panel_width - 300.0).abs() < f64::EPSILON);
    assert!(!state.side_panel_open);
}

#[test]
fn unknown_theme_and_locale_ids_degrade_to_defaults() {
    let raw = r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":280.0,"sidePanelOpen":true,"theme":"sepia","locale":"fr-FR"}"#;
    let state = UiState::parse(raw);
    assert_eq!(state.theme, THEME_LIGHT);
    assert_eq!(state.locale, store::LOCALE_EN_US);
}

#[test]
fn app_starts_from_the_persisted_appearance_with_en_us_copy() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = crate::config_test_support::temp_store()?;
    store.save_ui_state(&UiState {
        theme: THEME_FOLLOW_SYSTEM.to_string(),
        locale: LOCALE_ZH_CN.to_string(),
        ..UiState::default()
    })?;
    let app = App::new(store);
    assert_eq!(app.theme_look, ThemeLook::FollowSystem);
    // The product ships en-US only: a persisted zh-CN from an older build is
    // ignored, never honoured.
    assert_eq!(app.locale, Locale::EnUs);
    Ok(())
}

#[test]
fn startup_applies_the_persisted_theme_and_locale() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    store.save_ui_state(&UiState {
        theme: THEME_DARK.to_string(),
        locale: LOCALE_ZH_CN.to_string(),
        ..UiState::default()
    })?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    assert!(
        ui.global::<crate::Theme>().get_dark(),
        "the dark look applies"
    );
    assert_eq!(
        ui.global::<crate::I18n>().get_config_title(),
        "Settings",
        "the English table is pushed before the first paint, whatever ui.json says"
    );
    Ok(())
}

#[test]
fn theme_pick_persists_and_repaints_the_global() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    assert!(
        !ui.global::<crate::Theme>().get_dark(),
        "the web default is light"
    );

    crate::app::with_app(&app, |app| app.theme_pick(&ui, "dark"));
    assert!(ui.global::<crate::Theme>().get_dark());
    assert_eq!(store.load_ui_state()?.theme, THEME_DARK);

    crate::app::with_app(&app, |app| app.theme_pick(&ui, "follow-system"));
    crate::app::with_app(&app, |app| {
        app.system_dark = true;
        app.apply_theme(&ui);
    });
    assert!(
        ui.global::<crate::Theme>().get_dark(),
        "follow-system paints the OS state"
    );
    crate::app::with_app(&app, |app| {
        app.system_dark = false;
        app.apply_theme(&ui);
    });
    assert!(!ui.global::<crate::Theme>().get_dark());
    assert_eq!(store.load_ui_state()?.theme, THEME_FOLLOW_SYSTEM);
    Ok(())
}

#[test]
fn the_locale_cannot_be_switched_and_stays_en_us() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    // A stale choice on disk: startup must still be English.
    store.save_ui_state(&UiState {
        locale: LOCALE_ZH_CN.to_string(),
        ..UiState::default()
    })?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    assert_eq!(ui.global::<crate::I18n>().get_config_title(), "Settings");
    assert_eq!(ui.global::<crate::I18n>().get_library_eyebrow(), "LIBRARY");
    assert_eq!(
        ui.global::<crate::I18n>().get_locale(),
        "en-US",
        "the Slint global advertises the only shipped locale"
    );
    crate::app::with_app(&app, |app| assert_eq!(app.locale, Locale::EnUs));
    Ok(())
}

#[test]
fn ui_state_round_trips_the_glass_levels_under_the_documented_keys() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let state = UiState {
        tint_opacity: 77.0,
        blur: 12.0,
        ..UiState::default()
    };
    store.save_ui_state(&state)?;
    let text = std::fs::read_to_string(store.ui_state_path())?;
    assert!(
        text.contains("\"tint_opacity\":77.0"),
        "the opacity persists as tint_opacity: {text}"
    );
    assert!(
        text.contains("\"blur\":12.0"),
        "the blur persists as blur: {text}"
    );
    let loaded = store.load_ui_state()?;
    assert!((loaded.tint_opacity - 77.0).abs() < f64::EPSILON);
    assert!((loaded.blur - 12.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn glass_levels_clamp_and_missing_fields_take_the_shipped_defaults() {
    assert!((store::clamp_percent(-5.0)).abs() < f64::EPSILON);
    assert!((store::clamp_percent(1000.0) - 100.0).abs() < f64::EPSILON);
    assert!((store::clamp_percent(f64::NAN)).abs() < f64::EPSILON);
    assert!((store::WINDOW_TINT_DEFAULT - 80.0).abs() < f64::EPSILON);
    assert!((store::WINDOW_BLUR_DEFAULT - 100.0).abs() < f64::EPSILON);

    let legacy = r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":300.0,"sidePanelOpen":false}"#;
    let state = UiState::parse(legacy);
    assert!((state.tint_opacity - store::WINDOW_TINT_DEFAULT).abs() < f64::EPSILON);
    assert!((state.blur - store::WINDOW_BLUR_DEFAULT).abs() < f64::EPSILON);

    let hostile = r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":300.0,"sidePanelOpen":true,"theme":"light","locale":"en-US","tint_opacity":-3.0,"blur":140.0}"#;
    let state = UiState::parse(hostile);
    assert!((state.tint_opacity).abs() < f64::EPSILON, "clamped low");
    assert!((state.blur - 100.0).abs() < f64::EPSILON, "clamped high");
}

#[test]
fn app_starts_from_the_persisted_glass_levels() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    store.save_ui_state(&UiState {
        tint_opacity: 66.0,
        blur: 74.0,
        ..UiState::default()
    })?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();
    assert!((theme.get_tint_opacity() - 66.0).abs() < f32::EPSILON);
    assert!((theme.get_blur() - 74.0).abs() < f32::EPSILON);
    assert!((ui.get_glass_opacity() - 66.0).abs() < f32::EPSILON);
    assert!((ui.get_glass_blur() - 74.0).abs() < f32::EPSILON);
    crate::app::with_app(&app, |app| {
        assert!((app.tint_opacity - 66.0).abs() < f64::EPSILON);
        assert!((app.blur - 74.0).abs() < f64::EPSILON);
    });
    Ok(())
}

/// The opacity slider's mapping: 0 is almost clear, 100 nearly opaque, and the
/// shipped 80 keeps the white asset-library frost (`window-tint` alpha < 255 at
/// every level, so the plate is never an opaque plate while the capture is on).
#[test]
fn the_plate_alpha_follows_the_opacity_slider() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    let theme = ui.global::<crate::Theme>();
    assert!(
        theme.get_window_tint().alpha() < 255,
        "the shipped plate is translucent ({} / 255)",
        theme.get_window_tint().alpha()
    );

    crate::app::with_app(&app, |app| app.glass_setting(&ui, "opacity", 0.0));
    let clear = theme.get_window_tint().alpha();
    assert!(clear > 0 && clear < 30, "0 is almost clear ({clear}/255)");

    crate::app::with_app(&app, |app| app.glass_setting(&ui, "opacity", 100.0));
    let opaque = theme.get_window_tint().alpha();
    assert!(
        opaque > 240 && opaque < 255,
        "100 is nearly opaque but never fully ({opaque}/255)"
    );

    crate::app::with_app(&app, |app| app.glass_setting(&ui, "opacity", 80.0));
    assert_eq!(store.load_ui_state()?.tint_opacity, 80.0);
    Ok(())
}

/// The blur slider: 0 disables the backdrop (the plate goes opaque because
/// nothing was accepted) and any level above 0 asks for the material the band
/// names. Persisted in `ui.json` with the level that was set.
#[test]
fn blur_zero_disables_the_backdrop_and_above_zero_reenables_it() -> Result<(), Box<dyn Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store.clone())?;
    let theme = ui.global::<crate::Theme>();

    crate::app::with_app(&app, |app| app.glass_setting(&ui, "blur", 0.0));
    assert!((theme.get_blur()).abs() < f32::EPSILON);
    assert!(
        !ui.get_backdrop(),
        "blur 0 asks for no material, so the plate stays opaque"
    );
    assert!((store.load_ui_state()?.blur).abs() < f64::EPSILON);

    crate::app::with_app(&app, |app| app.glass_setting(&ui, "blur", 70.0));
    assert!((theme.get_blur() - 70.0).abs() < f32::EPSILON);
    assert_eq!(store.load_ui_state()?.blur, 70.0);
    assert_eq!(
        crate::glass::BackdropMaterial::for_blur(70),
        crate::glass::BackdropMaterial::Acrylic,
        "the top range asks for acrylic"
    );
    Ok(())
}
