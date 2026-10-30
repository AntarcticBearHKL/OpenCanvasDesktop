//! Pointer handlers on the shared app: pan/zoom, selection, node drag, resize,
//! connection gestures, wire selection/deletion, the Ctrl+right cut tool and
//! the node-drop bindings. Every position reaching these handlers is in
//! canvas-area coordinates; the conversion to world space happens here.
//!
//! Canvas mouse mapping: left-drag on the empty background moves the window
//! (started from the `.slint` side; the handler only distinguishes a click
//! from a drag so an unmoved press still clears the selection), middle-drag
//! and right-drag pan the viewport, shift+left-drag keeps the marquee, and
//! Ctrl+right starts the wire-cut stroke. Left presses on a node keep their
//! select/drag/resize/connect behaviour — the window move only exists on the
//! background layer.

use crate::app::{home_hold_arc, App, HomeHold, HOME_HOLD_MS, HOME_HOLD_SHOW_AT, HOME_HOLD_SLOP};
use crate::coords::ScreenPos;
use crate::doc::Drag;
use crate::drop_bindings;
use crate::gesture::Corner;
use crate::input::{handle_type, Button, PointerEvent, PointerKind};
use crate::paths;
use crate::AppWindow;

/// The active pointer gesture, mirrored for overlay visibility decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveKind {
    None,
    Pan,
    Nodes,
    Marquee,
    Connect,
    Resize,
    Cut,
}

impl App {
    pub fn active_kind(&self) -> ActiveKind {
        match self.doc.drag() {
            None => ActiveKind::None,
            Some(Drag::Pan { .. }) => ActiveKind::Pan,
            Some(Drag::Nodes { .. }) => ActiveKind::Nodes,
            Some(Drag::Marquee { .. }) => ActiveKind::Marquee,
            Some(Drag::Connect { .. }) => ActiveKind::Connect,
            Some(Drag::Resize(_)) => ActiveKind::Resize,
            Some(Drag::Cut { .. }) => ActiveKind::Cut,
        }
    }

    /// Starts a middle/right-button canvas pan and raises the grabbing cursor.
    fn begin_canvas_pan(&mut self, ui: &AppWindow, pos: ScreenPos) {
        self.doc.begin_pan(pos);
        self.panning = true;
        self.sync_view_hot(ui);
    }

    /// Paints the Zelda-stamina ring at the canvas-local hold position.
    fn show_home_hold(ui: &AppWindow, pos: ScreenPos, progress: f64) {
        ui.set_home_hold_visible(true);
        ui.set_home_hold_x(crate::vm::len(pos.x));
        ui.set_home_hold_y(crate::vm::len(pos.y));
        ui.set_home_hold_progress(crate::vm::len32(progress));
        ui.set_home_hold_arc(home_hold_arc(progress).into());
    }

    /// Drops the still-right hold (a move, a release, a cancel or a new
    /// press) and hides the ring. Idempotent.
    pub fn cancel_home_hold(&mut self, ui: &AppWindow) {
        self.home_hold = None;
        if ui.get_home_hold_visible() {
            ui.set_home_hold_visible(false);
            ui.set_home_hold_progress(0.0);
            ui.set_home_hold_arc("".into());
        }
    }

    /// Advances the still-right hold once per interactive frame. Completing
    /// the ring returns to the library; releasing early leaves the ordinary
    /// short right-click (the side-panel toggle) to the release handler.
    #[expect(
        clippy::cast_precision_loss,
        reason = "elapsed milliseconds are bounded by the 900ms hold window; f64 is exact enough for the ring"
    )]
    pub fn home_hold_tick(&mut self, ui: &AppWindow) {
        let Some(hold) = self.home_hold else {
            return;
        };
        let elapsed = self.now_ms().saturating_sub(hold.started_ms);
        let progress = (elapsed as f64 / HOME_HOLD_MS as f64).clamp(0.0, 1.0);
        if progress < HOME_HOLD_SHOW_AT {
            return;
        }
        if progress < 1.0 {
            Self::show_home_hold(ui, hold.pos, progress);
            return;
        }
        self.home_hold = None;
        ui.set_home_hold_visible(false);
        ui.set_home_hold_progress(0.0);
        ui.set_home_hold_arc("".into());
        // The press has become a navigation gesture: its eventual release
        // must not also toggle the side panel.
        self.right_click_pending = false;
        self.right_click_moved = false;
        self.right_press = None;
        self.canvas_click_pending = false;
        self.panning = false;
        self.doc.end_pan();
        ui.set_canvas_window_drag(false);
        self.go_home(ui);
    }

    /// A left background press that never moved the window: clear the node and
    /// wire selection and paint once, like the old click-on-empty-canvas path.
    fn finish_background_click(&mut self, ui: &AppWindow) {
        let had_wire = self.doc.selected_connection().is_some();
        self.doc.clear_selection();
        self.doc.select_connection(None);
        self.sync_selection(ui);
        if had_wire {
            self.sync_wires(ui);
        }
        // One redraw for the click, through the hot tier's single redraw site
        // (the viewport itself did not move).
        self.sync_view_hot(ui);
    }

    /// Press housekeeping shared by every canvas button: drop a lingering
    /// assets drag, leave prompt-edit mode, dismiss a popover, and reset the
    /// left-click/window-drag classification for the new press.
    fn begin_canvas_press(&mut self, ui: &AppWindow) {
        // A new canvas press means the previous assets drag's release never
        // reached the app (a cancelled row drag can linger). Clear it here
        // instead of dropping it on this unrelated click.
        if self.asset_drag.is_some() {
            self.asset_drag_cancel(ui);
        }
        // A click outside the prompt editor commits it (the typed text was
        // already written through; this only leaves edit mode).
        if self.panel.prompt_editing.is_some() {
            self.panel_prompt_edit_end(ui);
        }
        self.panel_dismiss_popover(ui);
        self.cancel_home_hold(ui);
        self.canvas_click_pending = false;
        self.right_click_pending = false;
        self.right_click_moved = false;
        self.right_press = None;
        ui.set_canvas_window_drag(false);
    }

    /// Ends whichever canvas gesture is active at a pointer release/cancel
    /// (pan, marquee or cut) and paints its final state.
    fn finish_canvas_gesture(&mut self, ui: &AppWindow, pos: ScreenPos) {
        match self.active_kind() {
            ActiveKind::Pan => {
                // Land the exact release position: the pending move may be a
                // frame older than the release point.
                self.pending.pan_to = Some(pos);
                self.apply_pending_view(ui);
                self.doc.end_pan();
                self.panning = false;
                // Exactly one hot sync (and one redraw) for the release.
                self.sync_view_hot(ui);
                self.schedule_cold(self.now_ms());
                self.sync_selection(ui);
            }
            ActiveKind::Marquee => {
                self.doc.end_marquee();
                self.sync_overlays(ui);
            }
            ActiveKind::Cut => {
                if self.doc.end_cut(self.now_ms()) > 0 {
                    self.prune_plugin_state();
                    self.note_doc_change();
                    self.sync_wires(ui);
                    self.sync_flags(ui);
                }
                self.sync_overlays(ui);
            }
            ActiveKind::None | ActiveKind::Nodes | ActiveKind::Connect | ActiveKind::Resize => {}
        }
    }

    /// Samples the native cursor for an in-flight canvas window drag. The
    /// `.slint` layer also ticks on pointer moves and on its 4 ms timer; this
    /// keeps the handler path (and its tests) complete and idempotent.
    fn tick_canvas_window_drag(&mut self, ui: &AppWindow) {
        if self.canvas_click_pending {
            self.drag_tick(ui);
        }
    }

    /// Background pointer: pan, marquee, wire selection, the cut tool,
    /// deselect.
    #[expect(
        clippy::too_many_lines,
        reason = "one match owns every canvas button/gesture transition"
    )]
    pub fn canvas_pointer(&mut self, ui: &AppWindow, event: &PointerEvent) {
        let had_popover = self.panel.popover.is_some();
        if event.kind == PointerKind::Down {
            self.begin_canvas_press(ui);
        }
        match event.kind {
            PointerKind::Down
                if event.button == Button::Right && event.ctrl && self.doc.drag().is_none() =>
            {
                let world = self.doc.viewport().screen_to_world(event.pos);
                self.doc.begin_cut(world);
                self.sync_overlays(ui);
            }
            // Middle-drag and right-drag both pan; Ctrl+right was the cut
            // stroke above, so the plain right button is free for panning.
            PointerKind::Down
                if (event.button == Button::Middle || event.button == Button::Right)
                    && self.doc.drag().is_none() =>
            {
                if event.button == Button::Right {
                    self.right_click_pending = true;
                    self.right_click_moved = false;
                    self.right_press = Some(event.pos);
                    // A press that dismisses a popover is that popover's
                    // "back" gesture; do not also open the side panel.
                    self.right_click_allow_sidebar = !had_popover;
                    if !had_popover {
                        self.home_hold = Some(HomeHold {
                            started_ms: self.now_ms(),
                            pos: event.pos,
                        });
                    }
                }
                self.begin_canvas_pan(ui, event.pos);
            }
            // Left drag on the background moves the window (the `.slint`
            // background layer drives the manual move and its 4 ms cursor
            // timer). Only a press that reaches this handler unclaimed is
            // empty background: a wire still selects on click, and Shift keeps
            // the marquee. A release with no window movement is a click and
            // clears the selection below.
            PointerKind::Down if event.button == Button::Left => {
                if let Some(id) = self
                    .doc
                    .wire_at(self.doc.viewport().screen_to_world(event.pos))
                {
                    self.doc.clear_selection();
                    self.doc.select_connection(Some(&id));
                    self.sync_selection(ui);
                    self.sync_wires(ui);
                    self.sync_flags(ui);
                } else if event.shift {
                    let world = self.doc.viewport().screen_to_world(event.pos);
                    self.doc.begin_marquee(world, true);
                } else {
                    self.canvas_click_pending = true;
                    // The window move starts on this same press. The `.slint`
                    // layer also starts the 4 ms cursor sampler; recording the
                    // anchor here keeps the handler testable and makes the
                    // release's click-vs-drag decision self-contained.
                    self.window_drag_moved = false;
                    self.drag_begin(ui);
                    ui.set_canvas_window_drag(true);
                }
            }
            PointerKind::Down => {}
            // A pan move only records the pointer: the 8 ms frame tick applies
            // the latest position once (`App::view_tick`), so a 1000 Hz mouse
            // costs one viewport application and one redraw per frame instead
            // of one per event.
            PointerKind::Move => match self.active_kind() {
                ActiveKind::Pan => {
                    if self.right_click_pending {
                        if let Some(press) = self.right_press {
                            if (event.pos.x - press.x).abs() > HOME_HOLD_SLOP
                                || (event.pos.y - press.y).abs() > HOME_HOLD_SLOP
                            {
                                self.right_click_moved = true;
                                self.cancel_home_hold(ui);
                            }
                        }
                    }
                    self.note_pan(event.pos);
                }
                ActiveKind::Marquee => {
                    let world = self.doc.viewport().screen_to_world(event.pos);
                    self.doc.marquee_to(world);
                    self.sync_selection(ui);
                    self.sync_overlays(ui);
                }
                ActiveKind::Cut => {
                    let world = self.doc.viewport().screen_to_world(event.pos);
                    if self.doc.cut_to(world) {
                        self.sync_overlays(ui);
                    }
                }
                ActiveKind::None => self.tick_canvas_window_drag(ui),
                ActiveKind::Nodes | ActiveKind::Connect | ActiveKind::Resize => {}
            },
            PointerKind::Up | PointerKind::Cancel => {
                // A left background press that never moved the window is a
                // click, not a drag: it clears the node/wire selection. The
                // window move itself ends in the `.slint` layer either just
                // before or just after this callback; the movement flag is
                // kept across `drag_end`.
                let background_click = self.canvas_click_pending
                    && event.kind == PointerKind::Up
                    && !self.window_drag_moved;
                let canvas_press = self.canvas_click_pending;
                let right_click = self.right_click_pending
                    && event.kind == PointerKind::Up
                    && !self.right_click_moved
                    && self.right_click_allow_sidebar;
                self.cancel_home_hold(ui);
                self.canvas_click_pending = false;
                self.right_click_pending = false;
                self.right_click_moved = false;
                self.right_press = None;
                ui.set_canvas_window_drag(false);
                if canvas_press {
                    // The `.slint` `drag-end` callback stops the 4 ms sampler
                    // right after this handler; dropping the anchor here also
                    // covers a direct (test) call with no `.slint` layer.
                    self.drag_end();
                }
                // A file row released over the canvas: when the row still owns
                // the pointer its own `drag-out` already consumed the drag
                // (asset_drag is None here). When the scrollable tree handed
                // the pointer back, the release only reaches the canvas, so
                // resolve the pending drag at this point before the ordinary
                // up handling.
                self.asset_drag_drop_at(ui, event.pos);
                self.finish_canvas_gesture(ui, event.pos);
                if right_click {
                    // Short right-click toggles the project rail: it opens a
                    // closed panel and closes an open one.
                    self.side_open = !self.side_open;
                    ui.set_side_panel_open(self.side_open);
                    self.persist_ui_state();
                }
                if background_click && self.doc.drag().is_none() {
                    self.finish_background_click(ui);
                }
            }
        }
        // Toolbar affordances only change on a press/release; a move only
        // records input (or updates its own overlays), so a 1000 Hz mouse does
        // not push ten properties per event.
        if event.kind != PointerKind::Move {
            self.sync_flags(ui);
        }
    }

    /// Product multiplier on the web's wheel step (1.0 = web speed).
    pub const WHEEL_ZOOM_SPEED: f64 = 1.5;

    /// Wheel zoom. The delta is accumulated per frame and applied once, at the
    /// latest cursor position, by `App::view_tick`: a burst of wheel events is
    /// one cursor-anchored `zoom_at`, so the point under the cursor stays put
    /// and a zoom in + out returns to the same viewport.
    pub fn canvas_scroll(&mut self, _ui: &AppWindow, pos: ScreenPos, delta_y: f64) {
        // Slint's `delta-y` is winit's `LineDelta.y * 60`: positive when the
        // wheel rolls up, which is the opposite of DOM `deltaY` — so the web's
        // `1.1^(-deltaY/100)` reads `1.1^(delta_y/100)` here. The 1.5 factor is
        // a deliberate product speed-up over the web wheel step.
        self.note_wheel(pos, delta_y * Self::WHEEL_ZOOM_SPEED);
    }

    pub fn canvas_resized(&mut self, ui: &AppWindow, width: f64, height: f64) {
        self.canvas_size = (width.max(1.0), height.max(1.0));
        // A resize is structural: hot + cold in one pass, and the tile lattice
        // is recomputed for the new canvas.
        self.sync_view(ui);
    }

    pub fn node_pointer(&mut self, ui: &AppWindow, id: &str, event: &PointerEvent) {
        // Middle and right always belong to the canvas, even when the pointer
        // is over a node card: the card forwards every button to this handler.
        // Right keeps the pan, the side-panel toggle and the hold-to-home ring.
        if matches!(event.button, Button::Middle | Button::Right) {
            self.canvas_pointer(ui, event);
            return;
        }
        // During a wheel zoom the cards are scaled from a 1x layout; their
        // local coordinates are not canvas coordinates until the cold pass.
        if ui.get_fast_zoom() {
            return;
        }
        // While a canvas pan is live, every move/up belongs to the pan even
        // when Slint reports the move button as `other` (the button is held,
        // but the move event carries no button). Without this the pointer
        // over a node only records the initial press and the pan lands in one
        // jump on release.
        if matches!(
            event.kind,
            PointerKind::Move | PointerKind::Up | PointerKind::Cancel
        ) && matches!(self.doc.drag(), Some(Drag::Pan { .. }))
        {
            self.canvas_pointer(ui, event);
            return;
        }
        match event.kind {
            PointerKind::Down if event.button == Button::Left => {
                self.pending_drag.to = None;
                // A press that reaches the card (never the editor overlay) is a
                // click outside it: commit and leave prompt edit mode.
                if self.panel.prompt_editing.is_some() {
                    self.panel_prompt_edit_end(ui);
                }
                self.panel_dismiss_popover(ui);
                self.doc.select_node(id, event.shift);
                self.sync_selection(ui);
                self.sync_flags(ui);
                self.sync_panel(ui);
                if self.doc.begin_node_drag(id, event.pos) {
                    ui.set_drag_active(true);
                    self.drag_ghost_pos = Some(event.pos);
                    self.sync_drag_ghost(ui);
                }
            }
            // A move only records the latest pointer: the 8 ms frame tick
            // applies the drag once (`App::node_drag_tick`), updates only the
            // dragged rows and the wires touching them, and requests one
            // redraw. Applying the whole row/wire/overlay sync per event is
            // what made a 1000 Hz drag lag.
            PointerKind::Move => {
                if matches!(self.doc.drag(), Some(Drag::Nodes { .. })) {
                    self.note_node_drag(event.pos);
                }
            }
            PointerKind::Up | PointerKind::Cancel => {
                // The last coalesced move may be a frame older than the
                // release: apply the release position exactly before the drop
                // resolves and the drag ends.
                self.pending_drag.to = None;
                if matches!(self.doc.drag(), Some(Drag::Nodes { .. })) {
                    self.doc.node_drag_to(event.pos);
                }
                // A file row released over a card reaches this handler instead
                // of the background's: resolve a pending assets drag at the
                // release point here too (a no-op without one), so a drop aimed
                // at a card still creates or binds where the user let go.
                self.asset_drag_drop_at(ui, event.pos);
                let applied = self.apply_node_drop(event.pos);
                let moved = self.doc.end_node_drag();
                ui.set_drag_active(false);
                if moved && !applied {
                    self.doc.commit(self.now_ms());
                }
                self.drop_target = None;
                self.drop_slot = None;
                self.drag_ghost_pos = None;
                if applied || moved {
                    self.note_doc_change();
                    // The one full rebuild of the gesture.
                    self.sync_all(ui);
                } else {
                    self.sync_overlays(ui);
                }
            }
            PointerKind::Down => {}
        }
    }

    // -----------------------------------------------------------------------
    // Coalesced node gestures
    // -----------------------------------------------------------------------

    /// Records the latest pointer position of a node drag, resize or connect
    /// gesture. No document change, no model write, no redraw: the frame tick
    /// applies it once.
    pub fn note_node_drag(&mut self, pos: ScreenPos) {
        self.pending_drag.to = Some(pos);
    }

    /// One interactive frame of a node gesture: apply the coalesced pointer,
    /// update the dragged rows in place, rebuild only the dirty wire paths and
    /// move their labels, refresh the drag overlays, and request one redraw.
    pub fn node_drag_tick(&mut self, ui: &AppWindow) {
        let Some(pos) = self.pending_drag.to.take() else {
            return;
        };
        if matches!(self.doc.drag(), Some(Drag::Nodes { .. })) {
            if !self.doc.node_drag_to(pos) {
                return;
            }
            self.drag_ghost_pos = Some(pos);
            self.track_drop_target(ui, pos);
            let ids = self.doc.drag_node_ids();
            self.sync_drag_rows(&ids);
            self.sync_drag_wires(ui, &ids);
            self.sync_drag_overlays(ui);
            self.note_drag_frame(ui);
            return;
        }
        let resize_id = match self.doc.drag() {
            Some(Drag::Resize(gesture)) => Some(gesture.node_id.clone()),
            _ => None,
        };
        if let Some(id) = resize_id {
            if !self.doc.resize_to(pos) {
                return;
            }
            let ids = [id];
            self.sync_drag_rows(&ids);
            self.sync_drag_wires(ui, &ids);
            self.sync_drag_overlays(ui);
            self.note_drag_frame(ui);
            return;
        }
        if matches!(self.doc.drag(), Some(Drag::Connect { .. })) {
            let world = self.doc.viewport().screen_to_world(pos);
            self.doc.connect_to(world);
            // The connect gesture draws a screen-space active wire and the
            // target highlight; `sync_overlays` is the one place that owns
            // both, and at one call per frame it is no longer hot.
            self.sync_overlays(ui);
            self.note_drag_frame(ui);
        }
    }

    /// Counts the frame and claims exactly one repaint through the single
    /// redraw site (`sync/view.rs`).
    fn note_drag_frame(&mut self, ui: &AppWindow) {
        self.drag_stats.frames = self.drag_stats.frames.saturating_add(1);
        self.drag_stats.redraws = self.drag_stats.redraws.saturating_add(1);
        Self::request_frame_redraw(ui);
    }

    /// The cheap overlay set for a node drag: snap guides, the drop-target
    /// scalars and the ghost chip. The hover bar and the multi-selection bounds
    /// are anchored to node rects, so they follow the drag too; rulers, wire
    /// labels (their positions are updated separately) and the side panel do not
    /// depend on a drag position and stay off this path.
    fn sync_drag_overlays(&mut self, ui: &AppWindow) {
        let viewport = self.doc.viewport();
        ui.set_guides(
            paths::guides_commands(self.doc.snap_guides(), self.doc.nodes(), viewport).into(),
        );
        ui.set_drop_target_id(self.drop_target.clone().unwrap_or_default().into());
        ui.set_drop_slot(self.drop_slot.clone().unwrap_or_default().into());
        self.sync_drag_ghost(ui);
        // Position-only: building the hover action model on the first drag
        // frame was a visible hitch. The bar already exists from hover; if it
        // does not, it stays hidden until the release sync.
        self.sync_hover_bar_position(ui);
        self.sync_selection_ui(ui);
    }

    /// Recomputes the drop target under the pointer and refreshes the affected
    /// rows' slot highlight so it follows the drag (project.tsx:1074-1080).
    fn track_drop_target(&mut self, _ui: &AppWindow, pos: ScreenPos) {
        let previous = self.drop_target.clone();
        let world = self.doc.viewport().screen_to_world(pos);
        let ids = self.doc.drag_node_ids();
        let target = drop_bindings::resolve_drop_target(&ids, self.doc.nodes(), world);
        self.drop_target = target.as_ref().map(|target| target.node_id.clone());
        self.drop_slot = target
            .and_then(|target| target.slot)
            .map(|slot| slot.id().to_string());
        if previous != self.drop_target {
            let mut refresh: Vec<String> = Vec::new();
            if let Some(id) = previous {
                refresh.push(id);
            }
            if let Some(id) = &self.drop_target {
                refresh.push(id.clone());
            }
            if !refresh.is_empty() {
                // Only the slot-highlight field changes; the card's drop ring
                // is the window-level `drop-target-id` scalar.
                self.sync_slot_drop_rows(&refresh);
            }
        }
    }

    /// Applies the drop resolved at release; the drag state must still be the
    /// node drag that produced it.
    fn apply_node_drop(&mut self, pos: ScreenPos) -> bool {
        let Some(Drag::Nodes { moved, initial, .. }) = self.doc.drag() else {
            return false;
        };
        if !*moved {
            return false;
        }
        let initial = initial.clone();
        let world = self.doc.viewport().screen_to_world(pos);
        let ids = self.doc.drag_node_ids();
        let Some(target) = drop_bindings::resolve_drop_target(&ids, self.doc.nodes(), world) else {
            return false;
        };
        self.doc.apply_drop(&target, &initial, self.now_ms())
    }

    pub fn resize_pointer(&mut self, ui: &AppWindow, id: &str, corner: &str, event: &PointerEvent) {
        match event.kind {
            PointerKind::Down if event.button == Button::Left => {
                if let Some(corner) = Corner::from_id(corner) {
                    // Image nodes are locked to the bitmap's ratio. Normalize
                    // any legacy card before the gesture starts so the first
                    // drag already uses the real image ratio. A locked node is
                    // left untouched: the resize below refuses anyway.
                    let locked = self
                        .doc
                        .node(id)
                        .is_some_and(open_canvas_core::geometry::is_node_locked);
                    if !locked && self.normalize_image_node_geometry(id) {
                        self.note_doc_change();
                        self.sync_rows_of(ui, &[id.to_string()]);
                    }
                    self.doc.begin_resize(id, corner, event.pos);
                }
            }
            PointerKind::Move => {
                if matches!(self.doc.drag(), Some(Drag::Resize(_))) {
                    self.note_node_drag(event.pos);
                }
            }
            PointerKind::Up | PointerKind::Cancel => {
                self.pending_drag.to = None;
                let was_resize = matches!(self.doc.drag(), Some(Drag::Resize(_)));
                // Land the exact release size, not the last coalesced sample.
                if was_resize {
                    self.doc.resize_to(event.pos);
                }
                if self.doc.end_resize() {
                    self.doc.commit(self.now_ms());
                    let ids = [id.to_string()];
                    if was_resize {
                        self.sync_drag_rows(&ids);
                        self.sync_drag_wires(ui, &ids);
                    }
                    self.sync_overlays(ui);
                    self.sync_flags(ui);
                    self.note_doc_change();
                }
            }
            PointerKind::Down => {}
        }
    }

    pub fn connect_pointer(
        &mut self,
        ui: &AppWindow,
        id: &str,
        handle: &str,
        event: &PointerEvent,
    ) {
        let Some(handle) = handle_type(handle) else {
            return;
        };
        match event.kind {
            PointerKind::Down if event.button == Button::Left => {
                self.doc.begin_connect(id, handle);
            }
            PointerKind::Move => {
                if matches!(self.doc.drag(), Some(Drag::Connect { .. })) {
                    self.note_node_drag(event.pos);
                }
            }
            PointerKind::Up | PointerKind::Cancel => {
                self.pending_drag.to = None;
                // Land the exact release point before the connection resolves.
                if matches!(self.doc.drag(), Some(Drag::Connect { .. })) {
                    let world = self.doc.viewport().screen_to_world(event.pos);
                    self.doc.connect_to(world);
                }
                if self.doc.end_connect(self.now_ms()) {
                    self.sync_wires(ui);
                    self.sync_flags(ui);
                    self.note_doc_change();
                } else if let Some(reason) = self.doc.take_connect_rejection() {
                    // Visible rejection: dropping a prompt on the wrong
                    // generator (or the reverse) must never be a silent no-op.
                    self.set_notice(ui, reason, true);
                }
                self.sync_overlays(ui);
            }
            PointerKind::Down => {}
        }
    }
}
