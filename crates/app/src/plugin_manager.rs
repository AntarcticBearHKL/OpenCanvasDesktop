//! Plugin node creation and plugin-manager handlers on the shared app. Node
//! creation is the dock's per-type buttons ([`App::create_plugin_node`]); the
//! manager opens from the library's top bar. The manager state machine has
//! ui-free cores (`apply_install` / `apply_toggle` / `apply_uninstall`) so the
//! flow is testable headlessly; the Slint wrappers add the row-model refresh
//! and the property sync.

use slint::{ModelRc, VecModel};

use open_canvas_core::node::CanvasNodeType;

use crate::app::App;
use crate::coords::ScreenPos;
use crate::plugin_manager_model::ManagerTab;
use crate::plugin_views;
use crate::AppWindow;

impl App {
    /// Creates a built-in node type at the viewport centre (the dock's node
    /// buttons). An `image` node is refused: it is a display node whose content
    /// arrives from generation or an asset drop, so it can never be created
    /// empty. A `text` node is refused too: it is created by a text-generation
    /// card (the type still renders documents that already contain one).
    /// Returns the new node id, empty when nothing was created.
    pub fn create_plugin_node(&mut self, ui: &AppWindow, type_id: &str) -> String {
        let kind = CanvasNodeType::from_id(type_id);
        if kind == CanvasNodeType::SmartCanvas {
            return self.create_smart_canvas_node(ui);
        }
        // Built-in types are only creatable through the dock's curated list.
        // Plugin-contributed types keep working here for callers that create
        // them directly (drops, restored documents). The legacy prompt type is
        // still reachable from old unit tests; it is not in the dock list and
        // migration removes it from user projects.
        let builtin = self.plugin.registry.get_node_plugin_id(type_id)
            == open_canvas_plugins::registry::BUILTIN_PLUGIN_ID;
        let legacy_test_prompt = cfg!(test) && kind == CanvasNodeType::Prompt;
        if !legacy_test_prompt && builtin && !plugin_views::is_creatable_node_type(type_id) {
            return String::new();
        }
        let world = self.viewport_center_world();
        let spec = self.plugin.spec(type_id);
        let extras = self.plugin.default_extras(type_id);
        let id = self.doc.create_node_with(kind, world, spec, self.now_ms());
        self.plugin.set_extras(&id, extras);
        self.note_doc_change();
        self.sync_all(ui);
        id
    }

    /// Creates a Smart Canvas card and the Image Studio project it owns: the
    /// project comes from the same factory the library's New Image button uses,
    /// the card is named after it and carried its id in metadata. The studio is
    /// not entered here - a double-click on the card opens it.
    fn create_smart_canvas_node(&mut self, ui: &AppWindow) -> String {
        let Some(file) = self.create_image_project_file(ui) else {
            return String::new();
        };
        let project_id = file.project.id.clone();
        let title = file.project.title.clone();
        let world = self.viewport_center_world();
        let mut spec = self.plugin.spec(CanvasNodeType::SmartCanvas.as_str());
        spec.title = title;
        spec.metadata.smart_canvas_project_id = Some(project_id);
        let id = self
            .doc
            .create_node_with(CanvasNodeType::SmartCanvas, world, spec, self.now_ms());
        self.note_doc_change();
        self.sync_all(ui);
        id
    }

    /// World point under the viewport centre: the dock node buttons' placement.
    fn viewport_center_world(&self) -> crate::coords::WorldPos {
        self.doc.viewport().screen_to_world(ScreenPos::new(
            self.canvas_size.0 / 2.0,
            self.canvas_size.1 / 2.0,
        ))
    }

    pub fn open_plugin_manager(&mut self, ui: &AppWindow) {
        self.plugin.ui.manager_open = true;
        // A registry that could not be restored (or a plugin whose persisted
        // manifest did not convert) reports itself once, here.
        if let Some(error) = self.plugin.registry_error.take() {
            self.plugin.ui.message = Some((error, true));
        }
        self.refresh_plugin_rows(ui);
        self.sync_plugin(ui);
    }

    pub fn close_plugin_manager(&mut self, ui: &AppWindow) {
        self.plugin.ui.manager_open = false;
        self.plugin.ui.message = None;
        self.sync_plugin(ui);
        self.sync_all(ui);
    }

    pub fn plugin_tab_pick(&mut self, ui: &AppWindow, tab: &str) {
        if let Some(tab) = ManagerTab::from_id(tab) {
            self.plugin.ui.tab = tab;
        }
        self.refresh_plugin_rows(ui);
        self.sync_plugin(ui);
    }

    pub fn plugin_url_edited(&mut self, ui: &AppWindow, url: &str) {
        self.plugin.ui.url = url.to_string();
        self.sync_plugin(ui);
    }

    /// Official-tab refresh: starts the loading window, rebuilds the rows and
    /// pushes the spinner state; the 60ms tick ends it.
    pub fn plugin_refresh(&mut self, ui: &AppWindow) {
        self.plugin.ui.begin_refresh(self.now_ms());
        self.refresh_plugin_rows(ui);
        self.sync_plugin(ui);
    }

    /// Ends the official-tab loading window when its deadline passed.
    pub fn tick_plugin_refresh(&mut self, ui: &AppWindow) {
        if self.plugin.ui.tick_refresh(self.now_ms()) {
            self.sync_plugin(ui);
        }
    }

    pub fn plugin_install(&mut self, ui: &AppWindow) {
        let url = self.plugin.ui.url.clone();
        self.apply_install(&url);
        self.refresh_after_manager_change(ui);
    }

    pub fn plugin_install_row(&mut self, ui: &AppWindow, url: &str) {
        self.plugin.ui.url = url.to_string();
        self.apply_install(url);
        self.refresh_after_manager_change(ui);
    }

    /// The record-only install state machine step (ui-free core). A successful
    /// install is written to the plugin registry right away, so a URL install
    /// survives a restart.
    pub fn apply_install(&mut self, url: &str) {
        self.plugin.ui.message = Some(match self.plugin.install_from_url(url) {
            Ok(record) => {
                let saved = self.plugin.save_registry();
                (
                    format!(
                        "Installed {} v{} (record only; no fetch){}",
                        record.name,
                        record.version,
                        registry_note(&saved)
                    ),
                    saved.is_err(),
                )
            }
            Err(error) => (error.message().to_string(), true),
        });
    }

    /// `setEnabled` through the host (ui-free core); the registry is written on
    /// every change so the enable/disable state survives a restart.
    pub fn apply_toggle(&mut self, id: &str, enabled: bool) -> bool {
        if !self.plugin.set_enabled(id, enabled) {
            return false;
        }
        let saved = self.plugin.save_registry();
        self.plugin.ui.message = Some((
            format!(
                "{id} {}{}",
                if enabled { "enabled" } else { "disabled" },
                registry_note(&saved)
            ),
            saved.is_err(),
        ));
        true
    }

    /// `uninstall` through the host (ui-free core); `true` when a record was
    /// removed (the message carries the reason either way). The removal is
    /// written to the registry, tombstones included, so an official plugin
    /// stays uninstalled across restarts.
    pub fn apply_uninstall(&mut self, id: &str) -> bool {
        let removed = self.plugin.uninstall(id);
        let saved = match removed {
            Some(_) => self.plugin.save_registry(),
            None => Ok(()),
        };
        self.plugin.ui.message = Some(match removed {
            Some(record) => (
                format!("Uninstalled {}{}", record.name, registry_note(&saved)),
                saved.is_err(),
            ),
            None => (format!("{id} is not installed"), true),
        });
        self.plugin
            .ui
            .message
            .as_ref()
            .is_some_and(|(_, error)| !error)
    }

    pub fn plugin_toggle(&mut self, ui: &AppWindow, id: &str, enabled: bool) {
        self.apply_toggle(id, enabled);
        self.refresh_after_manager_change(ui);
    }

    pub fn plugin_uninstall(&mut self, ui: &AppWindow, id: &str) {
        self.apply_uninstall(id);
        self.refresh_after_manager_change(ui);
    }

    fn refresh_after_manager_change(&mut self, ui: &AppWindow) {
        self.refresh_plugin_rows(ui);
        self.sync_plugin(ui);
        self.sync_rows_full(ui);
    }

    fn refresh_plugin_rows(&self, ui: &AppWindow) {
        ui.set_plugin_rows(ModelRc::new(VecModel::from(plugin_views::manager_rows(
            &self.plugin,
            self.plugin.ui.tab,
        ))));
    }
}

/// `" (not saved: …)"` when a registry write failed; empty on success, so a
/// successful change keeps the message it always had.
fn registry_note(result: &Result<(), String>) -> String {
    match result {
        Ok(()) => String::new(),
        Err(error) => format!(" (not saved: {error})"),
    }
}

#[cfg(test)]
#[path = "plugin_tests/create_menu_tests.rs"]
mod tests;
