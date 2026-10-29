//! Shared helpers for the settings/appearance test modules: a per-thread
//! software-rendering Slint backend, a temp-dir `Store`, and a bare `AppWindow`
//! with an installed `App`. The backend lives in [`crate::test_support`] with
//! the other snapshot helpers; this module adds the settings-specific pieces.

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use crate::app::App;
use crate::store::Store;
use crate::AppWindow;

pub(crate) use crate::test_support::init_backend;

pub fn temp_store() -> Result<(tempfile::TempDir, Store), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let store = Store::at(dir.path().to_path_buf());
    Ok((dir, store))
}

/// A window + app wired like `main.rs` does, for tests that drive the real
/// callbacks (`events::wire` installs the models and the 60 ms timer).
pub type WiredApp = (AppWindow, Rc<RefCell<App>>);

pub fn new_wired_app(store: Store) -> Result<WiredApp, Box<dyn Error>> {
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    Ok((ui, app))
}
