//! `inspect`: describe a PSD's raw layers, or the project summary of one of our
//! raster inputs.

use std::path::Path;

use open_canvas_ps::doc::BoardResolution;
use open_canvas_ps::{decode_rgba, CanvasPsLayerKind};
use open_canvas_psd::{BlendMode, ColorMode, LayerNode, LayerRecord, PsdFile};

use crate::args::InspectArgs;
use crate::project;
use crate::CliError;

/// Everything `inspect` prints, independent of the output format.
struct Report {
    path: String,
    format: &'static str,
    width: usize,
    height: usize,
    color_mode: String,
    depth: Option<u16>,
    board: Option<BoardInfo>,
    layers: Vec<LayerRow>,
}

struct BoardInfo {
    ratio: String,
    resolution: &'static str,
    width: f64,
    height: f64,
}

struct LayerRow {
    name: String,
    kind: String,
    depth: usize,
    visible: bool,
    /// Normalized 0..1 opacity.
    opacity: f64,
    blend: String,
}

/// Runs `inspect`; PSD bytes are recognised by their magic so a misnamed file
/// still reports its layers.
pub fn run(args: &InspectArgs) -> Result<(), CliError> {
    let text = if args.json {
        json_report(&args.file).map_err(CliError::failure)?
    } else {
        text_report(&args.file).map_err(CliError::failure)?
    };
    println!("{text}");
    Ok(())
}

/// The human-readable report for `path` (the testable core of `inspect`).
pub fn text_report(path: &Path) -> Result<String, String> {
    Ok(build_report(path)?.render_text())
}

/// The JSON report for `path` (the testable core of `inspect --json`).
pub fn json_report(path: &Path) -> Result<String, String> {
    let value = build_report(path)?.to_json();
    serde_json::to_string_pretty(&value).map_err(|error| format!("cannot render JSON: {error}"))
}

fn build_report(path: &Path) -> Result<Report, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.starts_with(b"8BPS") {
        psd_report(path, &bytes)
    } else {
        raster_report(path, &bytes)
    }
}

fn psd_report(path: &Path, bytes: &[u8]) -> Result<Report, String> {
    let file = PsdFile::from_bytes(bytes)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let header = &file.header;
    let mut layers = Vec::new();
    let tree = file.layer_tree();
    let mut stack: Vec<(&LayerNode, usize)> = tree.iter().rev().map(|node| (node, 0)).collect();
    while let Some((node, depth)) = stack.pop() {
        match node {
            LayerNode::Layer { index } => {
                if let Some(layer) = file.layer(*index) {
                    layers.push(row(layer.record, &layer.name(), "layer", depth));
                }
            }
            LayerNode::Group {
                index, children, ..
            } => {
                if let Some(layer) = file.layer(*index) {
                    layers.push(row(layer.record, &layer.name(), "group", depth));
                }
                for child in children.iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
    }
    Ok(Report {
        path: path.display().to_string(),
        format: if header.version.is_psb() {
            "psb"
        } else {
            "psd"
        },
        width: usize::try_from(header.width).unwrap_or(usize::MAX),
        height: usize::try_from(header.height).unwrap_or(usize::MAX),
        color_mode: color_mode_name(header.color_mode).to_string(),
        depth: Some(header.depth),
        board: None,
        layers,
    })
}

fn raster_report(path: &Path, bytes: &[u8]) -> Result<Report, String> {
    let surface =
        decode_rgba(bytes).map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
    let width = surface.width();
    let height = surface.height();
    let loaded = project::raster_loaded(path, surface);
    let layers = loaded
        .project
        .board_layers
        .iter()
        .map(|layer| LayerRow {
            name: layer.name.clone(),
            kind: layer_kind_name(layer.kind).to_string(),
            depth: 0,
            visible: !layer.hidden,
            opacity: layer.opacity.clamp(0.0, 1.0),
            blend: layer.blend_mode.clone(),
        })
        .collect();
    Ok(Report {
        path: path.display().to_string(),
        format: loaded.format,
        width,
        height,
        color_mode: loaded.color_mode.to_string(),
        depth: loaded.depth,
        board: Some(BoardInfo {
            ratio: loaded.project.board_ratio.clone(),
            resolution: resolution_id(loaded.project.board_resolution),
            width: loaded.project.width,
            height: loaded.project.height,
        }),
        layers,
    })
}

fn row(record: &LayerRecord, name: &str, kind: &'static str, depth: usize) -> LayerRow {
    LayerRow {
        name: if name.trim().is_empty() {
            "Layer".to_string()
        } else {
            name.to_string()
        },
        kind: kind.to_string(),
        depth,
        visible: record.is_visible(),
        opacity: f64::from(record.opacity) / 255.0,
        blend: blend_name(record.blend_mode),
    }
}

impl Report {
    fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("file:       {}\n", self.path));
        out.push_str(&format!("format:     {}\n", self.format));
        out.push_str(&format!("size:       {} x {}\n", self.width, self.height));
        out.push_str(&format!("color mode: {}\n", self.color_mode));
        if let Some(depth) = self.depth {
            out.push_str(&format!("depth:      {depth} bit\n"));
        }
        if let Some(board) = &self.board {
            out.push_str(&format!(
                "board:      {} @ {} ({} x {})\n",
                board.ratio, board.resolution, board.width, board.height
            ));
        }
        out.push_str(&format!("layers:     {}\n", self.layers.len()));
        for (index, layer) in self.layers.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {}{} [{}] {} opacity={:.2} blend={}\n",
                index + 1,
                "  ".repeat(layer.depth),
                layer.name,
                layer.kind,
                if layer.visible { "visible" } else { "hidden" },
                layer.opacity,
                layer.blend
            ));
        }
        out
    }

    fn to_json(&self) -> serde_json::Value {
        let layers: Vec<serde_json::Value> = self
            .layers
            .iter()
            .map(|layer| {
                serde_json::json!({
                    "name": layer.name,
                    "kind": layer.kind,
                    "depth": layer.depth,
                    "visible": layer.visible,
                    "opacity": (layer.opacity * 1000.0).round() / 1000.0,
                    "blend": layer.blend,
                })
            })
            .collect();
        let mut value = serde_json::json!({
            "file": self.path,
            "format": self.format,
            "width": self.width,
            "height": self.height,
            "color_mode": self.color_mode,
            "layers": layers,
        });
        if let Some(depth) = self.depth {
            value["depth"] = serde_json::json!(depth);
        }
        if let Some(board) = &self.board {
            value["board"] = serde_json::json!({
                "ratio": board.ratio,
                "resolution": board.resolution,
                "width": board.width,
                "height": board.height,
            });
        }
        value
    }
}

fn color_mode_name(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::Bitmap => "bitmap",
        ColorMode::Grayscale => "grayscale",
        ColorMode::Indexed => "indexed",
        ColorMode::Rgb => "rgb",
        ColorMode::Cmyk => "cmyk",
        ColorMode::Multichannel => "multichannel",
        ColorMode::Duotone => "duotone",
        ColorMode::Lab => "lab",
        ColorMode::Unknown(_) => "unknown",
    }
}

fn blend_name(mode: BlendMode) -> String {
    match mode {
        BlendMode::Unknown(key) => format!("unknown({})", String::from_utf8_lossy(&key)),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn resolution_id(resolution: BoardResolution) -> &'static str {
    match resolution {
        BoardResolution::K1 => "1k",
        BoardResolution::K2 => "2k",
        BoardResolution::K4 => "4k",
    }
}

fn layer_kind_name(kind: CanvasPsLayerKind) -> &'static str {
    match kind {
        CanvasPsLayerKind::Image => "image",
        CanvasPsLayerKind::Text => "text",
        CanvasPsLayerKind::Group => "group",
        CanvasPsLayerKind::Pixel => "pixel",
        CanvasPsLayerKind::Shape => "shape",
        CanvasPsLayerKind::Adjustment => "adjustment",
    }
}
