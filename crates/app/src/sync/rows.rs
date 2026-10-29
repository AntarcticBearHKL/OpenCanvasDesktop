//! Node-row sync: the canvas node rows and their selection/related flags.
//! Owns the derived `rows` view model; the document stays the source of truth.

use slint::Model as _;

use crate::app::App;
use crate::vm;
use crate::{AppWindow, NodeItem};

impl App {
    pub(crate) fn rows_for_nodes(&mut self, ids: Option<&[String]>) -> Vec<(usize, NodeItem)> {
        // Preserve the enter animation progress of the rows being rebuilt. The
        // scan used to clone every row id and then linearly search it per node;
        // only the requested ids are kept, so a one-node refresh is O(rows)
        // copies of nothing.
        let enters: std::collections::HashMap<String, f32> = (0..self.rows.row_count())
            .filter_map(|index| self.rows.row_data(index))
            .filter(|row| ids.is_none_or(|ids| ids.iter().any(|id| id == row.id.as_str())))
            .map(|row| (row.id.to_string(), row.enter))
            .collect();
        let focus = self.focus_node_id();
        let related = crate::canvas_content::related_ids(focus.as_deref(), &self.doc.connections);
        let drop_target = self.drop_target.as_deref();
        let drop_slot = self.drop_slot.as_deref();
        // Smart Canvas previews: at most one composite per distinct linked
        // project, and only when the document actually has such a card.
        let mut smart_previews = std::collections::HashMap::new();
        for node in self.doc.nodes() {
            if node.node_type != open_canvas_core::node::CanvasNodeType::SmartCanvas {
                continue;
            }
            let Some(project_id) = node
                .metadata
                .as_ref()
                .and_then(|meta| meta.smart_canvas_project_id.as_deref())
            else {
                continue;
            };
            if smart_previews.contains_key(project_id) {
                continue;
            }
            if let Some(image) = self.smart_canvas_preview(project_id) {
                smart_previews.insert(project_id.to_string(), image);
            }
        }
        let Self {
            doc,
            plugin,
            media,
            edit,
            library,
            panel,
            folders,
            asset_expanded,
            video_ref_slots,
            ..
        } = self;
        let index = vm::NodeIndex::new(doc.nodes());
        let mut context = vm::RowContext {
            nodes: doc.nodes(),
            connections: doc.connections(),
            index: &index,
            library: &library.index,
            media,
            edit,
            related: &related,
            reference_slots: video_ref_slots,
            drop_target,
            drop_slot,
            panel,
            folders,
            asset_expanded,
            smart_previews: &smart_previews,
        };
        let mut rows = Vec::new();
        let mut model_index = 0_usize;
        for node in doc.nodes() {
            if open_canvas_core::geometry::is_node_hidden(node) {
                continue;
            }
            let current = model_index;
            model_index += 1;
            if let Some(ids) = ids {
                if !ids.iter().any(|id| id == &node.id) {
                    continue;
                }
            }
            let selected = doc.selection().contains(&node.id);
            let enter = enters.get(&node.id).copied().unwrap_or(1.0);
            rows.push((
                current,
                vm::node_row(node, selected, enter, plugin, &mut context),
            ));
        }
        rows
    }

    pub(crate) fn visible_rows(&mut self) -> Vec<NodeItem> {
        self.rows_for_nodes(None)
            .into_iter()
            .map(|(_, row)| row)
            .collect()
    }

    /// Structural refresh: rebuilds the whole row model (create/delete/undo).
    pub fn sync_rows_full(&mut self, ui: &AppWindow) {
        let rows = self.visible_rows();
        self.rows.set_vec(rows);
        ui.set_has_selection(!self.doc.selection().is_empty());
        self.drag_stats.row_rebuilds = self.drag_stats.row_rebuilds.saturating_add(1);
    }

    /// Cheap refresh for in-place changes (drag/resize of specific nodes).
    pub fn sync_rows_of(&mut self, ui: &AppWindow, ids: &[String]) {
        for (index, row) in self.rows_for_nodes(Some(ids)) {
            if index < self.rows.row_count() {
                self.rows.set_row_data(index, row);
            }
        }
        ui.set_has_selection(!self.doc.selection().is_empty());
        self.drag_stats.row_rebuilds = self.drag_stats.row_rebuilds.saturating_add(1);
    }

    /// Incremental refresh for a live node gesture: rewrites only the geometry
    /// fields of the dragged rows, in place.
    ///
    /// `sync_rows_of` would rebuild a whole `NodeItem` per dragged node (media
    /// lookups, plugin cells, every `SharedString` and sub-model) on every
    /// pointer move. A drag changes nothing on a card except its rectangle, so
    /// the row model is scanned once (O(visible rows)) and the position fields
    /// of the handful of dragged rows are written; Slint then repaints only
    /// those cards. The full rebuild happens once, on release.
    #[expect(
        clippy::float_cmp,
        reason = "the row value and the doc value come from the same `vm::len` call; exact equality is the correct 'unchanged' test here"
    )]
    pub fn sync_drag_rows(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        for index in 0..self.rows.row_count() {
            let Some(mut row) = self.rows.row_data(index) else {
                continue;
            };
            if !ids.iter().any(|id| id == row.id.as_str()) {
                continue;
            }
            let Some(node) = self.doc.node(row.id.as_str()) else {
                continue;
            };
            let x = vm::len(node.position.x);
            let y = vm::len(node.position.y);
            let width = vm::len(node.width);
            let height = vm::len(node.height);
            if row.world_x == x && row.world_y == y && row.width == width && row.height == height {
                continue;
            }
            row.world_x = x;
            row.world_y = y;
            row.width = width;
            row.height = height;
            self.rows.set_row_data(index, row);
            self.drag_stats.row_updates = self.drag_stats.row_updates.saturating_add(1);
        }
    }

    /// Incremental drop-slot highlight for the rows whose target changed. The
    /// drop target itself is a window scalar (`drop-target-id`); the only row
    /// field that depends on it is `slot-drop`, so a target change never costs
    /// a full `NodeItem` rebuild while the drag is live.
    pub fn sync_slot_drop_rows(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let target = self.drop_target.clone();
        let slot = self.drop_slot.clone().unwrap_or_default();
        for index in 0..self.rows.row_count() {
            let Some(mut row) = self.rows.row_data(index) else {
                continue;
            };
            if !ids.iter().any(|id| id == row.id.as_str()) {
                continue;
            }
            let next = if target.as_deref() == Some(row.id.as_str()) {
                slot.as_str()
            } else {
                ""
            };
            if row.slot_drop.as_str() == next {
                continue;
            }
            row.slot_drop = next.into();
            self.rows.set_row_data(index, row);
            self.drag_stats.row_updates = self.drag_stats.row_updates.saturating_add(1);
        }
    }

    /// Selection-only refresh: rewrites the `selected` flag of every row.
    pub fn sync_selection(&mut self, ui: &AppWindow) {
        for index in 0..self.rows.row_count() {
            let Some(mut row) = self.rows.row_data(index) else {
                continue;
            };
            let selected = self.doc.selection().contains(row.id.as_str());
            if row.selected != selected {
                row.selected = selected;
                self.rows.set_row_data(index, row);
            }
        }
        ui.set_has_selection(!self.doc.selection().is_empty());
        self.sync_related();
    }

    /// Rewrites the related flags for the current focus in place: no row is
    /// rebuilt and no bitmap is hashed, so hovering a node stays cheap even with
    /// large media on the canvas. Selection never dims anything: `related` is
    /// the only flag this touches.
    pub fn sync_related(&mut self) {
        let focus = self.focus_node_id();
        let related = crate::canvas_content::related_ids(focus.as_deref(), &self.doc.connections);
        for index in 0..self.rows.row_count() {
            let Some(mut row) = self.rows.row_data(index) else {
                continue;
            };
            let is_related = related.contains(row.id.as_str());
            if row.related != is_related {
                row.related = is_related;
                self.rows.set_row_data(index, row);
            }
        }
    }
}
