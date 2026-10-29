//! Handlers and Slint wiring for the canvas interaction wave: copy/paste, the
//! zoom commands, the shortcuts modal and the side panel.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::app::{with_app, App};
use crate::clipboard;
use crate::coords::ScreenPos;
use crate::input::PointerKind;
use crate::store::clamp_side_panel_width;
use crate::system_clipboard;
use crate::AppWindow;

impl App {
    /// Ctrl+C: captures the selection and mirrors it to the system clipboard
    /// (`copySelectedNodes`, use-canvas-document.ts:116-134).
    pub fn copy_nodes(&mut self, _ui: &AppWindow) {
        let Some(clipboard) = self.doc.copy_selection() else {
            return;
        };
        if let Ok(text) = clipboard::to_clipboard_text(&clipboard) {
            system_clipboard::write_text(&text);
        }
        self.clipboard = Some(clipboard);
    }

    /// Ctrl+X: copy, then delete the copied selection.
    pub fn cut_nodes(&mut self, ui: &AppWindow) {
        if self.doc.selection().is_empty() {
            self.delete_selection(ui);
            return;
        }
        let Some(clipboard) = self.doc.copy_selection() else {
            return;
        };
        if let Ok(text) = clipboard::to_clipboard_text(&clipboard) {
            system_clipboard::write_text(&text);
        }
        self.clipboard = Some(clipboard);
        self.delete_selection(ui);
    }

    /// Ctrl+V: a bitmap on the system clipboard wins over the node register and
    /// becomes a new Image node at the viewport centre. Otherwise the internal
    /// node register is pasted, else the clipboard text envelope is parsed
    /// (`if (!pasteCopiedNodes()) void pasteSystemClipboard()`, project.tsx:1231).
    pub fn paste_nodes(&mut self, ui: &AppWindow) {
        let now = self.now_ms();
        if let Some(image) = system_clipboard::read_image() {
            if let Some(surface) =
                open_canvas_ps::Surface::from_rgba(image.width, image.height, image.rgba)
            {
                let centre = ScreenPos::new(self.canvas_size.0 / 2.0, self.canvas_size.1 / 2.0);
                let at = self.doc.viewport().screen_to_world(centre);
                if self
                    .insert_image_node("Pasted Image", &surface, at, now)
                    .is_some()
                {
                    self.note_doc_change();
                    self.sync_all(ui);
                    return;
                }
            }
        }
        let pasted = if let Some(clipboard) = self.clipboard.clone() {
            self.doc.paste_nodes(&clipboard, now)
        } else if let Some(text) = system_clipboard::read_text() {
            self.doc
                .paste_clipboard_text(&text, now)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if pasted.is_empty() {
            return;
        }
        self.note_doc_change();
        self.sync_all(ui);
    }

    /// Ctrl+0 / "Reset view": world origin back at the canvas center at 100%.
    pub fn zoom_reset(&mut self, ui: &AppWindow) {
        self.doc.reset_viewport(self.canvas_size);
        self.sync_view(ui);
        self.sync_overlays(ui);
        self.note_view_change();
        self.note_zoom_change(ui);
    }

    /// Ctrl+1 / "Fit to content": frames the visible node bounds.
    pub fn zoom_fit(&mut self, ui: &AppWindow) {
        self.doc.fit_to_content(self.canvas_size);
        self.sync_view(ui);
        self.sync_overlays(ui);
        self.note_view_change();
        self.note_zoom_change(ui);
    }

    /// Opens the shortcuts reference (`?`; the zoom panel that used to carry
    /// the link is gone).
    pub fn open_shortcuts(&mut self, ui: &AppWindow) {
        self.shortcuts_open = true;
        ui.set_shortcuts_open(true);
    }

    pub fn close_shortcuts(&mut self, ui: &AppWindow) {
        self.shortcuts_open = false;
        ui.set_shortcuts_open(false);
    }

    /// Dock side-panel toggle; the open flag persists immediately.
    pub fn toggle_side_panel(&mut self, ui: &AppWindow) {
        self.side_open = !self.side_open;
        ui.set_side_panel_open(self.side_open);
        self.persist_ui_state();
    }

    /// Side-panel resize handle: pointer events carry the window x, so the
    /// width follows the pointer 1:1 clamped to 220..480 and persists on
    /// release (`canvas-side-panel.tsx:28-41`).
    pub fn side_resize(&mut self, ui: &AppWindow, kind: PointerKind, x: f64) {
        match kind {
            PointerKind::Down => {
                self.side_drag = Some((x, self.side_width));
                ui.set_side_panel_resizing(true);
                if !self.side_open {
                    self.side_open = true;
                    ui.set_side_panel_open(true);
                }
            }
            PointerKind::Move => {
                if let Some((start, width)) = self.side_drag {
                    self.side_width = clamp_side_panel_width(width + (x - start));
                    ui.set_side_panel_width(crate::vm::len(self.side_width));
                }
            }
            PointerKind::Up | PointerKind::Cancel => {
                ui.set_side_panel_resizing(false);
                if self.side_drag.take().is_some() {
                    self.persist_ui_state();
                }
            }
        }
    }
}

/// Registers the interaction-wave callbacks.
pub fn wire_edit_events(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_shortcuts_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.close_shortcuts(&ui));
        });
    }
    {
        // The top bar's panel button emits the root `toggle-panel` callback.
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_toggle_panel(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.toggle_side_panel(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_resize(move |kind, x| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(kind) = PointerKind::from_id(kind.as_str()) else {
                return;
            };
            with_app(&app, |app| app.side_resize(&ui, kind, f64::from(x)));
        });
    }
}
