//! Viewport gestures: wheel zoom, pan and the marquee.

use std::collections::BTreeSet;

use open_canvas_core::geometry::{is_node_hidden, is_node_locked};

use crate::coords::{ScreenPos, Viewport, WorldPos};
use crate::doc::{Doc, Drag};

/// Screen-space padding kept around the node bounds by "fit to content".
pub const FIT_PADDING: f64 = 80.0;

impl Doc {
    pub fn zoom(&mut self, anchor: ScreenPos, delta: f64) {
        self.viewport.zoom_at(anchor, delta);
    }

    pub fn begin_pan(&mut self, at: ScreenPos) {
        self.drag = Some(Drag::Pan {
            start: at,
            last: at,
            moved: false,
        });
    }

    /// Returns whether the viewport moved.
    pub fn pan_to(&mut self, at: ScreenPos) -> bool {
        let Some(Drag::Pan { start, last, moved }) = &mut self.drag else {
            return false;
        };
        self.viewport.pan_by(at.x - last.x, at.y - last.y);
        *last = at;
        *moved = *moved || (at.x - start.x).abs() > 3.0 || (at.y - start.y).abs() > 3.0;
        true
    }

    /// Ends the pan; a press that never moved deselects (open-canvas.tsx:227-229).
    pub fn end_pan(&mut self) {
        if let Some(Drag::Pan { moved, .. }) = self.drag.take() {
            if !moved {
                self.selection.clear();
                self.selected_connection = None;
            }
        }
    }

    pub fn begin_marquee(&mut self, at: WorldPos, additive: bool) {
        self.selected_connection = None;
        let initial = if additive {
            self.selection.clone()
        } else {
            BTreeSet::new()
        };
        self.drag = Some(Drag::Marquee {
            start: at,
            current: at,
            additive,
            initial,
        });
    }

    /// Marquee selection by rect intersection over unlocked, visible nodes
    /// (project.tsx:1123-1131).
    pub fn marquee_to(&mut self, at: WorldPos) -> bool {
        let Some(Drag::Marquee {
            start,
            current,
            initial,
            ..
        }) = &mut self.drag
        else {
            return false;
        };
        *current = at;
        let left = start.x.min(at.x);
        let top = start.y.min(at.y);
        let right = start.x.max(at.x);
        let bottom = start.y.max(at.y);
        let mut next = initial.clone();
        for node in &self.nodes {
            if is_node_locked(node) || is_node_hidden(node) {
                continue;
            }
            let hits = left < node.position.x + node.width
                && right > node.position.x
                && top < node.position.y + node.height
                && bottom > node.position.y;
            if hits {
                next.insert(node.id.clone());
            }
        }
        let changed = next != self.selection;
        self.selection = next;
        changed
    }

    pub fn end_marquee(&mut self) {
        if matches!(self.drag, Some(Drag::Marquee { .. })) {
            self.drag = None;
        }
    }

    /// Centers the viewport on a world point at the current zoom
    /// (canvas-mini-map.tsx:85-95).
    pub fn center_on(&mut self, point: WorldPos, canvas: (f64, f64)) {
        self.viewport.x = canvas.0 / 2.0 - point.x * self.viewport.k;
        self.viewport.y = canvas.1 / 2.0 - point.y * self.viewport.k;
    }

    /// `resetViewport` (use-canvas-document.ts:182-184): world origin back at the
    /// canvas center, 100%.
    pub fn reset_viewport(&mut self, canvas: (f64, f64)) {
        self.viewport = Viewport {
            x: canvas.0 / 2.0,
            y: canvas.1 / 2.0,
            k: 1.0,
        };
    }

    /// Fits the visible node bounds into the canvas with a [`FIT_PADDING`]
    /// screen-space margin; an empty document resets the viewport.
    pub fn fit_to_content(&mut self, canvas: (f64, f64)) {
        if canvas.0 <= 1.0 || canvas.1 <= 1.0 {
            self.reset_viewport(canvas);
            return;
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for node in &self.nodes {
            if is_node_hidden(node) {
                continue;
            }
            min_x = min_x.min(node.position.x);
            min_y = min_y.min(node.position.y);
            max_x = max_x.max(node.position.x + node.width);
            max_y = max_y.max(node.position.y + node.height);
        }
        if !min_x.is_finite() {
            self.reset_viewport(canvas);
            return;
        }
        let width = (max_x - min_x).max(1.0);
        let height = (max_y - min_y).max(1.0);
        let k = (canvas.0 / (width + 2.0 * FIT_PADDING))
            .min(canvas.1 / (height + 2.0 * FIT_PADDING))
            .clamp(Viewport::MIN_K, Viewport::MAX_K);
        let center = WorldPos::new(f64::midpoint(min_x, max_x), f64::midpoint(min_y, max_y));
        self.viewport.k = k;
        self.center_on(center, canvas);
    }

    /// World rectangle of the active marquee for rendering.
    pub fn marquee_rect(&self) -> Option<(f64, f64, f64, f64)> {
        match &self.drag {
            Some(Drag::Marquee { start, current, .. }) => Some((
                start.x.min(current.x),
                start.y.min(current.y),
                (current.x - start.x).abs(),
                (current.y - start.y).abs(),
            )),
            _ => None,
        }
    }
}
