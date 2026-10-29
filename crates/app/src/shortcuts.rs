//! Keyboard shortcuts and dock actions, plus the node-enter animation timer.

use slint::Timer;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::{with_app, App, View, ENTER_DELAY};
use crate::AppWindow;

impl App {
    pub fn key_down(&mut self, ui: &AppWindow, text: &str, ctrl: bool, shift: bool) {
        // Canvas shortcuts belong to the editor; in the library the keyboard
        // belongs to the inline rename inputs — except Escape, which still
        // dismisses the library's one overlay (the export/import modal).
        if self.view == View::Library {
            if text == "\u{1b}" && self.library.export.is_some() {
                self.export_import_close(ui);
            }
            return;
        }
        match text {
            " " => {
                self.space_held = true;
                self.sync_flags(ui);
            }
            "?" => self.open_shortcuts(ui),
            "\u{1b}" => {
                if self.shortcuts_open {
                    self.close_shortcuts(ui);
                } else if !self.close_overlays(ui) {
                    self.doc.clear_selection();
                    self.doc.cancel_drag();
                    ui.set_drag_active(false);
                    self.panning = false;
                    self.sync_all(ui);
                }
            }
            "\u{8}" | "\u{7f}" => self.delete_selection(ui),
            _ if ctrl => match text.to_ascii_lowercase().as_str() {
                "z" if shift => self.redo(ui),
                "z" => self.undo(ui),
                "y" => self.redo(ui),
                "a" => {
                    self.doc.select_all();
                    self.sync_selection(ui);
                    self.sync_flags(ui);
                }
                "c" => self.copy_nodes(ui),
                "x" => self.cut_nodes(ui),
                "v" => self.paste_nodes(ui),
                "0" => self.zoom_reset(ui),
                "1" => self.zoom_fit(ui),
                _ => {}
            },
            _ => {}
        }
    }

    pub fn key_up(&mut self, ui: &AppWindow, text: &str) {
        if text == " " {
            self.space_held = false;
            self.sync_flags(ui);
        }
    }

    pub fn undo(&mut self, ui: &AppWindow) {
        if self.doc.undo() {
            self.prune_plugin_state();
            self.note_doc_change();
            self.sync_all(ui);
        }
    }

    pub fn redo(&mut self, ui: &AppWindow) {
        if self.doc.redo() {
            self.prune_plugin_state();
            self.note_doc_change();
            self.sync_all(ui);
        }
    }

    pub fn delete_selection(&mut self, ui: &AppWindow) {
        // With no node selected, Delete removes the selected wire
        // (project.tsx:1234-1238).
        if self.doc.selection().is_empty() {
            if let Some(id) = self.doc.selected_connection().map(str::to_string) {
                if self.doc.delete_connection(&id, self.now_ms()) {
                    self.note_doc_change();
                    self.sync_all(ui);
                }
                return;
            }
        }
        // A loading image node is a running generation's placeholder: deleting
        // it is the stop gesture. Cancel the owning task before the node goes.
        let selected: Vec<String> = self.doc.selection().iter().cloned().collect();
        for id in selected {
            self.cancel_generation_for_node(ui, &id);
        }
        if self.doc.delete_selection(self.now_ms()) {
            self.prune_plugin_state();
            self.note_doc_change();
            self.sync_all(ui);
        }
    }
}

pub fn start_node_enter(app: &Rc<RefCell<App>>, id: &str) {
    let weak_app = Rc::downgrade(app);
    let id = id.to_string();
    Timer::single_shot(ENTER_DELAY, move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        with_app(&app, |app| app.set_enter(&id, 1.0));
    });
}
