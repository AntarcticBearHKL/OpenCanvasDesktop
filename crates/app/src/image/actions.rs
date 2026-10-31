//! Image Studio actions: top-bar menus, tool options, layer/history/fx panel
//! commands, properties fields, the colour picker and the board settings
//! popover — the dispatch behind every panel control (`image-studio.tsx`'s
//! `PS_MENU_COMMANDS` + panel handlers).
//! Owns the panel/menu command dispatch and the `image_*` row-model pushes it triggers.

use open_canvas_ps::layer_ops::{ensure_ps_layer_style, find_ps_layer, patch_ps_layer_style};
use open_canvas_ps::{
    add_ps_layer, board::smart_canvas_size_for_ratio, create_ps_pixel_layer, create_ps_text_layer,
    duplicate_ps_layer, group_ps_layers, move_ps_layer_step, patch_ps_layer, ps_offset_layers,
    ps_rotate_layers, ps_trim_box, remove_ps_layer, selection_all, selection_bounds,
    selection_feather, selection_invert, ungroup_ps_layer, CanvasPsLayerStyleType,
    CanvasPsParamValue,
};

use crate::app::App;
use crate::image::{ColorSlot, ImageTool};
use crate::AppWindow;

impl App {
    /// Top-bar menu buttons and every menu item (ids like `select.all`).
    pub fn image_action(&mut self, ui: &AppWindow, id: &str) {
        if matches!(
            id,
            "file" | "edit" | "image" | "layer" | "select" | "filter" | "view" | "dock"
        ) {
            self.image.menu = if self.image.menu == id {
                String::new()
            } else {
                id.to_string()
            };
            self.sync_image(ui);
            return;
        }
        if !self.image.menu.is_empty() {
            self.image.menu.clear();
            self.sync_image(ui);
        }
        // Gradient tool options' blend-mode chips (`image-studio.tsx`'s
        // gradient `mode` select): `gradient.blend.<id>`.
        if let Some(blend) = id.strip_prefix("gradient.blend.") {
            if self.image_gradient_blend_core(blend) {
                self.sync_image(ui);
            }
            return;
        }
        // Document commands funnel through the recorder's replay path, so a
        // recorded command and a replayed command are always the same code.
        if crate::image::channels::is_recordable_command(id) {
            self.image_command_core(id);
            self.sync_image(ui);
            return;
        }
        if self.image_file_layer_command(ui, id) {
            return;
        }
        match id {
            "export" | "file.png" => self.image_export_open(ui),
            "file.jpeg" => self.image_export_open_as(ui, crate::image::export::ExportFormat::Jpeg),
            "file.bmp" => self.image_export_open_as(ui, crate::image::export::ExportFormat::Bmp),
            "edit.undo" => self.image_undo(ui),
            "edit.redo" => self.image_redo(ui),
            "edit.duplicate" => self.image_layer_action(ui, "duplicate", ""),
            "edit.delete" => self.image_layer_action(ui, "delete", ""),
            "edit.group" => self.image_layer_action(ui, "group", ""),
            "edit.ungroup" => self.image_layer_action(ui, "ungroup", ""),
            "edit.clear-transform" => self.image_clear_transform(ui),
            "edit.repeat-transform" => self.image_repeat_transform(ui),
            "edit.free-transform"
            | "edit.skew"
            | "edit.distort"
            | "edit.perspective"
            | "edit.warp" => self.image_transform_mode(ui, id),
            "edit.exit-transform" => self.image_transform_exit(ui),
            "edit.transform.free"
            | "edit.transform.skew"
            | "edit.transform.distort"
            | "edit.transform.perspective"
            | "edit.transform.warp" => {
                let mode = id.trim_start_matches("edit.transform.");
                let menu_id = if mode == "free" {
                    "edit.free-transform".to_string()
                } else {
                    format!("edit.{mode}")
                };
                self.image_transform_mode(ui, &menu_id);
            }
            "image.size"
            | "image.canvas-size"
            | "image.rotate-arbitrary"
            | "edit.numeric-transform" => self.image_modal_open(ui, id),
            "view.grid" => self.image_toggle(ui, "grid"),
            "view.rulers" => self.image_toggle(ui, "rulers"),
            "view.guides" => self.image_toggle(ui, "guides"),
            "view.snap" => self.image_toggle(ui, "snap"),
            "view.clear-guides" => self.image_clear_guides(ui),
            "view.zoom-in" => self.image_zoom_center(ui, crate::image::ZOOM_STEP),
            "view.zoom-out" => self.image_zoom_center(ui, 1.0 / crate::image::ZOOM_STEP),
            "view.fit" => {
                self.image_fit();
                self.sync_image(ui);
            }
            "view.actual" => {
                self.image_set_zoom(1.0);
                self.sync_image(ui);
            }
            // The Dock menu drives the shared dock engine; the `panels` flags
            // are derived from the layout on the way back (`sync_dock`).
            "dock.reset" => self.dock_reset(ui, crate::image::dock_engine::STUDIO_IMAGE),
            "settings" | "dock.settings" => {
                self.image.settings_open = !self.image.settings_open;
                self.sync_image(ui);
            }
            _ if id.starts_with("dock.") => {
                self.dock_toggle_panel(
                    ui,
                    crate::image::dock_engine::STUDIO_IMAGE,
                    id.trim_start_matches("dock."),
                );
            }
            _ => {}
        }
    }

    /// The File and Layer menu entries: each routes to the flow the studio
    /// already implements (no new behaviour), returning whether it was one.
    fn image_file_layer_command(&mut self, ui: &AppWindow, id: &str) -> bool {
        match id {
            "file.psd-export" => self.image_psd_export_open(ui),
            "layer.new" => self.image_layer_action(ui, "add-pixel", ""),
            "layer.new-text" => self.image_layer_action(ui, "add-text", ""),
            "layer.up" => self.image_layer_action(ui, "up", ""),
            "layer.down" => self.image_layer_action(ui, "down", ""),
            _ => return false,
        }
        true
    }

    fn image_zoom_center(&mut self, ui: &AppWindow, factor: f64) {
        let (cw, ch) = self.image.canvas;
        self.image_zoom_at(cw / 2.0, ch / 2.0, factor);
        self.sync_image(ui);
    }

    pub fn image_tool(&mut self, ui: &AppWindow, id: &str) {
        if let Some(tool) = ImageTool::from_id(id) {
            if self.image.text_edit.is_editing() {
                self.image_text_edit_commit(ui, None);
            }
            self.image.tool = tool;
            self.image.gesture = None;
            // Choosing a tool is a document action, not a menu action: close
            // any open menu in the same click so the rail never needs a
            // second press just to dismiss the click-away layer.
            self.image.menu.clear();
            self.image.settings_open = false;
            self.sync_image(ui);
        }
    }

    /// Checkbox/icon toggles: view flags and tool-option switches.
    pub fn image_toggle(&mut self, ui: &AppWindow, id: &str) {
        match id {
            "grid" => self.image.show_grid = !self.image.show_grid,
            "rulers" => self.image.show_rulers = !self.image.show_rulers,
            "guides" => self.image.show_guides = !self.image.show_guides,
            "snap" => self.image.snap = !self.image.snap,
            "contiguous" => self.image.contiguous = !self.image.contiguous,
            "sample-all" => self.image.sample_all = !self.image.sample_all,
            "antialias" => self.image.antialias = !self.image.antialias,
            "gradient-radial" => self.image.gradient_radial = !self.image.gradient_radial,
            "gradient-reverse" => self.image.gradient_reverse = !self.image.gradient_reverse,
            "shape-fill" => self.image.shape_fill = !self.image.shape_fill,
            _ => return,
        }
        self.sync_image(ui);
    }

    /// Tool-option sliders (0..100 UI units where the web shows percent).
    pub fn image_slider(&mut self, ui: &AppWindow, key: &str, value: f64) {
        if self.image_slider_core(key, value) {
            self.sync_image(ui);
        }
    }

    /// The tool-option slider without a window (unit tests drive this directly).
    pub fn image_slider_core(&mut self, key: &str, value: f64) -> bool {
        match key {
            "brush" => self.image.brush_size = value.round().clamp(1.0, 500.0),
            "hardness" => self.image.brush_hardness = (value / 100.0).clamp(0.0, 1.0),
            "opacity" => self.image.brush_opacity = (value / 100.0).clamp(0.0, 1.0),
            "tolerance" => self.image.tolerance = value.round().clamp(0.0, 255.0),
            "feather" => self.image.feather = value.round().clamp(0.0, 250.0),
            "stroke-width" => self.image.shape_stroke_width = value.round().clamp(0.0, 100.0),
            // Gradient tool options (`gradient.opacity` 1..100, `paint.stop` 0..1).
            "gradient-opacity" => self.image.gradient_opacity = value.clamp(1.0, 100.0),
            "gradient-stop" => {
                self.image.gradient_stop = (value / 100.0).clamp(0.0, 1.0);
            }
            "background-opacity" => {
                self.image.project.board_background_opacity = (value / 100.0).clamp(0.0, 1.0);
                self.note_doc_change();
            }
            _ => return false,
        }
        true
    }

    /// Gradient tool options' blend mode (`gradient.mode`) without a window;
    /// unknown ids are ignored. Returns whether the mode changed. The UI path
    /// arrives through `image_action("gradient.blend.<id>")`.
    pub fn image_gradient_blend_core(&mut self, id: &str) -> bool {
        if !open_canvas_ps::BLEND_MODE_IDS.contains(&id) {
            return false;
        }
        self.image.gradient_blend = id.to_string();
        true
    }

    /// Shape kind chip: patches the tool options (new layers use it).
    pub fn image_shape_pick(&mut self, ui: &AppWindow, id: &str) {
        if let Some(kind) = crate::image::ops::shape_from_id(id) {
            self.image.shape_kind = kind;
            self.sync_image(ui);
        }
    }

    /// Colour-slot pick: the picker fields switch between fg and bg.
    pub fn image_slot_pick(&mut self, ui: &AppWindow, slot: &str) {
        self.image.color_slot = if slot == "bg" {
            ColorSlot::Background
        } else {
            ColorSlot::Foreground
        };
        self.sync_image(ui);
    }

    /// The `SmartCanvasSettingsPopover`: ratio, resolution, background, opacity.
    pub fn image_settings_pick(&mut self, ui: &AppWindow, key: &str, value: &str) {
        if self.image_settings_pick_core(key, value) {
            self.sync_image(ui);
        }
    }

    /// The settings pick without a window (unit tests drive this directly).
    /// Returns whether anything changed.
    pub fn image_settings_pick_core(&mut self, key: &str, value: &str) -> bool {
        if key == "close" {
            let was_open = self.image.settings_open;
            self.image.settings_open = false;
            return was_open;
        }
        match key {
            "ratio" => {
                if value.is_empty() {
                    return false;
                }
                self.image.project.board_ratio = value.to_string();
                let (width, height) = smart_canvas_size_for_ratio(value);
                self.image.project.width = width;
                self.image.project.height = height;
                self.image.board_w = width.round().to_string();
                self.image.board_h = height.round().to_string();
                self.image.fitted.clear();
                self.note_doc_change();
            }
            "resolution" => {
                self.image.project.board_resolution = match value {
                    "1k" => open_canvas_ps::doc::BoardResolution::K1,
                    "4k" => open_canvas_ps::doc::BoardResolution::K4,
                    _ => open_canvas_ps::doc::BoardResolution::K2,
                };
                self.note_doc_change();
            }
            "background" => {
                self.image.project.board_background = value.to_string();
                self.note_doc_change();
            }
            // The custom background colour picker (`bgCustom`): applies without
            // closing the popover so the user can keep tweaking.
            "background-custom" => {
                let hex = open_canvas_ps::color::normalize_hex(value);
                if hex.is_empty() {
                    return false;
                }
                self.image.project.board_background = hex;
                self.note_doc_change();
                return true;
            }
            _ => return false,
        }
        self.image.settings_open = false;
        true
    }

    /// Board W/H fields + Apply (resizes the document box, not the bitmaps —
    /// the `ps` `changeSize` counterpart).
    pub fn image_board_apply(&mut self, ui: &AppWindow) {
        let Ok(width) = self.image.board_w.trim().parse::<f64>() else {
            return;
        };
        let Ok(height) = self.image.board_h.trim().parse::<f64>() else {
            return;
        };
        let width = width.round().clamp(1.0, crate::image::MAX_IMAGE_DIM);
        let height = height.round().clamp(1.0, crate::image::MAX_IMAGE_DIM);
        if (width - self.image.project.width).abs() < 0.5
            && (height - self.image.project.height).abs() < 0.5
        {
            return;
        }
        self.image.project.width = width;
        self.image.project.height = height;
        self.image.board_w = width.round().to_string();
        self.image.board_h = height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image(ui, "Board Size");
    }

    /// Layer-row action dispatch (`ps-layers-panel.tsx` footer actions).
    pub fn image_layer_pick(&mut self, ui: &AppWindow, id: &str) {
        if self.image.text_edit.is_editing() && !self.image.text_edit.is_editing_layer(id) {
            self.image_text_edit_commit(ui, None);
        }
        if find_ps_layer(&self.image.project.board_layers, id).is_some() {
            self.image.active_layer = id.to_string();
            self.image.mask_target.clear();
            self.image.mask_view = false;
            self.sync_image(ui);
        }
    }

    /// Inline rename draft (`editingProjectTitle` counterpart).
    pub fn image_layer_name_edited(&mut self, _ui: &AppWindow, text: &str) {
        if let Some((_, draft)) = self.image.editing_layer.as_mut() {
            *draft = text.to_string();
        }
    }

    /// Active-layer opacity slider (0..100 in the UI).
    pub fn image_layer_opacity(&mut self, ui: &AppWindow, value: f64) {
        let active = self.image.active_layer_id();
        let opacity = (value / 100.0).clamp(0.0, 1.0);
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                layer.opacity = opacity;
            });
        self.commit_image(ui, "Layer Opacity");
    }

    /// Active-layer blend mode (16 ids, `BLEND_MODE_IDS`).
    pub fn image_layer_blend(&mut self, ui: &AppWindow, id: &str) {
        if !open_canvas_ps::BLEND_MODE_IDS.contains(&id) {
            return;
        }
        let active = self.image.active_layer_id();
        let blend = id.to_string();
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                layer.blend_mode.clone_from(&blend);
            });
        self.commit_image(ui, "Layer Blend Mode");
    }

    /// The Slint callback entry point: dispatch + one repaint.
    pub fn image_layer_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        match action {
            "import" => {
                self.image_import_open(ui);
                return;
            }
            "fx" => {
                // The row's fx button opens the Layer Effects panel; with the
                // shipped layout starting empty this must reveal it too.
                self.image_layer_action_core(action, id);
                self.dock_reveal_panel(ui, crate::image::dock_engine::STUDIO_IMAGE, "fx");
                self.sync_image(ui);
                return;
            }
            "mask-target" => {
                self.image_mask_target(ui, id, true, true);
                return;
            }
            "mask-view" => {
                let view = !self.image.mask_view;
                self.image_mask_target(ui, id, true, view);
                return;
            }
            "collapse" => {
                self.image.toggle_collapse(id);
                self.sync_image(ui);
                return;
            }
            "reorder" => {
                if self.image_reorder_core(id) {
                    self.sync_image(ui);
                }
                return;
            }
            _ => {}
        }
        self.image_layer_action_core(action, id);
        self.sync_image(ui);
    }

    /// The layers-panel row drop (`movePsLayerTo`): `drag|target|before|
    /// after|into`. Returns whether the order changed.
    pub fn image_reorder_core(&mut self, spec: &str) -> bool {
        let Some((drag, rest)) = spec.split_once('|') else {
            return false;
        };
        let Some((target, position)) = rest.split_once('|') else {
            return false;
        };
        if drag == target
            || find_ps_layer(&self.image.project.board_layers, drag).is_none()
            || find_ps_layer(&self.image.project.board_layers, target).is_none()
        {
            return false;
        }
        let position = match position {
            "after" => open_canvas_ps::LayerPosition::After,
            "into" => open_canvas_ps::LayerPosition::Into,
            _ => open_canvas_ps::LayerPosition::Before,
        };
        let next = open_canvas_ps::move_ps_layer_to(
            &self.image.project.board_layers,
            drag,
            target,
            position,
        );
        if next == self.image.project.board_layers {
            return false;
        }
        self.image.project.board_layers = next;
        self.finish_image("Reorder Layer");
        true
    }

    /// Layer-row dispatch without a window (unit tests drive this directly).
    #[expect(
        clippy::too_many_lines,
        reason = "flat layer-action dispatch mirroring the web panel handlers"
    )]
    pub fn image_layer_action_core(&mut self, action: &str, id: &str) {
        let active = self.image.active_layer_id();
        let target = if id.is_empty() {
            active
        } else {
            id.to_string()
        };
        if crate::image::channels::is_recordable_layer_action(action) {
            self.image.recorder.record("layer", action, "");
        }
        match action {
            "add-text" => {
                let board = self.image.board();
                let mut gen = self.image_gen("ps-text");
                let layer = create_ps_text_layer(&mut gen, board, "Hello", "Text");
                let id = layer.id.clone();
                self.image.project.board_layers =
                    add_ps_layer(&self.image.project.board_layers, layer, None);
                self.image.active_layer = id;
                self.finish_image("Add Text Layer");
            }
            "add-pixel" => {
                let board = self.image.board();
                let mut gen = self.image_gen("ps-pixel");
                let count = self.image.project.board_layers.len() + 1;
                let layer = create_ps_pixel_layer(&mut gen, board, &format!("Layer {count}"));
                let id = layer.id.clone();
                self.image.project.board_layers =
                    add_ps_layer(&self.image.project.board_layers, layer, None);
                self.image.active_layer = id;
                self.finish_image("Add Layer");
            }
            "duplicate" => {
                let Some(original) =
                    find_ps_layer(&self.image.project.board_layers, &target).cloned()
                else {
                    return;
                };
                let copy_id = self.mint_image_layer_id();
                let mut gen = self.image_gen("ps-child");
                let mut layers = duplicate_ps_layer(
                    &self.image.project.board_layers,
                    &target,
                    "copy",
                    &copy_id,
                    &mut gen,
                );
                // Give the copy its own bitmap so painting it never aliases.
                if let Some(key) = original.storage_key.clone() {
                    if let Some(bitmap) = self.image.bitmaps.get(&key).cloned() {
                        let copy_key = format!("image:{copy_id}");
                        self.image.bitmaps.insert(copy_key.clone(), bitmap);
                        layers = patch_ps_layer(&layers, &copy_id, |layer| {
                            layer.storage_key = Some(copy_key);
                        });
                    }
                }
                self.image.project.board_layers = layers;
                self.image.active_layer = copy_id;
                self.finish_image("Duplicate Layer");
            }
            "delete" => {
                self.image.project.board_layers =
                    remove_ps_layer(&self.image.project.board_layers, &target);
                self.image.fix_active_ids();
                self.finish_image("Delete Layer");
            }
            "up" => {
                self.image.project.board_layers =
                    move_ps_layer_step(&self.image.project.board_layers, &target, true);
                self.finish_image("Move Layer Up");
            }
            "down" => {
                self.image.project.board_layers =
                    move_ps_layer_step(&self.image.project.board_layers, &target, false);
                self.finish_image("Move Layer Down");
            }
            "group" => {
                let group_id = self.mint_image_layer_id();
                self.image.project.board_layers = group_ps_layers(
                    &self.image.project.board_layers,
                    std::slice::from_ref(&target),
                    &group_id,
                    "Group",
                );
                self.finish_image("Group Layers");
            }
            "ungroup" => {
                self.image.project.board_layers =
                    ungroup_ps_layer(&self.image.project.board_layers, &target);
                self.image.fix_active_ids();
                self.finish_image("Ungroup Layer");
            }
            "visible" => {
                let Some(layer) = find_ps_layer(&self.image.project.board_layers, &target) else {
                    return;
                };
                let hidden = !layer.hidden;
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &target, |layer| {
                        layer.hidden = hidden;
                    });
                self.finish_image("Toggle Visibility");
            }
            "lock" => {
                let Some(layer) = find_ps_layer(&self.image.project.board_layers, &target) else {
                    return;
                };
                let locked = !layer.locked;
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &target, |layer| {
                        layer.locked = locked;
                    });
                self.finish_image("Toggle Lock");
            }
            "rename" => {
                if let Some(layer) = find_ps_layer(&self.image.project.board_layers, &target) {
                    self.image.editing_layer = Some((target.clone(), layer.name.clone()));
                }
            }
            "rename-save" => {
                let Some((id, name)) = self.image.editing_layer.take() else {
                    return;
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &id, |layer| {
                        layer.name.clone_from(&name);
                    });
                self.finish_image("Rename Layer");
            }
            "rename-cancel" => {
                self.image.editing_layer = None;
            }
            "fx" => {
                self.image.panels.fx = true;
                self.image.active_layer = target;
            }
            "fx-add" => {
                // The panel passes the style kind in `id`; the target is always
                // the active layer.
                let active = self.image.active_layer_id();
                self.image_fx_add_core(&active, id);
            }
            "rasterize" => {
                self.image_rasterize_core(&target).ok();
            }
            _ if action.starts_with("fx-edit:") => {
                let style_id = action.trim_start_matches("fx-edit:").to_string();
                self.image_fx_edit_core(&style_id);
            }
            _ if action.starts_with("fx-remove:") => {
                let style_id = action.trim_start_matches("fx-remove:");
                if let Some(layer) = find_ps_layer(&self.image.project.board_layers, &target) {
                    let styles: Vec<open_canvas_ps::CanvasPsLayerStyle> = layer
                        .styles
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|style| style.id != style_id)
                        .collect();
                    let styles = (!styles.is_empty()).then_some(styles);
                    self.image.project.board_layers =
                        patch_ps_layer(&self.image.project.board_layers, &target, |layer| {
                            layer.styles = styles;
                        });
                    self.finish_image("Remove Layer Style");
                }
            }
            _ if action.starts_with("fx-toggle:") => {
                let style_id = action.trim_start_matches("fx-toggle:");
                self.image_fx_toggle_core(&target, style_id);
            }
            _ => {}
        }
    }

    /// `ensurePsLayerStyle`: adds a style of `style_id` kind to the layer.
    fn image_fx_add_core(&mut self, layer_id: &str, style_kind: &str) {
        let Some(kind) = layer_style_from_id(style_kind) else {
            return;
        };
        let style_id = self.mint_image_id("ps-style");
        self.image.project.board_layers =
            ensure_ps_layer_style(&self.image.project.board_layers, layer_id, kind, &style_id);
        self.finish_image("Add Layer Style");
    }

    /// Enable/disable one fx entry (`patchPsLayerStyle`).
    fn image_fx_toggle_core(&mut self, layer_id: &str, style_id: &str) {
        let Some(layer) = find_ps_layer(&self.image.project.board_layers, layer_id) else {
            return;
        };
        let Some(style) = layer
            .styles
            .as_ref()
            .and_then(|styles| styles.iter().find(|style| style.id == style_id))
        else {
            return;
        };
        let enabled = !style.enabled;
        self.image.project.board_layers = patch_ps_layer_style(
            &self.image.project.board_layers,
            layer_id,
            style_id,
            Some(enabled),
            None,
        );
        self.finish_image("Toggle Layer Style");
    }

    /// History-panel click: jumps the cursor to that snapshot.
    pub fn image_history_pick(&mut self, ui: &AppWindow, index: i32) {
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if self.image.jump_history(index) {
            self.note_doc_change();
            self.sync_image(ui);
        }
    }

    /// Properties numeric/text fields (`key` names the field).
    pub fn image_apply_field(&mut self, ui: &AppWindow, key: &str, text: &str) {
        let active = self.image.active_layer_id();
        match key {
            "layer-name" => {
                let name = text.trim().to_string();
                if name.is_empty() {
                    return;
                }
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                        layer.name.clone_from(&name);
                    });
                self.commit_image(ui, "Rename Layer");
            }
            "x" | "y" | "w" | "h" | "rotation" => {
                let Ok(value) = text.trim().parse::<f64>() else {
                    return;
                };
                self.image.project.board_layers = patch_ps_layer(
                    &self.image.project.board_layers,
                    &active,
                    |layer| match key {
                        "x" => layer.x = value,
                        "y" => layer.y = value,
                        "w" => layer.width = value.max(1.0),
                        "h" => layer.height = value.max(1.0),
                        _ => layer.rotation = value,
                    },
                );
                self.commit_image(ui, "Layer Transform");
            }
            "board-w" => {
                self.image.board_w = text.to_string();
                self.sync_image(ui);
            }
            "board-h" => {
                self.image.board_h = text.to_string();
                self.sync_image(ui);
            }
            "text-content" => {
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                        layer.text = Some(text.to_string());
                    });
                self.commit_image(ui, "Edit Text");
            }
            "text-size" => {
                let Ok(value) = text.trim().parse::<f64>() else {
                    return;
                };
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                        layer.font_size = Some(value.clamp(1.0, 1000.0));
                    });
                self.commit_image(ui, "Edit Text");
            }
            "text-color" => {
                let hex = open_canvas_ps::color::normalize_hex(text);
                if hex.is_empty() {
                    return;
                }
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                        layer.color = Some(hex.clone());
                    });
                self.commit_image(ui, "Edit Text");
            }
            _ => {}
        }
    }

    /// Text font-family chip.
    pub fn image_font_pick(&mut self, ui: &AppWindow, family: &str) {
        let active = self.image.active_layer_id();
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                layer.font_family = Some(family.to_string());
            });
        self.commit_image(ui, "Edit Text");
    }

    /// Generic adjustment param slider (the P5c-3 editors replace this hook).
    pub fn image_param_set(&mut self, ui: &AppWindow, key: &str, value: f64) {
        let active = self.image.active_layer_id();
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                if let Some(params) = layer.adjustment_params.as_mut() {
                    params.insert(key.to_string(), CanvasPsParamValue::Number(value));
                }
            });
        self.commit_image(ui, "Adjustment Parameter");
    }

    /// Colour editor fields: `h`/`s`/`v` (HSB), `sv` ("sat,val" from the picker
    /// square), `r`/`g`/`b` (RGB), `hex`.
    pub fn image_color_field(&mut self, ui: &AppWindow, field: &str, text: &str) {
        if self.image_color_field_core(field, text) {
            self.sync_image(ui);
        }
    }

    /// The colour field without a window (unit tests drive this directly).
    pub fn image_color_field_core(&mut self, field: &str, text: &str) -> bool {
        let current = match self.image.color_slot {
            ColorSlot::Foreground => self.image.color.clone(),
            ColorSlot::Background => self.image.bg_color.clone(),
        };
        let next = match field {
            "h" | "s" | "v" => {
                let Ok(value) = text.trim().parse::<f64>() else {
                    return false;
                };
                let mut hsb = open_canvas_ps::color::hex_to_hsb(&current);
                match field {
                    "h" => hsb[0] = value.clamp(0.0, 360.0),
                    "s" => hsb[1] = value.clamp(0.0, 100.0),
                    _ => hsb[2] = value.clamp(0.0, 100.0),
                }
                open_canvas_ps::color::hsb_to_hex(hsb)
            }
            "sv" => {
                let Some((sat, val)) = text.trim().split_once(',') else {
                    return false;
                };
                let (Ok(sat), Ok(val)) = (sat.trim().parse::<f64>(), val.trim().parse::<f64>())
                else {
                    return false;
                };
                let mut hsb = open_canvas_ps::color::hex_to_hsb(&current);
                hsb[1] = sat.clamp(0.0, 100.0);
                hsb[2] = val.clamp(0.0, 100.0);
                open_canvas_ps::color::hsb_to_hex(hsb)
            }
            "r" | "g" | "b" => {
                let Ok(value) = text.trim().parse::<f64>() else {
                    return false;
                };
                let mut rgb = open_canvas_ps::color::hex_to_rgb_triple(&current);
                let slot = match field {
                    "r" => 0,
                    "g" => 1,
                    _ => 2,
                };
                rgb[slot] = value.round().clamp(0.0, 255.0);
                open_canvas_ps::color::rgb_to_hex(rgb)
            }
            "hex" => {
                let hex = open_canvas_ps::color::normalize_hex(text);
                if hex.is_empty() {
                    return false;
                }
                hex
            }
            _ => return false,
        };
        match self.image.color_slot {
            ColorSlot::Foreground => self.image.color = next,
            ColorSlot::Background => self.image.bg_color = next,
        }
        true
    }

    /// Swatch click: sets the active slot.
    pub fn image_color_pick(&mut self, ui: &AppWindow, hex: &str) {
        let hex = open_canvas_ps::color::normalize_hex(hex);
        if hex.is_empty() {
            return;
        }
        match self.image.color_slot {
            ColorSlot::Foreground => self.image.color = hex,
            ColorSlot::Background => self.image.bg_color = hex,
        }
        self.sync_image(ui);
    }

    pub fn image_swatch_add(&mut self, ui: &AppWindow) {
        let hex = match self.image.color_slot {
            ColorSlot::Foreground => self.image.color.clone(),
            ColorSlot::Background => self.image.bg_color.clone(),
        };
        if self
            .image
            .swatches
            .iter()
            .any(|swatch| swatch.eq_ignore_ascii_case(&hex))
        {
            return;
        }
        self.image.swatches.push(hex);
        self.sync_image(ui);
    }

    pub fn image_swatch_remove(&mut self, ui: &AppWindow, index: i32) {
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= self.image.swatches.len() || self.image.swatches.len() <= 1 {
            return;
        }
        self.image.swatches.remove(index);
        self.sync_image(ui);
    }

    pub fn image_color_swap(&mut self, ui: &AppWindow) {
        self.image.swap_colors();
        self.sync_image(ui);
    }

    /// Reset: foreground black, background white (the picker's reset button).
    pub fn image_color_reset(&mut self, ui: &AppWindow) {
        self.image.color = "#000000".to_string();
        self.image.bg_color = "#ffffff".to_string();
        self.sync_image(ui);
    }

    /// Select All (`Ctrl+A`).
    pub fn image_select_all(&mut self, ui: &AppWindow) {
        self.image_select_all_core();
        self.sync_image(ui);
    }

    pub(crate) fn image_select_all_core(&mut self) {
        self.with_selection(selection_all);
    }

    /// Deselect (`Ctrl+D`).
    pub fn image_deselect(&mut self, ui: &AppWindow) {
        self.image_deselect_core();
        self.sync_image(ui);
    }

    pub(crate) fn image_deselect_core(&mut self) {
        self.image.selection = None;
        self.image.poly_points.clear();
    }

    /// Inverse (`Ctrl+Shift+I`).
    pub fn image_invert(&mut self, ui: &AppWindow) {
        self.image_invert_core();
        self.sync_image(ui);
    }

    pub(crate) fn image_invert_core(&mut self) {
        if self.image.selection.is_none() {
            self.with_selection(selection_all);
        }
        self.with_selection(selection_invert);
    }

    /// Applies the current selection's feather (`Select > Feather`), shared by
    /// the menu command and the action recorder's replay.
    pub(crate) fn image_feather_core(&mut self) {
        let feather = self.image.feather;
        if self.image.selection.is_some() && feather > 0.0 {
            self.with_selection(|selection| selection_feather(selection, feather));
        }
    }

    fn image_clear_transform(&mut self, ui: &AppWindow) {
        self.image_clear_transform_core();
        self.sync_image(ui);
    }

    pub(crate) fn image_clear_transform_core(&mut self) {
        let active = self.image.active_layer_id();
        self.image.project.board_layers =
            open_canvas_ps::clear_transform(&self.image.project.board_layers, &active);
        self.commit_image_core("Clear Transform");
    }

    pub(crate) fn image_rotate_core(&mut self, degrees: f64) {
        let (layers, width, height) = ps_rotate_layers(
            &self.image.project.board_layers,
            degrees,
            self.image.project.width,
            self.image.project.height,
        );
        self.image.project.board_layers = layers;
        self.image.project.width = width;
        self.image.project.height = height;
        self.image.board_w = width.round().to_string();
        self.image.board_h = height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image_core("Rotate Board");
    }

    /// Image > Crop to Selection: offsets layers and shrinks the board.
    pub(crate) fn image_crop_core(&mut self) {
        let Some(selection) = self.image.selection.as_ref() else {
            return;
        };
        let Some(bounds) = selection_bounds(selection) else {
            return;
        };
        self.image.project.board_layers =
            ps_offset_layers(&self.image.project.board_layers, -bounds.x, -bounds.y);
        self.image.project.width = bounds.width.max(1.0);
        self.image.project.height = bounds.height.max(1.0);
        self.image.board_w = self.image.project.width.round().to_string();
        self.image.board_h = self.image.project.height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image_core("Crop");
    }

    /// Image > Trim: crops to the visible layers' union (`psTrimBox`).
    pub(crate) fn image_trim_core(&mut self) {
        let box_ = ps_trim_box(
            &self.image.project.board_layers,
            self.image.project.width,
            self.image.project.height,
        );
        if box_.width <= 0.0 || box_.height <= 0.0 {
            return;
        }
        self.image.project.board_layers =
            ps_offset_layers(&self.image.project.board_layers, -box_.x, -box_.y);
        self.image.project.width = box_.width.max(1.0);
        self.image.project.height = box_.height.max(1.0);
        self.image.board_w = self.image.project.width.round().to_string();
        self.image.board_h = self.image.project.height.round().to_string();
        self.image.selection = None;
        self.image.fitted.clear();
        self.commit_image_core("Trim");
    }

    /// Applies one filter with explicit parameters (the dialog + replay path).
    pub fn image_filter_apply(
        &mut self,
        choice: crate::image::filter::FilterChoice,
        params: &crate::image::filter::FilterParams,
    ) -> Result<(), String> {
        if !choice.supported() {
            return Err(choice
                .unsupported_reason()
                .unwrap_or("This filter needs an input the document model does not provide")
                .to_string());
        }
        self.image
            .recorder
            .record("filter", choice.id(), &params.record_string());
        let active = self.image.active_layer_id();
        let Some(key) = self.image.ensure_layer_bitmap(&active) else {
            return Err("Filters need a pixel or image layer".to_string());
        };
        let filtered = self.image.bitmaps.get(&key).map(|bitmap| {
            let mut copy = bitmap.clone();
            crate::image::filter::apply_choice(choice, params, &mut copy)?;
            Ok::<_, String>(copy)
        });
        let Some(filtered) = filtered else {
            return Ok(());
        };
        let filtered = filtered?;
        self.image.begin_edit(std::slice::from_ref(&key));
        self.image.bitmaps.insert(key, filtered);
        self.commit_image_core("Filter");
        Ok(())
    }

    /// Image > Rasterize (layer actions panel + the filter dialog's warning
    /// path): bakes a text/shape/group layer into a pixel bitmap.
    pub fn image_rasterize_core(&mut self, layer_id: &str) -> Result<String, String> {
        let id = if layer_id.is_empty() {
            self.image.active_layer_id()
        } else {
            layer_id.to_string()
        };
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &id)
                .cloned()
        else {
            return Err("Rasterize needs a layer".to_string());
        };
        if matches!(layer.kind, open_canvas_ps::CanvasPsLayerKind::Pixel) {
            return Ok(id);
        }
        let Some(surface) = self.rasterize_layer_surface(&id) else {
            return Err("The layer has nothing to rasterize".to_string());
        };
        let key = format!("image:{id}");
        self.image.bitmaps.insert(key.clone(), surface);
        self.image.project.board_layers =
            open_canvas_ps::rasterize_ps_layer(&self.image.project.board_layers, &id, &key);
        self.commit_image_core("Rasterize Layer");
        Ok(id)
    }
}

/// Kebab id -> filter identity: the native kernels first, then every entry of
/// the vendored engine's catalogue (filters.ts:41 + `algo_bridge::available`).
pub(crate) fn filter_from_id(id: &str) -> Option<crate::image::filter::FilterChoice> {
    crate::image::filter::FilterChoice::from_id(id)
}

/// The stable kebab id of a filter (the inverse of [`filter_from_id`], for the
/// recorder and the `filter.<id>` action namespace).
pub fn filter_id(choice: crate::image::filter::FilterChoice) -> &'static str {
    choice.id()
}

/// Kebab id -> layer style enum for the fx panel.
pub(crate) fn layer_style_from_id(id: &str) -> Option<CanvasPsLayerStyleType> {
    use CanvasPsLayerStyleType as Style;
    Some(match id {
        "stroke" => Style::Stroke,
        "drop-shadow" => Style::DropShadow,
        "inner-shadow" => Style::InnerShadow,
        "outer-glow" => Style::OuterGlow,
        "inner-glow" => Style::InnerGlow,
        "bevel" => Style::Bevel,
        "satin" => Style::Satin,
        "color-overlay" => Style::ColorOverlay,
        "gradient-overlay" => Style::GradientOverlay,
        "pattern-overlay" => Style::PatternOverlay,
        _ => return None,
    })
}

/// The inverse of [`layer_style_from_id`] (`FX_STYLE_LABELS` keys).
pub(crate) fn layer_style_id(kind: CanvasPsLayerStyleType) -> &'static str {
    use CanvasPsLayerStyleType as Style;
    match kind {
        Style::Stroke => "stroke",
        Style::DropShadow => "drop-shadow",
        Style::InnerShadow => "inner-shadow",
        Style::OuterGlow => "outer-glow",
        Style::InnerGlow => "inner-glow",
        Style::Bevel => "bevel",
        Style::Satin => "satin",
        Style::ColorOverlay => "color-overlay",
        Style::GradientOverlay => "gradient-overlay",
        Style::PatternOverlay => "pattern-overlay",
    }
}

/// `clearTransform` (transform.ts): drops the layer's quad/warp.
#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_ps::filters::PsFilterType;

    #[test]
    fn style_and_filter_ids_round_trip() {
        assert_eq!(
            layer_style_from_id("drop-shadow"),
            Some(CanvasPsLayerStyleType::DropShadow)
        );
        assert_eq!(layer_style_from_id("nope"), None);
        assert_eq!(
            filter_from_id("sharpen"),
            Some(crate::image::filter::FilterChoice::Native(
                PsFilterType::Sharpen
            ))
        );
        assert_eq!(filter_from_id("nope"), None);

        let filters = [
            PsFilterType::GaussianBlur,
            PsFilterType::BoxBlur,
            PsFilterType::MotionBlur,
            PsFilterType::RadialBlur,
            PsFilterType::TiltShift,
            PsFilterType::IrisBlur,
            PsFilterType::Sharpen,
            PsFilterType::UnsharpMask,
            PsFilterType::AddNoise,
            PsFilterType::Despeckle,
            PsFilterType::DustScratches,
            PsFilterType::Pixelate,
            PsFilterType::Crystallize,
            PsFilterType::Emboss,
            PsFilterType::FindEdges,
            PsFilterType::Solarize,
            PsFilterType::Clouds,
            PsFilterType::LensFlare,
            PsFilterType::Ripple,
            PsFilterType::Pinch,
            PsFilterType::Spherize,
            PsFilterType::Liquify,
        ];
        for kind in filters {
            let choice = crate::image::filter::FilterChoice::Native(kind);
            let id = filter_id(choice);
            assert_eq!(filter_from_id(id), Some(choice), "{id} decodes back");
        }

        let styles = [
            CanvasPsLayerStyleType::Stroke,
            CanvasPsLayerStyleType::DropShadow,
            CanvasPsLayerStyleType::InnerShadow,
            CanvasPsLayerStyleType::OuterGlow,
            CanvasPsLayerStyleType::InnerGlow,
            CanvasPsLayerStyleType::Bevel,
            CanvasPsLayerStyleType::Satin,
            CanvasPsLayerStyleType::ColorOverlay,
            CanvasPsLayerStyleType::GradientOverlay,
            CanvasPsLayerStyleType::PatternOverlay,
        ];
        for kind in styles {
            let id = layer_style_id(kind);
            assert_eq!(layer_style_from_id(id), Some(kind), "{id} decodes back");
        }
    }

    #[test]
    fn image_kind_checker_matches_the_index_tag() {
        assert!(crate::image::is_image_kind("image"));
        assert!(!crate::image::is_image_kind("pixel"));
        assert!(!crate::image::is_image_kind("canvas"));
    }

    fn reorder_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 16.0;
        project.height = 16.0;
        let mut gen = open_canvas_ps::IdGen::new("t");
        project.board_layers = vec![
            open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Bottom"),
            open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Middle"),
            open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Top"),
        ];
        app.enter_image(
            project,
            std::collections::HashMap::new(),
            crate::image::assets::ImageAssets::default(),
        );
        Ok((app, dir))
    }

    fn order(app: &App) -> Vec<String> {
        app.image
            .project
            .board_layers
            .iter()
            .map(|layer| layer.name.clone())
            .collect()
    }

    #[test]
    fn reorder_moves_rows_before_after_and_into_groups() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        let ids: Vec<String> = app
            .image
            .project
            .board_layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        let (bottom, middle, top) = (
            ids.first().cloned().ok_or("bottom")?,
            ids.get(1).cloned().ok_or("middle")?,
            ids.get(2).cloned().ok_or("top")?,
        );
        assert_eq!(order(&app), vec!["Bottom", "Middle", "Top"]);

        assert!(app.image_reorder_core(&format!("{bottom}|{top}|after")));
        assert_eq!(order(&app), vec!["Middle", "Top", "Bottom"]);
        assert_eq!(app.image.history.label(1), Some("Reorder Layer"));

        assert!(app.image_reorder_core(&format!("{bottom}|{middle}|before")));
        assert_eq!(order(&app), vec!["Bottom", "Middle", "Top"]);

        assert!(!app.image_reorder_core(&format!("{top}|{top}|after")));
        assert!(!app.image_reorder_core("garbage"));
        assert_eq!(
            order(&app),
            vec!["Bottom", "Middle", "Top"],
            "no-ops change nothing"
        );
        Ok(())
    }

    #[test]
    fn reorder_into_and_out_of_a_group() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        let ids: Vec<String> = app
            .image
            .project
            .board_layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        let (bottom, top) = (
            ids.first().cloned().ok_or("bottom")?,
            ids.get(2).cloned().ok_or("top")?,
        );
        app.image_layer_action_core("group", &bottom);
        let group = app
            .image
            .project
            .board_layers
            .iter()
            .find(|layer| layer.kind == open_canvas_ps::CanvasPsLayerKind::Group)
            .cloned()
            .ok_or("group")?;
        assert_eq!(group.children.as_deref(), Some(&[bottom.clone()][..]));

        assert!(app.image_reorder_core(&format!("{top}|{}|into", group.id)));
        let group = find_ps_layer(&app.image.project.board_layers, &group.id)
            .cloned()
            .ok_or("group")?;
        assert_eq!(
            group.children.as_deref(),
            Some(&[bottom.clone(), top.clone()][..]),
            "the row moved into the group (appended on top of the existing child)"
        );

        let child = group
            .children
            .clone()
            .unwrap_or_default()
            .first()
            .cloned()
            .ok_or("child")?;
        let outside = app
            .image
            .project
            .board_layers
            .iter()
            .find(|layer| {
                layer.kind != open_canvas_ps::CanvasPsLayerKind::Group && layer.id != child
            })
            .cloned()
            .ok_or("outside")?;
        assert!(app.image_reorder_core(&format!("{child}|{}|before", outside.id)));
        let group = find_ps_layer(&app.image.project.board_layers, &group.id)
            .cloned()
            .ok_or("group")?;
        assert_eq!(
            group.children.as_deref(),
            Some(&[top.clone()][..]),
            "the row left the group"
        );
        Ok(())
    }

    /// A 16x16 board with one pixel layer (bitmap pre-filled with mid grey so
    /// blend/opacity differences are visible), the layer active.
    fn paint_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let (mut app, dir) = reorder_app()?;
        app.image.project.board_layers.truncate(1);
        let id = app.image.active_layer_id();
        if let Some(key) = app.image.ensure_layer_bitmap(&id) {
            if let Some(bitmap) = app.image.bitmaps.get_mut(&key) {
                bitmap.fill([128, 128, 128, 255]);
            }
        }
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        Ok((app, dir))
    }

    /// Runs one left-drag gradient gesture from (0,0) to (15,0).
    fn drag_gradient(app: &mut App) {
        app.image.tool = ImageTool::Gradient;
        for (kind, x) in [
            (crate::input::PointerKind::Down, 0.0),
            (crate::input::PointerKind::Move, 15.0),
            (crate::input::PointerKind::Up, 15.0),
        ] {
            app.image_pointer_core(kind, x, 0.0, crate::input::Button::Left, false, false);
        }
    }

    fn layer_pixels(app: &App) -> Vec<u8> {
        let id = app.image.active_layer_id();
        app.image
            .project
            .board_layers
            .iter()
            .find(|layer| layer.id == id)
            .and_then(|layer| layer.storage_key.clone())
            .and_then(|key| app.image.bitmaps.get(&key))
            .map(|bitmap| bitmap.data().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn gradient_tool_options_reach_the_model_and_the_render(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = paint_app()?;
        assert_eq!(app.image.gradient_blend, "normal");
        assert!((app.image.gradient_opacity - 100.0).abs() < 1e-9);
        assert!((app.image.gradient_stop - 1.0).abs() < 1e-9);

        drag_gradient(&mut app);
        let plain = layer_pixels(&app);
        assert!(!plain.is_empty(), "the gradient painted the layer bitmap");
        let solid = plain
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] == 255 && px[0] != 128);
        assert!(
            solid,
            "the default gradient replaced the grey with the ramp"
        );

        // Blend mode: multiply over the mid-grey base darkens the ramp.
        assert!(app.image_gradient_blend_core("multiply"));
        assert_eq!(app.image.gradient_blend, "multiply");
        assert!(!app.image_gradient_blend_core("nope"));
        drag_gradient(&mut app);
        assert_ne!(layer_pixels(&app), plain, "the blend mode changes pixels");

        // Opacity: at the same blend mode, 20% leaves the base showing through
        // while 100% overwrites it.
        assert!(app.image_gradient_blend_core("normal"));
        assert!(app.image_slider_core("gradient-opacity", 20.0));
        assert!((app.image.gradient_opacity - 20.0).abs() < 1e-9);
        drag_gradient(&mut app);
        let faded = layer_pixels(&app);
        assert!(
            app.image_slider_core("gradient-opacity", 0.0),
            "clamped to 1"
        );
        assert!((app.image.gradient_opacity - 1.0).abs() < 1e-9);
        assert!(app.image_slider_core("gradient-opacity", 100.0));
        drag_gradient(&mut app);
        assert_ne!(layer_pixels(&app), faded, "opacity changes pixels");

        // Stop position: the mid stop moves the second colour inward.
        assert!(app.image_slider_core("gradient-stop", 30.0));
        assert!((app.image.gradient_stop - 0.3).abs() < 1e-9);
        let stopped = app.image.gradient_stops();
        assert!((stopped.last().map_or(0.0, |(_, position)| *position) - 0.3).abs() < 1e-9);
        assert!(!app.image_slider_core("gradient-nope", 1.0));
        Ok(())
    }

    #[test]
    fn the_nine_media_ratios_resize_the_board() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        // The nine `mediaRatioOptions` (minus `auto`).
        for ratio in [
            "1:1", "2:3", "3:2", "4:3", "3:4", "16:9", "9:16", "21:9", "9:21",
        ] {
            app.image.settings_open = true;
            assert!(
                app.image_settings_pick_core("ratio", ratio),
                "{ratio} applied"
            );
            assert_eq!(app.image.project.board_ratio, ratio);
            assert!(app.image.project.width >= 1.0 && app.image.project.height >= 1.0);
            assert!(!app.image.settings_open, "the popover closes after a pick");
        }
        assert!(
            !app.image_settings_pick_core("ratio", ""),
            "empty is ignored"
        );
        assert!(!app.image_settings_pick_core("nope", "1:1"));
        assert_eq!(app.image.project.board_ratio, "9:21");
        Ok(())
    }

    #[test]
    fn a_custom_background_colour_applies_without_closing_the_popover(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        app.image.settings_open = true;
        assert!(app.image_settings_pick_core("background-custom", "#336699"));
        assert_eq!(app.image.project.board_background, "#336699");
        assert!(
            app.image.settings_open,
            "the popover stays open for tweaking"
        );
        // An unparsable value is ignored.
        assert!(!app.image_settings_pick_core("background-custom", "junk"));
        assert_eq!(app.image.project.board_background, "#336699");
        Ok(())
    }

    #[test]
    fn zoom_bounds_and_step_match_the_web() {
        assert!((crate::image::ZOOM_MIN - 0.05).abs() < 1e-9);
        assert!((crate::image::ZOOM_MAX - 8.0).abs() < 1e-9);
        assert!((crate::image::ZOOM_STEP - 1.2).abs() < 1e-9);
    }

    #[test]
    fn wheel_up_zooms_in_at_the_cursor_and_wheel_down_zooms_out(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        app.image.canvas = (800.0, 600.0);
        app.image.fitted = "wheel:16x16".to_string();
        app.image.view = crate::image::ImageView {
            x: 100.0,
            y: 50.0,
            k: 1.0,
        };
        let (x, y) = (300.0, 200.0);
        let anchor = app.image_doc_point(x, y);

        // Slint reports wheel-up as a positive delta-y (the canvas workspace
        // relies on the same sign). Wheel-up must zoom in.
        app.image_wheel(x, y, 120.0);
        assert!(
            app.image.view.k > 1.0,
            "wheel-up zooms in: {}",
            app.image.view.k
        );
        let after = app.image_doc_point(x, y);
        assert!(
            (after.x - anchor.x).abs() < 1e-9 && (after.y - anchor.y).abs() < 1e-9,
            "the document point under the cursor stays put: {anchor:?} vs {after:?}"
        );

        let zoomed_in = app.image.view.k;
        app.image_wheel(x, y, -120.0);
        assert!(
            app.image.view.k < zoomed_in,
            "wheel-down zooms out: {} vs {zoomed_in}",
            app.image.view.k
        );
        assert!(
            (app.image.view.k - 1.0).abs() < 1e-9,
            "one step in then out returns to 1x: {}",
            app.image.view.k
        );
        Ok(())
    }

    #[test]
    fn a_stroke_with_no_pixel_layer_creates_one_with_a_hint(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        let mut gen = open_canvas_ps::IdGen::new("t");
        let text = open_canvas_ps::create_ps_text_layer(&mut gen, (16.0, 16.0), "Hi", "Text");
        let text_id = text.id.clone();
        app.image.project.board_layers = vec![text];
        app.image.active_layer = text_id;
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        let before = app.image.project.board_layers.len();

        app.image.tool = ImageTool::Brush;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            2.0,
            2.0,
            crate::input::Button::Left,
            false,
            false,
        );
        assert_eq!(
            app.image.project.board_layers.len(),
            before + 1,
            "a pixel layer was created"
        );
        assert!(app.image.auto_layer_hint, "the hint is flagged");
        let active = app
            .image
            .project
            .board_layers
            .iter()
            .find(|layer| layer.id == app.image.active_layer_id())
            .map(|layer| layer.kind);
        assert_eq!(active, Some(open_canvas_ps::CanvasPsLayerKind::Pixel));
        assert_eq!(
            app.image.history.label(1),
            None,
            "the gesture is not committed yet"
        );
        Ok(())
    }

    #[test]
    fn the_board_grows_to_fit_an_overflowing_layer() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = reorder_app()?;
        assert_eq!(
            (app.image.project.width, app.image.project.height),
            (16.0, 16.0)
        );
        // Inside the board: no growth.
        assert!(!app.grow_image_board_to_fit());
        // A layer pushed past the board edge, still inside the 16:9 preset.
        app.image.project.board_ratio = "16:9".to_string();
        app.image.project.board_layers = vec![open_canvas_ps::create_ps_pixel_layer(
            &mut open_canvas_ps::IdGen::new("t"),
            (16.0, 16.0),
            "Wide",
        )];
        if let Some(layer) = app.image.project.board_layers.first_mut() {
            layer.x = 40.0;
            layer.y = 0.0;
            layer.width = 16.0;
            layer.height = 16.0;
        }
        assert!(app.grow_image_board_to_fit(), "the board grew");
        assert_eq!(
            (app.image.project.width, app.image.project.height),
            (640.0, 360.0)
        );
        assert_eq!(app.image.board_w, "640");
        assert_eq!(app.image.board_h, "360");
        // Already at the preset size: no further growth.
        assert!(!app.grow_image_board_to_fit());
        Ok(())
    }

    #[test]
    fn colour_panel_sv_and_hsb_fields_round_trip() {
        let mut app = crate::app::App::new(crate::store::Store::at(std::env::temp_dir()));
        app.image.color = "#336699".to_string();
        assert!(app.image_color_field_core("sv", "50,80"));
        let hsb = open_canvas_ps::color::hex_to_hsb(&app.image.color);
        assert!((hsb[1] - 50.0).abs() < 0.01, "saturation from the square");
        assert!((hsb[2] - 80.0).abs() < 0.01, "brightness from the square");

        let hue = hsb[0];
        assert!(app.image_color_field_core("sv", "25,40"));
        let hsb = open_canvas_ps::color::hex_to_hsb(&app.image.color);
        assert!(
            (hsb[0] - hue).abs() < 2.0,
            "the square keeps the hue ({hue})"
        );

        assert!(app.image_color_field_core("h", "200"));
        let hsb = open_canvas_ps::color::hex_to_hsb(&app.image.color);
        assert!((hsb[0] - 200.0).abs() < 2.0);
        assert!((hsb[1] - 25.0).abs() < 2.0, "the strip keeps sat/val");
        assert!((hsb[2] - 40.0).abs() < 2.0);
        assert!(!app.image_color_field_core("sv", "junk"));
    }
}
