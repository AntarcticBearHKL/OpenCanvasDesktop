//! `open-canvas-ps` — the UI-agnostic Image Studio domain for the OpenCanvas
//! desktop port. It mirrors `web/src/lib/canvas/smart-canvas.ts`,
//! `web/src/lib/canvas/ps/*.ts`, `ps-adjustments.ts`, `ps-layer-styles.ts`,
//! `ps-text.ts`, `blend-modes.ts`, `ps-color.ts` and the PS types in
//! `web/src/types/canvas.ts` + `web/src/stores/use-image-store.ts`.
//!
//! Everything is pure Rust on an RGBA8 `Surface` (the Rust match for Canvas2D's
//! `ImageData`): `doc` is the serde document, `compositor` is the board
//! renderer, `blend`/`composite` are the pixel formulas, and the rest are the
//! deterministic math modules (ops, color, transform, path, selection,
//! adjustments, filters, layer styles, text). Only `png` touches I/O.
//!
//! Environment-specific concerns stay out: bitmaps reach the compositor through
//! a `HashMap<String, Surface>` keyed by storage key, and text glyphs through a
//! host [`TextRasterizer`] (the pure layout/warp math lives in `text`).

pub mod adjustments;
pub mod algo_bridge;
pub mod analysis;
pub mod angle;
pub mod blend;
pub mod board;
pub mod color;
pub mod composite;
pub mod compositor;
pub mod doc;
pub mod error;
pub mod filters;
pub mod geom;
pub mod image_ops;
pub mod layer_ops;
pub mod layer_styles;
pub mod ocr;
pub mod path;
pub mod path_ops;
pub mod png;
pub mod psd_io;
pub mod segmentation;
pub mod selection;
pub mod surface;
pub mod text;
pub mod transform;
pub mod upscale;

pub use algo_bridge::{
    apply_filter, available as available_algo_filters, entry as algo_filter_entry,
    gallery_categories, gallery_category, AlgoFilter, FilterEntry, ParamKind, ParamSpec,
    ParamValue,
};
pub use analysis::{
    crop_box_for_aspect, extract_palette, metadata_rows, parse_aspect, perceptual_hash,
    read_image_metadata, resolve_smart_crop_area, suggest_smart_crop, CropRect, ImageMetadata,
    PaletteColor, DEFAULT_CROP_RATIOS, DEFAULT_PALETTE_COUNT, PALETTE_LEVELS, PERCEPTUAL_HASH_SIZE,
};
pub use angle::{
    angle_variant_plan, build_angle_label, build_angle_prompt, AngleParams, AngleVariant,
    AngleVariantPlan, ANGLE_VARIANT_MAX, ANGLE_VARIANT_MIN,
};
pub use blend::{
    clamp_layer_opacity, resolve_blend_mode, BlendMode, BLEND_MODE_IDS, DEFAULT_BLEND_MODE,
};
pub use board::{
    arrange_ps_layers, base_layer, create_ps_adjustment_layer, create_ps_image_layer,
    create_ps_pixel_layer, create_ps_shape_layer, create_ps_text_layer, document_needs_raster,
    layer_box, read_media_dimensions, shape_path_data, smart_canvas_background, smart_canvas_ratio,
    text_render_style, top_layers, BoardLayoutTemplate,
};
pub use composite::{blend_pixel, draw_surface};
pub use compositor::{render_document, render_document_with_text, TextRasterizer};
pub use doc::{
    image_presets, CanvasPsAdjustmentType, CanvasPsAlphaChannel, CanvasPsLayer, CanvasPsLayerKind,
    CanvasPsLayerStyle, CanvasPsLayerStyleType, CanvasPsParamValue, CanvasPsPath,
    CanvasPsPathAnchor, CanvasPsShapeKind, CanvasPsTextCase, CanvasPsTextParagraph,
    CanvasPsTextWarp, CanvasPsTextWarpStyle, CanvasPsTransform, ChannelVisibility, IdGen,
    ImageGroup, ImagePreset, ImageProject, PsParams, SmartCanvasBoard,
};
pub use error::PsError;
pub use geom::{Bounds, Point};
pub use image_ops::{
    ps_anchor_offset, ps_offset_layers, ps_rotate_layers, ps_scale_layers, ps_trim_box,
};
pub use layer_ops::{
    add_ps_layer, add_ps_layer_above, duplicate_ps_layer, group_ps_layers, move_ps_layer_step,
    move_ps_layer_to, patch_ps_layer, ps_bake_mask, rasterize_ps_layer, remove_ps_layer,
    resize_ps_layer, rotate_ps_layer, scale_ps_layer, translate_ps_layer, ungroup_ps_layer,
    LayerPosition, PsResizeCorner,
};
pub use layer_styles::{create_ps_layer_style, ps_compose_layer_styles, ps_style_padding};
pub use ocr::{
    build_ocr_request, build_ocr_request_default, normalise_ocr_text, ocr_prompt,
    parse_ocr_response, OcrContentPart, OcrImageRef, OcrImageUrl, OcrLayoutBlock, OcrMessage,
    OcrRequest, OcrResult, OCR_PROMPT,
};
pub use path::{create_path, path_anchor, path_bounds, path_data, polyline, PathSampler};
pub use path_ops::paint_path;
pub use png::{decode_rgba, encode_png};
pub use psd_io::{
    export_project, export_project_with_text, import_psd, studio_size_for, PsdIoError,
    MAX_PSD_DIMENSION,
};
pub use segmentation::{
    binarize_mask_logits, build_decoder_inputs, clamp_sam_score, pick_best_mask_index,
    postprocess_mask, prepare_segment_input, prepare_segment_input_with_size, sam_model_point,
    sam_resize_scale, segment_image, SamPoint, SegmentDecoderInputs, SegmentModelInput,
    SegmentModelOutput, SegmentOptions, SegmentationResult, Segmenter, SEGMENT_INPUT_SIZE,
    SEGMENT_MASK_INPUT_SIZE, SEGMENT_PAD_COLOR,
};
pub use selection::{
    selection_all, selection_bounds, selection_clear, selection_combine, selection_create,
    selection_edges, selection_feather, selection_invert, selection_polygon, selection_quick,
    selection_rect, selection_wand, SelectionMode,
};
pub use surface::{clamp_u8, hex_to_rgba, px_size, rgba_to_hex, Rgba, Surface};
pub use transform::{
    apply_numeric_transform, clear_transform, default_transform, identity_mesh, identity_quad,
    is_identity_transform, move_transform_handle, transform_mesh, warp_mesh, PsNumericTransform,
    PsTransformMode,
};
pub use upscale::{
    crop_surface, plan_upscale_tiles, resample_surface, resolve_upscale_size, stitch_upscale_tiles,
    upscale_ai_prompt, TileRect, UpscaleAlgorithm, UpscaleTile, UpscaleTilePlan,
    DEFAULT_UPSCALE_TILE, MAX_UPSCALE_LONG_EDGE, MIN_UPSCALE_LONG_EDGE,
    RESOLUTION_AI_DEFAULT_PROMPT, RESOLUTION_AI_FACTOR_PROMPT,
};
