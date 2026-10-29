//! Config modal state: the fixed provider/model rows the Slint modal renders and
//! the one editable provider field (the API key). Every structural change is
//! impossible by design: [`open_canvas_ai::config::AiConfig::normalize`] locks the
//! two providers and the three capability models, so this module never adds,
//! removes or re-assigns anything.

use slint::{ModelRc, VecModel};

use open_canvas_ai::config::{AiConfig, ModelCapability};

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::app::{with_app, App};
use crate::i18n::{self, Locale};
use crate::{AppWindow, ConfigChannelRow, ConfigModelRow};

/// Which settings tab is open (`ConfigTabKey`, `use-config-store.ts`). The
/// modal's remaining tabs, in the web's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTab {
    Providers,
    Appearance,
    LocalModels,
    LocalStorage,
    Cost,
    Logs,
    About,
}

impl ConfigTab {
    /// The tab for a rail id. Unknown ids (including the retired `models`,
    /// `generation` and `agent` tabs) fall back to Providers.
    pub fn from_id(id: &str) -> Self {
        match id {
            "appearance" => Self::Appearance,
            "local-models" => Self::LocalModels,
            "local-storage" => Self::LocalStorage,
            "cost" => Self::Cost,
            "logs" => Self::Logs,
            "about" => Self::About,
            _ => Self::Providers,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Providers => "providers",
            Self::Appearance => "appearance",
            Self::LocalModels => "local-models",
            Self::LocalStorage => "local-storage",
            Self::Cost => "cost",
            Self::Logs => "logs",
            Self::About => "about",
        }
    }
}

fn capability_label(capability: ModelCapability) -> &'static str {
    match capability {
        ModelCapability::Text => "Text",
        ModelCapability::Image => "Image",
        ModelCapability::Video => "Video",
    }
}

/// The provider cards: exactly the two fixed presets, in preset order.
pub fn channel_rows(config: &AiConfig, locale: Locale) -> Vec<ConfigChannelRow> {
    config
        .channels
        .iter()
        .map(|channel| ConfigChannelRow {
            id: channel.id.as_str().into(),
            name: channel.name.as_str().into(),
            base_url: channel.base_url.as_str().into(),
            api_key: channel.api_key.as_str().into(),
            model_count: i32::try_from(channel.models.len()).unwrap_or(i32::MAX),
            model_count_label: i18n::model_count_label(locale, channel.models.len()).into(),
            models: ModelRc::new(VecModel::from(
                channel
                    .models
                    .iter()
                    .map(|model| ConfigModelRow {
                        name: model.name.as_str().into(),
                        capability: model.capability.as_str().into(),
                        capability_label: capability_label(model.capability).into(),
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect()
}

impl App {
    /// Pushes the modal open/closed state, its tab, and every tab's rows.
    pub fn sync_config(&mut self, ui: &AppWindow) {
        ui.set_config_open(self.config_open);
        ui.set_config_tab(self.config_tab.as_str().into());
        if self.config_open {
            self.refresh_config_rows(ui);
        }
    }

    fn refresh_config_rows(&mut self, ui: &AppWindow) {
        self.config_channels
            .set_vec(channel_rows(&self.config, self.locale));
        ui.set_config_open(self.config_open);
        self.sync_config_tabs(ui);
    }

    /// Persists the config; a write failure is reported but does not block editing.
    pub(crate) fn persist_config(&mut self, ui: &AppWindow) {
        if let Err(error) = self.store.save_ai_config(&self.config) {
            let message = format!("Could not save settings: {error}");
            self.log(
                open_canvas_ai::runtime_log::RuntimeLogLevel::Warn,
                "settings",
                message.clone(),
                None,
            );
            self.set_notice(ui, message, true);
        }
    }

    pub fn config_open(&mut self, ui: &AppWindow, tab: &str) {
        self.config_tab = ConfigTab::from_id(tab);
        self.config_open = true;
        self.sync_config(ui);
    }

    pub fn config_close(&mut self, ui: &AppWindow) {
        self.config_open = false;
        self.sync_config(ui);
    }

    pub fn config_tab_pick(&mut self, ui: &AppWindow, tab: &str) {
        self.config_tab = ConfigTab::from_id(tab);
        self.sync_config(ui);
    }

    /// The provider card's only editable field. Name and base URL are fixed by
    /// the presets and the model assignment is locked, so anything else is
    /// ignored.
    pub fn config_channel_field(&mut self, ui: &AppWindow, id: &str, field: &str, value: &str) {
        if field != "api-key" {
            return;
        }
        if let Some(channel) = self.config.channels.iter_mut().find(|c| c.id == id) {
            channel.api_key = value.to_string();
            self.persist_config(ui);
        }
    }
}

/// Registers the config-modal callbacks on the window.
pub fn wire_config(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_open_config(move || {
            let Some(ui) = weak.upgrade() else { return };
            with_app(&app, |app| app.config_open(&ui, "providers"));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_close(move || {
            let Some(ui) = weak.upgrade() else { return };
            with_app(&app, |app| app.config_close(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_tab_pick(move |tab| {
            let Some(ui) = weak.upgrade() else { return };
            with_app(&app, |app| app.config_tab_pick(&ui, tab.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_config_channel_field(move |id, field, value| {
            let Some(ui) = weak.upgrade() else { return };
            with_app(&app, |app| {
                app.config_channel_field(&ui, id.as_str(), field.as_str(), value.as_str());
            });
        });
    }
}
