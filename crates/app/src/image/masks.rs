//! Layer masks: Add (selection- or white-filled), Apply (bake into the layer
//! bitmap), Delete, the mask-target toggle that routes painting at the mask and
//! the mask preview mode — the Rust half of `ps-properties-panel.tsx`'s mask
//! section, the layers panel's mask thumbnail and `image-studio.tsx`'s
//! `addMask`/`applyMask`/`deleteMask`/`setMaskTargetId`/`setMaskView`.
//! Owns mask create/apply/delete and the mask-target/preview flags.

use open_canvas_ps::{patch_ps_layer, px_size, selection, Surface};

use crate::app::App;
use crate::AppWindow;

/// History labels the web uses for the mask actions (`historyMask`,
/// `historyMaskPaint`, `historyApplyMask`).
pub const MASK_HISTORY_ADD: &str = "Layer mask";
pub const MASK_HISTORY_APPLY: &str = "Apply Mask";
pub const MASK_HISTORY_DELETE: &str = "Delete Mask";

impl App {
    /// Properties-panel mask buttons: `add` | `apply` | `delete`.
    pub fn image_mask_action(&mut self, ui: &AppWindow, action: &str) {
        let changed = match action {
            "add" => self.image_mask_add_core(),
            "apply" => self.image_mask_apply_core(),
            "delete" => self.image_mask_delete_core(),
            _ => false,
        };
        if changed {
            self.sync_image(ui);
        }
    }

    /// The mask thumbnail click: `target` routes painting at the mask (and
    /// `view` shows it); `target = false` clears both.
    pub fn image_mask_target(&mut self, ui: &AppWindow, layer: &str, target: bool, view: bool) {
        self.image_mask_target_core(layer, target, view);
        self.sync_image(ui);
    }

    /// `addMask`: a mask from the selection mapped into layer space, or an
    /// all-white reveal-everything mask; then masks become the paint target.
    pub fn image_mask_add_core(&mut self) -> bool {
        let layer = open_canvas_ps::layer_ops::find_ps_layer(
            &self.image.project.board_layers,
            &self.image.active_layer_id(),
        )
        .cloned();
        let Some(layer) = layer else {
            return false;
        };
        if matches!(layer.kind, open_canvas_ps::CanvasPsLayerKind::Group) {
            return false;
        }
        let width = px_size(layer.width).max(1);
        let height = px_size(layer.height).max(1);
        let mask = self.image.selection.as_ref().map_or_else(
            || {
                let mut white = Surface::new(width, height);
                white.fill([255, 255, 255, 255]);
                white
            },
            |selection| selection::selection_to_layer_space(selection, &layer, width, height),
        );
        let key = format!("image:mask-{}", layer.id);
        self.image.bitmaps.insert(key.clone(), mask);
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &layer.id, |target| {
                target.mask_storage_key = Some(key);
            });
        self.image.active_layer.clone_from(&layer.id);
        self.image.mask_target.clone_from(&layer.id);
        self.image.mask_view = true;
        self.commit_image_core(MASK_HISTORY_ADD);
        true
    }

    /// `applyMask`: bakes the mask's alpha into the layer bitmap and drops the
    /// mask. Needs a storage bitmap (the panel disables it otherwise).
    pub fn image_mask_apply_core(&mut self) -> bool {
        let layer = open_canvas_ps::layer_ops::find_ps_layer(
            &self.image.project.board_layers,
            &self.image.active_layer_id(),
        )
        .cloned();
        let Some(layer) = layer else {
            return false;
        };
        let (Some(key), Some(mask_key)) =
            (layer.storage_key.clone(), layer.mask_storage_key.clone())
        else {
            return false;
        };
        let (Some(content), Some(mask)) = (
            self.image.bitmaps.get(&key).cloned(),
            self.image.bitmaps.get(&mask_key).cloned(),
        ) else {
            return false;
        };
        let baked = open_canvas_ps::ps_bake_mask(&content, &mask);
        self.image.begin_edit(std::slice::from_ref(&key));
        self.image.bitmaps.insert(key, baked);
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &layer.id, |target| {
                target.mask_storage_key = None;
            });
        self.clear_mask_target_for(&layer.id);
        self.commit_image_core(MASK_HISTORY_APPLY);
        true
    }

    /// `deleteMask`: unlinks the mask, keeping the mask bitmap orphaned the way
    /// the web's uploaded blob is.
    pub fn image_mask_delete_core(&mut self) -> bool {
        let layer = open_canvas_ps::layer_ops::find_ps_layer(
            &self.image.project.board_layers,
            &self.image.active_layer_id(),
        )
        .cloned();
        let Some(layer) = layer else {
            return false;
        };
        if layer.mask_storage_key.is_none() {
            return false;
        }
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &layer.id, |target| {
                target.mask_storage_key = None;
            });
        self.clear_mask_target_for(&layer.id);
        self.commit_image_core(MASK_HISTORY_DELETE);
        true
    }

    /// The thumbnail click without a window (unit tests drive this directly).
    pub fn image_mask_target_core(&mut self, layer: &str, target: bool, view: bool) {
        let exists =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, layer)
                .is_some_and(|entry| entry.mask_storage_key.is_some());
        if !target || !exists {
            if !target {
                self.image.mask_target.clear();
                self.image.mask_view = false;
            }
            return;
        }
        self.image.active_layer = layer.to_string();
        self.image.mask_target = layer.to_string();
        self.image.mask_view = view;
    }

    fn clear_mask_target_for(&mut self, layer: &str) {
        if self.image.mask_target == layer {
            self.image.mask_target.clear();
            self.image.mask_view = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::composite;
    use std::collections::HashMap;

    use open_canvas_ps::{Bounds, CanvasPsLayerKind, IdGen, Surface};

    fn masks_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
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
        surface.fill([220, 30, 30, 255]);
        bitmaps.insert("image:t-1".to_string(), surface);
        crate::test_support::app_with_image(project, bitmaps)
    }

    #[test]
    fn add_mask_stores_a_white_mask_and_targets_it() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = masks_board_app()?;
        assert!(app.image_mask_add_core());
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        let key = layer.mask_storage_key.clone().ok_or("mask key")?;
        assert_eq!(key, "image:mask-t-1");
        let mask = app.image.bitmaps.get(&key).ok_or("mask bitmap")?;
        assert_eq!(
            mask.get(3, 3),
            [255, 255, 255, 255],
            "a fresh mask is opaque"
        );
        assert_eq!(app.image.mask_target, "t-1");
        assert!(app.image.mask_view);
        assert_eq!(app.image.history.label(1), Some(MASK_HISTORY_ADD));
        assert!(app.image.mask_layer().is_some());

        assert!(app.image_mask_delete_core());
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        assert!(layer.mask_storage_key.is_none());
        assert_eq!(app.image.mask_target, "");
        assert!(!app.image.mask_view);
        assert_eq!(app.image.history.label(2), Some(MASK_HISTORY_DELETE));
        Ok(())
    }

    #[test]
    fn a_selection_shaped_mask_carries_the_selection_into_layer_space(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = masks_board_app()?;
        app.image_select_all_core();
        app.with_selection(|selection| {
            open_canvas_ps::selection_rect(
                selection,
                Bounds::new(0.0, 0.0, 8.0, 16.0),
                false,
                0.0,
                false,
                open_canvas_ps::SelectionMode::Replace,
            );
        });
        assert!(app.image_mask_add_core());
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        let key = layer.mask_storage_key.ok_or("mask key")?;
        let mask = app.image.bitmaps.get(&key).ok_or("mask bitmap")?;
        assert_eq!(mask.get(2, 8)[3], 255, "the selected half is revealed");
        assert_eq!(mask.get(12, 8)[3], 0, "the unselected half is hidden");
        Ok(())
    }

    #[test]
    fn mask_view_shows_the_mask_and_apply_bakes_it_into_the_bitmap(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = masks_board_app()?;
        assert!(app.image_mask_add_core());
        let key = app
            .image
            .project
            .board_layers
            .first()
            .and_then(|layer| layer.mask_storage_key.clone())
            .ok_or("mask key")?;
        if let Some(mask) = app.image.bitmaps.get_mut(&key) {
            for y in 0..16 {
                for x in 8..16 {
                    mask.set(x, y, [0, 0, 0, 0]);
                }
            }
        }

        let view = app.image.mask_view_project();
        let shown = crate::image::render::board_surface(&view, &app.image.bitmaps).ok_or("view")?;
        assert_eq!(
            shown.get(2, 8),
            [255, 255, 255, 255],
            "mask view paints the mask"
        );
        assert_eq!(shown.get(12, 8)[3], 0);

        assert!(app.image_mask_apply_core());
        let layer = app
            .image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or("layer")?;
        assert!(layer.mask_storage_key.is_none());
        assert_eq!(app.image.mask_target, "");
        let baked = composite(&app);
        assert_eq!(
            &baked[(8 * 16 + 2) * 4..(8 * 16 + 2) * 4 + 4],
            [220, 30, 30, 255]
        );
        assert_eq!(
            &baked[(8 * 16 + 12) * 4..(8 * 16 + 12) * 4 + 4],
            [0, 0, 0, 0]
        );
        assert_eq!(app.image.history.label(2), Some(MASK_HISTORY_APPLY));
        Ok(())
    }

    #[test]
    fn apply_mask_requires_a_storage_bitmap_and_group_masks_are_refused(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = masks_board_app()?;
        app.image.project.board_layers =
            patch_ps_layer(&app.image.project.board_layers, "t-1", |layer| {
                layer.storage_key = None;
            });
        assert!(app.image_mask_add_core(), "a mask still attaches");
        assert!(
            !app.image_mask_apply_core(),
            "without a bitmap there is nothing to bake"
        );

        app.image.project.board_layers =
            patch_ps_layer(&app.image.project.board_layers, "t-1", |layer| {
                layer.kind = CanvasPsLayerKind::Group;
            });
        assert!(!app.image_mask_add_core(), "groups take no mask");
        Ok(())
    }

    #[test]
    fn mask_target_round_trips_and_clears() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = masks_board_app()?;
        assert!(app.image_mask_add_core());
        app.image_mask_target_core("t-1", false, false);
        assert_eq!(app.image.mask_target, "");
        assert!(!app.image.mask_view);

        app.image_mask_target_core("t-1", true, true);
        assert_eq!(app.image.mask_target, "t-1");
        assert!(app.image.mask_view);
        app.image_mask_target_core("t-1", true, false);
        assert_eq!(
            app.image.mask_target, "t-1",
            "alt-click only toggles the view"
        );
        assert!(!app.image.mask_view);

        app.image_mask_target_core("t-1", true, true);
        app.image_mask_target_core("missing", true, true);
        assert_eq!(
            app.image.mask_target, "t-1",
            "an unknown layer never targets"
        );
        Ok(())
    }
}
