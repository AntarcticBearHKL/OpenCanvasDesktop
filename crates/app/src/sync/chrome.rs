//! Chrome sync: which screen shows, the toast stack and the OS maximise state.
//! There is no window title bar any more: the home screen carries the window
//! controls, the canvas moves with a right-drag and the studio has its own menu.

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::app::{App, View};
use crate::AppWindow;

impl App {
    /// Navigation chrome: which screen shows and the toast stack.
    pub fn sync_chrome(&self, ui: &AppWindow) {
        ui.set_view(
            match self.view {
                View::Library => "library",
                View::Editor => "editor",
                View::Image => "image",
            }
            .into(),
        );
        self.sync_notice(ui);
    }

    /// Pushes the toast stack into the window. The stack is the whole rendering
    /// contract (`app.slint` `notices`), so the auto-dismiss and the dismiss
    /// button are one source of truth: this list.
    pub fn sync_notice(&self, ui: &AppWindow) {
        let rows: Vec<crate::ToastRow> = self
            .notices
            .iter()
            .map(|notice| crate::ToastRow {
                id: i32::try_from(notice.id).unwrap_or(i32::MAX),
                text: notice.text.as_str().into(),
                error: notice.error,
            })
            .collect();
        ui.set_notices(ModelRc::new(VecModel::from(rows)));
    }

    /// Re-reads the OS maximise state (a Win+Up or a title-bar double-click by
    /// the window manager); called from the refresh loop, never from a test.
    pub fn sync_window_state(&mut self, ui: &AppWindow) {
        let maximized = ui.window().is_maximized();
        if maximized != self.window_maximized {
            self.window_maximized = maximized;
            ui.set_window_maximized(maximized);
        }
    }
}
