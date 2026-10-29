//! The shared studio dock engine: the arrangeable side/bottom dock areas of the
//! Image Studio, mirroring
//! `web/src/components/canvas/dock/{dock-layout.ts,dock-panel.tsx,dock-tabs.tsx}`
//! (edge sizes, split ratio, tab groups, move-to-edge, reset) plus the tooltip
//! state machine behind `crates/app/ui/components/dock.slint`.
//!
//! Native difference from the web (deliberate): the shipped default opens the
//! studio as a **professional editor** does — tool rail + board only, with
//! **every edge empty**. Panels are not laid out until the user asks for one
//! from the Dock menu (or an in-studio entry point such as a layer's fx
//! button); the first reveal of a panel puts it on its home edge at that
//! edge's natural size. Tab stacking is something the user does by dragging a
//! panel onto another group, and every panel can be closed again from its ×.
//!
//! The split ratio therefore only governs a side edge that holds exactly two
//! groups (the web's own `DOCK_MAX_GROUPS` ceiling); every other count shares
//! the height evenly. Sizes clamp to the web's numbers: sides 168..640, bottom
//! 120..420, split 0.2..0.8.
//!
//! Persistence: `<data-root>\dock.json`, the same data directory and the same
//! app/version-discriminated JSON style as `ui.json` (see [`crate::store`]).
//! Every layout change is written immediately, exactly like the web's
//! `dockSaveLayout` commit path.
//! Owns the persisted dock layout (`DockLayout`) and its tooltip state.

use std::collections::BTreeMap;

use slint::ComponentHandle;

use crate::store::Store;
use crate::AppWindow;

/// Side dock width bounds (`DOCK_MIN_SIZE` / `DOCK_MAX_SIZE`).
pub const DOCK_MIN_SIDE: f64 = 168.0;
pub const DOCK_MAX_SIDE: f64 = 640.0;
/// Bottom dock height bounds (`dockClampSize` bottom branch).
pub const DOCK_MIN_BOTTOM: f64 = 120.0;
pub const DOCK_MAX_BOTTOM: f64 = 420.0;
/// Split ratio bounds (`DOCK_SPLIT_MIN` / `DOCK_SPLIT_MAX`).
pub const DOCK_SPLIT_MIN: f64 = 0.2;
pub const DOCK_SPLIT_MAX: f64 = 0.8;
/// The web's default split (`DOCK_SPLIT_DEFAULT`).
pub const DOCK_SPLIT_DEFAULT: f64 = 0.5;
/// The first tooltip stage: the action's name appears after this hover delay
/// (the web's `AntD` tooltip `mouseEnterDelay=0.2`). The one-line description
/// follows after [`TOOLTIP_DESCRIPTION_DELAY_MS`] more.
pub const TOOLTIP_DELAY_MS: u64 = 200;
/// The second stage: the description joins the name after this much extra
/// hovering (~800ms more).
pub const TOOLTIP_DESCRIPTION_DELAY_MS: u64 = 800;

/// Studio ids the dock file keys layouts by.
pub const STUDIO_IMAGE: &str = "image";

/// A dock edge (`DockEdge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockEdge {
    Left,
    Right,
    Bottom,
}

impl DockEdge {
    /// The persisted / protocol id.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Bottom => "bottom",
        }
    }

    /// Parses a persisted / protocol id.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    /// A side edge (left/right) as opposed to the bottom strip.
    #[must_use]
    pub fn is_side(self) -> bool {
        !matches!(self, Self::Bottom)
    }
}

/// Every edge, in dock order (`DOCK_EDGES`).
pub const DOCK_EDGES: [DockEdge; 3] = [DockEdge::Left, DockEdge::Right, DockEdge::Bottom];

/// One tab group: panel ids in tab order plus the active tab (`DockGroup`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockGroup {
    pub panels: Vec<String>,
    pub active: String,
}

/// Per-edge dock sizes (`sizes`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockSizes {
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
}

/// Per-side stacked-group height share (`split`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockSplit {
    pub left: f64,
    pub right: f64,
}

/// One studio's dock layout: tab groups per edge, edge sizes, side splits.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockLayout {
    pub left: Vec<DockGroup>,
    pub right: Vec<DockGroup>,
    pub bottom: Vec<DockGroup>,
    pub sizes: DockSizes,
    pub split: DockSplit,
}

/// One dock panel: the id the studio binds by and the edge it returns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockPanelDef {
    pub id: &'static str,
    pub edge: DockEdge,
}

/// One studio's panel definitions, default sizes and display labels.
pub struct Studio {
    pub id: &'static str,
    pub defs: &'static [DockPanelDef],
    pub sizes: DockSizes,
    pub labels: &'static [(&'static str, &'static str)],
}

const fn panel(id: &'static str, edge: DockEdge) -> DockPanelDef {
    DockPanelDef { id, edge }
}

/// `PS_DOCK_PANELS` in the studio's column order (right column first, then the
/// left column, then the bottom strip).
pub const IMAGE_DEFS: [DockPanelDef; 13] = [
    panel("layers", DockEdge::Right),
    panel("channels", DockEdge::Right),
    panel("paths", DockEdge::Right),
    panel("fx", DockEdge::Right),
    panel("history", DockEdge::Right),
    panel("actions", DockEdge::Right),
    panel("properties", DockEdge::Left),
    panel("adjustments", DockEdge::Left),
    panel("assets", DockEdge::Left),
    panel("color", DockEdge::Bottom),
    panel("gradients", DockEdge::Bottom),
    panel("patterns", DockEdge::Bottom),
    panel("brushes", DockEdge::Bottom),
];

/// The Image studio's dock-menu labels (the studio's own Dock menu wording).
pub const IMAGE_LABELS: [(&str, &str); 13] = [
    ("layers", "Layers"),
    ("channels", "Channels"),
    ("paths", "Paths"),
    ("fx", "Layer Effects"),
    ("history", "History"),
    ("actions", "Actions"),
    ("properties", "Properties"),
    ("adjustments", "Adjustments"),
    ("assets", "Assets"),
    ("color", "Color"),
    ("gradients", "Gradients"),
    ("patterns", "Patterns"),
    ("brushes", "Brushes"),
];

/// The one studio, in a `const`-friendly table (labels reuse the studio
/// constants so the dock menu and the tab strips always agree).
pub const STUDIOS: [Studio; 1] = [Studio {
    id: STUDIO_IMAGE,
    defs: &IMAGE_DEFS,
    sizes: DockSizes {
        left: 270.0,
        right: 288.0,
        bottom: 178.0,
    },
    labels: &IMAGE_LABELS,
}];

/// Looks a studio up by id.
#[must_use]
pub fn studio(id: &str) -> Option<&'static Studio> {
    STUDIOS.iter().find(|studio| studio.id == id)
}

/// The display label of one panel.
#[must_use]
pub fn label(studio: &Studio, id: &str) -> String {
    studio
        .labels
        .iter()
        .find(|(panel, _)| *panel == id)
        .map_or_else(|| id.to_string(), |(_, label)| (*label).to_string())
}

/// Clamps an edge size into the web's bounds (`dockClampSize`), falling back to
/// the studio default for non-finite input.
#[must_use]
pub fn clamp_size(edge: DockEdge, size: f64, fallback: f64) -> f64 {
    if !size.is_finite() {
        return fallback.round();
    }
    let (min, max) = if edge.is_side() {
        (DOCK_MIN_SIDE, DOCK_MAX_SIDE)
    } else {
        (DOCK_MIN_BOTTOM, DOCK_MAX_BOTTOM)
    };
    size.round().clamp(min, max)
}

/// Clamps a split ratio into `0.2..0.8` (`dockClampSplit`).
#[must_use]
pub fn clamp_split(ratio: f64) -> f64 {
    if ratio.is_finite() {
        ratio.clamp(DOCK_SPLIT_MIN, DOCK_SPLIT_MAX)
    } else {
        DOCK_SPLIT_DEFAULT
    }
}

/// The studio's natural size for one edge (what the first reveal of a panel
/// restores on an empty edge).
#[must_use]
pub fn natural_size(studio: &Studio, edge: DockEdge) -> f64 {
    match edge {
        DockEdge::Left => studio.sizes.left,
        DockEdge::Right => studio.sizes.right,
        DockEdge::Bottom => studio.sizes.bottom,
    }
}

/// The group a pointer position drops into (`dropTarget`): the bottom quarter
/// is the bottom edge, the outer quarters of the two halves are the side edges.
#[must_use]
pub fn drop_target(px: f64, py: f64, width: f64, height: f64) -> Option<(DockEdge, usize)> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    if py > height * 0.75 {
        return Some((DockEdge::Bottom, 0));
    }
    let group = usize::from(py >= height / 2.0);
    if px < width * 0.25 {
        return Some((DockEdge::Left, group));
    }
    if width - px < width * 0.25 {
        return Some((DockEdge::Right, group));
    }
    None
}

impl DockLayout {
    /// The shipped default: every edge empty, the studio opens as tool rail +
    /// board only. Edge sizes still carry the studio's natural numbers, so the
    /// first panel revealed on an edge can occupy its natural footprint.
    #[must_use]
    pub fn default_for(studio: &Studio) -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
            bottom: Vec::new(),
            sizes: studio.sizes,
            split: DockSplit {
                left: DOCK_SPLIT_DEFAULT,
                right: DOCK_SPLIT_DEFAULT,
            },
        }
    }

    /// The groups of one edge.
    #[must_use]
    pub fn groups(&self, edge: DockEdge) -> &Vec<DockGroup> {
        match edge {
            DockEdge::Left => &self.left,
            DockEdge::Right => &self.right,
            DockEdge::Bottom => &self.bottom,
        }
    }

    fn groups_mut(&mut self, edge: DockEdge) -> &mut Vec<DockGroup> {
        match edge {
            DockEdge::Left => &mut self.left,
            DockEdge::Right => &mut self.right,
            DockEdge::Bottom => &mut self.bottom,
        }
    }

    /// One edge's stored size.
    #[must_use]
    pub fn size(&self, edge: DockEdge) -> f64 {
        match edge {
            DockEdge::Left => self.sizes.left,
            DockEdge::Right => self.sizes.right,
            DockEdge::Bottom => self.sizes.bottom,
        }
    }

    /// One side's stacked-group height share.
    #[must_use]
    pub fn split(&self, edge: DockEdge) -> f64 {
        match edge {
            DockEdge::Right => self.split.right,
            _ => self.split.left,
        }
    }

    /// True while the panel sits in some group (`dockIsVisible`).
    #[must_use]
    pub fn is_visible(&self, id: &str) -> bool {
        DOCK_EDGES.iter().any(|edge| {
            self.groups(*edge)
                .iter()
                .any(|group| group.panels.iter().any(|panel| panel == id))
        })
    }

    /// Which edge and group the panel currently sits in.
    #[must_use]
    pub fn locate(&self, id: &str) -> Option<(DockEdge, usize)> {
        for edge in DOCK_EDGES {
            for (index, group) in self.groups(edge).iter().enumerate() {
                if group.panels.iter().any(|panel| panel == id) {
                    return Some((edge, index));
                }
            }
        }
        None
    }

    /// Removes the panel from every group and drops the groups it emptied
    /// (`dockWithoutPanel` + `dockCollapseGroups`).
    fn without_panel(&mut self, id: &str) {
        for edge in DOCK_EDGES {
            let groups = self.groups_mut(edge);
            for group in groups.iter_mut() {
                group.panels.retain(|panel| panel != id);
            }
            groups.retain(|group| !group.panels.is_empty());
            for group in groups.iter_mut() {
                if !group.panels.contains(&group.active) {
                    if let Some(first) = group.panels.first() {
                        group.active.clone_from(first);
                    }
                }
            }
        }
    }

    /// Moves a panel into `edge`'s group `index` (created when it does not
    /// exist yet), making it the active tab (`dockMovePanel`).
    pub fn move_panel(&mut self, id: &str, edge: DockEdge, index: usize) {
        self.without_panel(id);
        let groups = self.groups_mut(edge);
        let index = index.min(groups.len());
        if let Some(group) = groups.get_mut(index) {
            group.panels.push(id.to_string());
            group.active = id.to_string();
        } else {
            groups.push(DockGroup {
                panels: vec![id.to_string()],
                active: id.to_string(),
            });
        }
    }

    /// Shows the panel again: a fresh group at the end of its home edge
    /// (`dockRevealPanel`). A panel revealed onto an empty edge also restores
    /// that edge's natural size, so a closed-back studio reopens at the
    /// editor's proportions rather than at a stale drag size.
    pub fn reveal(&mut self, studio: &Studio, id: &str) {
        let Some(def) = studio.defs.iter().find(|def| def.id == id) else {
            return;
        };
        let first_on_edge = self.groups(def.edge).is_empty();
        self.without_panel(id);
        if first_on_edge {
            self.set_size(def.edge, natural_size(studio, def.edge));
        }
        self.groups_mut(def.edge).push(DockGroup {
            panels: vec![id.to_string()],
            active: id.to_string(),
        });
    }

    /// Closes a visible panel: it leaves every group, groups it emptied are
    /// dropped, and it stays available in the Dock menu. Returns whether the
    /// layout changed.
    pub fn close(&mut self, id: &str) -> bool {
        if !self.is_visible(id) {
            return false;
        }
        self.without_panel(id);
        true
    }

    /// Hides a visible panel / reveals a hidden one (`dockTogglePanel`).
    pub fn toggle(&mut self, studio: &Studio, id: &str) {
        if self.is_visible(id) {
            self.without_panel(id);
        } else {
            self.reveal(studio, id);
        }
    }

    /// Activates a tab inside one group (`dockSetActive`); returns whether the
    /// layout changed.
    pub fn set_active(&mut self, edge: DockEdge, index: usize, id: &str) -> bool {
        let groups = self.groups_mut(edge);
        let Some(group) = groups.get_mut(index) else {
            return false;
        };
        if !group.panels.iter().any(|panel| panel == id) || group.active == id {
            return false;
        }
        group.active = id.to_string();
        true
    }

    /// Stores a clamped edge size (`dockSetSize`); returns whether it changed.
    pub fn set_size(&mut self, edge: DockEdge, size: f64) -> bool {
        let fallback = self.size(edge);
        let next = clamp_size(edge, size, fallback);
        let slot = match edge {
            DockEdge::Left => &mut self.sizes.left,
            DockEdge::Right => &mut self.sizes.right,
            DockEdge::Bottom => &mut self.sizes.bottom,
        };
        if (*slot - next).abs() < f64::EPSILON {
            return false;
        }
        *slot = next;
        true
    }

    /// Stores a clamped split ratio (`dockSetSplit`); returns whether it
    /// changed.
    pub fn set_split(&mut self, edge: DockEdge, ratio: f64) -> bool {
        let next = clamp_split(ratio);
        let slot = if edge == DockEdge::Right {
            &mut self.split.right
        } else {
            &mut self.split.left
        };
        if (*slot - next).abs() < f64::EPSILON {
            return false;
        }
        *slot = next;
        true
    }

    /// Validates a persisted payload against the studio: unknown and duplicate
    /// panels are dropped, empty groups collapse, sizes and splits clamp. A
    /// panel missing from every group stays hidden (`dockLoadLayout`).
    #[must_use]
    pub fn sanitize(mut self, studio: &Studio) -> Self {
        let known: Vec<&str> = studio.defs.iter().map(|def| def.id).collect();
        let mut seen: Vec<String> = Vec::new();
        for edge in DOCK_EDGES {
            let groups = self.groups_mut(edge);
            let mut cleaned: Vec<DockGroup> = Vec::new();
            for group in groups.drain(..) {
                let mut panels: Vec<String> = Vec::new();
                for id in group.panels {
                    if known.contains(&id.as_str()) && !seen.contains(&id) {
                        seen.push(id.clone());
                        panels.push(id);
                    }
                }
                if panels.is_empty() {
                    continue;
                }
                let active = if panels.contains(&group.active) {
                    group.active
                } else {
                    panels.first().cloned().unwrap_or_default()
                };
                cleaned.push(DockGroup { panels, active });
            }
            *groups = cleaned;
        }
        self.sizes.left = clamp_size(DockEdge::Left, self.sizes.left, studio.sizes.left);
        self.sizes.right = clamp_size(DockEdge::Right, self.sizes.right, studio.sizes.right);
        self.sizes.bottom = clamp_size(DockEdge::Bottom, self.sizes.bottom, studio.sizes.bottom);
        self.split.left = clamp_split(self.split.left);
        self.split.right = clamp_split(self.split.right);
        self
    }
}

/// `dock.json`'s discriminator and schema version (the `ui.json` style).
pub const DOCK_FILE_APP: &str = "open-canvas-desktop-dock";
pub const DOCK_FILE_VERSION: u32 = 1;

/// The dock preferences file: one layout per studio id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockFile {
    pub app: String,
    pub version: u32,
    #[serde(default)]
    pub layouts: BTreeMap<String, DockLayout>,
}

impl Default for DockFile {
    fn default() -> Self {
        Self {
            app: DOCK_FILE_APP.to_string(),
            version: DOCK_FILE_VERSION,
            layouts: BTreeMap::new(),
        }
    }
}

/// The untouched layout of one studio (an empty layout for an unknown id).
#[must_use]
pub fn default_layout(studio_id: &str) -> DockLayout {
    studio(studio_id).map_or_else(
        || DockLayout {
            left: Vec::new(),
            right: Vec::new(),
            bottom: Vec::new(),
            sizes: DockSizes {
                left: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
            split: DockSplit {
                left: DOCK_SPLIT_DEFAULT,
                right: DOCK_SPLIT_DEFAULT,
            },
        },
        DockLayout::default_for,
    )
}

/// Reads the persisted layout of one studio; a missing, foreign or malformed
/// file (and any single malformed layout) falls back to the default so a broken
/// state file can never block a studio.
#[must_use]
pub fn load(store: &Store, studio_id: &str) -> DockLayout {
    let Some(studio) = studio(studio_id) else {
        return default_layout(studio_id);
    };
    store
        .load_dock_file()
        .ok()
        .and_then(|file| file.layouts.get(studio_id).cloned())
        .map_or_else(
            || DockLayout::default_for(studio),
            |layout| layout.sanitize(studio),
        )
}

/// Writes one studio's layout back, preserving the other studios' entries.
/// Storage failures must never break the studio (the layout simply stops
/// persisting), matching `dockSaveLayout`.
pub fn save(store: &Store, studio_id: &str, layout: &DockLayout) {
    let mut file = store.load_dock_file().unwrap_or_default();
    file.app = DOCK_FILE_APP.to_string();
    file.version = DOCK_FILE_VERSION;
    file.layouts.insert(studio_id.to_string(), layout.clone());
    if let Err(error) = store.save_dock_file(&file) {
        eprintln!("dock: could not persist the {studio_id} layout: {error}");
    }
}

/// The studio whose dock the request belongs to, as an owned `Studio` handle.
fn studio_for(id: &str) -> Option<&'static Studio> {
    studio(id)
}

/// One panel of a Slint-side tab group.
#[must_use]
pub fn slint_group(studio: &Studio, group: &DockGroup) -> crate::DockTabGroup {
    let labels: Vec<slint::SharedString> = group
        .panels
        .iter()
        .map(|id| slint::SharedString::from(label(studio, id)))
        .collect();
    let panels: Vec<slint::SharedString> = group
        .panels
        .iter()
        .map(|id| slint::SharedString::from(id.as_str()))
        .collect();
    crate::DockTabGroup {
        panels: slint::ModelRc::new(slint::VecModel::from(panels)),
        labels: slint::ModelRc::new(slint::VecModel::from(labels)),
        active: slint::SharedString::from(group.active.as_str()),
    }
}

/// One edge's groups as a Slint model.
#[must_use]
pub fn slint_groups(studio: &Studio, groups: &[DockGroup]) -> slint::ModelRc<crate::DockTabGroup> {
    slint::ModelRc::new(slint::VecModel::from(
        groups
            .iter()
            .map(|group| slint_group(studio, group))
            .collect::<Vec<_>>(),
    ))
}

/// The whole layout as the Slint state struct.
#[must_use]
pub fn slint_layout(studio: &Studio, layout: &DockLayout) -> crate::DockLayoutState {
    crate::DockLayoutState {
        left: slint_groups(studio, &layout.left),
        right: slint_groups(studio, &layout.right),
        bottom: slint_groups(studio, &layout.bottom),
        left_size: crate::vm::len(layout.sizes.left),
        right_size: crate::vm::len(layout.sizes.right),
        bottom_size: crate::vm::len(layout.sizes.bottom),
        left_split: crate::vm::len32(layout.split(DockEdge::Left)),
        right_split: crate::vm::len32(layout.split(DockEdge::Right)),
    }
}

/// The layout's panel visibility as the studio's legacy `panels` flags (the
/// Dock menu check marks and every binding that still reads one). Derived, so
/// the flags can never disagree with the layout.
#[must_use]
pub fn panels_for(layout: &DockLayout) -> crate::image::ImagePanels {
    let visible = |id: &str| layout.is_visible(id);
    crate::image::ImagePanels {
        layers: visible("layers"),
        fx: visible("fx"),
        history: visible("history"),
        properties: visible("properties"),
        color: visible("color"),
        channels: visible("channels"),
        paths: visible("paths"),
        actions: visible("actions"),
        assets: visible("assets"),
        adjustments: visible("adjustments"),
        gradients: visible("gradients"),
        patterns: visible("patterns"),
        brushes: visible("brushes"),
    }
}

/// The tooltip state machine (`crates/app/ui/components/dock.slint` renders it;
/// the web's `AntD` tooltip waits `mouseEnterDelay=0.2` before showing and hides
/// at once on leave; a disabled target never shows one). Two stages: the name
/// first, then the one-line description if the pointer stays
/// [`TOOLTIP_DESCRIPTION_DELAY_MS`] longer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TooltipState {
    text: String,
    description: String,
    x: f64,
    y: f64,
    /// Hover started at this app-clock millisecond; `None` while not hovering.
    since: Option<u64>,
    visible: bool,
    /// The second stage: the description is showing under the name.
    detail: bool,
}

impl TooltipState {
    /// Hover starts over a target. A disabled target (or empty name) clears
    /// the tooltip and never shows one; a target already hovered keeps its
    /// timer. `description` is the one-line second-stage text ("" = none).
    pub fn hover(
        &mut self,
        text: &str,
        description: &str,
        x: f64,
        y: f64,
        enabled: bool,
        now_ms: u64,
    ) {
        if !enabled || text.is_empty() {
            self.leave();
            return;
        }
        if self.text != text {
            self.since = Some(now_ms);
            self.visible = false;
            self.detail = false;
        } else if self.since.is_none() {
            self.since = Some(now_ms);
        }
        self.text = text.to_string();
        self.description = description.to_string();
        self.x = x;
        self.y = y;
    }

    /// The pointer left the target: hide immediately.
    pub fn leave(&mut self) {
        self.since = None;
        self.visible = false;
        self.detail = false;
    }

    /// Advances the hover delays; returns whether the painted state changed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        let Some(since) = self.since else {
            return false;
        };
        let elapsed = now_ms.saturating_sub(since);
        let visible = elapsed >= TOOLTIP_DELAY_MS;
        let detail = visible
            && !self.description.is_empty()
            && elapsed >= TOOLTIP_DELAY_MS.saturating_add(TOOLTIP_DESCRIPTION_DELAY_MS);
        if visible == self.visible && detail == self.detail {
            return false;
        }
        self.visible = visible;
        self.detail = detail;
        true
    }

    /// The label to paint, empty while hidden.
    #[must_use]
    pub fn visible_text(&self) -> &str {
        if self.visible {
            &self.text
        } else {
            ""
        }
    }

    /// The description to paint under the label, empty during the first stage.
    #[must_use]
    pub fn visible_description(&self) -> &str {
        if self.visible && self.detail {
            &self.description
        } else {
            ""
        }
    }

    /// Whether the tooltip is on screen.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// The window-space anchor the tip hangs off.
    #[must_use]
    pub fn anchor(&self) -> (f64, f64) {
        (self.x, self.y)
    }
}

/// Parses the registry's `MinAnimate` value (`"    MinAnimate    REG_SZ    0"`):
/// `0` means the OS asks for reduced motion. Anything unparsable is `None`.
#[must_use]
pub fn parse_min_animate(text: &str) -> Option<bool> {
    let value = text.split_whitespace().last()?;
    let parsed = match value.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => value.parse::<u64>().ok()?,
    };
    Some(parsed == 0)
}

/// Whether the OS asks for reduced motion ("Show animations in Windows",
/// `HKCU\Control Panel\Desktop\WindowMetrics\MinAnimate`). Read once and cached:
/// the setting does not change while the app runs, and the read spawns a
/// windowless `reg query` exactly like `appearance::read_system_dark`.
#[must_use]
pub fn reduced_motion() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(read_min_animate)
}

#[cfg(windows)]
fn read_min_animate() -> bool {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parse_min_animate(&String::from_utf8_lossy(&output.stdout)).unwrap_or(false)
        }
        _ => false,
    }
}

#[cfg(not(windows))]
fn read_min_animate() -> bool {
    false
}

/// Splits an `"edge|index|panel"` / `"panel"` request argument.
fn request_parts(arg: &str) -> Vec<&str> {
    arg.split('|').collect()
}

impl crate::app::App {
    /// One dock/tooltip request from `crates/app/ui/components/dock.slint`
    /// (`DockStore.dock-request`): resize, split, activate, move, toggle, reset,
    /// drop and the tooltip hover pair. Every layout change persists and
    /// re-renders the owning studio immediately (the web's commit path).
    #[expect(
        clippy::too_many_arguments,
        reason = "one wire payload (studio, op, argument, detail, a, b) plus the window"
    )]
    pub fn dock_request(
        &mut self,
        ui: &AppWindow,
        studio_id: &str,
        op: &str,
        arg: &str,
        detail: &str,
        a: f64,
        b: f64,
    ) {
        if op.starts_with("tip.") {
            self.dock_tooltip_request(ui, studio_id, op, arg, detail, a, b);
            return;
        }
        let Some(studio) = studio_for(studio_id) else {
            return;
        };
        let parts = request_parts(arg);
        match op {
            "resize" => {
                let Some(edge) = parts.first().and_then(|id| DockEdge::from_id(id)) else {
                    return;
                };
                let layout = self.dock_layout_mut(studio_id);
                if layout.set_size(edge, a) {
                    let layout = layout.clone();
                    self.dock_commit(ui, studio, &layout);
                }
            }
            "split" => {
                let Some(edge) = parts.first().and_then(|id| DockEdge::from_id(id)) else {
                    return;
                };
                if !edge.is_side() {
                    return;
                }
                let layout = self.dock_layout_mut(studio_id);
                if layout.set_split(edge, a) {
                    let layout = layout.clone();
                    self.dock_commit(ui, studio, &layout);
                }
            }
            "activate" => {
                let (Some(edge), Some(index), Some(id)) = (
                    parts.first().and_then(|id| DockEdge::from_id(id)),
                    parts.get(1).and_then(|index| index.parse::<usize>().ok()),
                    parts.get(2).copied(),
                ) else {
                    return;
                };
                let layout = self.dock_layout_mut(studio_id);
                if layout.set_active(edge, index, id) {
                    let layout = layout.clone();
                    self.dock_commit(ui, studio, &layout);
                }
            }
            "move" => {
                let (Some(edge), Some(index), Some(id)) = (
                    parts.first().and_then(|id| DockEdge::from_id(id)),
                    parts.get(1).and_then(|index| index.parse::<usize>().ok()),
                    parts.get(2).copied(),
                ) else {
                    return;
                };
                let layout = self.dock_layout_mut(studio_id);
                // A drop onto the group the panel already sits in changes
                // nothing; skip the commit (and the write) entirely.
                if layout.locate(id) == Some((edge, index)) {
                    return;
                }
                layout.move_panel(id, edge, index);
                let layout = layout.clone();
                self.dock_commit(ui, studio, &layout);
            }
            "toggle" => {
                if let Some(id) = parts.first().copied() {
                    self.dock_toggle_panel(ui, studio_id, id);
                }
            }
            "close" => {
                if let Some(id) = parts.first().copied() {
                    self.dock_close_panel(ui, studio_id, id);
                }
            }
            "reset" => self.dock_reset(ui, studio_id),
            "drag-start" => {
                let id = parts.first().copied().unwrap_or_default();
                self.dock_drag(studio_id, id, a, b, false);
                self.sync_dock(ui, studio_id);
            }
            "drag-move" => {
                let id = self.dock_drag_panel(studio_id);
                self.dock_drag(studio_id, &id, a, b, false);
                self.sync_dock(ui, studio_id);
            }
            "drag-end" => {
                let id = self.dock_drag_panel(studio_id);
                let size = self.dock_area_size();
                let target = drop_target(a, b, size.0, size.1);
                self.dock_drag(studio_id, "", 0.0, 0.0, true);
                if let Some((edge, index)) = target {
                    let layout = self.dock_layout_mut(studio_id);
                    layout.move_panel(&id, edge, index);
                    let layout = layout.clone();
                    self.dock_commit(ui, studio, &layout);
                } else {
                    self.sync_dock(ui, studio_id);
                }
            }
            _ => {}
        }
    }

    /// Resets one studio's dock layout to the untouched default (the Dock
    /// menu's "Reset Layout").
    pub fn dock_reset(&mut self, ui: &AppWindow, studio_id: &str) {
        let Some(studio) = studio(studio_id) else {
            return;
        };
        let layout = DockLayout::default_for(studio);
        *self.dock_layout_mut(studio_id) = layout.clone();
        self.dock_commit(ui, studio, &layout);
    }

    /// Hides a visible panel / reveals a hidden one (the Dock menu's rows and
    /// the drop targets). An unknown panel id is ignored.
    pub fn dock_toggle_panel(&mut self, ui: &AppWindow, studio_id: &str, id: &str) {
        let Some(studio) = studio(studio_id) else {
            return;
        };
        if !studio.defs.iter().any(|def| def.id == id) {
            return;
        }
        let layout = self.dock_layout_mut(studio_id);
        layout.toggle(studio, id);
        let layout = layout.clone();
        self.dock_commit(ui, studio, &layout);
    }

    /// Reveals a panel on its home edge (no-op when it is already visible).
    /// The in-studio entry points (a layer row's fx button) use this so they
    /// still open their panel now that the shipped layout starts empty.
    pub fn dock_reveal_panel(&mut self, ui: &AppWindow, studio_id: &str, id: &str) {
        let Some(studio) = studio(studio_id) else {
            return;
        };
        if !studio.defs.iter().any(|def| def.id == id) {
            return;
        }
        let layout = self.dock_layout_mut(studio_id);
        if layout.is_visible(id) {
            return;
        }
        layout.reveal(studio, id);
        let layout = layout.clone();
        self.dock_commit(ui, studio, &layout);
    }

    /// Closes a visible panel (the × on its tab / grip bar): it leaves the
    /// layout, the groups it emptied collapse, and it stays in the Dock menu.
    /// An unknown or already closed panel id is ignored.
    pub fn dock_close_panel(&mut self, ui: &AppWindow, studio_id: &str, id: &str) {
        let Some(studio) = studio(studio_id) else {
            return;
        };
        if !studio.defs.iter().any(|def| def.id == id) {
            return;
        }
        let layout = self.dock_layout_mut(studio_id);
        if !layout.close(id) {
            return;
        }
        let layout = layout.clone();
        self.dock_commit(ui, studio, &layout);
    }

    /// The tooltip half of [`Self::dock_request`].
    #[expect(
        clippy::too_many_arguments,
        reason = "same wire payload as dock_request; splitting obscures the mapping"
    )]
    fn dock_tooltip_request(
        &mut self,
        ui: &AppWindow,
        studio_id: &str,
        op: &str,
        text: &str,
        detail: &str,
        a: f64,
        b: f64,
    ) {
        let now = self.now_ms();
        // Hovering a new target hides whatever tooltip was up elsewhere, so at
        // most one studio's tooltip is ever on screen.
        if studio_id != STUDIO_IMAGE {
            self.image.tooltip.leave();
        }
        let state = self.dock_tooltip_mut(studio_id);
        if op == "tip.hide" {
            state.leave();
        } else {
            state.hover(text, detail, a, b, true, now);
        }
        self.sync_tooltip(ui);
    }

    /// The studio's live dock layout.
    fn dock_layout_mut(&mut self, _studio_id: &str) -> &mut DockLayout {
        &mut self.image.dock
    }

    /// The studio's tooltip state.
    fn dock_tooltip_mut(&mut self, _studio_id: &str) -> &mut TooltipState {
        &mut self.image.tooltip
    }

    /// The panel currently being dragged on a studio's dock ("" = none).
    fn dock_drag_panel(&self, studio_id: &str) -> String {
        if self.image.dock_drag.0 == studio_id {
            return self.image.dock_drag.1.clone();
        }
        String::new()
    }

    /// Records (or clears) the in-flight panel drag shared with the Slint drop
    /// overlay.
    fn dock_drag(&mut self, studio_id: &str, id: &str, x: f64, y: f64, clear: bool) {
        let slot = &mut self.image.dock_drag;
        if clear {
            *slot = (String::new(), String::new(), 0.0, 0.0);
        } else {
            *slot = (studio_id.to_string(), id.to_string(), x, y);
        }
    }

    /// The window size the drop zones are measured against (the studios fill
    /// the window).
    fn dock_area_size(&self) -> (f64, f64) {
        (self.canvas_size.0.max(1.0), self.canvas_size.1.max(1.0))
    }

    /// Applies a layout change: derive the legacy panel flags, persist, render.
    fn dock_commit(&mut self, ui: &AppWindow, studio: &'static Studio, layout: &DockLayout) {
        save(&self.store, studio.id, layout);
        *self.dock_dirty_mut(studio.id) = false;
        self.sync_dock(ui, studio.id);
    }

    /// The studio's "the window still needs the new layout" flag.
    fn dock_dirty_mut(&mut self, _studio_id: &str) -> &mut bool {
        &mut self.image.dock_dirty
    }

    /// Pushes one studio's dock layout, the shared drag state and the derived
    /// panel flags to the window.
    pub fn sync_dock(&mut self, ui: &AppWindow, studio_id: &str) {
        let Some(studio) = studio_for(studio_id) else {
            return;
        };
        let layout = self.dock_layout_mut(studio_id).clone();
        let state = slint_layout(studio, &layout);
        let theme = ui.global::<crate::Theme>();
        // Reduced motion is a theme-wide flag the dock chrome (and the modal
        // reveal) honour; it is read once from the OS.
        theme.set_reduced_motion(reduced_motion());
        theme.set_dock_image(state);
        let drag = self.image.dock_drag.clone();
        theme.set_dock_drag_studio(slint::SharedString::from(drag.0.as_str()));
        theme.set_dock_drag_panel(slint::SharedString::from(drag.1.as_str()));
        theme.set_dock_drag_label(slint::SharedString::from(label(studio, &drag.1).as_str()));
        theme.set_dock_drag_x(crate::vm::len(drag.2));
        theme.set_dock_drag_y(crate::vm::len(drag.3));
        let target = if drag.1.is_empty() {
            String::new()
        } else {
            let size = self.dock_area_size();
            drop_target(drag.2, drag.3, size.0, size.1).map_or_else(String::new, |(edge, index)| {
                format!("{}|{index}", edge.id())
            })
        };
        theme.set_dock_drag_target(slint::SharedString::from(target.as_str()));
        self.sync_dock_panels(ui, studio_id, &layout);
    }

    /// The studio's legacy panel visibility flags, derived from the layout so
    /// the existing `panels` bindings (dock menu check marks, column presence)
    /// stay in step with the dock engine.
    fn sync_dock_panels(&mut self, ui: &AppWindow, _studio_id: &str, layout: &DockLayout) {
        self.image.panels = panels_for(layout);
        ui.set_image_panels(crate::ImagePanels {
            layers: self.image.panels.layers,
            fx: self.image.panels.fx,
            history: self.image.panels.history,
            properties: self.image.panels.properties,
            color: self.image.panels.color,
            channels: self.image.panels.channels,
            paths: self.image.panels.paths,
            actions: self.image.panels.actions,
            assets: self.image.panels.assets,
            adjustments: self.image.panels.adjustments,
            gradients: self.image.panels.gradients,
            patterns: self.image.panels.patterns,
            brushes: self.image.panels.brushes,
        });
    }

    /// Pushes the visible tooltip (whichever studio owns it) to the shared
    /// tooltip host.
    pub fn sync_tooltip(&mut self, ui: &AppWindow) {
        let state = &self.image.tooltip;
        let (text, description, x, y) = if state.is_visible() {
            let (x, y) = state.anchor();
            (
                state.visible_text().to_string(),
                state.visible_description().to_string(),
                x,
                y,
            )
        } else {
            (String::new(), String::new(), 0.0, 0.0)
        };
        ui.global::<crate::Theme>()
            .set_tooltip(crate::TooltipState {
                text: slint::SharedString::from(text.as_str()),
                description: slint::SharedString::from(description.as_str()),
                visible: !text.is_empty(),
                x: crate::vm::len(x),
                y: crate::vm::len(y),
            });
    }

    /// The 60 ms tick: the tooltip hover delay (the dock layouts themselves are
    /// event-driven).
    pub fn tick_dock(&mut self, ui: &AppWindow) {
        let now = self.now_ms();
        if self.image.tooltip.tick(now) {
            self.sync_tooltip(ui);
        }
    }
}

#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dock_render_tests.rs"]
mod render_tests;
