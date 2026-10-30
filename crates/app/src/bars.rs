//! Floating bars and modals: the hover toolbar, the selection toolbar, the node
//! info modal and the image preview. Geometry mirrors
//! `canvas-floating-toolbar.tsx`, `canvas-selection-toolbar.tsx`,
//! `canvas-node-hover-toolbar.tsx` and the preview modal in `project.tsx`.

use std::time::Instant;

use open_canvas_core::geometry::{is_node_hidden, Bounds};
use open_canvas_core::node::{CanvasNodeData, CanvasNodeStatus, CanvasNodeType};

use crate::app::App;
use crate::handlers::ActiveKind;
use crate::overlay;
use crate::vm;
use crate::AppWindow;
/// Selection box padding (canvas-selection-toolbar.tsx:18).
const SELECTION_PAD: f64 = 14.0;
/// Selection box corner radius and dash pattern (`rx/ry 16`, `7 5`).
const SELECTION_RADIUS: f64 = 16.0;
const SELECTION_DASH: (f64, f64) = (7.0, 5.0);

/// Which selection popover is open above the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Align,
    Layout,
}

impl Menu {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "align" => Some(Self::Align),
            "layout" => Some(Self::Layout),
            _ => None,
        }
    }
}

/// Toolbar/modal state (web: `toolbarNodeId`, `infoNodeId`, `previewNodeId`,
/// the selection menu and the hover-keep flag).
#[derive(Debug, Default)]
pub struct Bars {
    pub hover_node: Option<String>,
    /// The node the bar last rendered; freezing it here is what lets the bar
    /// keep itself alive after the pointer leaves the node (the grace timer may
    /// win the race, so `bar_hover` cannot recompute the node by then).
    pub last_node: Option<String>,
    /// When the pointer left the node or the bar; the bar stays frozen on
    /// `last_node` until this deadline (hover intent across the 14px gap).
    pub hide_at: Option<Instant>,
    /// The pointer is inside the bar's hover region.
    pub bar_hover: bool,
    /// A bar button is currently pressed; the bar must survive until release.
    pub bar_press: bool,
    pub info_node: Option<String>,
    pub info_json: bool,
    pub preview_node: Option<String>,
    pub menu: Option<Menu>,
}

impl Bars {
    /// True while the pointer is over or pressed on the bar: the action model
    /// must not be rebuilt and the bar must not be hidden (`canvas-node-hover-toolbar`
    /// `onKeep`; a rebuilt repeater would destroy an in-flight click).
    pub fn interacting(&self) -> bool {
        self.bar_hover || self.bar_press
    }
}

/// The node the hover toolbar follows: the hovered node, else the single
/// selected node — never with 2+ selected, never while a gesture runs and never
/// while a modal is open (project.tsx:545-546, 1997). While the pointer is on or
/// pressed over the bar the frozen [`Bars::last_node`] wins.
pub fn toolbar_node_id(app: &App) -> Option<String> {
    if app.bars.info_node.is_some()
        || app.bars.preview_node.is_some()
        || app.canvas_media.tool.is_some()
        || app.panel.image_ai.is_some()
    {
        return None;
    }
    match app.active_kind() {
        ActiveKind::None => {}
        ActiveKind::Pan
        | ActiveKind::Nodes
        | ActiveKind::Marquee
        | ActiveKind::Connect
        | ActiveKind::Resize
        | ActiveKind::Cut => {
            return None;
        }
    }
    if app.bars.interacting() {
        return app.bars.last_node.clone();
    }
    if app.doc.selection().len() >= 2 {
        return None;
    }
    app.bars
        .hover_node
        .clone()
        .or_else(|| app.doc.selection().iter().next().cloned())
}

/// Hover-toolbar actions (canvas-node-hover-toolbar.tsx:136-176): the
/// content-independent set, the plugin-contributed items spliced in after
/// `duplicate` (`buildNodeToolbarItems` in `use-plugin-host.tsx:128`), the
/// LOCAL image quick tools of image nodes with content, the text font
/// steppers, then the layer popover and delete. An image node is display-only:
/// it never offers an upload, and an empty image node has no affordance at all.
/// The glyphs of the new ids come from the `HoverIcons` global in
/// `floating.slint`.
pub fn hover_action_rows_with(
    node: &CanvasNodeData,
    plugin_extras: &[crate::HoverAction],
) -> Vec<crate::HoverAction> {
    let action = |id: &str, danger: bool| crate::HoverAction {
        id: id.into(),
        danger,
        active: false,
    };
    let active_action = |id: &str, active: bool| crate::HoverAction {
        id: id.into(),
        danger: false,
        active,
    };
    let is_image = node.node_type == CanvasNodeType::Image;
    let has_content = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.as_ref())
        .is_some_and(|content| !content.is_empty());
    let mut actions = vec![action("info", false), action("duplicate", false)];
    actions.extend(plugin_extras.iter().cloned());
    if is_image && has_content {
        // Download/Copy live in the focused image viewer, not on the canvas
        // card's hover toolbar.
        actions.push(action("crop", false));
        actions.push(action("mask", false));
        actions.push(action("split", false));
        actions.push(action("resolution", false));
        actions.push(action("analyze", false));
        // AI-backed tools (`image_ai`): OCR, multi-angle and segmentation.
        actions.push(action("ocr", false));
        actions.push(action("segment", false));
        actions.push(action("angle", false));
        let free_resize = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.free_resize)
            .unwrap_or(false);
        actions.push(active_action("free-resize", free_resize));
    }
    if node.node_type == CanvasNodeType::ImageAttempt {
        // One-click shortcut: run the attempt and extract its first output into
        // a standalone Image node, without opening the workbench.
        actions.push(action("attempt-generate", false));
    }
    if node.node_type == CanvasNodeType::Text {
        actions.push(action("font-down", false));
        actions.push(action("font-up", false));
    }
    actions.push(action("layers", false));
    actions.push(action("delete", true));
    actions
}

/// Text font-size steps (project.tsx:1997-1998): ±2, clamped to 10..=32.
pub fn font_step(current: f64, increase: bool) -> f64 {
    if increase {
        (current + 2.0).min(32.0)
    } else {
        (current - 2.0).max(10.0)
    }
}

fn status_label(status: CanvasNodeStatus) -> &'static str {
    match status {
        CanvasNodeStatus::Idle => "idle",
        CanvasNodeStatus::Success => "success",
        CanvasNodeStatus::Loading => "loading",
        CanvasNodeStatus::Error => "error",
    }
}

/// Info-modal rows (canvas-node-hover-toolbar.tsx:238-256).
pub fn info_rows(node: &CanvasNodeData) -> Vec<crate::InfoRow> {
    let row = |label: &str, value: String| crate::InfoRow {
        label: label.into(),
        value: value.into(),
    };
    let mut rows = vec![
        row("ID", node.id.clone()),
        row("Name", node.title.clone()),
        row("Type", node.node_type.as_str().to_string()),
        row(
            "Size",
            format!("{} x {}", node.width.round(), node.height.round()),
        ),
        row(
            "Position",
            format!("{}, {}", node.position.x.round(), node.position.y.round()),
        ),
        row(
            "Status",
            node.metadata
                .as_ref()
                .and_then(|meta| meta.status)
                .map_or("idle".to_string(), |status| {
                    status_label(status).to_string()
                }),
        ),
    ];
    if let Some(images) = node.metadata.as_ref().and_then(|meta| meta.images.as_ref()) {
        if images.len() > 1 {
            rows.push(row("Image group", format!("{} images", images.len())));
        }
    }
    if let Some(task) = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.video_task_id.as_ref())
    {
        rows.push(row("Video task ID", task.clone()));
    }
    if let Some(source) = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.as_deref())
        .and_then(|content| content.strip_prefix("file:"))
    {
        rows.push(row("Local file", source.to_string()));
    }
    rows
}

/// Pretty JSON for the info modal, with embedded data URLs redacted
/// (canvas-node-hover-toolbar.tsx:170-183).
pub fn info_json(node: &CanvasNodeData) -> String {
    let Ok(mut value) = serde_json::to_value(node) else {
        return String::new();
    };
    if let Some(slot) = value
        .get_mut("metadata")
        .and_then(|meta| meta.get_mut("content"))
    {
        if slot
            .as_str()
            .is_some_and(|text| text.starts_with("data:image/"))
        {
            *slot = serde_json::Value::String("[base64 image]".to_string());
        }
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Previewable kinds: the single-image preview and the stack gallery
/// (project.tsx:2102-2130, `image-stack-preview.tsx`).
pub fn previewable(kind: &CanvasNodeType) -> bool {
    matches!(kind, CanvasNodeType::Image | CanvasNodeType::ImageStack)
}

/// Screen anchor of the hover toolbar: centred under the node, 8px below the
/// card (canvas-node-hover-toolbar.tsx).
///
/// The returned y is the bar's **top** edge. Pure function of the node's
/// **current** rect and the **current** viewport: there is no stored screen
/// position to go stale, so a window move, a canvas pan, a zoom or a node drag
/// recomputes the bar from scratch instead of leaving it behind. `HoverBar`
/// clamps the placed bar inside the visible canvas.
pub fn hover_anchor(node: &CanvasNodeData, viewport: crate::coords::Viewport) -> (f32, f32) {
    let x = viewport.x + (node.position.x + node.width / 2.0) * viewport.k;
    let y = viewport.y + (node.position.y + node.height) * viewport.k + 8.0;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates are far below f32's exact integer range"
    )]
    {
        (x as f32, y as f32)
    }
}

impl App {
    /// Hover toolbar anchor: centered under the node, 8px below the card, and
    /// clamped by the bar inside the visible canvas.
    ///
    /// Rebuilding the action model recreates the repeater items, which would
    /// destroy a pressed bar button mid-click — so the model is rebuilt only
    /// when the node changes and never while the pointer is over or pressed on
    /// the bar. The bar is also kept visible until the leave-grace deadline:
    /// an invisible bar is not hit-testable, so hiding it early would make the
    /// pointer's arrival at the bar land on the canvas instead.
    /// Re-anchors the existing hover bar without rebuilding its action model.
    /// Used by the live drag tick so the first frame does not pay for a model
    /// rebuild just because the bar was not hovered yet.
    pub fn sync_hover_bar_position(&mut self, ui: &AppWindow) {
        self.anchor_hover_bar(ui);
    }

    pub fn sync_hover_bar(&mut self, ui: &AppWindow) {
        if self.bars.interacting() && self.bars.last_node.is_some() {
            self.anchor_hover_bar(ui);
            return;
        }
        let Some(id) = toolbar_node_id(self) else {
            let within_grace = self
                .bars
                .hide_at
                .is_some_and(|deadline| Instant::now() < deadline);
            if within_grace && self.bars.last_node.is_some() {
                self.anchor_hover_bar(ui);
                return;
            }
            self.bars.last_node = None;
            self.bars.hide_at = None;
            ui.set_hover_bar_visible(false);
            return;
        };
        let Some(node) = self.doc.node(&id) else {
            self.bars.hover_node = None;
            self.bars.last_node = None;
            ui.set_hover_bar_visible(false);
            return;
        };
        if self.bars.last_node.as_deref() != Some(id.as_str()) {
            self.bars.last_node = Some(id.clone());
            let extras = crate::plugin_views::toolbar_extras(&self.plugin, node);
            self.hover_actions
                .set_vec(hover_action_rows_with(node, &extras));
        }
        self.anchor_hover_bar(ui);
    }

    /// Places the bar over [`Bars::last_node`] without touching the action model.
    fn anchor_hover_bar(&mut self, ui: &AppWindow) {
        let Some(node) = self
            .bars
            .last_node
            .as_deref()
            .and_then(|id| self.doc.node(id))
        else {
            self.bars.last_node = None;
            ui.set_hover_bar_visible(false);
            return;
        };
        let viewport = self.doc.viewport();
        let (anchor_x, anchor_y) = hover_anchor(node, viewport);
        ui.set_hover_bar_x(vm::len(f64::from(anchor_x)));
        ui.set_hover_bar_y(vm::len(f64::from(anchor_y)));
        ui.set_hover_bar_visible(true);
        // The layer popover is attached to this bar; keep it with the bar on the
        // same follow-tier tick instead of waiting for the cold settle pass.
        if self.node_layer.is_some() {
            self.sync_node_layer(ui);
        }
    }

    /// Selection bounds box + bar (canvas-selection-toolbar.tsx:72-91).
    pub fn sync_selection_ui(&mut self, ui: &AppWindow) {
        // Borrow the selected cards: cloning a node copies its metadata and
        // embedded media, which made the first frame of every drag on a large
        // image/attempt card visibly hitch.
        let selected: Vec<&CanvasNodeData> = self
            .doc
            .nodes()
            .iter()
            .filter(|node| self.doc.selection().contains(&node.id))
            .filter(|node| !is_node_hidden(node))
            .collect();
        if selected.len() < 2 {
            ui.set_selection_visible(false);
            ui.set_selection_bar_visible(false);
            return;
        }
        let bounds = selected.iter().fold(Bounds::empty(), |acc, node| Bounds {
            left: acc.left.min(node.position.x),
            top: acc.top.min(node.position.y),
            right: acc.right.max(node.position.x + node.width),
            bottom: acc.bottom.max(node.position.y + node.height),
        });
        let viewport = self.doc.viewport();
        let left = viewport.x + bounds.left * viewport.k - SELECTION_PAD;
        let top = viewport.y + bounds.top * viewport.k - SELECTION_PAD;
        let width = (bounds.right - bounds.left) * viewport.k + SELECTION_PAD * 2.0;
        let height = (bounds.bottom - bounds.top) * viewport.k + SELECTION_PAD * 2.0;
        ui.set_selection_visible(true);
        ui.set_selection_x(vm::len(left));
        ui.set_selection_y(vm::len(top));
        ui.set_selection_width(vm::len(width));
        ui.set_selection_height(vm::len(height));
        let identity = crate::coords::Viewport {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        ui.set_selection_border(
            overlay::rounded_border_commands(
                (left, top, width, height),
                SELECTION_RADIUS,
                SELECTION_DASH,
                identity,
            )
            .into(),
        );
        ui.set_selection_bar_visible(self.active_kind() == ActiveKind::None);
        ui.set_selection_bar_x(vm::len(left + width / 2.0));
        ui.set_selection_bar_y(vm::len(top - 8.0));
        ui.set_distributable(selected.len() >= 3);
        ui.set_align_menu_open(self.bars.menu == Some(Menu::Align));
        ui.set_layout_menu_open(self.bars.menu == Some(Menu::Layout));
    }

    pub fn sync_modals(&mut self, ui: &AppWindow) {
        let info = self
            .bars
            .info_node
            .as_ref()
            .and_then(|id| self.doc.node(id));
        if let Some(node) = info {
            ui.set_info_open(true);
            ui.set_info_title(node.title.as_str().into());
            self.info_rows.set_vec(info_rows(node));
            ui.set_info_error(
                node.metadata
                    .as_ref()
                    .and_then(|meta| meta.error_details.clone())
                    .unwrap_or_default()
                    .into(),
            );
            if self.bars.info_json {
                ui.set_info_json_text(slint::SharedString::from(info_json(node)));
            }
        } else {
            ui.set_info_open(false);
            ui.set_info_json(false);
        }
        let preview = self
            .bars
            .preview_node
            .as_ref()
            .and_then(|id| self.doc.node(id))
            .cloned();
        match preview {
            Some(node) => {
                ui.set_preview_open(true);
                ui.set_preview_title(node.title.as_str().into());
                let stack = node.node_type == CanvasNodeType::ImageStack;
                ui.set_preview_stack(stack);
                let content = crate::canvas_content::preview_content(&node, &mut self.media);
                let count = content.rows.len();
                ui.set_preview_image(content.image);
                ui.set_preview_info(content.info.as_str().into());
                ui.set_preview_rows(slint::ModelRc::new(slint::VecModel::from(content.rows)));
                ui.set_preview_count(vm::count_i32(count));
            }
            None => ui.set_preview_open(false),
        }
    }

    /// Escape precedence: closes exactly one overlay per press, topmost first
    /// (the reverse of `app.slint`'s declaration order: modal stack, then the
    /// node-level popovers, then the dock-anchored menus). Returns whether a
    /// layer was closed; when none was, the caller clears the selection.
    pub fn close_overlays(&mut self, ui: &AppWindow) -> bool {
        // Topmost modal: the plugin manager is declared after every other
        // modal, the config modal before it, then the image dialogs.
        if self.plugin.ui.manager_open {
            self.close_plugin_manager(ui);
            return true;
        }
        if self.config_open {
            self.config_close(ui);
            return true;
        }
        if self.panel.image_ai.is_some() {
            self.image_ai_close(ui);
            return true;
        }
        if self.canvas_media.tool.is_some() {
            self.image_tool_close(ui);
            return true;
        }
        if self.image.modal.is_some() {
            self.image_modal_close(ui);
            return true;
        }
        if self.bars.info_node.is_some() {
            self.bars.info_node = None;
            self.sync_modals(ui);
            return true;
        }
        if self.bars.preview_node.is_some() {
            self.bars.preview_node = None;
            self.sync_modals(ui);
            return true;
        }
        // Node-level layer popover (z-75), then the node panel popover, then
        // the hover-bar menu.
        if self.node_layer.is_some() {
            self.close_node_layer(ui);
            return true;
        }
        if self.panel.popover.is_some() {
            self.panel.popover = None;
            self.sync_panel(ui);
            self.sync_selection_ui(ui);
            return true;
        }
        if self.bars.menu.is_some() {
            self.bars.menu = None;
            self.sync_selection_ui(ui);
            return true;
        }
        // Dock-anchored menus, bottom-most.
        if self.plugin.palette_node.is_some() {
            self.plugin.palette_node = None;
            self.sync_rows_full(ui);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::Position;

    fn node(id: &str, kind: CanvasNodeType) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(0.0, 0.0),
            width: 10.0,
            height: 10.0,
            metadata: None,
        }
    }

    #[test]
    fn hover_actions_include_font_steps_only_for_text() {
        let text = hover_action_rows_with(&node("t", CanvasNodeType::Text), &[]);
        let ids: Vec<String> = text.into_iter().map(|a| a.id.to_string()).collect();
        assert!(ids.contains(&"font-up".to_string()));
        assert_eq!(ids.last().map(String::as_str), Some("delete"));
        let image = hover_action_rows_with(&node("i", CanvasNodeType::Image), &[]);
        let ids: Vec<String> = image.into_iter().map(|a| a.id.to_string()).collect();
        assert!(
            !ids.contains(&"upload".to_string()),
            "an image node is display-only: no upload affordance"
        );
        assert!(
            !ids.contains(&"crop".to_string()),
            "no content, no quick tools"
        );
        assert_eq!(font_step(14.0, true), 16.0);
        assert_eq!(font_step(31.0, true), 32.0);
        assert_eq!(font_step(11.0, false), 10.0);
    }

    #[test]
    fn hover_actions_of_an_attempt_offer_the_direct_generate_shortcut() {
        let ids: Vec<String> =
            hover_action_rows_with(&node("a", CanvasNodeType::ImageAttempt), &[])
                .into_iter()
                .map(|action| action.id.to_string())
                .collect();
        assert!(
            ids.contains(&"attempt-generate".to_string()),
            "the attempt card can generate and export an image in one click"
        );
    }

    #[test]
    fn hover_actions_of_an_image_with_content_offer_the_local_tools() {
        let mut image = node("i", CanvasNodeType::Image);
        image.metadata = Some(open_canvas_core::node::CanvasNodeMetadata {
            content: Some("data:image/png;base64,AAAA".into()),
            ..Default::default()
        });
        let ids: Vec<String> = hover_action_rows_with(&image, &[])
            .into_iter()
            .map(|action| action.id.to_string())
            .collect();
        for tool in [
            "crop",
            "mask",
            "split",
            "resolution",
            "analyze",
            "ocr",
            "segment",
            "angle",
            "free-resize",
        ] {
            assert!(ids.contains(&tool.to_string()), "{tool} is offered");
        }
        assert!(
            !ids.contains(&"upload".to_string()),
            "content replaces upload"
        );
        assert!(
            !ids.contains(&"download".to_string()) && !ids.contains(&"copy".to_string()),
            "download/copy live in the focused image viewer"
        );
        // The free-resize toggle reflects the node's stored flag.
        let actions = hover_action_rows_with(&image, &[]);
        let free = actions
            .iter()
            .find(|action| action.id.as_str() == "free-resize")
            .cloned()
            .unwrap_or_default();
        assert!(!free.active);
        if let Some(meta) = image.metadata.as_mut() {
            meta.free_resize = Some(true);
        }
        let actions = hover_action_rows_with(&image, &[]);
        let free = actions
            .iter()
            .find(|action| action.id.as_str() == "free-resize")
            .cloned()
            .unwrap_or_default();
        assert!(free.active, "the toggle reports the stored fit mode");
        // A stack without content has no upload either (the collect rule is the
        // drag-and-drop path).
        let stack = node("s", CanvasNodeType::ImageStack);
        let ids: Vec<String> = hover_action_rows_with(&stack, &[])
            .into_iter()
            .map(|action| action.id.to_string())
            .collect();
        assert!(!ids.contains(&"upload".to_string()));
    }

    #[test]
    fn info_json_redacts_embedded_images() {
        let mut image = node("i", CanvasNodeType::Image);
        image.metadata = Some(open_canvas_core::node::CanvasNodeMetadata {
            content: Some("data:image/png;base64,AAAA".into()),
            ..Default::default()
        });
        let json = info_json(&image);
        assert!(json.contains("[base64 image]"));
        assert!(!json.contains("AAAA"));
        assert!(info_rows(&image)
            .iter()
            .any(|row| row.label.as_str() == "Position"));
    }

    #[test]
    fn preview_is_limited_to_image_kinds() {
        assert!(previewable(&CanvasNodeType::Image));
        assert!(previewable(&CanvasNodeType::ImageStack));
        assert!(!previewable(&CanvasNodeType::Text));
    }

    /// The hover bar is recomputed from the node's current rect every sync, so
    /// it cannot stay behind when the canvas pans, the node moves or the window
    /// is moved/resized (no screen-space anchor is stored between syncs).
    #[test]
    fn hover_anchor_follows_the_node_and_the_viewport() {
        use crate::coords::Viewport;

        let mut card = node("n", CanvasNodeType::Image);
        card.position = Position::new(100.0, 200.0);
        card.width = 340.0;
        card.height = 240.0;

        let identity = Viewport::new();
        let (x, y) = hover_anchor(&card, identity);
        assert!((x - 270.0).abs() < 0.01, "centred on the card: {x}");
        assert!(
            (y - 448.0).abs() < 0.01,
            "8px below the card's bottom edge: {y}"
        );

        // A canvas pan moves the anchor by exactly the pan delta.
        let panned = Viewport {
            x: -120.0,
            y: 40.0,
            k: 1.0,
        };
        let (px, py) = hover_anchor(&card, panned);
        assert!((px - (x - 120.0)).abs() < 0.01);
        assert!((py - (y + 40.0)).abs() < 0.01);

        // A node drag moves it with the card (the rect is read live).
        card.position = Position::new(160.0, 260.0);
        let (dx, dy) = hover_anchor(&card, identity);
        assert!((dx - (x + 60.0)).abs() < 0.01);
        assert!((dy - (y + 60.0)).abs() < 0.01);

        // The same world rect at a different zoom lands at the zoomed position.
        let zoomed = Viewport {
            x: 0.0,
            y: 0.0,
            k: 2.0,
        };
        let (zx, zy) = hover_anchor(&card, zoomed);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the test's world coordinates are small exact f64 values"
        )]
        let (expect_x, expect_y) = (
            ((card.position.x + card.width / 2.0) * 2.0) as f32,
            ((card.position.y + card.height) * 2.0 + 8.0) as f32,
        );
        assert!((zx - expect_x).abs() < 0.01);
        assert!((zy - expect_y).abs() < 0.01);
        assert!(zy > 1000.0, "below the moved card's bottom edge");
    }
}
