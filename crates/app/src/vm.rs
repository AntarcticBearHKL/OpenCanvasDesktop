//! Slint view rows: the `NodeItem` model and the per-kind display mapping.

use std::collections::BTreeSet;

use slint::{ModelRc, VecModel};

use open_canvas_core::document::LibraryIndex;
use open_canvas_core::node::{CanvasConnection, CanvasNodeData, CanvasNodeType};
use open_canvas_plugins::view::BackgroundMode;

use crate::canvas_content;
use crate::inline_edit::EditState;
use crate::media::MediaCache;
use crate::panels::Tick;
use crate::plugin_content::PluginCellKind;
use crate::plugin_host::PluginHost;
use crate::plugin_views;

/// Image-stack cards rendered per node: the cover plus three backing photos
/// (`ImageStackNodeContent` renders at most the cover + 4 backing cards; the
/// contact-sheet cap keeps the heap legible on a card).
pub const STACK_THUMBS: usize = 4;
/// Assets rows listed on a card before the footer count reports the remainder.
pub const ASSET_ROWS: usize = 6;

/// Node lookup shared by one row-build pass: `id -> node`, built once so the
/// per-row wiring lookups (`can_generate`, reference images, slot titles) are
/// O(1) instead of a scan of the whole document per row.
#[derive(Default)]
pub struct NodeIndex<'a> {
    by_id: std::collections::HashMap<&'a str, &'a CanvasNodeData>,
}

impl<'a> NodeIndex<'a> {
    pub fn new(nodes: &'a [CanvasNodeData]) -> Self {
        let mut by_id = std::collections::HashMap::with_capacity(nodes.len());
        // First occurrence wins, like the `iter().find()` this replaces.
        for node in nodes {
            by_id.entry(node.id.as_str()).or_insert(node);
        }
        Self { by_id }
    }

    pub fn get(&self, id: &str) -> Option<&'a CanvasNodeData> {
        self.by_id.get(id).copied()
    }
}

/// Live app state `node_row` reads beside the node itself. Bundled so callers
/// destructure `App`'s fields once instead of threading eight parameters.
pub struct RowContext<'a> {
    pub nodes: &'a [CanvasNodeData],
    pub connections: &'a [CanvasConnection],
    /// `nodes` by id, so a row never scans the document to find a wire peer.
    pub index: &'a NodeIndex<'a>,
    pub library: &'a LibraryIndex,
    pub media: &'a mut MediaCache,
    pub edit: &'a EditState,
    /// Related-node set of the current focus (`relatedHighlight`). Selection
    /// draws an outline on these; it never fades anything else.
    pub related: &'a BTreeSet<String>,
    /// Per-node reference-slot count of a video-prompt card's `reference` mode
    /// (the editable N); absent means [`REFERENCE_SLOT_DEFAULT`].
    pub reference_slots: &'a std::collections::HashMap<String, usize>,
    /// Node id currently highlighted as a drop target.
    pub drop_target: Option<&'a str>,
    /// Slot zone under the pointer during a slot drop (`firstFrame` | ...).
    pub drop_slot: Option<&'a str>,
    /// Inline prompt/generation interaction state (editor session, popover,
    /// running node).
    pub panel: &'a crate::node_ui::Panel,
    /// Bound-asset-folder scans, cached per node (`asset_folder::FolderCache`).
    pub folders: &'a mut crate::asset_folder::FolderCache,
    /// Per assets node: the expanded folder paths (relative, slash-separated).
    pub asset_expanded: &'a std::collections::HashMap<String, BTreeSet<String>>,
    /// Smart Canvas card previews, keyed by the linked Image Studio project id.
    /// Absent means the project is missing (or paints nothing), which renders
    /// the card's empty placeholder.
    pub smart_previews: &'a std::collections::HashMap<String, slint::Image>,
}

/// Count phrase with the web's singular/plural shape ("1 image", "3 images").
pub fn count_label(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Non-negative count -> i32 for Slint model fields.
pub fn count_i32(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// One `[NodeItem]` model row.
#[expect(
    clippy::too_many_lines,
    reason = "a flat field map of the per-kind content layer; splitting it would only scatter the view contract"
)]
pub fn node_row(
    node: &CanvasNodeData,
    selected: bool,
    enter: f32,
    plugin: &mut PluginHost,
    ctx: &mut RowContext<'_>,
) -> crate::NodeItem {
    // Only a plugin card reads its upstream nodes; a built-in card's cell is
    // built from the node alone, so collecting (and cloning) the upstream
    // nodes for every built-in row on the board was pure waste.
    let upstream = if node.node_type.is_builtin() {
        Vec::new()
    } else {
        crate::plugin_host::upstream_nodes(ctx.nodes, ctx.connections, &node.id)
    };
    let cell = plugin.cell(node, &upstream);
    let view = plugin.view_for(node);
    let meta = node.metadata.as_ref();
    let kind = node.node_type.clone();
    let status = canvas_content::status_id(meta.and_then(|m| m.status));
    let status_label = canvas_content::status_label(meta.and_then(|m| m.status));
    let error_text = meta
        .and_then(|m| m.error_details.clone())
        .unwrap_or_default();
    let thumbnail = meta.and_then(|m| m.thumbnail.as_deref());
    let content = meta.and_then(|m| m.content.as_deref()).unwrap_or_default();
    // Handles are asymmetric: an Image keeps its output handle but has no
    // input, and an Image Stack / Smart Canvas has neither, whatever a plugin
    // definition declares.
    let accepts_source = open_canvas_core::connections::accepts_source_connections(&node.node_type);
    let accepts_target = open_canvas_core::connections::accepts_target_connections(&node.node_type);
    let content_image = match kind {
        CanvasNodeType::Image | CanvasNodeType::Video if status.is_empty() => ctx
            .media
            .image_or_thumbnail(thumbnail, content)
            .unwrap_or_default(),
        CanvasNodeType::ImageAttempt => meta
            .and_then(|m| {
                let images = m.images.as_ref()?;
                let primary = m.primary_image_id.as_deref();
                images
                    .iter()
                    .find(|image| primary.is_some_and(|id| id == image.id))
                    .or_else(|| images.first())
            })
            .map(|image| {
                ctx.media
                    .image_or_thumbnail(image.thumbnail.as_deref(), &image.content)
                    .unwrap_or_default()
            })
            .unwrap_or_default(),
        CanvasNodeType::SmartCanvas if status.is_empty() => meta
            .and_then(|m| m.smart_canvas_project_id.as_deref())
            .and_then(|project_id| ctx.smart_previews.get(project_id).cloned())
            .unwrap_or_default(),
        _ => slint::Image::default(),
    };
    let content_empty = match kind {
        CanvasNodeType::ImageAttempt => content_image.size().width == 0,
        CanvasNodeType::Image | CanvasNodeType::Video | CanvasNodeType::SmartCanvas
            if status.is_empty() =>
        {
            content_image.size().width == 0
        }
        _ => true,
    };
    let sheet = if kind == CanvasNodeType::ImageStack && status.is_empty() {
        canvas_content::stack_sheet(node, ctx.media)
    } else {
        canvas_content::StackSheet::default()
    };
    let slots = if kind == CanvasNodeType::VideoPrompt {
        slot_cells(node, ctx.index)
    } else {
        SlotCells::default()
    };
    let mut assets = if kind == CanvasNodeType::Assets {
        canvas_content::asset_rows(node, ctx.library)
    } else {
        Vec::new()
    };
    let (asset_tree, asset_bound, asset_folder, asset_error, asset_files) =
        if kind == CanvasNodeType::Assets {
            canvas_content::asset_folder_view(node, ctx.folders, ctx.asset_expanded.get(&node.id))
        } else {
            (Vec::new(), false, String::new(), String::new(), 0)
        };
    let asset_count = if asset_bound {
        asset_files
    } else {
        assets.len()
    };
    assets.truncate(ASSET_ROWS);
    let text_count = meta.and_then(|m| m.texts.as_ref()).map_or(0, Vec::len);
    let media_info = kind == CanvasNodeType::Image && status.is_empty() && !content.is_empty();
    let editing = ctx.edit.is_editing_body(&node.id);
    let editing_title = ctx.edit.is_editing_title(&node.id);
    let related = ctx.related.contains(&node.id);
    let generation = matches!(
        kind,
        CanvasNodeType::ImageAttempt
            | CanvasNodeType::ImageGeneration
            | CanvasNodeType::VideoGeneration
            | CanvasNodeType::Text
    );
    let prompt_kind = matches!(&kind, CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt);
    let mode = crate::node_ui::node_mode(node);
    // The card shows the fixed model for its capability; the old per-node pick
    // is gone and `metadata.model` no longer drives this label.
    let model = if kind == CanvasNodeType::ImageAttempt {
        meta.and_then(|m| m.model.clone())
            .unwrap_or_else(|| crate::node_ui::default_model(mode).to_string())
    } else {
        crate::node_ui::default_model(mode).to_string()
    };
    let required = if kind == CanvasNodeType::VideoGeneration {
        CanvasNodeType::VideoPrompt
    } else {
        CanvasNodeType::Prompt
    };
    let can_generate = if kind == CanvasNodeType::ImageAttempt {
        meta.and_then(|m| m.prompt.as_ref())
            .is_some_and(|prompt| !prompt.trim().is_empty())
            && status.is_empty()
    } else {
        ctx.connections.iter().any(|connection| {
            connection.to_node_id == node.id
                && ctx
                    .index
                    .get(&connection.from_node_id)
                    .is_some_and(|source| source.node_type == required)
        })
    };
    let (prompt_caret, prompt_caret_seq) = if ctx.panel.node.as_deref() == Some(node.id.as_str()) {
        (
            i32::try_from(ctx.panel.caret).unwrap_or(i32::MAX),
            ctx.panel.caret_seq,
        )
    } else {
        (-1, 0)
    };
    let ref_images: Vec<crate::RefImage> = if prompt_kind {
        ctx.connections
            .iter()
            .filter(|connection| connection.to_node_id == node.id)
            .filter_map(|connection| ctx.index.get(&connection.from_node_id))
            .filter(|source| source.node_type == CanvasNodeType::Image)
            .map(|source| {
                let meta = source.metadata.as_ref();
                let content = meta.and_then(|m| m.content.as_deref()).unwrap_or_default();
                let thumb = ctx
                    .media
                    .image_or_thumbnail(meta.and_then(|m| m.thumbnail.as_deref()), content)
                    .unwrap_or_default();
                crate::RefImage {
                    id: source.id.as_str().into(),
                    title: source.title.as_str().into(),
                    thumb,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let batch: Vec<crate::TextOptionRow> = meta
        .and_then(|m| m.texts.as_ref())
        .map(|texts| {
            let primary = meta
                .and_then(|m| m.primary_text_id.as_deref())
                .or_else(|| texts.first().map(|text| text.id.as_str()));
            texts
                .iter()
                .map(|text| crate::TextOptionRow {
                    id: text.id.as_str().into(),
                    content: text.content.as_str().into(),
                    primary: primary == Some(text.id.as_str()),
                    can_take_out: texts.len() > 1,
                })
                .collect()
        })
        .unwrap_or_default();
    let open_popover = ctx
        .panel
        .popover
        .as_ref()
        .filter(|open| open.node == node.id)
        .map_or_else(String::new, |open| open.id.as_str().to_string());
    crate::NodeItem {
        id: node.id.as_str().into(),
        kind: node.node_type.as_str().into(),
        title: node.title.as_str().into(),
        body: node_body(node).into(),
        body_size: len(node
            .metadata
            .as_ref()
            .and_then(|m| m.font_size)
            .unwrap_or(14.0)),
        body_color: node
            .metadata
            .as_ref()
            .and_then(|meta| meta.text_color.as_deref())
            .and_then(crate::plugin_views::parse_hex_color)
            .unwrap_or_default(),
        body_colored: node
            .metadata
            .as_ref()
            .and_then(|meta| meta.text_color.as_deref())
            .is_some_and(|color| !color.trim().is_empty()),
        media: node_media(&node.node_type),
        world_x: len(node.position.x),
        world_y: len(node.position.y),
        width: len(node.width),
        height: len(node.height),
        selected,
        locked: open_canvas_core::geometry::is_node_locked(node),
        enter,
        plugin_kind: plugin_kind_id(cell.kind).into(),
        plugin_text: cell.text.as_str().into(),
        plugin_raw: cell.raw.as_str().into(),
        plugin_empty: cell.empty,
        plugin_editing: cell.editing,
        plugin_interactive: view.content_interactive,
        plugin_transparent: view.background == BackgroundMode::Transparent,
        plugin_bg: cell
            .color
            .as_deref()
            .and_then(plugin_views::parse_hex_color)
            .unwrap_or_default(),
        plugin_image: plugin.svg_image(&node.id).unwrap_or_default(),
        plugin_palette_open: plugin.palette_node.as_deref() == Some(node.id.as_str()),
        plugin_palette: ModelRc::new(VecModel::from(if cell.kind == PluginCellKind::Sticky {
            plugin_views::palette_colors()
        } else {
            Vec::new()
        })),
        has_source: view.has_source_handle && accepts_source,
        has_target: view.has_target_handle && accepts_target,
        content_image,
        content_fit: canvas_content::media_fit(node).into(),
        content_empty,
        status: status.into(),
        error_text: error_text.into(),
        status_label: status_label.into(),
        info: if media_info {
            canvas_content::media_info_line(node).into()
        } else {
            "".into()
        },
        media_info,
        stack_cover: sheet.cover,
        stack: ModelRc::new(VecModel::from(sheet.rows)),
        stack_count: count_i32(sheet.count),
        stack_count_label: count_label(sheet.count, "image").into(),
        stack_cover_error: sheet.cover_error.into(),
        assets: ModelRc::new(VecModel::from(assets)),
        asset_count: count_i32(asset_count),
        asset_count_label: canvas_content::asset_count_label(asset_count).into(),
        asset_source: canvas_content::asset_source_id(node).into(),
        asset_tree: ModelRc::new(VecModel::from(asset_tree)),
        asset_bound,
        asset_folder: asset_folder.into(),
        asset_error: asset_error.into(),
        duration: canvas_content::duration_label(meta.and_then(|m| m.duration_ms)).into(),
        batch_count: count_i32(text_count),
        batch_label: if text_count > 1 {
            count_label(text_count, "option").into()
        } else {
            "".into()
        },
        batch: ModelRc::new(VecModel::from(batch)),
        related,
        editing,
        edit_text: if editing {
            ctx.edit.body_draft(&node.id).into()
        } else {
            "".into()
        },
        editing_title,
        title_draft: if editing_title {
            ctx.edit.title_draft(&node.id).into()
        } else {
            "".into()
        },
        slot_first: slots.first.clone().into(),
        slot_last: slots.last.clone().into(),
        ref_slot_count: count_i32(reference_slot_count(ctx.reference_slots, &node.id)),
        ref_slot_titles: ModelRc::new(VecModel::from(reference_slot_titles(
            &slots,
            reference_slot_count(ctx.reference_slots, &node.id),
        ))),
        slot_drop: if ctx.drop_target == Some(node.id.as_str()) {
            ctx.drop_slot.unwrap_or_default().into()
        } else {
            "".into()
        },
        video_mode: if kind == CanvasNodeType::VideoPrompt {
            video_mode_id(node).into()
        } else {
            "".into()
        },
        prompt_caret,
        prompt_caret_seq,
        prompt_editing: ctx.panel.prompt_editing.as_deref() == Some(node.id.as_str()),
        ref_images: ModelRc::new(VecModel::from(ref_images)),
        mode: mode.into(),
        model: model.clone().into(),
        model_brand: crate::node_ui::model_brand_letter(&model).into(),
        settings_label: if generation {
            crate::node_ui::settings_label(node).into()
        } else {
            "".into()
        },
        generation_settings: if generation {
            crate::node_ui::generation_settings(node)
        } else {
            crate::GenerationSettings::default()
        },
        open_popover: open_popover.into(),
        can_generate: generation && can_generate,
        running: ctx.panel.running.as_deref() == Some(node.id.as_str()),
        attempt_version: i32::try_from(meta.and_then(|m| m.attempt_version).unwrap_or(1))
            .unwrap_or(i32::MAX),
        attempt_title: meta
            .and_then(|m| m.attempt_title.as_deref())
            .unwrap_or_default()
            .into(),
        attempt_frozen: meta.and_then(|m| m.attempt_frozen).unwrap_or(false),
        attempt_run_count: meta
            .and_then(|m| m.attempt_runs.as_ref())
            .map_or(0, |runs| count_i32(runs.len())),
        attempt_image_count: meta
            .and_then(|m| m.attempt_runs.as_ref())
            .map_or(0, |runs| {
                count_i32(runs.iter().map(|run| run.images.len()).sum())
            }),
        attempt_trace_count: meta
            .and_then(|m| m.agent_steps.as_ref())
            .map_or(0, |steps| count_i32(steps.len())),
    }
}

/// `metadata.videoMode` normalized to the wire string (default `frames`).
fn video_mode_id(node: &CanvasNodeData) -> &'static str {
    match node.metadata.as_ref().and_then(|meta| meta.video_mode) {
        Some(open_canvas_core::node::CanvasVideoMode::Reference) => "reference",
        _ => "frames",
    }
}

/// Video-prompt slot cells: the titles of the bound first/last frame nodes plus
/// the titles of the bound reference images (`metadata.videoSlots`).
#[derive(Default)]
struct SlotCells {
    first: String,
    last: String,
    /// One title per bound reference (empty entries keep their slot).
    references: Vec<String>,
}

fn slot_cells(node: &CanvasNodeData, index: &NodeIndex<'_>) -> SlotCells {
    let Some(slots) = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.video_slots.as_ref())
    else {
        return SlotCells::default();
    };
    let title_of = |id: Option<&String>| -> String {
        id.and_then(|id| index.get(id))
            .map_or_else(String::new, |node| node.title.clone())
    };
    let references = slots
        .references
        .as_ref()
        .map(|ids| ids.iter().map(|id| title_of(Some(id))).collect())
        .unwrap_or_default();
    SlotCells {
        first: title_of(slots.first_frame.as_ref()),
        last: title_of(slots.last_frame.as_ref()),
        references,
    }
}

/// Default number of reference-image slots on a video-prompt card in
/// `reference` mode. Three mirrors the web's default reference area and keeps
/// the card readable at its 340 px width.
pub const REFERENCE_SLOT_DEFAULT: usize = 3;
/// Lower bound of the slot stepper: a reference-mode card always offers at
/// least one reference image.
pub const REFERENCE_SLOT_MIN: usize = 1;
/// Upper bound: the provider's reference-image limit
/// (`VIDEO_REFERENCE_IMAGE_LIMIT`, `crates/ai/src/video_request.rs`), one slot
/// per image a request may carry.
pub const REFERENCE_SLOT_MAX: usize = 9;

/// Clamps a requested reference-slot count into
/// [`REFERENCE_SLOT_MIN`]..=[`REFERENCE_SLOT_MAX`]. A non-numeric or absent
/// request falls back to [`REFERENCE_SLOT_DEFAULT`].
pub fn clamp_reference_slots(count: i64) -> usize {
    let clamped = count.clamp(
        i64::try_from(REFERENCE_SLOT_MIN).unwrap_or(1),
        i64::try_from(REFERENCE_SLOT_MAX).unwrap_or(9),
    );
    usize::try_from(clamped).unwrap_or(REFERENCE_SLOT_DEFAULT)
}

/// The stored (or default) reference-slot count of one node.
pub fn reference_slot_count(
    slots: &std::collections::HashMap<String, usize>,
    node_id: &str,
) -> usize {
    slots
        .get(node_id)
        .copied()
        .map_or(REFERENCE_SLOT_DEFAULT, |count| {
            clamp_reference_slots(i64::try_from(count).unwrap_or(0))
        })
}

/// Per-slot values of the reference mode: the bound reference title when there
/// is one, otherwise an empty placeholder - always exactly `count` slots so the
/// card's slot row matches the stepper.
fn reference_slot_titles(cells: &SlotCells, count: usize) -> Vec<slint::SharedString> {
    (0..count)
        .map(|index| {
            cells
                .references
                .get(index)
                .map_or_else(Default::default, |title| title.as_str().into())
        })
        .collect()
}

/// The slot labels a video-prompt card shows for one mode: `frames` ->
/// the first-frame and last-frame slots only; `reference` -> exactly `count`
/// reference-image slots. Pure, so the tab rule is testable without Slint; the
/// `.slint` slot rows mirror this set and the test keeps them in step.
#[cfg(test)]
pub fn video_slot_labels(mode: &str, count: usize) -> Vec<&'static str> {
    if mode == "reference" {
        vec!["Reference"; clamp_reference_slots(i64::try_from(count).unwrap_or(0))]
    } else {
        vec!["First frame", "Last frame"]
    }
}

/// Slint-side tag for the plugin content renderer (empty for built-in nodes).
pub fn plugin_kind_id(kind: PluginCellKind) -> &'static str {
    match kind {
        PluginCellKind::None => "",
        PluginCellKind::Missing => "missing",
        PluginCellKind::Sticky => "sticky",
        PluginCellKind::Markdown => "markdown",
        PluginCellKind::Svg => "svg",
        PluginCellKind::Html => "html",
        PluginCellKind::Panorama => "panorama",
    }
}

/// Display body per node kind - the exhaustive dispatch that keeps the `.slint`
/// side free of domain logic. Text nodes show their primary batch alternative
/// (web `TextContent`), prompts their prompt, generation nodes nothing (their
/// card is the config hint, web `ImageGenerationContent`).
pub fn node_body(node: &CanvasNodeData) -> String {
    let meta = node.metadata.as_ref();
    match &node.node_type {
        CanvasNodeType::Text => canvas_content::text_body(meta),
        CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt | CanvasNodeType::ImageAttempt => {
            meta.and_then(|m| m.prompt.clone()).unwrap_or_default()
        }
        CanvasNodeType::ImageGeneration
        | CanvasNodeType::VideoGeneration
        | CanvasNodeType::Image
        | CanvasNodeType::ImageStack
        | CanvasNodeType::Video
        | CanvasNodeType::Assets
        | CanvasNodeType::SmartCanvas
        | CanvasNodeType::Plugin(_) => String::new(),
    }
}

/// Media kinds fill their card with `node.fill` (web: no glass card).
pub fn node_media(kind: &CanvasNodeType) -> bool {
    matches!(
        kind,
        CanvasNodeType::Image | CanvasNodeType::ImageStack | CanvasNodeType::Video
    )
}

/// One `[RulerTick]` model row: screen position + world-unit label.
pub fn ruler_tick_row(tick: Tick) -> crate::RulerTick {
    crate::RulerTick {
        pos: len(tick.screen),
        label: tick.value.to_string().into(),
    }
}

/// One `[RulerTickImage]` row for the vertical ruler: the label pre-rotated in
/// Rust (`ruler_text.rs`), cached per distinct value, drawn upright so Slint
/// never resamples it.
pub fn ruler_label_row(tick: Tick, ink: slint::Color, scale: f32) -> crate::RulerTickImage {
    let label = crate::ruler_text::label_image(&tick.value.to_string(), ink, scale);
    let (_, y) = crate::ruler_text::place_label(
        tick.screen,
        crate::ruler_text::STRIP_WIDTH,
        label.width,
        label.height,
    );
    crate::RulerTickImage {
        pos: len(tick.screen),
        image: label.image,
        label_width: len(f64::from(label.width)),
        label_height: len(f64::from(label.height)),
        label_y: len(y),
    }
}

/// f64 world value -> f32 Slint length. Canvas coordinates stay far inside the
/// f32 range, so the narrowing is lossless for any plausible document.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Slint lengths are f32; canvas coordinates are small finite values"
)]
pub fn len(value: f64) -> f32 {
    value as f32
}

/// Finite f64 -> i64 with truncation (pixel coordinates are floored by callers).
#[expect(
    clippy::cast_possible_truncation,
    reason = "pixel coordinates are small finite values; the cast truncates them like the web's Math.floor/map"
)]
pub fn f2i(value: f64) -> i64 {
    value as i64
}

/// Small i64 -> f64 (document dimensions, pixel coordinates).
#[expect(
    clippy::cast_precision_loss,
    reason = "document dimensions stay far below 2^53, so the conversion is exact"
)]
pub fn i2f(value: i64) -> f64 {
    value as f64
}

/// Small usize -> i64 (buffer dimensions).
#[expect(
    clippy::cast_possible_wrap,
    reason = "buffer dimensions come from a checked pixel count, never near i64::MAX"
)]
pub fn u2i(value: usize) -> i64 {
    value as i64
}

/// Non-negative i64 -> usize (pixel counts); negatives clamp to 0.
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "value is clamped to >= 0 just before the cast; counts stay far below usize::MAX"
)]
pub fn i2u(value: i64) -> usize {
    value.max(0) as usize
}

/// 0..255 f64 -> u8 with truncation (pointer tolerance slider).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "callers clamp to 0..=255; the cast truncates the integer value"
)]
pub fn f2u8(value: f64) -> u8 {
    value.clamp(0.0, 255.0) as u8
}

/// f64 seconds -> f32 (the texture renderer's time uniform); animation times
/// stay far inside the f32 range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "texture time is a small non-negative seconds value, exactly representable in f32"
)]
pub fn len32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
#[path = "vm_tests.rs"]
mod tests;
