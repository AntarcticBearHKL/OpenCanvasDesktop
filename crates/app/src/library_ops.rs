//! Library CRUD actions: Projects (create/select/open/rename/delete) and the
//! Canvases inside them (create/rename/duplicate/delete), mirroring the web
//! `use-canvas-store` semantics — including the two-click arm/confirm delete and
//! "deleting a project deletes its canvases" (`use-canvas-store.ts` deleteGroup).
//!
//! Image projects keep their storage and the shared `create_image_project_file`
//! factory for Smart Canvas nodes; the home screen itself only creates, lists and
//! opens canvas projects.

use open_canvas_core::document::{CanvasGroup, CanvasProject, ProjectIndexEntry};

use crate::app::App;
use crate::image::{ImageFile, PROJECT_KIND_IMAGE};
use crate::library::{is_listed_row, Edit};
use crate::store;
use crate::AppWindow;

impl App {
    /// Adds one project at level 1. The home screen's New Project also mints the
    /// project's first canvas ("Canvas 1") so a project is never empty; the
    /// image-project factory asks for the shell only, keeping the studio asset's
    /// behaviour (an empty project) as it was.
    fn add_project(&mut self, ui: &AppWindow, with_default_canvas: bool) -> String {
        let now = self.now_ms();
        self.library.seq = self.library.seq.wrapping_add(1);
        let id = format!("group-{now}-{}", self.library.seq);
        let name = format!("Project {}", self.library.index.groups.len() + 1);
        self.library.index.groups.push(CanvasGroup {
            id: id.clone(),
            name,
            created_at: store::now_iso(),
        });
        self.library.selected_group = Some(id.clone());
        self.library.editing_group = None;
        self.library.armed_group = None;
        self.library.selected_project = None;
        if with_default_canvas {
            self.library.selected_project = self.new_canvas(ui, &id).map(|canvas| canvas.id);
        }
        self.persist_index(ui);
        self.sync_library(ui);
        id
    }

    /// Persists a fresh canvas inside `group_id` and adds it to the index. The
    /// index is not saved here: the caller persists once after its change.
    fn new_canvas(&mut self, ui: &AppWindow, group_id: &str) -> Option<CanvasProject> {
        let count = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| row.group_id.as_deref() == Some(group_id) && is_listed_row(row))
            .count();
        let now = self.now_ms();
        self.library.seq = self.library.seq.wrapping_add(1);
        let id = format!("project-{now}-{}", self.library.seq);
        let title = format!("Canvas {}", count + 1);
        let stamp = store::now_iso();
        let mut project = CanvasProject::new(id, title, stamp.clone(), stamp);
        project.group_id = Some(group_id.to_string());
        if let Err(error) = self.store.save_project(&project) {
            self.set_notice(ui, format!("Create failed: {error}"), true);
            return None;
        }
        self.library.index.append_project(&project);
        Some(project)
    }

    /// The home screen's New Project: creates the project, gives it one default
    /// canvas ("Canvas 1") and selects the project in the sidebar. It stays on
    /// the home screen; a single click is what opens a canvas.
    pub fn library_create_group(&mut self, ui: &AppWindow) {
        let _project_id = self.add_project(ui, true);
    }

    pub fn library_select_group(&mut self, ui: &AppWindow, id: &str) {
        if self.library.index.group(id).is_none() {
            return;
        }
        self.library.selected_group = Some(id.to_string());
        self.library.editing_group = None;
        self.library.armed_group = None;
        self.library.selected_project = None;
        self.sync_library(ui);
    }

    /// Double-click on a sidebar project: select it and open its most recently
    /// edited canvas (newest `updated_at`; index order breaks a tie). A project
    /// with no canvases simply gets selected — its empty state offers New Canvas.
    fn open_group_canvas(&mut self, ui: &AppWindow, group_id: &str) {
        self.library.selected_group = Some(group_id.to_string());
        self.library.editing_group = None;
        self.library.armed_group = None;
        let latest = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| row.group_id.as_deref() == Some(group_id) && is_listed_row(row))
            .max_by(|a, b| {
                a.updated_at
                    .cmp(&b.updated_at)
                    .then_with(|| a.id.cmp(&b.id))
            })
            .map(|row| row.id.clone());
        self.sync_library(ui);
        if let Some(canvas_id) = latest {
            self.library.selected_project = Some(canvas_id.clone());
            self.library_open_project(ui, &canvas_id);
        }
    }

    /// Project actions: `open` (double-click on the project row), `rename` (inline editor), `delete`
    /// (two-click arm/confirm). Deleting a project deletes its canvases too
    /// (canvas and the hidden image projects alike).
    pub fn library_group_action(&mut self, ui: &AppWindow, id: &str, action: &str) {
        let Some(group) = self.library.index.group(id) else {
            return;
        };
        if action == "open" {
            self.open_group_canvas(ui, id);
            return;
        }
        match action {
            "rename" => {
                self.library.editing_group = Some(Edit {
                    id: id.to_string(),
                    draft: group.name.clone(),
                });
                self.library.armed_group = None;
            }
            "rename-save" => {
                let Some(edit) = self.library.editing_group.take() else {
                    return;
                };
                let name = edit.draft.trim().to_string();
                if !name.is_empty() {
                    if let Some(group) = self
                        .library
                        .index
                        .groups
                        .iter_mut()
                        .find(|group| group.id == edit.id)
                    {
                        group.name = name;
                    }
                    self.persist_index(ui);
                }
            }
            "rename-cancel" => self.library.editing_group = None,
            "delete" => {
                if self.library.armed_group.as_deref() == Some(id) {
                    self.library.armed_group = None;
                    self.delete_group(ui, id);
                    return;
                }
                self.library.armed_group = Some(id.to_string());
            }
            _ => return,
        }
        self.sync_library(ui);
    }

    fn delete_group(&mut self, ui: &AppWindow, id: &str) {
        let doomed: Vec<(String, String)> = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| row.group_id.as_deref() == Some(id))
            .map(|row| (row.id.clone(), row.kind.clone()))
            .collect();
        for (project_id, kind) in &doomed {
            let result = self.store.delete_by_kind(project_id, kind);
            if let Err(error) = result {
                self.set_notice(ui, format!("Delete failed: {error}"), true);
            }
            self.library.index.remove_project(project_id);
        }
        self.library.index.groups.retain(|group| group.id != id);
        self.library.editing_group = None;
        self.library.editing_project = None;
        self.library.armed_project = None;
        self.library.selected_project = None;
        if self
            .image
            .meta
            .as_ref()
            .is_some_and(|meta| doomed.iter().any(|(project_id, _)| project_id == &meta.id))
        {
            self.image.meta = None;
        }
        self.pick_valid_group();
        self.persist_index(ui);
        self.sync_library(ui);
    }

    pub fn library_group_name_edited(&mut self, _ui: &AppWindow, text: &str) {
        if let Some(edit) = self.library.editing_group.as_mut() {
            edit.draft = text.to_string();
        }
    }

    /// The right pane's New Canvas: creates a canvas in the selected project and
    /// opens it (web `createAndEnter`), so adding a canvas and starting to work
    /// stay one action.
    pub fn library_create_project(&mut self, ui: &AppWindow) {
        let Some(group_id) = self.library.selected_group.clone() else {
            return;
        };
        let Some(project) = self.new_canvas(ui, &group_id) else {
            return;
        };
        self.persist_index(ui);
        self.open_project_ui(ui, project);
    }

    /// The project a new canvas/image lands in: the selection when there is one,
    /// the first project otherwise, creating the first project when the library
    /// is still empty. The implicit project is a shell (no default canvas), so
    /// the image factory keeps its old "empty project" shape.
    pub(crate) fn new_project_group_id(&mut self, ui: &AppWindow) -> Option<String> {
        if self.library.selected_group.is_none() {
            if self.library.index.groups.is_empty() {
                self.add_project(ui, false);
            } else {
                self.library.selected_group = Some(self.library.index.groups[0].id.clone());
                self.sync_library(ui);
            }
        }
        self.library.selected_group.clone()
    }

    /// The shared "New Image" project factory: persists a fresh image project
    /// (640x360 `16:9` `2k` board with one pixel layer) and adds it to the
    /// library index, without switching screens. A Smart Canvas node uses it for
    /// its own project and stays on the canvas; the home screen no longer offers
    /// a "New Image" entry, so nothing else calls it.
    pub(crate) fn create_image_project_file(&mut self, ui: &AppWindow) -> Option<ImageFile> {
        let group_id = self.new_project_group_id(ui)?;
        let now = self.now_ms();
        self.library.seq = self.library.seq.wrapping_add(1);
        let id = format!("image-{now}-{}", self.library.seq);
        let count = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| crate::image::is_image_kind(&row.kind))
            .count();
        let title = format!("Image {}", count + 1);
        let stamp = store::now_iso();
        let mut gen = open_canvas_ps::IdGen::new(&format!("ps-{now}"));
        let layer = open_canvas_ps::create_ps_pixel_layer(
            &mut gen,
            (crate::image::DEFAULT_WIDTH, crate::image::DEFAULT_HEIGHT),
            "Layer 1",
        );
        let project = open_canvas_ps::ImageProject {
            id,
            title,
            created_at: stamp.clone(),
            updated_at: stamp,
            group_id,
            width: crate::image::DEFAULT_WIDTH,
            height: crate::image::DEFAULT_HEIGHT,
            board_ratio: crate::image::DEFAULT_RATIO.to_string(),
            board_resolution: open_canvas_ps::doc::BoardResolution::K2,
            board_background: "transparent".to_string(),
            board_background_opacity: 1.0,
            board_layers: vec![layer],
            board_paths: None,
            board_alpha_channels: None,
            board_channel_visibility: None,
        };
        let file = match ImageFile::from_state(
            &project,
            &std::collections::HashMap::new(),
            &crate::image::assets::ImageAssets::default(),
        ) {
            Ok(file) => file,
            Err(error) => {
                self.set_notice(ui, format!("Create failed: {error}"), true);
                return None;
            }
        };
        if let Err(error) = self.store.save_image(&file) {
            self.set_notice(ui, format!("Create failed: {error}"), true);
            return None;
        }
        self.library.index.upsert_entry(ProjectIndexEntry {
            id: project.id.clone(),
            title: project.title.clone(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            group_id: Some(project.group_id.clone()),
            kind: PROJECT_KIND_IMAGE.to_string(),
        });
        self.persist_index(ui);
        Some(file)
    }

    /// Project actions: `open`, `select`, `rename` (inline editor), `duplicate`,
    /// `delete` (arm/confirm).
    pub fn library_project_action(&mut self, ui: &AppWindow, id: &str, action: &str) {
        let Some(row) = self.library.index.project(id) else {
            return;
        };
        match action {
            "open" => self.library_open_project(ui, id),
            "select" => {
                self.library.selected_project = Some(id.to_string());
                self.library.armed_project = None;
                self.sync_library(ui);
            }
            "rename" => {
                self.library.editing_project = Some(Edit {
                    id: id.to_string(),
                    draft: row.title.clone(),
                });
                self.library.armed_project = None;
                self.sync_library(ui);
            }
            "rename-save" => {
                let Some(edit) = self.library.editing_project.take() else {
                    return;
                };
                let title = edit.draft.trim().to_string();
                if !title.is_empty() {
                    self.rename_project(ui, &edit.id, &title);
                }
                self.sync_library(ui);
            }
            "rename-cancel" => {
                self.library.editing_project = None;
                self.sync_library(ui);
            }
            "duplicate" => self.duplicate_project(ui, id),
            "delete" => {
                if self.library.armed_project.as_deref() == Some(id) {
                    self.library.armed_project = None;
                    self.delete_project(ui, id);
                } else {
                    self.library.armed_project = Some(id.to_string());
                    self.sync_library(ui);
                }
            }
            _ => {}
        }
    }

    /// Copies a canvas into the same project: a fresh id and title, the
    /// same document (nodes, connections, media) and the plugin extras sidecar,
    /// so a duplicated canvas is fully self-contained.
    fn duplicate_project(&mut self, ui: &AppWindow, id: &str) {
        if self
            .library
            .index
            .project(id)
            .is_some_and(|row| crate::image::is_image_kind(&row.kind))
        {
            return;
        }
        let Ok(source) = self.store.load_project(id) else {
            self.set_notice(
                ui,
                "Duplicate failed: the project could not be read".to_string(),
                true,
            );
            return;
        };
        let now = self.now_ms();
        self.library.seq = self.library.seq.wrapping_add(1);
        let copy_id = format!("project-{now}-{}", self.library.seq);
        let stamp = store::now_iso();
        let mut copy = source;
        copy.id.clone_from(&copy_id);
        copy.title = format!("Copy of {}", copy.title);
        copy.created_at.clone_from(&stamp);
        copy.updated_at = stamp;
        if let Err(error) = self.store.save_project(&copy) {
            self.set_notice(ui, format!("Duplicate failed: {error}"), true);
            return;
        }
        if let Ok(extras) = self.store.load_project_extras(id) {
            if let Err(error) = self.store.save_project_extras(&copy_id, &extras) {
                self.set_notice(ui, format!("Duplicate failed: {error}"), true);
            }
        }
        self.library.index.upsert_project(&copy);
        self.library.selected_project = Some(copy_id);
        self.persist_index(ui);
        self.sync_library(ui);
    }

    /// Side-panel reorder: swaps the row with its neighbour **inside the
    /// visible (project filtered) list** and persists the index order (the panel
    /// is the only place the user orders projects).
    pub fn library_side_reorder(&mut self, ui: &AppWindow, id: &str, direction: &str) {
        let ids = self.filtered_project_ids();
        let Some(index) = ids.iter().position(|row| row == id) else {
            return;
        };
        let target = match direction {
            "up" => index.checked_sub(1),
            "down" if index + 1 < ids.len() => Some(index + 1),
            _ => None,
        };
        let Some(target) = target else {
            return;
        };
        let mut next = ids;
        next.swap(index, target);
        self.apply_filtered_order(&next);
        self.persist_index(ui);
        self.sync_library(ui);
    }

    /// Drag reorder for the home sidebar: move a project group to another
    /// visible index and persist the new group order.
    pub fn library_group_reorder(&mut self, ui: &AppWindow, from: i32, to: i32) {
        let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
            return;
        };
        let len = self.library.index.groups.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let group = self.library.index.groups.remove(from);
        self.library.index.groups.insert(to, group);
        self.persist_index(ui);
        self.sync_library(ui);
    }

    /// Drag reorder for the canvas table: move a canvas within the visible
    /// project list and keep the non-canvas index slots stable.
    pub fn library_project_reorder(&mut self, ui: &AppWindow, from: i32, to: i32) {
        let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
            return;
        };
        let mut ids = self.filtered_project_ids();
        if from >= ids.len() || to >= ids.len() || from == to {
            return;
        }
        let id = ids.remove(from);
        ids.insert(to, id);
        self.apply_filtered_order(&ids);
        self.persist_index(ui);
        self.sync_library(ui);
    }

    /// Renames a project through the one path the table's inline rename and the
    /// side panel share: the stored file, the in-memory meta, the index row and
    /// the persisted index.
    pub fn rename_project(&mut self, ui: &AppWindow, id: &str, title: &str) {
        let mut result = Ok(());
        let kind = self
            .library
            .index
            .project(id)
            .map_or_else(String::new, |row| row.kind.clone());
        if crate::image::is_image_kind(&kind) {
            if let Ok(mut file) = self.store.load_image(id) {
                file.project.title = title.to_string();
                result = self.store.save_image(&file);
            }
            if let Some(meta) = self.image.meta.as_mut().filter(|meta| meta.id == id) {
                meta.title = title.to_string();
            }
        } else if let Ok(mut project) = self.store.load_project(id) {
            project.title = title.to_string();
            result = self.store.save_project(&project);
        }
        if let Some(row) = self
            .library
            .index
            .projects
            .iter_mut()
            .find(|row| row.id == id)
        {
            row.title = title.to_string();
        }
        if let Some(meta) = self.editor.as_mut().filter(|meta| meta.id == id) {
            meta.title = title.to_string();
        }
        if let Err(error) = result {
            self.set_notice(ui, format!("Rename failed: {error}"), true);
        }
        self.persist_index(ui);
    }

    fn delete_project(&mut self, ui: &AppWindow, id: &str) {
        let kind = self
            .library
            .index
            .project(id)
            .map_or_else(String::new, |row| row.kind.clone());
        if let Err(error) = self.store.delete_by_kind(id, &kind) {
            self.set_notice(ui, format!("Delete failed: {error}"), true);
            return;
        }
        self.library.index.remove_project(id);
        self.library.editing_project = None;
        if self.library.selected_project.as_deref() == Some(id) {
            self.library.selected_project = None;
        }
        if self.image.meta.as_ref().is_some_and(|meta| meta.id == id) {
            self.image.meta = None;
        }
        self.persist_index(ui);
        self.sync_library(ui);
    }

    pub fn library_project_name_edited(&mut self, _ui: &AppWindow, text: &str) {
        if let Some(edit) = self.library.editing_project.as_mut() {
            edit.draft = text.to_string();
        }
    }
}
