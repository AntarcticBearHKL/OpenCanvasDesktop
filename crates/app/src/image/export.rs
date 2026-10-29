//! Image Studio export: composite the board through the `open-canvas-ps`
//! compositor and write it through the shared path-prompt modal (the web
//! downloaded the `composeSmartCanvas` blob; a desktop app writes to the typed
//! path).
//!
//! File ▸ Export as PNG/JPEG/BMP share this flow: the path's extension picks
//! the encoder, so the prompt stays one modal. PNG is the studio's native
//! format (the same encoder the canvas media path uses); JPEG and BMP go
//! through the `image` crate already in the workspace.
//! Owns the composite surface, the format choice and encoding/writing of it.

use std::path::{Path, PathBuf};

use open_canvas_ps::{encode_png, ImageProject, Surface};

use crate::app::App;
use crate::image::ImageExport;
use crate::AppWindow;

/// The raster formats File ▸ Export as … writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Bmp,
}

impl ExportFormat {
    /// The default file extension of the format.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
        }
    }

    /// The user-facing name ("JPEG").
    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Bmp => "BMP",
        }
    }

    /// The format a path asks for, if any (`None` for an unknown extension).
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }
}

/// The board composite at the project's export resolution: the same surface
/// the studio draws and every exporter encodes.
pub(crate) fn board_surface_with(
    project: &ImageProject,
    bitmaps: &std::collections::HashMap<String, Surface>,
) -> Surface {
    let board = open_canvas_ps::SmartCanvasBoard::from(project);
    let text = crate::image::text_raster::shared();
    open_canvas_ps::render_document_with_text(
        &board,
        bitmaps,
        text.map(|rasterizer| rasterizer as &dyn open_canvas_ps::TextRasterizer),
    )
}

/// Board PNG with explicit bitmaps (the tests' convenience path; the app writes
/// PNG through [`encode_surface`] like every other format).
#[cfg(test)]
pub(crate) fn board_png_bytes_with(
    project: &ImageProject,
    bitmaps: &std::collections::HashMap<String, Surface>,
) -> Result<Vec<u8>, String> {
    encode_png(&board_surface_with(project, bitmaps))
        .map_err(|error| format!("Export failed: {error}"))
}

/// Encodes `surface` as `format`.
///
/// The studio's composite is always 8-bit RGBA, so the fallback here is the
/// document not being representable in the chosen format (JPEG has no alpha):
/// the pixels are flattened onto white and the returned note says so, which the
/// status line reports instead of failing silently.
pub(crate) fn encode_surface(
    format: ExportFormat,
    surface: &Surface,
) -> Result<(Vec<u8>, Option<String>), String> {
    if format == ExportFormat::Png {
        return encode_png(surface)
            .map(|bytes| (bytes, None))
            .map_err(|error| format!("Export failed: {error}"));
    }
    let (width, height) = (surface.width(), surface.height());
    if width == 0 || height == 0 {
        return Err("Export failed: the document has no pixels".to_string());
    }
    let raw = surface.data();
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Export failed: the document is too large".to_string())?;
    if raw.len() != expected {
        // Defensive: a 16-bit or CMYK surface cannot be handed to these
        // encoders, so re-pack it as 8-bit RGBA instead of failing.
        return Err(
            "Export failed: the document is not 8-bit RGBA and cannot be flattened".to_string(),
        );
    }
    let mut rgba = image::RgbaImage::from_raw(
        u32::try_from(width).map_err(|_| "Export failed: too wide".to_string())?,
        u32::try_from(height).map_err(|_| "Export failed: too tall".to_string())?,
        raw.to_vec(),
    )
    .ok_or_else(|| "Export failed: the document has no pixels".to_string())?;
    let mut note = None;
    if format == ExportFormat::Jpeg {
        let transparent = rgba.pixels().any(|pixel| pixel.0[3] != 255);
        if transparent {
            for pixel in rgba.pixels_mut() {
                let channels = &mut pixel.0;
                let alpha = u32::from(channels[3]);
                for channel in &mut channels[..3] {
                    *channel =
                        u8::try_from((u32::from(*channel) * alpha + 255 * (255 - alpha)) / 255)
                            .unwrap_or(255);
                }
                channels[3] = 255;
            }
            note = Some(
                "flattened the transparency onto white \u{2014} JPEG has no alpha".to_string(),
            );
        }
    }
    let mut bytes = Vec::new();
    if format == ExportFormat::Jpeg {
        // Quality 92: visually lossless enough for a document export and the
        // same order as Photoshop's "high" JPEG preset.
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92);
        encoder
            .encode_image(&rgba)
            .map_err(|error| format!("Export failed: {error}"))?;
    } else {
        rgba.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Bmp,
        )
        .map_err(|error| format!("Export failed: {error}"))?;
    }
    Ok((bytes, note))
}

impl App {
    /// Opens the path-prompt modal pre-filled next to the app data directory.
    pub fn image_export_open(&mut self, ui: &AppWindow) {
        self.image_export_open_as(ui, ExportFormat::Png);
    }

    /// Opens the path-prompt modal for one File ▸ Export as … entry.
    pub fn image_export_open_as(&mut self, ui: &AppWindow, format: ExportFormat) {
        let Some(meta) = self.image.meta.clone() else {
            return;
        };
        let path = self
            .store
            .export_path_for(&meta.title)
            .with_extension(format.extension());
        self.image.export = Some(ImageExport {
            path: path.display().to_string(),
            message: String::new(),
            error: false,
        });
        self.image.menu.clear();
        self.sync_image(ui);
    }

    pub fn image_export_path(&mut self, _ui: &AppWindow, path: &str) {
        if let Some(flow) = self.image.export.as_mut() {
            flow.path = path.to_string();
        }
    }

    pub fn image_export_close(&mut self, ui: &AppWindow) {
        self.image.export = None;
        self.sync_image(ui);
    }

    /// Runs the export flow; failures stay in the modal as an error line.
    pub fn image_export_run(&mut self, ui: &AppWindow) {
        let Some(flow) = self.image.export.clone() else {
            return;
        };
        let trimmed = flow.path.trim();
        if trimmed.is_empty() {
            self.set_image_export_message(ui, "Enter a file path first".to_string());
            return;
        }
        let path = PathBuf::from(trimmed);
        let Some(format) = ExportFormat::from_path(&path) else {
            self.set_image_export_message(
                ui,
                "Export failed: use a .png, .jpg/.jpeg or .bmp path".to_string(),
            );
            return;
        };
        match self.image_run_raster(&path, format) {
            Ok(message) => {
                self.image.export = None;
                self.set_notice(ui, message, false);
                self.sync_image(ui);
            }
            Err(message) => self.set_image_export_message(ui, message),
        }
    }

    fn set_image_export_message(&mut self, ui: &AppWindow, message: String) {
        if let Some(flow) = self.image.export.as_mut() {
            flow.message = message;
            flow.error = true;
        }
        self.sync_image(ui);
    }

    /// Export the board composite at the project's export resolution, encoding
    /// it as `format` (the File menu's PNG/JPEG/BMP entries share the flow).
    fn image_run_raster(&self, path: &Path, format: ExportFormat) -> Result<String, String> {
        let surface = board_surface_with(&self.image.project, &self.image.bitmaps);
        let (bytes, note) = encode_surface(format, &surface)?;
        write_bytes(path, &bytes)?;
        let note = note.map_or_else(String::new, |note| format!(" ({note})"));
        Ok(format!(
            "Exported {} to {}{note}",
            format.label(),
            path.display()
        ))
    }
}

pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err("Export failed: the path has no parent directory".to_string());
    };
    std::fs::create_dir_all(parent).map_err(|error| format!("Export failed: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("Export failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_ps::{decode_rgba, CanvasPsLayerKind};

    fn project() -> ImageProject {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        project.width = 16.0;
        project.height = 16.0;
        project.board_ratio = "1:1".to_string();
        project.board_resolution = open_canvas_ps::doc::BoardResolution::K1;
        project.board_layers = vec![];
        project
    }

    #[test]
    fn export_png_carries_the_export_size_and_composited_pixels(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut project = project();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            CanvasPsLayerKind::Pixel,
            open_canvas_ps::Bounds::new(0.0, 0.0, 16.0, 16.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = std::collections::HashMap::new();
        let mut surface = Surface::new(16, 16);
        for y in 0..8 {
            for x in 0..8 {
                surface.set(x, y, [10, 200, 30, 255]);
            }
        }
        bitmaps.insert("image:l1".to_string(), surface);

        let png = board_png_bytes_with(&project, &bitmaps)?;
        assert_eq!(&png[..4], b"\x89PNG");
        let back = decode_rgba(&png).map_err(std::io::Error::other)?;
        // 1:1 at 1k exports 1024x1024 (readMediaDimensions).
        assert_eq!((back.width(), back.height()), (1024, 1024));
        let mid = back.get(256, 256);
        assert_eq!(mid, [10, 200, 30, 255]);
        assert_eq!(back.get(900, 900), [0, 0, 0, 0]);
        Ok(())
    }

    #[test]
    fn export_png_renders_the_background_and_layer_opacity(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut project = project();
        project.board_background = "#ffffff".to_string();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            CanvasPsLayerKind::Pixel,
            open_canvas_ps::Bounds::new(0.0, 0.0, 16.0, 16.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        layer.opacity = 0.5;
        project.board_layers = vec![layer];
        let mut bitmaps = std::collections::HashMap::new();
        let mut surface = Surface::new(16, 16);
        surface.fill([0, 0, 0, 255]);
        bitmaps.insert("image:l1".to_string(), surface);

        let png = board_png_bytes_with(&project, &bitmaps)?;
        let back = decode_rgba(&png).map_err(std::io::Error::other)?;
        let pixel = back.get(512, 512);
        // 50% black over white => mid gray.
        assert!(pixel[0] >= 126 && pixel[0] <= 129, "{pixel:?}");
        assert_eq!(pixel[3], 255);
        Ok(())
    }

    #[test]
    fn empty_project_still_exports_a_transparent_board() -> Result<(), Box<dyn std::error::Error>> {
        let png = board_png_bytes_with(&project(), &std::collections::HashMap::new())?;
        let back = decode_rgba(&png).map_err(std::io::Error::other)?;
        assert_eq!((back.width(), back.height()), (1024, 1024));
        assert_eq!(back.get(0, 0), [0, 0, 0, 0]);
        Ok(())
    }

    #[test]
    fn export_format_follows_the_file_extension() {
        let of = |path: &str| ExportFormat::from_path(Path::new(path));
        assert_eq!(of("Board.png"), Some(ExportFormat::Png));
        assert_eq!(of("Board.PNG"), Some(ExportFormat::Png));
        assert_eq!(of("Board.jpg"), Some(ExportFormat::Jpeg));
        assert_eq!(of("Board.jpeg"), Some(ExportFormat::Jpeg));
        assert_eq!(of("Board.JPEG"), Some(ExportFormat::Jpeg));
        assert_eq!(of("Board.bmp"), Some(ExportFormat::Bmp));
        assert_eq!(of("Board.psd"), None, "PSD has its own layered flow");
        assert_eq!(of("Board"), None);
        assert_eq!(ExportFormat::Jpeg.extension(), "jpg");
        assert_eq!(ExportFormat::Jpeg.label(), "JPEG");
    }

    #[test]
    fn jpeg_export_flattens_transparency_onto_white() -> Result<(), Box<dyn std::error::Error>> {
        let mut surface = Surface::new(8, 8);
        surface.fill([0, 0, 0, 0]);
        surface.set(2, 2, [200, 40, 10, 255]);
        let (bytes, note) = encode_surface(ExportFormat::Jpeg, &surface)?;
        assert_eq!(&bytes[..2], b"\xFF\xD8", "JPEG SOI marker");
        assert!(
            note.as_ref().is_some_and(|note| note.contains("white")),
            "the alpha fallback is reported: {note:?}"
        );
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)?;
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.dimensions(), (8, 8));
        let transparent = rgba.get_pixel(0, 0);
        assert!(
            transparent[0] > 245 && transparent[1] > 245 && transparent[2] > 245,
            "the transparent board area became white: {transparent:?}"
        );
        let painted = rgba.get_pixel(2, 2);
        assert!(painted[0] > 150 && painted[1] < 90, "{painted:?}");
        Ok(())
    }

    #[test]
    fn jpeg_export_does_not_report_a_fallback_for_an_opaque_board(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut surface = Surface::new(4, 4);
        surface.fill([12, 120, 240, 255]);
        let (bytes, note) = encode_surface(ExportFormat::Jpeg, &surface)?;
        assert!(note.is_none(), "an opaque document needs no fallback");
        assert!(bytes.len() > 4);
        Ok(())
    }

    #[test]
    fn bmp_export_keeps_the_rgba_pixels() -> Result<(), Box<dyn std::error::Error>> {
        let mut surface = Surface::new(4, 4);
        surface.fill([10, 200, 30, 255]);
        surface.set(1, 1, [250, 4, 60, 128]);
        let (bytes, note) = encode_surface(ExportFormat::Bmp, &surface)?;
        assert_eq!(&bytes[..2], b"BM", "BMP file header");
        assert!(note.is_none(), "BMP carries alpha as-is");
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp)?;
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.dimensions(), (4, 4));
        assert_eq!(rgba.get_pixel(0, 0), &image::Rgba([10, 200, 30, 255]));
        Ok(())
    }

    #[test]
    fn the_raster_export_writes_each_extension_with_its_encoder(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let (app, _store) =
            crate::test_support::app_with_image(project(), std::collections::HashMap::new())?;
        // Magic numbers: PNG's 0x89 + "PNG", JPEG's SOI, BMP's "BM".
        let cases: [(&str, ExportFormat, &[u8]); 3] = [
            ("Board.png", ExportFormat::Png, &[137, 80, 78, 71]),
            ("Board.jpg", ExportFormat::Jpeg, &[255, 216]),
            ("Board.bmp", ExportFormat::Bmp, &[66, 77]),
        ];
        for (name, format, magic) in cases {
            let path = dir.path().join(name);
            let message = app.image_run_raster(&path, format)?;
            assert!(
                message.starts_with(&format!("Exported {} to", format.label())),
                "the status line names the format: {message}"
            );
            let bytes = std::fs::read(&path)?;
            assert_eq!(&bytes[..magic.len()], magic, "{name} has its magic bytes");
            if format == ExportFormat::Jpeg {
                assert!(
                    message.contains("flattened the transparency onto white"),
                    "the transparent board's JPEG fallback is reported: {message}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn png_export_needs_no_fallback_note() -> Result<(), Box<dyn std::error::Error>> {
        let mut surface = Surface::new(4, 4);
        surface.set(0, 0, [1, 2, 3, 4]);
        let (bytes, note) = encode_surface(ExportFormat::Png, &surface)?;
        assert_eq!(&bytes[..4], b"\x89PNG");
        assert!(note.is_none());
        Ok(())
    }
}
