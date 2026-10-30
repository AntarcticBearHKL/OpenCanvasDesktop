//! Canvas local media pipeline: importing local files as canvas nodes, the
//! image node quick tools (crop, mask, split, resolution, analysis, copy,
//! download) and the image-stack gallery actions.
//!
//! Provenance: `use-canvas-insertion.ts` (file → node insertion, `fitNodeSize`,
//! the 40px stagger), `canvas-node-crop-dialog.tsx` (normalized crop rect, ratio
//! presets, `resizeCrop`), `canvas-node-mask-edit-dialog.tsx` (shape + feather +
//! invert mask), `canvas-node-split-dialog.tsx` (`buildSplitCuts`,
//! `findLineSpot`), `canvas-node-resolution-dialog.tsx` /
//! `canvas-image-data.ts` (`resolveUpscaleSize`, algorithm choice),
//! `canvas-image-analysis-dialog.tsx` (palette, perceptual hash, smart crop) and
//! `image-stack-preview.tsx` (set cover / take out / remove / retry / download /
//! copy / split all).
//!
//! Everything here is local: no provider call, no AI service. Pixel work goes
//! through the `open-canvas-ps` domain functions on the same `Surface` the
//! studios use, and the result is written back as the same
//! `data:image/png;base64,…` URL the studios' "save to canvas" stores, so the
//! existing renderer picks the node up for free.
//!
//! Slint has no OS file drop (the winit backend never forwards
//! `WindowEvent::DroppedFile` to `DropArea`), so file ingestion is the app's
//! path-prompt affordance: the canvas media import modal, the stack card's
//! empty state, the hover bar's upload action and Ctrl+V with a system-clipboard
//! image.
//! Owns the canvas media dialogs (`CanvasMedia`) and the image-node quick tools.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use open_canvas_core::geometry::{fit_node_size, FIT_NODE_MAX_SIZE};
use open_canvas_core::node::{
    CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType, Position,
};
use open_canvas_ps::analysis::{
    extract_palette, metadata_rows, parse_aspect, perceptual_hash, read_image_metadata,
    suggest_smart_crop, CropRect, ImageMetadata, PaletteColor, DEFAULT_PALETTE_COUNT,
};
use open_canvas_ps::{
    crop_surface, encode_png, ps_bake_mask, resample_surface, resolve_upscale_size,
    selection_create, selection_invert, selection_rect, Bounds, SelectionMode, Surface, TileRect,
    UpscaleAlgorithm,
};

use slint::ComponentHandle;

use crate::app::App;
use crate::coords::WorldPos;
use crate::AppWindow;

/// Gap between split pieces (`project.tsx` `splitImageNode`).
pub const SPLIT_GAP: f64 = 16.0;
/// World-space gap between the split source and its children
/// (`project.tsx:1571` `startX = node.position.x + node.width + 96`).
pub const SPLIT_START_OFFSET: f64 = 96.0;
/// Rows/columns ceiling of the split dialog (`maxGridSize`, split-dialog:14).
pub const SPLIT_MAX_GRID: usize = 12;
/// Default crop rect (`defaultCrop`, crop-dialog:22).
pub const DEFAULT_CROP: NormRect = NormRect {
    x: 0.12,
    y: 0.12,
    width: 0.76,
    height: 0.76,
};
/// Smallest crop dimension (`minSize`, crop-dialog:21).
pub const MIN_CROP: f64 = 0.06;
/// Normalized tolerance that snaps a crop press onto a frame edge/corner.
pub const CROP_HANDLE_TOLERANCE: f64 = 0.035;
/// Ratio chips of the crop dialog (`canvas-node-crop-dialog.tsx:130-136`).
pub const CROP_RATIOS: [&str; 7] = ["free", "fixed", "original", "1:1", "4:3", "16:9", "9:16"];
/// Ratio chips of the analysis dialog (`canvas-image-analysis-dialog.tsx:11`).
pub const ANALYSIS_RATIOS: [&str; 5] = ["1:1", "4:3", "3:4", "16:9", "9:16"];
/// Mask shapes of the mask-edit dialog.
pub const MASK_SHAPES: [&str; 2] = ["rect", "ellipse"];
/// Resample algorithms (`ImageUpscaleAlgorithm`, canvas-image-data.ts:10).
pub const UPSCALE_ALGORITHMS: [&str; 3] = ["nearest", "bilinear", "high"];
/// Long-edge presets offered by the resolution dialog.
pub const RESOLUTION_PRESETS: [usize; 4] = [1024, 2048, 3072, 4096];
/// A normalized (0..1) crop rectangle (`CanvasImageCropRect`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormRect {
    /// Clamps into `0..1` keeping at least [`MIN_CROP`] of each axis.
    pub fn clamp(self) -> Self {
        let width = self.width.clamp(MIN_CROP, 1.0);
        let height = self.height.clamp(MIN_CROP, 1.0);
        Self {
            x: self.x.clamp(0.0, 1.0 - width),
            y: self.y.clamp(0.0, 1.0 - height),
            width,
            height,
        }
    }
}

/// The crop dialog's live state.
#[derive(Debug, Clone, PartialEq)]
pub struct CropTool {
    pub rect: NormRect,
    /// `free` | `fixed` | `original` | a `W:H` chip.
    pub ratio: String,
    /// The locked ratio when the `fixed` chip is active.
    pub fixed: Option<f64>,
    pub natural: (usize, usize),
    /// Active drag: the mode (`move` or a handle id) and the last normalized
    /// pointer position.
    pub drag: Option<(String, f64, f64)>,
}

impl CropTool {
    pub fn new(natural: (usize, usize)) -> Self {
        Self {
            rect: DEFAULT_CROP,
            ratio: "free".to_string(),
            fixed: None,
            natural,
            drag: None,
        }
    }

    /// The crop size readout (`"{w} x {h}"`, the dialog's `cropSize` line).
    pub fn size_label(&self) -> String {
        let rect = crop_rect_pixels(self.rect, self.natural.0, self.natural.1);
        format!("{} x {}", rect.width, rect.height)
    }

    /// `formatRatio(cropSize.width, cropSize.height)`.
    pub fn ratio_label(&self) -> String {
        let rect = crop_rect_pixels(self.rect, self.natural.0, self.natural.1);
        format_ratio(rect.width, rect.height)
    }

    /// `resolveRatio(preset, image, fixedRatio)` (crop-dialog:221-227).
    pub fn resolved_ratio(&self) -> Option<f64> {
        crop_ratio(&self.ratio, self.natural, self.fixed)
    }

    /// The ratio chip pick; `fixed` captures the rect's current ratio. Unknown
    /// chips are ignored (`CROP_RATIOS` is the dialog's chip set).
    pub fn pick_ratio(&mut self, preset: &str) {
        if !CROP_RATIOS.contains(&preset) {
            return;
        }
        self.ratio = preset.to_string();
        self.fixed = if preset == "fixed" {
            let rect = crop_rect_pixels(self.rect, self.natural.0, self.natural.1);
            if rect.height == 0 {
                None
            } else {
                Some(
                    crate::vm::i2f(crate::vm::u2i(rect.width))
                        / crate::vm::i2f(crate::vm::u2i(rect.height)),
                )
            }
        } else {
            None
        };
        if let Some(ratio) = self.resolved_ratio() {
            self.rect = fit_crop_to_ratio(self.rect, ratio, self.natural);
        }
    }
}

/// The mask-edit dialog's state (local mask: shape + feather + invert).
#[derive(Debug, Clone, PartialEq)]
pub struct MaskTool {
    /// `rect` | `ellipse`.
    pub shape: String,
    pub feather: f64,
    pub invert: bool,
}

impl Default for MaskTool {
    fn default() -> Self {
        Self {
            shape: "rect".to_string(),
            feather: 0.0,
            invert: false,
        }
    }
}

/// The split dialog's state (`CanvasImageSplitParams`).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitTool {
    pub rows: usize,
    pub columns: usize,
    pub horizontal_lines: Vec<f64>,
    pub vertical_lines: Vec<f64>,
}

impl Default for SplitTool {
    fn default() -> Self {
        Self {
            rows: 2,
            columns: 2,
            horizontal_lines: vec![0.5],
            vertical_lines: vec![0.5],
        }
    }
}

impl SplitTool {
    /// Total piece count (the dialog's `total`).
    pub fn total(&self) -> usize {
        self.rows.max(1) * self.columns.max(1)
    }

    fn add_line(&mut self, horizontal: bool) {
        let lines = if horizontal {
            &mut self.horizontal_lines
        } else {
            &mut self.vertical_lines
        };
        lines.push(largest_gap_spot(lines));
        lines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if horizontal {
            self.rows = self.horizontal_lines.len() + 1;
        } else {
            self.columns = self.vertical_lines.len() + 1;
        }
    }

    fn remove_line(&mut self) {
        if self.horizontal_lines.len() >= self.vertical_lines.len()
            && !self.horizontal_lines.is_empty()
        {
            self.horizontal_lines.pop();
            self.rows = self.horizontal_lines.len() + 1;
        } else if !self.vertical_lines.is_empty() {
            self.vertical_lines.pop();
            self.columns = self.vertical_lines.len() + 1;
        }
    }

    /// `resetLines` (split-dialog:106): even interior lines for both axes.
    fn reset_lines(&mut self) {
        self.horizontal_lines = grid_lines(self.rows);
        self.vertical_lines = grid_lines(self.columns);
    }
}

/// The resolution dialog's state.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolutionTool {
    pub target: usize,
    /// `nearest` | `bilinear` | `high`.
    pub algorithm: String,
    pub natural: (usize, usize),
}

impl ResolutionTool {
    pub fn new(natural: (usize, usize)) -> Self {
        Self {
            target: RESOLUTION_PRESETS[1],
            algorithm: "high".to_string(),
            natural,
        }
    }

    /// `resolveUpscaleSize(natural, target)` for the readout.
    pub fn resolved(&self) -> (usize, usize) {
        resolve_upscale_size(self.natural.0, self.natural.1, self.target)
    }
}

/// The analysis dialog's state (local, AI-free).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisTool {
    pub palette: Vec<PaletteColor>,
    pub metadata: ImageMetadata,
    pub hash: String,
    pub ratio: String,
    pub suggestion: CropRect,
    pub natural: (usize, usize),
}

impl AnalysisTool {
    /// The suggested smart-crop rectangle as a normalized rect (the web's
    /// `area / image` division before `cropDataUrl`).
    pub fn suggested_rect(&self) -> NormRect {
        let width = crate::vm::i2f(crate::vm::u2i(self.natural.0)).max(1.0);
        let height = crate::vm::i2f(crate::vm::u2i(self.natural.1)).max(1.0);
        NormRect {
            x: crate::vm::i2f(self.suggestion.x) / width,
            y: crate::vm::i2f(self.suggestion.y) / height,
            width: crate::vm::i2f(self.suggestion.width) / width,
            height: crate::vm::i2f(self.suggestion.height) / height,
        }
        .clamp()
    }
}

/// Which quick tool the dialog shows.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageToolKind {
    Crop(CropTool),
    Mask(MaskTool),
    Split(SplitTool),
    Resolution(ResolutionTool),
    Analysis(AnalysisTool),
}

impl ImageToolKind {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Crop(_) => "crop",
            Self::Mask(_) => "mask",
            Self::Split(_) => "split",
            Self::Resolution(_) => "resolution",
            Self::Analysis(_) => "analysis",
        }
    }
}

/// One open quick-tool dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageTool {
    pub node_id: String,
    pub kind: ImageToolKind,
    pub message: String,
    pub error: bool,
}

/// The open image quick-tool dialog (Rust owns it).
#[derive(Debug, Default)]
pub struct CanvasMedia {
    pub tool: Option<ImageTool>,
}

/// `buildSplitCuts` (canvas-image-data.ts:63): the even grid when the dialog
/// carries no lines, else the sorted, de-duplicated in-bounds cuts.
pub fn split_cuts(lines: &[f64], size: usize, count: usize) -> Vec<usize> {
    let count = count.max(1);
    if lines.is_empty() {
        return (0..=count).map(|index| index * size / count).collect();
    }
    let mut cuts = vec![0_usize];
    let mut mapped: Vec<usize> = lines
        .iter()
        .map(|line| crate::vm::i2u(crate::vm::f2i(line * crate::vm::i2f(crate::vm::u2i(size)))))
        .filter(|line| *line > 0 && *line < size)
        .collect();
    mapped.sort_unstable();
    mapped.dedup();
    cuts.extend(mapped);
    cuts.push(size);
    cuts
}

/// `buildGridLines(count)` (split-dialog:261): the even interior lines.
pub fn grid_lines(count: usize) -> Vec<f64> {
    let count = count.max(1);
    (1..count)
        .map(|index| crate::vm::i2f(crate::vm::u2i(index)) / crate::vm::i2f(crate::vm::u2i(count)))
        .collect()
}

/// `findLineSpot(lines)` (split-dialog:265): the midpoint of the largest gap.
pub fn largest_gap_spot(lines: &[f64]) -> f64 {
    let mut cuts = vec![0.0];
    cuts.extend(lines.iter().copied());
    cuts.push(1.0);
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut spot = 0.5;
    let mut widest = 0.0;
    for pair in cuts.windows(2) {
        let gap = pair[1] - pair[0];
        if gap > widest {
            widest = gap;
            spot = pair[0] + gap / 2.0;
        }
    }
    spot
}

/// `clampGrid` (split-dialog:283): `1..=`[`SPLIT_MAX_GRID`], rounded.
pub fn clamp_grid(value: f64) -> usize {
    let rounded = if value.is_finite() {
        value.round()
    } else {
        1.0
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped into 1..=12 before the cast"
    )]
    let clamped = rounded.clamp(1.0, crate::vm::i2f(crate::vm::u2i(SPLIT_MAX_GRID))) as usize;
    clamped
}

/// The normalized crop rect in source pixels (`cropDataUrl`: floor the origin,
/// ceil the size, clamped into bounds).
pub fn crop_rect_pixels(rect: NormRect, width: usize, height: usize) -> TileRect {
    let rect = rect.clamp();
    let source_w = crate::vm::i2f(crate::vm::u2i(width)).max(1.0);
    let source_h = crate::vm::i2f(crate::vm::u2i(height)).max(1.0);
    let x = (rect.x * source_w).floor().clamp(0.0, source_w - 1.0);
    let y = (rect.y * source_h).floor().clamp(0.0, source_h - 1.0);
    let box_width = (rect.width * source_w).ceil().clamp(1.0, source_w - x);
    let box_height = (rect.height * source_h).ceil().clamp(1.0, source_h - y);
    TileRect {
        x: crate::vm::i2u(crate::vm::f2i(x)),
        y: crate::vm::i2u(crate::vm::f2i(y)),
        width: crate::vm::i2u(crate::vm::f2i(box_width)),
        height: crate::vm::i2u(crate::vm::f2i(box_height)),
    }
}

/// `resolveRatio(preset, image, fixedRatio)` (crop-dialog:221).
pub fn crop_ratio(preset: &str, image: (usize, usize), fixed: Option<f64>) -> Option<f64> {
    if preset == "free" || image.0 == 0 || image.1 == 0 {
        return None;
    }
    if preset == "fixed" {
        return fixed;
    }
    if preset == "original" {
        return Some(
            crate::vm::i2f(crate::vm::u2i(image.0)) / crate::vm::i2f(crate::vm::u2i(image.1)),
        );
    }
    let (width, height) = preset.split_once(':')?;
    let (Ok(width), Ok(height)) = (width.trim().parse::<f64>(), height.trim().parse::<f64>())
    else {
        return None;
    };
    if width > 0.0 && height > 0.0 {
        Some(width / height)
    } else {
        None
    }
}

/// `fitCropToRatio` (crop-dialog:229): shrink/center the rect onto a ratio.
pub fn fit_crop_to_ratio(rect: NormRect, ratio: f64, image: (usize, usize)) -> NormRect {
    if !ratio.is_finite() || ratio <= 0.0 || image.0 == 0 || image.1 == 0 {
        return rect.clamp();
    }
    let normalized =
        ratio * (crate::vm::i2f(crate::vm::u2i(image.1)) / crate::vm::i2f(crate::vm::u2i(image.0)));
    let mut width = rect.width;
    let mut height = width / normalized;
    if height > rect.height {
        height = rect.height;
        width = height * normalized;
    }
    if width > 1.0 {
        width = 1.0;
        height = width / normalized;
    }
    if height > 1.0 {
        height = 1.0;
        width = height * normalized;
    }
    NormRect {
        x: rect.x + (rect.width - width) / 2.0,
        y: rect.y + (rect.height - height) / 2.0,
        width,
        height,
    }
    .clamp()
}

/// `moveCrop` (crop-dialog:175): translate, clamped in bounds.
pub fn move_crop(rect: NormRect, dx: f64, dy: f64) -> NormRect {
    NormRect {
        x: (rect.x + dx).clamp(0.0, 1.0 - rect.width),
        y: (rect.y + dy).clamp(0.0, 1.0 - rect.height),
        ..rect
    }
}

/// `resizeCrop` (crop-dialog:179-219): the handle drag, ratio-aware.
///
/// `stage` is the source image size: the web's `box.height / box.width` term is
/// the displayed image's own aspect, which is the source aspect.
pub fn resize_crop(
    rect: NormRect,
    dx: f64,
    dy: f64,
    handle: &str,
    ratio: Option<f64>,
    stage: (f64, f64),
) -> NormRect {
    let mut next = rect;
    if handle.contains('e') {
        next.width = rect.width + dx;
    }
    if handle.contains('s') {
        next.height = rect.height + dy;
    }
    if handle.contains('w') {
        next.x = rect.x + dx;
        next.width = rect.width - dx;
    }
    if handle.contains('n') {
        next.y = rect.y + dy;
        next.height = rect.height - dy;
    }
    if let Some(ratio) = ratio.filter(|value| value.is_finite() && *value > 0.0) {
        let aspect = stage.0.max(1.0) / stage.1.max(1.0);
        let normalized = ratio / aspect;
        let horizontal_only = (handle.contains('e') || handle.contains('w'))
            && !handle.contains('n')
            && !handle.contains('s');
        let use_width =
            horizontal_only || (handle.len() > 1 && (dx * stage.0).abs() >= (dy * stage.1).abs());
        if use_width {
            next.height = next.width / normalized;
        } else {
            next.width = next.height * normalized;
        }
        if handle.contains('w') {
            next.x = rect.x + rect.width - next.width;
        }
        if handle.contains('n') {
            next.y = rect.y + rect.height - next.height;
        }
        let scale_down = (1.0 / next.width.max(0.001))
            .min(1.0 / next.height.max(0.001))
            .min(1.0);
        next.width *= scale_down;
        next.height *= scale_down;
        if next.width < MIN_CROP || next.height < MIN_CROP {
            let minimum = (MIN_CROP / next.width.max(0.001)).max(MIN_CROP / next.height.max(0.001));
            next.width *= minimum;
            next.height *= minimum;
        }
        next.width = next.width.min(next.height * normalized);
        next.height = next.width / normalized;
    } else {
        next.width = next.width.clamp(MIN_CROP, 1.0);
        next.height = next.height.clamp(MIN_CROP, 1.0);
    }
    next.x = next.x.clamp(0.0, 1.0 - next.width);
    next.y = next.y.clamp(0.0, 1.0 - next.height);
    next
}

/// The resize handle under a normalized pointer position: `""` outside the
/// frame (no drag), `move` inside it, else the nearest edge/corner id.
pub fn crop_handle_at(rect: NormRect, x: f64, y: f64, tolerance: f64) -> String {
    let rect = rect.clamp();
    if x < rect.x - tolerance
        || x > rect.x + rect.width + tolerance
        || y < rect.y - tolerance
        || y > rect.y + rect.height + tolerance
    {
        return String::new();
    }
    let horizontal = if (x - rect.x).abs() <= tolerance {
        "w"
    } else if (x - (rect.x + rect.width)).abs() <= tolerance {
        "e"
    } else {
        ""
    };
    let vertical = if (y - rect.y).abs() <= tolerance {
        "n"
    } else if (y - (rect.y + rect.height)).abs() <= tolerance {
        "s"
    } else {
        ""
    };
    if horizontal.is_empty() && vertical.is_empty() {
        "move".to_string()
    } else {
        format!("{vertical}{horizontal}")
    }
}

/// `formatRatio` + `gcd` (crop-dialog:267-273).
pub fn format_ratio(width: usize, height: usize) -> String {
    let gcd = greatest_common_divisor(width, height);
    format!("{}:{}", width / gcd, height / gcd)
}

fn greatest_common_divisor(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}

/// The extension matching a stored mime type (download naming).
pub fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        _ => "png",
    }
}

/// The stored data URL and byte size of a surface (encoded once).
pub fn surface_png(surface: &Surface) -> Result<(String, u64), String> {
    let png = encode_png(surface).map_err(|error| format!("Encode failed: {error}"))?;
    let bytes = u64::try_from(png.len()).unwrap_or(u64::MAX);
    Ok((
        format!("data:image/png;base64,{}", BASE64.encode(&png)),
        bytes,
    ))
}

/// The `CanvasNodeImage` entry for a freshly imported surface.
pub fn node_image_entry(
    id: &str,
    surface: &Surface,
    storage_key: Option<String>,
) -> CanvasNodeImage {
    let (content, bytes) = surface_png(surface).unwrap_or_default();
    CanvasNodeImage {
        id: id.to_string(),
        status: CanvasNodeStatus::Success,
        error_details: None,
        content,
        storage_key,
        thumbnail: None,
        thumbnail_key: None,
        natural_width: crate::vm::i2f(crate::vm::u2i(surface.width())),
        natural_height: crate::vm::i2f(crate::vm::u2i(surface.height())),
        bytes,
        mime_type: "image/png".to_string(),
    }
}

/// The decoded bitmap text of a node: the primary `metadata.images` entry, else
/// the plain `metadata.content`/thumbnail.
pub fn node_source_text(node: &CanvasNodeData) -> Option<String> {
    let meta = node.metadata.as_ref()?;
    if let Some(image) = primary_image(node) {
        if !image.content.is_empty() {
            return Some(image.content.clone());
        }
    }
    meta.content
        .clone()
        .filter(|content| !content.is_empty())
        .or_else(|| meta.thumbnail.clone().filter(|thumb| !thumb.is_empty()))
}

/// The primary image entry of a node (`primaryImageId`, else the first).
pub fn primary_image(node: &CanvasNodeData) -> Option<&CanvasNodeImage> {
    let meta = node.metadata.as_ref()?;
    let images = meta.images.as_ref()?;
    let primary = meta.primary_image_id.as_deref();
    primary
        .and_then(|id| images.iter().find(|image| image.id == id))
        .or_else(|| images.first())
}

/// The bitmap's natural dimensions, preferring stored metadata and falling
/// back to the image header. An Image node uses this for both its initial
/// card size and every corner resize, so the card can never distort the
/// picture.
pub fn image_natural_size(node: &CanvasNodeData) -> Option<(f64, f64)> {
    if let Some(meta) = node.metadata.as_ref() {
        if let Some((width, height)) = meta.natural_width.zip(meta.natural_height) {
            if width > 0.0 && height > 0.0 {
                return Some((width, height));
            }
        }
    }
    if let Some(image) = primary_image(node) {
        if image.natural_width > 0.0 && image.natural_height > 0.0 {
            return Some((image.natural_width, image.natural_height));
        }
    }
    let text = node_source_text(node)?;
    let (width, height) = crate::media::decode_dimensions(&text)?;
    Some((f64::from(width), f64::from(height)))
}

impl App {
    /// Locks every Image node to its bitmap's aspect ratio and drops the
    /// legacy free-resize flag. Called when a project opens, when a generated
    /// image lands, and before an Image node resize gesture.
    pub(crate) fn normalize_image_geometry(&mut self) -> bool {
        let ids: Vec<String> = self
            .doc
            .nodes()
            .iter()
            .filter(|node| node.node_type == CanvasNodeType::Image)
            .map(|node| node.id.clone())
            .collect();
        let mut changed = false;
        for id in ids {
            changed |= self.normalize_image_node_geometry(&id);
        }
        changed
    }

    /// Normalizes one Image node. The width stays put (the user's chosen card
    /// scale), the height follows the bitmap ratio, and the card grows/shrinks
    /// around its vertical centre.
    pub(crate) fn normalize_image_node_geometry(&mut self, id: &str) -> bool {
        let Some(node) = self.doc.node(id).cloned() else {
            return false;
        };
        if node.node_type != CanvasNodeType::Image {
            return false;
        }
        let Some((natural_width, natural_height)) = image_natural_size(&node) else {
            // No decodable bitmap: still remove the legacy flag so a future
            // resize cannot unlock the node.
            let Some(target) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
                return false;
            };
            let meta = target
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            let changed = meta.free_resize.is_some();
            meta.free_resize = None;
            return changed;
        };
        let ratio = natural_width / natural_height.max(1.0);
        let width = node.width.max(1.0);
        let height = (width / ratio).max(1.0);
        let meta_needs = node.metadata.as_ref().is_none_or(|meta| {
            meta.free_resize.is_some()
                || meta.natural_width != Some(natural_width)
                || meta.natural_height != Some(natural_height)
        });
        if !meta_needs && (node.height - height).abs() < 0.01 {
            return false;
        }
        let center_y = node.position.y + node.height / 2.0;
        let Some(target) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
            return false;
        };
        target.height = height;
        target.position.y = center_y - height / 2.0;
        let meta = target
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.natural_width = Some(natural_width);
        meta.natural_height = Some(natural_height);
        meta.free_resize = None;
        true
    }
}

/// Every `metadata.images` entry of a node.
pub fn stack_images(node: &CanvasNodeData) -> &[CanvasNodeImage] {
    node.metadata
        .as_ref()
        .and_then(|meta| meta.images.as_ref())
        .map_or(&[], Vec::as_slice)
}

/// Decodes a node's stored bitmap into a `Surface`.
pub fn node_surface(node: &CanvasNodeData) -> Option<Surface> {
    crate::media::decode_surface(&node_source_text(node)?)
}

/// Local image analysis (`canvas-image-analysis-dialog.tsx`): palette, pixel
/// metadata, perceptual hash and the smart-crop suggestion for a ratio.
pub fn analyze_surface(surface: &Surface, ratio: &str) -> AnalysisTool {
    AnalysisTool {
        palette: extract_palette(surface, DEFAULT_PALETTE_COUNT),
        metadata: read_image_metadata(surface),
        hash: perceptual_hash(surface),
        ratio: ratio.to_string(),
        suggestion: suggest_smart_crop(surface, parse_aspect(ratio)),
        natural: (surface.width(), surface.height()),
    }
}

/// Applies a mask to a surface: shape + feather + invert, baked into alpha
/// (`ps_bake_mask` over a `psSelectionRect` mask).
pub fn masked_surface(surface: &Surface, tool: &MaskTool) -> Surface {
    let width = surface.width().max(1);
    let height = surface.height().max(1);
    let mut mask = selection_create(width, height);
    let ellipse = tool.shape == "ellipse";
    let inset = 0.06;
    let bounds = Bounds::new(
        inset * crate::vm::i2f(crate::vm::u2i(width)),
        inset * crate::vm::i2f(crate::vm::u2i(height)),
        (1.0 - inset * 2.0) * crate::vm::i2f(crate::vm::u2i(width)),
        (1.0 - inset * 2.0) * crate::vm::i2f(crate::vm::u2i(height)),
    );
    selection_rect(
        &mut mask,
        bounds,
        ellipse,
        tool.feather.max(0.0),
        true,
        SelectionMode::Replace,
    );
    if tool.invert {
        selection_invert(&mut mask);
    }
    ps_bake_mask(surface, &mask)
}

/// The split cuts as pixel bands: `(row, column, rect)` in reading order.
pub fn split_pieces(surface: &Surface, tool: &SplitTool) -> Vec<(usize, usize, TileRect)> {
    let rows = tool.rows.max(1);
    let columns = tool.columns.max(1);
    let y_cuts = split_cuts(&tool.horizontal_lines, surface.height(), rows);
    let x_cuts = split_cuts(&tool.vertical_lines, surface.width(), columns);
    let mut pieces = Vec::new();
    for row in 0..y_cuts.len().saturating_sub(1) {
        for column in 0..x_cuts.len().saturating_sub(1) {
            let x = x_cuts[column];
            let y = y_cuts[row];
            pieces.push((
                row,
                column,
                TileRect {
                    x,
                    y,
                    width: x_cuts[column + 1].saturating_sub(x).max(1),
                    height: y_cuts[row + 1].saturating_sub(y).max(1),
                },
            ));
        }
    }
    pieces
}

/// The algorithm enum behind its dialog id.
pub fn upscale_algorithm(id: &str) -> UpscaleAlgorithm {
    match id {
        "nearest" => UpscaleAlgorithm::Nearest,
        "bilinear" => UpscaleAlgorithm::Bilinear,
        _ => UpscaleAlgorithm::High,
    }
}

/// `crop_surface` with the pixel rect clamped into the source.
pub fn crop_surface_clamped(surface: &Surface, rect: TileRect) -> Surface {
    let x = rect.x.min(surface.width().saturating_sub(1));
    let y = rect.y.min(surface.height().saturating_sub(1));
    crop_surface(
        surface,
        TileRect {
            x,
            y,
            width: rect.width.min(surface.width().saturating_sub(x)).max(1),
            height: rect.height.min(surface.height().saturating_sub(y)).max(1),
        },
    )
}

/// The `data:` URL bytes for download/copy; plain base64 passes through.
pub fn data_url_bytes(text: &str) -> Option<Vec<u8>> {
    let trimmed = text.trim();
    let payload = if trimmed.starts_with("data:") {
        trimmed.split_once(',')?.1
    } else {
        trimmed
    };
    BASE64.decode(payload).ok()
}

/// The metadata rows of the analysis dialog as Slint rows.
pub fn analysis_rows(metadata: &ImageMetadata) -> Vec<crate::InfoRow> {
    metadata_rows(metadata)
        .into_iter()
        .map(|(label, value)| crate::InfoRow {
            label: label.into(),
            value: value.into(),
        })
        .collect()
}

/// The palette of the analysis dialog as Slint swatches.
pub fn palette_swatches(palette: &[PaletteColor]) -> Vec<slint::Brush> {
    palette
        .iter()
        .map(|color| slint::Brush::SolidColor(slint::Color::from_rgb_u8(color.r, color.g, color.b)))
        .collect()
}

/// Copies a decoded bitmap to the system clipboard (`copyImage`); `false` when
/// the clipboard is unavailable.
fn clipboard_image(pixels: &crate::media::DecodedPixels) -> bool {
    let image = arboard::ImageData {
        width: pixels.width as usize,
        height: pixels.height as usize,
        bytes: std::borrow::Cow::Owned(pixels.rgba.clone()),
    };
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_image(image))
        .is_ok()
}

impl App {
    /// Creates an Image node from a decoded surface.
    pub fn insert_image_node(
        &mut self,
        title: &str,
        surface: &Surface,
        at: WorldPos,
        now_ms: u64,
    ) -> Option<String> {
        let (width, height) = fit_node_size(
            crate::vm::i2f(crate::vm::u2i(surface.width())),
            crate::vm::i2f(crate::vm::u2i(surface.height())),
            FIT_NODE_MAX_SIZE,
            FIT_NODE_MAX_SIZE,
        );
        let id = self.doc.create_node(CanvasNodeType::Image, at, now_ms);
        let node = self.doc.nodes_mut().iter_mut().find(|node| node.id == id)?;
        node.title = title.to_string();
        center_node(node, at, width, height);
        let image = node_image_entry(&format!("import-{id}"), surface, None);
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.status = Some(CanvasNodeStatus::Success);
        meta.error_details = None;
        meta.natural_width = Some(image.natural_width);
        meta.natural_height = Some(image.natural_height);
        meta.bytes = Some(image.bytes);
        meta.mime_type = Some(image.mime_type.clone());
        meta.content = Some(image.content.clone());
        meta.primary_image_id = Some(image.id.clone());
        meta.images = Some(vec![image]);
        Some(id)
    }

    /// Opens a quick-tool dialog for an image node, decoding its bitmap once.
    pub fn image_tool_open(&mut self, ui: &AppWindow, node_id: &str, tool: &str) {
        let Some(node) = self.doc.node(node_id).cloned() else {
            return;
        };
        if !matches!(
            node.node_type,
            CanvasNodeType::Image | CanvasNodeType::ImageStack
        ) {
            return;
        }
        let Some(surface) = node_surface(&node) else {
            self.set_notice(ui, "This node has no image content yet".to_string(), true);
            return;
        };
        let natural = (surface.width(), surface.height());
        let kind = match tool {
            "crop" => ImageToolKind::Crop(CropTool::new(natural)),
            "mask" => ImageToolKind::Mask(MaskTool::default()),
            "split" => ImageToolKind::Split(SplitTool::default()),
            "resolution" => ImageToolKind::Resolution(ResolutionTool::new(natural)),
            "analysis" => ImageToolKind::Analysis(analyze_surface(&surface, ANALYSIS_RATIOS[0])),
            _ => return,
        };
        self.canvas_media.tool = Some(ImageTool {
            node_id: node_id.to_string(),
            kind,
            message: String::new(),
            error: false,
        });
        self.bars.preview_node = None;
        self.bars.info_node = None;
        self.sync_canvas_media(ui);
        self.sync_modals(ui);
    }

    pub fn image_tool_close(&mut self, ui: &AppWindow) {
        self.canvas_media.tool = None;
        self.sync_canvas_media(ui);
    }

    /// Chip pick of an open dialog (`ratio`, `shape`, `algorithm`, `target`).
    pub fn image_tool_pick(&mut self, ui: &AppWindow, key: &str, value: &str) {
        let Some(tool) = self.canvas_media.tool.as_mut() else {
            return;
        };
        match tool.kind {
            ImageToolKind::Crop(ref mut crop) => {
                if key == "ratio" {
                    crop.pick_ratio(value);
                }
            }
            ImageToolKind::Mask(ref mut mask) => {
                if key == "shape" && MASK_SHAPES.contains(&value) {
                    mask.shape = value.to_string();
                }
            }
            ImageToolKind::Resolution(ref mut resolution) => {
                if key == "algorithm" && UPSCALE_ALGORITHMS.contains(&value) {
                    resolution.algorithm = value.to_string();
                }
            }
            ImageToolKind::Split(_) | ImageToolKind::Analysis(_) => {}
        }
        tool.error = false;
        self.sync_canvas_media(ui);
    }

    /// Field/button of an open dialog (`feather`, `invert`, `rows`, `columns`,
    /// `add-horizontal`, `add-vertical`, `remove-line`, `reset-lines`, `target`).
    pub fn image_tool_field(&mut self, ui: &AppWindow, key: &str, text: &str) {
        let Some(tool) = self.canvas_media.tool.as_mut() else {
            return;
        };
        match tool.kind {
            ImageToolKind::Crop(ref mut crop) => {
                if let Some(rect) = crop_field(crop.rect, key, text) {
                    crop.rect = rect.clamp();
                }
            }
            ImageToolKind::Mask(ref mut mask) => match key {
                "feather" => {
                    if let Ok(value) = text.parse::<f64>() {
                        mask.feather = value.clamp(0.0, 200.0);
                    }
                }
                "invert" => mask.invert = text == "true",
                _ => {}
            },
            ImageToolKind::Split(ref mut split) => match key {
                "rows" => {
                    let rows = clamp_grid(text.parse::<f64>().unwrap_or(2.0));
                    split.rows = rows;
                    split.horizontal_lines = grid_lines(rows);
                }
                "columns" => {
                    let columns = clamp_grid(text.parse::<f64>().unwrap_or(2.0));
                    split.columns = columns;
                    split.vertical_lines = grid_lines(columns);
                }
                "add-horizontal" => split.add_line(true),
                "add-vertical" => split.add_line(false),
                "remove-line" => split.remove_line(),
                "reset-lines" => split.reset_lines(),
                _ => {}
            },
            ImageToolKind::Resolution(ref mut resolution) => {
                if key == "target" {
                    if let Ok(target) = text.parse::<usize>() {
                        resolution.target = target.clamp(
                            open_canvas_ps::MIN_UPSCALE_LONG_EDGE,
                            open_canvas_ps::MAX_UPSCALE_LONG_EDGE,
                        );
                    }
                }
            }
            ImageToolKind::Analysis(_) => {}
        }
        self.sync_canvas_media(ui);
    }

    /// A normalized pointer event of the crop stage: `handle` is "" while the
    /// crop box itself is dragged, else a compass id (`nw`, `n`, …).
    pub fn image_tool_pointer(&mut self, ui: &AppWindow, kind: &str, x: f64, y: f64, handle: &str) {
        let Some(tool) = self.canvas_media.tool.as_mut() else {
            return;
        };
        let ImageToolKind::Crop(ref mut crop) = tool.kind else {
            return;
        };
        match kind {
            "down" => {
                let mode = if handle.is_empty() {
                    crop_handle_at(crop.rect, x, y, CROP_HANDLE_TOLERANCE)
                } else {
                    handle.to_string()
                };
                crop.drag = if mode.is_empty() {
                    None
                } else {
                    Some((mode, x, y))
                };
            }
            "move" => {
                let Some((mode, last_x, last_y)) = crop.drag.clone() else {
                    return;
                };
                let dx = x - last_x;
                let dy = y - last_y;
                let ratio = crop.resolved_ratio();
                let stage = (
                    crate::vm::i2f(crate::vm::u2i(crop.natural.0)),
                    crate::vm::i2f(crate::vm::u2i(crop.natural.1)),
                );
                crop.rect = if mode == "move" {
                    move_crop(crop.rect, dx, dy)
                } else {
                    resize_crop(crop.rect, dx, dy, &mode, ratio, stage)
                };
                crop.drag = Some((mode, x, y));
            }
            _ => crop.drag = None,
        }
        self.sync_canvas_media(ui);
    }

    /// The crop dialog's "Suggest" button: the smart-crop window for the active
    /// ratio fills the rect (`suggestSmartCrop`).
    pub fn image_tool_suggest(&mut self, ui: &AppWindow) {
        let Some(node_id) = self
            .canvas_media
            .tool
            .as_ref()
            .map(|tool| tool.node_id.clone())
        else {
            return;
        };
        let Some(surface) = self.doc.node(&node_id).and_then(node_surface) else {
            return;
        };
        let Some(ImageTool {
            kind: ImageToolKind::Crop(crop),
            ..
        }) = self.canvas_media.tool.as_mut()
        else {
            return;
        };
        let width = crate::vm::i2f(crate::vm::u2i(surface.width())).max(1.0);
        let height = crate::vm::i2f(crate::vm::u2i(surface.height())).max(1.0);
        let ratio = crop.resolved_ratio().unwrap_or(width / height);
        let suggestion = suggest_smart_crop(&surface, ratio);
        crop.rect = NormRect {
            x: crate::vm::i2f(suggestion.x) / width,
            y: crate::vm::i2f(suggestion.y) / height,
            width: crate::vm::i2f(suggestion.width) / width,
            height: crate::vm::i2f(suggestion.height) / height,
        }
        .clamp();
        self.sync_canvas_media(ui);
    }

    /// The crop dialog's "Reset" button.
    pub fn image_tool_reset(&mut self, ui: &AppWindow) {
        if let Some(ImageTool {
            kind: ImageToolKind::Crop(crop),
            ..
        }) = self.canvas_media.tool.as_mut()
        {
            crop.rect = DEFAULT_CROP;
            crop.ratio = "free".to_string();
            crop.fixed = None;
            crop.drag = None;
        }
        self.sync_canvas_media(ui);
    }

    /// The analysis dialog's ratio chip: re-runs the smart-crop suggestion.
    pub fn image_tool_analysis_ratio(&mut self, ui: &AppWindow, ratio: &str) {
        if !ANALYSIS_RATIOS.contains(&ratio) {
            return;
        }
        let Some(node_id) = self
            .canvas_media
            .tool
            .as_ref()
            .map(|tool| tool.node_id.clone())
        else {
            return;
        };
        let Some(surface) = self.doc.node(&node_id).and_then(node_surface) else {
            return;
        };
        let analysis = analyze_surface(&surface, ratio);
        if let Some(ImageTool {
            kind: ImageToolKind::Analysis(current),
            ..
        }) = self.canvas_media.tool.as_mut()
        {
            *current = analysis;
        }
        self.sync_canvas_media(ui);
    }

    /// Applies the open dialog (Confirm / Apply / Smart crop).
    pub fn image_tool_apply(&mut self, ui: &AppWindow) {
        let Some(tool) = self.canvas_media.tool.clone() else {
            return;
        };
        let node_id = tool.node_id.clone();
        let Some(node) = self.doc.node(&node_id).cloned() else {
            self.image_tool_close(ui);
            return;
        };
        let Some(surface) = node_surface(&node) else {
            self.set_image_tool_message(ui, "This node has no image content".to_string());
            return;
        };
        let now = self.now_ms();
        let result = match &tool.kind {
            ImageToolKind::Crop(crop) => {
                let rect = crop_rect_pixels(crop.rect, surface.width(), surface.height());
                let cropped = crop_surface_clamped(&surface, rect);
                self.write_node_surface(&node_id, &cropped, now);
                Ok("Cropped the image".to_string())
            }
            ImageToolKind::Mask(mask) => {
                let masked = masked_surface(&surface, mask);
                self.write_node_surface(&node_id, &masked, now);
                Ok("Masked the image".to_string())
            }
            ImageToolKind::Split(split) => {
                let pieces = split_pieces(&surface, split);
                let count = pieces.len();
                let created = self.split_into_children(&node, &surface, &pieces, now);
                Ok(format!("Created {created} of {count} pieces"))
            }
            ImageToolKind::Resolution(resolution) => {
                let (width, height) = resolution.resolved();
                let resized = resample_surface(
                    &surface,
                    width,
                    height,
                    upscale_algorithm(&resolution.algorithm),
                );
                let child = self.derived_image_node(&node, &resized, "Resized", now);
                if child.is_some() {
                    Ok(format!("Created a {width} x {height} copy"))
                } else {
                    Err("The resampled image could not be stored".to_string())
                }
            }
            ImageToolKind::Analysis(analysis) => {
                let rect =
                    crop_rect_pixels(analysis.suggested_rect(), surface.width(), surface.height());
                let cropped = crop_surface_clamped(&surface, rect);
                self.write_node_surface(&node_id, &cropped, now);
                Ok("Smart-cropped the image".to_string())
            }
        };
        match result {
            Ok(message) => {
                self.canvas_media.tool = None;
                self.note_doc_change();
                self.set_notice(ui, message, false);
                self.sync_all(ui);
            }
            Err(message) => self.set_image_tool_message(ui, message),
        }
    }

    fn set_image_tool_message(&mut self, ui: &AppWindow, message: String) {
        if let Some(tool) = self.canvas_media.tool.as_mut() {
            tool.message = message;
            tool.error = true;
        }
        self.sync_canvas_media(ui);
    }

    /// Writes a new bitmap into an image node: content, first stack entry,
    /// natural size, bytes and the stale thumbnail cleared so the renderer's
    /// `thumbnail || content` order shows the new pixels.
    pub fn write_node_surface(&mut self, node_id: &str, surface: &Surface, now_ms: u64) -> bool {
        let Ok((data_url, bytes)) = surface_png(surface) else {
            return false;
        };
        let natural_width = crate::vm::i2f(crate::vm::u2i(surface.width()));
        let natural_height = crate::vm::i2f(crate::vm::u2i(surface.height()));
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return false;
        };
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.content = Some(data_url.clone());
        meta.thumbnail = None;
        meta.thumbnail_key = None;
        meta.natural_width = Some(natural_width);
        meta.natural_height = Some(natural_height);
        meta.bytes = Some(bytes);
        meta.mime_type = Some("image/png".to_string());
        let entry_id = meta
            .primary_image_id
            .clone()
            .unwrap_or_else(|| format!("edit-{node_id}"));
        let entry = CanvasNodeImage {
            id: entry_id.clone(),
            status: CanvasNodeStatus::Success,
            error_details: None,
            content: data_url,
            storage_key: meta.storage_key.clone(),
            thumbnail: None,
            thumbnail_key: None,
            natural_width,
            natural_height,
            bytes,
            mime_type: "image/png".to_string(),
        };
        match meta.images.as_mut() {
            Some(images) if !images.is_empty() => {
                if let Some(slot) = images.iter_mut().find(|image| image.id == entry_id) {
                    *slot = entry;
                } else {
                    images[0] = entry;
                    meta.primary_image_id = images.first().map(|image| image.id.clone());
                }
            }
            _ => {
                meta.primary_image_id = Some(entry_id);
                meta.images = Some(vec![entry]);
            }
        }
        self.doc.commit(now_ms);
        true
    }

    /// Creates one Image node per split piece to the right of the source
    /// (`splitImageNode`: `startX = x + width + 96`, cell size `width / columns`,
    /// 16px gap) and selects them. Returns the number of nodes created.
    pub fn split_into_children(
        &mut self,
        source: &CanvasNodeData,
        surface: &Surface,
        pieces: &[(usize, usize, TileRect)],
        now_ms: u64,
    ) -> usize {
        let columns = pieces
            .iter()
            .map(|(_, column, _)| *column)
            .max()
            .unwrap_or(0)
            + 1;
        let rows = pieces.iter().map(|(row, _, _)| *row).max().unwrap_or(0) + 1;
        let cell_width = source.width / crate::vm::i2f(crate::vm::u2i(columns)).max(1.0);
        let cell_height = source.height / crate::vm::i2f(crate::vm::u2i(rows)).max(1.0);
        let start_x = source.position.x + source.width + SPLIT_START_OFFSET;
        let start_y = source.position.y;
        let mut created: Vec<String> = Vec::new();
        for (row, column, rect) in pieces {
            let piece = crop_surface_clamped(surface, *rect);
            let title = format!("{} {}x{}", source.title, row + 1, column + 1);
            let at = WorldPos::new(
                start_x
                    + crate::vm::i2f(crate::vm::u2i(*column)) * (cell_width + SPLIT_GAP)
                    + cell_width / 2.0,
                start_y
                    + crate::vm::i2f(crate::vm::u2i(*row)) * (cell_height + SPLIT_GAP)
                    + cell_height / 2.0,
            );
            let id = self.doc.create_node(CanvasNodeType::Image, at, now_ms);
            if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                node.title = title;
                center_node(node, at, cell_width, cell_height);
                let image = node_image_entry(&format!("split-{id}"), &piece, None);
                let meta = node
                    .metadata
                    .get_or_insert_with(CanvasNodeMetadata::default);
                meta.status = Some(CanvasNodeStatus::Success);
                meta.natural_width = Some(image.natural_width);
                meta.natural_height = Some(image.natural_height);
                meta.bytes = Some(image.bytes);
                meta.mime_type = Some(image.mime_type.clone());
                meta.content = Some(image.content.clone());
                meta.primary_image_id = Some(image.id.clone());
                meta.images = Some(vec![image]);
                meta.prompt = source
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.prompt.clone());
                created.push(id);
            }
        }
        self.select_nodes(&created);
        created.len()
    }

    /// Creates a derived Image node from a processed surface (the resolution
    /// tool's child; `insertDerivedAsset` with one child).
    pub fn derived_image_node(
        &mut self,
        source: &CanvasNodeData,
        surface: &Surface,
        suffix: &str,
        now_ms: u64,
    ) -> Option<String> {
        let (width, height) = fit_node_size(
            crate::vm::i2f(crate::vm::u2i(surface.width())),
            crate::vm::i2f(crate::vm::u2i(surface.height())),
            FIT_NODE_MAX_SIZE,
            FIT_NODE_MAX_SIZE,
        );
        let at = WorldPos::new(
            source.position.x + source.width + SPLIT_START_OFFSET + width / 2.0,
            source.position.y + height / 2.0,
        );
        let id = self.doc.create_node(CanvasNodeType::Image, at, now_ms);
        let node = self.doc.nodes_mut().iter_mut().find(|node| node.id == id)?;
        node.title = format!("{} {suffix}", source.title);
        center_node(node, at, width, height);
        let image = node_image_entry(&format!("derived-{id}"), surface, None);
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.status = Some(CanvasNodeStatus::Success);
        meta.natural_width = Some(image.natural_width);
        meta.natural_height = Some(image.natural_height);
        meta.bytes = Some(image.bytes);
        meta.mime_type = Some(image.mime_type.clone());
        meta.content = Some(image.content.clone());
        meta.primary_image_id = Some(image.id.clone());
        meta.images = Some(vec![image]);
        meta.prompt = source
            .metadata
            .as_ref()
            .and_then(|meta| meta.prompt.clone());
        Some(id)
    }

    /// Selects a freshly created node set (the web's `select: "children"`).
    pub fn select_nodes(&mut self, ids: &[String]) {
        for (index, id) in ids.iter().enumerate() {
            self.doc.select_node(id, index > 0);
        }
    }

    /// Copy-to-clipboard (`copyImage`): the decoded bitmap goes to the system
    /// clipboard as an image, else its data URL as text.
    pub fn image_copy_core(&mut self, node_id: &str) -> Result<String, String> {
        let node = self.doc.node(node_id).ok_or("Node not found")?.clone();
        let text = node_source_text(&node).ok_or("This node has no image content")?;
        if let Some(pixels) = crate::media::decode_pixels(&text) {
            if clipboard_image(&pixels) {
                return Ok("Copied the image to the clipboard".to_string());
            }
        }
        if crate::system_clipboard::write_text(&text) {
            return Ok("Copied the image data URL to the clipboard".to_string());
        }
        Err("The clipboard is unavailable".to_string())
    }

    /// Download-to-file: writes the stored bytes beside the exports with the
    /// extension of the stored mime type.
    pub fn image_download_core(&mut self, node_id: &str) -> Result<String, String> {
        let node = self.doc.node(node_id).ok_or("Node not found")?.clone();
        let text = node_source_text(&node).ok_or("This node has no image content")?;
        let mime = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.mime_type.clone())
            .unwrap_or_else(|| "image/png".to_string());
        let bytes = data_url_bytes(&text).ok_or("The stored image could not be decoded")?;
        let path = self
            .store
            .export_path_for(&node.title)
            .with_extension(extension_for_mime(&mime));
        crate::image::export::write_bytes(&path, &bytes)?;
        Ok(format!("Downloaded {}", path.display()))
    }

    /// Image-stack gallery actions (`image-stack-preview.tsx`): set cover,
    /// take out, remove, retry, download, copy and split all.
    pub fn stack_action_core(
        &mut self,
        node_id: &str,
        image_id: &str,
        action: &str,
    ) -> Result<String, String> {
        let now = self.now_ms();
        let Some(node) = self.doc.node(node_id).cloned() else {
            return Err("Node not found".to_string());
        };
        if node.node_type != CanvasNodeType::ImageStack {
            return Err("Not an image stack".to_string());
        }
        match action {
            "set-cover" => self.stack_set_cover(node_id, image_id, now),
            "take-out" => self.stack_take_out(&node, image_id, now),
            "remove" => {
                if self.remove_stack_image(node_id, image_id) {
                    self.doc.commit(now);
                    Ok("Removed the image from the stack".to_string())
                } else {
                    Err("Image not found in the stack".to_string())
                }
            }
            "retry" => self.stack_retry(node_id, image_id, now),
            "download" => {
                let image = stack_image(&node, image_id)?;
                let bytes = data_url_bytes(&image.content)
                    .ok_or("The stored image could not be decoded")?;
                let path = self
                    .store
                    .export_path_for(&format!("{}-{image_id}", node.title))
                    .with_extension(extension_for_mime(&image.mime_type));
                crate::image::export::write_bytes(&path, &bytes)?;
                Ok(format!("Downloaded {}", path.display()))
            }
            "copy" => {
                let image = stack_image(&node, image_id)?;
                let pixels = crate::media::decode_pixels(&image.content)
                    .ok_or("The stored image could not be decoded")?;
                if clipboard_image(&pixels) {
                    Ok("Copied the image to the clipboard".to_string())
                } else {
                    Err("The clipboard is unavailable".to_string())
                }
            }
            "split-all" => self.split_stack_all_core(node_id),
            _ => Err("Unknown stack action".to_string()),
        }
    }

    fn stack_set_cover(
        &mut self,
        node_id: &str,
        image_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return Err("Node not found".to_string());
        };
        let Some(meta) = node.metadata.as_mut() else {
            return Err("Node has no metadata".to_string());
        };
        let Some(images) = meta.images.as_ref() else {
            return Err("Node has no images".to_string());
        };
        let Some(cover) = images.iter().find(|image| image.id == image_id).cloned() else {
            return Err("Image not found in the stack".to_string());
        };
        meta.primary_image_id = Some(image_id.to_string());
        meta.content = Some(cover.content.clone());
        meta.natural_width = Some(cover.natural_width);
        meta.natural_height = Some(cover.natural_height);
        meta.bytes = Some(cover.bytes);
        meta.mime_type = Some(cover.mime_type.clone());
        self.doc.commit(now_ms);
        Ok("Cover updated".to_string())
    }

    fn stack_take_out(
        &mut self,
        node: &CanvasNodeData,
        image_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let taken = stack_image(node, image_id)?.clone();
        let position = stack_images(node)
            .iter()
            .position(|image| image.id == image_id)
            .unwrap_or(0);
        if !self.remove_stack_image(&node.id, image_id) {
            return Err("Image not found in the stack".to_string());
        }
        let Some(surface) = crate::media::decode_surface(&taken.content) else {
            self.doc.commit(now_ms);
            return Ok("Removed the image from the stack".to_string());
        };
        let title = format!("{} {}", node.title, position + 1);
        let at = WorldPos::new(
            node.position.x + node.width + SPLIT_START_OFFSET,
            node.position.y + node.height / 2.0,
        );
        let created = self.insert_image_node(&title, &surface, at, now_ms);
        if created.is_some() {
            self.doc.commit(now_ms);
            Ok("Took the image out of the stack".to_string())
        } else {
            Ok("Removed the image from the stack".to_string())
        }
    }

    fn stack_retry(
        &mut self,
        node_id: &str,
        image_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return Err("Node not found".to_string());
        };
        let Some(meta) = node.metadata.as_mut() else {
            return Err("Node has no metadata".to_string());
        };
        let Some(images) = meta.images.as_mut() else {
            return Err("Node has no images".to_string());
        };
        let Some(image) = images.iter_mut().find(|image| image.id == image_id) else {
            return Err("Image not found in the stack".to_string());
        };
        let recovered = !image.content.is_empty();
        image.status = if recovered {
            CanvasNodeStatus::Success
        } else {
            CanvasNodeStatus::Idle
        };
        image.error_details = None;
        self.doc.commit(now_ms);
        Ok(if recovered {
            "Retried the image".to_string()
        } else {
            "The image has no content to retry".to_string()
        })
    }

    /// `splitStackImages` (project.tsx:1386): every stack image with content
    /// becomes its own Image node in a square-ish grid to the right.
    pub fn split_stack_all_core(&mut self, node_id: &str) -> Result<String, String> {
        let now = self.now_ms();
        let Some(node) = self.doc.node(node_id).cloned() else {
            return Err("Node not found".to_string());
        };
        let images: Vec<CanvasNodeImage> = stack_images(&node)
            .iter()
            .filter(|image| !image.content.is_empty())
            .cloned()
            .collect();
        if images.len() < 2 {
            return Err("A stack needs at least two images to split".to_string());
        }
        let columns = integer_sqrt_ceil(images.len());
        let edge = node.width.max(node.height);
        let sizes: Vec<(f64, f64)> = images
            .iter()
            .map(|image| fit_node_size(image.natural_width, image.natural_height, edge, edge))
            .collect();
        let cell_width = sizes.iter().map(|size| size.0).fold(0.0, f64::max);
        let cell_height = sizes.iter().map(|size| size.1).fold(0.0, f64::max);
        let start_x = node.position.x + node.width + SPLIT_START_OFFSET;
        let start_y = node.position.y;
        let mut created: Vec<String> = Vec::new();
        for (index, image) in images.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            let at = WorldPos::new(
                start_x
                    + crate::vm::i2f(crate::vm::u2i(column)) * (cell_width + SPLIT_GAP)
                    + cell_width / 2.0,
                start_y
                    + crate::vm::i2f(crate::vm::u2i(row)) * (cell_height + SPLIT_GAP)
                    + cell_height / 2.0,
            );
            let id = self.doc.create_node(CanvasNodeType::Image, at, now);
            let Some(child) = self.doc.nodes_mut().iter_mut().find(|child| child.id == id) else {
                continue;
            };
            child.title = format!("{} {}", node.title, index + 1);
            center_node(child, at, sizes[index].0, sizes[index].1);
            let meta = child
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.content = Some(image.content.clone());
            meta.storage_key.clone_from(&image.storage_key);
            meta.thumbnail.clone_from(&image.thumbnail);
            meta.thumbnail_key.clone_from(&image.thumbnail_key);
            meta.natural_width = Some(image.natural_width);
            meta.natural_height = Some(image.natural_height);
            meta.bytes = Some(image.bytes);
            meta.mime_type = Some(image.mime_type.clone());
            meta.status = Some(CanvasNodeStatus::Success);
            meta.primary_image_id = Some(image.id.clone());
            meta.images = Some(vec![image.clone()]);
            meta.prompt = node.metadata.as_ref().and_then(|meta| meta.prompt.clone());
            created.push(id);
        }
        if created.is_empty() {
            return Err("No stack image could be split out".to_string());
        }
        self.select_nodes(&created);
        self.doc.commit(now);
        Ok(format!("Split {} images out of the stack", created.len()))
    }

    fn remove_stack_image(&mut self, node_id: &str, image_id: &str) -> bool {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return false;
        };
        let Some(meta) = node.metadata.as_mut() else {
            return false;
        };
        let Some(images) = meta.images.as_mut() else {
            return false;
        };
        let before = images.len();
        images.retain(|image| image.id != image_id);
        if images.len() == before {
            return false;
        }
        if meta.primary_image_id.as_deref() == Some(image_id) {
            meta.primary_image_id = images.first().map(|image| image.id.clone());
        }
        if let Some(cover) = images.first().cloned() {
            meta.content = Some(cover.content.clone());
            meta.natural_width = Some(cover.natural_width);
            meta.natural_height = Some(cover.natural_height);
            meta.bytes = Some(cover.bytes);
            meta.mime_type = Some(cover.mime_type.clone());
        }
        true
    }

    /// The preview modal's download button: the node's bitmap, or one gallery
    /// image when `image_id` is non-empty.
    pub fn preview_download(&mut self, image_id: &str) -> Result<String, String> {
        let node_id = self.bars.preview_node.clone().ok_or("No preview is open")?;
        if image_id.is_empty() {
            self.image_download_core(&node_id)
        } else {
            self.stack_action_core(&node_id, image_id, "download")
        }
    }

    /// The preview modal's copy button.
    pub fn preview_copy(&mut self, image_id: &str) -> Result<String, String> {
        let node_id = self.bars.preview_node.clone().ok_or("No preview is open")?;
        if image_id.is_empty() {
            self.image_copy_core(&node_id)
        } else {
            self.stack_action_core(&node_id, image_id, "copy")
        }
    }

    /// Applies a gallery action and pushes the result: the notice line plus a
    /// full refresh (the stack metadata changed). The stack is the node the
    /// preview modal is bound to (`bars.preview_node`).
    pub fn stack_action(&mut self, ui: &AppWindow, image_id: &str, action: &str) {
        let Some(node_id) = self.bars.preview_node.clone() else {
            return;
        };
        match self.stack_action_core(&node_id, image_id, action) {
            Ok(message) => {
                if action == "split-all" {
                    self.bars.preview_node = None;
                    self.note_doc_change();
                    self.sync_all(ui);
                } else {
                    self.note_doc_change();
                    self.sync_rows_of(ui, std::slice::from_ref(&node_id));
                    self.sync_modals(ui);
                }
                self.set_notice(ui, message, false);
            }
            Err(message) => self.set_notice(ui, message, true),
        }
    }

    /// Slint push of the open quick-tool dialog.
    pub fn sync_canvas_media(&mut self, ui: &AppWindow) {
        self.sync_image_tool(ui);
    }

    fn sync_image_tool(&mut self, ui: &AppWindow) {
        let Some(tool) = self.canvas_media.tool.clone() else {
            ui.set_image_tool_open(false);
            return;
        };
        let node = self.doc.node(&tool.node_id).cloned();
        let preview = node
            .as_ref()
            .and_then(node_source_text)
            .and_then(|text| self.media.image(&text))
            .unwrap_or_default();
        let mut state = tool_state_base(&tool, node.as_ref(), preview);
        fill_tool_kind(&mut state, &tool.kind);
        ui.set_image_tool_state(state);
        ui.set_image_tool_open(true);
    }
}

/// The state fields every tool dialog shares.
fn tool_state_base(
    tool: &ImageTool,
    node: Option<&CanvasNodeData>,
    preview: slint::Image,
) -> crate::ImageToolState {
    crate::ImageToolState {
        kind: tool.kind.id().into(),
        title: node
            .map_or_else(String::new, |node| node.title.clone())
            .as_str()
            .into(),
        node: tool.node_id.as_str().into(),
        image: preview,
        source: slint::SharedString::default(),
        readout: tool.message.as_str().into(),
        message: tool.message.as_str().into(),
        error: tool.error,
        ratio: slint::SharedString::default(),
        rect_x: 0.0,
        rect_y: 0.0,
        rect_width: 0.0,
        rect_height: 0.0,
        mask_shape: slint::SharedString::default(),
        mask_feather: 0.0,
        mask_invert: false,
        rows: 1,
        columns: 1,
        horizontal_lines: empty_floats(),
        vertical_lines: empty_floats(),
        target: 0,
        algorithm: slint::SharedString::default(),
        palette: empty_brushes(),
        palette_labels: empty_strings(),
        meta_rows: slint::ModelRc::new(slint::VecModel::from(Vec::new())),
        hash: slint::SharedString::default(),
        suggestion: slint::SharedString::default(),
        apply_label: "Apply".into(),
    }
}

/// The per-kind fields of the tool dialog state.
fn fill_tool_kind(state: &mut crate::ImageToolState, kind: &ImageToolKind) {
    match kind {
        ImageToolKind::Crop(crop) => {
            state.source = format!("{} x {}", crop.natural.0, crop.natural.1).into();
            state.ratio = crop.ratio.as_str().into();
            state.rect_x = crate::vm::len(crop.rect.x);
            state.rect_y = crate::vm::len(crop.rect.y);
            state.rect_width = crate::vm::len(crop.rect.width);
            state.rect_height = crate::vm::len(crop.rect.height);
            state.readout = format!(
                "{} px · {} · original {} x {}",
                crop.size_label(),
                crop.ratio_label(),
                crop.natural.0,
                crop.natural.1
            )
            .into();
            state.apply_label = "Crop".into();
        }
        ImageToolKind::Mask(mask) => {
            state.mask_shape = mask.shape.as_str().into();
            state.mask_feather = crate::vm::len(mask.feather);
            state.mask_invert = mask.invert;
            state.readout = "Shape + feather bake into the alpha channel".into();
            state.apply_label = "Apply mask".into();
        }
        ImageToolKind::Split(split) => {
            state.rows = crate::vm::count_i32(split.rows);
            state.columns = crate::vm::count_i32(split.columns);
            state.horizontal_lines = float_model(&split.horizontal_lines);
            state.vertical_lines = float_model(&split.vertical_lines);
            state.readout = format!("{} pieces", split.total()).into();
            state.apply_label = "Split into pieces".into();
        }
        ImageToolKind::Resolution(resolution) => {
            let (width, height) = resolution.resolved();
            state.target = crate::vm::count_i32(resolution.target);
            state.algorithm = resolution.algorithm.as_str().into();
            state.source = format!("{} x {}", resolution.natural.0, resolution.natural.1).into();
            state.readout = format!("{width} x {height} px").into();
            state.apply_label = "Resample".into();
        }
        ImageToolKind::Analysis(analysis) => {
            state.ratio = analysis.ratio.as_str().into();
            state.hash = analysis.hash.as_str().into();
            state.source = format!("{} x {}", analysis.natural.0, analysis.natural.1).into();
            state.palette =
                slint::ModelRc::new(slint::VecModel::from(palette_swatches(&analysis.palette)));
            state.palette_labels = slint::ModelRc::new(slint::VecModel::from(
                analysis
                    .palette
                    .iter()
                    .map(|color| slint::SharedString::from(color.hex()))
                    .collect::<Vec<_>>(),
            ));
            state.meta_rows =
                slint::ModelRc::new(slint::VecModel::from(analysis_rows(&analysis.metadata)));
            let suggestion = analysis.suggested_rect();
            state.suggestion = format!(
                "{} x {} at {}, {}",
                analysis.suggestion.width,
                analysis.suggestion.height,
                analysis.suggestion.x,
                analysis.suggestion.y
            )
            .into();
            state.rect_x = crate::vm::len(suggestion.x);
            state.rect_y = crate::vm::len(suggestion.y);
            state.rect_width = crate::vm::len(suggestion.width);
            state.rect_height = crate::vm::len(suggestion.height);
            state.apply_label = "Smart crop".into();
        }
    }
}

/// A normalized line list as the Slint `[float]` model.
fn float_model(lines: &[f64]) -> slint::ModelRc<f32> {
    slint::ModelRc::new(slint::VecModel::from(
        lines
            .iter()
            .map(|line| crate::vm::len(*line))
            .collect::<Vec<f32>>(),
    ))
}

/// Centers a freshly created node on `at` for its final size: `Doc::create_node`
/// lays out the type's default spec size, while the web computes the position
/// from the fitted size before inserting (`use-canvas-insertion.ts:53`).
fn center_node(node: &mut CanvasNodeData, at: WorldPos, width: f64, height: f64) {
    node.position = Position::new(at.x - width / 2.0, at.y - height / 2.0);
    node.width = width;
    node.height = height;
}

/// The stack image with `image_id`, or the not-found error.
fn stack_image<'a>(
    node: &'a CanvasNodeData,
    image_id: &str,
) -> Result<&'a CanvasNodeImage, String> {
    stack_images(node)
        .iter()
        .find(|image| image.id == image_id)
        .ok_or_else(|| "Image not found in the stack".to_string())
}

fn empty_floats() -> slint::ModelRc<f32> {
    slint::ModelRc::new(slint::VecModel::from(Vec::new()))
}

fn empty_brushes() -> slint::ModelRc<slint::Brush> {
    slint::ModelRc::new(slint::VecModel::from(Vec::new()))
}

fn empty_strings() -> slint::ModelRc<slint::SharedString> {
    slint::ModelRc::new(slint::VecModel::from(Vec::new()))
}

fn integer_sqrt_ceil(value: usize) -> usize {
    let mut root = 1_usize;
    while root * root < value {
        root += 1;
    }
    root
}

/// One numeric crop field edit (`x`, `y`, `width`, `height`; a trailing `%` is
/// accepted and divided by 100).
fn crop_field(rect: NormRect, key: &str, text: &str) -> Option<NormRect> {
    let trimmed = text.trim();
    let percent = trimmed.ends_with('%');
    let raw = trimmed.trim_end_matches('%').trim().parse::<f64>().ok()?;
    let value = if percent { raw / 100.0 } else { raw }.clamp(0.0, 1.0);
    Some(match key {
        "x" => NormRect { x: value, ..rect },
        "y" => NormRect { y: value, ..rect },
        "width" => NormRect {
            width: value.max(MIN_CROP),
            ..rect
        },
        "height" => NormRect {
            height: value.max(MIN_CROP),
            ..rect
        },
        _ => return None,
    })
}

/// Registers the canvas media callbacks (image tool dialog, stack gallery
/// actions and the preview modal's buttons).
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of fifteen callbacks; splitting adds indirection, not clarity"
)]
pub fn wire_image_tools(ui: &AppWindow, app: &std::rc::Rc<std::cell::RefCell<App>>) {
    let with = |app: &std::rc::Rc<std::cell::RefCell<App>>,
                ui: &AppWindow,
                f: &dyn Fn(&mut App, &AppWindow)| {
        crate::app::with_app(app, |app| f(app, ui));
    };
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with(&app, &ui, &|app, ui| app.image_tool_close(ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_pick(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (key, value) = (key.to_string(), value.to_string());
            with(&app, &ui, &|app, ui| app.image_tool_pick(ui, &key, &value));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_field(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (key, value) = (key.to_string(), value.to_string());
            with(&app, &ui, &|app, ui| {
                app.image_tool_field(ui, &key, &value);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_pointer(move |kind, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let kind = kind.to_string();
            with(&app, &ui, &|app, ui| {
                app.image_tool_pointer(ui, &kind, f64::from(x), f64::from(y), "");
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_suggest(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with(&app, &ui, &|app, ui| app.image_tool_suggest(ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_reset(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with(&app, &ui, &|app, ui| app.image_tool_reset(ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_apply(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with(&app, &ui, &|app, ui| app.image_tool_apply(ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_tool_ratio(move |ratio| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let ratio = ratio.to_string();
            with(&app, &ui, &|app, ui| {
                app.image_tool_analysis_ratio(ui, &ratio);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_stack_action(move |image_id, action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let (image_id, action) = (image_id.to_string(), action.to_string());
            with(&app, &ui, &|app, ui| {
                app.stack_action(ui, &image_id, &action);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_preview_download(move |image_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let image_id = image_id.to_string();
            with(
                &app,
                &ui,
                &|app, ui| match app.preview_download(&image_id) {
                    Ok(message) => app.set_notice(ui, message, false),
                    Err(message) => app.set_notice(ui, message, true),
                },
            );
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_preview_copy(move |image_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let image_id = image_id.to_string();
            with(&app, &ui, &|app, ui| match app.preview_copy(&image_id) {
                Ok(message) => app.set_notice(ui, message, false),
                Err(message) => app.set_notice(ui, message, true),
            });
        });
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tools_render_tests.rs"]
mod render_tests;
