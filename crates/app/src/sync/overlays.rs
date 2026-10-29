//! Screen-space overlays. Owns the overlay scalars.

use crate::app::App;
use crate::overlay;
use crate::paths;
use crate::vm;
use crate::AppWindow;

impl App {
    /// Screen-space overlays: snap guides, marquee, active connection, target.
    pub fn sync_overlays(&mut self, ui: &AppWindow) {
        let viewport = self.doc.viewport();
        ui.set_guides(
            paths::guides_commands(self.doc.snap_guides(), self.doc.nodes(), viewport).into(),
        );
        let rect = self.doc.marquee_rect();
        ui.set_marquee_visible(rect.is_some());
        let (x, y, width, height) =
            rect.map_or((0.0, 0.0, 0.0, 0.0), |r| overlay::screen_rect(r, viewport));
        ui.set_marquee_x(vm::len(x));
        ui.set_marquee_y(vm::len(y));
        ui.set_marquee_width(vm::len(width));
        ui.set_marquee_height(vm::len(height));
        ui.set_marquee_border(overlay::marquee_border_commands(rect, viewport).into());
        ui.set_active_wire(
            overlay::active_wire_commands(self.doc.drag(), self.doc.nodes(), viewport).into(),
        );
        ui.set_connect_target_id(self.doc.connect_target().unwrap_or_default().into());
        ui.set_cut_stroke(overlay::cut_stroke_commands(self.doc.cut_points(), viewport).into());
        ui.set_drop_target_id(self.drop_target.clone().unwrap_or_default().into());
        ui.set_drop_slot(self.drop_slot.clone().unwrap_or_default().into());
        // Every gesture passes through here, so the floating bars and modals stay
        // in step with selection/drag state without extra call sites.
        self.sync_hover_bar(ui);
        self.sync_selection_ui(ui);
        self.sync_panel(ui);
    }

    /// Retained for the canvas pointer handlers, which record a legacy pointer
    /// anchor for a node gesture. No drag ghost is painted any more: a dragged
    /// node (and a dragged assets file, which becomes a node at the press
    /// threshold) is its own preview.
    pub fn sync_drag_ghost(&self, _ui: &AppWindow) {
        // Nothing paints a ghost any more: the dragged node itself is the
        // preview. The pointer handlers still record the legacy anchor; this
        // hook only documents that it has no rendering path.
        if self.drag_ghost_pos.is_some() {}
    }
}
