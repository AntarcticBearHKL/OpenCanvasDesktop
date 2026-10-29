//! Interactive Edit-menu transforms: the per-mode handle sets, handle hit
//! testing, the live `psMoveTransformHandle` drag and the free-transform rotate
//! handle - the Rust half of `ps-transform.ts`'s `psTransformHandlesDoc` /
//! `psMoveTransformHandle` driven by `image-studio.tsx`'s transform gesture.
//! Owns the transform handle sets and their hit testing.

use open_canvas_ps::layer_ops::find_ps_layer;
use open_canvas_ps::transform::{transform_handles_doc, transform_point_doc};
use open_canvas_ps::{patch_ps_layer, CanvasPsLayer, Point, PsTransformMode};

use crate::app::App;
use crate::image::Gesture;
use crate::AppWindow;

/// `HANDLE_SCREEN_SIZE` half (image-studio.tsx:86): the interaction radius.
pub(crate) const TRANSFORM_HIT: f64 = 8.0;
/// `ROTATE_SCREEN_OFFSET` (image-studio.tsx:87).
pub(crate) const ROTATE_OFFSET: f64 = 26.0;

/// `psTransformHandlesDoc`: the mode's handles as `(index, doc point)` pairs -
/// 8 for skew, 4 for distort/perspective, the 16-point mesh for warp.
pub(crate) fn transform_handle_points(
    layer: &CanvasPsLayer,
    mode: PsTransformMode,
) -> Vec<(usize, Point)> {
    transform_handles_doc(layer, mode)
}

/// The free/free-transform rotate handle: above the box's top-centre, pushed
/// out along the layer's own "up" axis by `ROTATE_SCREEN_OFFSET` screen px.
pub(crate) fn rotate_handle_point(layer: &CanvasPsLayer, k: f64) -> Point {
    let top = transform_point_doc(layer, 0.5, 0.0);
    let radians = layer.rotation.to_radians();
    let offset = ROTATE_OFFSET / k.max(0.01);
    Point::new(
        top.x + radians.sin() * offset,
        top.y - radians.cos() * offset,
    )
}

/// The pointer's angle around `centre` in degrees (`atan2 dy, dx`).
fn angle_at(centre: Point, point: Point) -> f64 {
    (point.y - centre.y).atan2(point.x - centre.x).to_degrees()
}

impl App {
    /// Edit > Done / Escape: leaves the armed transform mode.
    pub fn image_transform_exit(&mut self, ui: &AppWindow) {
        if self.image_transform_exit_core() {
            self.sync_image(ui);
        }
    }

    /// The transform exit without a window (unit tests drive this directly).
    pub fn image_transform_exit_core(&mut self) -> bool {
        let had = self.image.transform_mode.is_some();
        self.image.transform_mode = None;
        if matches!(self.image.gesture, Some(Gesture::Transform { .. })) {
            self.image.gesture = None;
        }
        had
    }

    /// Move-tool pointer-down: hit-tests the armed transform mode's handles
    /// (and the free mode's rotate handle) before the move/scale fallback.
    /// Returns true when a transform gesture started.
    pub(crate) fn image_transform_begin(&mut self, screen: (f64, f64)) -> bool {
        let Some(mode) = self.image.transform_mode else {
            return false;
        };
        let layer_id = self.image.active_layer_id();
        let Some(layer) = find_ps_layer(&self.image.project.board_layers, &layer_id).cloned()
        else {
            return false;
        };
        let tolerance = TRANSFORM_HIT / self.image.view.k.max(0.01);
        let point = self.image_doc_point(screen.0, screen.1);
        if mode == PsTransformMode::Free {
            let handle = rotate_handle_point(&layer, self.image.view.k);
            if (handle.x - point.x).hypot(handle.y - point.y) <= tolerance {
                let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
                self.image.gesture = Some(Gesture::Rotate {
                    layer: layer_id,
                    start_angle: angle_at(centre, point),
                    rotation: layer.rotation,
                });
                return true;
            }
            return false;
        }
        for (handle, target) in transform_handle_points(&layer, mode) {
            if (target.x - point.x).hypot(target.y - point.y) <= tolerance {
                self.image.gesture = Some(Gesture::Transform {
                    layer: layer_id,
                    mode,
                    handle,
                });
                return true;
            }
        }
        false
    }

    /// One transform-handle drag step (`moveTransform`'s `applyDraft`).
    pub(crate) fn apply_transform_drag(
        &mut self,
        layer_id: &str,
        mode: PsTransformMode,
        handle: usize,
        point: Point,
    ) {
        let Some(layer) = find_ps_layer(&self.image.project.board_layers, layer_id).cloned() else {
            return;
        };
        let transform = open_canvas_ps::move_transform_handle(&layer, mode, handle, point);
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, layer_id, |target| {
                target.transform = Some(transform);
            });
    }

    /// One rotate-handle drag step; shift snaps the delta to 15° steps
    /// (`HANDLE_SCREEN_SIZE` ... `Math.round(delta / 15) * 15`).
    pub(crate) fn apply_rotate_drag(
        &mut self,
        layer_id: &str,
        start_angle: f64,
        start_rotation: f64,
        shift: bool,
        point: Point,
    ) {
        let Some(layer) = find_ps_layer(&self.image.project.board_layers, layer_id) else {
            return;
        };
        let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
        let mut delta = angle_at(centre, point) - start_angle;
        if shift {
            delta = (delta / 15.0).round() * 15.0;
        }
        let rotation = start_rotation + delta;
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, layer_id, |target| {
                target.rotation = rotation;
            });
    }

    /// The rotate start without a window: hit-tests the rotate handle and arms
    /// the gesture. Returns the layer id that started when hit.
    pub fn image_rotate_begin_core(&mut self, x: f64, y: f64) -> Option<String> {
        let layer_id = self.image.active_layer_id();
        let layer = find_ps_layer(&self.image.project.board_layers, &layer_id).cloned()?;
        if layer.locked {
            return None;
        }
        let tolerance = TRANSFORM_HIT / self.image.view.k.max(0.01);
        let point = self.image_doc_point(x, y);
        let handle = rotate_handle_point(&layer, self.image.view.k);
        if (handle.x - point.x).hypot(handle.y - point.y) > tolerance {
            return None;
        }
        let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
        self.image.gesture = Some(Gesture::Rotate {
            layer: layer_id.clone(),
            start_angle: angle_at(centre, point),
            rotation: layer.rotation,
        });
        Some(layer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{IdGen, Surface};

    fn app_with_square() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        app_with_bitmap(|x, y| {
            if (x / 4 + y / 4) % 2 == 0 {
                [220, 30, 30, 255]
            } else {
                [30, 30, 220, 255]
            }
        })
    }

    fn app_with_bitmap(
        pixel: impl Fn(i64, i64) -> [u8; 4],
    ) -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 16.0;
        project.height = 16.0;
        project.board_ratio = "1:1".to_string();
        project.board_resolution = open_canvas_ps::doc::BoardResolution::K1;
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                surface.set(x, y, pixel(x, y));
            }
        }
        bitmaps.insert("image:t-1".to_string(), surface);
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

    fn layer(app: &App) -> Result<CanvasPsLayer, Box<dyn std::error::Error>> {
        app.image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or_else(|| "layer".into())
    }

    #[test]
    fn every_mode_shows_its_handle_set() -> Result<(), Box<dyn std::error::Error>> {
        let (app, _dir) = app_with_square()?;
        let layer = layer(&app)?;
        assert_eq!(
            transform_handle_points(&layer, PsTransformMode::Skew).len(),
            8,
            "skew keeps corners plus edge midpoints"
        );
        assert_eq!(
            transform_handle_points(&layer, PsTransformMode::Distort).len(),
            4
        );
        assert_eq!(
            transform_handle_points(&layer, PsTransformMode::Perspective).len(),
            4
        );
        assert_eq!(
            transform_handle_points(&layer, PsTransformMode::Warp).len(),
            16,
            "warp shows the 4x4 mesh"
        );
        let handles = transform_handle_points(&layer, PsTransformMode::Warp);
        assert_eq!(handles.first().map(|(index, _)| *index), Some(0));
        assert_eq!(handles.last().map(|(index, _)| *index), Some(15));
        assert_eq!(
            handles.first().map(|(_, point)| *point),
            Some(Point::new(0.0, 0.0))
        );
        assert_eq!(
            handles.last().map(|(_, point)| *point),
            Some(Point::new(16.0, 16.0))
        );
        Ok(())
    }

    #[test]
    fn handle_hit_testing_picks_the_dragged_handle() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_square()?;
        app.image_transform_mode_core("edit.distort");
        assert_eq!(app.image.transform_mode, Some(PsTransformMode::Distort));

        assert!(
            app.image_transform_begin((15.5, 0.5)),
            "the TR corner hits handle 1"
        );
        match app.image.gesture.clone() {
            Some(Gesture::Transform { mode, handle, .. }) => {
                assert_eq!(mode, PsTransformMode::Distort);
                assert_eq!(handle, 1);
            }
            other => return Err(format!("expected a transform gesture, got {other:?}").into()),
        }

        app.image.gesture = None;
        assert!(
            !app.image_transform_begin((8.0, 8.0)),
            "the middle hits nothing"
        );
        Ok(())
    }

    #[test]
    fn a_distort_drag_moves_pixels_through_the_mesh() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_bitmap(|_, _| [220, 30, 30, 255])?;
        app.image_transform_mode_core("edit.distort");
        assert!(app.image_transform_begin((15.5, 0.5)));
        // Drag the top-right handle to the top-centre: the top edge compresses.
        app.apply_transform_drag("t-1", PsTransformMode::Distort, 1, Point::new(8.0, 0.0));
        let layer = layer(&app)?;
        let quad = layer.transform.as_ref().ok_or("transform")?;
        assert_eq!(quad.quad.get(1).map(|point| point.x), Some(0.5));

        let composite = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?;
        assert_eq!(composite.get(15, 1)[3], 0, "the old corner is empty");
        assert!(
            composite.get(4, 1)[0] > 100,
            "content re-renders under the compressed top edge: {:?}",
            composite.get(4, 1)
        );
        Ok(())
    }

    #[test]
    fn warp_drags_one_mesh_control_point() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_square()?;
        let before = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?
            .data()
            .to_vec();
        app.image_transform_mode_core("edit.warp");
        let handles = transform_handle_points(&layer(&app)?, PsTransformMode::Warp);
        let (index, target) = handles.get(5).copied().ok_or("mesh handle")?;
        assert!(
            app.image_transform_begin((target.x, target.y)),
            "mesh point 5 hits"
        );

        app.apply_transform_drag(
            "t-1",
            PsTransformMode::Warp,
            index,
            Point::new(target.x + 2.0, target.y + 1.0),
        );
        let layer = layer(&app)?;
        let warp = layer
            .transform
            .as_ref()
            .and_then(|transform| transform.warp.as_ref())
            .ok_or("warp mesh")?;
        assert_eq!(warp.len(), 16);
        assert!(
            (warp[index].x - (target.x + 2.0) / 16.0).abs() < 1e-9,
            "mesh points are normalized"
        );
        assert!((warp[index].y - (target.y + 1.0) / 16.0).abs() < 1e-9);

        let after = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?
            .data()
            .to_vec();
        let changed = before
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 0, "the mesh warp rewrites the composite pixels");
        Ok(())
    }

    #[test]
    fn the_rotate_handle_turns_and_shift_snaps() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_square()?;
        let handle = rotate_handle_point(&layer(&app)?, 1.0);
        assert!(
            (handle.y + ROTATE_OFFSET).abs() < 1e-9,
            "the handle floats above the box: {handle:?}"
        );
        let started = app
            .image_rotate_begin_core(handle.x, handle.y)
            .ok_or("rotate start")?;
        assert_eq!(started, "t-1");

        // Grab at the top of the box (angle -90°) and drag to the right (0°).
        app.apply_rotate_drag("t-1", -90.0, 0.0, false, Point::new(32.0, 8.0));
        assert!((layer(&app)?.rotation - 90.0).abs() < 1e-9);

        app.apply_rotate_drag("t-1", -90.0, 0.0, true, Point::new(32.0, 8.0));
        assert!(
            (layer(&app)?.rotation - 90.0).abs() < 1e-9,
            "90 snaps to 90"
        );

        // A 7° drag snaps away under shift, lands true otherwise.
        let centre = Point::new(8.0, 8.0);
        let start = angle_at(centre, Point::new(8.0, 0.0));
        let seven = Point::new(centre.x + 20.0, centre.y - 2.5);
        app.apply_rotate_drag("t-1", start, 0.0, false, seven);
        let free = layer(&app)?.rotation;
        app.apply_rotate_drag("t-1", start, 0.0, true, seven);
        let snapped = layer(&app)?.rotation;
        assert!(
            (free - snapped).abs() > 1.0,
            "shift snaps: {free} vs {snapped}"
        );
        assert!(((snapped / 15.0).round() * 15.0 - snapped).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn transform_modes_arm_and_exit() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_square()?;
        app.image_transform_mode_core("edit.perspective");
        assert_eq!(app.image.transform_mode, Some(PsTransformMode::Perspective));
        assert!(app.image_transform_exit_core());
        assert_eq!(app.image.transform_mode, None);
        assert!(!app.image_transform_exit_core());
        app.image_transform_mode_core("edit.free-transform");
        let handle = rotate_handle_point(&layer(&app)?, 1.0);
        assert!(
            app.image_transform_begin((handle.x, handle.y)),
            "free mode uses the rotate handle"
        );
        Ok(())
    }
}
