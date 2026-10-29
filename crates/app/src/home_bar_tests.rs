//! Home-screen chrome + ruler raster tests. There is no title bar any more, so
//! the home screen's own header carries the window controls; the ruler tests
//! stay here because they render the real pixel buffer.
//!
//! Code-only verification: the Slint testing backend embeds the software
//! rasterizer, so `Window::take_snapshot` returns the real pixel buffer.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::store::Store;
use crate::test_support::init_backend_allow_existing;
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Buffer = slint::SharedPixelBuffer<slint::Rgba8Pixel>;

type WiredApp = (tempfile::TempDir, AppWindow, Rc<RefCell<crate::app::App>>);

fn wired() -> Result<WiredApp, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(crate::app::App::new(Store::at(
        dir.path().to_path_buf(),
    ))));
    crate::events::wire(&ui, &app);
    Ok((dir, ui, app))
}

/// The home screen paints its one flat surface (action row, folder strip and
/// table) through the library screen only; it never mounts the removed title bar.
#[test]
fn the_home_screen_has_no_title_bar() -> TestResult {
    init_backend_allow_existing();
    let (_dir, ui, app) = wired()?;
    crate::app::with_app(&app, |app| {
        app.view = crate::app::View::Library;
        app.sync_all(&ui);
    });
    let app_source = include_str!("../ui/app.slint");
    assert!(
        !app_source.contains("TopBar"),
        "the app mounts no title bar"
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("ui/components/top-bar.slint")
            .exists(),
        "the top-bar source is deleted, not just unmounted"
    );
    assert_eq!(ui.get_view(), "library");
    Ok(())
}

fn ruler_snapshot(pos: f32) -> Result<Buffer, Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (_dir, ui, _app) = wired()?;
    ui.set_view("editor".into());
    ui.set_ruler_y(ModelRc::new(VecModel::from(vec![
        crate::vm::ruler_label_row(
            crate::panels::Tick {
                screen: f64::from(pos),
                value: 1200,
            },
            ui.global::<crate::Theme>().get_node_muted(),
            1.0,
        ),
    ])));
    ui.window().set_size(slint::LogicalSize::new(640.0, 480.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// Two neighbouring fractional tick positions snap to the same whole-pixel box,
/// so the rotated label rasterises to the exact same glyph coverage ??no
/// doubling or thickening as the canvas pans.
#[test]
#[expect(
    clippy::cast_precision_loss,
    reason = "glyph pixel counts of a 640x480 strip are exact in f64"
)]
fn vertical_ruler_labels_snap_to_whole_pixels() -> TestResult {
    let a = ruler_snapshot(100.2)?;
    let b = ruler_snapshot(100.4)?;
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));

    // The left strip (x < 26) around the label is the proof region.
    let width = a.width() as usize;
    let mut differing = 0usize;
    let mut glyphs = 0usize;
    for y in 0..a.height() as usize {
        for x in 0..width.min(26) {
            let index = y * width + x;
            let p = a.as_slice()[index];
            let q = b.as_slice()[index];
            if (p.r, p.g, p.b) != (q.r, q.g, q.b) {
                differing += 1;
            }
            // Glyph pixels are the dark ink against the light strip.
            if p.r < 160 && p.g < 160 && p.b < 160 {
                glyphs += 1;
            }
        }
    }
    assert!(glyphs > 0, "the label actually painted glyphs in the strip");
    assert_eq!(
        differing, 0,
        "the snapped label must rasterise identically at 100.2 and 100.4 ({differing} pixels differ)"
    );

    // A snapped box may translate, but its coverage (glyph pixel count) must be
    // stable: no doubling from a second rasterisation at a fractional offset.
    let c = ruler_snapshot(101.2)?;
    let mut glyphs_c = 0usize;
    for y in 0..c.height() as usize {
        for x in 0..width.min(26) {
            let p = c.as_slice()[y * width + x];
            if p.r < 160 && p.g < 160 && p.b < 160 {
                glyphs_c += 1;
            }
        }
    }
    let ratio = glyphs_c as f64 / glyphs as f64;
    assert!(
        (0.85..=1.15).contains(&ratio),
        "glyph coverage stays stable when the label moves one pixel ({glyphs} vs {glyphs_c})"
    );
    Ok(())
}
