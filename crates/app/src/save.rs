//! Debounced autosave for the open document.
//!
//! The web persists the project list through a 400 ms debounce
//! (`use-canvas-store.ts:57-68`, `createDebouncedPersist`) and the viewport
//! through a separate 500 ms debounce (`project.tsx:394-404`); every document
//! change here marks the editor dirty with the matching window and the 60 ms app
//! tick flushes once the window elapsed — one write per burst, no save storms.
//! Saving also refreshes the project's library row (title/timestamps) and the
//! index itself. `Home`/project switches flush synchronously.

use crate::app::{App, View};
use crate::store::{self, StoreError};
use crate::AppWindow;

/// Document-change debounce (web store debounce).
pub const DOC_SAVE_DEBOUNCE_MS: u64 = 400;
/// Viewport-change debounce (web viewport debounce).
pub const VIEW_SAVE_DEBOUNCE_MS: u64 = 500;

impl App {
    /// A document mutation happened (nodes, connections, metadata).
    pub fn note_doc_change(&mut self) {
        self.mark_dirty(DOC_SAVE_DEBOUNCE_MS);
    }

    /// Only the viewport moved (pan/zoom/marquee-recenter).
    pub fn note_view_change(&mut self) {
        self.mark_dirty(VIEW_SAVE_DEBOUNCE_MS);
    }

    fn mark_dirty(&mut self, delay_ms: u64) {
        if self.editor.is_none() && self.image.meta.is_none() {
            return;
        }
        self.save_dirty = true;
        self.save_due = Some(self.now_ms().saturating_add(delay_ms));
    }

    /// Autosave check, called from the history tick while the window lives.
    pub fn tick_save(&mut self, ui: &AppWindow) {
        if !self.save_dirty {
            return;
        }
        if self.save_due.is_some_and(|due| self.now_ms() >= due) {
            self.flush_save(ui);
        }
    }

    /// Writes the open document when dirty (Home, project switch, debounce).
    pub fn flush_save(&mut self, ui: &AppWindow) {
        if !self.save_dirty {
            return;
        }
        self.save_dirty = false;
        self.save_due = None;
        match self.save_now() {
            Ok(()) => {
                self.sync_notice(ui);
                if self.view == View::Library {
                    self.sync_library(ui);
                }
            }
            Err(error) => self.set_notice(ui, format!("Autosave failed: {error}"), true),
        }
    }

    /// The write itself (no UI): document file, library row, index file. A
    /// failure leaves the caller free to report it; the dirty flag is already
    /// cleared so a broken disk cannot spin the tick in a retry storm.
    pub fn save_now(&mut self) -> Result<(), StoreError> {
        if self.view == View::Image {
            return self.image_save_now();
        }
        self.canvas_save_now()
    }

    /// Writes the open canvas project and refreshes its library row.
    fn canvas_save_now(&mut self) -> Result<(), StoreError> {
        let Some(mut meta) = self.editor.clone() else {
            return Ok(());
        };
        self.prune_plugin_state();
        meta.updated_at = store::now_iso();
        let project = self.doc.to_project(&meta);
        self.store.save_project(&project)?;
        let extras = self.plugin.extras_snapshot();
        self.store.save_project_extras(&project.id, &extras)?;
        self.library.index.upsert_project(&project);
        self.store.save_index(&self.library.index)?;
        self.editor = Some(meta);
        Ok(())
    }
}
