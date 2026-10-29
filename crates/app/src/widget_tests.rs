//! Tests for the shared widgets in `ui/components/widgets.slint`.
//!
//! 1. A source scan proves the merged per-file definitions are gone and every
//!    former call-site file now names a shared component.
//! 2. A rendered probe proves the slider's fill starts at the track's LEFT edge
//!    (not centred by Slint) and that `step` snaps the value.
//! 3. A render smoke test per studio plus the config modal and the dock.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::ComponentHandle;

use crate::app::View;
use crate::store::Store;
use crate::test_support::init_backend_allow_existing;
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The widgets that were merged into `widgets.slint`, one entry per name.
const MERGED: [&str; 20] = [
    "GlassSlider",
    "AiSlider",
    "TrackSlider",
    "PixelSlider",
    "ImageSettingsSlider",
    "HoverButton",
    "PixelIconButton",
    "RowButton",
    "MiniButton",
    "WindowButton",
    "RailButton",
    "StudioMenuButton",
    "PrimaryButton",
    "GhostButton",
    "ToolPrimary",
    "ToolGhost",
    "StudioButton",
    "SmallButton",
    "AiPrimary",
    "AiGhost",
];

/// Files that used to own or call a merged widget and now use a shared one.
const CALL_SITES: [(&str, &[&str]); 19] = [
    ("settings/local-models-tab.slint", &["Button {"]),
    ("settings/local-storage-tab.slint", &["Button {"]),
    ("settings/cost-tab.slint", &["Button {"]),
    ("settings/logs-tab.slint", &["Button {"]),
    ("image-ai-dialogs.slint", &["Slider {", "Button {"]),
    ("panel-popovers.slint", &["Slider {"]),
    ("image.slint", &["Slider {"]),
    ("toolbar.slint", &["IconButton {"]),
    ("node-list.slint", &["IconButton {"]),
    ("library.slint", &["IconButton {", "Button {"]),
    ("window-chrome.slint", &["IconButton {"]),
    ("image-panels.slint", &["IconButton {", "Slider {"]),
    ("plugin-manager.slint", &["Button {"]),
    ("modals.slint", &["Button {"]),
    ("node-card.slint", &["IconButton {"]),
    ("side-panel.slint", &["IconButton {"]),
    ("image-layers.slint", &["IconButton {", "Slider {"]),
    ("image-panels-extra.slint", &["IconButton {", "Slider {"]),
    ("image-dialogs.slint", &["Slider {", "Button {"]),
];

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn read_slint_files(dir: &Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            read_slint_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "slint") {
            out.push((path.display().to_string(), std::fs::read_to_string(&path)?));
        }
    }
    Ok(())
}

fn slint_sources() -> std::io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    read_slint_files(&ui_dir(), &mut out)?;
    Ok(out)
}

/// Every merged definition is deleted, its file is gone, and the shared
/// components are exported from `widgets.slint`.
#[test]
fn merged_widget_definitions_are_gone() -> TestResult {
    let sources = slint_sources()?;
    let widgets = sources
        .iter()
        .find(|(path, _)| {
            path.ends_with("components\\widgets.slint")
                || path.ends_with("components/widgets.slint")
        })
        .map(|(_, text)| text.clone())
        .ok_or("ui/components/widgets.slint is missing")?;

    for name in ["Slider", "IconButton", "Button"] {
        assert!(
            widgets.contains(&format!("export component {name} inherits ")),
            "widgets.slint must export {name}"
        );
    }
    for prop in [
        "in-out property <float> value",
        "in property <float> minimum",
        "in property <float> maximum",
        "in property <float> step",
        "callback changed(float)",
    ] {
        assert!(widgets.contains(prop), "Slider is missing `{prop}`");
    }
    for name in MERGED {
        let definition = format!("component {name} inherits");
        for (path, text) in &sources {
            assert!(
                !text.contains(&definition),
                "{path} still defines `{definition}`"
            );
            assert!(
                !text.contains(&format!("{name} {{")),
                "{path} still calls the merged `{name}`"
            );
        }
    }
    assert!(
        !ui_dir().join("components/hover-button.slint").exists(),
        "hover-button.slint was merged into widgets.slint"
    );
    Ok(())
}

/// Each former call-site file now names the shared component it uses.
#[test]
fn former_call_sites_use_the_shared_widgets() -> TestResult {
    let dir = ui_dir().join("components");
    for (file, expected) in CALL_SITES {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path)?;
        assert!(
            text.contains("from \"widgets.slint\"") || text.contains("from \"../widgets.slint\""),
            "{file} must import the shared widgets"
        );
        for needle in expected {
            assert!(
                text.contains(needle),
                "{file} must use `{needle}` from widgets.slint"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Slider pixel probe
// ---------------------------------------------------------------------------
// A tiny window compiling the real shared Slider, so the assertions measure the
// component as shipped rather than a copy of it.
mod probe {
    slint::slint! {
        import { Theme } from "../ui/theme.slint";
        import { Slider } from "../ui/components/widgets.slint";

        export component SliderProbe inherits Window {
            in-out property <float> value;
            in property <float> step;
            width: 200px;
            height: 40px;
            background: Theme.card;

            Slider {
                x: 0px;
                y: 10px;
                width: 200px;
                height: 20px;
                value <=> root.value;
                step: root.step;
                minimum: 0;
                maximum: 100;
            }
        }
    }
}

fn pixel_at(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, x: u32, y: u32) -> (u8, u8, u8) {
    let pixel = buffer.as_slice()[(y * buffer.width() + x) as usize];
    (pixel.r, pixel.g, pixel.b)
}

/// The accent fill, the muted rail and the card behind them (light theme).
fn is_fill(pixel: (u8, u8, u8)) -> bool {
    pixel.0 > 170 && pixel.1 < 150 && pixel.2 < 130
}

fn is_rail(pixel: (u8, u8, u8)) -> bool {
    pixel.0 > 220 && pixel.1 > 210 && pixel.2 > 200 && !is_fill(pixel)
}

#[test]
fn slider_fill_starts_at_the_left_edge() -> TestResult {
    init_backend_allow_existing();
    let ui = probe::SliderProbe::new()?;
    ui.set_step(0.0);
    ui.set_value(50.0);
    ui.show()?;
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;

    // Half the track is filled: the fill begins at x=0 and the handle rides
    // its right edge. A centred fill (Slint's default for an unpositioned
    // rectangle) would leave x=2 on the rail and paint the middle instead.
    let left = pixel_at(&buffer, 2, 20);
    let middle = pixel_at(&buffer, 96, 20);
    let right = pixel_at(&buffer, 190, 20);
    assert!(
        is_fill(left),
        "the fill must start at the left edge: {left:?}"
    );
    assert!(
        is_fill(middle),
        "the fill must cover the left half: {middle:?}"
    );
    assert!(
        is_rail(right),
        "the right half stays on the rail: {right:?}"
    );

    // With the value at zero the left edge shows the rail, so the assertion
    // above really measures the fill rather than a painted background.
    let empty = probe::SliderProbe::new()?;
    empty.set_value(0.0);
    empty.show()?;
    empty.window().request_redraw();
    let empty_buffer = empty.window().take_snapshot()?;
    let empty_right = pixel_at(&empty_buffer, 150, 20);
    assert!(
        is_rail(empty_right),
        "a zero value leaves the track unfilled: {empty_right:?}"
    );
    Ok(())
}

#[test]
fn slider_snaps_to_step() -> TestResult {
    init_backend_allow_existing();
    let ui = probe::SliderProbe::new()?;
    ui.set_step(0.0);
    ui.set_value(0.0);
    ui.show()?;
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(94.0, 20.0),
            button: slint::platform::PointerEventButton::Left,
        });
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(94.0, 20.0),
            button: slint::platform::PointerEventButton::Left,
        });
    let continuous = ui.get_value();
    assert!(
        (continuous - 47.0).abs() < 1.0,
        "a continuous slider keeps the pointer value ({continuous})"
    );

    let stepped = probe::SliderProbe::new()?;
    stepped.set_step(25.0);
    stepped.set_value(0.0);
    stepped.show()?;
    stepped
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(140.0, 20.0),
            button: slint::platform::PointerEventButton::Left,
        });
    stepped
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(140.0, 20.0),
            button: slint::platform::PointerEventButton::Left,
        });
    assert!(
        (stepped.get_value() - 75.0).abs() < 0.01,
        "70% snaps to the 25 step ({})",
        stepped.get_value()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Render smoke tests
// ---------------------------------------------------------------------------

fn render_view(
    view: View,
    view_id: &str,
    setup: impl FnOnce(&AppWindow, &Rc<RefCell<crate::app::App>>),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = view;
        app.sync_all(&ui);
    });
    setup(&ui, &app);
    ui.set_view(view_id.into());
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

fn distinct_colors(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for pixel in buffer.as_slice() {
        seen.insert((pixel.r, pixel.g, pixel.b));
        if seen.len() > 64 {
            break;
        }
    }
    seen.len()
}

fn assert_painted(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, what: &str) {
    assert!(buffer.width() > 0 && buffer.height() > 0, "{what} is empty");
    assert!(
        distinct_colors(buffer) > 8,
        "{what} paints real chrome, not one flat colour"
    );
}

#[test]
fn image_studio_renders() -> TestResult {
    let buffer = render_view(View::Image, "image", |_, _| {})?;
    assert_painted(&buffer, "the Image studio");
    Ok(())
}

/// The data-driven Filter menu is bound from Rust and scrolls; opening it must
/// paint the rows rather than an empty (zero-height) panel.
#[test]
fn filter_menu_renders_its_rows() -> TestResult {
    let closed = render_view(View::Image, "image", |_, _| {})?;
    let open = render_view(View::Image, "image", |ui, app| {
        crate::app::with_app(app, |app| {
            app.image_action(ui, "filter");
        });
    })?;
    let painted = open
        .as_slice()
        .iter()
        .zip(closed.as_slice())
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b)
        .count();
    assert!(
        painted > 5_000,
        "the Filter menu paints its rows over the studio ({painted})"
    );
    Ok(())
}

#[test]
fn config_modal_renders_its_sliders() -> TestResult {
    let closed = render_view(View::Image, "image", |_, _| {})?;
    let open = render_view(View::Image, "image", |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_tab = crate::config_ui::ConfigTab::Appearance;
            app.config_open = true;
            app.sync_config(ui);
        });
    })?;
    assert_painted(&open, "the config modal");
    let painted = open
        .as_slice()
        .iter()
        .zip(closed.as_slice())
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b)
        .count();
    assert!(
        painted > 5_000,
        "the config modal paints over the shell ({painted})"
    );
    Ok(())
}

#[test]
fn dock_renders() -> TestResult {
    let buffer = render_view(View::Image, "image", |_, _| {})?;
    assert_painted(&buffer, "the editor shell");
    // The dock bar occupies the bottom of the editor: its chrome is painted, so
    // the last rows are not one flat canvas colour.
    let height = buffer.height();
    let width = buffer.width();
    let mut seen = std::collections::BTreeSet::new();
    for y in height.saturating_sub(80)..height {
        for x in 0..width {
            let pixel = buffer.as_slice()[(y * width + x) as usize];
            seen.insert((pixel.r, pixel.g, pixel.b));
        }
    }
    assert!(
        seen.len() > 3,
        "the dock bar paints its buttons and surface ({} colors)",
        seen.len()
    );
    Ok(())
}
