//! Headless render tests for the dock chrome, the tooltip surface, the empty
//! shipped bottom edge and the modal width tiers (project rule: code-only
//! verification — the Slint testing backend embeds the software rasterizer, so
//! `Window::take_snapshot` returns a real pixel buffer).

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::app::View;
use crate::image::dock_engine::{studio, DockEdge, STUDIO_IMAGE};
use crate::store::Store;
use crate::test_support::{differing_pixels, init_backend_allow_existing};
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Renders one studio view with a wired app, runs `setup`, and returns the
/// snapshot. `view` is both the Rust view (for the ticks) and the Slint view
/// property (for the studio that paints).
fn render_studio(
    view: View,
    view_id: &str,
    setup: impl FnOnce(&AppWindow, &Rc<RefCell<crate::app::App>>),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
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

/// Reveals the given panels on their home edges and pushes the layout, as the
/// Dock menu does (the shipped default is empty).
fn reveal(ui: &AppWindow, app: &Rc<RefCell<crate::app::App>>, ids: &[&str]) {
    crate::app::with_app(app, |app| {
        if let Some(studio) = studio(STUDIO_IMAGE) {
            for id in ids {
                app.image.dock.reveal(studio, id);
            }
        }
        app.sync_dock(ui, STUDIO_IMAGE);
    });
}

/// The modal tier widths, read from the one design source (`theme.slint`).
#[test]
fn modal_tiers_map_to_the_web_widths() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let theme = ui.global::<crate::Theme>();
    assert_eq!(f64::from(theme.invoke_modal_width("default".into())), 520.0);
    assert_eq!(f64::from(theme.invoke_modal_width("medium".into())), 640.0);
    assert_eq!(f64::from(theme.invoke_modal_width("wide".into())), 880.0);
    // An unknown tier degrades to the AntD default rather than zero.
    assert_eq!(
        f64::from(theme.invoke_modal_width("nonsense".into())),
        520.0
    );
    // `ModalShell`'s default width is the default tier.
    assert_eq!(f64::from(theme.get_modal_width_default()), 520.0);
    Ok(())
}

/// The typography tokens the studios read (panel titles, layer rows, icons).
#[test]
fn typography_tokens_carry_the_web_values() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let theme = ui.global::<crate::Theme>();
    assert_eq!(f64::from(theme.get_text_panel_title()), 12.0);
    assert_eq!(f64::from(theme.get_text_layer_name()), 14.0);
    assert_eq!(f64::from(theme.get_row_layer_height()), 36.0);
    assert_eq!(f64::from(theme.get_icon_layer()), 20.0);
    assert_eq!(theme.get_motion_ants(), 120);
    Ok(())
}

/// The tooltip surface paints only while the tooltip state is showing one, and
/// the description line adds paint in the second stage.
#[test]
fn tooltip_layer_paints_only_while_the_host_shows_it() -> TestResult {
    init_backend_allow_existing();
    let baseline = render_studio(View::Image, "image", |_, _| {})?;
    let shown = render_studio(View::Image, "image", |ui, _| {
        ui.global::<crate::Theme>()
            .set_tooltip(crate::TooltipState {
                text: "Layer Effects".into(),
                description: "".into(),
                visible: true,
                x: 120.0,
                y: 120.0,
            });
    })?;
    let painted = differing_pixels(&baseline, &shown).map_err(ToString::to_string)?;
    assert!(
        painted > 300,
        "the tooltip label paints at its anchor ({painted} pixels)"
    );
    let described = render_studio(View::Image, "image", |ui, _| {
        ui.global::<crate::Theme>()
            .set_tooltip(crate::TooltipState {
                text: "Layer Effects".into(),
                description: "Show or hide the layer effects".into(),
                visible: true,
                x: 120.0,
                y: 120.0,
            });
    })?;
    let extra = differing_pixels(&shown, &described).map_err(ToString::to_string)?;
    assert!(
        extra > 150,
        "the second stage adds the description line ({extra} pixels)"
    );
    Ok(())
}

/// A stacked tab group paints its 30px tab strip with the active tab's ×; the
/// single-panel front paints the slimmer grip bar instead.
#[test]
fn a_stacked_group_paints_its_tab_strip() -> TestResult {
    init_backend_allow_existing();
    let single = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
    })?;
    let stacked = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers", "history"]);
        crate::app::with_app(app, |app| {
            app.image.dock.move_panel("history", DockEdge::Right, 0);
            app.sync_dock(ui, STUDIO_IMAGE);
        });
    })?;
    let painted = differing_pixels(&single, &stacked).map_err(ToString::to_string)?;
    assert!(
        painted > 500,
        "the tab strip and the close × paint ({painted} pixels)"
    );
    Ok(())
}

/// The dock resize splitter is a real 6px grab strip on the column's inner
/// edge: a layout with a different edge size paints a different column width.
#[test]
fn a_resized_edge_paints_a_different_column_width() -> TestResult {
    init_backend_allow_existing();
    let natural = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
    })?;
    let wider = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
        crate::app::with_app(app, |app| {
            app.image.dock.set_size(DockEdge::Right, 480.0);
            app.sync_dock(ui, STUDIO_IMAGE);
        });
    })?;
    let painted = differing_pixels(&natural, &wider).map_err(ToString::to_string)?;
    assert!(
        painted > 5_000,
        "the resized right column repaints the dock area ({painted} pixels)"
    );
    Ok(())
}

/// The studio's dock state is what Rust pushes: closing a panel removes its
/// column, so the studio repaints.
#[test]
fn closing_a_panel_repaints_the_dock() -> TestResult {
    init_backend_allow_existing();
    let open = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
    })?;
    let closed = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
        crate::app::with_app(app, |app| app.dock_close_panel(ui, STUDIO_IMAGE, "layers"));
    })?;
    let painted = differing_pixels(&open, &closed).map_err(ToString::to_string)?;
    assert!(
        painted > 5_000,
        "closing the layers panel repaints the right column ({painted} pixels)"
    );
    Ok(())
}

/// The shipped layout has no bottom row: an empty bottom edge paints nothing
/// and reserves no height, even when its remembered size is large. Revealing a
/// bottom panel is the only thing that paints that row.
#[test]
fn the_default_layout_paints_no_bottom_row() -> TestResult {
    init_backend_allow_existing();
    let default = render_studio(View::Image, "image", |_, _| {})?;
    let oversized = render_studio(View::Image, "image", |ui, app| {
        crate::app::with_app(app, |app| {
            app.image.dock.set_size(DockEdge::Bottom, 420.0);
            app.sync_dock(ui, STUDIO_IMAGE);
        });
    })?;
    assert_eq!(
        differing_pixels(&default, &oversized).map_err(ToString::to_string)?,
        0,
        "an empty bottom edge must not reserve height or paint a splitter"
    );

    let revealed = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["color"]);
    })?;
    let painted = differing_pixels(&default, &revealed).map_err(ToString::to_string)?;
    assert!(
        painted > 5_000,
        "revealing Color paints the bottom row ({painted} pixels)"
    );
    // The default's bottom edge is board canvas; a revealed bottom panel paints
    // the opaque dock surface there instead (no spacer either way). The status
    // bar owns the last 24px of the window, so sample just above it.
    let bottom_pixel = |buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>| {
        let x = buffer.width().saturating_sub(8);
        let y = buffer.height().saturating_sub(30);
        buffer.as_slice()[(y * buffer.width() + x) as usize]
    };
    let card = slint::Color::from_rgb_u8(0x2e, 0x2e, 0x2e);
    let (cr, cg, cb) = (card.red(), card.green(), card.blue());
    let plain = bottom_pixel(&default);
    assert!(
        plain.r != cr || plain.g != cg || plain.b != cb,
        "the default bottom edge is board, not a dock panel"
    );
    let docked = bottom_pixel(&revealed);
    assert!(
        docked.r != plain.r || docked.g != plain.g || docked.b != plain.b,
        "the revealed bottom row occupies the bottom edge"
    );
    Ok(())
}

/// The studio's dock state is what Rust pushes: a hidden panel really leaves
/// the layout, so the column repaints.
#[test]
fn toggling_a_panel_repaints_the_dock() -> TestResult {
    init_backend_allow_existing();
    let open = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
    })?;
    let hidden = render_studio(View::Image, "image", |ui, app| {
        reveal(ui, app, &["layers"]);
        crate::app::with_app(app, |app| {
            let studio = studio(STUDIO_IMAGE);
            if let Some(studio) = studio {
                app.image.dock.toggle(studio, "layers");
            }
            app.sync_dock(ui, STUDIO_IMAGE);
        });
    })?;
    let painted = differing_pixels(&open, &hidden).map_err(ToString::to_string)?;
    assert!(
        painted > 5_000,
        "hiding the layers panel repaints the right column ({painted} pixels)"
    );
    Ok(())
}
