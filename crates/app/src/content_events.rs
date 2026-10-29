//! Handlers and Slint wiring for the canvas content layer: the card's status
//! surfaces (retry), the media info bar, the image-stack view-all affordance,
//! the in-place editors and the prompt chip delete.
//!
//! Provenance: `canvas-node.tsx` (`ErrorContent` retry, `MediaInfoBar` info
//! button, `TextContent`/title editing, `onTitleChange`/`onContentChange`) and
//! `image-stack-node-content.tsx` (`onViewAll` on the count chip / double-click).

use std::cell::RefCell;
use std::rc::Rc;

use open_canvas_core::geometry::{is_node_locked, MovedNode};
use open_canvas_core::node::{
    CanvasNodeData, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType, Position,
};
use slint::ComponentHandle;

use crate::app::{with_app, App};
use crate::coords::{ScreenPos, WorldPos};
use crate::inline_edit::{CommittedEdit, EditTarget};
use crate::AppWindow;

impl App {
    /// Retry on the error surface (`ErrorContent`): a node with a generation
    /// pipeline reruns its request; any other node clears the stale error so
    /// its content shows again.
    pub fn node_retry(&mut self, ui: &AppWindow, id: &str) {
        let Some(node) = self.doc.node(id).cloned() else {
            return;
        };
        if is_node_locked(&node) {
            return;
        }
        if crate::ai_service::gen_mode(self, &node).is_some() {
            self.ai_start(ui, id);
            return;
        }
        // A produced image / image stack owns no generation pipeline of its
        // own: its Retry reruns the generation that placed it, which resets
        // this failed card in place.
        let generation = self
            .ai
            .as_ref()
            .and_then(|service| service.generation_for_output(id))
            .map(str::to_string);
        if let Some(generation) = generation {
            if self.doc.node(&generation).is_some() {
                self.ai_start(ui, &generation);
                return;
            }
        }
        self.reset_node_error(id);
        self.sync_rows_of(ui, &[id.to_string()]);
        self.sync_panel(ui);
        self.note_doc_change();
    }

    /// Image-stack *view all* (`onViewAll`): opens the stack preview modal.
    pub fn node_view_all(&mut self, ui: &AppWindow, id: &str) {
        let stack = self
            .doc
            .node(id)
            .is_some_and(|node| node.node_type == CanvasNodeType::ImageStack);
        if !stack {
            return;
        }
        self.bars.preview_node = Some(id.to_string());
        self.sync_modals(ui);
    }

    /// Info button of the media info bar: opens the node info modal (same flow
    /// as the hover toolbar's info action).
    pub fn node_info(&mut self, ui: &AppWindow, id: &str) {
        if self.doc.node(id).is_none() {
            return;
        }
        self.bars.info_node = Some(id.to_string());
        self.bars.info_json = false;
        ui.set_info_json(false);
        self.sync_modals(ui);
        self.sync_hover_bar(ui);
    }

    /// Opens the node's in-place editor (`double-click`): the body editor for
    /// Text nodes, the rename input for any unlocked card.
    pub fn node_edit_begin(&mut self, ui: &AppWindow, id: &str, target: &str) {
        let Some(node) = self.doc.node(id).cloned() else {
            return;
        };
        if is_node_locked(&node) {
            return;
        }
        let started = if target == EditTarget::Title.as_str() {
            self.edit.begin_title(id, &node.title)
        } else if node.node_type == CanvasNodeType::Text {
            let body = crate::vm::node_body(&node);
            self.edit.begin_body(id, &body)
        } else {
            false
        };
        if started {
            self.sync_rows_of(ui, &[id.to_string()]);
        }
    }

    /// One keystroke of an open editor.
    pub fn node_edit_change(&mut self, id: &str, text: &str) {
        self.edit.change(id, text);
    }

    /// Commit (blur / Enter / Ctrl+Enter): writes one history entry when the
    /// draft differs; an unchanged or already-cancelled editor writes nothing.
    pub fn node_edit_commit(&mut self, ui: &AppWindow, id: &str) {
        let Some(committed) = self.edit.commit() else {
            self.sync_rows_of(ui, &[id.to_string()]);
            return;
        };
        self.apply_committed_edit(ui, &committed);
    }

    /// Escape: drop the draft and re-render the stored text.
    pub fn node_edit_cancel(&mut self, ui: &AppWindow, id: &str) {
        let cancelled = self
            .edit
            .cancel()
            .map_or_else(|| id.to_string(), |(_, node_id)| node_id);
        self.sync_rows_of(ui, &[cancelled]);
    }

    /// Card chip delete: drops one id from `metadata.references` (the composer
    /// field still edits the prompt text itself).
    pub fn node_chip_remove(&mut self, ui: &AppWindow, id: &str, reference_id: &str) {
        if self.remove_chip_from_doc(id, reference_id) {
            self.note_doc_change();
            self.sync_rows_of(ui, &[id.to_string()]);
            self.sync_panel(ui);
        }
    }

    /// The document half of the chip delete (history entry included).
    pub(crate) fn remove_chip_from_doc(&mut self, id: &str, reference_id: &str) -> bool {
        let now = self.now_ms();
        let mut changed = false;
        if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
            if is_node_locked(node) {
                return false;
            }
            changed = crate::canvas_content::remove_reference(node, reference_id);
        }
        if changed {
            self.doc.commit(now);
        }
        changed
    }

    fn apply_committed_edit(&mut self, ui: &AppWindow, committed: &CommittedEdit) {
        if self.apply_edit_to_doc(committed) {
            self.note_doc_change();
            self.sync_wires(ui);
        }
        self.sync_rows_of(ui, std::slice::from_ref(&committed.node_id));
        self.sync_panel(ui);
    }

    /// The document half of an editor commit: one history entry when the text
    /// differs from the stored value.
    pub(crate) fn apply_edit_to_doc(&mut self, committed: &CommittedEdit) -> bool {
        let now = self.now_ms();
        let mut changed = false;
        if let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == committed.node_id)
        {
            match committed.target {
                EditTarget::Title => {
                    if node.title != committed.text {
                        node.title.clone_from(&committed.text);
                        changed = true;
                    }
                }
                EditTarget::Body => {
                    let meta = node
                        .metadata
                        .get_or_insert_with(CanvasNodeMetadata::default);
                    if meta.content.as_deref() != Some(committed.text.as_str()) {
                        meta.content = Some(committed.text.clone());
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.doc.commit(now);
        }
        changed
    }

    /// Binds (or re-binds) an assets node to a local folder. The folder path is
    /// stored as project data, the scan cache is dropped and the node re-renders
    /// its tree. The picker is the Win32 shell dialog; a cancelled dialog writes
    /// nothing.
    pub fn asset_bind_folder(&mut self, ui: &AppWindow, id: &str) {
        let Some(node) = self.doc.node(id) else {
            return;
        };
        if node.node_type != CanvasNodeType::Assets {
            return;
        }
        let owner = crate::file_drop::hwnd_of(ui.window()).unwrap_or(0);
        let Some(folder) = crate::file_drop::pick_folder(owner) else {
            return;
        };
        let now = self.now_ms();
        if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.asset_folder = Some(folder);
            meta.asset_source = Some(open_canvas_core::node::CanvasAssetSource::Folder);
        }
        self.doc.commit(now);
        self.folders.clear();
        self.asset_expanded.remove(id);
        self.note_doc_change();
        self.sync_rows_of(ui, &[id.to_string()]);
        self.sync_panel(ui);
    }

    /// Expands/collapses one folder of a bound tree (relative path, never a
    /// filesystem path: the root stays the root).
    pub fn asset_toggle_folder(&mut self, ui: &AppWindow, id: &str, path: &str) {
        let Some(node) = self.doc.node(id) else {
            return;
        };
        if node.node_type != CanvasNodeType::Assets {
            return;
        }
        let expanded = self.asset_expanded.entry(id.to_string()).or_default();
        let path = path.to_string();
        if !expanded.remove(&path) {
            expanded.insert(path);
        }
        self.sync_rows_of(ui, &[id.to_string()]);
    }

    /// Releases a file-row drag at its last pointer position. The row calls
    /// this from `drag-out`, immediately after reporting the release point, so
    /// the node lands exactly where the pointer was let go even when the last
    /// move was never ticked.
    pub fn asset_drag_file(&mut self, ui: &AppWindow, _id: &str, _relative: &str) {
        let Some(asset) = self.asset_drag.clone() else {
            return;
        };
        self.finish_asset_drag(ui, Some(asset.last));
    }

    /// Resolves an in-flight assets drag at a canvas-local screen point. The
    /// canvas pointer hook calls this on `up`/`cancel` while a row drag is
    /// pending, so a release that the assets tree no longer receives still
    /// commits (or cancels) at the point the user let go.
    ///
    /// `false` when no drag is pending, so the canvas pointer path can stay
    /// untouched for ordinary clicks.
    pub fn asset_drag_drop_at(&mut self, ui: &AppWindow, point: ScreenPos) -> bool {
        if self.asset_drag.is_none() {
            return false;
        }
        self.finish_asset_drag(ui, Some(point));
        true
    }

    /// Drops a pending assets drag without creating anything, removing the
    /// temporary node and restoring the selection the canvas had before it.
    /// Used when a release back on the assets card or a new canvas press proves
    /// the drag is over.
    pub fn asset_drag_cancel(&mut self, ui: &AppWindow) {
        let Some(asset) = self.asset_drag.take() else {
            self.drag_ghost_pos = None;
            return;
        };
        self.pending_drag.to = None;
        self.drag_ghost_pos = None;
        self.drop_target = None;
        self.drop_slot = None;
        self.doc.end_node_drag();
        self.remove_asset_drag_node(&asset.node_id);
        self.doc.selection = asset.previous_selection;
        self.pending_media.remove(&asset.path);
        self.sync_all(ui);
    }

    /// `drag-end` fallback: a row that ends without a drop (for example a
    /// cancelled gesture whose release never reached the canvas) must not leave
    /// a temporary card behind. A normal `drag-out` already consumed the drag,
    /// so this is a no-op on the success path.
    pub fn asset_drag_end(&mut self, ui: &AppWindow) {
        if self.asset_drag.is_some() {
            self.asset_drag_cancel(ui);
        }
    }

    /// Shared release path for an assets drag. `drop` is canvas-local screen
    /// coordinates; `None` falls back to the last pointer sample.
    ///
    /// The temporary node is already in the document (it *is* the preview), so
    /// release only lands it exactly, loads its payload, and applies the same
    /// drop rules as a node drag: an image stack/slot absorbs it, while empty
    /// canvas keeps it. A release on an assets card cancels and removes it.
    fn finish_asset_drag(&mut self, ui: &AppWindow, drop: Option<ScreenPos>) {
        let Some(asset) = self.asset_drag.take() else {
            return;
        };
        self.pending_drag.to = None;
        self.drag_ghost_pos = None;
        self.drop_target = None;
        self.drop_slot = None;
        let point = drop
            .or(Some(asset.last))
            .unwrap_or_else(|| ScreenPos::new(self.canvas_size.0 / 2.0, self.canvas_size.1 / 2.0));
        let cancel = point.x < 0.0 || self.point_on_assets_card(point);
        if cancel {
            self.doc.end_node_drag();
            self.remove_asset_drag_node(&asset.node_id);
            self.doc.selection = asset.previous_selection;
            self.pending_media.remove(&asset.path);
            self.sync_all(ui);
            return;
        }
        let world = self.doc.viewport().screen_to_world(point);
        self.place_asset_drag_node(&asset.node_id, world);
        if !self.load_asset_drag_payload(&asset.node_id, &asset.path) {
            // The file vanished or exceeded the embedded-media cap: leave no
            // half-node behind.
            self.doc.end_node_drag();
            self.remove_asset_drag_node(&asset.node_id);
            self.doc.selection = asset.previous_selection;
            self.pending_media.remove(&asset.path);
            self.set_notice(ui, "Could not read the dragged file".to_string(), true);
            self.sync_all(ui);
            return;
        }
        let now = self.now_ms();
        let target = crate::drop_bindings::resolve_drop_target(
            std::slice::from_ref(&asset.node_id),
            self.doc.nodes(),
            world,
        )
        .filter(|target| {
            !self
                .doc
                .node(&target.node_id)
                .is_some_and(|node| node.node_type == CanvasNodeType::Assets)
        });
        let mut changed = false;
        if let Some(target) = target {
            let initial = self.doc.node(&asset.node_id).map(|node| MovedNode {
                id: node.id.clone(),
                x: node.position.x,
                y: node.position.y,
            });
            if let Some(initial) = initial {
                changed = self
                    .doc
                    .apply_drop(&target, std::slice::from_ref(&initial), now);
                let consumed = self.doc.node(&asset.node_id).is_none();
                if changed && !consumed {
                    // A video-prompt slot binds by node id, but the preview is
                    // still a temporary card: bind it, remove the card, and
                    // keep the prompt selected.
                    self.remove_asset_drag_node(&asset.node_id);
                    self.doc.selection.clear();
                    self.doc.selection.insert(target.node_id);
                    self.doc.commit(now);
                }
            }
            self.doc.end_node_drag();
        } else {
            self.doc.end_node_drag();
            self.doc.commit(now);
            changed = true;
        }
        self.pending_media.remove(&asset.path);
        if changed {
            self.note_doc_change();
        }
        self.sync_all(ui);
    }

    /// Converts a pointer reported in window coordinates (a row's
    /// `absolute-position + mouse`) into the canvas column's coordinates.
    ///
    /// The assets tree lives in the side panel left of the canvas, and the
    /// canvas view (edge-to-edge: no title strip) starts at the panel's right
    /// edge, top at the window's top. The drop point is fed to
    /// `screen_to_world` with the canvas viewport, so the panel offset is
    /// removed exactly once here — the node and the release use the same point.
    fn canvas_point(ui: &AppWindow, point: ScreenPos) -> ScreenPos {
        let origin = if ui.get_side_panel_open() {
            f64::from(ui.get_side_panel_width())
        } else {
            0.0
        };
        ScreenPos::new(point.x - origin, point.y)
    }

    /// Whether a canvas-local screen point lies on any assets card: releasing
    /// a file drag back over the tree (its source or another card) is a cancel.
    fn point_on_assets_card(&self, point: ScreenPos) -> bool {
        let viewport = self.doc.viewport();
        self.doc.nodes().iter().any(|node| {
            if node.node_type != CanvasNodeType::Assets {
                return false;
            }
            let top_left =
                viewport.world_to_screen(WorldPos::new(node.position.x, node.position.y));
            point.x >= top_left.x
                && point.x <= top_left.x + node.width * viewport.k
                && point.y >= top_left.y
                && point.y <= top_left.y + node.height * viewport.k
        })
    }

    /// A file row crossed the press threshold: put a real media node on the
    /// canvas at the pointer immediately, select it, and start an ordinary node
    /// drag so frame-tick coalescing, snapping overlays and drop targeting all
    /// come from the existing gesture path. The node starts with a loading
    /// status; [`Self::note_asset_prepared`] swaps in the decoded bitmap.
    ///
    /// `x`/`y` are the row's window coordinates; they are converted to the
    /// canvas column once, so the card, its live moves, and the release all
    /// share one coordinate space.
    pub fn asset_drag_begin(&mut self, ui: &AppWindow, id: &str, relative: &str, x: f64, y: f64) {
        if self.asset_drag.is_some() {
            self.asset_drag_cancel(ui);
        }
        let Some(node) = self.doc.node(id) else {
            return;
        };
        if node.node_type != CanvasNodeType::Assets {
            return;
        }
        let Some(folder) = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.asset_folder.clone())
        else {
            return;
        };
        let Some(path) =
            crate::asset_folder::resolve_in_root(std::path::Path::new(&folder), relative)
        else {
            return;
        };
        let Some(path) = path.to_str() else {
            return;
        };
        let Some(kind) = crate::file_drop::classify(path) else {
            return;
        };
        let point = Self::canvas_point(ui, ScreenPos::new(x, y));
        let world = self.doc.viewport().screen_to_world(point);
        let now = self.now_ms();
        let previous_selection = self.doc.selection().clone();
        let node_id = self.create_asset_drag_node(kind, path, world, now);
        self.doc.selection.clear();
        self.doc.selection.insert(node_id.clone());
        if !self.doc.begin_node_drag(&node_id, point) {
            self.remove_asset_drag_node(&node_id);
            self.doc.selection = previous_selection;
            return;
        }
        self.pending_drag.to = None;
        self.drag_ghost_pos = None;
        self.asset_drag = Some(crate::app::AssetDrag {
            node_id: node_id.clone(),
            path: path.to_string(),
            source_id: id.to_string(),
            source_path: relative.to_string(),
            last: point,
            previous_selection,
        });
        // Append just the preview row instead of a full model reset: the row
        // under the pressed pointer lives in the same repeater, and rebuilding
        // it would drop the drag that is still driving this gesture.
        self.append_asset_drag_row(ui, &node_id);
        spawn_asset_prepare(ui.as_weak(), path.to_string(), kind);
    }

    /// Appends the temporary node's row to the live model in place. Existing
    /// rows (including the assets card that owns the pressed row) keep their
    /// identities, so the pointer grab survives the first drag frame.
    fn append_asset_drag_row(&mut self, ui: &AppWindow, node_id: &str) {
        let ids = [node_id.to_string()];
        let Some((_, row)) = self.rows_for_nodes(Some(&ids)).into_iter().next() else {
            // The node is hidden by a filter: fall back to a full rebuild so
            // the document and model cannot disagree.
            self.sync_all(ui);
            return;
        };
        self.rows.push(row);
        self.sync_selection(ui);
        self.sync_overlays(ui);
        self.sync_flags(ui);
        self.sync_view_hot(ui);
    }

    /// Pointer move of an assets drag: record the latest point and let the
    /// frame tick apply it through the node-drag path. No document write, model
    /// write, or redraw happens per pointer event.
    pub fn asset_drag_moved(&mut self, ui: &AppWindow, x: f64, y: f64) {
        let point = Self::canvas_point(ui, ScreenPos::new(x, y));
        let Some(asset) = self.asset_drag.as_mut() else {
            return;
        };
        asset.last = point;
        self.note_node_drag(point);
    }

    /// The worker finished reading/decoding the dragged file. While the drag is
    /// still in flight the temporary node gains its real bitmap and drops its
    /// loading plate; the payload is also cached for a release that lands
    /// before the next worker result.
    pub fn note_asset_prepared(
        &mut self,
        ui: &AppWindow,
        path: String,
        prepared: Option<(String, Option<crate::media::DecodedPixels>)>,
    ) {
        let Some((content, pixels)) = prepared else {
            return;
        };
        let Some(asset) = self
            .asset_drag
            .as_ref()
            .filter(|asset| asset.path == path)
            .cloned()
        else {
            // The drag is already over; the release path loaded the payload
            // synchronously. Nothing else owns this result.
            return;
        };
        let (width, height) = match &pixels {
            Some(pixels) => {
                self.media.insert_decoded(&content, pixels);
                (pixels.width, pixels.height)
            }
            None => (0, 0),
        };
        self.apply_asset_payload(&asset.node_id, &content, (width, height));
        self.pending_media.insert(
            path,
            crate::app::PendingMedia {
                content,
                width,
                height,
            },
        );
        self.sync_rows_of(ui, &[asset.node_id]);
    }

    /// Creates the temporary Image/Video node centred on `at` without a
    /// history commit: a cancelled drag must leave no undo entry. Its loading
    /// plate is the only placeholder until the worker returns.
    fn create_asset_drag_node(
        &mut self,
        kind: crate::file_drop::DropKind,
        path: &str,
        at: WorldPos,
        now: u64,
    ) -> String {
        use open_canvas_core::constants::get_node_spec;

        let (node_type, content, status) = match kind {
            crate::file_drop::DropKind::Image => {
                (CanvasNodeType::Image, None, Some(CanvasNodeStatus::Loading))
            }
            crate::file_drop::DropKind::Video => (
                CanvasNodeType::Video,
                Some(format!("file:{path}")),
                Some(CanvasNodeStatus::Success),
            ),
        };
        let mut spec = get_node_spec(&node_type);
        spec.title = crate::file_drop::file_title(path);
        spec.metadata.content = content;
        spec.metadata.mime_type = Some(crate::file_drop::mime_for(path).to_string());
        spec.metadata.status = status;
        self.doc.seq = self.doc.seq.wrapping_add(1);
        let id = format!("{}-{now}-{}", node_type.as_str(), self.doc.seq);
        self.doc.nodes_mut().push(CanvasNodeData {
            id: id.clone(),
            node_type,
            title: spec.title,
            position: Position::new(at.x - spec.width / 2.0, at.y - spec.height / 2.0),
            width: spec.width,
            height: spec.height,
            metadata: Some(spec.metadata),
        });
        id
    }

    /// Lands the preview at the release point exactly: no grid snapping and no
    /// nudge away from a card underneath, because the user already placed the
    /// node with the pointer.
    fn place_asset_drag_node(&mut self, node_id: &str, at: WorldPos) {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        node.position = Position::new(at.x - node.width / 2.0, at.y - node.height / 2.0);
    }

    /// Ensures the temporary node owns its decoded payload before a drop can
    /// bind it: the worker cache is preferred, otherwise the file is read on
    /// this thread (a fast release can beat the worker). `false` when the file
    /// cannot be read at all.
    fn load_asset_drag_payload(&mut self, node_id: &str, path: &str) -> bool {
        let Some(node) = self.doc.node(node_id) else {
            return false;
        };
        if node.node_type == CanvasNodeType::Video {
            return true;
        }
        let already_loaded = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.content.as_deref())
            .is_some_and(|content| !content.is_empty())
            && node.metadata.as_ref().and_then(|meta| meta.status)
                == Some(CanvasNodeStatus::Success);
        if already_loaded {
            return true;
        }
        if let Some(prepared) = self.pending_media.remove(path) {
            self.apply_asset_payload(
                node_id,
                &prepared.content,
                (prepared.width, prepared.height),
            );
            return true;
        }
        let Some((content, pixels)) = prepare_asset_now(path, crate::file_drop::DropKind::Image)
        else {
            return false;
        };
        let dimensions = pixels
            .as_ref()
            .map_or((0, 0), |pixels| (pixels.width, pixels.height));
        if let Some(pixels) = &pixels {
            self.media.insert_decoded(&content, pixels);
        }
        self.apply_asset_payload(node_id, &content, dimensions);
        true
    }

    /// Writes a prepared payload into the temporary node and drops its loading
    /// plate. The MIME type follows the data URL (sniffed from the bytes), so a
    /// `.jpg` that is really a PNG is labelled by what it contains.
    fn apply_asset_payload(&mut self, node_id: &str, content: &str, dimensions: (u32, u32)) {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.content = Some(content.to_string());
        meta.status = Some(CanvasNodeStatus::Success);
        meta.error_details = None;
        if let Some(mime) = content
            .strip_prefix("data:")
            .and_then(|rest| rest.split(';').next())
            .filter(|mime| !mime.is_empty())
        {
            meta.mime_type = Some(mime.to_string());
        }
        if dimensions.0 > 0 && dimensions.1 > 0 {
            meta.natural_width = Some(f64::from(dimensions.0));
            meta.natural_height = Some(f64::from(dimensions.1));
        }
    }

    /// Removes the temporary preview without touching history (it was never
    /// committed) or lingering drag state.
    fn remove_asset_drag_node(&mut self, node_id: &str) {
        self.doc.nodes_mut().retain(|node| node.id != node_id);
        self.doc.connections.retain(|connection| {
            connection.from_node_id != node_id && connection.to_node_id != node_id
        });
        self.doc.selection.remove(node_id);
    }

    /// Refreshes a bound tree after files changed on disk.
    pub fn asset_refresh_folder(&mut self, ui: &AppWindow, id: &str) {
        if self.doc.node(id).is_none() {
            return;
        }
        self.folders.clear();
        self.sync_rows_of(ui, &[id.to_string()]);
    }

    /// Clears a stale failure: idle status, dropped error detail. Returns    /// whether the node actually changed (the retry branch of a node without a
    /// generation pipeline).
    pub(crate) fn reset_node_error(&mut self, id: &str) -> bool {
        let now = self.now_ms();
        let mut changed = false;
        if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            changed = meta.error_details.is_some() || meta.status == Some(CanvasNodeStatus::Error);
            meta.error_details = None;
            meta.status = Some(CanvasNodeStatus::Idle);
        }
        if changed {
            self.doc.commit(now);
        }
        changed
    }
}

/// Reads and decodes one assets-tree file: the data URL (or `file:` reference
/// for video) and, for images, its decoded pixels. Shared by the worker and the
/// release fallback, so both paths store exactly the same payload.
fn prepare_asset_now(
    path: &str,
    kind: crate::file_drop::DropKind,
) -> Option<(String, Option<crate::media::DecodedPixels>)> {
    match kind {
        crate::file_drop::DropKind::Image => {
            crate::file_drop::image_data_url(path).map(|content| {
                let pixels = crate::media::decode_pixels(&content);
                (content, pixels)
            })
        }
        crate::file_drop::DropKind::Video => Some((format!("file:{path}"), None)),
    }
}

/// Reads and decodes an assets-tree file off the UI thread. The payload lands
/// back on the event loop, where the temporary node's loading plate is replaced
/// by the bitmap (or the content is cached for a release already in flight).
fn spawn_asset_prepare(ui: slint::Weak<AppWindow>, path: String, kind: crate::file_drop::DropKind) {
    std::thread::spawn(move || {
        let prepared = prepare_asset_now(&path, kind);
        let result = slint::invoke_from_event_loop(move || {
            let Some(ui) = ui.upgrade() else {
                return;
            };
            crate::with_shared_app(|app| app.note_asset_prepared(&ui, path, prepared));
        });
        if result.is_err() {
            // The window is gone; the payload has no consumer.
        }
    });
}

/// Registers the content-layer callbacks (retry, info, view-all, editing and
/// chip delete).
pub fn wire_content(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_retry(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_retry(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_view_all(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_view_all(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_info(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_info(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_edit_begin(move |id, target| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.node_edit_begin(&ui, id.as_str(), target.as_str());
            });
        });
    }
    {
        let app = app.clone();
        ui.on_node_edit_change(move |id, text| {
            with_app(&app, |app| app.node_edit_change(id.as_str(), text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_edit_commit(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_edit_commit(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_edit_cancel(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.node_edit_cancel(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_chip_remove(move |id, reference_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.node_chip_remove(&ui, id.as_str(), reference_id.as_str());
            });
        });
    }
    asset_callbacks(ui, app);
}

/// Assets-node folder binding: bind/rebind, expand a folder, refresh, and the
/// drag-out of one file row.
fn asset_callbacks(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_bind(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.asset_bind_folder(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_toggle(move |id, path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.asset_toggle_folder(&ui, id.as_str(), path.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_refresh(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.asset_refresh_folder(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_drag(move |id, path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.asset_drag_file(&ui, id.as_str(), path.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_drag_start(move |id, path, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.asset_drag_begin(&ui, id.as_str(), path.as_str(), f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_drag_move(move |_id, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.asset_drag_moved(&ui, f64::from(x), f64::from(y));
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_asset_drag_end(move |_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.asset_drag_end(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_ref_insert(move |id, image, x, y, width, _height| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.insert_prompt_reference_at(
                    &ui,
                    id.as_str(),
                    image.as_str(),
                    f64::from(x),
                    f64::from(y),
                    f64::from(width),
                );
            });
        });
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
