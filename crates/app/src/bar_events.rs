//! Handlers and Slint wiring for the floating bars and modals.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc};

use open_canvas_core::alignment::{align_nodes, AlignAxis};
use open_canvas_core::node::CanvasNodeType;

use crate::app::{with_app, App};
use crate::bars::{self, Menu};
use crate::AppWindow;

/// Hover-intent grace: the pointer crosses a 14px canvas gap between the node
/// and the bar, and the bar must stay visible until the pointer arrives — an
/// invisible bar is not hit-testable, so an early hide would drop the click.
const HOVER_GRACE: Duration = Duration::from_millis(160);
/// The grace is checked one tick after it can expire (timer granularity).
const HOVER_CHECK_DELAY: Duration = Duration::from_millis(210);

impl App {
    pub fn node_hover_entered(&mut self, ui: &AppWindow, id: &str) {
        self.bars.hover_node = Some(id.to_string());
        self.bars.hide_at = None;
        self.sync_hover_bar(ui);
        self.sync_related();
    }

    /// Starts the leave grace; entering the bar (or the gap bridge) cancels it.
    pub fn node_hover_left(&mut self, ui: &AppWindow) {
        self.bars.hide_at = Some(Instant::now() + HOVER_GRACE);
        self.sync_hover_bar(ui);
    }

    /// The bar itself keeps the toolbar alive while the pointer is on it
    /// (canvas-node-hover-toolbar.tsx:190-197 `onKeep`/`onLeave`).
    pub fn bar_hover(&mut self, ui: &AppWindow, hovered: bool) {
        self.bars.bar_hover = hovered;
        if hovered {
            self.bars.hide_at = None;
        } else {
            self.bars.hide_at = Some(Instant::now() + HOVER_GRACE);
        }
        self.sync_hover_bar(ui);
    }

    /// A bar button press freezes the bar until release (`clicked` needs the
    /// same repeater item to see both the press and the release).
    pub fn bar_press(&mut self, ui: &AppWindow, pressed: bool) {
        self.bars.bar_press = pressed;
        self.sync_hover_bar(ui);
    }

    /// The leave grace elapsed: drop the hovered node unless the pointer is on
    /// or pressed over the bar, then let `sync_hover_bar` hide or keep. A stale
    /// timer (no pending deadline: the pointer re-entered) does nothing.
    pub fn hover_grace_expired(&mut self, ui: &AppWindow) {
        if self.bars.interacting() {
            return;
        }
        let Some(deadline) = self.bars.hide_at else {
            return;
        };
        if Instant::now() < deadline {
            return;
        }
        self.bars.hide_at = None;
        self.bars.hover_node = None;
        self.sync_hover_bar(ui);
        self.sync_related();
    }

    pub fn hover_action(&mut self, ui: &AppWindow, action: &str) {
        let Some(id) = bars::toolbar_node_id(self) else {
            return;
        };
        let now = self.now_ms();
        if action.starts_with("plugin-") {
            self.plugin_action(ui, &id, action);
            return;
        }
        match action {
            "info" => {
                self.bars.info_node = Some(id);
                self.bars.info_json = false;
                ui.set_info_json(false);
                self.sync_modals(ui);
                self.sync_hover_bar(ui);
            }
            "duplicate" => {
                if let Some(copy) = self.doc.duplicate_node(&id, now) {
                    self.plugin.copy_extras(&id, &copy);
                }
                self.note_doc_change();
                self.sync_all(ui);
            }
            "delete" => {
                self.cancel_generation_for_node(ui, &id);
                self.doc.select_node(&id, false);
                self.doc.delete_selection(now);
                self.prune_plugin_state();
                self.note_doc_change();
                self.sync_all(ui);
            }
            "font-up" | "font-down" => {
                let Some(node) = self.doc.node(&id) else {
                    return;
                };
                let current = node
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.font_size)
                    .unwrap_or(14.0);
                let next = bars::font_step(current, action == "font-up");
                if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                    if let Some(meta) = node.metadata.as_mut() {
                        meta.font_size = Some(next);
                    }
                }
                self.doc.commit(now);
                self.note_doc_change();
                self.sync_all(ui);
            }
            "attempt-generate" => self.attempt_generate_to_image(ui, &id),
            "layers" => self.toggle_node_layers(ui, &id),
            // Local image quick tools (`image_tools`): every one of them either
            // opens a dialog or acts directly on the node's stored bitmap.
            "crop" | "mask" | "split" | "resolution" | "analyze" => {
                self.image_tool_open(ui, &id, action);
            }
            // AI-backed image tools (`image_ai`): OCR, multi-angle and
            // segmentation dialogs.
            "ocr" | "segment" | "angle" => {
                self.image_ai_open(ui, &id, action);
            }
            "copy" => match self.image_copy_core(&id) {
                Ok(message) => self.set_notice(ui, message, false),
                Err(message) => self.set_notice(ui, message, true),
            },
            "download" => match self.image_download_core(&id) {
                Ok(message) => self.set_notice(ui, message, false),
                Err(message) => self.set_notice(ui, message, true),
            },
            _ => {}
        }
    }

    /// Double-click dispatch (`canvas-node.tsx:411-426`): plugin edit toggle,
    /// then the prompt body's editor, then the Text node's in-place editor, then
    /// the image/stack preview. A single click never reaches this (it goes to
    /// the drag gesture), so a prompt card drags on one click and edits on two.
    pub fn node_double_click(&mut self, ui: &AppWindow, id: &str) {
        if self.toggle_plugin_editing(ui, id) {
            return;
        }
        let Some(node) = self.doc.node(id).cloned() else {
            return;
        };
        if open_canvas_core::geometry::is_node_locked(&node) {
            return;
        }
        if node.node_type == CanvasNodeType::ImageAttempt {
            self.attempt_open(ui, id);
            return;
        }
        if matches!(
            node.node_type,
            CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt
        ) {
            self.panel_prompt_edit_begin(ui, id);
            return;
        }
        if node.node_type == CanvasNodeType::Text {
            self.node_edit_begin(ui, id, crate::inline_edit::EditTarget::Body.as_str());
            return;
        }
        // A Smart Canvas card is the entry point into its Image Studio project:
        // double-click opens the studio, with the canvas remembered as origin.
        if node.node_type == CanvasNodeType::SmartCanvas {
            self.open_smart_canvas_node(ui, id);
            return;
        }
        if !bars::previewable(&node.node_type) {
            return;
        }
        self.bars.preview_node = Some(id.to_string());
        self.sync_modals(ui);
    }

    pub fn selection_menu(&mut self, ui: &AppWindow, id: &str) {
        let next = Menu::from_id(id);
        self.bars.menu = match (self.bars.menu, next) {
            (Some(current), Some(clicked)) if current == clicked => None,
            (_, clicked) => clicked,
        };
        self.sync_selection_ui(ui);
    }

    /// Align/distribute via `core::alignment` and commit one history entry
    /// (project.tsx `alignSelection`).
    pub fn align_action(&mut self, ui: &AppWindow, axis_id: &str) {
        let Some(axis) = AlignAxis::from_id(axis_id) else {
            return;
        };
        let selected: Vec<String> = self.doc.selection().iter().cloned().collect();
        let positions = align_nodes(self.doc.nodes(), &selected, axis);
        if positions.is_empty() {
            return;
        }
        for (id, position) in &positions {
            if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == *id) {
                node.position = *position;
            }
        }
        self.doc.commit(self.now_ms());
        self.note_doc_change();
        self.bars.menu = None;
        self.sync_all(ui);
    }

    pub fn info_view_set(&mut self, ui: &AppWindow, json: bool) {
        self.bars.info_json = json;
        ui.set_info_json(json);
        self.sync_modals(ui);
    }

    pub fn modal_close(&mut self, ui: &AppWindow) {
        self.bars.info_node = None;
        self.bars.preview_node = None;
        self.sync_modals(ui);
    }
}

/// Registers the floating-bar/modal callbacks and installs their models.
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the bar callbacks; splitting adds indirection, not clarity"
)]
pub fn wire_bars(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    with_app(app, |app| {
        ui.set_hover_actions(ModelRc::new(app.hover_actions.clone()));
        ui.set_info_rows(ModelRc::new(app.info_rows.clone()));
    });

    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_hover(move |id, hovered| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if hovered {
                with_app(&app, |app| app.node_hover_entered(&ui, id.as_str()));
            } else {
                with_app(&app, |app| app.node_hover_left(&ui));
                schedule_hover_check(&app, &ui);
            }
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_double_click(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_double_click(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_bar_hover(move |hovered| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.bar_hover(&ui, hovered));
            if !hovered {
                schedule_hover_check(&app, &ui);
            }
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_bar_press(move |pressed| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.bar_press(&ui, pressed));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_hover_action(move |action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.hover_action(&ui, action.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_selection_menu(move |menu| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.selection_menu(&ui, menu.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_align_action(move |axis| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.align_action(&ui, axis.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_info_view(move |json| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.info_view_set(&ui, json));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_modal_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.modal_close(&ui));
        });
    }
}

/// Re-evaluates the bar one tick after the leave grace can expire, so the check
/// sees the final hover state even if the pointer crossed the gap.
fn schedule_hover_check(app: &Rc<RefCell<App>>, ui: &AppWindow) {
    let weak_app = Rc::downgrade(app);
    let weak_ui = ui.as_weak();
    slint::Timer::single_shot(HOVER_CHECK_DELAY, move || {
        let (Some(app), Some(ui)) = (weak_app.upgrade(), weak_ui.upgrade()) else {
            return;
        };
        with_app(&app, |app| app.hover_grace_expired(&ui));
    });
}
