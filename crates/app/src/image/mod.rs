//! Image Studio: state, project lifecycle and the app-side behaviour of the
//! board (`image-studio.tsx` + `use-image-store`). This file owns the single
//! source of truth for a studio session — [`ImageState`]: the `open-canvas-ps`
//! document, the bitmap map and the history snapshots.
//!
//! The studio's other concerns live beside this one:
//!
//! - [`render`] composites the board and pushes the studio view model.
//! - [`ops`] / [`transform`] / [`tools`] are the pointer pipelines (tools,
//!   handle drags, canvas media).
//! - [`actions`] / [`menu`] / [`filter`] / [`channels`] / [`editor`] are the
//!   panel + menu dispatchers and their row models.
//! - [`assets`] / [`masks`] / [`guides`] / [`text`] / [`text_edit`] /
//!   [`text_raster`] are the per-concern state and commands.
//! - [`ai`] is the studio's half of the AI tools; [`output`] / [`export`] /
//!   [`psd_io`] move files in and out; [`wire`] binds the Slint callbacks;
//!   [`dock_engine`] owns the arrangeable dock layout.

use std::collections::{BTreeMap, HashMap};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use open_canvas_core::document::ProjectIndexEntry;
use open_canvas_ps::{
    encode_png, hex_to_rgba, CanvasPsLayerKind, CanvasPsShapeKind, ImageProject, Point, Surface,
};

use crate::app::App;
use crate::image::assets::{
    default_swatches, ActionRecorder, BrushTool, GradientEdit, ImageAssetRow, ImageAssets,
};
use crate::store::StoreError;
use crate::AppWindow;

pub mod actions;
pub mod ai;
pub mod assets;
pub mod channels;
pub mod dock_engine;
pub mod editor;
pub mod export;
pub mod filter;
pub mod guides;
pub mod masks;
pub mod menu;
pub mod ops;
pub mod output;
pub mod psd_io;
pub mod render;
pub mod text;
pub mod text_edit;
pub mod text_raster;
pub mod tools;
pub mod transform;
pub mod wire;

#[cfg(test)]
mod ants_tests;

#[cfg(test)]
mod studio_tool_tests;

/// The library kind tag for image projects (`ProjectIndexEntry.kind`).
pub const PROJECT_KIND_IMAGE: &str = "image";

/// Marching-ants cadence (`ps-overlays.tsx:21`: `ANT_TICK_MS = 120`): the
/// desktop advances the dash phase on that cadence, one document pixel per
/// tick, through the native 2px-dash pattern (period 4).
pub const ANTS_TICK_MS: u64 = 120;
/// Dash run length in document pixels; the pattern period is twice this.
pub const ANTS_DASH: i64 = 2;
/// Phase step per tick.
pub const ANTS_PHASE_STEP: i64 = 1;
/// Phase wrap (`ANTS_DASH * 2`).
pub const ANTS_PHASE_CYCLE: i64 = ANTS_DASH * 2;

/// Whether the light half of the ants covers the boundary pixel at `x`/`y`
/// (`ps-overlays.tsx:52`, pass 1); the dark half covers the rest. At phase 0
/// this is the desktop's original `(x + y) % 4 >= 2` pattern.
#[must_use]
pub fn ants_light(x: i64, y: i64, phase: i64) -> bool {
    (x + y + phase).div_euclid(ANTS_DASH) % 2 == 1
}

/// The next marching-ants phase (`phaseRef.current + 2) % (ANT_DASH * 2)` in
/// the web; the desktop steps one pixel per tick).
#[must_use]
pub fn ants_advance(phase: i64) -> i64 {
    (phase + ANTS_PHASE_STEP).rem_euclid(ANTS_PHASE_CYCLE)
}
/// Board size input cap (the web resamples bitmaps to the new size).
pub const MAX_IMAGE_DIM: f64 = 4096.0;
/// History depth (`PS_HISTORY_LIMIT`, image-studio.tsx:16).
pub const HISTORY_LIMIT: usize = 50;
/// New board size (`smart-canvas.ts:40-47`: 640x360 at `16:9`, `2k`).
pub const DEFAULT_WIDTH: f64 = 640.0;
pub const DEFAULT_HEIGHT: f64 = 360.0;
pub const DEFAULT_RATIO: &str = "16:9";
/// Zoom bounds (`ZOOM_MIN`/`ZOOM_MAX`/`ZOOM_STEP`, image-studio.tsx:92-94).
pub const ZOOM_MIN: f64 = 0.05;
pub const ZOOM_MAX: f64 = 8.0;
pub const ZOOM_STEP: f64 = 1.2;

/// `TOOLS`: id, label, hotkey. Shift hotkeys keep their `⇧` prefix so
/// [`ImageTool::from_hotkey`] can demand the modifier. Move is folded into
/// Hand: one tool pans the view and drags the active layer under the pointer.
pub const TOOLS: [(&str, &str, &str); 15] = [
    ("hand", "Hand", "H"),
    ("pen", "Pen", "P"),
    ("direct-select", "Direct Select", "⇧P"),
    ("marquee", "Marquee", "M"),
    ("ellipse-marquee", "Ellipse Marquee", "⇧M"),
    ("lasso", "Lasso", "L"),
    ("polygon-lasso", "Polygon Lasso", "⇧L"),
    ("wand", "Magic Wand", "W"),
    ("quick-select", "Quick Select", "A"),
    ("brush", "Brush", "B"),
    ("eraser", "Eraser", "E"),
    ("bucket", "Bucket", "G"),
    ("gradient", "Gradient", "⇧G"),
    ("shape", "Shape", "U"),
    ("eyedropper", "Eyedropper", "I"),
];

/// `PsTool` (image-studio.tsx:99).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTool {
    Hand,
    Pen,
    DirectSelect,
    Marquee,
    EllipseMarquee,
    Lasso,
    PolygonLasso,
    Wand,
    QuickSelect,
    Brush,
    Eraser,
    Bucket,
    Gradient,
    Shape,
    Eyedropper,
}

impl ImageTool {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "hand" => Self::Hand,
            "pen" => Self::Pen,
            "direct-select" => Self::DirectSelect,
            "marquee" => Self::Marquee,
            "ellipse-marquee" => Self::EllipseMarquee,
            "lasso" => Self::Lasso,
            "polygon-lasso" => Self::PolygonLasso,
            "wand" => Self::Wand,
            "quick-select" => Self::QuickSelect,
            "brush" => Self::Brush,
            "eraser" => Self::Eraser,
            "bucket" => Self::Bucket,
            "gradient" => Self::Gradient,
            "shape" => Self::Shape,
            "eyedropper" => Self::Eyedropper,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hand => "hand",
            Self::Pen => "pen",
            Self::DirectSelect => "direct-select",
            Self::Marquee => "marquee",
            Self::EllipseMarquee => "ellipse-marquee",
            Self::Lasso => "lasso",
            Self::PolygonLasso => "polygon-lasso",
            Self::Wand => "wand",
            Self::QuickSelect => "quick-select",
            Self::Brush => "brush",
            Self::Eraser => "eraser",
            Self::Bucket => "bucket",
            Self::Gradient => "gradient",
            Self::Shape => "shape",
            Self::Eyedropper => "eyedropper",
        }
    }

    /// `TOOL_HOTKEYS` (image-studio.tsx:124) —`⇧X` entries need shift.
    pub fn from_hotkey(key: &str, shift: bool) -> Option<Self> {
        TOOLS.iter().find_map(|(id, _, hotkey)| {
            let (needs_shift, plain) = match hotkey.strip_prefix('⇧') {
                Some(rest) => (true, rest),
                None => (false, *hotkey),
            };
            (needs_shift == shift && plain.eq_ignore_ascii_case(key)).then(|| Self::from_id(id))?
        })
    }
}

/// `View { x, y, k }` (image-studio.tsx): screen transform of the board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageView {
    pub x: f64,
    pub y: f64,
    pub k: f64,
}

impl Default for ImageView {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        }
    }
}

/// Which colour slot the picker edits (foreground swaps with background).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSlot {
    Foreground,
    Background,
}

/// One pointer gesture (the web's `panRef`/`dragRef`/`gestureRef`).
#[derive(Debug, Clone)]
pub enum Gesture {
    Pan {
        start_x: f64,
        start_y: f64,
        view: ImageView,
    },
    Move {
        layer: String,
        start: Point,
        x: f64,
        y: f64,
    },
    Scale {
        layer: String,
        corner: usize,
        start: Point,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// A transform-mode handle drag (`psMoveTransformHandle`).
    Transform {
        layer: String,
        mode: open_canvas_ps::PsTransformMode,
        handle: usize,
    },
    /// Free-transform rotate handle (`beginLayerGesture` "rotate").
    Rotate {
        layer: String,
        start_angle: f64,
        rotation: f64,
    },
    Paint {
        layer: String,
        erase: bool,
        last: Point,
    },
    Marquee {
        tool: ImageTool,
        start: Point,
        current: Point,
        shift: bool,
        alt: bool,
    },
    Lasso {
        points: Vec<Point>,
    },
    Shape {
        start: Point,
        current: Point,
    },
    Gradient {
        start: Point,
        current: Point,
    },
    PenDrag {
        path: String,
        anchor: usize,
    },
    AnchorDrag {
        path: String,
        anchor: usize,
        part: u8,
    },
}

/// Which parameters the per-type editor addresses (`ps-adjustment-editor.tsx`
/// for adjustment layers, `ps-fx-panel.tsx` for one layer style).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum EditorTarget {
    #[default]
    Adjustment,
    Style(String),
}

/// Which text-panel tab the properties panel shows (`ps-text-panel.tsx`'s
/// `useState<"character" | "paragraph">`). The rows the panel renders are
/// filtered by this, so the switch is observable in the row model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextPanelTab {
    #[default]
    Character,
    Paragraph,
}

impl TextPanelTab {
    /// The tab a tab-row option selects (unknown values keep the default).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_ascii_lowercase().as_str() {
            "character" => Some(Self::Character),
            "paragraph" => Some(Self::Paragraph),
            _ => None,
        }
    }
}

/// The open image project's identity (content lives in [`ImageState`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub group_id: Option<String>,
}

/// The path-prompt export flow (PNG only in P5c-2).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageExport {
    pub path: String,
    pub message: String,
    pub error: bool,
}

/// The thirteen dock-panel visibility flags (`PS_DOCK_PANELS`).
///
/// `Default` is every flag false: the shipped dock layout starts with all
/// three edges empty, so nothing is visible until the Dock menu (or an
/// in-studio entry point) reveals a panel. `dock_engine::panels_for` derives
/// these flags from the layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent one-bit panel flags, not a mode enum"
)]
pub struct ImagePanels {
    pub layers: bool,
    pub fx: bool,
    pub history: bool,
    pub properties: bool,
    pub color: bool,
    pub channels: bool,
    pub paths: bool,
    pub actions: bool,
    pub assets: bool,
    pub adjustments: bool,
    pub gradients: bool,
    pub patterns: bool,
    pub brushes: bool,
}

/// One bitmap's transition inside a history entry.
#[derive(Debug, Clone)]
pub struct BitmapDelta {
    pub key: String,
    pub before: Surface,
    pub after: Surface,
}

/// A history snapshot: the document plus every touched bitmap's before/after.
#[derive(Debug, Clone)]
pub struct ImageSnap {
    pub label: String,
    pub project: ImageProject,
    pub cells: Vec<BitmapDelta>,
}

/// Bounded snapshot stack with labels (the history panel lists every entry and
/// jumps by index, so labels/index live beside the pixel crate's stack scheme).
#[derive(Debug, Clone)]
pub struct SnapStack {
    stack: Vec<ImageSnap>,
    index: isize,
    limit: usize,
}

impl SnapStack {
    pub fn new(limit: usize) -> Self {
        Self {
            stack: Vec::new(),
            index: -1,
            limit: limit.max(1),
        }
    }

    pub fn reset(&mut self, snapshot: ImageSnap) {
        self.stack.clear();
        self.stack.push(snapshot);
        self.index = 0;
    }

    /// Drops the redo tail, appends, caps at `limit` (oldest entries fall off).
    pub fn push(&mut self, snapshot: ImageSnap) {
        let keep = usize::try_from(self.index + 1).unwrap_or(0);
        self.stack.truncate(keep);
        self.stack.push(snapshot);
        if self.stack.len() > self.limit {
            let overflow = self.stack.len() - self.limit;
            self.stack.drain(0..overflow);
        }
        self.index = isize::try_from(self.stack.len()).unwrap_or(1) - 1;
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index < isize::try_from(self.stack.len()).unwrap_or(0) - 1
    }

    pub fn current(&self) -> Option<&ImageSnap> {
        usize::try_from(self.index)
            .ok()
            .and_then(|index| self.stack.get(index))
    }

    pub fn undo(&mut self) -> Option<&ImageSnap> {
        if !self.can_undo() {
            return None;
        }
        self.index -= 1;
        self.current()
    }

    pub fn redo(&mut self) -> Option<&ImageSnap> {
        if !self.can_redo() {
            return None;
        }
        self.index += 1;
        self.current()
    }

    /// Jumps the cursor to `target` (history-panel click). Undo/redo steps clamp
    /// to the stack bounds; the entry under the cursor is returned.
    /// `(index, len)` —the history row model's active flag and row count.
    pub fn position(&self) -> (usize, usize) {
        (usize::try_from(self.index).unwrap_or(0), self.stack.len())
    }

    pub fn label(&self, index: usize) -> Option<&str> {
        self.stack.get(index).map(|snap| snap.label.as_str())
    }
}

/// Everything the Image Studio workspace reads and writes.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent one-bit view/tool flags (grid/rulers/guides/snap/contiguous/sample-all/reverse) —not a mode enum (precedent: PixelState)"
)]
pub struct ImageState {
    pub meta: Option<ImageMeta>,
    pub project: ImageProject,
    pub bitmaps: HashMap<String, Surface>,
    pub history: SnapStack,
    pub tool: ImageTool,
    pub color: String,
    pub bg_color: String,
    pub color_slot: ColorSlot,
    pub swatches: Vec<String>,
    pub brush_size: f64,
    pub brush_hardness: f64,
    pub brush_opacity: f64,
    pub tolerance: f64,
    pub contiguous: bool,
    pub sample_all: bool,
    pub feather: f64,
    pub antialias: bool,
    pub gradient_radial: bool,
    pub gradient_reverse: bool,
    /// Gradient tool options (`image-studio.tsx` `useState({...})`): the blend
    /// mode, opacity (1..100) and the mid stop position (0..1) the next
    /// gradient gesture fills with.
    pub gradient_blend: String,
    pub gradient_opacity: f64,
    pub gradient_stop: f64,
    /// The text panel's active tab (`ps-text-panel.tsx` `tab`).
    pub text_tab: TextPanelTab,
    /// Set when a stroke auto-created a pixel layer, so the pointer wrapper can
    /// report the hint once (the web's `paintAutoLayer` hint).
    pub auto_layer_hint: bool,
    pub shape_kind: CanvasPsShapeKind,
    pub shape_fill: bool,
    pub shape_stroke_width: f64,
    /// The in-canvas text editor (double-click on a text layer).
    pub text_edit: crate::image::text_edit::TextEditState,
    pub active_layer: String,
    pub active_path: String,
    pub poly_points: Vec<Point>,
    pub selection: Option<Surface>,
    pub view: ImageView,
    pub canvas: (f64, f64),
    pub fitted: String,
    pub menu: String,
    pub settings_open: bool,
    pub panels: ImagePanels,
    pub show_grid: bool,
    pub show_rulers: bool,
    pub show_guides: bool,
    pub snap: bool,
    pub space_held: bool,
    pub guides: Vec<(bool, f64)>,
    pub snap_lines: Vec<(bool, f64)>,
    pub editing_layer: Option<(String, String)>,
    pub board_w: String,
    pub board_h: String,
    pub gesture: Option<Gesture>,
    pub export: Option<ImageExport>,
    /// The preset store (`use-ps-asset-store`) + its panel editing state.
    pub assets: ImageAssets,
    pub gradient_edit: GradientEdit,
    /// Expanded stops the gradient tool fills with (`{:?}` = fg/background).
    pub gradient_picked: Vec<(String, f64)>,
    /// Picked gradient id ("" = dynamic fg/background preset).
    pub gradient_picked_id: String,
    pub pattern_picked: String,
    pub brush_picked: String,
    pub brush_tool: BrushTool,
    pub recorder: ActionRecorder,
    pub channel_view: String,
    pub channel_edit: Option<(String, String)>,
    pub path_edit: Option<(String, String)>,
    pub action_edit: Option<(String, String)>,
    pub asset_search: String,
    pub asset_rows: Vec<ImageAssetRow>,
    pub asset_loaded: bool,
    /// The per-type editor's target + picked gradient stop.
    pub editor_target: EditorTarget,
    pub editor_stop: usize,
    /// The open filter dialog + the last applied filter (Repeat Last Filter).
    pub filter_dialog: Option<crate::image::filter::FilterDialog>,
    pub last_filter: Option<(String, crate::image::filter::FilterParams)>,
    /// The open menu modal + the last numeric transform (Repeat Transform).
    pub modal: Option<crate::image::menu::TransformModal>,
    pub last_transform: Option<open_canvas_ps::PsNumericTransform>,
    pub transform_mode: Option<open_canvas_ps::PsTransformMode>,
    /// Layers-panel group collapse set (`collapsed: Set<string>`).
    pub collapsed: Vec<String>,
    /// The layer whose mask is the paint target (`maskTargetId`); "" = layer.
    pub mask_target: String,
    /// Mask preview mode (`maskView`).
    pub mask_view: bool,
    /// The live guide drag (`guideRef` + `guideDraft`).
    pub guide_drag: Option<crate::image::guides::GuideDrag>,
    /// The import-bitmap path prompt.
    pub import: Option<crate::image::output::ImageImport>,
    /// File > Export PSD...: the PSD export path prompt.
    pub psd_export: Option<ImageExport>,
    /// The arrangeable dock layout (see [`dock_engine`]): edge sizes, split
    /// ratios and the tab groups of the left/right/bottom dock areas.
    pub dock: dock_engine::DockLayout,
    /// Whether `dock` was read from `dock.json` yet. The load happens on the
    /// first render, where the app's store is in hand.
    pub dock_loaded: bool,
    /// Set when the layout changed and the window still needs the new push.
    pub dock_dirty: bool,
    /// The in-flight panel drag the dock chrome shares with the drop overlay:
    /// (studio, panel, pointer x, pointer y).
    pub dock_drag: (String, String, f64, f64),
    /// The dock tooltip's hover state (see [`dock_engine::TooltipState`]).
    pub tooltip: dock_engine::TooltipState,
    /// Marching-ants phase (see [`ants_advance`]) and the next advance due.
    pub ants_phase: i64,
    pub ants_due: Option<u64>,
    /// Keys touched since the last history push, and their pre-edit bitmaps.
    dirty: Vec<String>,
    before: HashMap<String, Surface>,
    /// The last committed board composite (see `render::DisplayCache`).
    pub display_cache: crate::image::render::DisplayCache,
    /// The canvas project a Smart Canvas card launched this studio from: the
    /// studio's Home/back returns there instead of the library. Cleared by
    /// every library/normal enter.
    pub origin_canvas: Option<String>,
    /// Panel thumbnails, derived from bitmaps (see [`ThumbCache`]).
    pub thumb_cache: ThumbCache,
}

impl Default for ImageState {
    #[expect(
        clippy::too_many_lines,
        reason = "one flat field initializer list for the studio state, mirroring the struct"
    )]
    fn default() -> Self {
        let project = open_canvas_ps::ImageProject {
            id: String::new(),
            title: "Image Studio".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            group_id: String::new(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            board_ratio: DEFAULT_RATIO.to_string(),
            board_resolution: open_canvas_ps::doc::BoardResolution::K2,
            board_background: "transparent".to_string(),
            board_background_opacity: 1.0,
            board_layers: Vec::new(),
            board_paths: None,
            board_alpha_channels: None,
            board_channel_visibility: None,
        };
        Self {
            meta: None,
            project,
            bitmaps: HashMap::new(),
            history: SnapStack::new(HISTORY_LIMIT),
            tool: ImageTool::Hand,
            color: "#000000".to_string(),
            bg_color: "#ffffff".to_string(),
            color_slot: ColorSlot::Foreground,
            swatches: default_swatches(),
            brush_size: 24.0,
            brush_hardness: 0.8,
            brush_opacity: 1.0,
            tolerance: 32.0,
            contiguous: true,
            sample_all: true,
            feather: 0.0,
            antialias: true,
            gradient_radial: false,
            gradient_reverse: false,
            gradient_blend: "normal".to_string(),
            gradient_opacity: 100.0,
            gradient_stop: 1.0,
            text_tab: TextPanelTab::Character,
            auto_layer_hint: false,
            shape_kind: CanvasPsShapeKind::Rectangle,
            shape_fill: true,
            shape_stroke_width: 2.0,
            active_layer: String::new(),
            text_edit: crate::image::text_edit::TextEditState::default(),
            active_path: String::new(),
            poly_points: Vec::new(),
            selection: None,
            view: ImageView::default(),
            canvas: (0.0, 0.0),
            fitted: String::new(),
            menu: String::new(),
            settings_open: false,
            panels: ImagePanels::default(),
            show_grid: false,
            show_rulers: false,
            show_guides: true,
            snap: true,
            space_held: false,
            guides: Vec::new(),
            snap_lines: Vec::new(),
            editing_layer: None,
            board_w: DEFAULT_WIDTH.to_string(),
            board_h: DEFAULT_HEIGHT.to_string(),
            gesture: None,
            export: None,
            assets: ImageAssets::default(),
            gradient_edit: GradientEdit::default(),
            gradient_picked: Vec::new(),
            gradient_picked_id: String::new(),
            pattern_picked: String::new(),
            brush_picked: String::new(),
            brush_tool: BrushTool::default(),
            recorder: ActionRecorder::default(),
            channel_view: "rgb".to_string(),
            channel_edit: None,
            path_edit: None,
            action_edit: None,
            asset_search: String::new(),
            asset_rows: Vec::new(),
            asset_loaded: false,
            editor_target: EditorTarget::default(),
            editor_stop: 0,
            filter_dialog: None,
            last_filter: None,
            modal: None,
            last_transform: None,
            transform_mode: None,
            collapsed: Vec::new(),
            mask_target: String::new(),
            mask_view: false,
            guide_drag: None,

            import: None,
            psd_export: None,
            dock: dock_engine::default_layout(dock_engine::STUDIO_IMAGE),
            dock_loaded: false,
            dock_dirty: false,
            dock_drag: (String::new(), String::new(), 0.0, 0.0),
            tooltip: dock_engine::TooltipState::default(),
            ants_phase: 0,
            ants_due: None,
            dirty: Vec::new(),
            before: HashMap::new(),
            display_cache: crate::image::render::DisplayCache::default(),
            origin_canvas: None,
            thumb_cache: ThumbCache::default(),
        }
    }
}

/// Board dimensions as bitmap sizes; `None` for a degenerate board.
pub fn board_dims(project: &ImageProject) -> Option<(usize, usize)> {
    let width = usize::try_from(crate::vm::f2i(project.width.round())).ok()?;
    let height = usize::try_from(crate::vm::f2i(project.height.round())).ok()?;
    (width > 0 && height > 0 && width.checked_mul(height)? <= 16_777_216).then_some((width, height))
}

/// Panel-thumbnail cache: the small `slint::Image` derived from one bitmap at
/// one size, keyed by the bitmap's content stamp. The layers panel used to
/// resample every full-size layer bitmap down to 28 px on every single sync
/// (a pointer move, a zoom, a slider tick); the stamp makes a repeat free
/// without ever showing stale pixels.
#[derive(Debug, Default)]
pub struct ThumbCache {
    entries:
        std::collections::HashMap<String, std::collections::HashMap<usize, (u64, slint::Image)>>,
}

impl ThumbCache {
    /// The `max`-long thumbnail of the bitmap stored under `key`.
    pub fn image(
        &mut self,
        key: &str,
        surface: &Surface,
        max: usize,
        render: impl FnOnce(&Surface, usize) -> slint::Image,
    ) -> slint::Image {
        let stamp = surface.stamp();
        if let Some((cached_stamp, image)) = self.entries.get(key).and_then(|sizes| sizes.get(&max))
        {
            if *cached_stamp == stamp {
                return image.clone();
            }
        }
        let image = render(surface, max);
        self.entries
            .entry(key.to_string())
            .or_default()
            .insert(max, (stamp, image.clone()));
        image
    }
}

impl ImageState {
    pub fn dims(&self) -> Option<(usize, usize)> {
        board_dims(&self.project)
    }

    pub fn board(&self) -> (f64, f64) {
        (self.project.width, self.project.height)
    }

    /// The active layer when it still exists; falls back to the top layer.
    pub fn active_layer_id(&self) -> String {
        if open_canvas_ps::layer_ops::find_ps_layer(&self.project.board_layers, &self.active_layer)
            .is_some()
        {
            return self.active_layer.clone();
        }
        self.project
            .board_layers
            .last()
            .map_or_else(String::new, |layer| layer.id.clone())
    }

    /// Keeps `active_layer`/`active_path` pointing at existing ids.
    pub fn fix_active_ids(&mut self) {
        self.active_layer = self.active_layer_id();
        let paths = self.project.board_paths.clone().unwrap_or_default();
        if !paths.iter().any(|path| path.id == self.active_path) {
            self.active_path = paths
                .first()
                .map_or_else(String::new, |path| path.id.clone());
        }
        if !self.mask_target.is_empty()
            && open_canvas_ps::layer_ops::find_ps_layer(
                &self.project.board_layers,
                &self.mask_target,
            )
            .is_none()
        {
            self.mask_target.clear();
            self.mask_view = false;
        }
    }

    /// Ensures the active layer has a bitmap (pixel/image layers); mints the
    /// `image:<id>` key the web stores under (image-storage.ts:18).
    pub fn ensure_layer_bitmap(&mut self, layer_id: &str) -> Option<String> {
        let layer = open_canvas_ps::layer_ops::find_ps_layer(&self.project.board_layers, layer_id)?;
        if !matches!(
            layer.kind,
            CanvasPsLayerKind::Pixel | CanvasPsLayerKind::Image
        ) {
            return None;
        }
        let width = usize::try_from(crate::vm::f2i(layer.width.round().max(1.0))).ok()?;
        let height = usize::try_from(crate::vm::f2i(layer.height.round().max(1.0))).ok()?;
        if width.checked_mul(height)? > 16_777_216 {
            return None;
        }
        if let Some(key) = layer.storage_key.clone() {
            self.bitmaps
                .entry(key.clone())
                .or_insert_with(|| Surface::new(width, height));
            return Some(key);
        }
        let key = format!("image:{layer_id}");
        self.bitmaps
            .entry(key.clone())
            .or_insert_with(|| Surface::new(width, height));
        self.project.board_layers =
            open_canvas_ps::patch_ps_layer(&self.project.board_layers, layer_id, |target| {
                target.storage_key = Some(key.clone());
            });
        Some(key)
    }

    /// Marks `keys` as touched and snapshots their pre-edit bitmaps.
    pub fn begin_edit(&mut self, keys: &[String]) {
        for key in keys {
            if !self.before.contains_key(key) {
                if let Some(bitmap) = self.bitmaps.get(key) {
                    self.before.insert(key.clone(), bitmap.clone());
                }
            }
            if !self.dirty.contains(key) {
                self.dirty.push(key.clone());
            }
        }
    }

    /// Pushes a labeled history snapshot (document + touched bitmap deltas).
    pub fn commit(&mut self, label: &str) {
        let mut cells = Vec::with_capacity(self.dirty.len());
        for key in std::mem::take(&mut self.dirty) {
            let Some(before) = self.before.remove(&key) else {
                continue;
            };
            let Some(after) = self.bitmaps.get(&key) else {
                continue;
            };
            cells.push(BitmapDelta {
                key,
                before,
                after: after.clone(),
            });
        }
        self.before.clear();
        self.history.push(ImageSnap {
            label: label.to_string(),
            project: self.project.clone(),
            cells,
        });
    }

    /// Undo to the previous snapshot (document + touched bitmaps).
    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }
        let restore: Vec<(String, Surface)> = self
            .history
            .current()
            .map(|snap| {
                snap.cells
                    .iter()
                    .map(|delta| (delta.key.clone(), delta.before.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let Some(target) = self.history.undo() else {
            return false;
        };
        self.project = target.project.clone();
        for (key, bitmap) in restore {
            self.bitmaps.insert(key, bitmap);
        }
        self.clear_transient();
        true
    }

    /// Redo to the next snapshot.
    pub fn redo(&mut self) -> bool {
        let Some(target) = self.history.redo() else {
            return false;
        };
        let restore: Vec<(String, Surface)> = target
            .cells
            .iter()
            .map(|delta| (delta.key.clone(), delta.after.clone()))
            .collect();
        self.project = target.project.clone();
        for (key, bitmap) in restore {
            self.bitmaps.insert(key, bitmap);
        }
        self.clear_transient();
        true
    }

    /// Jumps the history cursor to `index` (the history panel's click).
    pub fn jump_history(&mut self, index: usize) -> bool {
        let (current, _) = self.history.position();
        if index == current {
            return false;
        }
        if index < current {
            while self.history.position().0 > index {
                if !self.undo() {
                    break;
                }
            }
        } else {
            while self.history.position().0 < index {
                if !self.redo() {
                    break;
                }
            }
        }
        true
    }

    /// Clears gesture/selection state a restore invalidates (web `restore`).
    fn clear_transient(&mut self) {
        self.gesture = None;
        self.poly_points.clear();
        self.selection = None;
        self.snap_lines.clear();
        self.guide_drag = None;
        self.fix_active_ids();
    }

    /// Clears workspace state when leaving the editor.
    pub fn close(&mut self) {
        self.gesture = None;
        self.poly_points.clear();
        self.menu.clear();
        self.settings_open = false;
        self.export = None;
        self.import = None;
        self.psd_export = None;
        self.editing_layer = None;
        self.snap_lines.clear();
        self.guide_drag = None;
        self.transform_mode = None;
        self.mask_target.clear();
        self.mask_view = false;
    }

    /// The active colour as RGBA (`hex_to_rgba`).
    pub fn active_rgba(&self) -> open_canvas_ps::Rgba {
        hex_to_rgba(&self.color)
    }

    /// The gradient tool's stops: the picked preset's expanded stops, or the
    /// foreground/background pair when nothing is picked.
    pub fn gradient_stops(&self) -> Vec<(String, f64)> {
        if self.gradient_picked.is_empty() {
            // The dynamic fg/background preset: the second stop sits at the
            // gradient options' stop position (`paint.stop`, default 1).
            vec![
                (self.color.clone(), 0.0),
                (self.bg_color.clone(), self.gradient_stop.clamp(0.0, 1.0)),
            ]
        } else {
            self.gradient_picked.clone()
        }
    }

    /// Swaps foreground/background (the picker's swap button).
    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.color, &mut self.bg_color);
    }

    /// The mask being painted/edited: the selected layer's mask when targeted
    /// (`maskTargetId === selected.id && selected.maskStorageKey`).
    pub fn mask_layer(&self) -> Option<&open_canvas_ps::CanvasPsLayer> {
        if self.mask_target.is_empty() || self.mask_target != self.active_layer_id() {
            return None;
        }
        open_canvas_ps::layer_ops::find_ps_layer(&self.project.board_layers, &self.mask_target)
            .filter(|layer| layer.mask_storage_key.is_some())
    }

    /// `viewBoard`: with mask view on, the targeted layer renders its mask as
    /// the layer content (the web swaps `storageKey`/`maskStorageKey`).
    pub fn mask_view_project(&self) -> ImageProject {
        let Some(target) = self
            .mask_layer()
            .filter(|layer| !matches!(layer.kind, CanvasPsLayerKind::Group))
            .map(|layer| layer.id.clone())
        else {
            return self.project.clone();
        };
        let mut project = self.project.clone();
        project.board_layers =
            open_canvas_ps::patch_ps_layer(&project.board_layers, &target, |layer| {
                layer.storage_key = layer.mask_storage_key.clone();
                layer.mask_storage_key = None;
                layer.content = None;
            });
        project
    }

    /// Group collapse state (`collapsed` set membership).
    pub fn is_collapsed(&self, id: &str) -> bool {
        self.collapsed.iter().any(|entry| entry == id)
    }

    pub fn toggle_collapse(&mut self, id: &str) {
        if let Some(index) = self.collapsed.iter().position(|entry| entry == id) {
            self.collapsed.remove(index);
        } else {
            self.collapsed.push(id.to_string());
        }
    }
}

/// The on-disk image project: the board document plus `storageKey -> base64 PNG`
/// (the web keeps those blobs in localforage; here they travel in the file so
/// `images\<id>.json` round-trips losslessly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFile {
    pub project: ImageProject,
    #[serde(default)]
    pub bitmaps: BTreeMap<String, String>,
    /// The preset store (`use-ps-asset-store`); older files default it.
    #[serde(default)]
    pub assets: ImageAssets,
}

impl ImageFile {
    /// Serializes every bitmap to base64 PNG; a broken encoder is a typed error.
    pub fn from_state(
        project: &ImageProject,
        bitmaps: &HashMap<String, Surface>,
        assets: &ImageAssets,
    ) -> Result<Self, String> {
        let mut encoded = BTreeMap::new();
        for (key, surface) in bitmaps {
            let png = encode_png(surface).map_err(|error| error.to_string())?;
            encoded.insert(key.clone(), BASE64.encode(png));
        }
        Ok(Self {
            project: project.clone(),
            bitmaps: encoded,
            assets: assets.clone(),
        })
    }

    /// Decodes every embedded PNG back into a `Surface` plus the asset store.
    pub fn into_state(
        self,
    ) -> Result<(ImageProject, HashMap<String, Surface>, ImageAssets), String> {
        let mut bitmaps = HashMap::new();
        for (key, blob) in self.bitmaps {
            let bytes = BASE64
                .decode(blob.as_bytes())
                .map_err(|error| error.to_string())?;
            let surface = open_canvas_ps::decode_rgba(&bytes).map_err(|error| error.to_string())?;
            bitmaps.insert(key, surface);
        }
        Ok((self.project, bitmaps, self.assets))
    }
}

impl App {
    pub(crate) fn mint_image_id(&mut self, prefix: &str) -> String {
        self.library.seq = self.library.seq.wrapping_add(1);
        format!("{prefix}-{}-{}", self.now_ms(), self.library.seq)
    }

    /// A fresh `IdGen` whose ids can never collide with a previous batch.
    pub(crate) fn image_gen(&mut self, prefix: &str) -> open_canvas_ps::IdGen {
        let seed = self.mint_image_id(prefix);
        open_canvas_ps::IdGen::new(&seed)
    }

    /// The board auto-grow (`image-studio.tsx:270-281`): when the visible top
    /// layers' union box overflows the board, and the ratio's preset size can
    /// still contain it, the board grows to that preset size. Returns whether
    /// the board changed. Runs inside the same history entry as the edit that
    /// overflowed it.
    pub(crate) fn grow_image_board_to_fit(&mut self) -> bool {
        let layers = &self.image.project.board_layers;
        let visible: Vec<open_canvas_ps::CanvasPsLayer> = open_canvas_ps::top_layers(layers)
            .into_iter()
            .filter(|layer| !layer.hidden)
            .collect();
        if visible.is_empty() {
            return false;
        }
        let boxes: Vec<open_canvas_ps::Bounds> = visible
            .iter()
            .map(|layer| open_canvas_ps::layer_box(layers, layer))
            .collect();
        let Some(content) = open_canvas_ps::Bounds::union(&boxes) else {
            return false;
        };
        let (content_right, content_bottom) =
            (content.x + content.width, content.y + content.height);
        let (width, height) = (self.image.project.width, self.image.project.height);
        if content_right <= width + 0.5 && content_bottom <= height + 0.5 {
            return false;
        }
        let (preset_w, preset_h) =
            open_canvas_ps::board::smart_canvas_size_for_ratio(&self.image.project.board_ratio);
        if preset_w + 0.5 < content_right || preset_h + 0.5 < content_bottom {
            return false;
        }
        if (preset_w - width).abs() < 0.5 && (preset_h - height).abs() < 0.5 {
            return false;
        }
        self.image.project.width = preset_w;
        self.image.project.height = preset_h;
        self.image.board_w = preset_w.round().to_string();
        self.image.board_h = preset_h.round().to_string();
        self.image.fitted.clear();
        true
    }

    pub(crate) fn mint_image_layer_id(&mut self) -> String {
        self.mint_image_id("ps-layer")
    }

    pub(crate) fn mint_image_path_id(&mut self) -> String {
        self.mint_image_id("ps-path")
    }

    /// Enters the Image Studio with a loaded project file (fresh history, view
    /// auto-fit, tool defaults kept from the previous session like the web).
    pub fn open_image_ui(&mut self, ui: &AppWindow, file: ImageFile) {
        self.flush_save(ui);
        let (project, bitmaps, assets) = match file.into_state() {
            Ok(triple) => triple,
            Err(error) => {
                self.set_notice(ui, format!("Open failed: {error}"), true);
                return;
            }
        };
        self.enter_image(project, bitmaps, assets);
        self.sync_all(ui);
    }

    /// Opens the Image Studio from a Smart Canvas card: remembers the canvas
    /// project it came from so the studio's Home/back returns there instead of
    /// the library, then enters through the normal `enter_image` path.
    pub(crate) fn open_smart_canvas_node(&mut self, ui: &AppWindow, node_id: &str) {
        let Some(node) = self.doc.node(node_id).cloned() else {
            return;
        };
        if node.node_type != open_canvas_core::node::CanvasNodeType::SmartCanvas {
            return;
        }
        let Some(project_id) = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.smart_canvas_project_id.clone())
        else {
            self.set_notice(ui, "This Smart Canvas has no project yet".to_string(), true);
            return;
        };
        let origin = self.editor.as_ref().map(|meta| meta.id.clone());
        // Persist the canvas before leaving it: back reloads it from the store.
        self.flush_save(ui);
        let file = match self.store.load_image(&project_id) {
            Ok(file) => file,
            Err(error) => {
                self.set_notice(ui, format!("Open failed: {error}"), true);
                return;
            }
        };
        self.open_image_ui(ui, file);
        if self.view == crate::app::View::Image {
            self.image.origin_canvas = origin;
            // The back arrow, the hold ring and the canvas-scoped asset pool
            // depend on the origin; the initial open sync ran before it existed.
            self.image_asset_refresh_core();
            self.sync_image(ui);
        }
    }

    /// Composite preview of a stored image project, for a Smart Canvas card.
    /// `None` when the project is missing/unreadable or paints nothing, which
    /// is exactly the card's empty placeholder state.
    pub(crate) fn smart_canvas_preview(&self, project_id: &str) -> Option<slint::Image> {
        let (project, bitmaps, _assets) =
            self.store.load_image(project_id).ok()?.into_state().ok()?;
        let surface = crate::image::render::board_surface(&project, &bitmaps)?;
        if !surface
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0)
        {
            return None;
        }
        Some(crate::image::render::image_of(&surface))
    }

    pub(crate) fn enter_image(
        &mut self,
        project: ImageProject,
        bitmaps: HashMap<String, Surface>,
        assets: ImageAssets,
    ) {
        let image = &mut self.image;
        image.origin_canvas = None;
        image.meta = Some(ImageMeta {
            id: project.id.clone(),
            title: project.title.clone(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            group_id: (!project.group_id.is_empty()).then(|| project.group_id.clone()),
        });
        image.project = project;
        image.bitmaps = bitmaps;
        image.assets = assets;
        if !image.assets.swatches.is_empty() {
            image.swatches.clone_from(&image.assets.swatches);
        }
        image.asset_rows.clear();
        image.asset_loaded = false;
        image.asset_search.clear();
        image.fix_active_ids();
        let snap = ImageSnap {
            label: "Open".to_string(),
            project: image.project.clone(),
            cells: Vec::new(),
        };
        image.history.reset(snap);
        image.selection = None;
        image.poly_points.clear();
        image.text_edit.cancel();
        image.view = ImageView::default();
        image.fitted = String::new();
        image.board_w = image.project.width.to_string();
        image.board_h = image.project.height.to_string();
        image.close();
        self.view = crate::app::View::Image;
        self.clear_library_edits();
        self.save_dirty = false;
        self.save_due = None;
    }

    /// Writes the open image project (metadata + board + embedded bitmaps) and
    /// refreshes its library row.
    pub fn image_save_now(&mut self) -> Result<(), StoreError> {
        let Some(mut meta) = self.image.meta.clone() else {
            return Ok(());
        };
        meta.updated_at = crate::store::now_iso();
        let mut project = self.image.project.clone();
        project.updated_at.clone_from(&meta.updated_at);
        project.group_id = meta.group_id.clone().unwrap_or_default();
        let mut assets = self.image.assets.clone();
        assets.swatches.clone_from(&self.image.swatches);
        let file = ImageFile::from_state(&project, &self.image.bitmaps, &assets)
            .map_err(StoreError::Image)?;
        self.store.save_image(&file)?;
        self.library.index.upsert_entry(ProjectIndexEntry {
            id: meta.id.clone(),
            title: meta.title.clone(),
            created_at: meta.created_at.clone(),
            updated_at: meta.updated_at.clone(),
            group_id: meta.group_id.clone(),
            kind: PROJECT_KIND_IMAGE.to_string(),
        });
        self.store.save_index(&self.library.index)?;
        self.image.project = project;
        self.image.meta = Some(meta);
        Ok(())
    }
}

/// True when `kind` names an image project (`ProjectIndexEntry.kind`).
pub fn is_image_kind(kind: &str) -> bool {
    kind == PROJECT_KIND_IMAGE
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_ps::{create_ps_pixel_layer, create_ps_text_layer, IdGen};

    pub(crate) fn state() -> ImageState {
        let mut state = ImageState::default();
        state.project.id = "img-1".to_string();
        state.project.title = "Board".to_string();
        let mut gen = IdGen::new("t");
        let layer = create_ps_pixel_layer(&mut gen, (8.0, 8.0), "Layer 1");
        let id = layer.id.clone();
        state.project.board_layers = vec![layer];
        state.active_layer = id;
        let snap = ImageSnap {
            label: "Open".to_string(),
            project: state.project.clone(),
            cells: Vec::new(),
        };
        state.history.reset(snap);
        state
    }

    #[test]
    fn rulers_are_off_by_default() {
        assert!(
            !ImageState::default().show_rulers,
            "the ruler band is a View toggle, not permanent chrome"
        );
    }

    #[test]
    fn commit_records_doc_and_bitmap_deltas_and_undo_redo_apply_them() {
        let mut state = state();
        let key = state.ensure_layer_bitmap("t-1").unwrap_or_default();
        assert_eq!(key, "image:t-1");
        state.begin_edit(std::slice::from_ref(&key));
        if let Some(bitmap) = state.bitmaps.get_mut(&key) {
            bitmap.set(2, 3, [255, 0, 0, 255]);
        }
        let added = create_ps_text_layer(&mut IdGen::new("x"), (8.0, 8.0), "hi", "Text");
        let added_id = added.id.clone();
        state.project.board_layers.push(added);
        state.commit("Stroke");
        assert!(state.history.can_undo());

        assert!(state.undo());
        assert_eq!(
            state.project.board_layers.len(),
            1,
            "structural change reverts"
        );
        assert_eq!(
            state.bitmaps.get(&key).map(|b| b.get(2, 3)),
            Some([0, 0, 0, 0])
        );
        assert!(state.history.can_redo());

        assert!(state.redo());
        assert_eq!(state.project.board_layers.len(), 2);
        assert_eq!(
            state.bitmaps.get(&key).map(|b| b.get(2, 3)),
            Some([255, 0, 0, 255])
        );
        assert!(state
            .project
            .board_layers
            .iter()
            .any(|layer| layer.id == added_id));
    }

    #[test]
    fn begin_edit_keeps_the_first_pre_edit_bitmap_per_key() {
        let mut state = state();
        let key = state.ensure_layer_bitmap("t-1").unwrap_or_default();
        state.begin_edit(std::slice::from_ref(&key));
        if let Some(bitmap) = state.bitmaps.get_mut(&key) {
            bitmap.set(0, 0, [1, 1, 1, 255]);
        }
        state.begin_edit(std::slice::from_ref(&key));
        if let Some(bitmap) = state.bitmaps.get_mut(&key) {
            bitmap.set(0, 0, [2, 2, 2, 255]);
        }
        state.commit("Stroke");
        assert!(state.undo());
        assert_eq!(
            state.bitmaps.get(&key).map(|b| b.get(0, 0)),
            Some([0, 0, 0, 0]),
            "undo returns to the pre-edit bitmap, not the mid-stroke one"
        );
    }

    #[test]
    fn jump_history_moves_the_cursor_both_ways() {
        let mut state = state();
        for step in 0..3 {
            if let Some(layer) = state.project.board_layers.first_mut() {
                layer.x = f64::from(step);
            }
            state.commit("Step");
        }
        assert_eq!(state.history.position().0, 3);
        assert!(state.jump_history(1));
        assert_eq!(state.project.board_layers[0].x, 0.0);
        assert!(state.jump_history(3));
        assert_eq!(state.project.board_layers[0].x, 2.0);
        assert!(!state.jump_history(3));
    }

    #[test]
    fn image_file_round_trips_project_and_bitmaps() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = state();
        let key = state.ensure_layer_bitmap("t-1").unwrap_or_default();
        if let Some(bitmap) = state.bitmaps.get_mut(&key) {
            bitmap.set(3, 4, [12, 34, 56, 255]);
        }
        let file = ImageFile::from_state(&state.project, &state.bitmaps, &ImageAssets::default())
            .map_err(std::io::Error::other)?;
        assert!(!file.bitmaps.is_empty());
        let (project, bitmaps, assets) = file.into_state().map_err(std::io::Error::other)?;
        assert_eq!(project.board_layers.len(), 1);
        assert!(
            !assets.gradients.is_empty(),
            "the default store round-trips"
        );
        assert_eq!(
            bitmaps.get(&key).map(|b| b.get(3, 4)),
            Some([12, 34, 56, 255])
        );
        Ok(())
    }

    #[test]
    fn the_default_tool_is_hand() {
        assert_eq!(ImageState::default().tool, ImageTool::Hand);
        assert_eq!(TOOLS[0].0, "hand", "the rail opens on Hand");
        assert_eq!(TOOLS[0].1, "Hand");
        assert!(
            ImageTool::from_id("move").is_none(),
            "the Move tool id is retired"
        );
    }

    #[test]
    fn tool_hotkeys_require_shift_for_the_shift_variants() {
        assert_eq!(ImageTool::from_hotkey("h", false), Some(ImageTool::Hand));
        assert_eq!(ImageTool::from_hotkey("p", false), Some(ImageTool::Pen));
        assert_eq!(
            ImageTool::from_hotkey("p", true),
            Some(ImageTool::DirectSelect)
        );
        assert_eq!(
            ImageTool::from_hotkey("m", true),
            Some(ImageTool::EllipseMarquee)
        );
        assert_eq!(
            ImageTool::from_hotkey("l", true),
            Some(ImageTool::PolygonLasso)
        );
        assert_eq!(ImageTool::from_hotkey("g", true), Some(ImageTool::Gradient));
        assert_eq!(ImageTool::from_hotkey("p", false), Some(ImageTool::Pen));
        assert_eq!(ImageTool::from_hotkey("v", true), None);
        assert_eq!(
            ImageTool::from_hotkey("v", false),
            None,
            "the V shortcut went with the Move tool"
        );
        assert_eq!(ImageTool::from_hotkey("q", false), None);
    }

    #[test]
    fn snap_stack_jump_truncates_after_a_new_push() {
        let mut stack = SnapStack::new(2);
        let snap = |label: &str| ImageSnap {
            label: label.to_string(),
            project: ImageState::default().project,
            cells: Vec::new(),
        };
        stack.reset(snap("a"));
        stack.push(snap("b"));
        stack.push(snap("c"));
        assert_eq!(stack.position().1, 2, "stack is capped");
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
        let _ = stack.undo();
        stack.push(snap("d"));
        assert!(!stack.can_redo());
        assert_eq!(stack.label(1), Some("d"));
    }

    #[test]
    fn entering_an_image_project_switches_the_workspace() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        assert_eq!(app.view, crate::app::View::Library);
        assert!(!app.workspace_open("image"));

        let project = state().project;
        app.enter_image(project, HashMap::new(), ImageAssets::default());
        assert_eq!(app.view, crate::app::View::Image);
        assert!(app.workspace_open("image"));
        assert!(!app.workspace_open("canvas"));
        assert!(!app.workspace_open("pixel"));
        assert_eq!(
            app.image.meta.as_ref().map(|meta| meta.title.as_str()),
            Some("Board")
        );
        assert!(!app.image.history.can_undo());
        Ok(())
    }

    #[test]
    fn brush_stroke_paints_the_layer_and_undo_restores_it() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        app.enter_image(state().project, HashMap::new(), ImageAssets::default());
        let key = "image:t-1".to_string();

        app.image.tool = ImageTool::Brush;
        app.image.brush_size = 4.0;
        app.image.color = "#ff0000".to_string();
        app.image.view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            2.0,
            2.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            5.0,
            2.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            5.0,
            2.0,
            crate::input::Button::Left,
            false,
            false,
        );

        let painted = app
            .image
            .bitmaps
            .get(&key)
            .map(|bitmap| bitmap.get(2, 2))
            .unwrap_or_default();
        assert_eq!(painted, [255, 0, 0, 255], "the stroke painted the layer");
        let end = app
            .image
            .bitmaps
            .get(&key)
            .map(|bitmap| bitmap.get(5, 2))
            .unwrap_or_default();
        assert_eq!(end[3], 255, "the stroke spans the drag");
        assert!(app.image.history.can_undo());
        assert_eq!(app.image.history.label(1), Some("Brush"));

        assert!(app.image.undo());
        assert_eq!(
            app.image.bitmaps.get(&key).map(|bitmap| bitmap.get(2, 2)),
            Some([0, 0, 0, 0]),
            "undo removed the stroke"
        );
        Ok(())
    }

    #[test]
    fn layer_ops_add_duplicate_and_delete_through_the_history(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        app.enter_image(state().project, HashMap::new(), ImageAssets::default());

        let layers_before = app.image.project.board_layers.len();
        app.image_layer_action_core("add-pixel", "");
        app.image_layer_action_core("duplicate", "");
        assert_eq!(app.image.project.board_layers.len(), layers_before + 2);
        let active = app.image.active_layer.clone();
        assert!(app
            .image
            .project
            .board_layers
            .iter()
            .any(|layer| layer.id == active));

        app.image_layer_action_core("delete", "");
        assert_eq!(app.image.project.board_layers.len(), layers_before + 1);
        assert!(app.image.undo());
        assert_eq!(app.image.project.board_layers.len(), layers_before + 2);
        Ok(())
    }

    #[test]
    fn snapping_aligns_a_dragged_layer_to_the_board_targets(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        app.enter_image(state().project, HashMap::new(), ImageAssets::default());
        app.image.view = ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        let (x, y) = app.snap_move("t-1", 2.0, 0.5);
        // An 8x8 layer on an 8x8 board: every target is 0, so both axes snap.
        assert_eq!((x, y), (0.0, 0.0));
        assert_eq!(app.image.snap_lines.len(), 2);
        app.image.snap = false;
        let (x, y) = app.snap_move("t-1", 2.0, 0.5);
        assert_eq!((x, y), (2.0, 0.5));
        assert_eq!(app.image.snap_lines.len(), 0);
        Ok(())
    }

    #[test]
    fn image_save_now_writes_the_file_and_the_library_row() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        app.enter_image(state().project, HashMap::new(), ImageAssets::default());
        app.image.brush_size = 4.0;
        app.image.tool = ImageTool::Brush;
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            1.0,
            1.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            1.0,
            1.0,
            crate::input::Button::Left,
            false,
            false,
        );

        app.image_save_now().map_err(std::io::Error::other)?;
        let file = app.store.load_image(
            &app.image
                .meta
                .as_ref()
                .map_or_else(String::new, |meta| meta.id.clone()),
        )?;
        assert_eq!(file.project.board_layers.len(), 1);
        assert!(!file.bitmaps.is_empty(), "the painted bitmap is embedded");
        let row = app.library.index.projects.first();
        assert!(row.is_some_and(|row| is_image_kind(&row.kind)));
        Ok(())
    }
}
