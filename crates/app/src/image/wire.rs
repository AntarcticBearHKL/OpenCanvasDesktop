//! Slint callback wiring for the Image Studio workspace (kept apart from the
//! state machine so both stay readable, mirroring `image/editor.rs`).
//! Owns the Image Studio callback registration.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc};

use crate::app::{with_app, App};
use crate::input::{Button, PointerKind};
use crate::AppWindow;

/// Registers the image callbacks and installs the row models.
pub fn wire_image(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    with_app(app, |app| {
        ui.set_image_tools(ModelRc::new(app.image_tools.clone()));
        ui.set_image_layers(ModelRc::new(app.image_layers.clone()));
        ui.set_image_history(ModelRc::new(app.image_history.clone()));
        ui.set_image_swatches(ModelRc::new(app.image_swatches.clone()));
        ui.set_image_params(ModelRc::new(app.image_params.clone()));
        ui.set_image_styles(ModelRc::new(app.image_styles.clone()));
        ui.set_image_fonts(ModelRc::new(slint::VecModel::from(font_rows())));
        app.sync_image(ui);
    });

    wire_image_simple(ui, app);
    wire_image_fields(ui, app);
    wire_image_canvas(ui, app);
    wire_image_panels(ui, app);
    wire_image_studio(ui, app);
    wire_dock(ui, app);
}

/// The shared dock engine's request channel (`DockStore.dock-request` in
/// `dock.slint`): the studio's dock chrome and tooltip targets report here,
/// and Rust clamps, persists and re-pushes. Registered once — the payload names
/// the studio.
fn wire_dock(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    let app = app.clone();
    let weak = ui.as_weak();
    ui.global::<crate::Theme>()
        .on_dock_request(move |studio, op, arg, detail, a, b| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.dock_request(
                    &ui,
                    studio.as_str(),
                    op.as_str(),
                    arg.as_str(),
                    detail.as_str(),
                    f64::from(a),
                    f64::from(b),
                );
            });
        });
}

/// Masks, guides, the text panel and the bitmap import flow (P5c-3 wave 2).
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the studio callbacks; splitting adds indirection, not clarity (precedent: wire_image_panels)"
)]
fn wire_image_studio(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_guide_drag(move |axis, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let horizontal = axis.as_str() == "y";
            with_app(&app, |app| {
                app.image_guide_drag(&ui, horizontal, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_guide_move(move |x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_guide_move(&ui, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_guide_drop(move |x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_guide_drop(&ui, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_mask_action(move |action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_mask_action(&ui, action.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_mask_target(move |layer, target, view| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_mask_target(&ui, layer.as_str(), target, view);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_set(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_row_set(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_text(move |key, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_row_text(&ui, key.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_option(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_option(&ui, key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_choice(move |key, index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_row_set(&ui, key.as_str(), f64::from(index));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_check(move |key, checked| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_row_check(&ui, key.as_str(), checked);
            });
        });
    }
    {
        let app = app.clone();
        ui.on_image_text_edit_changed(move |text| {
            with_app(&app, |app| app.image_text_edit_changed(text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_edit_commit(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_text_edit_commit(&ui, Some(text.as_str()));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_text_edit_cancel(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_text_edit_cancel(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_import_path_edited(move |path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_import_path(&ui, path.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_import_run(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_import_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_import_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_import_close(&ui));
        });
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the P5c-3 panel callbacks; splitting adds indirection, not clarity (precedent: image/editor.rs)"
)]
fn wire_image_panels(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_channel_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_channel_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_path_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_path_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_actions_panel(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_actions_panel(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_asset_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                if action.as_str() == "add" {
                    if let Some((project_id, layer_id)) = id.split_once('/') {
                        app.image_asset_add(&ui, project_id, layer_id);
                    }
                }
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_asset_drop(move |id, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                if let Some((project_id, layer_id)) = id.split_once('/') {
                    app.image_asset_drop(&ui, project_id, layer_id, f64::from(x), f64::from(y));
                }
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_assets_refresh(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_asset_refresh(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_asset_search_edited(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_asset_search(&ui, text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_panel_name_edited(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_panel_name_edited(&ui, text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_adjustment_create(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_adjustment_create(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_gradient_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_gradient_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_pattern_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_pattern_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_brush_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_brush_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_brush_slider(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_brush_slider(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_set(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_set(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_text(move |key, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_text(&ui, key.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_option(move |key, option| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_option(&ui, key.as_str(), option.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_check(move |key, checked| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_check(&ui, key.as_str(), checked);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_curve_add(move |x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_curve_add(&ui, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_curve_move(move |index, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_curve_move(
                    &ui,
                    usize::try_from(index).unwrap_or(usize::MAX),
                    f64::from(x),
                    f64::from(y),
                );
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_curve_delete(move |index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_curve_delete(&ui, usize::try_from(index).unwrap_or(usize::MAX));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_stop_add(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_editor_stop_add(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_stop_select(move |index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_stop_select(&ui, usize::try_from(index).unwrap_or(usize::MAX));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_stop_remove(move |index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_stop_remove(&ui, usize::try_from(index).unwrap_or(usize::MAX));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_stop_color(move |index, hex| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_stop_color(
                    &ui,
                    usize::try_from(index).unwrap_or(usize::MAX),
                    hex.as_str(),
                );
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_editor_stop_pos(move |index, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_editor_stop_pos(
                    &ui,
                    usize::try_from(index).unwrap_or(usize::MAX),
                    f64::from(value),
                );
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_param(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_filter_param(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_param_text(move |key, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_filter_param_text(&ui, key.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_reset(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_filter_reset(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_apply(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_filter_apply_dialog(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_cancel(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_filter_cancel(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_filter_push(move |x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_filter_push(&ui, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_modal_field(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_modal_field(&ui, key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_modal_toggle(move |key| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_modal_toggle(&ui, key.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_modal_run(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_modal_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_modal_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_modal_close(&ui));
        });
    }
}

/// The Photoshop text layer families (`PS_FONT_FAMILIES`, ps-text.ts:8).
fn font_rows() -> Vec<slint::SharedString> {
    open_canvas_ps::text::PS_FONT_FAMILIES
        .iter()
        .map(|family| (*family).into())
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the image panel callbacks; splitting adds indirection, not clarity (precedent: image/editor.rs)"
)]
fn wire_image_simple(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_action(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_action(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_tool(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_toggle(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_toggle(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_slider(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_slider(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_layer_pick(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_layer_pick(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_layer_action(move |action, id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_layer_action(&ui, action.as_str(), id.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_layer_opacity_changed(move |value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_layer_opacity(&ui, f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_layer_blend_changed(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_layer_blend(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_layer_name_edited(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_layer_name_edited(&ui, text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_history_pick(move |index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_history_pick(&ui, index));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_shape_pick(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_shape_pick(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_settings_pick(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_settings_pick(&ui, key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_board_apply(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_board_apply(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_slot_pick(move |slot| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_slot_pick(&ui, slot.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_swatch_add(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_swatch_add(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_swatch_remove(move |index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_swatch_remove(&ui, index));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_color_swap(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_color_swap(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_color_reset(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_color_reset(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_font_pick(move |family| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_font_pick(&ui, family.as_str()));
        });
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the image field/export callbacks; splitting adds indirection, not clarity (precedent: image/editor.rs)"
)]
fn wire_image_fields(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_color_field(move |field, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_color_field(&ui, field.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_color_pick(move |hex| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_color_pick(&ui, hex.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_prop_field(move |key, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_apply_field(&ui, key.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_param_set(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_param_set(&ui, key.as_str(), f64::from(value));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_export(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_export_open(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_export_path_edited(move |path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_export_path(&ui, path.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_export_run(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_export_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_export_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_export_close(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_psd_export_path_edited(move |path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_psd_export_path(&ui, path.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_psd_export_run(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_psd_export_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_psd_export_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_psd_export_close(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_key(move |text, ctrl, shift| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_key_down(&ui, text.as_str(), ctrl, shift);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_key_up(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.image_key_up(&ui, text.as_str()));
        });
    }
}

fn wire_image_canvas(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_canvas_pointer(move |kind, x, y, button, shift, alt| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (Some(kind), Some(button)) = (
                PointerKind::from_id(kind.as_str()),
                Button::from_id(button.as_str()),
            ) else {
                return;
            };
            with_app(&app, |app| {
                app.image_pointer(&ui, kind, f64::from(x), f64::from(y), button, shift, alt);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_canvas_scroll(move |x, y, delta_y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_scroll(&ui, f64::from(x), f64::from(y), f64::from(delta_y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_canvas_double_click(move |x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.image_canvas_double_click(&ui, f64::from(x), f64::from(y));
            });
        });
    }
}
