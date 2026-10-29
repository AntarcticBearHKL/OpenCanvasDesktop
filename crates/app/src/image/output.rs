//! Image Studio import: a bitmap file becomes a new Image layer through the
//! shared path prompt (the layers panel's Import button).
//! Owns the import prompt (`ImageImport`).

use std::path::Path;

use open_canvas_ps::decode_rgba;

use crate::app::App;
use crate::AppWindow;

/// The import path prompt (`ExportImportModal` in import mode).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageImport {
    pub path: String,
    pub message: String,
    pub error: bool,
}

impl App {
    /// Layers-panel import button: opens the path prompt.
    pub fn image_import_open(&mut self, ui: &AppWindow) {
        let path = self.store.export_path_for("import").with_extension("png");
        self.image.import = Some(ImageImport {
            path: path.display().to_string(),
            message: String::new(),
            error: false,
        });
        self.image.menu.clear();
        self.sync_image(ui);
    }

    pub fn image_import_path(&mut self, _ui: &AppWindow, path: &str) {
        if let Some(flow) = self.image.import.as_mut() {
            flow.path = path.to_string();
        }
    }

    pub fn image_import_close(&mut self, ui: &AppWindow) {
        self.image.import = None;
        self.sync_image(ui);
    }

    /// Runs the import flow; failures stay in the modal as an error line.
    pub fn image_import_run(&mut self, ui: &AppWindow) {
        let Some(flow) = self.image.import.clone() else {
            return;
        };
        let trimmed = flow.path.trim();
        if trimmed.is_empty() {
            self.set_image_import_message(ui, "Enter a file path first".to_string());
            return;
        }
        match self.image_import_core(Path::new(trimmed)) {
            Ok(message) => {
                self.image.import = None;
                self.set_notice(ui, message, false);
                self.sync_image(ui);
            }
            Err(message) => self.set_image_import_message(ui, message),
        }
    }

    fn set_image_import_message(&mut self, ui: &AppWindow, message: String) {
        if let Some(flow) = self.image.import.as_mut() {
            flow.message = message;
            flow.error = true;
        }
        self.sync_image(ui);
    }

    /// Decodes `path` and adds it as a board-fitted Image layer named after the
    /// file stem (`addImageLayer`: `createPsImageLayer` + `addPsLayerAbove`).
    pub fn image_import_core(&mut self, path: &Path) -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("Import failed: {error}"))?;
        let surface = decode_rgba(&bytes).map_err(|error| format!("Import failed: {error}"))?;
        let natural = (
            crate::vm::i2f(crate::vm::u2i(surface.width())),
            crate::vm::i2f(crate::vm::u2i(surface.height())),
        );
        let title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Imported")
            .to_string();
        let board = self.image.board();
        let mut gen = self.image_gen("ps-image");
        let mut layer = open_canvas_ps::create_ps_image_layer(&mut gen, board, &title, natural);
        layer.content = Some(format!("file:{}", path.display()));
        let id = layer.id.clone();
        let key = format!("image:{id}");
        layer.storage_key = Some(key.clone());
        self.image.bitmaps.insert(key, surface);
        let active = self.image.active_layer_id();
        self.image.project.board_layers = open_canvas_ps::add_ps_layer_above(
            &self.image.project.board_layers,
            layer,
            Some(&active),
        );
        self.image.active_layer = id;
        self.finish_image("Import Image");
        Ok(format!("Imported {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{encode_png, CanvasPsLayerKind, IdGen, Surface};

    fn output_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        project.width = 64.0;
        project.height = 32.0;
        project.board_ratio = "2:1".to_string();
        project.board_resolution = open_canvas_ps::doc::BoardResolution::K1;
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (64.0, 32.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(64, 32);
        surface.fill([40, 90, 160, 255]);
        bitmaps.insert("image:t-1".to_string(), surface);
        crate::test_support::app_with_image(project, bitmaps)
    }

    fn write_png(
        dir: &Path,
        name: &str,
        width: usize,
        height: usize,
    ) -> Result<std::path::PathBuf, String> {
        let mut surface = Surface::new(width, height);
        surface.fill([230, 60, 20, 255]);
        let png = encode_png(&surface).map_err(|error| error.to_string())?;
        let path = dir.join(name);
        std::fs::write(&path, png).map_err(|error| error.to_string())?;
        Ok(path)
    }

    #[test]
    fn import_adds_a_board_fitted_image_layer() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, dir) = output_board_app()?;
        let path = write_png(dir.path(), "art.png", 32, 16).map_err(std::io::Error::other)?;
        let message = app
            .image_import_core(&path)
            .map_err(std::io::Error::other)?;
        assert!(message.contains("art.png"));

        let layer = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("layer")?;
        assert_eq!(layer.kind, CanvasPsLayerKind::Image);
        assert_eq!(layer.name, "art");
        assert_eq!(
            (layer.width, layer.height),
            (64.0, 32.0),
            "fitted to the board"
        );
        let key = layer.storage_key.clone().ok_or("storage key")?;
        assert_eq!(
            app.image.bitmaps.get(&key).map(|bitmap| bitmap.get(4, 4)),
            Some([230, 60, 20, 255])
        );
        assert_eq!(app.image.active_layer, layer.id);
        assert_eq!(app.image.history.label(1), Some("Import Image"));

        let composite = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?;
        assert_eq!(
            composite.get(8, 8),
            [230, 60, 20, 255],
            "the import composites on top"
        );
        Ok(())
    }

    #[test]
    fn import_reports_missing_files_and_junk_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, dir) = output_board_app()?;
        let missing = dir.path().join("nope.png");
        assert!(app.image_import_core(&missing).is_err());
        let junk = dir.path().join("junk.png");
        std::fs::write(&junk, b"not a png")?;
        assert!(app.image_import_core(&junk).is_err());
        assert_eq!(
            app.image.project.board_layers.len(),
            1,
            "a failed import adds nothing"
        );
        Ok(())
    }
}
