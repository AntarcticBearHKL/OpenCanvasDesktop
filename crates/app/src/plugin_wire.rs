//! Slint callback registration for the plugin UI. The handlers themselves live
//! in `plugin_actions.rs` (node actions) and `plugin_manager.rs` (create menu +
//! manager modal); this module binds them, installs the initial rows and owns
//! the one push of the plugin scalar state ([`App::sync_plugin`]).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{with_app, App};
use crate::plugin_views;
use crate::AppWindow;

/// Pushes the plugin-manager scalar state; the row models are installed by
/// [`wire_plugins`] and refreshed on every manager mutation.
impl App {
    pub fn sync_plugin(&self, ui: &AppWindow) {
        ui.set_plugins_open(self.plugin.ui.manager_open);
        ui.set_plugin_tab(self.plugin.ui.tab.as_str().into());
        ui.set_plugin_url(self.plugin.ui.url.as_str().into());
        ui.set_plugin_refreshing(self.plugin.ui.refreshing);
        if let Some((text, error)) = &self.plugin.ui.message {
            ui.set_plugin_message(text.as_str().into());
            ui.set_plugin_message_error(*error);
        } else {
            ui.set_plugin_message(SharedString::default());
            ui.set_plugin_message_error(false);
        }
    }
}

/// Registers the plugin callbacks and installs the initial row models.
#[expect(
    clippy::too_many_lines,
    reason = "flat registration of the plugin callbacks; splitting adds indirection, not clarity"
)]
pub fn wire_plugins(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    with_app(app, |app| {
        ui.set_plugin_rows(ModelRc::new(VecModel::from(plugin_views::manager_rows(
            &app.plugin,
            app.plugin.ui.tab,
        ))));
        app.sync_plugin(ui);
    });

    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_create_node(move |type_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let created =
                crate::app::with_app_r(&app, |app| app.create_plugin_node(&ui, type_id.as_str()));
            if let Some(id) = created.filter(|id| !id.is_empty()) {
                crate::shortcuts::start_node_enter(&app, &id);
            }
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_open_plugins(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.open_plugin_manager(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_manager_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.close_plugin_manager(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_tab_pick(move |tab| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_tab_pick(&ui, tab.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_refresh(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_refresh(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_url_edited(move |url| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_url_edited(&ui, url.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_install(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_install(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_install_row(move |url| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_install_row(&ui, url.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_toggle(move |id, enabled| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_toggle(&ui, id.as_str(), enabled));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_uninstall(move |id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| app.plugin_uninstall(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_action(move |id, action| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.plugin_action(&ui, id.as_str(), action.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_text_edited(move |id, text| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.plugin_text_edited(&ui, id.as_str(), text.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_plugin_palette_pick(move |id, index| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            with_app(&app, |app| {
                app.plugin_palette_pick(&ui, id.as_str(), index);
            });
        });
    }
}

#[cfg(test)]
#[path = "plugin_tests/manager_tests.rs"]
mod tests;
