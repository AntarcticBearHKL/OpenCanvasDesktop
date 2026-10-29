//! Channels, paths and the action-recorder panels — the Rust half of
//! `ps-channels-panel.tsx`, `ps-paths-panel.tsx` and `ps-actions-panel.tsx`
//! (including `usePsActionRecorder` and its command whitelists).
//! Owns channel/path/action-recorder state and its row models.

use open_canvas_ps::{
    selection_all, selection_polygon, CanvasPsAlphaChannel, CanvasPsParamValue, ChannelVisibility,
    PsParams, SelectionMode, Surface,
};

use crate::app::App;
use crate::image::actions::filter_from_id;
use crate::image::assets::PsActionStep;
use crate::vm;
use crate::AppWindow;

/// `PS_MENU_COMMANDS` (ps-actions-panel.tsx:10) as the app's menu ids.
pub const PS_MENU_COMMANDS: [&str; 9] = [
    "select.all",
    "select.deselect",
    "select.inverse",
    "select.feather",
    "image.crop",
    "image.trim",
    "image.rotate-cw",
    "image.rotate-ccw",
    "image.rotate-180",
];

/// `PS_LAYER_COMMANDS` (ps-actions-panel.tsx:11) as the app's layer actions.
pub const PS_LAYER_COMMANDS: [&str; 7] = [
    "duplicate",
    "delete",
    "up",
    "down",
    "group",
    "ungroup",
    "rasterize",
];

/// True when `id` is a recordable document command (menu + filters).
pub fn is_recordable_command(id: &str) -> bool {
    PS_MENU_COMMANDS.contains(&id) || id.starts_with("filter.")
}

/// True when `action` is a recordable layer command. Panel toggles (visibility,
/// lock, rename, fx) are view/state edits, not document commands, and strokes
/// are intentionally not recordable (ps-actions-panel.tsx:18-21).
pub fn is_recordable_layer_action(action: &str) -> bool {
    PS_LAYER_COMMANDS.contains(&action)
}

/// `key=value;key2=value2` — the recorder's compact parameter encoding.
pub fn param_string(params: &PsParams) -> String {
    let mut parts: Vec<String> = params
        .iter()
        .map(|(key, value)| match value {
            CanvasPsParamValue::Number(number) => format!("{key}={number}"),
            CanvasPsParamValue::Text(text) => format!("{key}={text}"),
            CanvasPsParamValue::Numbers(numbers) => {
                let list: Vec<String> = numbers.iter().map(ToString::to_string).collect();
                format!("{key}=[{}]", list.join(","))
            }
            CanvasPsParamValue::Texts(texts) => format!("{key}=[{}]", texts.join(",")),
        })
        .collect();
    parts.sort();
    parts.join(";")
}

/// The inverse of [`param_string`] (replay decodes the recorded values).
pub fn parse_params(text: &str) -> PsParams {
    let mut params = PsParams::new();
    for entry in text.split(';').filter(|entry| !entry.is_empty()) {
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        if let Some(list) = value
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let items: Vec<&str> = list.split(',').filter(|item| !item.is_empty()).collect();
            if items.iter().all(|item| item.parse::<f64>().is_ok()) {
                params.insert(
                    key.to_string(),
                    CanvasPsParamValue::Numbers(
                        items.iter().filter_map(|item| item.parse().ok()).collect(),
                    ),
                );
            } else {
                params.insert(
                    key.to_string(),
                    CanvasPsParamValue::Texts(items.iter().map(ToString::to_string).collect()),
                );
            }
            continue;
        }
        match value.parse::<f64>() {
            Ok(number) => params.insert(key.to_string(), CanvasPsParamValue::Number(number)),
            Err(_) => params.insert(key.to_string(), CanvasPsParamValue::Text(value.to_string())),
        };
    }
    params
}

/// The channel visibility flags, defaulting to "all shown" (`DEFAULT_VISIBILITY`).
pub fn channel_visibility(project: &open_canvas_ps::ImageProject) -> ChannelVisibility {
    project
        .board_channel_visibility
        .unwrap_or(ChannelVisibility {
            r: true,
            g: true,
            b: true,
        })
}

impl App {
    /// Channels panel actions (`PsChannelsPanel`).
    pub fn image_channel_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_channel_action_core(action, id);
        self.sync_image(ui);
    }

    /// The channel dispatch without a window (unit tests drive this directly).
    pub fn image_channel_action_core(&mut self, action: &str, id: &str) {
        match action {
            "save" => self.channel_save(),
            "load" => self.channel_load(id),
            "delete" => self.channel_delete(id),
            "rename" => {
                if let Some(channel) = self.find_channel(id) {
                    self.image.channel_edit = Some((id.to_string(), channel.name.clone()));
                }
            }
            "rename-save" => {
                let Some((id, name)) = self.image.channel_edit.take() else {
                    return;
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let channels = self.channels_mut();
                if let Some(channel) = channels.iter_mut().find(|channel| channel.id == id) {
                    channel.name = name;
                }
                self.commit_image_core("Rename Channel");
            }
            "rename-cancel" => {
                self.image.channel_edit = None;
            }
            "visible" => {
                let mut visibility = channel_visibility(&self.image.project);
                match id {
                    "r" => visibility.r = !visibility.r,
                    "g" => visibility.g = !visibility.g,
                    "b" => visibility.b = !visibility.b,
                    _ => return,
                }
                if !visibility.r && !visibility.g && !visibility.b {
                    return;
                }
                self.image.project.board_channel_visibility = Some(visibility);
                self.commit_image_core("Channel Visibility");
            }
            "view" => {
                let view = if id.is_empty() { "rgb" } else { id };
                self.image.channel_view = view.to_string();
            }
            _ => {}
        }
    }

    fn find_channel(&self, id: &str) -> Option<&CanvasPsAlphaChannel> {
        self.image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.iter().find(|channel| channel.id == id))
    }

    fn channels_mut(&mut self) -> &mut Vec<CanvasPsAlphaChannel> {
        self.image
            .project
            .board_alpha_channels
            .get_or_insert_with(Vec::new)
    }

    /// `PsChannelsPanel.saveSelection`: stores the current selection mask as a
    /// new alpha channel bitmap.
    fn channel_save(&mut self) {
        let Some(selection) = self.image.selection.clone() else {
            return;
        };
        let id = self.mint_image_id("ps-channel");
        let key = format!("image:channel-{id}");
        let count = self
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .map_or(0, Vec::len);
        let channel = CanvasPsAlphaChannel {
            id,
            name: format!("Alpha {}", count + 1),
            storage_key: key.clone(),
        };
        self.image.bitmaps.insert(key, selection);
        self.channels_mut().push(channel);
        self.commit_image_core("Save Channel");
    }

    /// `PsChannelsPanel.loadChannel`: restores a channel mask into the selection.
    fn channel_load(&mut self, id: &str) {
        let Some(key) = self
            .find_channel(id)
            .map(|channel| channel.storage_key.clone())
        else {
            return;
        };
        let Some(mut mask) = self.image.bitmaps.get(&key).cloned() else {
            return;
        };
        if let Some((width, height)) = self.image.dims() {
            if mask.width() != width || mask.height() != height {
                mask = mask.resample_bilinear(width, height);
            }
        }
        self.image.selection = Some(mask);
    }

    fn channel_delete(&mut self, id: &str) {
        let key = self
            .find_channel(id)
            .map(|channel| channel.storage_key.clone());
        if let Some(channels) = self.image.project.board_alpha_channels.as_mut() {
            channels.retain(|channel| channel.id != id);
        }
        if let Some(key) = key {
            self.image.bitmaps.remove(&key);
        }
        if self.image.channel_view == id {
            self.image.channel_view = "rgb".to_string();
        }
        self.commit_image_core("Delete Channel");
    }

    /// Paths panel actions (`PsPathsPanel`).
    pub fn image_path_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_path_action_core(action, id);
        self.sync_image(ui);
    }

    /// The path dispatch without a window (unit tests drive this directly).
    pub fn image_path_action_core(&mut self, action: &str, id: &str) {
        let target = if id.is_empty() {
            self.image.active_path.clone()
        } else {
            id.to_string()
        };
        match action {
            "new" => {
                let path_id = self.mint_image_path_id();
                let count = self.paths().len() + 1;
                let path = open_canvas_ps::create_path(
                    &path_id,
                    &format!("Path {count}"),
                    Vec::new(),
                    false,
                );
                self.paths_mut().push(path);
                self.image.active_path = path_id;
                self.commit_image_core("New Path");
            }
            "duplicate" => {
                let Some(original) = self.find_path(&target).cloned() else {
                    return;
                };
                let id = self.mint_image_path_id();
                let copy = open_canvas_ps::create_path(
                    &id,
                    &format!("{} copy", original.name),
                    original.anchors,
                    original.closed,
                );
                self.paths_mut().push(copy);
                self.image.active_path = id;
                self.commit_image_core("Duplicate Path");
            }
            "delete" => {
                if let Some(paths) = self.image.project.board_paths.as_mut() {
                    paths.retain(|path| path.id != target);
                }
                self.commit_image_core("Delete Path");
            }
            "visible" => {
                let visible = self.find_path(&target).map(|path| !path.visible);
                let Some(visible) = visible else {
                    return;
                };
                if let Some(paths) = self.image.project.board_paths.as_mut() {
                    if let Some(path) = paths.iter_mut().find(|path| path.id == target) {
                        path.visible = visible;
                    }
                }
                self.commit_image_core("Toggle Path Visibility");
            }
            "rename" => {
                if let Some(path) = self.find_path(&target) {
                    self.image.path_edit = Some((target.clone(), path.name.clone()));
                }
            }
            "rename-save" => {
                let Some((id, name)) = self.image.path_edit.take() else {
                    return;
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Some(paths) = self.image.project.board_paths.as_mut() {
                    if let Some(path) = paths.iter_mut().find(|path| path.id == id) {
                        path.name = name;
                    }
                }
                self.commit_image_core("Rename Path");
            }
            "rename-cancel" => {
                self.image.path_edit = None;
            }
            "active" => {
                if self.find_path(&target).is_some() {
                    self.image.active_path = target;
                }
            }
            "selection" => self.path_selection(&target),
            "fill" => self.path_paint(&target, 0.0, "Fill Path"),
            "stroke" => {
                let width = self.image.shape_stroke_width.max(1.0);
                self.path_paint(&target, width, "Stroke Path");
            }
            _ => {}
        }
    }

    fn paths(&self) -> &Vec<open_canvas_ps::CanvasPsPath> {
        self.image
            .project
            .board_paths
            .as_ref()
            .unwrap_or(&EMPTY_PATHS)
    }

    fn paths_mut(&mut self) -> &mut Vec<open_canvas_ps::CanvasPsPath> {
        self.image.project.board_paths.get_or_insert_with(Vec::new)
    }

    fn find_path(&self, id: &str) -> Option<&open_canvas_ps::CanvasPsPath> {
        self.paths().iter().find(|path| path.id == id)
    }

    /// `PsPathsPanel.onSelection`: the path polygon becomes the selection.
    fn path_selection(&mut self, path_id: &str) {
        let Some(path) = self.find_path(path_id).cloned() else {
            return;
        };
        if path.anchors.len() < 2 {
            return;
        }
        let points = open_canvas_ps::polyline(&path, 24);
        let feather = self.image.feather;
        let antialias = self.image.antialias;
        self.with_selection(|selection| {
            selection_polygon(
                selection,
                &points,
                feather,
                antialias,
                SelectionMode::Replace,
            );
        });
    }

    /// `PsPathsPanel.onFill` / `onStroke`: paint the path onto the active bitmap.
    fn path_paint(&mut self, path_id: &str, stroke_width: f64, label: &str) {
        let Some(path) = self.find_path(path_id).cloned() else {
            return;
        };
        if path.anchors.len() < 2 {
            return;
        }
        let active = self.image.active_layer_id();
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &active)
                .cloned()
        else {
            return;
        };
        let Some(key) = self.image.ensure_layer_bitmap(&active) else {
            return;
        };
        let color = self.image.color.clone();
        let selection = self.image.selection.clone();
        self.image.begin_edit(std::slice::from_ref(&key));
        let base = self.image.bitmaps.get(&key);
        let painted = open_canvas_ps::paint_path(
            &layer,
            &path,
            &color,
            stroke_width,
            base,
            selection.as_ref(),
        );
        self.image.bitmaps.insert(key, painted);
        self.commit_image_core(label);
    }

    /// One inline rename keystroke: writes the draft of whichever panel row is
    /// being edited (channels, paths, actions or the gradient name).
    pub fn image_panel_name_edited(&mut self, ui: &AppWindow, text: &str) {
        self.image_panel_name_edited_core(text);
        self.sync_image(ui);
    }

    /// The inline rename keystroke without a window (unit tests drive this
    /// directly); the gradient-name field has no per-row draft, so its target
    /// is the gradient edit entry itself.
    pub fn image_panel_name_edited_core(&mut self, text: &str) {
        if let Some((_, draft)) = self.image.channel_edit.as_mut() {
            *draft = text.to_string();
            return;
        }
        if let Some((_, draft)) = self.image.path_edit.as_mut() {
            *draft = text.to_string();
            return;
        }
        if let Some((_, draft)) = self.image.action_edit.as_mut() {
            *draft = text.to_string();
            return;
        }
        self.image.gradient_edit.name = text.to_string();
    }

    /// Actions panel (`PsActionsPanel` + `usePsActionRecorder`).
    pub fn image_actions_panel(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_actions_panel_core(action, id);
        self.sync_image(ui);
    }

    /// The actions dispatch without a window (unit tests drive this directly).
    pub fn image_actions_panel_core(&mut self, action: &str, id: &str) {
        match action {
            "record" => {
                self.image.recorder.recording = true;
                self.image.recorder.steps.clear();
            }
            "stop" => self.image.recorder.recording = false,
            "save" => {
                let name = if id.trim().is_empty() {
                    format!("Action {}", self.image.assets.actions.len() + 1)
                } else {
                    id.trim().to_string()
                };
                let steps = std::mem::take(&mut self.image.recorder.steps);
                if steps.is_empty() {
                    return;
                }
                let action_id = self.mint_image_id("ps-action");
                self.image.assets.actions.insert(
                    0,
                    crate::image::assets::PsAction {
                        id: action_id,
                        name,
                        steps,
                        created_at: crate::store::now_iso(),
                    },
                );
                self.image.recorder.recording = false;
                self.note_doc_change();
            }
            "play" => self.image_action_play_core(id),
            "rename" => {
                if let Some(action) = self.action_by_id(id) {
                    self.image.action_edit = Some((id.to_string(), action.name.clone()));
                }
            }
            "rename-save" => {
                let Some((id, name)) = self.image.action_edit.take() else {
                    return;
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Some(action) = self
                    .image
                    .assets
                    .actions
                    .iter_mut()
                    .find(|action| action.id == id)
                {
                    action.name = name;
                }
                self.note_doc_change();
            }
            "rename-cancel" => {
                self.image.action_edit = None;
            }
            "delete" => {
                self.image.assets.actions.retain(|action| action.id != id);
                self.note_doc_change();
            }
            _ => {}
        }
    }

    fn action_by_id(&self, id: &str) -> Option<&crate::image::assets::PsAction> {
        self.image
            .assets
            .actions
            .iter()
            .find(|action| action.id == id)
    }

    /// Replays a saved action step by step (`usePsActionRecorder.play`).
    pub fn image_action_play_core(&mut self, action_id: &str) {
        let Some(action) = self.action_by_id(action_id).cloned() else {
            return;
        };
        self.image.recorder.replaying = true;
        for step in &action.steps {
            self.image_replay_step(step);
        }
        self.image.recorder.replaying = false;
    }

    /// One recorded step: layer commands, filter applications and menu commands.
    fn image_replay_step(&mut self, step: &PsActionStep) {
        match step.kind.as_str() {
            "layer" => self.image_layer_action_core(&step.command, &step.arg),
            "filter" => {
                if let Some(choice) = filter_from_id(&step.command) {
                    let params = crate::image::filter::FilterParams::parse_for(choice, &step.arg);
                    drop(self.image_filter_apply(choice, &params));
                }
            }
            _ => self.image_command_core(&step.command),
        }
    }

    /// One recordable menu command (`PS_MENU_COMMANDS` + `filter.<id>`).
    pub fn image_command_core(&mut self, id: &str) {
        if !id.starts_with("filter.") && is_recordable_command(id) {
            self.image.recorder.record("menu", id, "");
        }
        match id {
            "select.all" => self.with_selection(selection_all),
            "select.deselect" => {
                self.image.selection = None;
                self.image.poly_points.clear();
            }
            "select.inverse" => self.image_invert_core(),
            "select.feather" => self.image_feather_core(),
            "image.crop" => self.image_crop_core(),
            "image.trim" => self.image_trim_core(),
            "image.rotate-cw" => self.image_rotate_core(90.0),
            "image.rotate-ccw" => self.image_rotate_core(-90.0),
            "image.rotate-180" => self.image_rotate_core(180.0),
            "edit.clear-transform" => self.image_clear_transform_core(),
            _ => {
                if let Some(rest) = id.strip_prefix("filter.") {
                    self.image_filter_open_core(rest).ok();
                }
            }
        }
    }
}

/// The path list when a board never had paths (avoids an `Option` unwrap).
static EMPTY_PATHS: Vec<open_canvas_ps::CanvasPsPath> = Vec::new();

/// Applies the active channel view to a board composite (`PsChannelPreview`):
/// "red"/"green"/"blue" isolate one colour channel, an alpha-channel id shows
/// that channel's mask as a grey image.
pub fn apply_channel_view(
    composite: &mut Surface,
    view: &str,
    bitmaps: &std::collections::HashMap<String, Surface>,
    channels: Option<&Vec<CanvasPsAlphaChannel>>,
) {
    match view {
        "" | "rgb" => {}
        "red" => isolate_channel(composite, 0),
        "green" => isolate_channel(composite, 1),
        "blue" => isolate_channel(composite, 2),
        id => {
            let key = channels
                .and_then(|channels| channels.iter().find(|channel| channel.id == id))
                .map(|channel| channel.storage_key.as_str());
            let Some(mask) = key.and_then(|key| bitmaps.get(key)) else {
                return;
            };
            let sized = if mask.width() == composite.width() && mask.height() == composite.height()
            {
                mask.clone()
            } else {
                mask.resample_bilinear(composite.width(), composite.height())
            };
            for y in 0..composite.height() {
                for x in 0..composite.width() {
                    let alpha = sized.get(vm::u2i(x), vm::u2i(y))[3];
                    composite.set(vm::u2i(x), vm::u2i(y), [alpha, alpha, alpha, 255]);
                }
            }
        }
    }
}

/// Zeroes every channel but `keep` (the web's `PsChannelPreview` isolation).
fn isolate_channel(composite: &mut Surface, keep: usize) {
    for pixel in composite.data_mut().as_chunks_mut::<4>().0 {
        for (index, channel) in pixel.iter_mut().enumerate().take(3) {
            if index != keep {
                *channel = 0;
            }
        }
        pixel[3] = 255;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{Bounds, IdGen, ImageProject};

    fn board() -> (ImageProject, HashMap<String, Surface>) {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                surface.set(x, y, [200, 100, 50, 255]);
            }
        }
        bitmaps.insert("image:t-1".to_string(), surface);
        (project, bitmaps)
    }

    fn channels_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let (project, bitmaps) = board();
        crate::test_support::app_with_image(project, bitmaps)
    }

    fn select_rect(app: &mut App, x: f64, y: f64, width: f64, height: f64) {
        app.with_selection(|selection| {
            open_canvas_ps::selection_rect(
                selection,
                Bounds::new(x, y, width, height),
                false,
                0.0,
                false,
                SelectionMode::Replace,
            );
        });
    }

    #[test]
    fn channel_save_load_delete_and_rename_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        select_rect(&mut app, 2.0, 2.0, 4.0, 4.0);
        app.image_channel_action_core("save", "");
        let channel = app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.first())
            .cloned()
            .ok_or("channel")?;
        assert_eq!(channel.name, "Alpha 1");
        assert!(app.image.bitmaps.contains_key(&channel.storage_key));
        assert_eq!(app.image.history.label(1), Some("Save Channel"));

        // The mask restores the selection exactly.
        app.image.selection = None;
        app.image_channel_action_core("load", &channel.id.clone());
        let mask = app.image.selection.as_ref().ok_or("selection")?;
        assert_eq!(mask.get(3, 3)[3], 255);
        assert_eq!(mask.get(10, 10)[3], 0);

        app.image_channel_action_core("rename", &channel.id.clone());
        app.image.channel_edit = Some((channel.id.clone(), "Spot".to_string()));
        app.image_channel_action_core("rename-save", "");
        let renamed = app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.first())
            .map(|channel| channel.name.clone());
        assert_eq!(renamed.as_deref(), Some("Spot"));

        app.image_channel_action_core("delete", &channel.id.clone());
        assert!(app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .is_none_or(Vec::is_empty));
        assert!(!app.image.bitmaps.contains_key(&channel.storage_key));
        Ok(())
    }

    #[test]
    fn escape_cancels_an_open_inline_rename() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        select_rect(&mut app, 2.0, 2.0, 4.0, 4.0);
        app.image_channel_action_core("save", "");
        let channel = app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.first())
            .cloned()
            .ok_or("channel")?;

        app.image_channel_action_core("rename", &channel.id);
        assert!(app.image.channel_edit.is_some());
        assert!(
            app.image_cancel_inline_rename_core(),
            "escape cancels the open rename"
        );
        assert!(app.image.channel_edit.is_none());
        assert!(
            !app.image_cancel_inline_rename_core(),
            "nothing left to cancel"
        );
        let name = app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.first())
            .map(|channel| channel.name.clone());
        assert_eq!(name.as_deref(), Some("Alpha 1"), "the draft is discarded");
        Ok(())
    }

    #[test]
    fn channel_visibility_toggles_change_the_composite() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        app.image.project.board_ratio = "1:1".to_string();
        let composite = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?;
        assert!(
            composite.get(2, 2)[1] > 0,
            "green channel visible by default"
        );

        app.image_channel_action_core("visible", "g");
        let hidden = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?;
        assert_eq!(hidden.get(2, 2)[1], 0, "green channel hidden");
        assert!(hidden.get(2, 2)[0] > 0, "red stays visible");

        // The last visible channel never turns off.
        app.image_channel_action_core("visible", "r");
        app.image_channel_action_core("visible", "b");
        app.image_channel_action_core("visible", "b");
        let hidden = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("composite")?;
        assert!(hidden.get(2, 2)[2] > 0, "blue cannot be the last one off");
        Ok(())
    }

    #[test]
    fn channel_view_isolates_colour_and_alpha_channels() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        app.image_channel_action_core("view", "red");
        let red = crate::image::render::display_of(&app.image).ok_or("display")?;
        let pixel = red.get(4, 4);
        assert_eq!((pixel[1], pixel[2]), (0, 0), "red view zeroes g/b");
        assert_eq!(pixel[0], 200);

        app.image_channel_action_core("view", "rgb");
        let rgb = crate::image::render::display_of(&app.image).ok_or("display")?;
        assert!(rgb.get(4, 4)[1] > 0);

        select_rect(&mut app, 2.0, 2.0, 4.0, 4.0);
        app.image_channel_action_core("save", "");
        let channel = app
            .image
            .project
            .board_alpha_channels
            .as_ref()
            .and_then(|channels| channels.first())
            .cloned()
            .ok_or("channel")?;
        app.image_channel_action_core("view", &channel.id.clone());
        let alpha = crate::image::render::display_of(&app.image).ok_or("display")?;
        assert_eq!(alpha.get(3, 3), [255, 255, 255, 255], "inside the mask");
        assert_eq!(alpha.get(10, 10), [0, 0, 0, 255], "outside the mask");
        Ok(())
    }

    #[test]
    fn path_new_duplicate_delete_and_visibility() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        app.image_path_action_core("new", "");
        let first = app.image.active_path.clone();
        assert_ne!(first, "", "a new path is active");
        assert!(app
            .image
            .project
            .board_paths
            .as_ref()
            .is_some_and(|paths| paths.len() == 1));
        assert_eq!(app.image.history.label(1), Some("New Path"));

        app.image_path_action_core("duplicate", "");
        assert_eq!(
            app.image.project.board_paths.as_ref().map(Vec::len),
            Some(2)
        );
        assert_ne!(app.image.active_path, first);

        app.image_path_action_core("visible", &first);
        let hidden = app
            .image
            .project
            .board_paths
            .as_ref()
            .and_then(|paths| paths.iter().find(|path| path.id == first));
        assert!(hidden.is_some_and(|path| !path.visible));

        app.image_path_action_core("delete", &first);
        assert_eq!(
            app.image.project.board_paths.as_ref().map(Vec::len),
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn path_make_selection_fill_and_stroke_paint_the_layer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        // A triangle around the middle of the 16x16 layer.
        let mut gen = open_canvas_ps::IdGen::new("p");
        let path = open_canvas_ps::create_path(
            &gen.next_id(),
            "Triangle",
            vec![
                open_canvas_ps::path_anchor(4.0, 4.0),
                open_canvas_ps::path_anchor(12.0, 4.0),
                open_canvas_ps::path_anchor(8.0, 12.0),
            ],
            true,
        );
        let path_id = path.id.clone();
        app.image.project.board_paths = Some(vec![path]);
        app.image.active_path = path_id.clone();

        app.image_path_action_core("selection", &path_id);
        let mask = app.image.selection.as_ref().ok_or("selection")?;
        assert_eq!(mask.get(8, 6)[3], 255, "inside the triangle is selected");
        assert_eq!(mask.get(1, 1)[3], 0, "outside is not");

        app.image.color = "#00ff00".to_string();
        app.image_path_action_core("fill", &path_id);
        let bitmap = app.image.bitmaps.get("image:t-1").ok_or("bitmap")?;
        assert_eq!(
            bitmap.get(8, 6),
            [0, 255, 0, 255],
            "fill paints the polygon"
        );
        // `psPaintPath` clips the whole target by the active selection, so the
        // unselected pixels keep their colour but lose alpha.
        assert_eq!(bitmap.get(1, 1)[3], 0, "outside the selection is clipped");
        assert_eq!(app.image.history.label(1), Some("Fill Path"));

        app.image.selection = None;
        app.image.color = "#ff0000".to_string();
        app.image.shape_stroke_width = 2.0;
        app.image_path_action_core("stroke", &path_id);
        let bitmap = app.image.bitmaps.get("image:t-1").ok_or("bitmap")?;
        assert_eq!(
            bitmap.get(8, 4),
            [255, 0, 0, 255],
            "stroke paints the outline"
        );
        assert_eq!(app.image.history.label(2), Some("Stroke Path"));
        Ok(())
    }

    #[test]
    fn recorder_captures_commands_and_replays_them() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        app.image_actions_panel_core("record", "");
        assert!(app.image.recorder.recording);
        app.image_command_core("select.all");
        app.image_layer_action_core("duplicate", "");
        app.image_actions_panel_core("stop", "");
        app.image_actions_panel_core("save", "Demo");
        assert!(!app.image.recorder.recording);
        let action = app.image.assets.actions.first().cloned().ok_or("action")?;
        assert_eq!(action.name, "Demo");
        assert_eq!(action.steps.len(), 2, "menu + layer step");
        assert_eq!(action.steps[0].command, "select.all");
        assert_eq!(action.steps[1].kind, "layer");

        // Undo the duplicate, then replay: the action restores it.
        app.image.selection = None;
        assert!(app.image.undo());
        let layers = app.image.project.board_layers.len();
        app.image_actions_panel_core("play", &action.id.clone());
        assert_eq!(app.image.project.board_layers.len(), layers + 1);
        assert!(app.image.selection.is_some(), "select all replayed");
        assert_eq!(app.image.recorder.steps.len(), 0, "replay never re-records");

        app.image_actions_panel_core("rename", &action.id.clone());
        app.image.action_edit = Some((action.id.clone(), "Renamed".to_string()));
        app.image_actions_panel_core("rename-save", "");
        let name = app
            .image
            .assets
            .actions
            .first()
            .map(|action| action.name.clone());
        assert_eq!(name.as_deref(), Some("Renamed"));
        app.image_actions_panel_core("delete", &action.id.clone());
        assert!(app.image.assets.actions.is_empty(), "action removed");
        Ok(())
    }

    #[test]
    fn recorder_round_trips_filter_parameters() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        let mut params = open_canvas_ps::filters::ps_filter_params(
            open_canvas_ps::filters::PsFilterType::GaussianBlur,
        );
        params.insert("radius".to_string(), CanvasPsParamValue::Number(6.0));
        let encoded = param_string(&params);
        let decoded = parse_params(&encoded);
        assert_eq!(decoded.get("radius"), params.get("radius"));

        app.image_actions_panel_core("record", "");
        open_canvas_ps::filters::apply_ps_filter(
            &mut Surface::new(2, 2),
            open_canvas_ps::filters::PsFilterType::GaussianBlur,
            &params,
        );
        app.image_filter_apply(
            crate::image::filter::FilterChoice::Native(
                open_canvas_ps::filters::PsFilterType::GaussianBlur,
            ),
            &crate::image::filter::FilterParams::native(params),
        )?;
        app.image_actions_panel_core("stop", "");
        let step = app.image.recorder.steps.first().cloned().ok_or("step")?;
        assert_eq!(step.kind, "filter");
        assert_eq!(step.command, "gaussian-blur");
        assert!(step.arg.contains("radius=6"), "{}", step.arg);

        app.image_actions_panel_core("save", "Blur");
        let action = app.image.assets.actions.first().cloned().ok_or("action")?;
        assert!(app.image.undo(), "the filter is undoable");
        let before = app
            .image
            .bitmaps
            .get("image:t-1")
            .map(|bitmap| bitmap.get(8, 8));
        app.image_actions_panel_core("play", &action.id.clone());
        assert_eq!(app.image.history.label(1), Some("Filter"));
        let after = app
            .image
            .bitmaps
            .get("image:t-1")
            .map(|bitmap| bitmap.get(8, 8));
        assert_eq!(before, after, "replay reproduces the filtered bitmap");
        Ok(())
    }

    #[test]
    fn replay_of_an_empty_record_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = channels_board_app()?;
        app.image_actions_panel_core("record", "");
        app.image_actions_panel_core("save", "Nothing");
        assert!(app.image.assets.actions.is_empty(), "no steps, no action");
        Ok(())
    }
}
