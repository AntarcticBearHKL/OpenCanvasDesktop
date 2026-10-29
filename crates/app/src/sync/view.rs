//! Viewport sync, in three tiers, driven by the coalesced input tick.
//!
//! A pan or zoom used to sync **everything** per input event: viewport scalars,
//! ruler ticks and label bitmaps, a regenerated dot-grid path,
//! the dock panel and the node layer. A 1000 Hz mouse therefore did that work
//! 1000 times per second, and every one of those passes claimed a frame.
//!
//! Now pointer/wheel events only record the pending viewport (`App::note_pan` /
//! `App::note_wheel`) and the 8 ms frame tick applies it once:
//!
//! * **hot** ([`App::sync_view_hot`]) — the viewport scalars the canvas paints
//!   from, the zoom read-out and exactly one `request_redraw`. Nothing else.
//!   A wheel burst runs **only** this tier per frame; its accumulated delta is
//!   one cursor-anchored `zoom_at` application.
//! * **follow** ([`App::sync_view_follow`]) — the screen-space layers that must
//!   move with the viewport but tolerate a lower cadence: ruler ticks/label
//!   images and the gesture overlays. A pan runs it at ~60 Hz while
//!   the gesture is live; a wheel gesture skips it entirely.
//! * **cold** ([`App::sync_view_cold`]) — everything that only has to be exact
//!   once the view settles: rulers again, the dot-grid tile and its
//!   lattice, the dock panel and the node-layer popover. It runs
//!   [`VIEW_COLD_SETTLE_MS`] after the last input (and on structural changes,
//!   where [`App::sync_view`] runs hot + cold together). For a wheel burst this
//!   is the single ruler/panel pass after the gesture.
//!
//! Everything else that reads the viewport (node cards, wires, selection) is
//! bound to `scale` / `vp-x` / `vp-y` in `app.slint`, so the hot tier already
//! moves it.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{App, GRID_ZOOM_RATIO};
use crate::coords::ScreenPos;
use crate::grid_tile::GridTile;
use crate::panels;
use crate::paths;
use crate::vm;
use crate::{AppWindow, RulerTick, RulerTickImage};

/// How long after the last input the cold tier runs. Long enough that a burst
/// of wheel events never pays for it mid-gesture, short enough that the rulers
/// catch up before the user notices they lagged.
pub const VIEW_COLD_SETTLE_MS: u64 = 100;
/// Cadence of the follow tier while a gesture is live (60 Hz against the 125 Hz
/// tick, i.e. every other tick).
pub const VIEW_FOLLOW_MS: u64 = 16;

impl App {
    // -----------------------------------------------------------------------
    // Input recording (per event) — no viewport work, no redraw
    // -----------------------------------------------------------------------

    /// Records the latest pointer position of an in-flight pan.
    pub fn note_pan(&mut self, pos: ScreenPos) {
        let now = self.now_ms();
        self.note_pan_at(pos, now);
    }

    /// [`App::note_pan`] with an explicit clock (the frame-budget tests drive
    /// the tick deterministically).
    pub fn note_pan_at(&mut self, pos: ScreenPos, now: u64) {
        self.pending.pan_to = Some(pos);
        // A pan is happy with the 60 Hz follow tier; a wheel burst (which may
        // have arrived just before) stays hot-only until its cold pass.
        self.pending.hot_only = false;
        self.schedule_cold(now);
    }

    /// Accumulates one wheel event: the delta sums up, the anchor is the latest
    /// cursor position. One tick then calls `zoom_at(cursor, delta)` once.
    pub fn note_wheel(&mut self, pos: ScreenPos, delta: f64) {
        let now = self.now_ms();
        self.note_wheel_at(pos, delta, now);
    }

    /// [`App::note_wheel`] with an explicit clock.
    pub fn note_wheel_at(&mut self, pos: ScreenPos, delta: f64, now: u64) {
        match &mut self.pending.wheel {
            Some(wheel) => {
                wheel.cursor = pos;
                wheel.delta += delta;
            }
            None => {
                self.pending.wheel = Some(crate::app::WheelZoom { cursor: pos, delta });
            }
        }
        self.pending.hot_only = true;
        self.schedule_cold(now);
    }

    /// Defers the cold tier to [`VIEW_COLD_SETTLE_MS`] after `now`. The gesture
    /// end calls this too, so the release frame is followed by one cold sync.
    pub fn schedule_cold(&mut self, now: u64) {
        self.pending.cold_due = Some(now.saturating_add(VIEW_COLD_SETTLE_MS));
    }

    /// Applies the coalesced viewport input, if any. Returns whether anything
    /// was applied; the caller decides whether that is a frame (the tick does
    /// exactly one hot sync + redraw per application).
    pub(crate) fn apply_pending_view(&mut self, ui: &AppWindow) -> bool {
        let now = self.now_ms();
        let pan = self.pending.pan_to.take();
        let wheel = self.pending.wheel.take();
        if pan.is_none() && wheel.is_none() {
            return false;
        }
        if let Some(pos) = pan {
            self.doc.pan_to(pos);
        }
        let zooming = wheel.is_some();
        if let Some(zoom) = wheel {
            // The accumulated delta is one zoom step at the latest cursor: the
            // world point under the cursor does not move (Viewport::zoom_at).
            self.doc.zoom(zoom.cursor, zoom.delta);
            if zoom.delta != 0.0 {
                self.note_zoom_change(ui);
            }
        }
        // A wheel burst keeps the tick on the hot tier until the cold pass;
        // a pure pan continues to run the 60 Hz follow tier. The `note_pan`
        // call already cleared the flag, so this is the only place that turns
        // it back on for a mixed batch.
        self.pending.hot_only = zooming;
        self.schedule_cold(now);
        self.view_stats.applications = self.view_stats.applications.saturating_add(1);
        // One autosave mark per frame, not per event (500 ms debounce).
        self.note_view_change();
        true
    }

    // -----------------------------------------------------------------------
    // The frame tick
    // -----------------------------------------------------------------------

    /// One interactive frame: apply the coalesced input (at most one viewport
    /// application and exactly one redraw), then run the follow/cold tiers on
    /// their own cadence.
    pub fn view_tick(&mut self, ui: &AppWindow) {
        let now = self.now_ms();
        self.view_tick_at(ui, now);
    }

    /// [`App::view_tick`] with an explicit clock.
    pub fn view_tick_at(&mut self, ui: &AppWindow, now: u64) {
        // Idle tick: no pending input and no deferred tier. This is the common
        // case (the loop runs at 125 Hz even when nobody touches the canvas).
        if !self.pending.has_input() && self.pending.cold_due.is_none() {
            return;
        }
        if self.apply_pending_view(ui) {
            // The one redraw of this frame. Node cards and wires follow the
            // scalars through their `.slint` bindings; nothing else is touched.
            self.sync_view_hot(ui);
            // A wheel burst is hot-only: rulers, panels and the node
            // layer wait for the cold pass, exactly like a zoom that is still
            // settling. A pan keeps the 60 Hz follow tier.
            if !self.pending.hot_only && now >= self.pending.follow_at {
                self.sync_view_follow(ui);
                self.pending.follow_at = now.saturating_add(VIEW_FOLLOW_MS);
            }
            return;
        }
        // No new input this frame: the cold tier lands once the gesture has
        // been quiet for VIEW_COLD_SETTLE_MS; until then the follow tier keeps
        // the rulers at 60 Hz for a pan. A wheel gesture stays hot-only until
        // that cold pass, so its ruler work happens exactly once.
        if self.pending.cold_due.is_some_and(|due| now >= due) {
            self.sync_view_cold(ui);
            self.pending.cold_due = None;
            self.pending.hot_only = false;
            return;
        }
        if !self.pending.hot_only
            && self.pending.cold_due.is_some()
            && now >= self.pending.follow_at
        {
            self.sync_view_follow(ui);
            self.pending.follow_at = now.saturating_add(VIEW_FOLLOW_MS);
        }
    }

    // -----------------------------------------------------------------------
    // The tiers
    // -----------------------------------------------------------------------

    /// **Hot tier**: the viewport scalars the canvas paints from, the zoom
    /// read-out, and exactly one redraw request.
    ///
    /// This is the whole per-frame cost of a pan or zoom, besides the input
    /// application itself. It must never grow: rulers and the grid live
    /// in the follow/cold tiers below.
    pub fn sync_view_hot(&mut self, ui: &AppWindow) {
        let viewport = self.doc.viewport();
        ui.set_scale(vm::len(viewport.k));
        ui.set_vp_x(vm::len(viewport.x));
        ui.set_vp_y(vm::len(viewport.y));
        let label = format!("{:.0}%", viewport.k * 100.0);
        if ui.get_zoom_label() != label.as_str() {
            ui.set_zoom_label(SharedString::from(label));
        }
        if ui.get_panning() != self.panning {
            ui.set_panning(self.panning);
        }
        let fast = self.pending.hot_only;
        if ui.get_fast_zoom() != fast {
            ui.set_fast_zoom(fast);
        }
        self.view_stats.hot_passes = self.view_stats.hot_passes.saturating_add(1);
        self.view_stats.redraws = self.view_stats.redraws.saturating_add(1);
        Self::request_frame_redraw(ui);
    }

    /// The one `request_redraw` call site for interactive frames. The viewport
    /// hot tier and the coalesced node-gesture tick (`handlers.rs`) both paint
    /// exactly the frame they just built, so both go through this helper; no
    /// other module calls `request_redraw`.
    pub(crate) fn request_frame_redraw(ui: &AppWindow) {
        ui.window().request_redraw();
    }

    /// **Follow tier** (~60 Hz during a gesture): the screen-space layers that
    /// must track the viewport without blocking the frame — ruler ticks and
    /// label bitmaps, and the gesture overlays (marquee, hover bar, selection
    /// bounds are all positioned from the viewport).
    pub fn sync_view_follow(&mut self, ui: &AppWindow) {
        self.view_stats.follow_passes = self.view_stats.follow_passes.saturating_add(1);
        self.sync_rulers(ui);
        self.sync_overlays(ui);
    }

    /// **Cold tier**: exact once the view settles. Rulers again (so the last
    /// follow tick's staleness is gone), the dot-grid tile, the dock panel and
    /// the node-layer popover.
    pub fn sync_view_cold(&mut self, ui: &AppWindow) {
        self.view_stats.cold_passes = self.view_stats.cold_passes.saturating_add(1);
        if ui.get_fast_zoom() {
            ui.set_fast_zoom(false);
        }
        self.sync_rulers(ui);
        self.sync_grid(ui);
        self.sync_panel(ui);
        self.sync_node_layer(ui);
    }

    /// Full viewport sync: hot + cold. Structural changes (`sync_all`, a
    /// resize, `Ctrl+0`/`Ctrl+1`) go through here.
    pub fn sync_view(&mut self, ui: &AppWindow) {
        self.sync_view_hot(ui);
        self.sync_view_cold(ui);
        self.pending.hot_only = false;
    }

    /// Ruler ticks along both axes, plus the pre-rotated label bitmaps.
    fn sync_rulers(&mut self, ui: &AppWindow) {
        self.view_stats.ruler_passes = self.view_stats.ruler_passes.saturating_add(1);
        let viewport = self.doc.viewport();
        let (ticks_x, ticks_y) =
            panels::ruler_ticks(viewport, self.canvas_size.0, self.canvas_size.1);
        let x_rows: Vec<RulerTick> = ticks_x.into_iter().map(vm::ruler_tick_row).collect();
        let ink = ui.global::<crate::Theme>().get_node_muted();
        let scale = ui.window().scale_factor();
        let y_rows: Vec<RulerTickImage> = ticks_y
            .into_iter()
            .map(|tick| vm::ruler_label_row(tick, ink, scale))
            .collect();
        self.ruler_x.set_vec(x_rows);
        self.ruler_y.set_vec(y_rows);
    }

    /// The canvas dot grid.
    ///
    /// Shipped path: one pre-rendered tile (`grid_tile`) repeated by a lattice
    /// of `Image` elements. The grid is screen-anchored, so the lattice
    /// geometry (spacing, tile edge, tile count) is the same at every zoom and
    /// panning never reaches this function at all (the offsets are
    /// `Math.mod(vp, tile)` bindings in `app.slint`). The *bitmap* is built once
    /// and only rebuilt when the theme ink or the device scale changes: the key
    /// has no viewport in it.
    ///
    /// `OC_LEGACY_GRID=1` switches back to the old world-space `Path` (kept for
    /// the before/after measurement in `perf_tests.rs`).
    fn sync_grid(&mut self, ui: &AppWindow) {
        let viewport = self.doc.viewport();
        if self.legacy_grid {
            ui.set_legacy_grid(true);
            let min_x = -viewport.x / viewport.k;
            let min_y = -viewport.y / viewport.k;
            let max_x = min_x + self.canvas_size.0 / viewport.k;
            let max_y = min_y + self.canvas_size.1 / viewport.k;
            let ratio = (self.grid_k / viewport.k).max(viewport.k / self.grid_k);
            if !self.grid.covers(min_x, min_y, max_x, max_y) || ratio > GRID_ZOOM_RATIO {
                let patch = paths::grid_patch(viewport, self.canvas_size.0, self.canvas_size.1);
                ui.set_grid(SharedString::from(patch.commands.clone()));
                self.grid = patch;
                self.grid_k = viewport.k;
            }
            return;
        }
        ui.set_legacy_grid(false);
        let layout = paths::grid_tile_layout(self.canvas_size.0, self.canvas_size.1);
        let ink = ui.global::<crate::Theme>().get_canvas_dot();
        let device_scale = f64::from(ui.window().scale_factor());
        let key = crate::grid_tile::GridTileKey::of(&layout, device_scale, ink);
        if self.grid_tile.as_ref().is_none_or(|tile| tile.key != key) {
            self.grid_tile = GridTile::build(&layout, device_scale, ink);
            if let Some(tile) = &self.grid_tile {
                ui.set_grid_tile(tile.image.clone());
            }
        }
        // Constant screen spacing: the lattice has the same geometry at every
        // zoom (only the tile count follows the canvas size).
        ui.set_grid_step(vm::len(layout.spacing));
        ui.set_grid_tile_w(vm::len(layout.tile));
        ui.set_grid_tile_h(vm::len(layout.tile));
        let cols = i32::try_from(layout.cols.max(1)).unwrap_or(i32::MAX);
        let rows = i32::try_from(layout.rows.max(1)).unwrap_or(i32::MAX);
        if ui.get_grid_cols() != cols || ui.get_grid_rows() != rows {
            ui.set_grid_cols(cols);
            ui.set_grid_rows(rows);
            let tiles = i32::try_from(layout.tiles()).unwrap_or(i32::MAX);
            ui.set_grid_indices(ModelRc::new(VecModel::from(
                (0..tiles).collect::<Vec<i32>>(),
            )));
        }
    }

    /// Dock/toolbar affordances and overlay visibility.
    pub fn sync_flags(&self, ui: &AppWindow) {
        ui.set_can_undo(self.doc.can_undo());
        ui.set_can_redo(self.doc.can_redo());
        ui.set_space_held(self.space_held);
        ui.set_panning(self.panning);
        ui.set_zoom_label(SharedString::from(format!(
            "{:.0}%",
            self.doc.viewport().k * 100.0
        )));
        ui.set_shortcuts_open(self.shortcuts_open);
        ui.set_side_panel_open(self.side_open);
        ui.set_side_panel_width(vm::len(self.side_width));
        ui.set_window_maximized(self.window_maximized);
    }
}
