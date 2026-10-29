//! Slint callback wiring for the home screen (kept apart from the state machine
//! in `library.rs` so both stay readable).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc};

use crate::app::{with_app, App};
use crate::AppWindow;

/// Registers the library callbacks and installs the row models.
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the library callbacks; splitting adds indirection, not clarity"
)]
pub fn wire_library(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    with_app(app, |app| {
        ui.set_library_groups(ModelRc::new(app.library_groups.clone()));
        ui.set_library_projects(ModelRc::new(app.library_projects.clone()));
        ui.set_side_rows(ModelRc::new(app.side_rows.clone()));
    });

    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_create_group(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.library_create_group(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_select_group(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.library_select_group(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_group_action(move |id, action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_group_action(&ui, id.as_str(), action.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_group_name_edited(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_group_name_edited(&ui, text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_create_project(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.library_create_project(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_project_action(move |id, action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_project_action(&ui, id.as_str(), action.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_library_project_name_edited(move |text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_project_name_edited(&ui, text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_export_path_edited(move |path| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.export_import_path_edited(&ui, path.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_export_run(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.export_import_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_export_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.export_import_close(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_panel_open_project(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.library_open_project(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_panel_create_project(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.library_create_project(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_panel_import(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.open_import_modal(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_panel_project_action(move |id, action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_project_action(&ui, id.as_str(), action.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_side_panel_reorder(move |id, direction| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.library_side_reorder(&ui, id.as_str(), direction.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_notice_dismiss(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.dismiss_notice(&ui, id));
        });
    }
}
