//! Shared helpers for the headless test modules: the per-thread software
//! Slint backend, snapshot pixel diffing, the editor/library render harness and
//! the Image-Studio board fixtures. Test-only — `main.rs` declares the module
//! under `#[cfg(test)]`, so none of this is compiled into the binary.
//!
//! A helper that differs meaningfully from the canonical one (a different
//! store, a different fixture) stays local to its test module under its own
//! name; only true duplicates live here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use open_canvas_ps::{ImageProject, Surface};
use slint::ComponentHandle;

use crate::app::App;
use crate::store::Store;
use crate::AppWindow;

/// A rendered snapshot of a Slint window (RGBA8, software rasterizer).
pub(crate) type Buffer = slint::SharedPixelBuffer<slint::Rgba8Pixel>;

/// Installs the software-rendering testing backend on this thread.
///
/// Fails when a platform is already installed on the thread; test modules that
/// only want a backend, whatever its provenance, use
/// [`init_backend_allow_existing`].
pub(crate) fn init_backend() -> Result<(), Box<dyn Error>> {
    let options = i_slint_backend_testing::TestingBackendOptions {
        mock_time: true,
        threading: false,
        renderer_name: Some(slint::SharedString::from("software")),
    };
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        options,
    )))
    .map_err(|error| format!("platform: {error}"))?;
    Ok(())
}

/// Installs the testing backend unless this thread already has one.
///
/// `set_platform` is thread-local, so every test thread starts clean; a module
/// that installs the backend from several tests nevertheless tolerates an
/// already-installed platform rather than failing the test.
pub(crate) fn init_backend_allow_existing() {
    if init_backend().is_err() {
        // A platform is already installed on this test thread.
    }
}

/// Pixels where two same-size renders differ (alpha is ignored: the snapshots
/// are opaque).
pub(crate) fn differing_pixels(left: &Buffer, right: &Buffer) -> Result<usize, &'static str> {
    if left.width() != right.width() || left.height() != right.height() {
        return Err("snapshots must share a size");
    }
    Ok(left
        .as_slice()
        .iter()
        .zip(right.as_slice())
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b)
        .count())
}

/// Opens a real window on the editor view with a wired app and a private
/// temp-dir store, runs `setup`, and returns the rendered buffer.
pub(crate) fn render_editor(
    setup: impl FnOnce(&AppWindow, &Rc<RefCell<App>>),
) -> Result<Buffer, Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    render_editor_with_store(Store::at(dir.path().to_path_buf()), setup)
}

/// Same as [`render_editor`] with an explicit store: tests that change
/// persisted appearance pass their own store so the shared temp root stays
/// clean.
pub(crate) fn render_editor_with_store(
    store: Store,
    setup: impl FnOnce(&AppWindow, &Rc<RefCell<App>>),
) -> Result<Buffer, Box<dyn Error>> {
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    setup(&ui, &app);
    // Last: sync_all switches the chrome from the library back to the editor.
    ui.set_view("editor".into());
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// A temp-dir-store `App` opened on an Image-Studio board. Each test module
/// keeps its own fixture (board size and pixels differ) and hands it here.
pub(crate) fn app_with_image(
    project: ImageProject,
    bitmaps: HashMap<String, Surface>,
) -> Result<(App, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let mut app = App::new(store);
    app.enter_image(
        project,
        bitmaps,
        crate::image::assets::ImageAssets::default(),
    );
    Ok((app, dir))
}

/// The composite of the open board as RGBA bytes (empty when it has no surface).
pub(crate) fn composite(app: &App) -> Vec<u8> {
    crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
        .map(|surface| surface.data().to_vec())
        .unwrap_or_default()
}
