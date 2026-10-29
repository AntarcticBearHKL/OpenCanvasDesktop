//! View-model refresh entry point for the whole window.
//!
//! Every concern keeps its own `impl App` block (rows, wires, view, overlays,
//! chrome) and owns the Slint properties it pushes; this module only owns the
//! refresh order [`App::sync_all`] guarantees after a structural change.

mod chrome;
mod overlays;
mod rows;
pub(crate) mod view;
mod wires;

use crate::app::App;
use crate::AppWindow;

impl App {
    /// Full refresh after a structural document change.
    ///
    /// The order is part of the contract and must not change:
    ///
    /// 1. `sync_rows_full`: rebuilds the node rows; every later stage reads
    ///    those models (hover bar, selection UI, overlays).
    /// 2. `sync_wires`: connection paths and the selected-wire overlay.
    /// 3. `sync_view`: the hot tier (viewport scalars + one redraw) followed by
    ///    the cold tier (rulers, dot-grid tile, dock panel and the
    ///    node-layer popover). Per-tick pan/zoom frames use the tiers directly;
    ///    see `sync/view.rs`.
    /// 4. `sync_overlays`: gestures, drag ghost, hover bar and selection UI; the
    ///    last step that reads the freshly rebuilt rows.
    /// 5. `sync_flags`: toolbar/dock affordances and overlay visibility.
    /// 6. `sync_modals`: modal scalar state.
    /// 7. `sync_panel`: the right-hand side panel.
    /// 8. `sync_plugin`: the plugin-manager scalars.
    /// 9. `sync_library`: library groups and project cards.
    /// 10. `sync_chrome`: screen, title, title-edit draft and the toast stack.
    /// 11. `sync_config`: config tab rows and scalars.
    /// 12. `sync_image`: the Image Studio view model.
    /// 13. `sync_canvas_media`: the canvas media dialogs.
    pub fn sync_all(&mut self, ui: &AppWindow) {
        self.sync_rows_full(ui);
        self.sync_wires(ui);
        self.sync_view(ui);
        self.sync_overlays(ui);
        self.sync_flags(ui);
        self.sync_modals(ui);
        self.sync_panel(ui);
        self.sync_plugin(ui);
        self.sync_library(ui);
        self.sync_chrome(ui);
        self.sync_config(ui);
        self.sync_image(ui);
        self.sync_canvas_media(ui);
    }
}
