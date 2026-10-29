//! Loading files into an `ImageProject` and writing projects back out. This is
//! the only place in the CLI that touches the filesystem.

use std::collections::HashMap;
use std::path::Path;

use open_canvas_ps::board::{create_ps_image_layer, read_media_dimensions};
use open_canvas_ps::psd_io::{import_psd, studio_size_for};
use open_canvas_ps::{
    decode_rgba, encode_png, export_project, render_document_with_text, IdGen, ImageProject,
    SmartCanvasBoard, Surface,
};

/// A loaded document plus everything needed to describe it.
pub struct Loaded {
    pub project: ImageProject,
    pub bitmaps: HashMap<String, Surface>,
    /// `png`, `jpeg`, `bmp`, `image`, `psd` or `psb`.
    pub format: &'static str,
    pub color_mode: &'static str,
    pub depth: Option<u16>,
}

/// Reads `path` into a project. PSD bytes are identified by their magic, so a
/// PSD without a `.psd` extension still opens.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.starts_with(b"8BPS") {
        let (mut project, bitmaps) = import_psd(&bytes)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        let title = title_of(path);
        if project.title.is_empty() {
            project.title.clone_from(&title);
        }
        if project.id.is_empty() {
            project.id.clone_from(&title);
        }
        Ok(Loaded {
            project,
            bitmaps,
            format: if bytes.get(5).copied() == Some(2) {
                "psb"
            } else {
                "psd"
            },
            color_mode: "rgb",
            depth: Some(8),
        })
    } else {
        let surface = decode_rgba(&bytes)
            .map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
        Ok(raster_loaded(path, surface))
    }
}

/// Wraps a decoded raster as a one-layer project on the studio board preset
/// that matches its aspect ratio (exactly, when the size is already a preset).
pub fn raster_loaded(path: &Path, surface: Surface) -> Loaded {
    let (project, bitmaps) = raster_project(path, surface);
    Loaded {
        project,
        bitmaps,
        format: magic_format_from_path(path),
        color_mode: "rgba8",
        depth: None,
    }
}

fn raster_project(path: &Path, surface: Surface) -> (ImageProject, HashMap<String, Surface>) {
    let width = surface.width();
    let height = surface.height();
    let (resolution, ratio) = studio_size_for(width, height);
    let (board_width, board_height) = read_media_dimensions(resolution, ratio);
    let title = title_of(path);
    let mut gen = IdGen::new("cli");
    let mut layer = create_ps_image_layer(
        &mut gen,
        (board_width, board_height),
        &title,
        (width as f64, height as f64),
    );
    let key = format!("image:{}", layer.id);
    layer.storage_key = Some(key.clone());
    let mut bitmaps = HashMap::new();
    bitmaps.insert(key, surface);
    let project = ImageProject {
        id: title.clone(),
        title,
        created_at: String::new(),
        updated_at: String::new(),
        group_id: String::new(),
        width: board_width,
        height: board_height,
        board_ratio: ratio.to_string(),
        board_resolution: resolution,
        board_background: "transparent".to_string(),
        board_background_opacity: 1.0,
        board_layers: vec![layer],
        board_paths: None,
        board_alpha_channels: None,
        board_channel_visibility: None,
    };
    (project, bitmaps)
}

/// Writes a project as PNG (board composite) or PSD (layers + composite),
/// chosen by the output extension.
pub fn write(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
    path: &Path,
) -> Result<(), String> {
    let bytes = match output_kind(path)? {
        OutputKind::Png => png_bytes(project, bitmaps)?,
        OutputKind::Psd => export_project(project, bitmaps).map_err(|error| error.to_string())?,
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
    }
    std::fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// The output formats the CLI can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Png,
    Psd,
}

/// Classifies an output path by extension; anything but `.png`/`.psd` is an
/// error (the caller turns this into a usage error).
pub fn output_kind(path: &Path) -> Result<OutputKind, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Ok(OutputKind::Png),
        "psd" => Ok(OutputKind::Psd),
        _ => Err("output must end in .png or .psd".to_string()),
    }
}

/// The board composite at the project's export size, as PNG bytes.
pub fn png_bytes(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
) -> Result<Vec<u8>, String> {
    let board = SmartCanvasBoard::from(project);
    let surface = render_document_with_text(&board, bitmaps, None);
    encode_png(&surface).map_err(|error| error.to_string())
}

fn title_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image")
        .to_string()
}

fn magic_format_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpeg",
        Some("bmp") => "bmp",
        Some("png") => "png",
        _ => "image",
    }
}
