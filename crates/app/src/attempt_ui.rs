//! Image Attempt workbench: the centered prompt/config/result/trace modal and
//! the duplicate/extract operations that keep the version graph coherent.

use slint::ComponentHandle;

use open_canvas_core::node::{
    CanvasNodeImage, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType,
};

use crate::app::App;
use crate::coords::WorldPos;
use crate::AppWindow;

fn primary_image(meta: &CanvasNodeMetadata) -> Option<CanvasNodeImage> {
    let images = meta.images.as_ref()?;
    let primary = meta.primary_image_id.as_deref();
    images
        .iter()
        .find(|image| primary.is_some_and(|id| id == image.id))
        .or_else(|| images.first())
        .cloned()
}

impl App {
    /// Opens the centered workbench for one attempt. The card itself stays
    /// read-only; all edits go through this modal.
    pub fn attempt_open(&mut self, ui: &AppWindow, id: &str) {
        let Some(node) = self.doc.node(id).cloned() else {
            return;
        };
        if node.node_type != CanvasNodeType::ImageAttempt {
            return;
        }
        let mut meta = node.metadata.clone().unwrap_or_default();
        let runs = meta.attempt_runs.as_deref().unwrap_or_default();
        let image_count = runs.iter().map(|run| run.images.len()).sum::<usize>();
        if !runs.is_empty()
            && meta.agent_raw_trace.as_deref().is_none_or(str::is_empty)
            && meta
                .agent_steps
                .as_deref()
                .is_none_or(<[open_canvas_core::node::CanvasAgentStep]>::is_empty)
        {
            let prompt = meta.prompt.clone().unwrap_or_default();
            let model = meta.model.clone().unwrap_or_default();
            let raw = format!(
                "Recovered attempt history
This attempt has {} run(s) and {} image(s) recorded before agent trace capture was enabled.

Prompt:
{}

Model: {}
",
                runs.len(),
                image_count,
                prompt,
                model
            );
            meta.agent_raw_trace = Some(raw);
            meta.agent_steps = Some(vec![
                open_canvas_core::node::CanvasAgentStep {
                    kind: "observe".into(),
                    title: "Recovered attempt history".into(),
                    detail: Some(format!("{} run(s) · {} image(s)", runs.len(), image_count)),
                    at: Some(crate::store::now_iso()),
                },
                open_canvas_core::node::CanvasAgentStep {
                    kind: "plan".into(),
                    title: "Trace capture was not enabled for this run".into(),
                    detail: Some("New runs record the agent activity timeline".into()),
                    at: Some(crate::store::now_iso()),
                },
            ]);
            if let Some(current) = self.doc.nodes_mut().iter_mut().find(|n| n.id == id) {
                current.metadata = Some(meta.clone());
            }
            self.doc.commit(self.now_ms());
            self.note_doc_change();
        }
        let image = primary_image(&meta)
            .and_then(|image| {
                self.media
                    .image_or_thumbnail(image.thumbnail.as_deref(), &image.content)
            })
            .unwrap_or_default();
        ui.set_attempt_id(id.into());
        ui.set_attempt_prompt(meta.prompt.clone().unwrap_or_default().into());
        ui.set_attempt_model(meta.model.clone().unwrap_or_default().into());
        ui.set_attempt_size(meta.size.clone().unwrap_or_else(|| "1:1".into()).into());
        ui.set_attempt_quality(meta.quality.clone().unwrap_or_else(|| "auto".into()).into());
        ui.set_attempt_count(i32::try_from(meta.count.unwrap_or(1)).unwrap_or(1));
        ui.set_attempt_version(i32::try_from(meta.attempt_version.unwrap_or(1)).unwrap_or(1));
        ui.set_attempt_intent(meta.attempt_title.clone().unwrap_or_default().into());
        ui.set_attempt_frozen(meta.attempt_frozen.unwrap_or(false));
        ui.set_attempt_run_count(i32::try_from(runs.len()).unwrap_or(i32::MAX));
        ui.set_attempt_image_count(i32::try_from(image_count).unwrap_or(i32::MAX));
        ui.set_attempt_image(image.clone());
        ui.set_attempt_has_image(image.size().width > 0);
        let timeline = meta
            .agent_steps
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|step| {
                let detail = step
                    .detail
                    .as_deref()
                    .map(|detail| format!(" — {detail}"))
                    .unwrap_or_default();
                format!("[{}] {}{detail}", step.kind, step.title)
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let raw = meta.agent_raw_trace.clone().unwrap_or_default();
        let trace = if timeline.is_empty() {
            raw
        } else if raw.is_empty() {
            format!(
                "TIMELINE
{timeline}"
            )
        } else {
            format!(
                "TIMELINE
{timeline}

RAW
{raw}"
            )
        };
        ui.set_attempt_trace(trace.into());
        ui.set_attempt_open(true);
        self.attempt_modal = Some(id.to_string());
    }

    pub fn attempt_close(&mut self, ui: &AppWindow) {
        let Some(id) = self.attempt_modal.take() else {
            ui.set_attempt_open(false);
            return;
        };
        self.doc.commit(self.now_ms());
        self.note_doc_change();
        self.sync_rows_of(ui, &[id]);
        ui.set_attempt_open(false);
    }

    /// Draft prompt edits write straight to the node metadata. A frozen attempt
    /// is read-only in the modal, so this is a no-op there.
    pub fn attempt_prompt_edited(&mut self, text: &str) {
        let Some(id) = self.attempt_modal.as_deref() else {
            return;
        };
        let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
            return;
        };
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        if meta.attempt_frozen.unwrap_or(false) {
            return;
        }
        meta.prompt = Some(text.to_string());
        self.note_doc_change();
    }

    pub fn attempt_config_edited(&mut self, key: &str, value: &str) {
        let Some(id) = self.attempt_modal.as_deref() else {
            return;
        };
        let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
            return;
        };
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        if meta.attempt_frozen.unwrap_or(false) {
            return;
        }
        match key {
            "model" => meta.model = Some(value.trim().to_string()),
            "size" => meta.size = Some(value.trim().to_string()),
            "quality" => meta.quality = Some(value.trim().to_string()),
            "count" => {
                if let Ok(count) = value.trim().parse::<i64>() {
                    meta.count = Some(count.clamp(1, 4));
                }
            }
            _ => return,
        }
        self.note_doc_change();
    }

    pub fn attempt_run(&mut self, ui: &AppWindow) {
        let Some(id) = self.attempt_modal.clone() else {
            return;
        };
        self.doc.commit(self.now_ms());
        self.note_doc_change();
        ui.set_attempt_open(false);
        self.attempt_modal = None;
        self.sync_rows_full(ui);
        self.node_generate(ui, &id);
    }

    /// Copies prompt/config into a fresh child attempt with no image. The
    /// duplicate edge records why the branch exists.
    pub fn attempt_duplicate(&mut self, ui: &AppWindow) {
        let Some(id) = self.attempt_modal.clone() else {
            return;
        };
        let Some(source) = self.doc.node(&id).cloned() else {
            return;
        };
        let now = self.now_ms();
        let source_meta = source.metadata.clone().unwrap_or_default();
        let version = self
            .doc
            .nodes()
            .iter()
            .filter_map(|node| node.metadata.as_ref().and_then(|meta| meta.attempt_version))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let at = WorldPos::new(
            source.position.x + source.width + 48.0 + 190.0,
            source.position.y + source.height / 2.0,
        );
        let new_id = self.doc.create_node(CanvasNodeType::ImageAttempt, at, now);
        let mut meta = CanvasNodeMetadata {
            prompt: source_meta.prompt.clone(),
            model: source_meta.model.clone(),
            size: source_meta.size.clone(),
            quality: source_meta.quality.clone(),
            background: source_meta.background.clone(),
            count: source_meta.count,
            generation_mode: source_meta.generation_mode,
            status: Some(CanvasNodeStatus::Idle),
            attempt_version: Some(version),
            attempt_title: Some(format!("v{version} · copy")),
            attempt_frozen: Some(false),
            attempt_runs: Some(Vec::new()),
            ..CanvasNodeMetadata::default()
        };
        meta.prompt = source_meta.prompt;
        if let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == new_id)
        {
            node.title = format!("v{version} · copy");
            node.metadata = Some(meta);
        }
        self.doc.commit(now);
        self.note_doc_change();
        self.attempt_modal = Some(new_id.clone());
        self.sync_rows_full(ui);
        self.attempt_open(ui, &new_id);
    }

    /// Extracts the primary output into a pure Image node and links it back to
    /// the attempt with an `extract` edge.
    pub fn attempt_extract(&mut self, ui: &AppWindow) {
        let Some(id) = self.attempt_modal.clone() else {
            return;
        };
        let Some(source) = self.doc.node(&id).cloned() else {
            return;
        };
        let Some(image) = source.metadata.as_ref().and_then(primary_image) else {
            return;
        };
        let now = self.now_ms();
        let at = WorldPos::new(
            source.position.x + source.width + 48.0 + 170.0,
            source.position.y + source.height / 2.0,
        );
        let new_id = self.doc.create_node(CanvasNodeType::Image, at, now);
        if let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == new_id)
        {
            node.title = "Extracted image".into();
            node.metadata = Some(CanvasNodeMetadata {
                content: Some(image.content.clone()),
                thumbnail: image.thumbnail.clone(),
                status: Some(CanvasNodeStatus::Success),
                natural_width: Some(image.natural_width),
                natural_height: Some(image.natural_height),
                mime_type: Some(image.mime_type.clone()),
                bytes: Some(image.bytes),
                free_resize: Some(true),
                ..CanvasNodeMetadata::default()
            });
        }
        self.doc.commit(now);
        self.note_doc_change();
        self.sync_rows_full(ui);
        self.sync_wires(ui);
    }
}

/// Wires the workbench callbacks.
pub fn wire(ui: &AppWindow, app: &std::rc::Rc<std::cell::RefCell<App>>) {
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_prompt_edited(move |text| {
            if weak.upgrade().is_none() {
                return;
            }
            crate::app::with_app(&app, |app| app.attempt_prompt_edited(text.as_str()));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_config_edited(move |key, value| {
            if weak.upgrade().is_none() {
                return;
            }
            crate::app::with_app(&app, |app| {
                app.attempt_config_edited(key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_run(move || {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| app.attempt_run(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_duplicate(move || {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| app.attempt_duplicate(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_extract(move || {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| app.attempt_extract(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_attempt_close(move || {
            let Some(ui) = weak.upgrade() else { return };
            crate::app::with_app(&app, |app| app.attempt_close(&ui));
        });
    }
}
