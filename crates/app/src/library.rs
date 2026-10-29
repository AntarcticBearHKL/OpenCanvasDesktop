//! The home screen's project library: Projects (level 1) holding Canvases,
//! the canvas table, create/open/rename/duplicate/delete and the library →
//! editor navigation. Mirrors the web `pages/canvas/index.tsx` (one flat
//! surface: project sidebar, header row, canvas table) and the
//! `use-canvas-store` CRUD semantics (delete-project also deletes its
//! canvases), minus cross-tab sync.
//!
//! Rust owns the state; the Slint rows ([`LibraryGroupRow`] = one project in
//! the sidebar, [`ProjectListRow`] = one canvas in the right pane) are rebuilt
//! from [`Library::index`] on every change. Image projects keep their storage
//! and their project factory for Smart Canvas nodes, but they are never listed
//! here: the home screen is the canvas library. The export/import flows live in
//! `library_export.rs`, the callback wiring in `library_wire.rs`.

use open_canvas_core::document::{CanvasProject, LibraryIndex, ProjectIndexEntry};
use slint::{Model as _, VecModel};

use crate::app::{App, View};
use crate::doc::Doc;
use crate::store;
use crate::{AppWindow, LibraryGroupRow, ProjectListRow};

/// The open editor's identity (content lives in [`Doc`]).
///
/// Defined next to [`Doc::to_project`] in `open-canvas-app-core`
/// (`doc_project::EditorMeta`) because the save shape is domain data; re-exported
/// here so the editor/sidebar code keeps using `crate::library::EditorMeta`.
pub use open_canvas_app_core::doc_project::EditorMeta;

/// One in-place edit (project rename / canvas rename): the id being edited plus
/// its draft text (web `editingGroupId` + `editingGroupName` and
/// `editingProjectId` + `editingProjectTitle`).
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub id: String,
    pub draft: String,
}

/// Whether the painted table rows are the same canvases, in the same order, as
/// `rows`. A selection (or an inline edit) then updates rows in place, keeping
/// the repeater's item instances alive; a canvas-set change rebuilds instead.
fn same_canvas_ids(model: &VecModel<ProjectListRow>, rows: &[ProjectListRow]) -> bool {
    model.row_count() == rows.len()
        && model
            .iter()
            .zip(rows)
            .all(|(painted, next)| painted.id == next.id)
}

/// Export/import modal state (the path prompt both flows share).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportFlow {
    pub import: bool,
    pub project_id: String,
    pub path: String,
    pub message: String,
    pub error: bool,
}

/// Library UI state (web: the canvas store's group/project arrays plus the UI
/// store's editing/selection bits).
#[derive(Debug)]
pub struct Library {
    pub index: LibraryIndex,
    pub selected_group: Option<String>,
    pub editing_group: Option<Edit>,
    pub armed_group: Option<String>,
    pub editing_project: Option<Edit>,
    pub armed_project: Option<String>,
    pub export: Option<ExportFlow>,
    /// The canvas row the user clicked last (`selectedProjectId`): the table
    /// paints it even though a single click is what actually opens the canvas.
    pub selected_project: Option<String>,
    /// Id suffix counter for freshly minted project/canvas ids.
    pub(crate) seq: u64,
    /// Canvas node counts for the side panel's rows, keyed by canvas id and
    /// invalidated by `updated_at` so a sync never re-reads the whole library.
    pub side_counts: std::collections::HashMap<String, (String, i32)>,
    /// The `project|canvas ids` signature of the last painted canvas table. A
    /// different signature (project switch, canvas added/removed) marks the
    /// rows for their short enter fade; the very first paint (empty signature)
    /// stays calm.
    pub(crate) rows_key: String,
}

impl Default for Library {
    /// A fresh library shows the first project's canvases; image projects
    /// exist in the index (Smart Canvas) but are never rows.
    fn default() -> Self {
        Self {
            index: LibraryIndex::default(),
            selected_group: None,
            editing_group: None,
            armed_group: None,
            editing_project: None,
            armed_project: None,
            export: None,
            selected_project: None,
            seq: 0,
            side_counts: std::collections::HashMap::new(),
            rows_key: String::new(),
        }
    }
}

/// A project row belongs to the home table only when it is a canvas project:
/// image projects stay in storage for Smart Canvas and are listed nowhere.
pub(crate) fn is_listed_row(row: &ProjectIndexEntry) -> bool {
    !crate::image::is_image_kind(&row.kind)
}

impl App {
    /// Node-count chip of one side-panel row. The open project reads the live
    /// document; every other canvas project is read once and cached against its
    /// `updated_at`, so a save-triggered library sync does no extra I/O.
    fn side_node_count(&mut self, id: &str, updated_at: &str) -> i32 {
        if self.editor.as_ref().is_some_and(|meta| meta.id == id) {
            return i32::try_from(self.doc.nodes().len()).unwrap_or(i32::MAX);
        }
        if let Some((stamp, count)) = self.library.side_counts.get(id) {
            if stamp == updated_at {
                return *count;
            }
        }
        let count = self.store.load_project(id).map_or(0, |project| {
            i32::try_from(project.nodes.len()).unwrap_or(i32::MAX)
        });
        self.library
            .side_counts
            .insert(id.to_string(), (updated_at.to_string(), count));
        count
    }

    /// Ids of the selected project's canvas rows, in index order: that is the
    /// visible order (and the persisted order) of both the table and the side
    /// panel's canvas project list.
    pub(crate) fn filtered_project_ids(&self) -> Vec<String> {
        let selected = self.library.selected_group.as_deref();
        self.library
            .index
            .projects
            .iter()
            .filter(|row| row.group_id.as_deref() == selected && is_listed_row(row))
            .map(|row| row.id.clone())
            .collect()
    }

    /// Applies a filtered order back onto `index.projects` in the slots the
    /// filtered rows already occupy, so other projects keep their positions.
    pub(crate) fn apply_filtered_order(&mut self, next: &[String]) {
        let ids: Vec<String> = next.to_vec();
        let mut entries: std::collections::HashMap<String, ProjectIndexEntry> = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| ids.contains(&row.id))
            .map(|row| (row.id.clone(), row.clone()))
            .collect();
        let slots: Vec<usize> = self
            .library
            .index
            .projects
            .iter()
            .enumerate()
            .filter(|(_, row)| ids.contains(&row.id))
            .map(|(index, _)| index)
            .collect();
        for (slot, id) in slots.into_iter().zip(next) {
            if let Some(entry) = entries.remove(id) {
                self.library.index.projects[slot] = entry;
            }
        }
    }

    /// Canvases whose project vanished fall back to the first project (web rehydrate
    /// repair, use-canvas-store.ts:213-225); the fix is persisted on next save.
    pub(crate) fn repair_groups(&mut self) {
        let fallback = self
            .library
            .index
            .groups
            .first()
            .map(|group| group.id.clone());
        let groups: Vec<String> = self
            .library
            .index
            .groups
            .iter()
            .map(|group| group.id.clone())
            .collect();
        for row in &mut self.library.index.projects {
            let valid = row.group_id.as_ref().is_some_and(|id| groups.contains(id));
            if !valid {
                row.group_id.clone_from(&fallback);
            }
        }
    }

    pub(crate) fn pick_valid_group(&mut self) {
        let valid = self
            .library
            .selected_group
            .as_ref()
            .is_some_and(|id| self.library.index.group(id).is_some());
        if !valid {
            self.library.selected_group = self
                .library
                .index
                .groups
                .first()
                .map(|group| group.id.clone());
        }
    }

    /// Rebuilds the project sidebar / canvas table / side-panel row models for
    /// the window.
    #[expect(
        clippy::too_many_lines,
        reason = "one flat rebuild of the three row models from the index; splitting hides the shared selection/edit state"
    )]
    pub fn sync_library(&mut self, ui: &AppWindow) {
        let editing_group = self.library.editing_group.clone();
        let armed_group = self.library.armed_group.clone();
        let selected_group = self.library.selected_group.clone();
        let editing_project = self.library.editing_project.clone();
        let armed_project = self.library.armed_project.clone();
        let selected_project = self.library.selected_project.clone();

        // The canvas table's rows: the selected project's canvas projects.
        let rows: Vec<ProjectIndexEntry> = self
            .library
            .index
            .projects
            .iter()
            .filter(|row| {
                row.group_id.as_deref() == selected_group.as_deref() && is_listed_row(row)
            })
            .cloned()
            .collect();

        // A project switch or a canvas set change replays the table's enter
        // fade. Renames keep the ids, so they do not restart it.
        let mut key = selected_group.clone().unwrap_or_default();
        key.push('|');
        for row in &rows {
            key.push_str(row.id.as_str());
            key.push(',');
        }
        let faded = !self.library.rows_key.is_empty() && self.library.rows_key != key;
        self.library.rows_key = key;

        let groups: Vec<LibraryGroupRow> = self
            .library
            .index
            .groups
            .iter()
            .map(|group| {
                let editing = editing_group
                    .as_ref()
                    .is_some_and(|edit| edit.id == group.id);
                LibraryGroupRow {
                    id: group.id.as_str().into(),
                    name: if editing {
                        editing_group
                            .as_ref()
                            .map_or(group.name.as_str(), |edit| edit.draft.as_str())
                    } else {
                        group.name.as_str()
                    }
                    .into(),
                    active: selected_group.as_deref() == Some(group.id.as_str()),
                    editing,
                    armed: armed_group.as_deref() == Some(group.id.as_str()),
                }
            })
            .collect();

        let projects: Vec<ProjectListRow> = rows
            .iter()
            .map(|row| {
                let editing = editing_project
                    .as_ref()
                    .is_some_and(|edit| edit.id == row.id);
                ProjectListRow {
                    id: row.id.as_str().into(),
                    title: if editing {
                        editing_project
                            .as_ref()
                            .map_or(row.title.as_str(), |edit| edit.draft.as_str())
                    } else {
                        row.title.as_str()
                    }
                    .into(),
                    updated: store::display_time(&row.updated_at).into(),
                    editing,
                    armed: armed_project.as_deref() == Some(row.id.as_str()),
                    selected: selected_project.as_deref() == Some(row.id.as_str()),
                    fade: faded,
                }
            })
            .collect();

        self.library_groups.set_vec(groups);
        // Same canvas set (a selection, a rename, an arm): update the rows in
        // place so the repeater keeps its item instances. A full model reset
        // would rebuild every row between two rapid clicks, and the second
        // click would land on a fresh item. A canvas-set change keeps the
        // reset, which is what replays the fade.
        if !faded && same_canvas_ids(&self.library_projects, &projects) {
            for (index, row) in projects.into_iter().enumerate() {
                self.library_projects.set_row_data(index, row);
            }
        } else {
            self.library_projects.set_vec(projects);
        }
        let open_id = self.editor.as_ref().map(|meta| meta.id.clone());
        // The side panel lists the same canvas rows as the home table, in the
        // same order.
        let project_rows: Vec<(String, String, String)> = rows
            .iter()
            .map(|row| (row.id.clone(), row.title.clone(), row.updated_at.clone()))
            .collect();
        let armed_project = self.library.armed_project.clone();
        let row_total = project_rows.len();
        let mut side_rows: Vec<crate::SidePanelRow> = Vec::with_capacity(row_total);
        for (index, (id, title, updated)) in project_rows.iter().enumerate() {
            let node_count = self.side_node_count(id, updated);
            side_rows.push(crate::SidePanelRow {
                id: id.as_str().into(),
                title: title.as_str().into(),
                active: open_id.as_deref() == Some(id.as_str()),
                node_count,
                can_up: index > 0,
                can_down: index + 1 < row_total,
                armed: armed_project.as_deref() == Some(id.as_str()),
            });
        }
        self.side_rows.set_vec(side_rows);
        let project_name = match &editing_group {
            Some(edit) if selected_group.as_deref() == Some(edit.id.as_str()) => edit.draft.clone(),
            _ => self
                .library
                .selected_group
                .as_deref()
                .and_then(|id| self.library.index.group(id))
                .map_or_else(|| "No project".to_string(), |group| group.name.clone()),
        };
        ui.set_side_group(project_name.clone().into());
        ui.set_library_group_title(project_name.into());
        ui.set_library_has_group(self.library.selected_group.is_some());
        self.sync_export(ui);
    }

    pub fn library_open_project(&mut self, ui: &AppWindow, id: &str) {
        let kind = self
            .library
            .index
            .project(id)
            .map_or_else(String::new, |row| row.kind.clone());
        if crate::image::is_image_kind(&kind) {
            match self.store.load_image(id) {
                Ok(file) => self.open_image_ui(ui, file),
                Err(error) => self.set_notice(ui, format!("Open failed: {error}"), true),
            }
            return;
        }
        match self.store.load_project(id) {
            Ok(project) => self.open_project_ui(ui, project),
            Err(error) => self.set_notice(ui, format!("Open failed: {error}"), true),
        }
    }

    /// Enters the editor with `project`: fresh document, reset history, cleared
    /// transient overlays and a clean save baseline. The plugin extras sidecar
    /// is loaded alongside (a failure reports and starts from empty extras).
    pub(crate) fn open_project_ui(&mut self, ui: &AppWindow, project: CanvasProject) {
        self.flush_save(ui);
        self.doc = Doc::from_project(&project);
        let migration = self.doc.migrate_legacy_workflows();
        if !migration.is_empty() {
            self.set_notice(ui, migration.summary(), false);
            self.save_dirty = true;
            self.save_due = Some(self.now_ms());
        }
        match self.store.load_project_extras(&project.id) {
            Ok(extras) => self.plugin.load_extras(extras),
            Err(error) => {
                self.plugin.load_extras(std::collections::BTreeMap::new());
                self.set_notice(
                    ui,
                    format!("Plugin metadata could not be loaded ({error})"),
                    true,
                );
            }
        }
        self.plugin.palette_node = None;
        self.clear_library_edits();
        self.library.selected_project = Some(project.id.clone());
        self.editor = Some(EditorMeta {
            id: project.id,
            title: project.title,
            created_at: project.created_at,
            updated_at: project.updated_at,
            group_id: project.group_id,
        });
        self.view = View::Editor;
        self.bars = crate::bars::Bars::default();
        self.panel = crate::node_ui::Panel::default();
        self.attempt_modal = None;
        ui.set_attempt_open(false);
        self.save_dirty = false;
        self.save_due = None;
        self.sync_all(ui);
    }

    /// Back to the home screen: saves the open document first.
    ///
    /// A studio entered from a Smart Canvas card is not "home" yet: its back
    /// action returns to the canvas project it came from, and only a studio
    /// opened from the library returns to the library.
    pub fn go_home(&mut self, ui: &AppWindow) {
        // A long right-hold or any other Home action cancels the ring; the
        // property is also cleared here so no route can leave a stale overlay.
        self.home_hold = None;
        ui.set_home_hold_visible(false);
        ui.set_home_hold_progress(0.0);
        ui.set_home_hold_arc("".into());
        if self.view == View::Image {
            if let Some(origin) = self.image.origin_canvas.take() {
                self.flush_save(ui);
                if let Ok(project) = self.store.load_project(&origin) {
                    self.open_project_ui(ui, project);
                    return;
                }
                self.set_notice(
                    ui,
                    "The origin canvas could not be reopened".to_string(),
                    true,
                );
            }
        }
        self.flush_save(ui);
        self.editor = None;
        self.doc = Doc::empty();
        self.plugin.load_extras(std::collections::BTreeMap::new());
        self.plugin.palette_node = None;
        self.plugin.ui.manager_open = false;
        self.view = View::Library;
        self.bars = crate::bars::Bars::default();
        self.panel = crate::node_ui::Panel::default();
        self.attempt_modal = None;
        ui.set_attempt_open(false);
        self.save_dirty = false;
        self.save_due = None;
        self.sync_all(ui);
    }

    /// True when the named workspace has an open project to return to. The
    /// Image Studio still asks this when its back/Home action decides where it
    /// came from (Smart Canvas round-trip); only its tests reach it now that the
    /// workspace tabs are gone.
    #[cfg(test)]
    pub fn workspace_open(&self, id: &str) -> bool {
        match id {
            "canvas" => self.editor.is_some(),
            "image" => self.image.meta.is_some(),
            _ => false,
        }
    }

    pub(crate) fn persist_index(&mut self, ui: &AppWindow) {
        if let Err(error) = self.store.save_index(&self.library.index) {
            self.set_notice(ui, format!("Library save failed: {error}"), true);
        }
    }

    pub(crate) fn persist_index_quiet(&mut self) {
        if let Err(error) = self.store.save_index(&self.library.index) {
            eprintln!("library: index save failed: {error}");
        }
    }

    /// Drops pending rename/delete states (switching screens leaves no inline
    /// editor open, like the web's unmount).
    pub(crate) fn clear_library_edits(&mut self) {
        self.library.editing_group = None;
        self.library.armed_group = None;
        self.library.editing_project = None;
        self.library.armed_project = None;
    }
}

#[cfg(test)]
#[path = "library_table_tests.rs"]
mod table_tests;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::error::Error;
    use std::rc::Rc;

    use open_canvas_core::document::{CanvasGroup, PROJECT_KIND_CANVAS};

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    fn entry(id: &str, group: &str, kind: &str) -> ProjectIndexEntry {
        ProjectIndexEntry {
            id: id.into(),
            title: id.into(),
            created_at: String::new(),
            updated_at: String::new(),
            group_id: Some(group.into()),
            kind: kind.into(),
        }
    }

    /// One window + app on an empty data dir, with a single project holding two
    /// canvases and two image projects. The image rows stay in the index
    /// (Smart Canvas) but must never surface as library rows.
    fn fixture() -> Result<(tempfile::TempDir, AppWindow, App), Box<dyn Error>> {
        let dir = tempfile::TempDir::new()?;
        let ui = AppWindow::new()?;
        let mut app = App::new(store::Store::at(dir.path().to_path_buf()));
        let group = "g1";
        app.library.index.groups = vec![CanvasGroup {
            id: group.into(),
            name: "Project A".into(),
            created_at: String::new(),
        }];
        app.library.index.projects = vec![
            entry("c1", group, PROJECT_KIND_CANVAS),
            entry("i1", group, crate::image::PROJECT_KIND_IMAGE),
            entry("c2", group, PROJECT_KIND_CANVAS),
            entry("i2", group, crate::image::PROJECT_KIND_IMAGE),
        ];
        app.library.selected_group = Some(group.into());
        app.sync_library(&ui);
        Ok((dir, ui, app))
    }

    fn row_ids(app: &App) -> Vec<String> {
        app.library_projects
            .iter()
            .map(|row| row.id.to_string())
            .collect()
    }

    fn side_ids(app: &App) -> Vec<String> {
        app.side_rows.iter().map(|row| row.id.to_string()).collect()
    }

    /// The table lists the selected project's canvases only, in index order;
    /// image projects never appear (not even as side-panel rows).
    #[test]
    fn the_table_lists_only_canvas_projects() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, _ui, app) = fixture()?;
        assert_eq!(row_ids(&app), vec!["c1".to_string(), "c2".to_string()]);
        assert_eq!(side_ids(&app), row_ids(&app), "panel and table agree");
        assert!(
            !row_ids(&app).contains(&"i1".to_string()),
            "a Smart Canvas image project is not a library row"
        );
        Ok(())
    }

    /// A project selection narrows the table to that project's canvases.
    #[test]
    fn the_table_follows_the_selected_project() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        app.library.index.groups.push(CanvasGroup {
            id: "g2".into(),
            name: "Archive".into(),
            created_at: String::new(),
        });
        for row in &mut app.library.index.projects {
            if row.id == "c2" || row.id == "i2" {
                row.group_id = Some("g2".into());
            }
        }
        app.sync_library(&ui);

        assert_eq!(row_ids(&app), vec!["c1".to_string()], "Project A only");
        assert_eq!(ui.get_library_group_title().as_str(), "Project A");
        app.library_select_group(&ui, "g2");
        assert_eq!(
            row_ids(&app),
            vec!["c2".to_string()],
            "Archive holds one canvas"
        );
        assert_eq!(
            ui.get_library_group_title().as_str(),
            "Archive",
            "the header names the project without a kind suffix"
        );
        assert_eq!(side_ids(&app), row_ids(&app), "panel and table agree");
        Ok(())
    }

    #[test]
    fn side_panel_reorder_moves_rows_and_reports_its_bounds() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;

        assert_eq!(
            side_ids(&app),
            vec!["c1".to_string(), "c2".to_string()],
            "canvas rows only"
        );
        let first = app.side_rows.row_data(0).ok_or("row 0")?;
        assert!(!first.can_up, "the head row cannot move up");
        assert!(first.can_down, "the head row can move down");
        let last = app.side_rows.row_data(1).ok_or("row 1")?;
        assert!(last.can_up && !last.can_down, "the tail row's bounds");

        app.library_side_reorder(&ui, "c2", "up");
        assert_eq!(
            side_ids(&app),
            vec!["c2".to_string(), "c1".to_string()],
            "moved one step inside the canvas rows"
        );
        assert_eq!(
            app.library
                .index
                .projects
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
            vec!["c2", "i1", "c1", "i2"],
            "only the canvas slots swapped; the image rows keep their places"
        );
        Ok(())
    }

    #[test]
    fn side_panel_arms_and_confirms_a_project_delete() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        assert!(app.library.armed_project.is_none());

        // First click arms only; the row survives and paints the confirm state.
        app.library_project_action(&ui, "c1", "delete");
        assert_eq!(app.library.armed_project.as_deref(), Some("c1"));
        assert!(
            app.side_rows.row_data(0).ok_or("row 0")?.armed,
            "the row shows the confirmation"
        );
        assert_eq!(app.library.index.projects.len(), 4, "nothing deleted yet");

        // Second click deletes the row.
        app.library_project_action(&ui, "c1", "delete");
        assert!(app.library.index.project("c1").is_none());
        assert_eq!(
            row_ids(&app),
            vec!["c2".to_string()],
            "the deleted row leaves"
        );
        Ok(())
    }

    /// The header's New Canvas button creates a canvas project and opens it;
    /// image projects are never created from the home screen.
    #[test]
    fn new_canvas_creates_and_opens_a_canvas_project() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        let before: Vec<String> = app
            .library
            .index
            .projects
            .iter()
            .map(|row| row.id.clone())
            .collect();
        app.library_create_project(&ui);

        let created = app
            .library
            .index
            .projects
            .iter()
            .find(|row| !before.contains(&row.id))
            .ok_or("the button created a project")?;
        assert_eq!(created.kind, PROJECT_KIND_CANVAS);
        assert_eq!(app.view, View::Editor);
        assert!(
            ui.get_library_has_group(),
            "the home header still knows its project"
        );
        Ok(())
    }

    /// A single click selects a row; a row action can duplicate it into the same
    /// project with a copy title and an independent file.
    #[test]
    fn a_row_can_be_selected_and_duplicated() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        let mut source = CanvasProject::new(
            "c1".to_string(),
            "Sketch".to_string(),
            String::new(),
            String::new(),
        );
        source.group_id = Some("g1".to_string());
        app.store.save_project(&source)?;

        app.library_project_action(&ui, "c1", "select");
        assert_eq!(app.library.selected_project.as_deref(), Some("c1"));
        assert!(
            app.library_projects
                .row_data(0)
                .is_some_and(|row| row.selected),
            "the table paints the selected row"
        );

        app.library_project_action(&ui, "c1", "duplicate");
        assert_eq!(row_ids(&app).len(), 3, "the duplicate joins the table");
        let copy = app
            .library
            .index
            .projects
            .iter()
            .find(|row| row.title == "Copy of Sketch")
            .ok_or("the duplicate carries its copy title")?;
        assert_ne!(copy.id, "c1", "the copy has its own id");
        assert_eq!(copy.group_id.as_deref(), Some("g1"));
        assert!(app.store.load_project(&copy.id).is_ok(), "its file exists");
        Ok(())
    }

    #[test]
    fn a_fresh_app_shows_the_first_project_without_a_kind_suffix() -> TestResult {
        crate::config_test_support::init_backend()?;
        let dir = tempfile::TempDir::new()?;
        let mut index = open_canvas_core::document::LibraryIndex::new();
        index.groups = vec![CanvasGroup {
            id: "g1".into(),
            name: "Project A".into(),
            created_at: String::new(),
        }];
        index.projects = vec![
            entry("c1", "g1", PROJECT_KIND_CANVAS),
            entry("i1", "g1", crate::image::PROJECT_KIND_IMAGE),
        ];
        let store = store::Store::at(dir.path().to_path_buf());
        store.save_index(&index)?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        crate::app::with_app(&app, |app| {
            assert_eq!(
                app.library_projects.iter().count(),
                1,
                "canvas projects only"
            );
            assert_eq!(ui.get_library_group_title().as_str(), "Project A");
        });
        Ok(())
    }

    /// A wired window + app whose persisted index holds one entry of each kind
    /// in a single project.
    type WiredFixture = (tempfile::TempDir, AppWindow, Rc<RefCell<App>>);

    fn wired_fixture() -> Result<WiredFixture, Box<dyn Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = store::Store::at(dir.path().to_path_buf());
        let mut index = open_canvas_core::document::LibraryIndex::new();
        index.groups = vec![CanvasGroup {
            id: "g1".into(),
            name: "Project A".into(),
            created_at: String::new(),
        }];
        index.projects = vec![
            entry("c1", "g1", PROJECT_KIND_CANVAS),
            entry("i1", "g1", crate::image::PROJECT_KIND_IMAGE),
        ];
        store.save_index(&index)?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        Ok((dir, ui, app))
    }

    /// The home screen never leaks an image project into the table, even though
    /// the project exists in storage/index for its Smart Canvas node.
    #[test]
    fn an_image_project_never_appears_in_the_home_table() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, _ui, app) = wired_fixture()?;
        crate::app::with_app(&app, |app| {
            assert_eq!(row_ids(app), vec!["c1".to_string()]);
            assert!(!row_ids(app).contains(&"i1".to_string()), "no image row");
        });
        Ok(())
    }

    /// Opening a canvas from the table lands in the editor and marks the row.
    #[test]
    fn opening_a_row_enters_the_editor_and_selects_it() -> TestResult {
        crate::config_test_support::init_backend()?;
        let dir = tempfile::TempDir::new()?;
        let mut app = App::new(store::Store::at(dir.path().to_path_buf()));
        let ui = AppWindow::new()?;
        let project = CanvasProject::new(
            "c1".to_string(),
            "Sketch".to_string(),
            String::new(),
            String::new(),
        );
        app.store.save_project(&project)?;
        app.library.index.groups = vec![CanvasGroup {
            id: "g1".into(),
            name: "Project A".into(),
            created_at: String::new(),
        }];
        app.library.index.upsert_project(&project);
        app.library.selected_group = Some("g1".into());
        app.library_open_project(&ui, "c1");
        assert_eq!(app.view, View::Editor);
        assert_eq!(app.library.selected_project.as_deref(), Some("c1"));
        Ok(())
    }

    /// Persists a canvas file and points its index row at a known edit stamp.
    fn save_canvas(app: &App, id: &str, title: &str, project: &str, updated: &str) -> TestResult {
        let mut canvas = CanvasProject::new(
            id.to_string(),
            title.to_string(),
            "2026-01-01T08:00:00Z".to_string(),
            updated.to_string(),
        );
        canvas.group_id = Some(project.to_string());
        app.store.save_project(&canvas)?;
        Ok(())
    }

    fn set_stamp(app: &mut App, id: &str, updated: &str) {
        if let Some(row) = app
            .library
            .index
            .projects
            .iter_mut()
            .find(|row| row.id == id)
        {
            row.updated_at = updated.to_string();
        }
    }

    fn canvas_ids(app: &App, project: &str) -> Vec<String> {
        app.library
            .index
            .projects
            .iter()
            .filter(|row| row.group_id.as_deref() == Some(project) && is_listed_row(row))
            .map(|row| row.id.clone())
            .collect()
    }

    /// New Project (level 1) mints one default canvas inside it ("Canvas 1"),
    /// selects both and stays on the home screen.
    #[test]
    fn creating_a_project_gives_it_one_default_canvas() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        assert_eq!(app.library.index.groups.len(), 1, "one project to start");

        app.library_create_group(&ui);

        assert_eq!(
            app.library.index.groups.len(),
            2,
            "the project joined the sidebar"
        );
        let project = app.library.index.groups.last().ok_or("the new project")?;
        assert_eq!(project.name, "Project 2");
        assert_eq!(
            app.library.selected_group.as_deref(),
            Some(project.id.as_str()),
            "the new project is selected"
        );
        let canvases = canvas_ids(&app, project.id.as_str());
        assert_eq!(canvases.len(), 1, "the project starts with one canvas");
        let canvas = app
            .library
            .index
            .project(&canvases[0])
            .ok_or("the default canvas row")?;
        assert_eq!(canvas.title, "Canvas 1");
        assert_eq!(
            app.library.selected_project.as_deref(),
            Some(canvas.id.as_str()),
            "the default canvas row is selected"
        );
        assert!(
            app.store.load_project(&canvas.id).is_ok(),
            "the default canvas is a real persisted canvas"
        );
        assert_eq!(
            app.view,
            View::Library,
            "creating a project stays on the home screen"
        );
        assert!(
            app.library_groups
                .iter()
                .any(|row| row.id.as_str() == project.id.as_str() && row.active),
            "the sidebar paints the new project as the selected one"
        );
        assert_eq!(ui.get_library_group_title().as_str(), "Project 2");
        Ok(())
    }

    /// Opening a project (its double-click action) opens its most recently edited canvas, not the
    /// first row: only `open` takes that path, plain selection does not.
    #[test]
    fn opening_a_project_opens_its_most_recent_canvas() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        save_canvas(&app, "c1", "Older", "g1", "2026-01-01T09:00:00Z")?;
        save_canvas(&app, "c2", "Newer", "g1", "2026-02-01T09:00:00Z")?;
        set_stamp(&mut app, "c1", "2026-01-01T09:00:00Z");
        set_stamp(&mut app, "c2", "2026-02-01T09:00:00Z");
        // The image project is the newest of all and must still be skipped.
        set_stamp(&mut app, "i1", "2026-03-01T09:00:00Z");

        app.library_group_action(&ui, "g1", "open");

        assert_eq!(app.view, View::Editor, "the open action opened a canvas");
        assert_eq!(
            app.editor.as_ref().map(|meta| meta.id.as_str()),
            Some("c2"),
            "the most recently edited canvas is the one that opens"
        );
        assert_eq!(app.library.selected_project.as_deref(), Some("c2"));
        Ok(())
    }

    /// Clicking a canvas row opens that exact canvas, even when another
    /// row in the project is newer.
    #[test]
    fn clicking_a_canvas_opens_that_canvas() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        save_canvas(&app, "c1", "Older", "g1", "2026-01-01T09:00:00Z")?;
        save_canvas(&app, "c2", "Newer", "g1", "2026-02-01T09:00:00Z")?;
        set_stamp(&mut app, "c1", "2026-01-01T09:00:00Z");
        set_stamp(&mut app, "c2", "2026-02-01T09:00:00Z");

        app.library_project_action(&ui, "c1", "open");

        assert_eq!(app.view, View::Editor);
        assert_eq!(
            app.editor.as_ref().map(|meta| meta.id.as_str()),
            Some("c1"),
            "the clicked row wins over the newest row"
        );
        Ok(())
    }

    /// New Canvas lives in the right pane and adds a canvas to the selected
    /// project: the title counts that project's canvases, not the whole library.
    #[test]
    fn a_new_canvas_joins_the_selected_project() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        assert_eq!(canvas_ids(&app, "g1").len(), 2);

        app.library_create_project(&ui);

        let ids = canvas_ids(&app, "g1");
        assert_eq!(ids.len(), 3, "the canvas joined the selected project");
        let created = app
            .library
            .index
            .projects
            .iter()
            .find(|row| row.title == "Canvas 3")
            .ok_or("the new canvas row")?;
        assert_eq!(
            created.group_id.as_deref(),
            Some("g1"),
            "numbered and placed inside the selected project"
        );
        assert_eq!(created.kind, PROJECT_KIND_CANVAS);
        assert_eq!(app.view, View::Editor, "New Canvas still opens the editor");
        assert_eq!(
            app.editor
                .as_ref()
                .and_then(|meta| meta.group_id.as_deref()),
            Some("g1")
        );
        Ok(())
    }

    /// The rows' enter fade is marked only when the visible canvas set changes:
    /// the first paint stays calm, a project switch or a canvas add marks the
    /// rows, and a rename (same ids) does not.
    #[test]
    fn the_table_fades_only_on_a_visible_canvas_change() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        assert!(
            app.library_projects.iter().all(|row| !row.fade),
            "the first paint is calm"
        );

        app.library_select_group(&ui, "g1");
        assert!(
            app.library_projects.iter().all(|row| !row.fade),
            "re-selecting does not replay"
        );

        app.library.index.groups.push(CanvasGroup {
            id: "g2".into(),
            name: "Archive".into(),
            created_at: String::new(),
        });
        app.library
            .index
            .projects
            .push(entry("c9", "g2", PROJECT_KIND_CANVAS));
        app.library_select_group(&ui, "g2");
        assert!(
            app.library_projects.iter().all(|row| row.fade),
            "switching to another project replays the enter fade"
        );

        if let Some(row) = app
            .library
            .index
            .projects
            .iter_mut()
            .find(|row| row.id == "c9")
        {
            row.title = "Renamed".into();
        }
        app.sync_library(&ui);
        assert!(
            app.library_projects.iter().all(|row| !row.fade),
            "a rename keeps the ids, so the fade does not restart"
        );

        app.library
            .index
            .projects
            .push(entry("c10", "g2", PROJECT_KIND_CANVAS));
        app.sync_library(&ui);
        assert!(
            app.library_projects.iter().all(|row| row.fade),
            "a new canvas row replays the fade"
        );
        Ok(())
    }

    /// Project rename (sidebar) and canvas rename (table) share the same
    /// inline-edit path and land in the persisted index.
    #[test]
    fn renaming_a_project_and_a_canvas_persists() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;

        app.library_group_action(&ui, "g1", "rename");
        assert_eq!(
            app.library
                .editing_group
                .as_ref()
                .map(|edit| edit.id.as_str()),
            Some("g1")
        );
        app.library_group_name_edited(&ui, "Sketchbook");
        app.library_group_action(&ui, "g1", "rename-save");
        assert_eq!(
            app.library
                .index
                .group("g1")
                .map(|group| group.name.as_str()),
            Some("Sketchbook")
        );
        assert_eq!(ui.get_library_group_title().as_str(), "Sketchbook");
        assert_eq!(
            app.store
                .load_index()?
                .group("g1")
                .map(|group| group.name.as_str()),
            Some("Sketchbook"),
            "the rename is persisted"
        );

        let canvas = CanvasProject::new(
            "c1".to_string(),
            "Sketches".to_string(),
            String::new(),
            String::new(),
        );
        app.store.save_project(&canvas)?;
        app.library_project_action(&ui, "c1", "rename");
        app.library_project_name_edited(&ui, "Sketches");
        app.library_project_action(&ui, "c1", "rename-save");
        assert_eq!(
            app.library
                .index
                .project("c1")
                .map(|row| row.title.as_str()),
            Some("Sketches")
        );
        assert_eq!(
            app.store.load_project("c1")?.title,
            "Sketches",
            "the canvas file follows the rename"
        );
        Ok(())
    }

    /// Deleting a project deletes each of its canvases (files and index rows),
    /// including the hidden image projects, through the two-click confirm.
    #[test]
    fn deleting_a_project_deletes_its_canvases() -> TestResult {
        crate::config_test_support::init_backend()?;
        let (_dir, ui, mut app) = fixture()?;
        save_canvas(&app, "c1", "Sketch", "g1", "2026-01-01T09:00:00Z")?;
        app.library_group_action(&ui, "g1", "delete");
        assert_eq!(app.library.armed_group.as_deref(), Some("g1"));
        assert_eq!(app.library.index.groups.len(), 1, "armed only, not deleted");

        app.library_group_action(&ui, "g1", "delete");

        assert!(app.library.index.groups.is_empty(), "the project is gone");
        assert!(
            app.library
                .index
                .projects
                .iter()
                .all(|row| row.group_id.as_deref() != Some("g1")),
            "its canvases left the index"
        );
        assert!(
            app.store.load_project("c1").is_err(),
            "its canvas file is gone"
        );
        assert!(
            !ui.get_library_has_group(),
            "the home header knows there is no project now"
        );
        assert_eq!(ui.get_library_group_title().as_str(), "No project");
        Ok(())
    }
}
