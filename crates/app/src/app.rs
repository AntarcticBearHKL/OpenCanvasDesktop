//! Rust-side application state: owns the document and pushes the Slint view
//! model. The view-model refresh itself lives in `sync/` (one `impl App` block
//! per concern, [`App::sync_all`] is the entry point).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, VecModel};

use open_canvas_ai::config::AiConfig;
use open_canvas_ai::runtime_log::{RuntimeLogBuffer, RuntimeLogLevel};

use crate::ai_service::AiService;
use crate::appearance::ThemeLook;
use crate::bars::Bars;
use crate::config_ui::ConfigTab;
use crate::coords::ScreenPos;
use crate::cost::CostLog;
use crate::doc::Doc;
use crate::i18n::Locale;
use crate::image::ImageState;
use crate::library::{EditorMeta, Library};
use crate::node_ui::Panel;
use crate::paths;
use crate::storage_scan::StorageUsage;
use crate::store::{self, Store};
use crate::{
    AppWindow, ConfigChannelRow, CostRecordRow, HoverAction, ImageHistoryRow, ImageLayerRow,
    ImageParamRow, ImageStyleRow, ImageSwatchGroup, ImageToolRow, InfoRow, LibraryGroupRow,
    LocalModelRow, LogRow, NodeItem, ProjectListRow, RulerTick, RulerTickImage, SidePanelRow,
    StorageKindRow,
};

/// P1 canvas area size, corrected by `canvas-resized` once the layout runs.
const DEFAULT_CANVAS: (f64, f64) = (1040.0, 744.0);
/// Grid regeneration hysteresis (zoom factor ratio); owned by `sync/view.rs`.
pub(crate) const GRID_ZOOM_RATIO: f64 = 1.5;
/// History debounce poll interval (`core::History` owns the 180ms window).
///
/// The background model tick is not an interaction throttle: it flushes
/// autosave, expires toasts and polls the AI/plugin/theme services.
/// Interaction (pan, zoom, node drags) is driven by pointer events and the
/// 8 ms frame tick in `main.rs`, so no canvas update waits for this timer.
pub const HISTORY_TICK: Duration = Duration::from_millis(60);
/// Delay before a fresh node animates in (`canvas-node-enter`, 420ms total).
pub const ENTER_DELAY: Duration = Duration::from_millis(30);

/// How long a toast lives before the tick drops it. The web's Ant Design
/// `message` auto-hides after 3s; a status line gets 4s here and a failure 6s,
/// because a failure carries actionable detail worth reading twice.
pub const NOTICE_TTL_MS: u64 = 4_000;
pub const NOTICE_TTL_ERROR_MS: u64 = 6_000;
/// At most this many toasts stack at once. The oldest leaves when a newer one
/// arrives, so a burst of failures can never cover the canvas.
pub const NOTICE_STACK_MAX: usize = 4;
/// How long the canvas zoom read-out stays fully visible after the last zoom
/// change; the Slint side fades it out over 400ms once this deadline passes.
pub const ZOOM_BADGE_TTL_MS: u64 = 900;
/// Physical-pixel movement below which a left background press is a click, not
/// a window drag. Matches the canvas pan's 3 px click/gesture threshold.
pub const WINDOW_CLICK_SLOP: i64 = 3;
/// How long a still right-button press has to be held before it returns to the
/// home screen. The ring fills over this window, Zelda-stamina style.
pub const HOME_HOLD_MS: u64 = 900;
/// The ring stays hidden for the first tenth of the hold, so a quick
/// right-click (the side-panel toggle) never flashes a home HUD.
pub const HOME_HOLD_SHOW_AT: f64 = 0.10;
/// Movement beyond this cancels the hold and becomes a right-drag pan.
pub const HOME_HOLD_SLOP: f64 = 3.0;

/// One active toast: the text, the tone and the wall-clock deadline the tick
/// expires it at. `id` is stable for the lifetime of the toast so the dismiss
/// button can name it (`ToastRow.id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub text: String,
    pub error: bool,
    pub expires_ms: u64,
}

/// One in-flight manual title-bar drag: the cursor position and the window
/// origin read at the previous tick. `drag-tick` applies `cursor`'s delta to
/// `window`, so the move follows the pointer without the OS-modal move loop
/// that used to stall the app's event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowDrag {
    pub cursor: (i32, i32),
    pub window: (i32, i32),
}

/// A still right-button hold on the canvas. The ring fills from
/// `started_ms`; moving past [`HOME_HOLD_SLOP`] cancels it and leaves the
/// ordinary right-drag pan. When it completes, the app returns home.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HomeHold {
    pub started_ms: u64,
    pub pos: ScreenPos,
}

/// SVG path for the Zelda-stamina progress arc in a 24x24 viewbox. The ring
/// starts at 12 o'clock and fills clockwise. `0` is empty; `1` is a full
/// circle (the same path the `Icons.circle` token uses).
pub fn home_hold_arc(progress: f64) -> String {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.001 {
        return String::new();
    }
    if progress >= 0.999 {
        return "M 12 3 A 9 9 0 1 1 11.99 3".to_string();
    }
    let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * progress;
    let x = 12.0 + 9.0 * angle.cos();
    let y = 12.0 + 9.0 * angle.sin();
    let large_arc = u8::from(progress > 0.5);
    format!("M 12 3 A 9 9 0 {large_arc} 1 {x:.2} {y:.2}")
}

/// In-flight assets-tree drag: a real Image/Video node created at the press
/// threshold and moved with the pointer. The node itself is the drag preview;
/// release keeps it, a stack/slot absorbs it, and a drop back on an assets card
/// removes it.
///
/// The source row and the file's absolute path travel with the drag so the
/// canvas-level release hook can resolve the same file when the scrollable tree
/// hands the pointer back before the row sees an `up`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetDrag {
    /// The temporary document node the drag moves.
    pub node_id: String,
    /// Absolute path of the dragged file (validated inside the bound folder).
    pub path: String,
    /// The assets card that owns the dragged row.
    pub source_id: String,
    /// The file path relative to that card's bound folder.
    pub source_path: String,
    /// Latest pointer position in canvas-local screen coordinates.
    pub last: ScreenPos,
    /// Selection before the drag so a cancel leaves the canvas as it was.
    pub previous_selection: std::collections::BTreeSet<String>,
}

/// A coalesced wheel burst: the accumulated delta and the cursor position of
/// the **latest** wheel event, so the tick applies one cursor-anchored zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelZoom {
    pub cursor: ScreenPos,
    pub delta: f64,
}

/// Viewport input waiting for the next interactive frame tick.
///
/// Pointer moves and wheel events used to apply the viewport and sync the whole
/// canvas immediately, so a 1000 Hz mouse produced 1000 `sync_view` calls (and
/// 1000 redraw requests) per second. They now only *record* the latest state;
/// [`crate::app::App::view_tick`] applies it once per 8 ms frame, which caps the
/// canvas at 125 updates/s with exactly one redraw each.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PendingView {
    /// Latest pointer position of an in-flight pan.
    pub pan_to: Option<ScreenPos>,
    /// Accumulated wheel delta and the latest cursor anchor.
    pub wheel: Option<WheelZoom>,
    /// Wall-clock deadline of the deferred cold tier: every input pushes it to
    /// `now + VIEW_COLD_SETTLE_MS`, so it only runs once the gesture settles.
    pub cold_due: Option<u64>,
    /// Earliest time the 60 Hz follow tier may run again.
    pub follow_at: u64,
    /// True while a wheel gesture owns the viewport: the 8 ms tick runs the
    /// hot tier only (scalars + one redraw) until the cold settle pass. Wire
    /// labels, rulers, panels and the node layer are screen-space work that a
    /// wheel burst must not pay for per frame.
    pub hot_only: bool,
}

impl PendingView {
    /// True while a coalesced viewport change has not been applied yet.
    pub fn has_input(&self) -> bool {
        self.pan_to.is_some() || self.wheel.is_some()
    }
}

/// Counters for the frame-budget tests: one viewport application ([`apply`]) and
/// one redraw request ([`redraw`]) per interactive frame, whatever the input
/// event rate was.
///
/// [`apply`]: PendingView
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// Coalesced viewport applications (one per tick at most).
    pub applications: u64,
    /// `request_redraw` calls made by the view tier (one per tick at most).
    pub redraws: u64,
    /// Hot-tier passes (`sync_view_hot`: viewport scalars + one redraw).
    pub hot_passes: u64,
    /// Follow-tier passes (`sync_view_follow`: rulers/overlays).
    pub follow_passes: u64,
    /// Cold-tier passes (`sync_view_cold`: exact settle pass).
    pub cold_passes: u64,
    /// Ruler tick/label-model rebuilds (`sync_rulers`).
    pub ruler_passes: u64,
}

impl ViewStats {
    /// Clears both counters (tests bracket one frame with this).
    #[cfg(test)]
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A node gesture sample waiting for the next interactive frame tick.
///
/// Pointer moves used to apply the drag and sync the dragged rows, every wire
/// and every overlay *per event*. A 1000 Hz mouse therefore rebuilt five
/// `NodeItem`s (media lookups, plugin cells, `SharedString`s), the whole wire
/// model and the screen-space overlays 1000 times per second. Moves now only
/// record the latest pointer position; [`crate::app::App::node_drag_tick`]
/// applies it once per 8 ms frame and updates only what changed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PendingNodeDrag {
    /// Latest pointer position of an in-flight node drag, resize or connect
    /// gesture (`None` once the tick consumed it).
    pub to: Option<ScreenPos>,
}

/// Per-frame node-gesture counters. Tests bracket one drag with a reset and
/// assert the coalescing contract: one redraw per frame, no row rebuild while
/// moving, and only the dragged rows written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DragStats {
    /// Frame ticks that applied a pending node gesture.
    pub frames: u64,
    /// `set_row_data` calls made by the incremental position path.
    pub row_updates: u64,
    /// Row rebuilds (`sync_rows_full` / `sync_rows_of`) during the gesture.
    pub row_rebuilds: u64,
    /// Wire path segments regenerated (incremental: dirty wires only).
    pub wire_paths: u64,
    /// Whole `wires` strings pushed to the window.
    pub wire_pushes: u64,
    /// Redraw requests made by the drag frame itself.
    pub redraws: u64,
}

impl DragStats {
    /// Clears every counter (tests bracket one gesture with this).
    #[cfg(test)]
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The per-connection wire path strings, in document order.
///
/// `ui.wires` is one path command string, so a node drag cannot patch a single
/// command in place. The cache keeps every wire's segment so a drag rebuilds
/// only the segments touching the dragged nodes and joins the rest unchanged;
/// a full sync replaces the whole cache. `connection_ids` guards against a
/// document swap that happens to have the same connection count.
#[derive(Debug, Default)]
pub struct WireCache {
    pub(crate) segments: Vec<(String, String)>,
    pub(crate) connection_ids: Vec<String>,
}

impl WireCache {
    /// True when the cache was built for exactly these connections.
    pub(crate) fn matches(&self, connections: &[open_canvas_core::node::CanvasConnection]) -> bool {
        self.segments.len() == connections.len()
            && self.connection_ids.len() == connections.len()
            && self
                .connection_ids
                .iter()
                .zip(connections)
                .all(|(cached, connection)| cached == &connection.id)
    }
}

/// A media payload decoded off the UI thread, waiting for the drop that uses it.
#[derive(Debug, Clone)]
pub struct PendingMedia {
    pub content: String,
    pub width: u32,
    pub height: u32,
}

/// Physical window origin after a cursor moved from `from_cursor` to
/// `to_cursor`. Pure, so the drag maths is testable without a real cursor.
pub fn drag_target(
    window: (i32, i32),
    from_cursor: (i32, i32),
    to_cursor: (i32, i32),
) -> (i32, i32) {
    (
        window
            .0
            .saturating_add(to_cursor.0.saturating_sub(from_cursor.0)),
        window
            .1
            .saturating_add(to_cursor.1.saturating_sub(from_cursor.1)),
    )
}

/// Which screen the single window shows (web: `/canvas` library vs
/// `/canvas/:id` editor; the web's top nav hides in the editor, here the one
/// top bar stays and its Home button returns to the library).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Editor,
    Image,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "interaction flags (space_held/panning and the overlay toggles) are independent one-bit states, not a mode enum"
)]
pub struct App {
    pub doc: Doc,
    pub view: View,
    pub store: Store,
    pub library: Library,
    pub image: ImageState,
    pub plugin: crate::plugin_host::PluginHost,
    pub media: crate::media::MediaCache,
    /// Canvas media pipeline state: the import prompt + the open image tool
    /// dialog (`image_tools::CanvasMedia`).
    pub canvas_media: crate::image::tools::CanvasMedia,
    pub edit: crate::inline_edit::EditState,
    /// Bound assets-node folder scans (`asset_folder::FolderCache`).
    pub folders: crate::asset_folder::FolderCache,
    /// Per assets node: which folders of its bound tree are expanded.
    pub asset_expanded: std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Last pointer position inside the canvas area (screen coords), used to
    /// place a node dropped from an assets row.
    pub last_canvas_pos: Option<ScreenPos>,
    pub editor: Option<EditorMeta>,
    /// The newest notice as `(text, is_error)` ??a mirror kept for the callers
    /// that only care "is anything being reported" (`config_tab_tests`). The
    /// rendered stack is `notices`.
    pub notice: Option<(String, bool)>,
    /// The active toast stack (oldest first). `set_notice` pushes, the 60ms tick
    /// drops expired entries, the dismiss button removes one by id.
    pub notices: Vec<Notice>,
    pub notice_seq: u64,
    pub save_dirty: bool,
    pub save_due: Option<u64>,
    pub space_held: bool,
    pub panning: bool,
    /// True between a left press on the empty canvas background and its
    /// release. A left press on a node, wire or with Shift (marquee) never
    /// arms it. On release, an unmoved background press clears the selection
    /// (the window move itself is driven from `ui/app.slint`).
    pub canvas_click_pending: bool,
    /// True once the current window drag has actually moved the window by more
    /// than the click threshold. The canvas release reads it to tell a window
    /// drag apart from a click that must deselect.
    pub window_drag_moved: bool,
    /// Window origin captured at `drag_begin`, for the click/drag threshold.
    pub(crate) window_drag_origin: (i32, i32),
    /// Right-button press classification: a plain right-click opens the side
    /// panel, while a right-drag still pans. The press point and movement flag
    /// are reset on every canvas press.
    pub right_click_pending: bool,
    pub right_click_moved: bool,
    pub right_press: Option<ScreenPos>,
    /// False when the press dismissed a popover: that right-click's job was
    /// the popover, not opening the side panel.
    pub right_click_allow_sidebar: bool,
    /// In-flight still right-button hold: the progress ring is drawn while it
    /// is active, and completing it returns to the home screen.
    pub home_hold: Option<HomeHold>,
    pub canvas_size: (f64, f64),
    pub shortcuts_open: bool,
    /// Deadline of the canvas zoom read-out (`ZOOM_BADGE_TTL_MS` after the last
    /// zoom change); `None` while it is hidden. The tick clears it, the pill's
    /// opacity animation owns the fade.
    pub zoom_badge_due: Option<u64>,
    pub drop_target: Option<String>,
    pub drop_slot: Option<String>,
    /// Legacy pointer anchor of a node gesture. It no longer drives a painted
    /// drag ghost: the dragged node itself is the preview. Kept because the
    /// canvas pointer handlers still record it on a press/move.
    pub drag_ghost_pos: Option<ScreenPos>,
    /// In-flight assets-tree drag: the temporary node moved under the pointer
    /// plus the row/file it came from.
    pub asset_drag: Option<AssetDrag>,
    /// Media decoded off the UI thread while the drag was in flight, keyed by
    /// absolute path: releasing over the canvas creates the node from this
    /// payload instead of reading and decoding on the UI thread.
    pub pending_media: HashMap<String, PendingMedia>,
    /// Side panel width (persisted), open flag and the resize anchor
    /// `(start_x, start_width)`.
    pub side_width: f64,
    pub side_open: bool,
    pub side_drag: Option<(f64, f64)>,
    /// Node whose own layer popover is open (the hover bar's `layers` action,
    /// `canvas-node-layer-popover.tsx`).
    pub node_layer: Option<String>,
    pub clipboard: Option<crate::clipboard::NodeClipboard>,
    pub bars: Bars,
    pub panel: Panel,
    pub config: AiConfig,
    pub config_open: bool,
    pub config_tab: ConfigTab,
    /// Appearance + localization state (persisted in `ui.json`). The locale is
    /// fixed at en-US: the product ships one language, so a stale persisted
    /// `zh-CN` is ignored on startup.
    pub theme_look: ThemeLook,
    pub system_dark: bool,
    pub system_theme_checked_ms: u64,
    /// The window plate's opacity (0–100) and the OS backdrop's blur level
    /// (0–100, 0 = no backdrop). Persisted in `ui.json` and applied live by
    /// [`App::glass_setting`].
    pub tint_opacity: f64,
    pub blur: f64,
    /// True once the capture-based glass engine has been adopted. Older
    /// `ui.json` files carry the OS-accent settings (often a near-zero tint
    /// that reads black over a dark desktop); the first launch of this engine
    /// migrates them to the shipped capture defaults once.
    pub glass_capture: bool,
    /// The current whole-desktop capture the frosted backdrop was blurred
    /// from. `None` until the first capture succeeds.
    pub glass_shot: Option<crate::glass_capture::Shot>,
    /// Set by a display change or the settings resample: take one more capture
    /// even when the window has not been idle long enough yet.
    pub glass_recapture: bool,
    /// The blur setting the current image was built with, so moving the slider
    /// re-blurs the stored shot instead of photographing the screen.
    pub glass_blur_at_render: Option<u8>,
    /// The last `(x, y, w, h)` registered with Slint, in logical pixels.
    pub glass_placed: Option<(f32, f32, f32, f32)>,
    /// A capture/blur job is running on a worker thread; never more than one.
    pub glass_pending: bool,
    /// When the window last moved: the backdrop only re-captures once it has
    /// been still long enough.
    pub glass_still_since: Instant,
    /// When the last capture started, so the idle refresh runs at most once
    /// per `GLASS_IDLE_REFRESH`.
    pub glass_last_grab: Instant,
    /// The physical window origin at the previous glass tick.
    pub glass_last_origin: Option<(i32, i32)>,
    pub locale: Locale,
    /// True while the window is maximised (the title bar's restore glyph).
    pub window_maximized: bool,
    /// The Image Attempt currently open in the centered workbench.
    pub attempt_modal: Option<String>,
    /// When set, the next successful attempt generation also extracts its
    /// primary output into a standalone Image node (hover toolbar shortcut).
    pub attempt_auto_export: Option<String>,
    /// In-flight manual title-bar drag: the cursor and window origin of the
    /// previous tick, so each tick applies the cursor delta and the event loop
    /// keeps running (unlike the OS-modal move loop).
    pub window_drag: Option<WindowDrag>,
    /// Runtime log ring buffer (`crates/ai`'s `RuntimeLogBuffer`) + filters.
    pub logs: RuntimeLogBuffer,
    pub logs_filter: String,
    /// The scope/category filter (`all` or an entry's scope).
    pub logs_category: String,
    /// Keys of the rows whose `data` detail is expanded.
    pub logs_expanded: Vec<String>,
    pub logs_version: u64,
    pub logs_shown_version: u64,
    /// Persisted generation-cost records (`costs.json`).
    pub costs: CostLog,
    /// Draft of the Cost tab's generation-id lookup field.
    pub cost_generation_id: String,
    /// Last scan of the data directory (lazy: refreshed when the tab opens).
    pub storage: Option<StorageUsage>,
    pub storage_clear_armed: bool,
    /// Last scan of the models folder + the build's runtime availability.
    pub local_models: crate::local_models::LocalModelsView,
    pub ai: Option<AiService>,
    pub config_channels: Rc<VecModel<ConfigChannelRow>>,
    pub appearance_theme: Rc<VecModel<crate::ConfigOptionRow>>,
    pub local_model_rows: Rc<VecModel<LocalModelRow>>,
    pub storage_rows: Rc<VecModel<StorageKindRow>>,
    pub cost_rows: Rc<VecModel<CostRecordRow>>,
    pub log_rows: Rc<VecModel<LogRow>>,
    pub logs_categories: Rc<VecModel<SharedString>>,
    pub library_groups: Rc<VecModel<LibraryGroupRow>>,
    pub library_projects: Rc<VecModel<ProjectListRow>>,
    pub side_rows: Rc<VecModel<SidePanelRow>>,
    pub image_tools: Rc<VecModel<ImageToolRow>>,
    pub image_layers: Rc<VecModel<ImageLayerRow>>,
    pub image_history: Rc<VecModel<ImageHistoryRow>>,
    pub image_swatches: Rc<VecModel<ImageSwatchGroup>>,
    pub image_params: Rc<VecModel<ImageParamRow>>,
    pub image_styles: Rc<VecModel<ImageStyleRow>>,
    pub startup_error: Option<String>,
    pub(crate) rows: Rc<VecModel<NodeItem>>,
    pub(crate) ruler_x: Rc<VecModel<RulerTick>>,
    pub(crate) ruler_y: Rc<VecModel<RulerTickImage>>,
    pub hover_actions: Rc<VecModel<HoverAction>>,
    pub info_rows: Rc<VecModel<InfoRow>>,
    /// Video-prompt `reference` mode: the editable slot count per node id
    /// (absent = `vm::REFERENCE_SLOT_DEFAULT`, clamped to the stepper range).
    pub video_ref_slots: HashMap<String, usize>,
    pub(crate) grid: paths::GridPatch,
    pub(crate) grid_k: f64,
    /// The pre-rendered dot-grid bitmap and the key it was built for (the
    /// shipped canvas background; `None` until the first cold sync).
    pub(crate) grid_tile: Option<crate::grid_tile::GridTile>,
    /// Draw the dot grid as one world-space `Path` instead of the tiled image
    /// (`OC_LEGACY_GRID=1`). Kept so the perf tests can measure the old
    /// re-tessellation path against the tiled one on the real window.
    pub legacy_grid: bool,
    /// Coalesced viewport input waiting for the next frame tick.
    pub pending: PendingView,
    /// Coalesced node drag / resize / connect input for the frame tick.
    pub pending_drag: PendingNodeDrag,
    /// Per-frame node-gesture counters (frame-budget tests).
    pub drag_stats: DragStats,
    /// Per-connection wire paths, so a drag rebuilds only the dirty ones.
    pub(crate) wire_cache: WireCache,
    /// Per-frame viewport application / redraw counters (frame-budget tests).
    pub view_stats: ViewStats,
    /// Test-only clock: when set, [`App::now_ms`] returns it verbatim, so the
    /// frame-budget tests can drive the 8 ms tick deterministically.
    #[cfg(test)]
    pub(crate) clock_override: Option<u64>,
    pub(crate) base: Instant,
    timers: Vec<Timer>,
}

impl App {
    #[expect(
        clippy::too_many_lines,
        reason = "flat field map of the initial app state; splitting scatters the defaults"
    )]
    pub fn new(store: Store) -> Self {
        let (config, config_error) = match store.load_ai_config() {
            Ok(config) => (config, None),
            Err(error) => (
                AiConfig::default(),
                Some(format!(
                    "AI settings could not be loaded ({error}); using defaults"
                )),
            ),
        };
        let ai = AiService::new();
        let plugin = crate::plugin_host::PluginHost::new(store.plugins_dir());
        let mut app = Self {
            doc: Doc::empty(),
            view: View::Library,
            store,
            library: Library::default(),
            image: crate::image::ImageState::default(),
            plugin,
            media: crate::media::MediaCache::default(),
            canvas_media: crate::image::tools::CanvasMedia::default(),
            edit: crate::inline_edit::EditState::default(),
            folders: crate::asset_folder::FolderCache::default(),
            asset_expanded: std::collections::HashMap::new(),
            last_canvas_pos: None,
            editor: None,
            notice: None,
            notices: Vec::new(),
            notice_seq: 0,
            save_dirty: false,
            save_due: None,
            space_held: false,
            panning: false,
            canvas_click_pending: false,
            window_drag_moved: false,
            window_drag_origin: (0, 0),
            right_click_pending: false,
            right_click_moved: false,
            right_press: None,
            right_click_allow_sidebar: true,
            home_hold: None,
            canvas_size: DEFAULT_CANVAS,
            shortcuts_open: false,
            zoom_badge_due: None,
            drop_target: None,
            drop_slot: None,
            drag_ghost_pos: None,
            asset_drag: None,
            pending_media: HashMap::new(),
            side_width: store::SIDE_PANEL_DEFAULT_WIDTH,
            side_open: true,
            side_drag: None,
            node_layer: None,
            clipboard: None,
            bars: Bars::default(),
            panel: Panel::default(),
            config,
            config_open: false,
            config_tab: ConfigTab::Providers,
            theme_look: ThemeLook::Light,
            system_dark: false,
            system_theme_checked_ms: 0,
            tint_opacity: store::WINDOW_TINT_DEFAULT,
            blur: store::WINDOW_BLUR_DEFAULT,
            glass_capture: false,
            glass_shot: None,
            glass_recapture: false,
            glass_blur_at_render: None,
            glass_placed: None,
            glass_pending: false,
            glass_still_since: Instant::now(),
            glass_last_grab: Instant::now(),
            glass_last_origin: None,
            locale: Locale::EnUs,
            window_maximized: false,
            attempt_modal: None,
            attempt_auto_export: None,
            window_drag: None,
            logs: RuntimeLogBuffer::default(),
            logs_filter: crate::log_view::FILTER_ALL.to_string(),
            logs_category: crate::log_view::CATEGORY_ALL.to_string(),
            logs_expanded: Vec::new(),
            logs_version: 0,
            logs_shown_version: 0,
            costs: CostLog::default(),
            cost_generation_id: String::new(),
            storage: None,
            storage_clear_armed: false,
            local_models: crate::local_models::LocalModelsView::default(),
            ai,
            config_channels: Rc::new(VecModel::from(Vec::new())),
            appearance_theme: Rc::new(VecModel::from(Vec::new())),
            local_model_rows: Rc::new(VecModel::from(Vec::new())),
            storage_rows: Rc::new(VecModel::from(Vec::new())),
            cost_rows: Rc::new(VecModel::from(Vec::new())),
            log_rows: Rc::new(VecModel::from(Vec::new())),
            logs_categories: Rc::new(VecModel::from(Vec::new())),
            library_groups: Rc::new(VecModel::from(Vec::new())),
            library_projects: Rc::new(VecModel::from(Vec::new())),
            side_rows: Rc::new(VecModel::from(Vec::new())),
            image_tools: Rc::new(VecModel::from(Vec::new())),
            image_layers: Rc::new(VecModel::from(Vec::new())),
            image_history: Rc::new(VecModel::from(Vec::new())),
            image_swatches: Rc::new(VecModel::from(Vec::new())),
            image_params: Rc::new(VecModel::from(Vec::new())),
            image_styles: Rc::new(VecModel::from(Vec::new())),
            rows: Rc::new(VecModel::from(Vec::new())),
            ruler_x: Rc::new(VecModel::from(Vec::new())),
            ruler_y: Rc::new(VecModel::from(Vec::new())),
            hover_actions: Rc::new(VecModel::from(Vec::new())),
            info_rows: Rc::new(VecModel::from(Vec::new())),
            video_ref_slots: HashMap::new(),
            startup_error: None,
            grid: paths::GridPatch::default(),
            grid_k: 1.0,
            grid_tile: None,
            // The legacy path is opt-in: the tiled image is the shipped
            // background and the path only exists for the before/after bench.
            legacy_grid: std::env::var_os("OC_LEGACY_GRID").is_some(),
            pending: PendingView::default(),
            pending_drag: PendingNodeDrag::default(),
            drag_stats: DragStats::default(),
            wire_cache: WireCache::default(),
            view_stats: ViewStats::default(),
            #[cfg(test)]
            clock_override: None,
            base: Instant::now(),
            timers: Vec::new(),
        };
        let ui_state = app.store.load_ui_state().unwrap_or_default();
        app.side_width = store::clamp_side_panel_width(ui_state.side_panel_width);
        app.side_open = ui_state.side_panel_open;
        app.theme_look = ThemeLook::from_id(&ui_state.theme);
        app.glass_capture = ui_state.glass_capture;
        if ui_state.glass_capture {
            app.tint_opacity = store::clamp_percent(ui_state.tint_opacity);
            app.blur = store::clamp_percent(ui_state.blur);
        } else {
            // One-time migration from the OS-accent era. Low persisted tint
            // values were chosen against an opaque/dark acrylic fallback and
            // are exactly the "black glass" this engine replaces; start from
            // the asset-library-equivalent frost instead of carrying them
            // forward. The user can still move both sliders afterwards.
            app.tint_opacity = store::WINDOW_TINT_DEFAULT;
            app.blur = store::WINDOW_BLUR_DEFAULT;
            app.glass_capture = true;
            app.persist_ui_state();
        }
        // The product ships en-US only: a persisted language choice from an
        // older build is ignored, not honoured.
        app.locale = Locale::EnUs;
        app.system_dark = crate::appearance::read_system_dark().unwrap_or(false);
        app.costs = app.store.load_costs().unwrap_or_default();
        // Restore the persisted runtime log (newest first on disk): replay it
        // oldest-first so the ring buffer ends up in the same order.
        let loaded_logs = app.store.load_logs().unwrap_or_default();
        for entry in loaded_logs.into_iter().rev() {
            app.logs.push(entry);
        }
        app.local_models = crate::local_models::scan(&app.store.models_dir());
        match app.store.load_index() {
            Ok(index) => {
                app.library.index = index;
                app.repair_groups();
                app.pick_valid_group();
            }
            Err(error) => {
                app.startup_error = Some(format!(
                    "Library could not be loaded ({error}); starting empty"
                ));
            }
        }
        if app.startup_error.is_none() {
            app.startup_error = config_error;
        }
        app
    }

    pub fn now_ms(&self) -> u64 {
        #[cfg(test)]
        if let Some(now) = self.clock_override {
            return now;
        }
        u64::try_from(self.base.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Installs the row models on the window.
    pub fn install(&self, ui: &AppWindow) {
        ui.set_nodes(ModelRc::new(self.rows.clone()));
        ui.set_ruler_x(ModelRc::new(self.ruler_x.clone()));
        ui.set_ruler_y(ModelRc::new(self.ruler_y.clone()));
        ui.set_config_channels(ModelRc::new(self.config_channels.clone()));
        ui.set_appearance_theme(ModelRc::new(self.appearance_theme.clone()));
        ui.set_local_models(ModelRc::new(self.local_model_rows.clone()));
        ui.set_storage_rows(ModelRc::new(self.storage_rows.clone()));
        ui.set_cost_rows(ModelRc::new(self.cost_rows.clone()));
        ui.set_log_rows(ModelRc::new(self.log_rows.clone()));
        ui.set_logs_categories(ModelRc::new(self.logs_categories.clone()));
    }

    /// Appends a runtime-log entry (`crates/ai`'s ring buffer, newest first).
    pub fn log(
        &mut self,
        level: RuntimeLogLevel,
        scope: &str,
        message: impl Into<String>,
        data: Option<String>,
    ) {
        self.logs.record(level, scope, message, data);
        self.logs_version = self.logs_version.wrapping_add(1);
        // Best-effort: log persistence must never block or fail the UI.
        drop(self.store.save_logs(&self.logs));
    }

    /// Keeps a running timer alive for the lifetime of the app.
    pub fn keep_timer(&mut self, timer: Timer) {
        self.timers.retain(Timer::running);
        self.timers.push(timer);
    }

    /// The node currently holding the related/focus highlight: the hovered
    /// node, else the single selected node, never with 2+ selected
    /// (project.tsx:569 `activeNodeId`).
    pub fn focus_node_id(&self) -> Option<String> {
        if self.doc.selection().len() >= 2 {
            return None;
        }
        self.bars
            .hover_node
            .clone()
            .or_else(|| self.doc.selection().iter().next().cloned())
    }

    /// Raises the canvas zoom read-out and re-arms its hide deadline. The
    /// overlay fades in immediately (`zoom-badge-visible`), the tick drops the
    /// flag again after [`ZOOM_BADGE_TTL_MS`], and the Slint `animate opacity`
    /// owns both fades, so an idle canvas runs no animation frames.
    pub fn note_zoom_change(&mut self, ui: &AppWindow) {
        self.zoom_badge_due = Some(self.now_ms().saturating_add(ZOOM_BADGE_TTL_MS));
        // A wheel burst calls this once per 8 ms frame; only the first frame
        // needs the property write (the Slint animation owns the fade).
        if !ui.get_zoom_badge_visible() {
            ui.set_zoom_badge_visible(true);
        }
    }

    /// Hide-deadline tick for the zoom read-out (called from the app tick).
    /// Returns true when the pill was dropped, so callers only re-sync when the
    /// state actually changed.
    pub fn tick_zoom_badge(&mut self, ui: &AppWindow) -> bool {
        let Some(due) = self.zoom_badge_due else {
            return false;
        };
        if self.now_ms() < due {
            return false;
        }
        self.zoom_badge_due = None;
        ui.set_zoom_badge_visible(false);
        true
    }

    /// Marks a freshly created row so its `enter` animation can start.
    pub fn set_enter(&mut self, id: &str, value: f32) {
        for index in 0..self.rows.row_count() {
            let Some(mut row) = self.rows.row_data(index) else {
                continue;
            };
            if row.id == id {
                row.enter = value;
                self.rows.set_row_data(index, row);
                return;
            }
        }
    }

    /// Reports a failure/status line in the UI (never crashes). The toast stacks
    /// (oldest first, capped at [`NOTICE_STACK_MAX`]) and auto-dismisses after
    /// [`NOTICE_TTL_MS`] / [`NOTICE_TTL_ERROR_MS`]; the dismiss button removes
    /// it early.
    pub fn set_notice(&mut self, ui: &AppWindow, text: String, error: bool) {
        let ttl = if error {
            NOTICE_TTL_ERROR_MS
        } else {
            NOTICE_TTL_MS
        };
        self.notice_seq = self.notice_seq.wrapping_add(1);
        self.notices.push(Notice {
            id: self.notice_seq,
            text: text.clone(),
            error,
            expires_ms: self.now_ms().saturating_add(ttl),
        });
        while self.notices.len() > NOTICE_STACK_MAX {
            self.notices.remove(0);
        }
        self.notice = Some((text, error));
        self.sync_notice(ui);
    }

    /// The dismiss button: removes one toast by id (`ToastRow.id`).
    pub fn dismiss_notice(&mut self, ui: &AppWindow, id: i32) {
        let id = u64::try_from(id).unwrap_or(u64::MAX);
        self.notices.retain(|notice| notice.id != id);
        self.notice = self
            .notices
            .last()
            .map(|notice| (notice.text.clone(), notice.error));
        self.sync_notice(ui);
    }

    /// One auto-dismiss tick. Returns true when the stack changed, so the caller
    /// only re-syncs the window when it has to.
    pub fn tick_notices_at(&mut self, now: u64) -> bool {
        let before = self.notices.len();
        self.notices.retain(|notice| notice.expires_ms > now);
        if self.notices.len() == before {
            return false;
        }
        self.notice = self
            .notices
            .last()
            .map(|notice| (notice.text.clone(), notice.error));
        true
    }

    /// The auto-dismiss tick on the real clock (called from the 60ms timer).
    pub fn tick_notices(&mut self, ui: &AppWindow) {
        if self.tick_notices_at(self.now_ms()) {
            self.sync_notice(ui);
        }
    }

    // -----------------------------------------------------------------------
    // Custom window chrome (`window-chrome.slint`)
    // -----------------------------------------------------------------------

    /// Title-bar maximise/restore (button or double-click on the bar): flips the
    /// tracked state and asks the OS window to match. The native style is
    /// re-applied immediately: winit may rewrite `GWL_STYLE` on the transition,
    /// and the frame tick's guard is only the backstop.
    pub fn toggle_maximize(&mut self, ui: &AppWindow) {
        let next = !self.window_maximized;
        self.window_maximized = next;
        ui.set_window_maximized(next);
        ui.window().set_maximized(next);
        crate::refresh_window_chrome(ui);
    }

    /// Title-bar minimise button (no app state involved).
    pub fn minimize_window(ui: &AppWindow) {
        ui.window().set_minimized(true);
        crate::refresh_window_chrome(ui);
    }

    /// Title-bar drag start: remembers where the cursor and the window were.
    /// The pointer's own position comes from Win32, not from Slint's
    /// window-relative coordinates, so it stays valid while the window moves.
    pub fn drag_begin(&mut self, ui: &AppWindow) {
        let Some(cursor) = crate::glass::cursor_pos() else {
            return;
        };
        let position = ui.window().position();
        self.window_drag = Some(WindowDrag {
            cursor,
            window: (position.x, position.y),
        });
        self.window_drag_origin = (position.x, position.y);
        self.window_drag_moved = false;
    }

    /// One pointer move of a title-bar drag: read the cursor, move the window by
    /// its delta, then remember the new cursor. Going through
    /// `Window::set_position` instead of a native modal move loop is what keeps
    /// the 8 ms frame tick alive during the drag.
    pub fn drag_tick(&mut self, ui: &AppWindow) {
        let Some(drag) = self.window_drag else {
            return;
        };
        let Some(cursor) = crate::glass::cursor_pos() else {
            return;
        };
        if cursor == drag.cursor {
            return;
        }
        let position = drag_target(drag.window, drag.cursor, cursor);
        ui.window()
            .set_position(slint::PhysicalPosition::new(position.0, position.1));
        let dx = i64::from(position.0) - i64::from(self.window_drag_origin.0);
        let dy = i64::from(position.1) - i64::from(self.window_drag_origin.1);
        if dx.abs() > WINDOW_CLICK_SLOP || dy.abs() > WINDOW_CLICK_SLOP {
            self.window_drag_moved = true;
        }
        self.window_drag = Some(WindowDrag {
            cursor,
            window: position,
        });
    }

    /// Title-bar drag end: forget the drag anchor (a stale anchor would make the
    /// next drag jump by the distance the cursor travelled in between).
    pub fn drag_end(&mut self) {
        self.window_drag = None;
    }
}

/// Runs `f` with the shared app and returns its result when borrowable.
pub fn with_app_r<R>(app: &Rc<RefCell<App>>, f: impl FnOnce(&mut App) -> R) -> Option<R> {
    app.try_borrow_mut().ok().map(|mut guard| f(&mut guard))
}

/// Runs `f` with the shared app when it is currently borrowable.
pub fn with_app(app: &Rc<RefCell<App>>, f: impl FnOnce(&mut App)) {
    if let Ok(mut guard) = app.try_borrow_mut() {
        f(&mut guard);
    }
}

#[cfg(test)]
#[path = "app_tests/frame_budget_tests.rs"]
mod frame_budget_tests;

#[cfg(test)]
#[path = "app_tests/copy_tests.rs"]
mod copy_tests;

#[cfg(test)]
#[path = "app_tests/no_dim_tests.rs"]
mod no_dim_tests;

#[cfg(test)]
#[path = "app_tests/glass_chrome_tests.rs"]
mod glass_chrome_tests;

#[cfg(test)]
#[path = "app_tests/notice_tests.rs"]
mod notice_tests;

#[cfg(test)]
#[path = "app_tests/window_drag_tests.rs"]
mod window_drag_tests;

#[cfg(test)]
#[path = "app_tests/canvas_input_tests.rs"]
mod canvas_input_tests;

#[cfg(test)]
#[path = "app_tests/view_coalescing_tests.rs"]
mod view_coalescing_tests;

#[cfg(test)]
#[path = "app_tests/drag_coalescing_tests.rs"]
mod drag_coalescing_tests;
