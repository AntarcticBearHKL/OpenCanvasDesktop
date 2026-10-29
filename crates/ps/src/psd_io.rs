//! The mapping between our `ImageProject` document and the vendored
//! `open-canvas-psd` codec.
//!
//! Export writes one PSD layer per project layer (name, visibility, opacity,
//! blend mode, pixels) and always supplies the merged composite, because the
//! codec never composites. Import turns every PSD layer into a pixel layer of a
//! new `ImageProject`.
//!
//! ## What survives the round trip
//!
//! * Layer name, visibility, opacity (2-decimal document precision) and the
//!   16 blend modes map 1:1 onto [`open_canvas_psd::BlendMode`]. PSD-only modes
//!   (`dissolve`, `linear-burn`, ...) and unknown keys import as `normal`.
//! * Pixel/image layers with no rotation, mesh transform or enabled layer style
//!   export their stored bitmap, cropped to the document and resampled to the
//!   export scale. That is the only path where layer pixels are byte-identical
//!   on import.
//! * Layer masks (`maskStorageKey`) export as PSD user masks and import back
//!   into `maskStorageKey` surfaces; a mask offset from its layer is shifted
//!   into the layer box with the mask's default value outside.
//!
//! ## What is flattened
//!
//! * Text, shape, adjustment, and any rotated/transformed/styled raster layer
//!   are rendered through the compositor and stored as one full-canvas raster
//!   layer carrying that appearance. Text becomes pixels (no editable text),
//!   shape paths and adjustment parameters are not written. Opacity and blend
//!   mode still apply normally to text/shape layers.
//! * An adjustment layer adjusts everything below it, so it cannot be rendered
//!   in isolation: it exports the changed pixels between the document rendered
//!   up to the layer and the document below it, as an opaque full-canvas layer
//!   with normal blending. Pixels the adjustment leaves alone stay transparent.
//!   The document looks the same; the adjustment is no longer editable.
//! * Groups export as real PSD groups (`GroupSpec`), including visibility,
//!   opacity and blend mode; a group nested deeper than [`MAX_GROUP_DEPTH`]
//!   flattens into its parent.
//! * Clipping layers, smart objects, text records, layer styles, paths and
//!   alpha channels have no counterpart in our document model: PSD import
//!   rasterizes them into the pixel layer that carries their appearance.
//!
//! The PSD document size is the board's export size (resolution tier × ratio),
//! and importing a PSD sets the board's ratio/resolution so that re-exporting
//! keeps the same pixel size when the PSD size is one of the studio's presets.
//! The board background is baked into the merged composite only; it is not a
//! layer, so import starts from a transparent board.
//!
//! This module is pure: no file paths, no menus, no app state. Hosts supply
//! the bitmaps keyed by storage key, and optionally a [`TextRasterizer`] so
//! text layers export as pixels (without one they stay transparent, like the
//! compositor's own headless default).

use std::collections::HashMap;

use open_canvas_psd::{
    BlendMode, ColorMode, GroupSpec, LayerNode, LayerSpec, MaskSpec, PixelData, PsdBuilder,
    PsdFile, Rect,
};
use thiserror::Error;

use crate::board::{group_children, read_media_dimensions, top_layers, DEFAULT_RATIO};
use crate::compositor::{render_document_with_text, TextRasterizer};
use crate::doc::{
    BoardResolution, CanvasPsLayer, CanvasPsLayerKind, ImageProject, SmartCanvasBoard,
};
use crate::geom::Bounds;
use crate::surface::{clamp_u8, px_size, Surface};

/// Failure of the document ↔ PSD mapping. The prefixes match the messages the
/// desktop app has always shown in its import/export modal.
#[derive(Debug, Error)]
pub enum PsdIoError {
    /// Writing a project as PSD bytes failed.
    #[error("Export PSD failed: {0}")]
    Export(String),
    /// Reading PSD bytes as a project failed.
    #[error("Open PSD failed: {0}")]
    Import(String),
}

/// Export scale: the number of document pixels a board unit maps to.
#[derive(Debug, Clone, Copy)]
struct Scale {
    x: f64,
    y: f64,
}

/// Group nesting the exporter writes; deeper groups flatten into the parent.
pub const MAX_GROUP_DEPTH: usize = 32;

/// Document/layer dimension cap. Mirrors the studio's `MAX_IMAGE_DIM`: a larger
/// PSD would make every layer box and bitmap exceed what the editor handles.
pub const MAX_PSD_DIMENSION: usize = 4096;

/// `usize` -> `i64` for buffer coordinates (dimensions come from checked pixel
/// counts, far below `i64::MAX`).
fn i64_of(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// PSD `u32` dimension -> `usize`.
fn usize_of(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// PSD `i32` coordinate -> `i64`.
fn coord(value: i32) -> i64 {
    i64::from(value)
}

/// Finite f64 -> i64 with truncation (pixel coordinates are floored by callers).
fn f2i(value: f64) -> i64 {
    value as i64
}

/// Small i64 -> f64 (document dimensions, pixel coordinates).
fn i2f(value: i64) -> f64 {
    value as f64
}

/// Small usize -> i64 (buffer dimensions).
fn u2i(value: usize) -> i64 {
    value as i64
}

/// Non-negative i64 -> usize (pixel counts); negatives clamp to 0.
fn i2u(value: i64) -> usize {
    value.max(0) as usize
}

/// The nine ratio ids the board's export-size table knows
/// (`crate::board::RATIOS`; that table is private, so the ids are mirrored here
/// for import sizing).
const PSD_RATIOS: [&str; 9] = [
    "1:1", "2:3", "3:2", "4:3", "3:4", "16:9", "9:16", "21:9", "9:21",
];

// ---------------------------------------------------------------------------
// Shared helpers: sizes, blend modes, opacity
// ---------------------------------------------------------------------------

/// The pixel size the compositor renders at (resolution tier × ratio), or a
/// clear error when the board's ratio is not in the table.
fn document_size(project: &ImageProject) -> Result<(usize, usize), PsdIoError> {
    let ratio = if project.board_ratio.is_empty() {
        DEFAULT_RATIO
    } else {
        project.board_ratio.as_str()
    };
    let (width, height) = read_media_dimensions(project.board_resolution, ratio);
    if width < 1.0 || height < 1.0 {
        return Err(PsdIoError::Export(format!(
            "the board ratio \"{ratio}\" has no export size"
        )));
    }
    Ok((px_size(width), px_size(height)))
}

/// The merged board composite at export size (the PSD's merged image).
fn composite_surface(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
    text: Option<&dyn TextRasterizer>,
) -> Surface {
    let board = SmartCanvasBoard::from(project);
    render_document_with_text(&board, bitmaps, text)
}

/// The export scale for a document of `doc` pixels over a `project` board.
fn export_scale(project: &ImageProject, doc: (usize, usize)) -> Scale {
    Scale {
        x: i2f(u2i(doc.0)) / project.width.max(1.0),
        y: i2f(u2i(doc.1)) / project.height.max(1.0),
    }
}

/// Our blend id -> PSD key. Unknown ids (the document only writes the 16 known
/// ones) fall back to normal.
fn psd_blend(id: &str) -> BlendMode {
    match id {
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "color-dodge" => BlendMode::ColorDodge,
        "color-burn" => BlendMode::ColorBurn,
        "hard-light" => BlendMode::HardLight,
        "soft-light" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

/// PSD blend mode -> our blend id. Modes our document model lacks (dissolve,
/// linear burn, pass-through, unknown keys) import as `normal`.
fn canvas_blend(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
        _ => "normal",
    }
}

/// 0..1 opacity -> PSD 0..=255 (`clamp_u8` rounds, like the document's own
/// two-decimal opacity clamp).
fn psd_opacity(opacity: f64) -> u8 {
    clamp_u8(opacity.clamp(0.0, 1.0) * 255.0)
}

/// PSD 0..=255 opacity -> 0..1 rounded to the document's two decimals, so
/// `0.5 -> 128 -> 0.5` round-trips.
fn canvas_opacity(opacity: u8) -> f64 {
    (f64::from(opacity) / 2.55).round() / 100.0
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Export `project` as a PSD/PSB byte string, merged composite included.
///
/// Text layers stay transparent because there is no glyph rasterizer; hosts
/// with one call [`export_project_with_text`].
pub fn export_project(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
) -> Result<Vec<u8>, PsdIoError> {
    export_project_with_text(project, bitmaps, None)
}

/// [`export_project`] with a host glyph rasterizer for text layers.
pub fn export_project_with_text(
    project: &ImageProject,
    bitmaps: &HashMap<String, Surface>,
    text: Option<&dyn TextRasterizer>,
) -> Result<Vec<u8>, PsdIoError> {
    let doc = document_size(project)?;
    let width =
        u32::try_from(doc.0).map_err(|_| PsdIoError::Export("the document is too wide".into()))?;
    let height =
        u32::try_from(doc.1).map_err(|_| PsdIoError::Export("the document is too tall".into()))?;
    let scale = export_scale(project, doc);
    let mut builder = PsdBuilder::new(width, height);

    // Our array is bottom-to-top, which is exactly the order the codec pushes
    // layers in. Groups are written bottom-to-top with their children inside.
    let layers = project.board_layers.as_slice();
    for layer in top_layers(layers) {
        push_export_layer(
            &mut builder,
            project,
            layers,
            &layer,
            bitmaps,
            scale,
            doc,
            0,
            text,
        )?;
    }

    // The codec never composites: always hand it the merged image.
    let composite = composite_surface(project, bitmaps, text);
    builder.composite(PixelData::Rgba8(composite.data().to_vec()));

    builder
        .to_bytes()
        .map_err(|error| PsdIoError::Export(error.to_string()))
}

/// Writes one layer, or a group and its children, into `builder`.
#[expect(
    clippy::too_many_arguments,
    reason = "the export walk threads builder, project layers, the current layer, bitmaps, scale, document size and the text rasterizer, like the compositor's own draw functions"
)]
fn push_export_layer(
    builder: &mut PsdBuilder,
    project: &ImageProject,
    layers: &[CanvasPsLayer],
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    scale: Scale,
    doc: (usize, usize),
    depth: usize,
    text: Option<&dyn TextRasterizer>,
) -> Result<(), PsdIoError> {
    if layer.kind == CanvasPsLayerKind::Group && depth < MAX_GROUP_DEPTH {
        let mut group = GroupSpec::new(layer_name(layer));
        group.blend_mode = psd_blend(&layer.blend_mode);
        group.opacity = psd_opacity(layer.opacity);
        group.visible = !layer.hidden;
        group.open = true;
        builder.begin_group(group);
        for child in group_children(layers, layer) {
            push_export_layer(
                builder,
                project,
                layers,
                &child,
                bitmaps,
                scale,
                doc,
                depth + 1,
                text,
            )?;
        }
        builder
            .end_group()
            .map_err(|error| PsdIoError::Export(error.to_string()))?;
        return Ok(());
    }
    builder.push_layer(layer_spec(project, layer, bitmaps, scale, doc, text)?);
    Ok(())
}

/// Layer name; PSD requires a name and our documents may leave it empty.
fn layer_name(layer: &CanvasPsLayer) -> String {
    if layer.name.trim().is_empty() {
        "Layer".to_string()
    } else {
        layer.name.clone()
    }
}

/// A raster layer whose stored bitmap is what the compositor would draw: no
/// rotation, no mesh transform, no enabled layer style, and a bitmap present.
fn has_plain_bitmap(layer: &CanvasPsLayer, bitmaps: &HashMap<String, Surface>) -> bool {
    matches!(
        layer.kind,
        CanvasPsLayerKind::Pixel | CanvasPsLayerKind::Image
    ) && layer.adjustment.is_none()
        && layer.rotation == 0.0
        && layer.transform.is_none()
        && !layer
            .styles
            .as_ref()
            .is_some_and(|styles| styles.iter().any(|style| style.enabled))
        && layer
            .storage_key
            .as_ref()
            .is_some_and(|key| bitmaps.contains_key(key))
}

fn layer_spec(
    project: &ImageProject,
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    scale: Scale,
    doc: (usize, usize),
    text: Option<&dyn TextRasterizer>,
) -> Result<LayerSpec, PsdIoError> {
    let mut blend = psd_blend(&layer.blend_mode);
    let mut opacity = psd_opacity(layer.opacity);
    let (rect, pixels, mask) = if has_plain_bitmap(layer, bitmaps) {
        let window = layer_window(layer, scale, doc);
        let pixels = bitmap_layer(layer, bitmaps, &window)?;
        (
            window.rect()?,
            pixels,
            exported_mask(layer, bitmaps, &window)?,
        )
    } else if layer.kind == CanvasPsLayerKind::Adjustment {
        // An adjustment cannot be isolated: bake the pixels it changed and
        // composite that layer normally.
        let surface = adjustment_appearance(project, layer, bitmaps, text);
        let rect = Rect::from_xywh(
            0,
            0,
            u32::try_from(doc.0).unwrap_or(u32::MAX),
            u32::try_from(doc.1).unwrap_or(u32::MAX),
        );
        blend = BlendMode::Normal;
        opacity = 255;
        (rect, surface.data().to_vec(), None)
    } else {
        // Text, shape, rotation, transform, layer style: store what the
        // compositor draws for this layer alone (mask already applied).
        let surface = layer_appearance(project, layer, bitmaps, text);
        let rect = Rect::from_xywh(
            0,
            0,
            u32::try_from(doc.0).unwrap_or(u32::MAX),
            u32::try_from(doc.1).unwrap_or(u32::MAX),
        );
        (rect, surface.data().to_vec(), None)
    };
    let mut spec = LayerSpec::new(
        layer_name(layer),
        rect.left,
        rect.top,
        u32::try_from(rect.width()).unwrap_or(0),
        u32::try_from(rect.height()).unwrap_or(0),
        PixelData::Rgba8(pixels),
    );
    spec.blend_mode = blend;
    spec.opacity = opacity;
    spec.visible = !layer.hidden;
    spec.mask = mask;
    Ok(spec)
}

/// The document-space window a layer's own bitmap occupies at export scale:
/// the full layer box, clipped to the document (an empty window becomes 1x1 so
/// the PSD layer rect is never empty).
#[derive(Debug, Clone, Copy)]
struct LayerWindow {
    full_w: usize,
    full_h: usize,
    left: i64,
    top: i64,
    offset_x: i64,
    offset_y: i64,
    width: usize,
    height: usize,
}

impl LayerWindow {
    fn rect(&self) -> Result<Rect, PsdIoError> {
        let left = i32::try_from(self.left)
            .map_err(|_| PsdIoError::Export("the layer is outside the document".into()))?;
        let top = i32::try_from(self.top)
            .map_err(|_| PsdIoError::Export("the layer is outside the document".into()))?;
        Ok(Rect::from_xywh(
            left,
            top,
            u32::try_from(self.width).unwrap_or(u32::MAX),
            u32::try_from(self.height).unwrap_or(u32::MAX),
        ))
    }
}

fn layer_window(layer: &CanvasPsLayer, scale: Scale, doc: (usize, usize)) -> LayerWindow {
    let full_w = px_size(layer.width * scale.x);
    let full_h = px_size(layer.height * scale.y);
    let left = f2i((layer.x * scale.x).floor());
    let top = f2i((layer.y * scale.y).floor());
    let x0 = left.max(0);
    let y0 = top.max(0);
    let x1 = left.saturating_add(i64_of(full_w)).min(i64_of(doc.0));
    let y1 = top.saturating_add(i64_of(full_h)).min(i64_of(doc.1));
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    if width <= 0 || height <= 0 {
        return LayerWindow {
            full_w: 1,
            full_h: 1,
            left: 0,
            top: 0,
            offset_x: 0,
            offset_y: 0,
            width: 1,
            height: 1,
        };
    }
    LayerWindow {
        full_w,
        full_h,
        left: x0,
        top: y0,
        offset_x: x0 - left,
        offset_y: y0 - top,
        width: i2u(width),
        height: i2u(height),
    }
}

/// Resamples `source` to the full layer box and crops the window out of it.
fn crop_to_window(source: &Surface, window: &LayerWindow) -> Surface {
    let full = if source.width() == window.full_w && source.height() == window.full_h {
        source.clone()
    } else {
        source.resample_bilinear(window.full_w, window.full_h)
    };
    let mut cropped = Surface::new(window.width, window.height);
    for y in 0..window.height {
        for x in 0..window.width {
            let sx = window.offset_x + i64_of(x);
            let sy = window.offset_y + i64_of(y);
            cropped.set(i64_of(x), i64_of(y), full.get(sx, sy));
        }
    }
    cropped
}

/// The stored bitmap resampled to the layer box at export scale and cropped to
/// the document.
fn bitmap_layer(
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    window: &LayerWindow,
) -> Result<Vec<u8>, PsdIoError> {
    let bitmap = layer
        .storage_key
        .as_ref()
        .and_then(|key| bitmaps.get(key))
        .ok_or_else(|| PsdIoError::Export(format!("layer \"{}\" has no bitmap", layer.name)))?;
    Ok(crop_to_window(bitmap, window).data().to_vec())
}

/// The layer's user mask, as an 8-bit PSD mask aligned with the exported layer
/// rect (the same crop window as the layer's pixels).
fn exported_mask(
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    window: &LayerWindow,
) -> Result<Option<MaskSpec>, PsdIoError> {
    let Some(key) = layer.mask_storage_key.as_deref() else {
        return Ok(None);
    };
    let Some(mask) = bitmaps.get(key) else {
        return Ok(None);
    };
    let cropped = crop_to_window(mask, window);
    let mut data = Vec::with_capacity(window.width * window.height);
    for y in 0..window.height {
        for x in 0..window.width {
            data.push(cropped.get(i64_of(x), i64_of(y))[3]);
        }
    }
    Ok(Some(MaskSpec {
        rect: window.rect()?,
        data,
        default_color: 255,
        disabled: false,
    }))
}

/// The board with exactly one layer, no background and no channel hiding: the
/// compositor draws that layer's visible appearance in document coordinates.
fn layer_appearance(
    project: &ImageProject,
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    text: Option<&dyn TextRasterizer>,
) -> Surface {
    let mut board = SmartCanvasBoard::from(project);
    let mut only = layer.clone();
    only.hidden = false;
    // Opacity and blend mode live on the PSD layer record, so the appearance
    // must be drawn without them or they would apply twice.
    only.opacity = 1.0;
    only.blend_mode = "normal".to_string();
    board.board_layers = vec![only];
    board.board_background = "transparent".to_string();
    board.board_background_opacity = 1.0;
    board.board_channel_visibility = None;
    render_document_with_text(&board, bitmaps, text)
}

/// The pixels an adjustment layer changed: the difference between the document
/// rendered up to and including it and the document below it. Unchanged pixels
/// stay transparent, so compositing the result normally over the layers below
/// reproduces the adjusted document.
fn adjustment_appearance(
    project: &ImageProject,
    layer: &CanvasPsLayer,
    bitmaps: &HashMap<String, Surface>,
    text: Option<&dyn TextRasterizer>,
) -> Surface {
    let index = project
        .board_layers
        .iter()
        .position(|candidate| candidate.id == layer.id);
    let below = index.map_or_else(Vec::new, |at| project.board_layers[..at].to_vec());
    let up_to = index.map_or_else(Vec::new, |at| project.board_layers[..=at].to_vec());

    let mut board = SmartCanvasBoard::from(project);
    board.board_background = "transparent".to_string();
    board.board_background_opacity = 1.0;
    board.board_channel_visibility = None;
    let render = |layers: Vec<CanvasPsLayer>| -> Surface {
        let mut board = board.clone();
        board.board_layers = layers;
        render_document_with_text(&board, bitmaps, text)
    };
    let before = render(below);
    let after = render(up_to);
    let mut delta = Surface::new(after.width(), after.height());
    for y in 0..after.height() {
        for x in 0..after.width() {
            let (px, py) = (i64_of(x), i64_of(y));
            let target = after.get(px, py);
            if target == before.get(px, py) {
                continue;
            }
            // Opaque where the adjustment changed the pixel, so the normal
            // blend below reproduces the adjusted value exactly.
            delta.set(px, py, [target[0], target[1], target[2], 255]);
        }
    }
    delta
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Import PSD bytes as a new document plus its layer bitmaps.
///
/// Only 8-bit RGB/Grayscale documents are accepted; every other depth or color
/// mode is a clear error. The project's identity fields (`id`, `title`,
/// timestamps, `group_id`) are left empty so callers can stamp their own.
pub fn import_psd(bytes: &[u8]) -> Result<(ImageProject, HashMap<String, Surface>), PsdIoError> {
    let file = PsdFile::from_bytes(bytes).map_err(|error| PsdIoError::Import(error.to_string()))?;
    let header = &file.header;
    if header.depth != 8 {
        return Err(PsdIoError::Import(format!(
            "{}-bit documents are not supported (only 8-bit RGB or Grayscale)",
            header.depth
        )));
    }
    if !matches!(header.color_mode, ColorMode::Rgb | ColorMode::Grayscale) {
        return Err(PsdIoError::Import(format!(
            "{:?} color is not supported (only RGB or Grayscale)",
            header.color_mode
        )));
    }
    let width = usize_of(header.width);
    let height = usize_of(header.height);
    if width == 0 || height == 0 {
        return Err(PsdIoError::Import("the document has no pixels".into()));
    }
    let limit = MAX_PSD_DIMENSION;
    if width > limit || height > limit {
        return Err(PsdIoError::Import(format!(
            "{width}×{height} exceeds the studio's {limit} px limit"
        )));
    }
    let (resolution, ratio) = studio_size_for(width, height);

    let mut context = ImportContext::default();
    if !file.layers().is_empty() {
        import_tree(&file, &mut context)?;
    }
    if context.layers.is_empty() {
        // A flattened PSD (or one whose layer records were all empty): its
        // merged image becomes one layer.
        import_flattened(&file, &mut context)?;
    }

    let project = ImageProject {
        id: String::new(),
        title: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        group_id: String::new(),
        width: i2f(u2i(width)),
        height: i2f(u2i(height)),
        board_ratio: ratio.to_string(),
        board_resolution: resolution,
        board_background: "transparent".to_string(),
        board_background_opacity: 1.0,
        board_layers: context.layers,
        board_paths: None,
        board_alpha_channels: None,
        board_channel_visibility: None,
    };
    Ok((project, context.bitmaps))
}

/// Adds the merged composite as one pixel layer (the flattened-PSD path).
fn import_flattened(file: &PsdFile, context: &mut ImportContext) -> Result<(), PsdIoError> {
    let composite = file
        .composite_rgba8()
        .map_err(|error| PsdIoError::Import(error.to_string()))?;
    let width = usize_of(composite.width);
    let height = usize_of(composite.height);
    let surface = Surface::from_rgba(width, height, composite.data)
        .ok_or_else(|| PsdIoError::Import("the merged image is malformed".to_string()))?;
    context.push_raster(
        "Background",
        Bounds::new(0.0, 0.0, i2f(u2i(width)), i2f(u2i(height))),
        surface,
        true,
        1.0,
        "normal",
        None,
    );
    Ok(())
}

/// The studio ratio/resolution whose export size equals `width × height`
/// exactly, else the closest ratio at 1k. Importing keeps the PSD's pixel size
/// as the board size, so an exact match also re-exports at the same scale.
pub fn studio_size_for(width: usize, height: usize) -> (BoardResolution, &'static str) {
    let target = (i2f(u2i(width)), i2f(u2i(height)));
    for resolution in [
        BoardResolution::K1,
        BoardResolution::K2,
        BoardResolution::K4,
    ] {
        for ratio in PSD_RATIOS {
            if read_media_dimensions(resolution, ratio) == target {
                return (resolution, ratio);
            }
        }
    }
    let aspect = target.0 / target.1.max(1.0);
    let mut best = ("16:9", f64::INFINITY);
    for ratio in PSD_RATIOS {
        let (rw, rh) = read_media_dimensions(BoardResolution::K1, ratio);
        let candidate = rw / rh.max(1.0);
        let score = (aspect / candidate).ln().abs();
        if score < best.1 {
            best = (ratio, score);
        }
    }
    (BoardResolution::K1, best.0)
}

/// Accumulates imported layers and bitmaps; ids are minted per import so they
/// cannot collide with an open document's ids.
#[derive(Debug, Default)]
struct ImportContext {
    next_id: usize,
    layers: Vec<CanvasPsLayer>,
    bitmaps: HashMap<String, Surface>,
}

impl ImportContext {
    fn id(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        format!("psd-layer-{}", self.next_id)
    }

    /// Adds one pixel layer and returns the id it was given.
    #[expect(
        clippy::too_many_arguments,
        reason = "one flat constructor for an imported layer: name, box, pixels, visibility, opacity, blend and mask"
    )]
    fn push_raster(
        &mut self,
        name: &str,
        bounds: Bounds,
        pixels: Surface,
        visible: bool,
        opacity: f64,
        blend: &str,
        mask: Option<Surface>,
    ) -> String {
        let id = self.id();
        let key = format!("image:{id}");
        let mut layer =
            crate::board::base_layer(id.clone(), name, CanvasPsLayerKind::Pixel, bounds);
        layer.hidden = !visible;
        layer.opacity = opacity;
        layer.blend_mode = blend.to_string();
        layer.storage_key = Some(key.clone());
        self.bitmaps.insert(key, pixels);
        if let Some(mask) = mask {
            let mask_key = format!("image:{id}-mask");
            self.bitmaps.insert(mask_key.clone(), mask);
            layer.mask_storage_key = Some(mask_key);
        }
        self.layers.push(layer);
        id
    }
}

/// Walks the PSD layer tree bottom-to-top, iteratively (PSD nesting can be
/// thousands of levels deep, so recursion is not an option). Children are
/// appended before their group; groups deeper than [`MAX_GROUP_DEPTH`] are
/// flattened into their parent.
fn import_tree(file: &PsdFile, context: &mut ImportContext) -> Result<(), PsdIoError> {
    enum Action<'a> {
        Node { node: &'a LayerNode, depth: usize },
        CloseGroup { index: usize },
    }
    let tree = file.layer_tree();
    let mut actions: Vec<Action<'_>> = tree
        .iter()
        .rev()
        .map(|node| Action::Node { node, depth: 0 })
        .collect();
    let mut open: Vec<Vec<String>> = vec![Vec::new()];
    while let Some(action) = actions.pop() {
        match action {
            Action::Node { node, depth } => match node {
                LayerNode::Layer { index } => {
                    let id = import_layer(file, context, *index)?;
                    if let Some(parent) = open.last_mut() {
                        parent.extend(id);
                    }
                }
                LayerNode::Group {
                    index, children, ..
                } => {
                    if depth >= MAX_GROUP_DEPTH {
                        for child in children.iter().rev() {
                            actions.push(Action::Node {
                                node: child,
                                depth: depth + 1,
                            });
                        }
                    } else {
                        actions.push(Action::CloseGroup { index: *index });
                        for child in children.iter().rev() {
                            actions.push(Action::Node {
                                node: child,
                                depth: depth + 1,
                            });
                        }
                        open.push(Vec::new());
                    }
                }
            },
            Action::CloseGroup { index } => {
                let child_ids = open.pop().unwrap_or_default();
                let mut group_id = None;
                if let Some(layer) = file.layer(index) {
                    let record = layer.record;
                    let bounds = bounds_for(&child_ids, &context.layers);
                    let id = context.id();
                    let mut group = crate::board::base_layer(
                        id.clone(),
                        &layer.name(),
                        CanvasPsLayerKind::Group,
                        bounds,
                    );
                    group.hidden = !record.is_visible();
                    group.opacity = canvas_opacity(record.opacity);
                    group.blend_mode = canvas_blend(record.blend_mode).to_string();
                    group.children = Some(child_ids.clone());
                    context.layers.push(group);
                    group_id = Some(id);
                }
                // Without a folder record the children simply join the parent
                // level (a malformed but readable file).
                if let Some(parent) = open.last_mut() {
                    match group_id {
                        Some(id) => parent.push(id),
                        None => parent.extend(child_ids),
                    }
                }
            }
        }
    }
    Ok(())
}

/// The union box of a group's children (a group's own record has an empty
/// rect; our document derives the box from the children anyway).
fn bounds_for(child_ids: &[String], layers: &[CanvasPsLayer]) -> Bounds {
    let mut union: Option<Bounds> = None;
    for id in child_ids {
        let Some(layer) = layers.iter().find(|layer| layer.id == *id) else {
            continue;
        };
        let box_ = Bounds::new(layer.x, layer.y, layer.width, layer.height);
        union = Some(match union {
            Some(current) => Bounds::union(&[current, box_]).unwrap_or(current),
            None => box_,
        });
    }
    union.unwrap_or_else(|| Bounds::new(0.0, 0.0, 1.0, 1.0))
}

/// Imports one non-group record; `None` for a zero-size layer (nothing to
/// show) and an error when the record cannot be decoded.
fn import_layer(
    file: &PsdFile,
    context: &mut ImportContext,
    index: usize,
) -> Result<Option<String>, PsdIoError> {
    let Some(layer) = file.layer(index) else {
        return Ok(None);
    };
    let record = layer.record;
    let name = layer_name_of(layer.name());
    let pixels = layer
        .rgba8()
        .map_err(|error| PsdIoError::Import(format!("layer \"{name}\": {error}")))?;
    let width = usize_of(pixels.width);
    let height = usize_of(pixels.height);
    if width == 0 || height == 0 {
        return Ok(None);
    }
    if width.saturating_mul(height) > MAX_PSD_DIMENSION.saturating_mul(MAX_PSD_DIMENSION) {
        return Err(PsdIoError::Import(format!(
            "layer \"{name}\" is {width}×{height}, above the studio's {MAX_PSD_DIMENSION}×{MAX_PSD_DIMENSION} layer limit"
        )));
    }
    let surface = Surface::from_rgba(width, height, pixels.data)
        .ok_or_else(|| PsdIoError::Import(format!("layer \"{name}\" has a malformed buffer")))?;
    let bounds = Bounds::new(
        f64::from(pixels.left),
        f64::from(pixels.top),
        i2f(u2i(width)),
        i2f(u2i(height)),
    );
    let mask = import_mask(&layer, &name, width, height, pixels.left, pixels.top)?;
    Ok(Some(context.push_raster(
        &name,
        bounds,
        surface,
        record.is_visible(),
        canvas_opacity(record.opacity),
        canvas_blend(record.blend_mode),
        mask,
    )))
}

fn layer_name_of(name: String) -> String {
    if name.trim().is_empty() {
        "Layer".to_string()
    } else {
        name
    }
}

/// A layer's user mask shifted into the layer box, as the alpha channel of a
/// `Surface` (our compositor multiplies layer alpha by this mask's alpha).
fn import_mask(
    layer: &open_canvas_psd::Layer<'_>,
    name: &str,
    width: usize,
    height: usize,
    left: i32,
    top: i32,
) -> Result<Option<Surface>, PsdIoError> {
    let Some(mask) = layer.user_mask() else {
        return Ok(None);
    };
    let mask = mask.map_err(|error| PsdIoError::Import(format!("mask of \"{name}\": {error}")))?;
    let mask_w = usize_of(mask.width);
    let mask_h = usize_of(mask.height);
    let mut surface = Surface::new(width, height);
    surface.fill([255, 255, 255, mask.default_value]);
    let offset_x = coord(mask.left) - coord(left);
    let offset_y = coord(mask.top) - coord(top);
    for y in 0..mask_h {
        for x in 0..mask_w {
            let value = mask.data.get(y * mask_w + x).copied().unwrap_or(0);
            let dx = offset_x + i64_of(x);
            let dy = offset_y + i64_of(y);
            surface.set(dx, dy, [255, 255, 255, value]);
        }
    }
    Ok(Some(surface))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::CanvasPsPath;

    /// A 1024x1024 board (the 1:1 1k preset), so export scale is exactly 1 and
    /// layer bitmaps round-trip without resampling.
    fn project(layers: Vec<CanvasPsLayer>) -> ImageProject {
        ImageProject {
            id: "img-1".to_string(),
            title: "Board".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            group_id: String::new(),
            width: 1024.0,
            height: 1024.0,
            board_ratio: "1:1".to_string(),
            board_resolution: BoardResolution::K1,
            board_background: "transparent".to_string(),
            board_background_opacity: 1.0,
            board_layers: layers,
            board_paths: None,
            board_alpha_channels: None,
            board_channel_visibility: None,
        }
    }

    fn pixel_layer(
        id: &str,
        name: &str,
        x: f64,
        y: f64,
        opacity: f64,
        blend: &str,
        hidden: bool,
    ) -> CanvasPsLayer {
        let mut layer = crate::board::base_layer(
            id.to_string(),
            name,
            CanvasPsLayerKind::Pixel,
            Bounds::new(x, y, 32.0, 32.0),
        );
        layer.opacity = opacity;
        layer.blend_mode = blend.to_string();
        layer.hidden = hidden;
        layer.storage_key = Some(format!("image:{id}"));
        layer
    }

    /// Deterministic RGBA buffer with varying alpha, so straight-alpha and
    /// channel ordering both show up in a round-trip mismatch.
    fn pattern(seed: usize) -> Surface {
        let mut surface = Surface::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let r = u8::try_from((x * 7 + y * 3 + seed) % 256).unwrap_or(0);
                let g = u8::try_from((x * 11 + seed) % 256).unwrap_or(0);
                let b = u8::try_from((y * 5 + seed * 2) % 256).unwrap_or(0);
                let a = u8::try_from((x * 13 + y * 5 + seed) % 256).unwrap_or(0);
                surface.set(i64_of(x), i64_of(y), [r, g, b, a]);
            }
        }
        surface
    }

    fn opaque_pattern(seed: usize) -> Surface {
        let mut surface = pattern(seed);
        for y in 0..32 {
            for x in 0..32 {
                let pixel = surface.get(i64_of(x), i64_of(y));
                surface.set(i64_of(x), i64_of(y), [pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        surface
    }

    fn mask_pattern() -> Surface {
        let mut surface = Surface::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let a = u8::try_from((x * 8) % 256).unwrap_or(0);
                surface.set(i64_of(x), i64_of(y), [255, 255, 255, a]);
            }
        }
        surface
    }

    fn layer_bitmap(layer: &CanvasPsLayer, bitmaps: &HashMap<String, Surface>) -> Option<Surface> {
        layer
            .storage_key
            .as_ref()
            .and_then(|key| bitmaps.get(key))
            .cloned()
    }

    /// Stand-in for the app's glyph rasterizer: paints the layer box solid, so
    /// the export path for text layers is exercised without fonts.
    struct SolidText;

    impl TextRasterizer for SolidText {
        fn rasterize(
            &self,
            _layer: &CanvasPsLayer,
            _paths: &[CanvasPsPath],
            width: usize,
            height: usize,
            _scale: f64,
        ) -> Option<Surface> {
            let mut surface = Surface::new(width, height);
            surface.fill([255, 0, 0, 255]);
            Some(surface)
        }
    }

    #[test]
    fn three_layer_project_round_trips_names_visibility_opacity_blend_and_pixels(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bottom = pixel_layer("l1", "Base", 0.0, 0.0, 1.0, "normal", false);
        let mut middle = pixel_layer("l2", "Paint", 100.0, 64.0, 0.5, "multiply", false);
        middle.mask_storage_key = Some("image:l2-mask".to_string());
        let top = pixel_layer("l3", "Hidden", 500.0, 500.0, 0.44, "screen", true);
        let project = project(vec![bottom, middle, top]);
        let mut bitmaps = HashMap::new();
        bitmaps.insert("image:l1".to_string(), pattern(1));
        bitmaps.insert("image:l2".to_string(), pattern(2));
        bitmaps.insert("image:l2-mask".to_string(), mask_pattern());
        bitmaps.insert("image:l3".to_string(), pattern(3));

        let bytes = export_project(&project, &bitmaps).map_err(std::io::Error::other)?;
        // The exported bytes are a readable PSD.
        assert_eq!(&bytes[..4], b"8BPS");
        let (imported, imported_bitmaps) = import_psd(&bytes).map_err(std::io::Error::other)?;
        assert_eq!(imported.board_layers.len(), 3);

        let expected = [
            ("Base", false, 1.0, "normal", 0.0, 0.0),
            ("Paint", false, 0.5, "multiply", 100.0, 64.0),
            ("Hidden", true, 0.44, "screen", 500.0, 500.0),
        ];
        let sources = [pattern(1), pattern(2), pattern(3)];
        for ((layer, (name, hidden, opacity, blend, x, y)), source) in
            imported.board_layers.iter().zip(expected).zip(sources)
        {
            assert_eq!(layer.name, name);
            assert_eq!(layer.hidden, hidden);
            assert!((layer.opacity - opacity).abs() < 1e-9, "{}", layer.opacity);
            assert_eq!(layer.blend_mode, blend);
            assert!((layer.x - x).abs() < 1e-9);
            assert!((layer.y - y).abs() < 1e-9);
            let bitmap = layer_bitmap(layer, &imported_bitmaps).ok_or("imported bitmap")?;
            assert_eq!((bitmap.width(), bitmap.height()), (32, 32));
            assert_eq!(bitmap.data(), source.data(), "{name} pixels");
        }

        // The mask the project stored comes back on the same layer.
        let paint = imported.board_layers.get(1).ok_or("paint layer")?;
        let mask_key = paint.mask_storage_key.as_ref().ok_or("mask key")?;
        let mask = imported_bitmaps.get(mask_key).ok_or("mask bitmap")?;
        assert_eq!(mask.data(), mask_pattern().data());
        Ok(())
    }

    #[test]
    fn text_layers_export_through_the_compositor_as_flat_pixels(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut text = crate::board::base_layer(
            "t1".to_string(),
            "Title",
            CanvasPsLayerKind::Text,
            Bounds::new(100.0, 100.0, 600.0, 200.0),
        );
        text.text = Some("Hello".to_string());
        text.font_size = Some(96.0);
        text.color = Some("#ff0000".to_string());
        let project = project(vec![text]);

        let bytes = export_project_with_text(&project, &HashMap::new(), Some(&SolidText))
            .map_err(std::io::Error::other)?;
        let file = PsdFile::from_bytes(&bytes)?;
        let layer = file.layer(0).ok_or("text layer")?;
        assert_eq!(layer.name(), "Title");
        let pixels = layer.rgba8()?;
        assert_eq!(
            (pixels.width, pixels.height),
            (1024, 1024),
            "flattened layers cover the document"
        );
        let visible = pixels
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 0)
            .count();
        assert!(visible > 0, "the text appearance is not empty");

        // Import keeps the flattened appearance as one pixel layer.
        let (imported, imported_bitmaps) = import_psd(&bytes).map_err(std::io::Error::other)?;
        let layer = imported.board_layers.first().ok_or("layer")?;
        assert_eq!(layer.kind, CanvasPsLayerKind::Pixel);
        let bitmap = layer_bitmap(layer, &imported_bitmaps).ok_or("bitmap")?;
        assert_eq!(bitmap.data(), pixels.data);
        Ok(())
    }

    #[test]
    fn malformed_and_unsupported_psd_files_report_clear_errors(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let error = import_psd(b"not a photoshop file")
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.starts_with("Open PSD failed:"), "{error}");

        // 16-bit documents are out of scope, not a crash.
        let mut deep = PsdBuilder::new(2, 2).depth(16);
        deep.push_layer(LayerSpec::new(
            "L",
            0,
            0,
            2,
            2,
            PixelData::Rgba16(vec![0x8080; 16]),
        ));
        deep.composite(PixelData::Rgba16(vec![0x8080; 16]));
        let bytes = deep.to_bytes()?;
        let error = import_psd(&bytes)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("16-bit"), "{error}");

        // CMYK is rejected as well.
        let mut cmyk = PsdBuilder::new(2, 2).color_mode(ColorMode::Cmyk);
        cmyk.push_layer(LayerSpec::new(
            "L",
            0,
            0,
            2,
            2,
            PixelData::Cmyka8(vec![0; 20]),
        ));
        cmyk.composite(PixelData::Cmyka8(vec![0; 20]));
        let bytes = cmyk.to_bytes()?;
        let error = import_psd(&bytes)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("Cmyk"), "{error}");
        Ok(())
    }

    #[test]
    fn exported_composite_matches_the_compositor() -> Result<(), Box<dyn std::error::Error>> {
        let mut bottom = crate::board::base_layer(
            "l1".to_string(),
            "Base",
            CanvasPsLayerKind::Pixel,
            Bounds::new(0.0, 0.0, 1024.0, 1024.0),
        );
        bottom.storage_key = Some("image:l1".to_string());
        let mut top = pixel_layer("l2", "Paint", 100.0, 64.0, 0.75, "screen", false);
        top.opacity = 0.75;
        let project = project(vec![bottom, top]);
        let mut bitmaps = HashMap::new();
        let mut base = Surface::new(1024, 1024);
        for y in 0..1024 {
            for x in 0..1024 {
                let r = u8::try_from((x / 4) % 256).unwrap_or(0);
                let g = u8::try_from((y / 4) % 256).unwrap_or(0);
                let b = u8::try_from((x + y) % 256).unwrap_or(0);
                base.set(i64_of(x), i64_of(y), [r, g, b, 255]);
            }
        }
        bitmaps.insert("image:l1".to_string(), base);
        bitmaps.insert("image:l2".to_string(), opaque_pattern(9));

        let bytes = export_project(&project, &bitmaps).map_err(std::io::Error::other)?;
        let file = PsdFile::from_bytes(&bytes)?;
        let merged = file.composite_rgba8()?;
        let rendered = composite_surface(&project, &bitmaps, None);
        assert_eq!(
            (usize_of(merged.width), usize_of(merged.height)),
            (rendered.width(), rendered.height())
        );
        assert_eq!(merged.data, rendered.data());
        // Opacity survives on the layer record itself.
        let paint = file.layer(1).ok_or("paint layer")?;
        assert_eq!(paint.record.opacity, clamp_u8(0.75 * 255.0));
        Ok(())
    }

    #[test]
    fn groups_export_as_psd_groups_and_import_back() -> Result<(), Box<dyn std::error::Error>> {
        let first = pixel_layer("c1", "Child A", 10.0, 10.0, 1.0, "normal", false);
        let second = pixel_layer("c2", "Child B", 60.0, 60.0, 1.0, "normal", false);
        let mut group = crate::board::base_layer(
            "g1".to_string(),
            "Folder",
            CanvasPsLayerKind::Group,
            Bounds::new(0.0, 0.0, 1024.0, 1024.0),
        );
        group.children = Some(vec!["c1".to_string(), "c2".to_string()]);
        group.blend_mode = "normal".to_string();
        let project = project(vec![first, second, group]);
        let mut bitmaps = HashMap::new();
        bitmaps.insert("image:c1".to_string(), pattern(4));
        bitmaps.insert("image:c2".to_string(), pattern(5));

        let bytes = export_project(&project, &bitmaps).map_err(std::io::Error::other)?;
        let file = PsdFile::from_bytes(&bytes)?;
        assert_eq!(file.layer_tree().len(), 1, "one root group");
        assert!(file.layer_tree().first().is_some_and(LayerNode::is_group));

        let (imported, _) = import_psd(&bytes).map_err(std::io::Error::other)?;
        let group = imported
            .board_layers
            .iter()
            .find(|layer| layer.kind == CanvasPsLayerKind::Group)
            .ok_or("imported group")?;
        assert_eq!(group.name, "Folder");
        let children = group.children.as_ref().ok_or("children")?;
        assert_eq!(children.len(), 2);
        for id in children {
            assert!(
                imported.board_layers.iter().any(|layer| &layer.id == id),
                "group child {id} exists"
            );
        }
        Ok(())
    }
}
