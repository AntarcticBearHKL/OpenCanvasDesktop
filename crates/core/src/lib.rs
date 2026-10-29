//! `open-canvas-core` — the UI-agnostic domain model for the OpenCanvas desktop port.
//!
//! This crate has no GUI, network, or persistence dependency: it is the pure data
//! model (nodes, connections, geometry, history, viewport, project document) shared
//! by whatever UI framework the app later adopts.
//!
//! Module map (mirroring the web source):
//! - [`node`] — node types, metadata, connections, selection (`web/src/types/canvas.ts`)
//! - [`constants`] — default sizes and initial metadata (`web/src/constant/canvas.ts`)
//! - [`geometry`] — bounds, snapping, size rules (`canvas-node-geometry.ts`, `canvas-node-size.ts`)
//! - [`connections`] — relations, bezier geometry, stroke crossing, normalization (`canvas-connections.ts`)
//! - [`history`] — snapshot undo/redo (`use-canvas-history.ts`)
//! - [`media_size`] — media scale/ratio presets and labels (`web/src/lib/media-size.ts`)
//! - [`references`] — composer `@`-mention resource references (`canvas-resource-references.ts`)
//! - [`document`] — [`document::CanvasProject`] and the `"open-canvas"` v3 export envelope

pub mod alignment;
pub mod archive;
mod archive_assets;
mod archive_base64;
mod archive_inline;
mod archive_paths;
mod archive_read;
mod archive_wire;
mod archive_write;
pub mod connections;
pub mod constants;
pub mod document;
pub mod geometry;
pub mod history;
pub mod media_size;
pub mod node;
pub mod references;

pub use alignment::{align_nodes, AlignAxis};
pub use archive::{
    archive_ref, collect_export_assets, pack_export, pack_export_with_limits,
    restore_export_assets, unpack_export, unpack_export_with_limits, ArchiveError, ArchiveLimits,
    Blob, CollectedExport, ARCHIVE_REF_PREFIX, PROJECTS_MANIFEST,
};
pub use connections::{
    connection_crosses_stroke, connection_geometry, connection_polyline, connection_relation,
    has_connection, infer_connection_relation, is_prompt_connection_allowed, normalize_connection,
    source_relation, ConnectionGeometry, NormalizedConnection, EDGE_SAMPLE_SEGMENTS,
};
pub use constants::{
    get_node_spec, node_specs, NodeSpec, DEFAULT_VIDEO_GENERATION_MODEL, FALLBACK_NODE_HEIGHT,
    FALLBACK_NODE_WIDTH,
};
pub use document::{
    deserialize_export, deserialize_library, from_json, to_json, CanvasExportAsset,
    CanvasExportFile, CanvasGroup, CanvasProject, CanvasProjectExportItem, DocumentError,
    LibraryIndex, ProjectIndexEntry, ViewportTransform, EXPORT_APP, EXPORT_VERSION, LIBRARY_APP,
    LIBRARY_VERSION,
};
pub use geometry::{
    connection_target_anchor, fit_node_size, is_node_hidden, is_node_locked, node_bounds,
    node_center_inside, node_size_from_ratio, snap_drag_to_guides, snap_threshold, Bounds,
    MovedNode, SnapGuides, SnapOptions, SnapResult, CANVAS_GRID_SIZE, FIT_NODE_MAX_SIZE,
    GUIDE_SNAP_THRESHOLD,
};
pub use history::{History, Snapshot, HISTORY_CAP, HISTORY_DEBOUNCE_MS};
pub use media_size::{
    clamp_video_seconds, compute_media_size, compute_video_size, image_quality_label,
    image_size_label, infer_media_ratio, infer_media_scale, infer_video_ratio, parse_aspect_ratio,
    parse_pixel_size, parse_video_resolution, read_media_dimensions, reasoning_label,
    video_resolution_label, video_size_label, MEDIA_RATIO_OPTIONS, MEDIA_SCALE_OPTIONS,
    VIDEO_RATIO_OPTIONS, VIDEO_RESOLUTION_OPTIONS, VIDEO_SECONDS_MAX, VIDEO_SECONDS_MIN,
};
pub use node::{
    apply_node_config_patch, create_canvas_node, create_node_id, merge_metadata, CanvasAssetSource,
    CanvasAssetStudioProjects, CanvasConnection, CanvasFontWeight, CanvasGenerationMode,
    CanvasImageGenerationType, CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata,
    CanvasNodeStatus, CanvasNodeText, CanvasNodeType, CanvasReasoningEffort, CanvasScope,
    CanvasScopeRole, CanvasTextAlign, CanvasVideoMode, CanvasVideoSlots, CanvasVideoTaskProvider,
    CanvasWorkspace, ConnectionHandle, ConnectionHandleType, Position, SelectionBox,
};
pub use references::{
    chip_span_after, chip_span_before, chip_spans, filter_candidates, mention_query,
    mention_references, resource_kind, resource_text, splice, ResourceKind, ResourceReference,
};
