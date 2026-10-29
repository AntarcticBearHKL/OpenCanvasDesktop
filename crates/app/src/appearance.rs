//! Appearance settings: theme look (light / dark / follow-system). The theme
//! look persists in `ui.json` beside the
//! side-panel prefs (`src/store.rs`) and is applied by pushing `Theme.dark` plus
//! the English string table into the Slint globals — with no transition, which
//! is the web's treatment too (`use-theme-store.ts` flips CSS variables that
//! re-evaluate instantly; motion-catalog.md §6 has no theme-change animation).
//!
//! There is no language setting: the product ships en-US only, so the persisted
//! `locale` field is written for compatibility but never honoured on startup.
//!
//! Follow-system reads the Windows personalization value
//! (`HKCU\...\Themes\Personalize\AppsUseLightTheme`) through `reg.exe`, throttled
//! to one read every [`SYSTEM_THEME_POLL_MS`], and only while that look is
//! selected — so the app tracks an OS switch without a background process.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::app::{with_app, App};
use crate::i18n;
use crate::store::{self, UiState};
use crate::AppWindow;

/// How often a follow-system look re-reads the OS theme (milliseconds).
pub const SYSTEM_THEME_POLL_MS: u64 = 2000;

/// The persisted theme look (`light` | `dark` | `follow-system`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeLook {
    Light,
    Dark,
    FollowSystem,
}

impl ThemeLook {
    /// Parses a persisted/UI id; anything unknown degrades to `Light` (the web
    /// default) so a hand-edited file cannot wedge the app.
    pub fn from_id(id: &str) -> Self {
        match id {
            "dark" => Self::Dark,
            "follow-system" => Self::FollowSystem,
            _ => Self::Light,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::FollowSystem => "follow-system",
        }
    }
}

/// Resolves the dark flag a look paints: the explicit looks are themselves, and
/// `follow-system` tracks the OS bool.
pub fn resolve_dark(look: ThemeLook, system_dark: bool) -> bool {
    match look {
        ThemeLook::Light => false,
        ThemeLook::Dark => true,
        ThemeLook::FollowSystem => system_dark,
    }
}

/// Parses `reg query ... /v AppsUseLightTheme` output: `0x1` means the OS apps
/// theme is light (so the app must paint dark = false), `0x0` means dark.
pub fn parse_system_theme(text: &str) -> Option<bool> {
    let line = text
        .lines()
        .find(|line| line.contains("AppsUseLightTheme"))?;
    let value = line.split_whitespace().last()?;
    match value {
        "0x1" => Some(false),
        "0x0" => Some(true),
        _ => None,
    }
}

/// Reads the Windows personalization value; `None` when the query fails or the
/// platform has no such key (non-Windows builds).
#[cfg(windows)]
pub fn read_system_dark() -> Option<bool> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_system_theme(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
pub fn read_system_dark() -> Option<bool> {
    None
}

impl App {
    /// The dark flag the current look paints (startup + switch + OS poll).
    pub fn resolved_dark(&self) -> bool {
        resolve_dark(self.theme_look, self.system_dark)
    }

    /// Pushes `Theme.dark`, the locale tables and the localized top-bar title.
    pub fn apply_appearance(&mut self, ui: &AppWindow) {
        let theme = ui.global::<crate::Theme>();
        theme.set_dark(self.resolved_dark());
        i18n::push_locale(ui, self.locale);
        self.apply_glass(ui);
        self.sync_chrome(ui);
    }

    /// Pushes the persisted window glass levels: `Theme.tint-opacity` drives the
    /// plate's alpha (the theme computes `window-tint` from it) and
    /// `Theme.blur` drives the in-app Gaussian on the next capture tick. The
    /// settings tab reads the same two values from the root properties.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "both levels are clamped to 0-100 before the cast, so every value is exact in f32"
    )]
    pub fn apply_glass(&mut self, ui: &AppWindow) {
        let opacity = store::clamp_percent(self.tint_opacity) as f32;
        let blur = store::clamp_percent(self.blur) as f32;
        let theme = ui.global::<crate::Theme>();
        theme.set_tint_opacity(opacity);
        theme.set_blur(blur);
        ui.set_glass_opacity(opacity);
        ui.set_glass_blur(blur);
    }

    /// Appearance tab: the **Opacity** / **Blur** sliders. `kind` is
    /// `"opacity"` or `"blur"`; the level is clamped to 0–100. Both persist to
    /// `ui.json` and apply immediately — a blur change re-blurs the stored
    /// capture (blur 0 clears it and falls back to the opaque plate; any level
    /// above 0 takes a fresh capture).
    pub fn glass_setting(&mut self, ui: &AppWindow, kind: &str, level: f64) {
        let level = store::clamp_percent(level);
        match kind {
            "opacity" => {
                if (self.tint_opacity - level).abs() < f64::EPSILON {
                    return;
                }
                self.tint_opacity = level;
            }
            "blur" => {
                if (self.blur - level).abs() < f64::EPSILON {
                    return;
                }
                self.blur = level;
            }
            _ => return,
        }
        self.apply_glass(ui);
        self.persist_ui_state();
        // Re-apply the OS material now instead of on the next 8 ms tick: the
        // blur level is part of the native pass's key, so this only costs the
        // two Win32 calls when the material actually changes. Blur 0 leaves the
        // backdrop off and the plate opaque.
        crate::refresh_window_chrome(ui);
    }

    pub(crate) fn apply_theme(&mut self, ui: &AppWindow) {
        ui.global::<crate::Theme>().set_dark(self.resolved_dark());
        // The transparent-window accent is theme-aware (white in light, dark in
        // dark). Re-dress immediately so the first-frame fallback never spends a
        // frame in the old theme's tint.
        crate::refresh_window_chrome(ui);
    }

    /// Writes `ui.json` (side panel, theme look, locale, window opacity/blur); a
    /// failure only means the next launch starts from the previous preferences.
    pub(crate) fn persist_ui_state(&self) -> bool {
        let state = UiState {
            side_panel_width: self.side_width,
            side_panel_open: self.side_open,
            theme: self.theme_look.as_str().to_string(),
            locale: crate::i18n::Locale::id().to_string(),
            tint_opacity: store::clamp_percent(self.tint_opacity),
            blur: store::clamp_percent(self.blur),
            glass_capture: self.glass_capture,
            ..UiState::default()
        };
        self.store.save_ui_state(&state).is_ok()
    }

    /// Appearance tab / top-bar toggle: `light` | `dark` | `follow-system`.
    pub fn theme_pick(&mut self, ui: &AppWindow, id: &str) {
        let look = ThemeLook::from_id(id);
        if look == self.theme_look {
            return;
        }
        self.theme_look = look;
        if look == ThemeLook::FollowSystem {
            self.system_dark = read_system_dark().unwrap_or(self.system_dark);
            self.system_theme_checked_ms = self.now_ms();
        }
        self.persist_ui_state();
        self.apply_theme(ui);
        self.sync_config(ui);
    }

    /// Re-reads the OS theme while `follow-system` is selected, at most once
    /// per [`SYSTEM_THEME_POLL_MS`]; called from the 60 ms app tick.
    pub fn poll_system_theme(&mut self, ui: &AppWindow) {
        if self.theme_look != ThemeLook::FollowSystem {
            return;
        }
        let now = self.now_ms();
        if now.saturating_sub(self.system_theme_checked_ms) < SYSTEM_THEME_POLL_MS {
            return;
        }
        self.system_theme_checked_ms = now;
        let Some(dark) = read_system_dark() else {
            return;
        };
        if dark != self.system_dark {
            self.system_dark = dark;
            self.apply_theme(ui);
        }
    }
}

/// Registers the appearance callbacks: the appearance option rows.
pub fn wire_appearance(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_option_pick(move |kind, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.config_option_pick(&ui, kind.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_glass_setting(move |kind, level| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.glass_setting(&ui, kind.as_str(), f64::from(level));
            });
        });
    }
}

#[cfg(test)]
#[path = "appearance_tests.rs"]
mod tests;
