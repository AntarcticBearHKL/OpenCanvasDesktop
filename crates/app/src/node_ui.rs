//! Inline node UI state: the prompt editor session, the generation card's
//! popover/running state and the AI dialog state the canvas shares. Every value
//! is pushed through the node rows (`vm::node_row`); there is no floating panel
//! any more.

use open_canvas_core::media_size::{
    clamp_video_seconds, compute_media_size, compute_video_size, image_quality_label,
    image_size_label, infer_media_ratio, infer_media_scale, infer_video_ratio,
    parse_video_resolution, reasoning_label, video_resolution_label,
};
use open_canvas_core::node::{
    CanvasFontWeight, CanvasNodeData, CanvasNodeType, CanvasReasoningEffort, CanvasTextAlign,
};

use crate::{AppWindow, GenerationSettings};
use slint::ComponentHandle;

/// Text-colour swatches of the style popover (`canvas-text-style-popover.tsx`
/// colour picker, reduced to the semantic palette). Index 0 is the `auto`
/// reset the UI renders separately; the rest are the brand inks.
const TEXT_COLOR_SWATCHES: [&str; 5] = ["#d85c3a", "#15803d", "#96620a", "#0f766e", "#b91c1c"];

/// Resolves a style-popover swatch value to the persisted hex: `auto` clears
/// the colour (`textColor: ""` in the web) and `0..=4` picks an ink.
fn text_color_setting(value: &str) -> Option<String> {
    if value == "auto" {
        return Some(String::new());
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|index| TEXT_COLOR_SWATCHES.get(index).map(|hex| (*hex).to_string()))
}

/// Which settings popover the generation card has open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popover {
    pub node: String,
    pub id: PopoverKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverKind {
    Model,
    Settings,
}

impl PopoverKind {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "model" => Some(Self::Model),
            "settings" => Some(Self::Settings),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Settings => "settings",
        }
    }
}

/// Rust-owned node interaction state: the prompt editor session, the open
/// generation popover, the running node, and the AI dialogs of `image_ai`.
#[derive(Debug, Default)]
pub struct Panel {
    pub node: Option<String>,
    pub popover: Option<Popover>,
    pub running: Option<String>,
    pub prompt: String,
    pub caret: usize,
    pub caret_seq: i32,
    /// The prompt node whose body is in edit mode (`None` = resting, read-only
    /// body). A single click never sets this; a double-click does.
    pub prompt_editing: Option<String>,
    /// The prompt text when editing began: Escape restores it.
    pub prompt_origin: String,
    pub image_ai: Option<crate::image::ai::ImageAiTool>,
}

/// The fixed model name shown on a generation card for its mode. The provider /
/// model assignment is locked by `AiConfig::normalize`.
pub fn default_model(mode: &str) -> &'static str {
    match mode {
        "video" => open_canvas_ai::config::VIDEO_MODEL,
        "text" => open_canvas_ai::config::DEEPSEEK_TEXT_MODEL,
        _ => open_canvas_ai::config::IMAGE_MODEL,
    }
}

/// Composer generation mode (`defaultMode`, canvas-node-prompt-panel.tsx:162-164).
pub fn node_mode(node: &CanvasNodeData) -> &'static str {
    match node.node_type {
        CanvasNodeType::Text => "text",
        CanvasNodeType::VideoPrompt | CanvasNodeType::VideoGeneration => "video",
        _ => "image",
    }
}

/// Provider brand of a model id, as the one-letter badge of the model row
/// (`components/model-picker.tsx` brand icons).
pub fn model_brand_letter(model: &str) -> &'static str {
    if model == open_canvas_ai::config::DEEPSEEK_TEXT_MODEL {
        return "D";
    }
    let provider = model.split('/').next().unwrap_or(model);
    match provider {
        "openai" => "O",
        "deepseek" => "D",
        "qwen" => "Q",
        "moonshotai" => "M",
        "minimax" => "m",
        "z-ai" => "Z",
        _ => "A",
    }
}

fn count_label(count: i64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// `Metadata.reasoning_effort` wire value ("auto".."xhigh").
pub fn reasoning_id(effort: CanvasReasoningEffort) -> &'static str {
    match effort {
        CanvasReasoningEffort::Auto => "auto",
        CanvasReasoningEffort::Low => "low",
        CanvasReasoningEffort::Medium => "medium",
        CanvasReasoningEffort::High => "high",
        CanvasReasoningEffort::Xhigh => "xhigh",
    }
}

fn clamp_count(value: Option<i64>) -> i64 {
    value.unwrap_or(1).clamp(1, 15)
}

fn metadata_str(value: Option<&String>, fallback: &str) -> String {
    value.cloned().unwrap_or_else(|| fallback.to_string())
}

/// One generation card's settings label (`settingsPanels` summary).
pub fn settings_label(node: &CanvasNodeData) -> String {
    let meta = node.metadata.as_ref();
    let mode = node_mode(node);
    let seconds = clamp_video_seconds(&metadata_str(meta.and_then(|m| m.seconds.as_ref()), "6"));
    let resolution =
        parse_video_resolution(&metadata_str(meta.and_then(|m| m.vquality.as_ref()), "720"));
    match mode {
        "video" => format!("{} · {seconds}s", video_resolution_label(&resolution)),
        "text" => format!(
            "Reasoning · {} · {}",
            reasoning_label(&metadata_str(
                meta.and_then(|m| m.reasoning_effort)
                    .map(|effort| reasoning_id(effort).to_string())
                    .as_ref(),
                "auto"
            )),
            count_label(clamp_count(meta.and_then(|m| m.text_count)), "generation")
        ),
        _ => format!(
            "{} · {} · {}",
            image_quality_label(&metadata_str(meta.and_then(|m| m.quality.as_ref()), "auto")),
            image_size_label(&metadata_str(meta.and_then(|m| m.size.as_ref()), "1:1")),
            count_label(clamp_count(meta.and_then(|m| m.count)), "image")
        ),
    }
}

/// The generation card's settings popover values.
pub fn generation_settings(node: &CanvasNodeData) -> GenerationSettings {
    let meta = node.metadata.as_ref();
    let size = metadata_str(meta.and_then(|m| m.size.as_ref()), "1:1");
    GenerationSettings {
        quality: metadata_str(meta.and_then(|m| m.quality.as_ref()), "auto").into(),
        size_scale: infer_media_scale(&size).into(),
        size_ratio: infer_media_ratio(&size).into(),
        count: i32::try_from(clamp_count(meta.and_then(|m| m.count))).unwrap_or(1),
        seconds: i32::try_from(clamp_video_seconds(&metadata_str(
            meta.and_then(|m| m.seconds.as_ref()),
            "6",
        )))
        .unwrap_or(6),
        resolution: parse_video_resolution(&metadata_str(
            meta.and_then(|m| m.vquality.as_ref()),
            "720",
        ))
        .into(),
        audio: meta.and_then(|m| m.generate_audio.as_deref()) != Some("false"),
        watermark: meta.and_then(|m| m.watermark.as_deref()) == Some("true"),
        reasoning: meta
            .and_then(|m| m.reasoning_effort)
            .map_or_else(
                || "auto".to_string(),
                |effort| reasoning_id(effort).to_string(),
            )
            .into(),
        text_count: i32::try_from(clamp_count(meta.and_then(|m| m.text_count))).unwrap_or(1),
    }
}

impl crate::app::App {
    /// Deferred UI sync after a metadata change: the node rows carry every
    /// inline value now, so only the AI dialogs need a separate push.
    pub fn sync_panel(&mut self, ui: &AppWindow) {
        self.sync_image_ai(ui);
    }

    pub fn node_popover(&mut self, ui: &AppWindow, node_id: &str, id: &str) {
        let next = PopoverKind::from_id(id).map(|id| Popover {
            node: node_id.to_string(),
            id,
        });
        self.panel.popover = match (&self.panel.popover, next) {
            (Some(current), Some(clicked))
                if current.node == clicked.node && current.id == clicked.id =>
            {
                None
            }
            (_, clicked) => clicked,
        };
        self.sync_rows_of(ui, &[node_id.to_string()]);
    }

    /// Settings pills/sliders → one metadata patch (image/video/text keys).
    pub fn node_setting(&mut self, ui: &AppWindow, node_id: &str, key: &str, value: &str) {
        let mut patch = open_canvas_core::node::CanvasNodeMetadata::default();
        match key {
            "quality" => patch.quality = Some(value.to_string()),
            "size-scale" | "size-ratio" => patch.size = self.size_patch(node_id, key, value),
            "count" => patch.count = value.parse().ok(),
            "seconds" => patch.seconds = Some(clamp_video_seconds(value).to_string()),
            "resolution" => {
                patch.vquality = Some(value.to_string());
                patch.size = self.video_size_patch(node_id, value);
            }
            "audio" => patch.generate_audio = Some(value.to_string()),
            "watermark" => patch.watermark = Some(value.to_string()),
            "reasoning" => patch.reasoning_effort = Some(Self::parse_reasoning(value)),
            "text-count" => patch.text_count = value.parse().ok(),
            "font-weight" => {
                patch.font_weight = Some(if value == "700" {
                    CanvasFontWeight::Bold
                } else {
                    CanvasFontWeight::Normal
                });
            }
            "italic" => patch.italic = Some(value == "true"),
            "font-family" => patch.font_family = Some(value.to_string()),
            "font-size" => {
                let current = self
                    .doc
                    .node(node_id)
                    .and_then(|node| node.metadata.as_ref())
                    .and_then(|meta| meta.font_size)
                    .unwrap_or(14.0);
                let delta = value.parse::<f64>().unwrap_or(0.0);
                patch.font_size = Some((current + delta).clamp(10.0, 32.0));
            }
            "text-align" => {
                patch.text_align = Some(match value {
                    "center" => CanvasTextAlign::Center,
                    "right" => CanvasTextAlign::Right,
                    _ => CanvasTextAlign::Left,
                });
            }
            "text-color" => {
                let Some(hex) = text_color_setting(value) else {
                    return;
                };
                patch.text_color = Some(hex);
            }
            _ => return,
        }
        self.patch_node_ui(ui, node_id, &patch);
    }

    /// Batch text alternatives: set the primary or take one out of
    /// `metadata.texts` (the chip list in the text card).
    pub fn node_text_option(
        &mut self,
        ui: &AppWindow,
        node_id: &str,
        option_id: &str,
        action: &str,
    ) {
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        let Some(meta) = node.metadata.as_mut() else {
            return;
        };
        match action {
            "primary" => meta.primary_text_id = Some(option_id.to_string()),
            "take-out" => {
                if let Some(texts) = meta.texts.as_mut() {
                    texts.retain(|text| text.id != option_id);
                    if meta.primary_text_id.as_deref() == Some(option_id) {
                        meta.primary_text_id = texts.first().map(|text| text.id.clone());
                    }
                    if texts.is_empty() {
                        meta.texts = None;
                        meta.primary_text_id = None;
                    }
                }
            }
            _ => return,
        }
        self.doc.commit(self.now_ms());
        self.note_doc_change();
        self.sync_rows_of(ui, &[node_id.to_string()]);
    }

    /// Canvas/node interaction closes the generation popover without touching
    /// the selection (`AntD` `pointerdown` outside the popover).
    pub fn panel_dismiss_popover(&mut self, ui: &AppWindow) {
        if self.panel.popover.is_some() {
            self.panel.popover = None;
            self.sync_rows_full(ui);
        }
    }

    /// Applies a metadata patch to one node and resyncs its row.
    fn patch_node_ui(
        &mut self,
        ui: &AppWindow,
        node_id: &str,
        patch: &open_canvas_core::node::CanvasNodeMetadata,
    ) {
        let now = self.now_ms();
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        let next = open_canvas_core::node::apply_node_config_patch(node, patch);
        *node = next;
        self.doc.commit(now);
        self.note_doc_change();
        self.sync_rows_of(ui, &[node_id.to_string()]);
        self.sync_wires(ui);
    }

    /// Generate for one generation card node id. The card never becomes a stop
    /// button; stopping is the placeholder image node's delete action.
    pub fn node_generate(&mut self, ui: &AppWindow, node_id: &str) {
        if self.panel.running.as_deref() == Some(node_id) {
            return;
        }
        self.ai_start(ui, node_id);
    }

    /// Removes the wire from `image_id` into the prompt card.
    pub fn node_ref_remove(&mut self, ui: &AppWindow, prompt_id: &str, image_id: &str) {
        let now = self.now_ms();
        let ids: Vec<String> = self
            .doc
            .connections()
            .iter()
            .filter(|connection| {
                connection.to_node_id == prompt_id && connection.from_node_id == image_id
            })
            .map(|connection| connection.id.clone())
            .collect();
        let mut removed = false;
        for id in ids {
            removed |= self.doc.delete_connection(&id, now);
        }
        if removed {
            self.note_doc_change();
            self.sync_rows_full(ui);
            self.sync_wires(ui);
        }
    }

    /// Pixel `size` string produced by a pill change (image scale/ratio pills).
    pub fn size_patch(&self, node_id: &str, key: &str, value: &str) -> Option<String> {
        let node = self.doc.node(node_id)?;
        let size = metadata_str(node.metadata.as_ref().and_then(|m| m.size.as_ref()), "1:1");
        match key {
            "size-scale" => {
                let ratio = infer_media_ratio(&size);
                let ratio = if ratio == "auto" { "1:1" } else { ratio };
                Some(compute_media_size(value, ratio))
            }
            "size-ratio" => Some(compute_media_size(infer_media_scale(&size), value)),
            _ => None,
        }
    }

    /// `settingsPanels.common.*` ids back to the metadata enum.
    fn parse_reasoning(value: &str) -> CanvasReasoningEffort {
        match value {
            "low" => CanvasReasoningEffort::Low,
            "medium" => CanvasReasoningEffort::Medium,
            "high" => CanvasReasoningEffort::High,
            "xhigh" => CanvasReasoningEffort::Xhigh,
            _ => CanvasReasoningEffort::Auto,
        }
    }

    /// Pixel `size` string produced by a resolution change on a video node.
    pub fn video_size_patch(&self, node_id: &str, resolution: &str) -> Option<String> {
        let node = self.doc.node(node_id)?;
        let size = metadata_str(node.metadata.as_ref().and_then(|m| m.size.as_ref()), "auto");
        if size == "auto" {
            return None;
        }
        Some(compute_video_size(resolution, infer_video_ratio(&size)))
    }
}

/// Registers the inline node callbacks (prompt editor, the generation card's
/// popovers/settings/action and the reference strip), plus the AI image tools
/// of `image_ai`.
pub fn wire_node_ui(ui: &AppWindow, app: &std::rc::Rc<std::cell::RefCell<crate::app::App>>) {
    crate::image::ai::wire_image_ai(ui, app);
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_prompt_edited(move |id, text, caret| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                app.panel.node = Some(id.to_string());
                app.panel_prompt_edited(&ui, text.as_str(), caret);
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_prompt_blur(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                if app.panel.node.as_deref() == Some(id.as_str()) {
                    app.panel_prompt_blur(&ui);
                }
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_prompt_edit_cancel(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                if app.panel.node.as_deref() == Some(id.as_str()) {
                    app.panel_prompt_edit_cancel(&ui);
                }
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_popover(move |id, kind| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                app.node_popover(&ui, id.as_str(), kind.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_setting(move |id, key, value| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                app.node_setting(&ui, id.as_str(), key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_text_option(move |id, option, action| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                app.node_text_option(&ui, id.as_str(), option.as_str(), action.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_generate(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| app.node_generate(&ui, id.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_ref_remove(move |id, image| {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| {
                app.node_ref_remove(&ui, id.as_str(), image.as_str());
            });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Doc;
    use crate::vm::{self, RowContext};
    use open_canvas_core::node::CanvasConnection;
    use slint::Model;

    fn app() -> crate::app::App {
        let mut app = crate::app::App::new(crate::store::Store::at(std::env::temp_dir()));
        app.doc = Doc::demo();
        app
    }

    fn row(app: &mut crate::app::App, id: &str) -> Option<crate::NodeItem> {
        let node = app.doc.node(id).cloned()?;
        let mut plugin = crate::plugin_host::PluginHost::new(std::env::temp_dir());
        let related = std::collections::BTreeSet::new();
        let expanded = std::collections::HashMap::new();
        let reference_slots = std::collections::HashMap::new();
        let smart_previews = std::collections::HashMap::new();
        let crate::app::App {
            doc,
            panel,
            media,
            edit,
            library,
            folders,
            ..
        } = app;
        let index = vm::NodeIndex::new(doc.nodes());
        let mut ctx = RowContext {
            nodes: doc.nodes(),
            connections: doc.connections(),
            index: &index,
            library: &library.index,
            media,
            edit,
            related: &related,
            reference_slots: &reference_slots,
            drop_target: None,
            drop_slot: None,
            panel,
            folders,
            asset_expanded: &expanded,
            smart_previews: &smart_previews,
        };
        Some(vm::node_row(&node, false, 1.0, &mut plugin, &mut ctx))
    }
    #[test]
    fn generation_cards_show_the_fixed_model_and_brand() {
        assert_eq!(model_brand_letter("openai/gpt-image-2.5-sunburst"), "O");
        assert_eq!(model_brand_letter("deepseek-flash"), "D");
        assert_eq!(model_brand_letter("minimax/hailuo-3-max"), "m");
        assert_eq!(model_brand_letter("mystery/model"), "A");

        let mut app = app();
        let image = row(&mut app, "image-generation");
        assert_eq!(
            image.as_ref().map(|row| row.model.as_str()),
            Some(open_canvas_ai::config::IMAGE_MODEL)
        );
        assert_eq!(
            image.as_ref().map(|row| row.model_brand.as_str()),
            Some("O")
        );
        let text = row(&mut app, "text");
        assert_eq!(
            text.as_ref().map(|row| row.model.as_str()),
            Some(open_canvas_ai::config::DEEPSEEK_TEXT_MODEL)
        );
        assert_eq!(text.as_ref().map(|row| row.model_brand.as_str()), Some("D"));
        let video = row(&mut app, "video-generation");
        assert_eq!(
            video.as_ref().map(|row| row.model.as_str()),
            Some(open_canvas_ai::config::VIDEO_MODEL)
        );
    }

    #[test]
    fn text_style_colour_swatches_write_metadata_and_the_row() {
        assert_eq!(text_color_setting("auto").as_deref(), Some(""));
        assert_eq!(text_color_setting("0").as_deref(), Some("#d85c3a"));
        assert_eq!(text_color_setting("4").as_deref(), Some("#b91c1c"));
        assert_eq!(text_color_setting("5"), None);
        assert_eq!(text_color_setting("nope"), None);

        let mut app = app();
        let hex = text_color_setting("1").unwrap_or_default();
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == "text")
            .and_then(|node| node.metadata.as_mut())
        {
            meta.text_color = Some(hex);
        }
        let data = row(&mut app, "text");
        assert!(data.as_ref().is_some_and(|row| row.body_colored));
        let color = data.as_ref().map(|row| row.body_color).unwrap_or_default();
        assert_eq!(
            (color.red(), color.green(), color.blue()),
            (0x15, 0x80, 0x3d),
            "the success ink reaches the row"
        );

        // Reset returns the row to the theme ink.
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == "text")
            .and_then(|node| node.metadata.as_mut())
        {
            meta.text_color = Some(String::new());
        }
        let data = row(&mut app, "text");
        assert!(!data.is_some_and(|row| row.body_colored));
    }

    #[test]
    fn typing_the_prompt_writes_the_node_metadata() {
        let mut app = app();
        app.panel.node = Some("prompt".to_string());
        app.panel.prompt = "typed inline".to_string();
        app.write_prompt("typed inline", 5);
        let stored = app
            .doc
            .node("prompt")
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.prompt.clone());
        assert_eq!(stored.as_deref(), Some("typed inline"));
        assert_eq!(
            app.doc.node("prompt").map(vm::node_body).as_deref(),
            Some("typed inline")
        );
        assert_eq!(app.panel.caret, 5);
    }

    #[test]
    fn reference_strip_follows_connections() {
        let mut app = app();
        assert_eq!(
            row(&mut app, "prompt").map(|row| row.ref_images.iter().count()),
            Some(0)
        );
        app.doc.connections_mut().push(CanvasConnection {
            id: "conn-ref".into(),
            from_node_id: "image".into(),
            to_node_id: "prompt".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        });
        assert_eq!(
            row(&mut app, "prompt").map(|row| row.ref_images.iter().count()),
            Some(1)
        );
        app.doc.delete_connection("conn-ref", 0);
        assert_eq!(
            row(&mut app, "prompt").map(|row| row.ref_images.iter().count()),
            Some(0)
        );
    }

    #[test]
    fn generation_body_reads_the_prompt_connection_per_node() {
        let mut app = app();
        let Some(image) = row(&mut app, "image-generation") else {
            return;
        };
        let Some(video) = row(&mut app, "video-generation") else {
            return;
        };
        assert!(image.can_generate);
        assert!(video.can_generate);
        assert_eq!(image.mode.as_str(), "image");
        assert_eq!(video.mode.as_str(), "video");
        assert_eq!(image.model.as_str(), open_canvas_ai::config::IMAGE_MODEL);
        app.doc.delete_connection("conn-prompt-image-generation", 0);
        assert_eq!(
            row(&mut app, "image-generation").map(|row| row.can_generate),
            Some(false)
        );
    }

    #[test]
    fn text_node_generation_body_reads_the_prompt_connection() {
        let mut app = app();
        let Some(text) = row(&mut app, "text") else {
            return;
        };
        assert!(text.can_generate, "the prompt connection arms the action");
        assert_eq!(text.mode.as_str(), "text");
        assert_eq!(
            text.model.as_str(),
            open_canvas_ai::config::DEEPSEEK_TEXT_MODEL,
            "the fixed text model labels the card"
        );
        assert_eq!(text.generation_settings.text_count, 1);
        assert_eq!(text.generation_settings.reasoning.as_str(), "auto");
    }

    #[test]
    fn generation_settings_and_label_cover_both_modes() {
        let app = app();
        let image = app.doc.node("image-generation");
        let video = app.doc.node("video-generation");
        assert!(image.is_some() && video.is_some());
        let Some(image) = image else { return };
        let Some(video) = video else { return };
        assert!(settings_label(image).contains("Auto"));
        assert_eq!(generation_settings(image).count, 1);
        assert_eq!(generation_settings(video).seconds, 6);
    }
}
