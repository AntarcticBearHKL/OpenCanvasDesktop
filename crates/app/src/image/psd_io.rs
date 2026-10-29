//! File > Export PSD... glue: paths, prompts and library state.
//!
//! The document ↔ PSD mapping itself is pure and lives in
//! [`open_canvas_ps::psd_io`]; this module only wraps it for the app: it writes
//! files and threads the app's glyph rasterizer into exports.
//!
//! Export writes one PSD layer per project layer (flattening text, shapes,
//! adjustments, styles and transforms through the compositor) plus the merged
//! composite.

use std::collections::HashMap;
use std::path::PathBuf;

use open_canvas_ps::psd_io::export_project_with_text;
use open_canvas_ps::{ImageProject, Surface, TextRasterizer};

use crate::app::App;
use crate::image::ImageExport;
use crate::AppWindow;

/// Export `project` as a PSD/PSB byte string, with the app's glyph rasterizer
/// so text layers become pixels.
pub(crate) fn export_psd_bytes(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
) -> Result<Vec<u8>, String> {
    let text = crate::image::text_raster::shared();
    export_project_with_text(
        project,
        bitmaps,
        text.map(|rasterizer| rasterizer as &dyn TextRasterizer),
    )
    .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// App flows (File menu)
// ---------------------------------------------------------------------------

impl App {
    /// File > Export PSD...: path prompt next to the project's export folder.
    pub fn image_psd_export_open(&mut self, ui: &AppWindow) {
        let Some(meta) = self.image.meta.clone() else {
            return;
        };
        let path = self
            .store
            .export_path_for(&meta.title)
            .with_extension("psd");
        self.image.psd_export = Some(ImageExport {
            path: path.display().to_string(),
            message: String::new(),
            error: false,
        });
        self.image.menu.clear();
        self.sync_image(ui);
    }

    pub fn image_psd_export_path(&mut self, _ui: &AppWindow, path: &str) {
        if let Some(flow) = self.image.psd_export.as_mut() {
            flow.path = path.to_string();
        }
    }

    pub fn image_psd_export_close(&mut self, ui: &AppWindow) {
        self.image.psd_export = None;
        self.sync_image(ui);
    }

    /// Runs the export; failures stay in the modal as an error line.
    pub fn image_psd_export_run(&mut self, ui: &AppWindow) {
        let Some(flow) = self.image.psd_export.clone() else {
            return;
        };
        let trimmed = flow.path.trim();
        if trimmed.is_empty() {
            self.set_psd_export_message(ui, "Enter a file path first".to_string());
            return;
        }
        let path = PathBuf::from(trimmed);
        let result = export_psd_bytes(&self.image.project, &self.image.bitmaps)
            .and_then(|bytes| crate::image::export::write_bytes(&path, &bytes));
        match result {
            Ok(()) => {
                self.image.psd_export = None;
                self.set_notice(ui, format!("Exported to {}", path.display()), false);
                self.sync_image(ui);
            }
            Err(message) => self.set_psd_export_message(ui, message),
        }
    }

    fn set_psd_export_message(&mut self, ui: &AppWindow, message: String) {
        if let Some(flow) = self.image.psd_export.as_mut() {
            flow.message = message;
            flow.error = true;
        }
        self.sync_image(ui);
    }
}
