//! Image Studio tool state machine: pointer gestures (move/scale, brush,
//! eraser, bucket, gradient, shape, marquee/ellipse/lasso/polygon/wand/quick
//! select, pen/direct-select, eyedropper, pan), wheel zoom and keyboard
//! shortcuts - the Rust half of `image-studio.tsx`'s event handlers.
//! Owns the tool gesture state machine and the cursor/drag state it drives.

use open_canvas_ps::selection::image_color_at;
use open_canvas_ps::{
    layer_box, selection_polygon, selection_quick, selection_rect, selection_wand, Bounds,
    CanvasPsShapeKind, Point, Rgba, SelectionMode, Surface,
};

use crate::app::{App, HomeHold, View, HOME_HOLD_SLOP};
use crate::coords::ScreenPos;
use crate::image::{Gesture, ImageTool};
use crate::input::{Button, PointerKind};
use crate::vm;
use crate::AppWindow;

/// Corner hit radius in screen pixels (transform handles).
const HANDLE_HIT: f64 = 8.0;
/// Arm-length threshold for a pen anchor drag.
const ANCHOR_HIT: f64 = 8.0;

/// A hit on a path anchor (part 0) or one of its handles (1 in / 2 out).
struct PathHit {
    path: String,
    index: usize,
    part: u8,
}

/// Distance from a document point to a line segment; used for path-stroke
/// hit-testing at the same screen-space tolerance as anchors.
fn distance_to_segment(point: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        return (point.x - a.x).hypot(point.y - a.y);
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_sq).clamp(0.0, 1.0);
    let x = a.x + t * dx;
    let y = a.y + t * dy;
    (point.x - x).hypot(point.y - y)
}

/// A stroke is a chain of brush dabs; this caps the per-move segment work.
const DAB_SPACING_RATIO: f64 = 0.35;

impl App {
    /// `docPoint`: screen -> document coordinates.
    pub fn image_doc_point(&self, x: f64, y: f64) -> Point {
        let view = self.image.view;
        Point::new((x - view.x) / view.k, (y - view.y) / view.k)
    }

    /// `fit`: 90% of the canvas, floored to whole pixels when zooming in,
    /// clamped at [`crate::image::ZOOM_MIN`] when zooming out; centered.
    pub fn image_fit(&mut self) {
        let (cw, ch) = self.image.canvas;
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
        let (width, height) = self.image.board();
        let k0 = (cw / width.max(1.0)).min(ch / height.max(1.0)) * 0.9;
        let k = if k0 >= 1.0 {
            (k0.floor()).max(1.0)
        } else {
            k0.max(crate::image::ZOOM_MIN)
        };
        self.image.view = crate::image::ImageView {
            k,
            x: (cw - width * k) / 2.0,
            y: (ch - height * k) / 2.0,
        };
    }

    /// Auto-fit once per project + board size (the web's `fittedRef` key).
    pub fn image_auto_fit(&mut self) {
        let key = self.image.meta.as_ref().map(|meta| {
            format!(
                "{}:{}x{}",
                meta.id, self.image.project.width, self.image.project.height
            )
        });
        let Some(key) = key else {
            return;
        };
        if self.image.fitted == key {
            return;
        }
        let (cw, ch) = self.image.canvas;
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
        self.image.fitted = key;
        self.image_fit();
    }

    /// `setZoom`: clamped, centered.
    pub fn image_set_zoom(&mut self, k: f64) {
        let (cw, ch) = self.image.canvas;
        let k = k.clamp(crate::image::ZOOM_MIN, crate::image::ZOOM_MAX);
        let (width, height) = self.image.board();
        self.image.view = crate::image::ImageView {
            k,
            x: (cw - width * k) / 2.0,
            y: (ch - height * k) / 2.0,
        };
    }

    /// `zoomAt`: keep the point under the cursor fixed.
    pub fn image_zoom_at(&mut self, x: f64, y: f64, factor: f64) {
        let view = self.image.view;
        let k = (view.k * factor).clamp(crate::image::ZOOM_MIN, crate::image::ZOOM_MAX);
        let ratio = k / view.k;
        self.image.view = crate::image::ImageView {
            k,
            x: x - (x - view.x) * ratio,
            y: y - (y - view.y) * ratio,
        };
    }

    /// Wheel zoom at the cursor (`ZOOM_STEP` in / 1/step out).
    ///
    /// Slint's `delta-y` is positive when the wheel rolls **up**, so wheel-up
    /// must zoom in — the same direction as the canvas workspace
    /// (`App::canvas_scroll`'s `1.1^(delta_y/100)`). The studio used the DOM
    /// sign here, which inverted the wheel against every other surface.
    pub fn image_wheel(&mut self, x: f64, y: f64, delta_y: f64) {
        let factor = if delta_y > 0.0 {
            crate::image::ZOOM_STEP
        } else {
            1.0 / crate::image::ZOOM_STEP
        };
        self.image_zoom_at(x, y, factor);
    }

    /// The Slint callback wrapper: apply the wheel zoom, then repaint.
    pub fn image_scroll(&mut self, ui: &AppWindow, x: f64, y: f64, delta_y: f64) {
        self.image_wheel(x, y, delta_y);
        self.sync_image(ui);
    }

    /// One pointer event on the board (the Slint callback entry point).
    #[expect(
        clippy::too_many_arguments,
        reason = "the Slint pointer callback payload (kind, x, y, button, shift, alt) is forwarded verbatim; grouping it would only add a wrapper struct"
    )]
    pub fn image_pointer(
        &mut self,
        ui: &AppWindow,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: Button,
        shift: bool,
        alt: bool,
    ) {
        // A click on the board outside the overlay commits the live edit first.
        if kind == PointerKind::Down && self.image.text_edit.is_editing() {
            self.image_text_edit_commit(ui, None);
        }
        if self.image_smart_canvas_pointer(ui, kind, x, y, button) {
            self.sync_image(ui);
            return;
        }
        self.image_pointer_core(kind, x, y, button, shift, alt);
        if std::mem::take(&mut self.image.auto_layer_hint) {
            self.set_notice(
                ui,
                "Painting on a new pixel layer (the active layer can't hold pixels).".to_string(),
                false,
            );
        }
        self.sync_image(ui);
    }

    /// Smart Canvas boards are a viewer, not a painting surface: left-drag
    /// pans, the wheel zooms, and right-hold returns to the origin canvas.
    /// Returning true means the event was consumed here.
    fn image_smart_canvas_pointer(
        &mut self,
        ui: &AppWindow,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: Button,
    ) -> bool {
        if self.image.origin_canvas.is_none() {
            return false;
        }
        match kind {
            PointerKind::Down if button == Button::Right => {
                self.home_hold = Some(HomeHold {
                    started_ms: self.now_ms(),
                    pos: ScreenPos::new(x, y),
                });
                true
            }
            PointerKind::Move => {
                if let Some(hold) = self.home_hold {
                    if (x - hold.pos.x).abs() > HOME_HOLD_SLOP
                        || (y - hold.pos.y).abs() > HOME_HOLD_SLOP
                    {
                        self.cancel_home_hold(ui);
                    }
                }
                self.home_hold.is_some()
            }
            PointerKind::Up | PointerKind::Cancel => {
                if self.home_hold.is_some() {
                    self.cancel_home_hold(ui);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The gesture state machine without a window: unit tests drive this
    /// directly, the callback wrapper above repaints once per event.
    pub fn image_pointer_core(
        &mut self,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: Button,
        shift: bool,
        alt: bool,
    ) {
        match kind {
            PointerKind::Down => self.image_pointer_down(x, y, button, shift, alt),
            PointerKind::Move => self.image_pointer_move(x, y, shift),
            PointerKind::Up | PointerKind::Cancel => self.image_pointer_up(),
        }
    }

    /// `PsSelectionMode` from the modifiers (shift add, alt subtract, both
    /// intersect; replace otherwise - image-studio.tsx:518-591).
    fn selection_mode(shift: bool, alt: bool) -> SelectionMode {
        match (shift, alt) {
            (true, true) => SelectionMode::Intersect,
            (true, false) => SelectionMode::Add,
            (false, true) => SelectionMode::Subtract,
            (false, false) => SelectionMode::Replace,
        }
    }

    fn image_pointer_down(&mut self, x: f64, y: f64, button: Button, shift: bool, alt: bool) {
        if button == Button::Middle || self.image.space_held {
            self.image.gesture = Some(Gesture::Pan {
                start_x: x,
                start_y: y,
                view: self.image.view,
            });
            return;
        }
        if button != Button::Left {
            return;
        }
        // Hand owns both halves of the canvas drag: a press on the active layer
        // grabs it (scale corners included), anything else pans the view. An
        // armed transform's handles win outright: the rotate handle sits above
        // the layer box. The right button never reaches this: it moves the
        // window.
        if self.image.tool == ImageTool::Hand {
            let point = self.image_doc_point(x, y);
            if !self.image_transform_begin((x, y)) {
                // Hand selects the topmost object under the pointer and can
                // drag that object. An empty board press does nothing: the
                // left button never pans the canvas.
                if let Some(layer) = self.layer_at(point) {
                    self.image.active_layer = layer;
                    self.image_move_start(x, y, point);
                }
            }
            return;
        }
        let point = self.image_doc_point(x, y);
        match self.image.tool {
            ImageTool::Eyedropper => {
                if let Some(hex) = self.sample_board_color(point) {
                    self.image.color = hex;
                }
            }
            ImageTool::Marquee | ImageTool::EllipseMarquee => {
                self.image.gesture = Some(Gesture::Marquee {
                    tool: self.image.tool,
                    start: point,
                    current: point,
                    shift,
                    alt,
                });
            }
            ImageTool::Lasso => {
                self.image.gesture = Some(Gesture::Lasso {
                    points: vec![point],
                });
            }
            ImageTool::PolygonLasso => {
                self.image.poly_points.push(point);
            }
            ImageTool::Wand => {
                self.apply_wand(point, shift, alt);
            }
            ImageTool::QuickSelect => {
                self.apply_quick_select(point, alt);
            }
            ImageTool::Brush | ImageTool::Eraser => {
                self.ensure_paint_layer();
                self.begin_paint();
                let layer = self.image.active_layer_id();
                let erase = self.paint_erase();
                self.image.gesture = Some(Gesture::Paint {
                    layer,
                    erase,
                    last: point,
                });
            }
            ImageTool::Bucket => {
                self.ensure_paint_layer();
                self.apply_bucket(point);
            }
            ImageTool::Gradient => {
                self.ensure_paint_layer();
                self.image.gesture = Some(Gesture::Gradient {
                    start: point,
                    current: point,
                });
            }
            ImageTool::Shape => {
                self.image.gesture = Some(Gesture::Shape {
                    start: point,
                    current: point,
                });
            }
            ImageTool::Pen => self.pen_down(point),
            ImageTool::DirectSelect => self.anchor_down(point),
            ImageTool::Hand => {}
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per gesture; splitting the match would scatter the drag state machine"
    )]
    fn image_pointer_move(&mut self, x: f64, y: f64, shift: bool) {
        let Some(gesture) = self.image.gesture.take() else {
            return;
        };
        let point = self.image_doc_point(x, y);
        match gesture {
            Gesture::Pan {
                start_x,
                start_y,
                view,
            } => {
                self.image.view = crate::image::ImageView {
                    x: view.x + (x - start_x),
                    y: view.y + (y - start_y),
                    k: view.k,
                };
                self.image.gesture = Some(Gesture::Pan {
                    start_x,
                    start_y,
                    view,
                });
            }
            Gesture::Move {
                layer,
                start,
                x: ox,
                y: oy,
            } => {
                let dx = point.x - start.x;
                let dy = point.y - start.y;
                let (next_x, next_y) = self.snap_move(&layer, ox + dx, oy + dy);
                // `translate_ps_layer` takes a delta; the gesture tracks the
                // layer's absolute target, so translate from wherever the
                // previous move left it (repeated moves must not compound).
                let (current_x, current_y) = open_canvas_ps::layer_ops::find_ps_layer(
                    &self.image.project.board_layers,
                    &layer,
                )
                .map_or((ox, oy), |current| (current.x, current.y));
                self.image.project.board_layers = open_canvas_ps::translate_ps_layer(
                    &self.image.project.board_layers,
                    &layer,
                    next_x - current_x,
                    next_y - current_y,
                );
                self.image.gesture = Some(Gesture::Move {
                    layer,
                    start,
                    x: ox,
                    y: oy,
                });
            }
            Gesture::Scale {
                layer,
                corner,
                start,
                x: ox,
                y: oy,
                width,
                height,
            } => {
                let (sx, sy) = match corner {
                    0 => (-1.0, -1.0),
                    1 => (1.0, -1.0),
                    2 => (1.0, 1.0),
                    _ => (-1.0, 1.0),
                };
                let dx = (point.x - start.x) * sx;
                let dy = (point.y - start.y) * sy;
                let factor = (1.0 + (dx + dy) / (width + height).max(1.0)).clamp(0.05, 20.0);
                let centre = Point::new(ox + width / 2.0, oy + height / 2.0);
                let new_w = (width * factor).max(1.0);
                let new_h = (height * factor).max(1.0);
                self.image.project.board_layers = open_canvas_ps::patch_ps_layer(
                    &self.image.project.board_layers,
                    &layer,
                    |target| {
                        target.width = new_w;
                        target.height = new_h;
                        target.x = centre.x - new_w / 2.0;
                        target.y = centre.y - new_h / 2.0;
                    },
                );
                self.image.gesture = Some(Gesture::Scale {
                    layer,
                    corner,
                    start,
                    x: ox,
                    y: oy,
                    width,
                    height,
                });
            }
            Gesture::Transform {
                layer,
                mode,
                handle,
            } => {
                self.apply_transform_drag(&layer, mode, handle, point);
                self.image.gesture = Some(Gesture::Transform {
                    layer,
                    mode,
                    handle,
                });
            }
            Gesture::Rotate {
                layer,
                start_angle,
                rotation,
            } => {
                self.apply_rotate_drag(&layer, start_angle, rotation, shift, point);
                self.image.gesture = Some(Gesture::Rotate {
                    layer,
                    start_angle,
                    rotation,
                });
            }
            Gesture::Paint { layer, erase, last } => {
                self.paint_segment(&layer, last, point, erase);
                self.image.gesture = Some(Gesture::Paint {
                    layer,
                    erase,
                    last: point,
                });
            }
            Gesture::Marquee {
                tool,
                start,
                current: _,
                shift,
                alt,
            } => {
                self.image.gesture = Some(Gesture::Marquee {
                    tool,
                    start,
                    current: point,
                    shift,
                    alt,
                });
            }
            Gesture::Lasso { mut points } => {
                let far = match points.last() {
                    Some(last) => {
                        (last.x - point.x).hypot(last.y - point.y)
                            > 2.0 / self.image.view.k.max(0.01)
                    }
                    None => true,
                };
                if far {
                    points.push(point);
                }
                self.image.gesture = Some(Gesture::Lasso { points });
            }
            Gesture::Shape { start, current: _ } => {
                self.image.gesture = Some(Gesture::Shape {
                    start,
                    current: point,
                });
            }
            Gesture::Gradient { start, current: _ } => {
                self.image.gesture = Some(Gesture::Gradient {
                    start,
                    current: point,
                });
            }
            Gesture::PenDrag { path, anchor } => {
                self.set_anchor_handle(&path, anchor, point);
                self.image.gesture = Some(Gesture::PenDrag { path, anchor });
            }
            Gesture::AnchorDrag { path, anchor, part } => {
                self.drag_anchor(&path, anchor, part, point);
                self.image.gesture = Some(Gesture::AnchorDrag { path, anchor, part });
            }
        }
    }

    fn image_pointer_up(&mut self) {
        self.image.snap_lines.clear();
        let Some(gesture) = self.image.gesture.take() else {
            return;
        };
        match gesture {
            Gesture::Paint { .. } => {
                let label = if self.image.mask_layer().is_some() {
                    "Paint Mask"
                } else {
                    "Brush"
                };
                self.finish_image(label);
            }
            Gesture::Marquee {
                tool,
                start,
                current,
                shift,
                alt,
            } => {
                self.apply_marquee(tool, start, current, shift, alt);
            }
            Gesture::Lasso { points } => {
                self.apply_lasso(&points);
            }
            Gesture::Shape { start, current } => {
                self.apply_shape(start, current);
            }
            Gesture::Gradient { start, current } => {
                self.apply_gradient(start, current);
            }
            Gesture::Move { .. } => self.finish_image("Move Layer"),
            Gesture::Scale { .. } => self.finish_image("Scale Layer"),
            Gesture::Transform { .. } => self.finish_image("Transform"),
            Gesture::Rotate { .. } => self.finish_image("Rotate Layer"),
            Gesture::PenDrag { .. } => self.finish_image("Pen Path"),
            Gesture::AnchorDrag { .. } => self.finish_image("Edit Path"),
            Gesture::Pan { .. } => {}
        }
    }

    /// History push + dirty flag without a repaint (the `ui` wrappers sync).
    pub(crate) fn finish_image(&mut self, label: &str) {
        self.grow_image_board_to_fit();
        self.image.commit(label);
        self.note_doc_change();
    }

    /// Snaps a dragged layer's edges/centre to the board's edges/centre when
    /// within 6 screen px (`PsSnapLines` targets, image-studio.tsx:518-591) and
    /// records the lines for the overlay.
    pub(crate) fn snap_move(&mut self, layer_id: &str, x: f64, y: f64) -> (f64, f64) {
        self.image.snap_lines.clear();
        if !self.image.snap {
            return (x, y);
        }
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, layer_id)
        else {
            return (x, y);
        };
        let (width, height) = (layer.width.max(1.0), layer.height.max(1.0));
        let (board_w, board_h) = self.image.board();
        let tolerance = 6.0 / self.image.view.k.max(0.01);
        let snap_axis = |offset: f64,
                         size: f64,
                         extent: f64,
                         horizontal: bool,
                         lines: &mut Vec<(bool, f64)>| {
            let targets = [0.0, (extent - size) / 2.0, extent - size];
            let mut best: Option<(f64, f64)> = None;
            for target in targets {
                let delta = target - offset;
                if delta.abs() <= tolerance
                    && best.is_none_or(|(_, best_delta)| delta.abs() < best_delta.abs())
                {
                    best = Some((target, delta));
                }
            }
            match best {
                Some((target, delta)) => {
                    lines.push((horizontal, target));
                    offset + delta
                }
                None => offset,
            }
        };
        let next_x = snap_axis(x, width, board_w, false, &mut self.image.snap_lines);
        let next_y = snap_axis(y, height, board_h, true, &mut self.image.snap_lines);
        (next_x, next_y)
    }

    /// The topmost visible, unlocked object under the pointer.
    fn layer_at(&self, point: Point) -> Option<String> {
        self.image
            .project
            .board_layers
            .iter()
            .rev()
            .find(|layer| self.hand_hits_layer(layer, point))
            .map(|layer| layer.id.clone())
    }

    /// Whether a pointer lands on one object. The layer's rect is the fallback;
    /// a layer that carries pixels is hit only where those pixels are not
    /// transparent, so a transparent bitmap area selects the object below.
    fn hand_hits_layer(&self, layer: &open_canvas_ps::CanvasPsLayer, point: Point) -> bool {
        if layer.locked || layer.hidden {
            return false;
        }
        // Layers rotate around their centre: test in the layer's own space.
        let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
        let local = open_canvas_ps::geom::rotate_point(point.x, point.y, centre, -layer.rotation);
        let bounds = layer_box(&self.image.project.board_layers, layer);
        if local.x < bounds.x
            || local.y < bounds.y
            || local.x > bounds.x + bounds.width
            || local.y > bounds.y + bounds.height
        {
            return false;
        }
        let Some(bitmap) = layer
            .storage_key
            .as_ref()
            .and_then(|key| self.image.bitmaps.get(key))
        else {
            // Text / shape / adjustment layers have no pixels of their own.
            return true;
        };
        // An empty layer has no pixels to aim at: its rect is the grab area.
        if bitmap
            .data()
            .iter()
            .skip(3)
            .step_by(4)
            .all(|alpha| *alpha == 0)
        {
            return true;
        }
        let x = vm::f2i((local.x - layer.x).floor());
        let y = vm::f2i((local.y - layer.y).floor());
        bitmap.get(x, y)[3] > 0
    }

    /// Move/scale start: a corner of the active layer's box starts a scale
    /// gesture, anywhere else moves the layer.
    fn image_move_start(&mut self, x: f64, y: f64, point: Point) {
        let layer_id = self.image.active_layer_id();
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &layer_id)
                .cloned()
        else {
            return;
        };
        if layer.locked {
            return;
        }
        if self.image_transform_begin((x, y)) {
            return;
        }
        let box_ = layer_box(&self.image.project.board_layers, &layer);
        let view = self.image.view;
        let screen = |doc_x: f64, doc_y: f64| (view.x + doc_x * view.k, view.y + doc_y * view.k);
        let corners = [
            (box_.x, box_.y),
            (box_.x + box_.width, box_.y),
            (box_.x + box_.width, box_.y + box_.height),
            (box_.x, box_.y + box_.height),
        ];
        for (corner, (cx, cy)) in corners.iter().enumerate() {
            let (px, py) = screen(*cx, *cy);
            if (px - x).hypot(py - y) <= HANDLE_HIT {
                self.image.gesture = Some(Gesture::Scale {
                    layer: layer_id,
                    corner,
                    start: point,
                    x: layer.x,
                    y: layer.y,
                    width: layer.width.max(1.0),
                    height: layer.height.max(1.0),
                });
                return;
            }
        }
        if self.image_rotate_begin_core(x, y).is_some() {
            return;
        }
        self.image.gesture = Some(Gesture::Move {
            layer: layer_id,
            start: point,
            x: layer.x,
            y: layer.y,
        });
    }

    /// `sampleColor`: the composited board colour at a document point.
    pub(crate) fn sample_board_color(&self, point: Point) -> Option<String> {
        let composite = self.image_composite()?;
        let rgba = image_color_at(
            &composite,
            vm::f2i(point.x.floor()),
            vm::f2i(point.y.floor()),
        )?;
        Some(open_canvas_ps::rgba_to_hex(rgba))
    }

    /// Ensures the active layer has a bitmap and snapshots it for history.
    /// The stroke's paint target (`paintTarget ?? createPsPixelLayer(...)`): when
    /// the active layer can carry pixels it is used as-is, otherwise a fresh
    /// pixel layer is created above it, selected, and the `paintAutoLayer` hint
    /// flagged for the pointer wrapper. Returns whether a layer was created.
    pub(crate) fn ensure_paint_layer(&mut self) -> bool {
        if self.image.mask_layer().is_some() {
            return false;
        }
        let active = self.image.active_layer_id();
        if self.image.ensure_layer_bitmap(&active).is_some() {
            return false;
        }
        let board = self.image.board();
        let mut gen = self.image_gen("ps-pixel");
        let count = self.image.project.board_layers.len() + 1;
        let layer =
            open_canvas_ps::create_ps_pixel_layer(&mut gen, board, &format!("Layer {count}"));
        let id = layer.id.clone();
        self.image.project.board_layers =
            open_canvas_ps::add_ps_layer(&self.image.project.board_layers, layer, None);
        self.image.active_layer = id;
        self.image.auto_layer_hint = true;
        true
    }

    fn begin_paint(&mut self) {
        if let Some(key) = self
            .image
            .mask_layer()
            .and_then(|layer| layer.mask_storage_key.clone())
        {
            self.image.begin_edit(std::slice::from_ref(&key));
            return;
        }
        let layer_id = self.image.active_layer_id();
        if let Some(key) = self.image.ensure_layer_bitmap(&layer_id) {
            self.image.begin_edit(std::slice::from_ref(&key));
        }
    }

    /// The stroke's erase flag: with a mask targeted the brush hides (clears
    /// mask alpha) and the eraser reveals - the web's
    /// `erase: maskLayer ? tool === "brush" : tool === "eraser"`.
    fn paint_erase(&self) -> bool {
        if self.image.mask_layer().is_some() {
            self.image.tool == ImageTool::Brush
        } else {
            self.image.tool == ImageTool::Eraser
        }
    }

    /// The paint target: the targeted mask's key, or the layer bitmap's,
    /// plus the layer's own origin for layer-space dab positions.
    fn paint_target(&mut self, layer_id: &str) -> Option<(String, (f64, f64), bool)> {
        let offset =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, layer_id)
                .map_or((0.0, 0.0), |layer| (layer.x, layer.y));
        if let Some(key) = self
            .image
            .mask_layer()
            .and_then(|layer| layer.mask_storage_key.clone())
        {
            return Some((key, offset, true));
        }
        self.image
            .ensure_layer_bitmap(layer_id)
            .map(|key| (key, offset, false))
    }

    /// Stamps dabs from `from` to `to` at the brush's spacing.
    fn paint_segment(&mut self, layer_id: &str, from: Point, to: Point, erase: bool) {
        let Some((key, (ox, oy), mask)) = self.paint_target(layer_id) else {
            return;
        };
        let radius = (self.image.brush_size / 2.0).max(0.5);
        let hardness = self.image.brush_hardness.clamp(0.0, 1.0);
        let opacity = self.image.brush_opacity.clamp(0.0, 1.0);
        let rgba = if erase {
            [0, 0, 0, 0]
        } else if mask {
            [255, 255, 255, 255]
        } else {
            self.image.active_rgba()
        };
        let Some(mut bitmap) = self.image.bitmaps.remove(&key) else {
            return;
        };
        let distance = (to.x - from.x).hypot(to.y - from.y);
        let steps = vm::f2i((distance / (radius * 2.0 * DAB_SPACING_RATIO).max(1.0)).ceil()).max(1);
        for step in 0..=steps {
            let t = vm::i2f(step) / vm::i2f(steps);
            let centre = Point::new(
                from.x + (to.x - from.x) * t - ox,
                from.y + (to.y - from.y) * t - oy,
            );
            stamp_dab(&mut bitmap, centre, radius, hardness, opacity, rgba, erase);
        }
        self.image.bitmaps.insert(key, bitmap);
    }

    /// Bucket: flood region from the wand mask, painted with the foreground.
    fn apply_bucket(&mut self, point: Point) {
        let source = if self.image.sample_all {
            self.image_composite()
        } else {
            let layer_id = self.image.active_layer_id();
            let key = self.image.ensure_layer_bitmap(&layer_id);
            key.and_then(|key| self.image.bitmaps.get(&key).cloned())
        };
        let Some(source) = source else {
            return;
        };
        let Some(mask) =
            selection_wand(&source, point, self.image.tolerance, self.image.contiguous)
        else {
            return;
        };
        let layer_id = self.image.active_layer_id();
        let Some(key) = self.image.ensure_layer_bitmap(&layer_id) else {
            return;
        };
        let rgba = self.image.active_rgba();
        self.image.begin_edit(std::slice::from_ref(&key));
        if let Some(mut bitmap) = self.image.bitmaps.remove(&key) {
            for y in 0..bitmap.height() {
                for x in 0..bitmap.width() {
                    if mask.get(vm::u2i(x), vm::u2i(y))[3] >= 128 {
                        bitmap.set(vm::u2i(x), vm::u2i(y), rgba);
                    }
                }
            }
            self.image.bitmaps.insert(key, bitmap);
        }
        self.finish_image("Bucket Fill");
    }

    /// Gradient fill across the drag path (`PsGradient` options drive it).
    fn apply_gradient(&mut self, start: Point, end: Point) {
        if (end.x - start.x).hypot(end.y - start.y) < 1.0 {
            return;
        }
        let layer_id = self.image.active_layer_id();
        let Some(key) = self.image.ensure_layer_bitmap(&layer_id) else {
            return;
        };
        let (ox, oy) = {
            let Some(layer) = open_canvas_ps::layer_ops::find_ps_layer(
                &self.image.project.board_layers,
                &layer_id,
            ) else {
                return;
            };
            (layer.x, layer.y)
        };
        let radial = self.image.gradient_radial;
        let reverse = self.image.gradient_reverse;
        let stops = self.image.gradient_stops();
        // The gradient options' blend mode + opacity (`psGradientFill`'s
        // `globalAlpha` / `globalCompositeOperation`); normal at 100% reproduces
        // the previous overwrite exactly.
        let mode = open_canvas_ps::resolve_blend_mode(&self.image.gradient_blend);
        let opacity = (self.image.gradient_opacity / 100.0).clamp(0.0, 1.0);
        self.image.begin_edit(std::slice::from_ref(&key));
        if let Some(mut bitmap) = self.image.bitmaps.remove(&key) {
            let (a, b) = (
                Point::new(start.x - ox, start.y - oy),
                Point::new(end.x - ox, end.y - oy),
            );
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let length_sq = (dx * dx + dy * dy).max(1e-9);
            let max_distance = length_sq.sqrt();
            for y in 0..bitmap.height() {
                for x in 0..bitmap.width() {
                    let px = vm::i2f(vm::u2i(x)) + 0.5;
                    let py = vm::i2f(vm::u2i(y)) + 0.5;
                    let mut t = if radial {
                        ((px - a.x).hypot(py - a.y) / max_distance).clamp(0.0, 1.0)
                    } else {
                        (((px - a.x) * dx + (py - a.y) * dy) / length_sq).clamp(0.0, 1.0)
                    };
                    if reverse {
                        t = 1.0 - t;
                    }
                    let ramp = crate::image::assets::sample_gradient_stops(&stops, t);
                    let existing = bitmap.get(vm::u2i(x), vm::u2i(y));
                    bitmap.set(
                        vm::u2i(x),
                        vm::u2i(y),
                        open_canvas_ps::blend_pixel(existing, ramp, opacity, mode),
                    );
                }
            }
            self.image.bitmaps.insert(key, bitmap);
        }
        self.finish_image("Gradient");
    }

    /// Drag-create a shape layer (`createPsShapeLayer`).
    fn apply_shape(&mut self, start: Point, end: Point) {
        let x0 = start.x.min(end.x);
        let y0 = start.y.min(end.y);
        let width = (end.x - start.x).abs().max(1.0);
        let height = (end.y - start.y).abs().max(1.0);
        let box_ = Bounds::new(x0, y0, width, height);
        let tool_shape = self.image.shape_kind;
        let fill = if self.image.shape_fill {
            self.image.color.clone()
        } else {
            String::new()
        };
        let stroke = if self.image.shape_fill {
            String::new()
        } else {
            self.image.color.clone()
        };
        let stroke_width = self.image.shape_stroke_width;
        let mut gen = self.image_gen("ps-shape");
        let layer = open_canvas_ps::create_ps_shape_layer(
            &mut gen,
            tool_shape,
            box_,
            "Shape",
            &fill,
            &stroke,
            stroke_width,
        );
        let id = layer.id.clone();
        self.image.project.board_layers =
            open_canvas_ps::add_ps_layer(&self.image.project.board_layers, layer, None);
        self.image.active_layer = id;
        self.finish_image("Add Shape");
    }

    /// Marquee/ellipse selection with the modifier combine mode.
    fn apply_marquee(&mut self, tool: ImageTool, start: Point, end: Point, shift: bool, alt: bool) {
        let mode = Self::selection_mode(shift, alt);
        let box_ = Bounds::new(
            start.x.min(end.x),
            start.y.min(end.y),
            (end.x - start.x).abs().max(1.0),
            (end.y - start.y).abs().max(1.0),
        );
        let ellipse = tool == ImageTool::EllipseMarquee;
        let feather = self.image.feather;
        let antialias = self.image.antialias;
        self.with_selection(|selection| {
            selection_rect(selection, box_, ellipse, feather, antialias, mode);
        });
    }

    /// Lasso polygon selection.
    fn apply_lasso(&mut self, points: &[Point]) {
        let mode = SelectionMode::Replace;
        let feather = self.image.feather;
        let antialias = self.image.antialias;
        self.with_selection(|selection| {
            selection_polygon(selection, points, feather, antialias, mode);
        });
    }

    /// Polygon-lasso finish (Enter or double-click): closes the point list.
    pub fn image_finish_polygon(&mut self, ui: &AppWindow) {
        self.finish_polygon_core();
        self.sync_image(ui);
    }

    pub(crate) fn finish_polygon_core(&mut self) {
        if self.image.poly_points.len() >= 3 {
            let points = std::mem::take(&mut self.image.poly_points);
            let feather = self.image.feather;
            let antialias = self.image.antialias;
            self.with_selection(|selection| {
                selection_polygon(
                    selection,
                    &points,
                    feather,
                    antialias,
                    SelectionMode::Replace,
                );
            });
        } else {
            self.image.poly_points.clear();
        }
    }

    /// Wand: flood-select with the tolerance settings.
    fn apply_wand(&mut self, point: Point, shift: bool, alt: bool) {
        let source = if self.image.sample_all {
            self.image_composite()
        } else {
            let layer_id = self.image.active_layer_id();
            self.image
                .ensure_layer_bitmap(&layer_id)
                .and_then(|key| self.image.bitmaps.get(&key).cloned())
        };
        let Some(source) = source else {
            return;
        };
        let Some(mask) =
            selection_wand(&source, point, self.image.tolerance, self.image.contiguous)
        else {
            return;
        };
        let mode = Self::selection_mode(shift, alt);
        self.with_selection(|selection| {
            open_canvas_ps::selection_combine(selection, &mask, mode);
        });
    }

    /// Quick select: round-brush grow under a colour constraint.
    fn apply_quick_select(&mut self, point: Point, alt: bool) {
        let source = if self.image.sample_all {
            self.image_composite()
        } else {
            let layer_id = self.image.active_layer_id();
            self.image
                .ensure_layer_bitmap(&layer_id)
                .and_then(|key| self.image.bitmaps.get(&key).cloned())
        };
        let sample = source
            .as_ref()
            .and_then(|source| image_color_at(source, vm::f2i(point.x), vm::f2i(point.y)));
        let radius = (self.image.brush_size / 2.0).max(1.0);
        let tolerance = self.image.tolerance;
        self.with_selection(|selection| {
            selection_quick(
                selection,
                source.as_ref(),
                point,
                radius,
                tolerance,
                sample,
                alt,
            );
        });
    }

    /// Runs `edit` on the selection mask, creating the board-size mask first.
    pub(crate) fn with_selection(&mut self, edit: impl FnOnce(&mut Surface)) {
        let Some((width, height)) = self.image.dims() else {
            return;
        };
        let mut selection = self
            .image
            .selection
            .take()
            .unwrap_or_else(|| open_canvas_ps::selection_create(width, height));
        edit(&mut selection);
        self.image.selection = Some(selection);
    }

    /// Pen: select an existing anchor/path under the pointer, else append an
    /// anchor to the active path (creating the path if needed). This mirrors
    /// the hand feel of the direct-select tool while keeping the pen's
    /// add-anchor flow.
    fn pen_down(&mut self, point: Point) {
        let threshold = ANCHOR_HIT / self.image.view.k.max(0.01);
        if let Some(hit) = self.path_anchor_hit(point, threshold) {
            self.image.active_path = hit.path.clone();
            self.image.gesture = Some(Gesture::AnchorDrag {
                path: hit.path,
                anchor: hit.index,
                part: hit.part,
            });
            return;
        }
        if let Some(path) = self.path_segment_hit(point, threshold) {
            self.image.active_path = path;
            return;
        }
        let existing = self
            .image
            .project
            .board_paths
            .as_ref()
            .is_some_and(|paths| paths.iter().any(|path| path.id == self.image.active_path));
        if !existing {
            let id = self.mint_image_path_id();
            let path = open_canvas_ps::create_path(
                &id,
                "Path",
                vec![open_canvas_ps::path_anchor(point.x, point.y)],
                false,
            );
            self.image
                .project
                .board_paths
                .get_or_insert_with(Vec::new)
                .push(path);
            self.image.active_path.clone_from(&id);
            self.image.gesture = Some(Gesture::PenDrag {
                path: id,
                anchor: 0,
            });
            return;
        }
        let active = self.image.active_path.clone();
        if let Some(paths) = self.image.project.board_paths.as_mut() {
            if let Some(path) = paths.iter_mut().find(|path| path.id == active) {
                path.anchors
                    .push(open_canvas_ps::path_anchor(point.x, point.y));
                let index = path.anchors.len() - 1;
                self.image.gesture = Some(Gesture::PenDrag {
                    path: path.id.clone(),
                    anchor: index,
                });
            }
        }
    }

    /// Sets the dragged anchor's out handle (in handle mirrors it).
    fn set_anchor_handle(&mut self, path_id: &str, index: usize, point: Point) {
        if let Some(paths) = self.image.project.board_paths.as_mut() {
            if let Some(path) = paths.iter_mut().find(|path| path.id == path_id) {
                if let Some(anchor) = path.anchors.get_mut(index) {
                    let dx = point.x - anchor.x;
                    let dy = point.y - anchor.y;
                    anchor.handle_out = open_canvas_ps::doc::Position { x: dx, y: dy };
                    anchor.handle_in = open_canvas_ps::doc::Position { x: -dx, y: -dy };
                }
            }
        }
    }

    /// Direct-select down: hit an anchor/handle of any visible path, or the
    /// path's stroke itself. A stroke hit activates the path so its anchors
    /// become visible; a second click can then drag one.
    fn anchor_down(&mut self, point: Point) {
        let threshold = ANCHOR_HIT / self.image.view.k.max(0.01);
        if let Some(hit) = self.path_anchor_hit(point, threshold) {
            self.image.active_path = hit.path.clone();
            self.image.gesture = Some(Gesture::AnchorDrag {
                path: hit.path,
                anchor: hit.index,
                part: hit.part,
            });
            return;
        }
        if let Some(path) = self.path_segment_hit(point, threshold) {
            self.image.active_path = path;
        }
    }

    /// Nearest visible anchor; handles are only tested on the active path
    /// because only that path paints them. An anchor always wins over a
    /// zero-length handle sitting on the same point.
    fn path_anchor_hit(&self, point: Point, threshold: f64) -> Option<PathHit> {
        let paths = self.image.project.board_paths.as_deref()?;
        let active = self.image.active_path.as_str();
        let mut best: Option<(f64, PathHit)> = None;
        for path in paths.iter().filter(|path| path.visible) {
            for (index, anchor) in path.anchors.iter().enumerate() {
                let distance = (anchor.x - point.x).hypot(anchor.y - point.y);
                if distance <= threshold
                    && best
                        .as_ref()
                        .is_none_or(|(best_distance, _)| distance < *best_distance)
                {
                    best = Some((
                        distance,
                        PathHit {
                            path: path.id.clone(),
                            index,
                            part: 0,
                        },
                    ));
                }
            }
        }
        if best.is_some() {
            return best.map(|(_, hit)| hit);
        }
        for path in paths
            .iter()
            .filter(|path| path.visible && path.id == active)
        {
            for (index, anchor) in path.anchors.iter().enumerate() {
                for (part, handle) in [(1_u8, anchor.handle_in), (2_u8, anchor.handle_out)] {
                    let hx = anchor.x + handle.x;
                    let hy = anchor.y + handle.y;
                    let distance = (hx - point.x).hypot(hy - point.y);
                    if distance <= threshold
                        && best
                            .as_ref()
                            .is_none_or(|(best_distance, _)| distance < *best_distance)
                    {
                        best = Some((
                            distance,
                            PathHit {
                                path: path.id.clone(),
                                index,
                                part,
                            },
                        ));
                    }
                }
            }
        }
        best.map(|(_, hit)| hit)
    }

    /// Nearest visible path stroke, measured against its flattened outline.
    fn path_segment_hit(&self, point: Point, threshold: f64) -> Option<String> {
        let paths = self.image.project.board_paths.as_deref()?;
        let mut best = threshold;
        let mut found = None;
        for path in paths.iter().filter(|path| path.visible) {
            let points = open_canvas_ps::polyline(path, 24);
            for pair in points.windows(2) {
                let distance = distance_to_segment(point, pair[0], pair[1]);
                if distance <= best {
                    best = distance;
                    found = Some(path.id.clone());
                }
            }
        }
        found
    }

    fn drag_anchor(&mut self, path_id: &str, index: usize, part: u8, point: Point) {
        if let Some(paths) = self.image.project.board_paths.as_mut() {
            if let Some(path) = paths.iter_mut().find(|path| path.id == path_id) {
                if let Some(anchor) = path.anchors.get_mut(index) {
                    match part {
                        0 => {
                            anchor.x = point.x;
                            anchor.y = point.y;
                        }
                        1 => {
                            anchor.handle_in = open_canvas_ps::doc::Position {
                                x: point.x - anchor.x,
                                y: point.y - anchor.y,
                            };
                        }
                        _ => {
                            anchor.handle_out = open_canvas_ps::doc::Position {
                                x: point.x - anchor.x,
                                y: point.y - anchor.y,
                            };
                        }
                    }
                }
            }
        }
    }

    /// History push + debounced save + repaint.
    pub(crate) fn commit_image(&mut self, ui: &AppWindow, label: &str) {
        self.commit_image_core(label);
        self.sync_image(ui);
    }

    /// History push + dirty flag without a repaint (the `ui` wrappers sync).
    pub(crate) fn commit_image_core(&mut self, label: &str) {
        self.grow_image_board_to_fit();
        self.image.commit(label);
        self.note_doc_change();
    }

    pub fn image_undo(&mut self, ui: &AppWindow) {
        if self.image.undo() {
            self.note_doc_change();
            self.sync_image(ui);
        }
    }

    pub fn image_redo(&mut self, ui: &AppWindow) {
        if self.image.redo() {
            self.note_doc_change();
            self.sync_image(ui);
        }
    }

    /// Escape cancels whichever inline rename is open, mirroring the web
    /// panels' `rename-cancel` (channels, paths, actions).
    pub(crate) fn image_cancel_inline_rename_core(&mut self) -> bool {
        let channel = self.image.channel_edit.take().is_some();
        let path = self.image.path_edit.take().is_some();
        let action = self.image.action_edit.take().is_some();
        channel || path || action
    }

    /// Escape outside an inline rename: drops a polygon-lasso draft, else ends
    /// the live gesture and leaves the armed transform mode (`exitTransform`).
    pub(crate) fn image_escape_core(&mut self) -> bool {
        if !self.image.poly_points.is_empty() {
            self.image.poly_points.clear();
            return false;
        }
        self.image.gesture = None;
        self.image_transform_exit_core()
    }

    /// Image Studio keyboard shortcuts (image-studio.tsx:408-497).
    pub fn image_key_down(&mut self, ui: &AppWindow, text: &str, ctrl: bool, shift: bool) {
        if text == " " {
            self.image.space_held = true;
            self.sync_image(ui);
            return;
        }
        if ctrl {
            match text.to_ascii_lowercase().as_str() {
                "z" if shift => self.image_redo(ui),
                "z" => self.image_undo(ui),
                "y" => self.image_redo(ui),
                "a" => self.image_select_all(ui),
                "d" => self.image_deselect(ui),
                "i" if shift => self.image_invert(ui),
                "t" if shift => self.image_repeat_transform(ui),
                "t" => self.image_transform_mode(ui, "edit.free-transform"),
                _ => {}
            }
            return;
        }
        if text == "[" || text == "]" {
            let step = if self.image.brush_size < 10.0 {
                1.0
            } else {
                (self.image.brush_size * 0.1).round().max(1.0)
            };
            let size = if text == "[" {
                self.image.brush_size - step
            } else {
                self.image.brush_size + step
            };
            self.image.brush_size = size.clamp(1.0, 500.0);
            self.sync_image(ui);
            return;
        }
        if text == "Enter" {
            self.image_finish_polygon(ui);
            return;
        }
        if text == "Escape" {
            if self.image_cancel_inline_rename_core() {
                self.sync_image(ui);
                return;
            }
            self.image_escape_core();
            self.sync_image(ui);
            return;
        }
        if let Some(tool) = ImageTool::from_hotkey(text, shift) {
            self.image.tool = tool;
            self.sync_image(ui);
        }
    }

    pub fn image_key_up(&mut self, ui: &AppWindow, text: &str) {
        if text == " " {
            self.image.space_held = false;
            self.sync_image(ui);
        }
    }

    /// 60ms tick while the workspace is open: polls the canvas-area size (the
    /// hidden workspace never sees a size-change event).
    pub fn tick_image(&mut self, ui: &AppWindow) {
        // The tooltip hover delay is shared by every studio, so it ticks before
        // the "is this studio open" guard.
        self.tick_dock(ui);
        if self.view != View::Image || self.image.meta.is_none() {
            return;
        }
        let width = f64::from(ui.get_image_canvas_width());
        let height = f64::from(ui.get_image_canvas_height());
        if (width, height) != self.image.canvas && width > 0.0 && height > 0.0 {
            self.image.canvas = (width, height);
            self.sync_image(ui);
        }
        // Marching ants: the dash phase steps every `ANTS_TICK_MS` while a
        // selection is up (`ps-overlays.tsx:59-62`).
        if self.image.selection.is_some() {
            let now = self.now_ms();
            if now >= self.image.ants_due.unwrap_or(0) {
                self.image.ants_due = Some(now + crate::image::ANTS_TICK_MS);
                self.image.ants_phase = crate::image::ants_advance(self.image.ants_phase);
                self.sync_image(ui);
            }
        }
    }
}

/// One brush dab: a soft round stamp (hardness = hard-core fraction).
fn stamp_dab(
    bitmap: &mut Surface,
    centre: Point,
    radius: f64,
    hardness: f64,
    opacity: f64,
    rgba: Rgba,
    erase: bool,
) {
    let (width, height) = (bitmap.width(), bitmap.height());
    let x0 = vm::f2i((centre.x - radius).floor()).max(0);
    let y0 = vm::f2i((centre.y - radius).floor()).max(0);
    let x1 = vm::f2i((centre.x + radius).ceil()).min(vm::u2i(width));
    let y1 = vm::f2i((centre.y + radius).ceil()).min(vm::u2i(height));
    let hard = radius * hardness.clamp(0.0, 1.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = vm::i2f(x) + 0.5 - centre.x;
            let dy = vm::i2f(y) + 0.5 - centre.y;
            let distance = dx.hypot(dy);
            if distance > radius {
                continue;
            }
            let coverage = if distance <= hard || radius <= hard {
                1.0
            } else {
                1.0 - (distance - hard) / (radius - hard).max(1e-6)
            };
            let coverage = coverage.clamp(0.0, 1.0) * opacity;
            let existing = bitmap.get(x, y);
            if erase {
                let alpha = (f64::from(existing[3]) * (1.0 - coverage)).round();
                bitmap.set(
                    x,
                    y,
                    [existing[0], existing[1], existing[2], vm::f2u8(alpha)],
                );
            } else {
                let out = blend_over(existing, rgba, coverage);
                bitmap.set(x, y, out);
            }
        }
    }
}

/// Source-over with an extra coverage factor (the web paints dabs with global
/// alpha; this is the per-pixel equivalent).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "channel blends are clamped to 0..=255 before the u8 casts"
)]
fn blend_over(backdrop: Rgba, source: Rgba, coverage: f64) -> Rgba {
    let sa = f64::from(source[3]) / 255.0 * coverage.clamp(0.0, 1.0);
    let ba = f64::from(backdrop[3]) / 255.0;
    let out_a = sa + ba * (1.0 - sa);
    if out_a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let channel = |index: usize| {
        let sc = f64::from(source[index]) * sa;
        let bc = f64::from(backdrop[index]) * ba * (1.0 - sa);
        ((sc + bc) / out_a).clamp(0.0, 255.0) as u8
    };
    [
        channel(0),
        channel(1),
        channel(2),
        (out_a * 255.0).round() as u8,
    ]
}

/// Kebab shape id -> enum (tool-options bar chips).
pub(crate) fn shape_from_id(id: &str) -> Option<CanvasPsShapeKind> {
    Some(match id {
        "rectangle" => CanvasPsShapeKind::Rectangle,
        "rounded-rectangle" => CanvasPsShapeKind::RoundedRectangle,
        "ellipse" => CanvasPsShapeKind::Ellipse,
        "polygon" => CanvasPsShapeKind::Polygon,
        "line" => CanvasPsShapeKind::Line,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dab_stamps_a_soft_round_stamp_and_erases_it() {
        let mut bitmap = Surface::new(20, 20);
        stamp_dab(
            &mut bitmap,
            Point::new(10.0, 10.0),
            4.0,
            0.5,
            1.0,
            [255, 0, 0, 255],
            false,
        );
        assert_eq!(bitmap.get(10, 10), [255, 0, 0, 255]);
        assert_eq!(bitmap.get(0, 0), [0, 0, 0, 0]);
        let edge = bitmap.get(13, 10);
        assert!(edge[3] > 0 && edge[3] < 255, "edge is feathered: {edge:?}");

        stamp_dab(
            &mut bitmap,
            Point::new(10.0, 10.0),
            4.0,
            1.0,
            1.0,
            [0, 0, 0, 0],
            true,
        );
        assert_eq!(bitmap.get(10, 10)[3], 0);
    }

    #[test]
    fn gradient_lerps_the_endpoints_and_reverse_flips_it() {
        let mut bitmap = Surface::new(4, 1);
        let fg = [255, 0, 0, 255];
        let bg = [0, 0, 255, 255];
        let lerp_rgba = crate::image::assets::lerp_rgba;
        for x in 0..4 {
            let t = (vm::i2f(x) + 0.5) / 4.0;
            bitmap.set(x, 0, lerp_rgba(fg, bg, t));
        }
        assert_eq!(bitmap.get(0, 0), [223, 0, 31, 255]);
        assert_eq!(bitmap.get(3, 0), [31, 0, 223, 255]);
        assert_eq!(lerp_rgba(fg, bg, 0.0), fg);
        assert_eq!(lerp_rgba(fg, bg, 1.0), bg);
        assert_eq!(lerp_rgba(fg, bg, 1.0 - 0.0), bg);
    }

    #[test]
    fn shape_ids_round_trip_the_tool_options_chips() {
        assert_eq!(
            shape_from_id("rectangle"),
            Some(CanvasPsShapeKind::Rectangle)
        );
        assert_eq!(shape_from_id("polygon"), Some(CanvasPsShapeKind::Polygon));
        assert_eq!(shape_from_id("nope"), None);
    }

    #[test]
    fn selection_mode_maps_the_modifiers() {
        assert_eq!(App::selection_mode(false, false), SelectionMode::Replace);
        assert_eq!(App::selection_mode(true, false), SelectionMode::Add);
        assert_eq!(App::selection_mode(false, true), SelectionMode::Subtract);
        assert_eq!(App::selection_mode(true, true), SelectionMode::Intersect);
    }

    #[test]
    fn blend_over_keeps_the_backdrop_outside_the_coverage() {
        let out = blend_over([100, 100, 100, 255], [255, 0, 0, 255], 0.0);
        assert_eq!(out, [100, 100, 100, 255]);
        let out = blend_over([0, 0, 0, 0], [255, 0, 0, 255], 1.0);
        assert_eq!(out, [255, 0, 0, 255]);
        let out = blend_over([0, 0, 0, 255], [255, 0, 0, 255], 0.5);
        assert!(out[0] > 100 && out[0] < 200, "half-covered blend: {out:?}");
    }

    fn mask_app() -> Result<(crate::app::App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = crate::app::App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 16.0;
        project.height = 16.0;
        let mut gen = open_canvas_ps::IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = std::collections::HashMap::new();
        bitmaps.insert("image:t-1".to_string(), Surface::new(16, 16));
        app.enter_image(
            project,
            bitmaps,
            crate::image::assets::ImageAssets::default(),
        );
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        Ok((app, dir))
    }

    #[test]
    fn mask_targeted_strokes_write_the_mask_never_the_layer_bitmap(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = mask_app()?;
        assert!(app.image_mask_add_core());
        let mask_key = app
            .image
            .project
            .board_layers
            .first()
            .and_then(|layer| layer.mask_storage_key.clone())
            .ok_or("mask key")?;

        app.image.tool = ImageTool::Brush;
        app.image.brush_size = 6.0;
        app.image.color = "#ff0000".to_string();
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            8.0,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            9.0,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            9.0,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );

        let mask = app.image.bitmaps.get(&mask_key).ok_or("mask bitmap")?;
        assert!(
            mask.get(8, 8)[3] < 255,
            "the brush hid part of the mask: {:?}",
            mask.get(8, 8)
        );
        let layer_bitmap = app.image.bitmaps.get("image:t-1").ok_or("layer bitmap")?;
        assert_eq!(
            layer_bitmap.get(8, 8),
            [0, 0, 0, 0],
            "the layer bitmap is untouched"
        );
        assert_eq!(app.image.history.label(2), Some("Paint Mask"));

        app.image.tool = ImageTool::Eraser;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            8.0,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            8.5,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            8.5,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        let mask = app.image.bitmaps.get(&mask_key).ok_or("mask bitmap")?;
        assert_eq!(mask.get(8, 8)[3], 255, "the eraser restores the mask");
        assert_eq!(app.image.history.label(3), Some("Paint Mask"));
        Ok(())
    }

    #[test]
    fn clearing_the_mask_target_paints_the_layer_again() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = mask_app()?;
        assert!(app.image_mask_add_core());
        app.image_mask_target_core("t-1", false, false);

        app.image.tool = ImageTool::Brush;
        app.image.brush_size = 6.0;
        app.image.color = "#ff0000".to_string();
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            4.0,
            4.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            4.5,
            4.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            4.5,
            4.0,
            crate::input::Button::Left,
            false,
            false,
        );

        let layer_bitmap = app.image.bitmaps.get("image:t-1").ok_or("layer bitmap")?;
        assert_eq!(layer_bitmap.get(4, 4), [255, 0, 0, 255]);
        assert_eq!(app.image.history.label(2), Some("Brush"));
        Ok(())
    }

    #[test]
    fn escape_exits_the_armed_transform_mode() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = mask_app()?;
        app.image_transform_mode_core("edit.warp");
        assert!(app.image_escape_core(), "Escape leaves the armed mode");
        assert_eq!(app.image.transform_mode, None);
        assert!(!app.image_escape_core(), "a second Escape is a no-op");

        app.image.poly_points.push(Point::new(1.0, 1.0));
        assert!(!app.image_escape_core(), "a polygon draft is dropped first");
        assert_eq!(app.image.poly_points.len(), 0);
        Ok(())
    }

    #[test]
    fn right_press_never_starts_a_canvas_gesture() -> Result<(), Box<dyn std::error::Error>> {
        // The right button belongs to the window move (Slint): the Rust
        // gesture machine must stay idle so the board neither pans nor drags.
        let (mut app, _dir) = mask_app()?;
        app.image.tool = ImageTool::Hand;
        let before = app.image.view;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            8.0,
            8.0,
            crate::input::Button::Right,
            false,
            false,
        );
        assert!(app.image.gesture.is_none(), "right-down starts nothing");
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            12.0,
            12.0,
            crate::input::Button::Right,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            12.0,
            12.0,
            crate::input::Button::Right,
            false,
            false,
        );
        assert_eq!(app.image.view, before, "the view did not pan");
        assert_eq!(
            app.image.project.board_layers[0].x, 0.0,
            "the layer did not move"
        );
        Ok(())
    }

    #[test]
    fn transform_handles_take_precedence_over_the_hand_drag(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = mask_app()?;
        app.image_transform_mode_core("edit.distort");
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        let handles = crate::image::transform::transform_handle_points(
            &layer,
            open_canvas_ps::PsTransformMode::Distort,
        );
        let (_, corner) = handles.first().copied().ok_or("handle")?;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            corner.x,
            corner.y,
            crate::input::Button::Left,
            false,
            false,
        );
        assert!(
            matches!(app.image.gesture, Some(Gesture::Transform { .. })),
            "the armed-mode handle beats the hand drag"
        );

        app.image.gesture = None;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            8.0,
            8.0,
            crate::input::Button::Left,
            false,
            false,
        );
        assert!(
            matches!(app.image.gesture, Some(Gesture::Move { .. })),
            "an empty spot still moves the layer"
        );
        Ok(())
    }
}
