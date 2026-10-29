//! Slint event wiring: registers every callback on the window.

use slint::{ComponentHandle, Timer, TimerMode};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::{with_app, App, HISTORY_TICK};
use crate::coords::ScreenPos;
use crate::input::{Button, PointerEvent, PointerKind};
use crate::AppWindow;
pub fn wire(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    with_app(app, |app| app.install(ui));
    with_app(app, |app| app.apply_appearance(ui));

    let weak_app = Rc::downgrade(app);
    let weak_ui = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, HISTORY_TICK, move || {
        let (Some(ui), Some(app)) = (weak_ui.upgrade(), weak_app.upgrade()) else {
            return;
        };
        with_app(&app, |app| {
            let now = app.now_ms();
            if app.doc.tick_history(now) {
                app.sync_flags(&ui);
            }
            app.tick_save(&ui);
            app.tick_image(&ui);
            app.tick_notices(&ui);
            app.tick_zoom_badge(&ui);
            // The hover toolbar is anchored to the node's current rect in the
            // current viewport, so a window move/resize, a pan or a node drag
            // must not leave it behind: recompute it on every app tick.
            app.sync_hover_bar(&ui);
            app.tick_plugin_refresh(&ui);
            app.poll_ai(&ui);
            app.poll_system_theme(&ui);
            app.poll_logs(&ui);
        });
    });
    with_app(app, |app| app.keep_timer(timer));
    with_app(app, |app| {
        if let Some(error) = app.startup_error.take() {
            app.set_notice(ui, error, true);
        }
    });
    wire_pointers(ui, app);
    wire_actions(ui, app);
    wire_chrome(ui, app);
    crate::edit_events::wire_edit_events(ui, app);
    crate::appearance::wire_appearance(ui, app);
    crate::config_tabs::wire_config_tabs(ui, app);
    crate::bar_events::wire_bars(ui, app);
    crate::content_events::wire_content(ui, app);
    crate::node_ui::wire_node_ui(ui, app);
    crate::attempt_ui::wire(ui, app);
    crate::library_wire::wire_library(ui, app);
    crate::image::wire::wire_image(ui, app);
    crate::config_ui::wire_config(ui, app);
    crate::plugin_wire::wire_plugins(ui, app);
    crate::image::tools::wire_image_tools(ui, app);
}

/// Registers the pointer callbacks (background, node, resize, connect).
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of eight callbacks; splitting adds indirection, not clarity"
)]
fn wire_pointers(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_canvas_pointer(move |kind, x, y, button, shift, ctrl| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (Some(kind), Some(button)) = (
                PointerKind::from_id(kind.as_str()),
                Button::from_id(button.as_str()),
            ) else {
                return;
            };
            let event = PointerEvent {
                kind,
                pos: ScreenPos::new(f64::from(x), f64::from(y)),
                button,
                shift,
                ctrl,
            };
            with_app(&app, |app| {
                app.last_canvas_pos = Some(event.pos);
                app.canvas_pointer(&ui, &event);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_canvas_scroll(move |x, y, delta_y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let pos = ScreenPos::new(f64::from(x), f64::from(y));
            with_app(&app, |app| app.canvas_scroll(&ui, pos, f64::from(delta_y)));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_canvas_resized(move |width, height| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.canvas_resized(&ui, f64::from(width), f64::from(height));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_pointer(move |id, kind, x, y, button, shift| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (Some(kind), Some(button)) = (
                PointerKind::from_id(kind.as_str()),
                Button::from_id(button.as_str()),
            ) else {
                return;
            };
            let event = PointerEvent {
                kind,
                pos: ScreenPos::new(f64::from(x), f64::from(y)),
                button,
                shift,
                ctrl: false,
            };
            with_app(&app, |app| app.node_pointer(&ui, id.as_str(), &event));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_resize_pointer(move |id, corner, kind, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(kind) = PointerKind::from_id(kind.as_str()) else {
                return;
            };
            let event = PointerEvent {
                kind,
                pos: ScreenPos::new(f64::from(x), f64::from(y)),
                button: Button::Left,
                shift: false,
                ctrl: false,
            };
            with_app(&app, |app| {
                app.resize_pointer(&ui, id.as_str(), corner.as_str(), &event);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_connect_pointer(move |id, handle, kind, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(kind) = PointerKind::from_id(kind.as_str()) else {
                return;
            };
            let event = PointerEvent {
                kind,
                pos: ScreenPos::new(f64::from(x), f64::from(y)),
                button: Button::Left,
                shift: false,
                ctrl: false,
            };
            with_app(&app, |app| {
                app.connect_pointer(&ui, id.as_str(), handle.as_str(), &event);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_layer_move(move |direction| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_layer_move(&ui, direction.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_layer_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.close_node_layer(&ui));
        });
    }
}

/// Registers the keyboard and dock-action callbacks.
fn wire_actions(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_key_down(move |text, ctrl, shift| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.key_down(&ui, text.as_str(), ctrl, shift);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_key_up(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.key_up(&ui, text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_undo(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.undo(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_redo(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.redo(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_delete_selection(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.delete_selection(&ui));
        });
    }
    {
        // Dock-bar Home: saves and returns to the unfiltered library.
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_go_home(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.go_home(&ui));
        });
    }
}

/// Registers the custom-chrome window buttons and the manual title-bar drag.
fn wire_chrome(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let weak = ui.as_weak();
        ui.on_win_minimize(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::App::minimize_window(&ui);
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_win_maximize_toggle(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.toggle_maximize(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_win_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            // Flush the appearance preferences (the theme pick already wrote
            // them, but this closes any gap) and stop the event loop.
            with_app(&app, |app| {
                app.persist_ui_state();
            });
            ui.window().hide().ok();
            slint::quit_event_loop().ok();
        });
    }
    wire_window_drag(ui, app);
}

/// The manual window move: begin / tick / end. Replacing Slint's
/// `WindowMoveArea` (a native modal loop on Windows) with this keeps the app's
/// timers, and therefore its 8 ms frame tick, running during the drag.
///
/// A dedicated 4 ms timer samples the cursor while a drag is in flight. The
/// frame tick alone is enough to keep the window alive, but 250 Hz sampling is
/// what makes the window follow the pointer instead of trailing it; the timer
/// only runs between `drag-begin` and `drag-end`, so an idle app pays nothing.
const DRAG_TICK_MS: u64 = 4;

fn wire_window_drag(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    let drag_timer = Rc::new(slint::Timer::default());
    {
        let app = app.clone();
        let weak = ui.as_weak();
        let drag_timer = drag_timer.clone();
        ui.on_drag_begin(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.drag_begin(&ui));
            let app = app.clone();
            let weak = weak.clone();
            let timer = drag_timer.clone();
            drag_timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(DRAG_TICK_MS),
                move || {
                    let Some(ui) = weak.upgrade() else {
                        timer.stop();
                        return;
                    };
                    with_app(&app, |app| app.drag_tick(&ui));
                },
            );
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_drag_tick(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.drag_tick(&ui));
        });
    }
    {
        let app = app.clone();
        let drag_timer = drag_timer.clone();
        ui.on_drag_end(move || {
            drag_timer.stop();
            with_app(&app, App::drag_end);
        });
    }
}
