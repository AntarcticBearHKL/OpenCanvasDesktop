//! The `ps-menus.tsx` modals: Image Size, Canvas Size, Rotate Arbitrary and
//! Numeric Transform, plus the Edit menu's transform commands (Free Transform
//! and the four mesh modes, Repeat, Clear).
//! Owns the transform/pixel-size modals (`TransformModal`).

use open_canvas_ps::transform::PsTransformMode;
use open_canvas_ps::{ps_offset_layers, PsNumericTransform};

use crate::app::App;
use crate::vm;
use crate::AppWindow;

/// One open menu modal (`ps-menus.tsx` Image Size / Canvas Size / Rotate /
/// Numeric Transform).
#[derive(Debug, Clone, PartialEq)]
pub struct TransformModal {
    pub mode: String,
    pub a: String,
    pub b: String,
    pub constrain: bool,
    pub anchor: String,
    pub angle: String,
    pub dx: String,
    pub dy: String,
    pub scale_x: String,
    pub scale_y: String,
    pub rotation: String,
    pub skew_x: String,
    pub skew_y: String,
    pub message: String,
}

impl TransformModal {
    fn parse(value: &str, fallback: f64) -> f64 {
        value.trim().parse::<f64>().unwrap_or(fallback)
    }

    /// The numeric transform the modal's fields describe (`ps-menus.tsx`).
    pub fn numeric(&self) -> PsNumericTransform {
        PsNumericTransform {
            dx: Self::parse(&self.dx, 0.0),
            dy: Self::parse(&self.dy, 0.0),
            scale_x: Self::parse(&self.scale_x, 100.0),
            scale_y: Self::parse(&self.scale_y, 100.0),
            rotation: Self::parse(&self.rotation, 0.0),
            skew_x: Self::parse(&self.skew_x, 0.0),
            skew_y: Self::parse(&self.skew_y, 0.0),
        }
    }
}

/// The nine canvas anchors (`ps-menus.tsx` 3x3 grid): (column, row).
fn anchor_offsets(anchor: &str) -> (f64, f64) {
    let (row, column) = anchor.split_once('-').unwrap_or(("middle", "center"));
    let horizontal = match column {
        "left" => 0.0,
        "right" => 1.0,
        _ => 0.5,
    };
    let vertical = match row {
        "top" => 0.0,
        "bottom" => 1.0,
        _ => 0.5,
    };
    (horizontal, vertical)
}

impl App {
    /// Modal opens from the Image/Edit menus (`image.size`, `image.canvas-size`,
    /// `image.rotate-arbitrary`, `edit.numeric-transform`).
    pub fn image_modal_open(&mut self, ui: &AppWindow, id: &str) {
        self.image_modal_open_core(id);
        self.sync_image(ui);
    }

    /// The modal open without a window (unit tests drive this directly).
    pub fn image_modal_open_core(&mut self, id: &str) {
        let width = self.image.project.width.round().to_string();
        let height = self.image.project.height.round().to_string();
        let mode = match id {
            "image.size" => "image-size",
            "image.canvas-size" => "canvas-size",
            "image.rotate-arbitrary" => "rotate",
            "edit.numeric-transform" => "numeric",
            _ => return,
        };
        self.image.modal = Some(TransformModal {
            mode: mode.to_string(),
            a: width,
            b: height,
            constrain: true,
            anchor: "middle-center".to_string(),
            angle: "0".to_string(),
            dx: "0".to_string(),
            dy: "0".to_string(),
            scale_x: "100".to_string(),
            scale_y: "100".to_string(),
            rotation: "0".to_string(),
            skew_x: "0".to_string(),
            skew_y: "0".to_string(),
            message: String::new(),
        });
    }

    /// One modal field (`a`/`b` = size, plus the numeric-transform fields).
    pub fn image_modal_field(&mut self, ui: &AppWindow, key: &str, text: &str) {
        self.image_modal_field_core(key, text);
        self.sync_image(ui);
    }

    /// The modal field edit without a window (unit tests drive this directly).
    pub fn image_modal_field_core(&mut self, key: &str, text: &str) {
        let Some(modal) = self.image.modal.as_mut() else {
            return;
        };
        let board_w = self.image.project.width.max(1.0);
        let board_h = self.image.project.height.max(1.0);
        match key {
            "a" => {
                modal.a = text.to_string();
                if modal.constrain && modal.mode == "image-size" {
                    if let Ok(value) = text.trim().parse::<f64>() {
                        if value > 0.0 {
                            let height = (value * board_h / board_w).round();
                            modal.b = height.to_string();
                        }
                    }
                }
            }
            "b" => {
                modal.b = text.to_string();
                if modal.constrain && modal.mode == "image-size" {
                    if let Ok(value) = text.trim().parse::<f64>() {
                        if value > 0.0 {
                            let width = (value * board_w / board_h).round();
                            modal.a = width.to_string();
                        }
                    }
                }
            }
            "angle" => modal.angle = text.to_string(),
            "dx" => modal.dx = text.to_string(),
            "dy" => modal.dy = text.to_string(),
            "scale-x" => modal.scale_x = text.to_string(),
            "scale-y" => modal.scale_y = text.to_string(),
            "rotation" => modal.rotation = text.to_string(),
            "skew-x" => modal.skew_x = text.to_string(),
            "skew-y" => modal.skew_y = text.to_string(),
            _ => {}
        }
    }

    /// Constrain toggle + the canvas-size anchor grid.
    pub fn image_modal_toggle(&mut self, ui: &AppWindow, key: &str) {
        self.image_modal_toggle_core(key);
        self.sync_image(ui);
    }

    /// The modal toggle without a window (unit tests drive this directly).
    pub fn image_modal_toggle_core(&mut self, key: &str) {
        let Some(modal) = self.image.modal.as_mut() else {
            return;
        };
        if key == "constrain" {
            modal.constrain = !modal.constrain;
            return;
        }
        if modal.mode == "canvas-size" {
            modal.anchor = key.to_string();
        }
    }

    /// Apply the modal (`ps-menus.tsx` handlers).
    pub fn image_modal_run(&mut self, ui: &AppWindow) {
        if let Err(message) = self.image_modal_run_core() {
            if let Some(modal) = self.image.modal.as_mut() {
                modal.message.clone_from(&message);
            }
            self.set_notice(ui, message, true);
            self.sync_image(ui);
            return;
        }
        self.image.modal = None;
        self.sync_image(ui);
    }

    /// The modal apply without a window (unit tests drive this directly).
    pub fn image_modal_run_core(&mut self) -> Result<(), String> {
        let modal = self.image.modal.clone().ok_or("No modal is open")?;
        match modal.mode.as_str() {
            "image-size" => {
                let width = TransformModal::parse(&modal.a, self.image.project.width)
                    .round()
                    .clamp(1.0, crate::image::MAX_IMAGE_DIM);
                let height = TransformModal::parse(&modal.b, self.image.project.height)
                    .round()
                    .clamp(1.0, crate::image::MAX_IMAGE_DIM);
                self.image_size_core(width, height);
            }
            "canvas-size" => {
                let width = TransformModal::parse(&modal.a, self.image.project.width)
                    .round()
                    .clamp(1.0, crate::image::MAX_IMAGE_DIM);
                let height = TransformModal::parse(&modal.b, self.image.project.height)
                    .round()
                    .clamp(1.0, crate::image::MAX_IMAGE_DIM);
                self.image_canvas_size_core(width, height, &modal.anchor);
            }
            "rotate" => {
                let angle = TransformModal::parse(&modal.angle, 0.0).clamp(-180.0, 180.0);
                if angle != 0.0 {
                    self.image_rotate_core(angle);
                }
            }
            "numeric" => {
                let transform = modal.numeric();
                let active = self.image.active_layer_id();
                self.image.project.board_layers = open_canvas_ps::apply_numeric_transform(
                    &self.image.project.board_layers,
                    &active,
                    transform,
                );
                self.image.last_transform = Some(transform);
                self.commit_image_core("Numeric Transform");
            }
            _ => return Err("Unknown modal".to_string()),
        }
        self.image.modal = None;
        Ok(())
    }

    pub fn image_modal_close(&mut self, ui: &AppWindow) {
        self.image.modal = None;
        self.sync_image(ui);
    }

    /// Image > Image Size: resamples every layer bitmap and scales the layer
    /// boxes (`psResampleLayerBitmaps` + `psScaleLayers`).
    pub fn image_size_core(&mut self, width: f64, height: f64) {
        let old_w = self.image.project.width.max(1.0);
        let old_h = self.image.project.height.max(1.0);
        if (width - old_w).abs() < 0.5 && (height - old_h).abs() < 0.5 {
            return;
        }
        let sx = width / old_w;
        let sy = height / old_h;
        for layer in &mut self.image.project.board_layers {
            layer.x *= sx;
            layer.y *= sy;
            layer.width = (layer.width * sx).max(1.0);
            layer.height = (layer.height * sy).max(1.0);
        }
        let keys: Vec<String> = self
            .image
            .project
            .board_layers
            .iter()
            .filter_map(|layer| layer.storage_key.clone())
            .collect();
        let mut before: Vec<(String, open_canvas_ps::Surface)> = Vec::new();
        for key in &keys {
            let Some(bitmap) = self.image.bitmaps.get(key) else {
                continue;
            };
            let next_w = vm::i2u(vm::f2i(vm::i2f(vm::u2i(bitmap.width())) * sx)).max(1);
            let next_h = vm::i2u(vm::f2i(vm::i2f(vm::u2i(bitmap.height())) * sy)).max(1);
            before.push((key.clone(), bitmap.clone()));
            let resampled = bitmap.resample_bilinear(next_w, next_h);
            self.image.bitmaps.insert(key.clone(), resampled);
        }
        for (key, _) in &before {
            self.image.begin_edit(std::slice::from_ref(key));
        }
        self.image.project.width = width;
        self.image.project.height = height;
        self.image.board_w = width.round().to_string();
        self.image.board_h = height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image_core("Image Size");
    }

    /// Image > Canvas Size: resizes the board and offsets the layers by the
    /// 3x3 anchor (the web keeps layer contents untouched).
    pub fn image_canvas_size_core(&mut self, width: f64, height: f64, anchor: &str) {
        let old_w = self.image.project.width.max(1.0);
        let old_h = self.image.project.height.max(1.0);
        let (hx, hy) = anchor_offsets(anchor);
        let dx = (width - old_w) * hx;
        let dy = (height - old_h) * hy;
        if dx.abs() > 0.01 || dy.abs() > 0.01 {
            self.image.project.board_layers =
                ps_offset_layers(&self.image.project.board_layers, dx, dy);
        }
        self.image.project.width = width;
        self.image.project.height = height;
        self.image.board_w = width.round().to_string();
        self.image.board_h = height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image_core("Canvas Size");
    }

    /// Edit > Free Transform / Skew / Distort / Perspective / Warp: arms the
    /// Hand tool with a mesh transform mode (`ps-menus.tsx`).
    pub fn image_transform_mode(&mut self, ui: &AppWindow, id: &str) {
        self.image_transform_mode_core(id);
        self.sync_image(ui);
    }

    /// The transform-mode pick without a window (unit tests drive this directly).
    pub fn image_transform_mode_core(&mut self, id: &str) {
        let mode = match id {
            "edit.free-transform" => Some(PsTransformMode::Free),
            "edit.skew" => Some(PsTransformMode::Skew),
            "edit.distort" => Some(PsTransformMode::Distort),
            "edit.perspective" => Some(PsTransformMode::Perspective),
            "edit.warp" => Some(PsTransformMode::Warp),
            _ => None,
        };
        self.image.transform_mode = mode;
        if mode.is_some() {
            // Transform handles live on the Hand tool (the Move tool is gone).
            self.image.tool = crate::image::ImageTool::Hand;
        }
    }

    /// Edit > Repeat Transform (`Ctrl+Shift+T`).
    pub fn image_repeat_transform(&mut self, ui: &AppWindow) {
        self.image_repeat_transform_core();
        self.sync_image(ui);
    }

    /// The repeat without a window (unit tests drive this directly).
    pub fn image_repeat_transform_core(&mut self) {
        let Some(transform) = self.image.last_transform else {
            return;
        };
        let active = self.image.active_layer_id();
        self.image.project.board_layers = open_canvas_ps::apply_numeric_transform(
            &self.image.project.board_layers,
            &active,
            transform,
        );
        self.commit_image_core("Repeat Transform");
    }
}

/// Pushes the modal's view model (`sync_image_extra`).
pub fn sync_modal_ui(app: &App, ui: &AppWindow) {
    match &app.image.modal {
        Some(modal) => {
            ui.set_image_modal_open(true);
            ui.set_image_modal_mode(modal.mode.as_str().into());
            ui.set_image_modal_a(modal.a.as_str().into());
            ui.set_image_modal_b(modal.b.as_str().into());
            ui.set_image_modal_constrain(modal.constrain);
            ui.set_image_modal_anchor(modal.anchor.as_str().into());
            ui.set_image_modal_angle(modal.angle.as_str().into());
            ui.set_image_modal_dx(modal.dx.as_str().into());
            ui.set_image_modal_dy(modal.dy.as_str().into());
            ui.set_image_modal_scale_x(modal.scale_x.as_str().into());
            ui.set_image_modal_scale_y(modal.scale_y.as_str().into());
            ui.set_image_modal_rotation(modal.rotation.as_str().into());
            ui.set_image_modal_skew_x(modal.skew_x.as_str().into());
            ui.set_image_modal_skew_y(modal.skew_y.as_str().into());
            ui.set_image_modal_message(modal.message.as_str().into());
        }
        None => ui.set_image_modal_open(false),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{IdGen, Surface};

    fn menu_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        project.width = 16.0;
        project.height = 16.0;
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(16, 16);
        surface.set(1, 1, [200, 10, 10, 255]);
        bitmaps.insert("image:t-1".to_string(), surface);
        crate::test_support::app_with_image(project, bitmaps)
    }

    #[test]
    fn image_size_modal_resamples_bitmaps_and_scales_layers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = menu_board_app()?;
        app.image_modal_open_core("image.size");
        app.image_modal_field_core("a", "32");
        let modal = app.image.modal.clone().ok_or("modal")?;
        assert_eq!(modal.b, "32", "constrain proportions fills the height");

        app.image_modal_run_core()?;
        assert!(app.image.modal.is_none(), "applying closes the modal");
        assert_eq!(
            (app.image.project.width, app.image.project.height),
            (32.0, 32.0)
        );
        let bitmap = app.image.bitmaps.get("image:t-1").ok_or("bitmap")?;
        assert_eq!(
            (bitmap.width(), bitmap.height()),
            (32, 32),
            "bitmaps resample"
        );
        let layer = app.image.project.board_layers.first().ok_or("layer")?;
        assert_eq!((layer.width, layer.height), (32.0, 32.0));
        assert_eq!(app.image.history.label(1), Some("Image Size"));
        Ok(())
    }

    #[test]
    fn canvas_size_modal_offsets_layers_by_the_anchor() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = menu_board_app()?;
        app.image.project.board_layers =
            open_canvas_ps::patch_ps_layer(&app.image.project.board_layers, "t-1", |layer| {
                layer.x = 4.0;
                layer.y = 4.0;
            });
        app.image_modal_open_core("image.canvas-size");
        app.image_modal_field_core("a", "24");
        app.image_modal_field_core("b", "24");
        app.image_modal_toggle_core("middle-center");
        app.image_modal_run_core()?;
        let layer = app.image.project.board_layers.first().ok_or("layer")?;
        assert_eq!(
            (layer.x, layer.y),
            (8.0, 8.0),
            "centered anchor shifts by half"
        );
        assert_eq!(app.image.project.width, 24.0);

        app.image_modal_open_core("image.canvas-size");
        app.image_modal_field_core("a", "32");
        app.image_modal_field_core("b", "32");
        app.image_modal_toggle_core("top-left");
        app.image_modal_run_core()?;
        let layer = app.image.project.board_layers.first().ok_or("layer")?;
        assert_eq!((layer.x, layer.y), (8.0, 8.0), "top-left keeps the offset");
        Ok(())
    }

    #[test]
    fn rotate_arbitrary_modal_rotates_the_board_and_layers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = menu_board_app()?;
        app.image_modal_open_core("image.rotate-arbitrary");
        app.image_modal_field_core("angle", "45");
        app.image_modal_run_core()?;
        assert!(app.image.project.width > 16.0, "the rotated bbox grows");
        assert!(app.image.project.height > 16.0);
        assert_eq!(app.image.history.label(1), Some("Rotate Board"));
        Ok(())
    }

    #[test]
    fn numeric_transform_modal_and_repeat_apply_twice() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = menu_board_app()?;
        // A skew is a non-identity mesh: it is stored on the layer.
        app.image_modal_open_core("edit.numeric-transform");
        app.image_modal_field_core("skew-x", "20");
        app.image_modal_run_core()?;
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        assert!(
            layer.transform.is_some(),
            "the skew writes a mesh transform"
        );
        assert_eq!(app.image.history.label(1), Some("Numeric Transform"));

        // Move + scale compounds when repeated.
        app.image_modal_open_core("edit.numeric-transform");
        app.image_modal_field_core("dx", "5");
        app.image_modal_field_core("scale-x", "150");
        app.image_modal_run_core()?;
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        assert!(
            (layer.width - 24.0).abs() < 0.5,
            "scale-x applies: {}",
            layer.width
        );
        let first_x = layer.x;
        assert!(first_x > 0.0, "dx moved the layer: {first_x}");

        app.image_repeat_transform_core();
        assert_eq!(app.image.history.label(3), Some("Repeat Transform"));
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        assert!(
            (layer.x - first_x).abs() > 0.01,
            "repeat moves the layer again: {} vs {first_x}",
            layer.x
        );
        Ok(())
    }

    #[test]
    fn transform_modes_arm_the_hand_tool() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = menu_board_app()?;
        app.image_transform_mode_core("edit.warp");
        assert_eq!(app.image.transform_mode, Some(PsTransformMode::Warp));
        assert_eq!(app.image.tool, crate::image::ImageTool::Hand);
        app.image_transform_mode_core("edit.free-transform");
        assert_eq!(app.image.transform_mode, Some(PsTransformMode::Free));
        Ok(())
    }
}
