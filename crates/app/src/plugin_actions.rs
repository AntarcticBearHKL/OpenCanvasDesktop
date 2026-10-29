//! Plugin node actions on the shared app: the edit toggle, the sticky-note
//! palette, the interaction flip and the generate/reset items, plus the
//! hover-toolbar action-model refresh those actions need.

use open_canvas_core::node::CanvasNodeMetadata;
use open_canvas_plugins::official::sticky_note;

use crate::app::App;
use crate::bars;
use crate::plugin_content::PluginCellKind;
use crate::plugin_views;
use crate::AppWindow;

/// Node actions the plugin toolbar contributes (see `plugin_views`).
pub const ACTION_INTERACT: &str = "plugin-interact";
pub const ACTION_GENERATE: &str = "plugin-generate";
pub const ACTION_RESET: &str = "plugin-reset";
pub const ACTION_PALETTE: &str = "plugin-palette";

impl App {
    /// The plugin cell kind of `id` (hot-path cache lookup).
    fn plugin_cell_kind(&mut self, id: &str) -> Option<PluginCellKind> {
        let node = self.doc.node(id)?.clone();
        let upstream =
            crate::plugin_host::upstream_nodes(self.doc.nodes(), self.doc.connections(), &node.id);
        Some(self.plugin.cell(&node, &upstream).kind)
    }

    /// Double-click on a plugin node toggles its editor (`editing` extra) for
    /// the four kinds that have one (sticky / markdown / html / svg).
    pub fn toggle_plugin_editing(&mut self, ui: &AppWindow, id: &str) -> bool {
        let editable = matches!(
            self.plugin_cell_kind(id),
            Some(
                PluginCellKind::Sticky
                    | PluginCellKind::Markdown
                    | PluginCellKind::Html
                    | PluginCellKind::Svg
            )
        );
        if !editable {
            return false;
        }
        let editing = self
            .plugin
            .extras_of(id)
            .get("editing")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        self.plugin
            .set_extra(id, "editing", serde_json::Value::Bool(!editing));
        self.sync_rows_of(ui, &[id.to_string()]);
        self.sync_hover_actions();
        true
    }

    /// Rebuild the hover toolbar's action model for the frozen node (used when
    /// a plugin action's active state changed without the node changing).
    pub fn sync_hover_actions(&mut self) {
        let Some(id) = self.bars.last_node.clone() else {
            return;
        };
        let Some(node) = self.doc.node(&id) else {
            return;
        };
        let extras = plugin_views::toolbar_extras(&self.plugin, node);
        self.hover_actions
            .set_vec(bars::hover_action_rows_with(node, &extras));
    }

    /// Plugin toolbar / manager-driven node action.
    pub fn plugin_action(&mut self, ui: &AppWindow, id: &str, action: &str) {
        match action {
            ACTION_INTERACT => {
                let Some(node) = self.doc.node(id) else {
                    return;
                };
                let interactive = node
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.interactive)
                    .unwrap_or(false);
                self.patch_node_metadata(
                    id,
                    &CanvasNodeMetadata {
                        interactive: Some(!interactive),
                        ..CanvasNodeMetadata::default()
                    },
                );
                self.sync_hover_actions();
            }
            ACTION_GENERATE => {
                self.doc.select_node(id, false);
                self.bars.hover_node = Some(id.to_string());
                self.sync_all(ui);
            }
            ACTION_RESET => {
                self.patch_node_metadata(
                    id,
                    &CanvasNodeMetadata {
                        content: Some(String::new()),
                        ..CanvasNodeMetadata::default()
                    },
                );
                self.plugin
                    .set_extra(id, "editing", serde_json::Value::Bool(false));
                self.sync_hover_actions();
            }
            ACTION_PALETTE => {
                let next = if self.plugin.palette_node.as_deref() == Some(id) {
                    None
                } else {
                    Some(id.to_string())
                };
                self.plugin.palette_node = next;
                self.sync_rows_of(ui, &[id.to_string()]);
                self.sync_hover_actions();
            }
            _ => {}
        }
    }

    /// Editor input from a plugin card: writes `metadata.content`.
    pub fn plugin_text_edited(&mut self, ui: &AppWindow, id: &str, text: &str) {
        self.patch_node_metadata(
            id,
            &CanvasNodeMetadata {
                content: Some(text.to_string()),
                ..CanvasNodeMetadata::default()
            },
        );
        self.sync_rows_of(ui, &[id.to_string()]);
    }

    /// Palette swatch pick: writes the preset color to the `pluginColor` extra.
    pub fn plugin_palette_pick(&mut self, ui: &AppWindow, id: &str, index: i32) {
        let palette = sticky_note::palette();
        let Some(color) = usize::try_from(index)
            .ok()
            .and_then(|index| palette.get(index))
        else {
            return;
        };
        self.plugin.set_extra(
            id,
            sticky_note::COLOR_KEY,
            serde_json::Value::String(color.to_string()),
        );
        self.plugin.palette_node = None;
        self.note_doc_change();
        self.sync_rows_of(ui, &[id.to_string()]);
        self.sync_hover_actions();
    }

    fn patch_node_metadata(&mut self, id: &str, patch: &CanvasNodeMetadata) {
        let now = self.now_ms();
        let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
            return;
        };
        let metadata = node.metadata.clone().unwrap_or_default();
        node.metadata = Some(open_canvas_core::node::merge_metadata(&metadata, patch));
        self.doc.commit(now);
        self.note_doc_change();
    }

    /// Drops plugin state that references nodes no longer in the document.
    pub fn prune_plugin_state(&mut self) {
        self.plugin.prune_extras(&self.doc);
        if let Some(id) = self.plugin.palette_node.clone() {
            if self.doc.node(&id).is_none() {
                self.plugin.palette_node = None;
            }
        }
    }
}
