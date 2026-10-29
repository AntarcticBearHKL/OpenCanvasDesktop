//! AI-backed image tools: the application half of the capability that already
//! lived in the library layers.
//!
//! * **OCR** (`canvas-ocr.ts`): the vision path of `crates/ai`
//!   (`request_image_question`) with the prompt/messages and result parsing of
//!   `crates/ps::ocr`; the transcription lands as a Text node.
//! * **Multi-angle regeneration** (`canvas-node-angle-dialog.tsx`): N
//!   deterministic variants from `crates/ps::angle`, each one image edit whose
//!   result becomes a derived Image node.
//! * **AI upscale** (`canvas-node-resolution-dialog.tsx` AI tab): the tiling and
//!   stitch of `crates/ps::upscale` around per-tile image edits, stitched back
//!   into one higher-resolution surface.
//! * **Segmentation** (`mobile-sam.ts`): the tensor/mask plumbing of
//!   `crates/ps::segmentation` wired to an honest "no local runtime" state —
//!   this build ships no ONNX runtime, so the request path fails with
//!   [`open_canvas_ps::PsError::SegmenterUnavailable`] instead of fabricating a
//!   mask.
//!
//! All network work runs on the shared [`AiService`] runtime and comes back as
//! [`AiEvent`]s drained by `poll_ai`; every dialog is a pure function of the
//! Rust state pushed by [`App::sync_image_ai`].
//! Owns the studio's AI job state (job ids, dialogs) and their view-model sync.

use slint::ComponentHandle;

use open_canvas_ai::config::{ModelCapability, ModelRequestConfig};
use open_canvas_ai::image::request_edit;
use open_canvas_ai::retry::{run_with_retry, GENERATION_MAX_ATTEMPTS, GENERATION_RETRY_DELAY_MS};
use open_canvas_ai::runtime_log::RuntimeLogLevel;
use open_canvas_ai::Cancel;
use open_canvas_core::geometry::{fit_node_size, FIT_NODE_MAX_SIZE};
use open_canvas_core::node::{
    CanvasNodeData, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType,
};
use open_canvas_ps::angle::{angle_variant_plan, AngleParams, AngleVariantPlan};
use open_canvas_ps::ocr::{
    build_ocr_request_default, parse_ocr_response, OcrImageRef, OcrRequest, OCR_PROMPT,
};
use open_canvas_ps::segmentation::{segment_image, SamPoint, SegmentOptions};
use open_canvas_ps::upscale::{
    crop_surface, plan_upscale_tiles, stitch_upscale_tiles, upscale_ai_prompt, UpscaleTilePlan,
    RESOLUTION_AI_DEFAULT_PROMPT,
};
use open_canvas_ps::Surface;

use crate::ai_service::{request_config, AiEvent, AiService};
use crate::app::App;
use crate::coords::WorldPos;
use crate::AppWindow;

/// Variant counts offered by the angle dialog (the plan itself accepts 1..=15).
pub const ANGLE_VARIANT_CHOICES: [usize; 6] = [1, 2, 3, 4, 6, 8];
/// Factors offered by the AI upscale tab (the web's 2x / 4x segmented).
pub const UPSCALE_FACTORS: [&str; 2] = ["2x", "4x"];
/// Gap between derived AI image nodes (the web's insertion stagger).
pub const AI_NODE_GAP: f64 = 16.0;
/// World-space offset of a derived node from its source (`insertDerivedAsset`).
pub const AI_NODE_OFFSET: f64 = 96.0;

/// One image edit of an AI image job: the prompt plus the reference data URL.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAiRequest {
    pub prompt: String,
    pub reference: String,
}

/// Which AI image tool a dialog shows.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageAiKind {
    Ocr,
    Angle(AngleTool),
    Upscale(UpscaleTool),
    Segment,
}

impl ImageAiKind {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Angle(_) => "angle",
            Self::Upscale(_) => "upscale",
            Self::Segment => "segment",
        }
    }
}

/// The angle dialog's live parameters and variant count.
#[derive(Debug, Clone, PartialEq)]
pub struct AngleTool {
    pub params: AngleParams,
    pub count: usize,
}

impl Default for AngleTool {
    fn default() -> Self {
        Self {
            params: AngleParams::default(),
            count: 2,
        }
    }
}

/// The AI upscale tab's factor and prompt prefill.
#[derive(Debug, Clone, PartialEq)]
pub struct UpscaleTool {
    /// `2x` | `4x`.
    pub factor: String,
    /// The editable prompt addition (`resolutionAiDefaultPrompt`).
    pub prompt: String,
}

impl Default for UpscaleTool {
    fn default() -> Self {
        Self {
            factor: "2x".to_string(),
            prompt: RESOLUTION_AI_DEFAULT_PROMPT.to_string(),
        }
    }
}

/// One open AI image tool dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAiTool {
    pub node_id: String,
    pub kind: ImageAiKind,
    /// The running background job, when a request is in flight.
    pub job_id: Option<u64>,
    pub done: usize,
    pub total: usize,
    pub message: String,
    pub error: bool,
    /// The OCR transcription once the vision request finished.
    pub text: String,
}

/// `upscaleAiFactor` read: the factor multiplier behind a `2x` / `4x` chip.
pub fn upscale_factor(value: &str) -> usize {
    match value.trim() {
        "4x" => 4,
        _ => 2,
    }
}

/// One image edit per angle variant, in plan order.
pub fn angle_requests(reference: &str, plan: &AngleVariantPlan) -> Vec<ImageAiRequest> {
    plan.variants
        .iter()
        .map(|variant| ImageAiRequest {
            prompt: variant.prompt.clone(),
            reference: reference.to_string(),
        })
        .collect()
}

/// The tile plan of an AI upscale for a source surface and factor.
pub fn upscale_plan(surface: &Surface, factor: &str) -> UpscaleTilePlan {
    let long_edge = surface.width().max(surface.height());
    let target = long_edge.saturating_mul(upscale_factor(factor));
    plan_upscale_tiles(
        surface.width(),
        surface.height(),
        target,
        open_canvas_ps::DEFAULT_UPSCALE_TILE,
    )
}

/// The tile plan and the per-tile image edits of an AI upscale: each tile is
/// cropped from the source, encoded as a data URL and enhanced by one request.
pub fn upscale_job(
    surface: &Surface,
    factor: &str,
    custom_prompt: &str,
) -> Result<(UpscaleTilePlan, Vec<ImageAiRequest>), String> {
    if surface.width() == 0 || surface.height() == 0 {
        return Err("This node has no image content".to_string());
    }
    let plan = upscale_plan(surface, factor);
    let prompt = upscale_ai_prompt(factor, custom_prompt);
    let mut requests = Vec::with_capacity(plan.tiles.len());
    for tile in &plan.tiles {
        let cropped = crop_surface(surface, tile.source);
        let (data_url, _bytes) = crate::image::tools::surface_png(&cropped)?;
        requests.push(ImageAiRequest {
            prompt: prompt.clone(),
            reference: data_url,
        });
    }
    Ok((plan, requests))
}

/// Decode the returned tile data URLs and stitch them into the target surface.
/// Any failed tile fails the whole stitch (a partially transparent image would
/// be a fabricated result).
pub fn stitch_ai_tiles(
    plan: &UpscaleTilePlan,
    results: &[Result<String, String>],
) -> Result<Surface, String> {
    if results.len() != plan.tiles.len() {
        return Err(format!(
            "The upscale returned {} of {} tiles",
            results.len(),
            plan.tiles.len()
        ));
    }
    let mut tiles: Vec<Option<Surface>> = Vec::with_capacity(results.len());
    for (tile, result) in plan.tiles.iter().zip(results) {
        match result {
            Ok(url) => {
                let surface = crate::media::decode_surface(url)
                    .ok_or_else(|| format!("Tile {} could not be decoded", tile.index + 1))?;
                tiles.push(Some(surface));
            }
            Err(error) => return Err(error.clone()),
        }
    }
    stitch_upscale_tiles(plan, &tiles)
        .ok_or_else(|| "The upscaled image could not be stitched".to_string())
}

/// The OCR result parser as the app consumes it (`canvas-ocr.ts:33`).
pub fn ocr_text(raw: &str) -> Result<String, String> {
    parse_ocr_response(raw)
        .map(|result| result.text)
        .map_err(|error| error.to_string())
}

/// `Metadata.reasoning_effort` is irrelevant here; this is the image-tool
/// request config: the node's image overrides via `request_config`.
fn image_request_config(app: &App, node: &CanvasNodeData) -> ModelRequestConfig {
    request_config(app, node, ModelCapability::Image)
}

/// The OCR vision request of a node (`canvas-ocr.ts:21-33`): the default OCR
/// prompt plus the node's stored image as the reference data URL. `None` when
/// the node has no image content.
pub fn ocr_request_for(node: &CanvasNodeData) -> Option<OcrRequest> {
    let source = crate::image::tools::node_source_text(node)?;
    let mime = node
        .metadata
        .as_ref()
        .and_then(|meta| meta.mime_type.clone())
        .filter(|mime| !mime.trim().is_empty())
        .unwrap_or_else(|| "image/png".to_string());
    let image = OcrImageRef::new(node.id.clone(), format!("{}.png", node.title), mime, source);
    Some(build_ocr_request_default(&image))
}

/// The error message the segmentation path reports: the honest
/// [`open_canvas_ps::PsError::SegmenterUnavailable`] state, never a mask.
pub fn segment_unavailable(surface: &Surface) -> String {
    let points = [SamPoint::new(
        crate::vm::i2f(crate::vm::u2i(surface.width())) / 2.0,
        crate::vm::i2f(crate::vm::u2i(surface.height())) / 2.0,
        true,
    )];
    match segment_image(None, surface, &points, &SegmentOptions::default()) {
        Ok(_) => "Segmentation returned a mask without a runtime".to_string(),
        Err(error) => format!(
            "No local segmentation runtime is available. This build ships no ONNX runtime, so MobileSAM cannot run; the request path reports \"{error}\" instead of a fabricated mask."
        ),
    }
}

impl AiService {
    /// Allocates the next background-job id (image AI tools).
    pub fn next_job_id(&mut self) -> u64 {
        self.job_seq = self.job_seq.wrapping_add(1);
        self.job_seq
    }

    /// OCR: one streaming vision request; the parsed transcription comes back
    /// as [`AiEvent::OcrDone`] (or [`AiEvent::OcrError`]). The job id registers
    /// the request for the dialog's cancel control.
    pub fn spawn_ocr(
        &mut self,
        req: ModelRequestConfig,
        node_id: String,
        request: OcrRequest,
        job_id: u64,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let cancel = Cancel::new();
        let task_cancel = cancel.clone();
        let handle = self.runtime.spawn(async move {
            let prompt = request.prompt.clone();
            let images = request.image_urls();
            let result = run_with_retry(
                GENERATION_MAX_ATTEMPTS,
                GENERATION_RETRY_DELAY_MS,
                &task_cancel,
                |_attempt| {
                    let client = client.clone();
                    let req = req.clone();
                    let prompt = prompt.clone();
                    let images = images.clone();
                    let cancel = task_cancel.clone();
                    async move {
                        open_canvas_ai::image::request_image_question(
                            &client,
                            &req,
                            &prompt,
                            &images,
                            &cancel,
                            &mut |_text| {},
                        )
                        .await
                    }
                },
            )
            .await;
            let event = match result {
                Ok(response) => match ocr_text(&response.content) {
                    Ok(text) => AiEvent::OcrDone { node_id, text },
                    Err(message) => AiEvent::OcrError { node_id, message },
                },
                Err(error) if error.is_canceled() => return,
                Err(error) => AiEvent::OcrError {
                    node_id,
                    message: error.to_string(),
                },
            };
            let _sent = tx.send(event);
        });
        self.jobs.insert(
            job_id,
            crate::ai_service::TaskHandle {
                join: handle,
                cancel,
            },
        );
    }

    /// One AI image job (angle variants / upscale tiles): sequential image
    /// edits with per-item progress events and a final result list.
    pub fn spawn_image_ai_job(
        &mut self,
        req: ModelRequestConfig,
        job_id: u64,
        requests: Vec<ImageAiRequest>,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let cancel = Cancel::new();
        let task_cancel = cancel.clone();
        let handle = self.runtime.spawn(async move {
            let total = requests.len();
            let mut results: Vec<Result<String, String>> = Vec::with_capacity(total);
            for (index, item) in requests.into_iter().enumerate() {
                let result = run_with_retry(
                    GENERATION_MAX_ATTEMPTS,
                    GENERATION_RETRY_DELAY_MS,
                    &task_cancel,
                    |_attempt| {
                        let client = client.clone();
                        let req = req.clone();
                        let cancel = task_cancel.clone();
                        let prompt = item.prompt.clone();
                        let reference = item.reference.clone();
                        async move {
                            request_edit(
                                &client,
                                &req,
                                &prompt,
                                std::slice::from_ref(&reference),
                                &cancel,
                            )
                            .await
                        }
                    },
                )
                .await;
                let item_result = match result {
                    Ok(response) => response
                        .images
                        .into_iter()
                        .map(|image| image.data_url)
                        .next()
                        .ok_or_else(|| "The provider returned no image".to_string()),
                    Err(error) if error.is_canceled() => {
                        // Cancelled: no further events; the UI clears its state.
                        return;
                    }
                    Err(error) => Err(error.to_string()),
                };
                let _sent = tx.send(AiEvent::ImageAiItem {
                    job_id,
                    index,
                    result: item_result.clone(),
                });
                results.push(item_result);
            }
            let _sent = tx.send(AiEvent::ImageAiDone { job_id, results });
        });
        self.jobs.insert(
            job_id,
            crate::ai_service::TaskHandle {
                join: handle,
                cancel,
            },
        );
    }

    /// Cancels a background image-AI job by id.
    pub fn cancel_job(&mut self, job_id: u64) -> bool {
        let Some(task) = self.jobs.remove(&job_id) else {
            return false;
        };
        task.cancel.cancel();
        task.join.abort();
        true
    }
}

impl App {
    /// Opens an AI image tool dialog for an image node.
    pub fn image_ai_open(&mut self, ui: &AppWindow, node_id: &str, tool: &str) {
        let Some(node) = self.doc.node(node_id).cloned() else {
            return;
        };
        if !matches!(
            node.node_type,
            CanvasNodeType::Image | CanvasNodeType::ImageStack
        ) {
            return;
        }
        if crate::image::tools::node_surface(&node).is_none() {
            self.set_notice(ui, "This node has no image content yet".to_string(), true);
            return;
        }
        let kind = match tool {
            "ocr" => ImageAiKind::Ocr,
            "angle" => ImageAiKind::Angle(AngleTool::default()),
            "ai-upscale" => ImageAiKind::Upscale(UpscaleTool::default()),
            "segment" => ImageAiKind::Segment,
            _ => return,
        };
        self.panel.image_ai = Some(ImageAiTool {
            node_id: node_id.to_string(),
            kind,
            job_id: None,
            done: 0,
            total: 0,
            message: String::new(),
            error: false,
            text: String::new(),
        });
        self.bars.preview_node = None;
        self.bars.info_node = None;
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            format!("Opened the {tool} dialog"),
            Some(node_id.to_string()),
        );
        self.sync_image_ai(ui);
        self.sync_modals(ui);
    }

    /// Closes the AI image tool dialog (an in-flight job is cancelled).
    pub fn image_ai_close(&mut self, ui: &AppWindow) {
        self.cancel_image_ai_job();
        self.panel.image_ai = None;
        self.sync_image_ai(ui);
    }

    /// Chip picks of an open dialog (`factor`, `format`, `variants`, `wide`).
    pub fn image_ai_pick(&mut self, ui: &AppWindow, key: &str, value: &str) {
        let Some(tool) = self.panel.image_ai.as_mut() else {
            return;
        };
        match (&mut tool.kind, key) {
            (ImageAiKind::Upscale(upscale), "factor") if UPSCALE_FACTORS.contains(&value) => {
                upscale.factor = value.to_string();
            }
            (ImageAiKind::Angle(angle), "variants") => {
                if let Ok(count) = value.parse::<usize>() {
                    angle.count = count.clamp(
                        open_canvas_ps::ANGLE_VARIANT_MIN,
                        open_canvas_ps::ANGLE_VARIANT_MAX,
                    );
                }
            }
            (ImageAiKind::Angle(angle), "wide") => angle.params.wide_angle = value == "wide",
            _ => {}
        }
        tool.error = false;
        self.sync_image_ai(ui);
    }

    /// Field edits of an open dialog (`prompt`, `horizontal`, `pitch`,
    /// `distance`, `voice`, `format`, `speed`).
    pub fn image_ai_field(&mut self, ui: &AppWindow, key: &str, value: &str) {
        let Some(tool) = self.panel.image_ai.as_mut() else {
            return;
        };
        let number = value.trim().parse::<f64>().ok();
        match (&mut tool.kind, key) {
            (ImageAiKind::Upscale(upscale), "prompt") => upscale.prompt = value.to_string(),
            (ImageAiKind::Angle(angle), "horizontal") => {
                if let Some(number) = number.filter(|value| value.is_finite()) {
                    angle.params.horizontal_angle = number.clamp(-60.0, 60.0);
                }
            }
            (ImageAiKind::Angle(angle), "pitch") => {
                if let Some(number) = number.filter(|value| value.is_finite()) {
                    angle.params.pitch_angle = number.clamp(-45.0, 45.0);
                }
            }
            (ImageAiKind::Angle(angle), "distance") => {
                if let Some(number) = number.filter(|value| value.is_finite()) {
                    angle.params.camera_distance = number.clamp(1.0, 10.0);
                }
            }
            _ => {}
        }
        tool.error = false;
        self.sync_image_ai(ui);
    }

    /// The dialog's primary action: run the request, or apply its result.
    pub fn image_ai_apply(&mut self, ui: &AppWindow) {
        let Some(tool) = self.panel.image_ai.clone() else {
            return;
        };
        if tool.job_id.is_some() {
            return;
        }
        match &tool.kind {
            ImageAiKind::Ocr if !tool.text.is_empty() => self.ocr_insert_text_node(ui, &tool),
            ImageAiKind::Ocr => self.ocr_run(ui, &tool),
            ImageAiKind::Angle(angle) => self.angle_run(ui, &tool, angle),
            ImageAiKind::Upscale(upscale) => self.upscale_run(ui, &tool, upscale),
            ImageAiKind::Segment => self.segment_run(ui, &tool),
        }
    }

    /// Cancels the dialog's running request without closing the dialog.
    pub fn image_ai_cancel(&mut self, ui: &AppWindow) {
        if !self.cancel_image_ai_job() {
            return;
        }
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.job_id = None;
            tool.done = 0;
            tool.total = 0;
            tool.message = "Cancelled".to_string();
            tool.error = false;
        }
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            "AI image request cancelled",
            None,
        );
        self.sync_image_ai(ui);
    }

    fn cancel_image_ai_job(&mut self) -> bool {
        let Some(job_id) = self.panel.image_ai.as_ref().and_then(|tool| tool.job_id) else {
            return false;
        };
        let cancelled = self
            .ai
            .as_mut()
            .is_some_and(|service| service.cancel_job(job_id));
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.job_id = None;
        }
        cancelled
    }

    fn set_image_ai_message(&mut self, ui: &AppWindow, message: String, error: bool) {
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.message = message;
            tool.error = error;
        }
        self.sync_image_ai(ui);
    }

    fn ocr_run(&mut self, ui: &AppWindow, tool: &ImageAiTool) {
        let Some(node) = self.doc.node(&tool.node_id).cloned() else {
            return;
        };
        let Some(request) = ocr_request_for(&node) else {
            self.set_image_ai_message(ui, "This node has no image content".to_string(), true);
            return;
        };
        let req = request_config(self, &node, ModelCapability::Text);
        if req.api_key.trim().is_empty() {
            self.set_image_ai_message(
                ui,
                "Set a provider API key in Settings before extracting text".to_string(),
                true,
            );
            return;
        }
        let node_id = node.id.clone();
        let Some(service) = self.ai.as_mut() else {
            self.set_image_ai_message(ui, "The AI runtime could not start".to_string(), true);
            return;
        };
        let job_id = service.next_job_id();
        service.spawn_ocr(req, node_id, request, job_id);
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.job_id = Some(job_id);
            tool.done = 0;
            tool.total = 1;
            tool.message = "Extracting text...".to_string();
            tool.error = false;
        }
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            "OCR request started",
            Some(tool.node_id.clone()),
        );
        self.sync_image_ai(ui);
    }

    fn ocr_insert_text_node(&mut self, ui: &AppWindow, tool: &ImageAiTool) {
        let Some(source) = self.doc.node(&tool.node_id).cloned() else {
            return;
        };
        let now = self.now_ms();
        let spec = open_canvas_core::constants::get_node_spec(&CanvasNodeType::Text);
        let at = WorldPos::new(
            source.position.x + source.width + AI_NODE_OFFSET + spec.width / 2.0,
            source.position.y + spec.height / 2.0,
        );
        let id = self.doc.create_node(CanvasNodeType::Text, at, now);
        if let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
            let title: String = tool.text.chars().take(32).collect();
            node.title = if title.trim().is_empty() {
                "OCR text".to_string()
            } else {
                title
            };
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.content = Some(tool.text.clone());
            meta.prompt = Some(OCR_PROMPT.to_string());
            meta.status = Some(CanvasNodeStatus::Success);
        }
        self.panel.image_ai = None;
        self.note_doc_change();
        self.set_notice(ui, "Created a text node from the image".to_string(), false);
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            "OCR result saved as a text node",
            Some(id),
        );
        self.sync_all(ui);
    }

    fn angle_run(&mut self, ui: &AppWindow, tool: &ImageAiTool, angle: &AngleTool) {
        let Some(node) = self.doc.node(&tool.node_id).cloned() else {
            return;
        };
        let Some(source) = crate::image::tools::node_source_text(&node) else {
            self.set_image_ai_message(ui, "This node has no image content".to_string(), true);
            return;
        };
        let req = image_request_config(self, &node);
        if req.api_key.trim().is_empty() {
            self.set_image_ai_message(
                ui,
                "Set a provider API key in Settings before generating".to_string(),
                true,
            );
            return;
        }
        let plan = angle_variant_plan(&angle.params, angle.count);
        let requests = angle_requests(&source, &plan);
        let total = requests.len();
        let Some(service) = self.ai.as_mut() else {
            self.set_image_ai_message(ui, "The AI runtime could not start".to_string(), true);
            return;
        };
        let job_id = service.next_job_id();
        service.spawn_image_ai_job(req, job_id, requests);
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.job_id = Some(job_id);
            tool.done = 0;
            tool.total = total;
            tool.message = format!("0 / {total}");
            tool.error = false;
        }
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            format!("Multi-angle generation started ({total} views)"),
            Some(tool.node_id.clone()),
        );
        self.sync_image_ai(ui);
    }

    fn upscale_run(&mut self, ui: &AppWindow, tool: &ImageAiTool, upscale: &UpscaleTool) {
        let Some(node) = self.doc.node(&tool.node_id).cloned() else {
            return;
        };
        let Some(surface) = crate::image::tools::node_surface(&node) else {
            self.set_image_ai_message(ui, "This node has no image content".to_string(), true);
            return;
        };
        let (plan, requests) = match upscale_job(&surface, &upscale.factor, &upscale.prompt) {
            Ok(job) => job,
            Err(message) => {
                self.set_image_ai_message(ui, message, true);
                return;
            }
        };
        let req = image_request_config(self, &node);
        if req.api_key.trim().is_empty() {
            self.set_image_ai_message(
                ui,
                "Set a provider API key in Settings before generating".to_string(),
                true,
            );
            return;
        }
        let total = requests.len();
        let Some(service) = self.ai.as_mut() else {
            self.set_image_ai_message(ui, "The AI runtime could not start".to_string(), true);
            return;
        };
        let job_id = service.next_job_id();
        service.spawn_image_ai_job(req, job_id, requests);
        if let Some(tool) = self.panel.image_ai.as_mut() {
            tool.job_id = Some(job_id);
            tool.done = 0;
            tool.total = total;
            tool.message = format!("0 / {total} tiles");
            tool.error = false;
        }
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            format!(
                "AI upscale started ({} x {} -> {} x {}, {total} tiles)",
                plan.source_width, plan.source_height, plan.target_width, plan.target_height
            ),
            Some(tool.node_id.clone()),
        );
        self.sync_image_ai(ui);
    }

    fn segment_run(&mut self, ui: &AppWindow, tool: &ImageAiTool) {
        let Some(node) = self.doc.node(&tool.node_id).cloned() else {
            return;
        };
        let Some(surface) = crate::image::tools::node_surface(&node) else {
            self.set_image_ai_message(ui, "This node has no image content".to_string(), true);
            return;
        };
        let message = segment_unavailable(&surface);
        self.log(
            RuntimeLogLevel::Error,
            "image-ai",
            "Segmentation requested without a local runtime",
            Some(tool.node_id.clone()),
        );
        self.set_image_ai_message(ui, message, true);
    }

    /// One AI image item finished (angle variant / upscale tile).
    pub fn apply_image_ai_item(
        &mut self,
        ui: &AppWindow,
        job_id: u64,
        index: usize,
        result: Result<String, String>,
    ) {
        let Some(tool) = self.panel.image_ai.as_mut() else {
            return;
        };
        if tool.job_id != Some(job_id) {
            return;
        }
        tool.done = tool.done.max(index + 1);
        match result {
            Ok(_) => {
                tool.message = format!("{} / {}", tool.done, tool.total);
            }
            Err(error) => {
                tool.error = true;
                tool.message = error;
            }
        }
        self.sync_image_ai(ui);
    }

    /// The whole AI image job finished.
    pub fn apply_image_ai_done(
        &mut self,
        ui: &AppWindow,
        job_id: u64,
        results: &[Result<String, String>],
    ) {
        let Some(tool) = self.panel.image_ai.clone() else {
            return;
        };
        if tool.job_id != Some(job_id) {
            return;
        }
        match &tool.kind {
            ImageAiKind::Angle(angle) => self.apply_angle_done(ui, &tool, angle, results),
            ImageAiKind::Upscale(upscale) => self.apply_upscale_done(ui, &tool, upscale, results),
            ImageAiKind::Ocr | ImageAiKind::Segment => {}
        }
    }

    fn apply_angle_done(
        &mut self,
        ui: &AppWindow,
        tool: &ImageAiTool,
        angle: &AngleTool,
        results: &[Result<String, String>],
    ) {
        let Some(source) = self.doc.node(&tool.node_id).cloned() else {
            self.image_ai_close(ui);
            return;
        };
        let plan = angle_variant_plan(&angle.params, angle.count);
        let now = self.now_ms();
        let mut created = 0_usize;
        let mut failures: Vec<String> = Vec::new();
        for (variant, result) in plan.variants.iter().zip(results) {
            let url = match result {
                Ok(url) => url,
                Err(error) => {
                    failures.push(error.clone());
                    continue;
                }
            };
            let Some(surface) = crate::media::decode_surface(url) else {
                failures.push(format!("{}: the image could not be decoded", variant.label));
                continue;
            };
            if self
                .insert_ai_image_node(&source, &surface, &variant.label, created, now)
                .is_some()
            {
                created += 1;
            }
        }
        if created == 0 {
            let message = failures
                .first()
                .cloned()
                .unwrap_or_else(|| "The provider returned no image".to_string());
            self.set_image_ai_message(ui, message, true);
            return;
        }
        let total = plan.variants.len();
        self.panel.image_ai = None;
        self.note_doc_change();
        let message = if created == total {
            format!("Created {created} angle views")
        } else {
            format!("Created {created} of {total} angle views")
        };
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            message.clone(),
            Some(tool.node_id.clone()),
        );
        self.set_notice(ui, message, failures.is_empty());
        self.sync_all(ui);
    }

    fn apply_upscale_done(
        &mut self,
        ui: &AppWindow,
        tool: &ImageAiTool,
        upscale: &UpscaleTool,
        results: &[Result<String, String>],
    ) {
        let Some(source) = self.doc.node(&tool.node_id).cloned() else {
            self.image_ai_close(ui);
            return;
        };
        let Some(surface) = crate::image::tools::node_surface(&source) else {
            self.set_image_ai_message(ui, "This node has no image content".to_string(), true);
            return;
        };
        let (plan, _) = match upscale_job(&surface, &upscale.factor, &upscale.prompt) {
            Ok(job) => job,
            Err(message) => {
                self.set_image_ai_message(ui, message, true);
                return;
            }
        };
        let stitched = match stitch_ai_tiles(&plan, results) {
            Ok(stitched) => stitched,
            Err(message) => {
                self.set_image_ai_message(ui, message, true);
                return;
            }
        };
        let now = self.now_ms();
        let label = format!("AI upscale {}", upscale.factor);
        if self
            .insert_ai_image_node(&source, &stitched, &label, 0, now)
            .is_none()
        {
            self.set_image_ai_message(
                ui,
                "The upscaled image could not be stored".to_string(),
                true,
            );
            return;
        }
        self.panel.image_ai = None;
        self.note_doc_change();
        let message = format!(
            "Created a {} x {} AI-upscaled copy",
            plan.target_width, plan.target_height
        );
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            message.clone(),
            Some(tool.node_id.clone()),
        );
        self.set_notice(ui, message, false);
        self.sync_all(ui);
    }

    /// Creates a derived Image node to the right of `source`, stacked by `row`.
    fn insert_ai_image_node(
        &mut self,
        source: &CanvasNodeData,
        surface: &Surface,
        suffix: &str,
        row: usize,
        now_ms: u64,
    ) -> Option<String> {
        let (width, height) = fit_node_size(
            crate::vm::i2f(crate::vm::u2i(surface.width())),
            crate::vm::i2f(crate::vm::u2i(surface.height())),
            FIT_NODE_MAX_SIZE,
            FIT_NODE_MAX_SIZE,
        );
        let at = WorldPos::new(
            source.position.x + source.width + AI_NODE_OFFSET + width / 2.0,
            source.position.y
                + crate::vm::i2f(crate::vm::u2i(row)) * (height + AI_NODE_GAP)
                + height / 2.0,
        );
        let id = self.doc.create_node(CanvasNodeType::Image, at, now_ms);
        let node = self.doc.nodes_mut().iter_mut().find(|node| node.id == id)?;
        node.title = format!("{} {suffix}", source.title);
        node.position =
            open_canvas_core::node::Position::new(at.x - width / 2.0, at.y - height / 2.0);
        node.width = width;
        node.height = height;
        let image = crate::image::tools::node_image_entry(&format!("ai-{id}"), surface, None);
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.status = Some(CanvasNodeStatus::Success);
        meta.natural_width = Some(image.natural_width);
        meta.natural_height = Some(image.natural_height);
        meta.bytes = Some(image.bytes);
        meta.mime_type = Some(image.mime_type.clone());
        meta.content = Some(image.content.clone());
        meta.primary_image_id = Some(image.id.clone());
        meta.images = Some(vec![image]);
        meta.prompt = source
            .metadata
            .as_ref()
            .and_then(|meta| meta.prompt.clone());
        Some(id)
    }

    /// OCR finished: preview the transcription in the dialog.
    pub fn apply_ocr_done(&mut self, ui: &AppWindow, node_id: &str, text: &str) {
        if let Some(tool) = self.panel.image_ai.as_mut() {
            if tool.node_id == node_id {
                tool.text = text.to_string();
                tool.job_id = None;
                tool.done = 1;
                tool.total = 1;
                tool.error = false;
                tool.message = format!("{} characters recognised", text.chars().count());
            }
        }
        self.log(
            RuntimeLogLevel::Info,
            "image-ai",
            "OCR finished",
            Some(node_id.to_string()),
        );
        self.sync_image_ai(ui);
    }

    /// OCR failed: report the provider/parse error in the dialog.
    pub fn apply_ocr_error(&mut self, ui: &AppWindow, node_id: &str, message: &str) {
        if let Some(tool) = self.panel.image_ai.as_mut() {
            if tool.node_id == node_id {
                tool.job_id = None;
                tool.error = true;
                tool.message = message.to_string();
            }
        }
        self.log(
            RuntimeLogLevel::Error,
            "image-ai",
            message.to_string(),
            Some(node_id.to_string()),
        );
        self.sync_image_ai(ui);
    }

    /// The video prompt card's Frames / Reference toggle
    /// (`onVideoModeChange`, video-prompt-node-content.tsx:54).
    pub fn node_video_mode(&mut self, ui: &AppWindow, node_id: &str, mode: &str) {
        let next = match mode {
            "reference" => open_canvas_core::node::CanvasVideoMode::Reference,
            "frames" => open_canvas_core::node::CanvasVideoMode::Frames,
            _ => return,
        };
        let now = self.now_ms();
        let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        if node.node_type != CanvasNodeType::VideoPrompt {
            return;
        }
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        if meta.video_mode == Some(next) {
            return;
        }
        meta.video_mode = Some(next);
        self.doc.commit(now);
        self.note_doc_change();
        self.sync_rows_of(ui, &[node_id.to_string()]);
        self.sync_panel(ui);
        self.log(
            RuntimeLogLevel::Info,
            "video",
            format!("Video prompt mode set to {mode}"),
            Some(node_id.to_string()),
        );
    }

    /// The reference-slot stepper of a video-prompt card: adjusts the editable
    /// N (1..=9 reference images, default 3). Ephemeral view state, so it is not
    /// written into the document or the undo history; the slot set itself is
    /// `vm::video_slot_labels`.
    pub fn node_video_slots(&mut self, ui: &AppWindow, node_id: &str, delta: i32) {
        let Some(node) = self.doc.node(node_id) else {
            return;
        };
        if node.node_type != CanvasNodeType::VideoPrompt {
            return;
        }
        let current = crate::vm::reference_slot_count(&self.video_ref_slots, node_id);
        let next = crate::vm::clamp_reference_slots(
            i64::try_from(current)
                .unwrap_or(0)
                .saturating_add(i64::from(delta)),
        );
        if next == current {
            return;
        }
        self.video_ref_slots.insert(node_id.to_string(), next);
        self.sync_rows_of(ui, &[node_id.to_string()]);
    }

    /// Pushes the open AI image tool dialog.
    pub fn sync_image_ai(&mut self, ui: &AppWindow) {
        self.sync_image_ai_dialog(ui);
    }

    fn sync_image_ai_dialog(&mut self, ui: &AppWindow) {
        let Some(tool) = self.panel.image_ai.clone() else {
            ui.set_image_ai_open(false);
            return;
        };
        let node = self.doc.node(&tool.node_id).cloned();
        let preview = node
            .as_ref()
            .and_then(crate::image::tools::node_source_text)
            .and_then(|text| self.media.image(&text))
            .unwrap_or_default();
        let state = image_ai_state(&tool, node.as_ref(), preview);
        ui.set_image_ai_state(state);
        ui.set_image_ai_open(true);
    }
}

/// The Slint state of the open AI image dialog.
fn image_ai_state(
    tool: &ImageAiTool,
    node: Option<&CanvasNodeData>,
    preview: slint::Image,
) -> crate::ImageAiState {
    let mut state = crate::ImageAiState {
        kind: tool.kind.id().into(),
        title: node
            .map_or_else(String::new, |node| node.title.clone())
            .as_str()
            .into(),
        image: preview,
        source: String::new().into(),
        readout: String::new().into(),
        message: tool.message.as_str().into(),
        error: tool.error,
        running: tool.job_id.is_some(),
        progress: progress_ratio(tool),
        text: tool.text.as_str().into(),
        can_apply: tool.job_id.is_none(),
        apply_label: "Apply".into(),
        horizontal: 0.0,
        pitch: 0.0,
        distance: 0.0,
        wide: false,
        variants: 0,
        factor: String::new().into(),
        prompt: String::new().into(),
        tiles: 0,
        runtime_available: false,
        notice: String::new().into(),
    };
    if let Some(node) = node {
        if let Some(surface) = crate::image::tools::node_surface(node) {
            state.source = format!("{} x {}", surface.width(), surface.height()).into();
        }
    }
    match &tool.kind {
        ImageAiKind::Ocr => {
            state.apply_label = if tool.text.is_empty() {
                "Extract text".into()
            } else {
                "Create text node".into()
            };
            state.notice = "The transcription runs on the configured text model.".into();
            state.readout = if tool.job_id.is_some() {
                "Reading the image...".into()
            } else {
                state.source.clone()
            };
        }
        ImageAiKind::Angle(angle) => {
            state.horizontal = crate::vm::len(angle.params.horizontal_angle);
            state.pitch = crate::vm::len(angle.params.pitch_angle);
            state.distance = crate::vm::len(angle.params.camera_distance);
            state.wide = angle.params.wide_angle;
            state.variants = crate::vm::count_i32(angle.count);
            let plan = angle_variant_plan(&angle.params, angle.count);
            state.apply_label = format!("Generate {} views", plan.count).into();
            state.readout = plan
                .variants
                .first()
                .map_or_else(String::new, |variant| variant.label.clone())
                .into();
            state.notice =
                "Each view is a new image request from the same reference; the camera angles cycle a fixed catalogue."
                    .into();
        }
        ImageAiKind::Upscale(upscale) => {
            state.factor = upscale.factor.as_str().into();
            state.prompt = upscale.prompt.as_str().into();
            state.apply_label = "Enhance resolution".into();
            state.tiles = 1;
            if let Some(node) = node {
                if let Some(surface) = crate::image::tools::node_surface(node) {
                    let plan = upscale_plan(&surface, &upscale.factor);
                    state.tiles = crate::vm::count_i32(plan.tiles.len());
                    state.readout = format!(
                        "{} x {} -> {} x {} px · {} tile(s)",
                        plan.source_width,
                        plan.source_height,
                        plan.target_width,
                        plan.target_height,
                        plan.tiles.len()
                    )
                    .into();
                }
            }
            state.notice =
                "The image is split into bounded tiles, each enhanced by one image edit, then stitched back."
                    .into();
        }
        ImageAiKind::Segment => {
            state.apply_label = "Run segmentation".into();
            state.notice = "No local segmentation runtime is configured in this build. The request path is wired to the MobileSAM tensor/mask plumbing and fails with the honest unavailable state instead of a fabricated mask.".into();
            state.readout = state.source.clone();
        }
    }
    state
}

/// 0..1 progress of a running job (0 when nothing ran yet).
fn progress_ratio(tool: &ImageAiTool) -> f32 {
    if tool.total == 0 {
        return 0.0;
    }
    crate::vm::len(
        crate::vm::i2f(crate::vm::u2i(tool.done)) / crate::vm::i2f(crate::vm::u2i(tool.total)),
    )
}

/// Registers the AI image tool callbacks (invoked from `wire_panel`).
pub fn wire_image_ai(ui: &AppWindow, app: &std::rc::Rc<std::cell::RefCell<App>>) {
    // Static chip rows of the dialogs: the angle variant counts and the
    // upscale factors.
    let strings = |items: &[String]| {
        slint::ModelRc::new(slint::VecModel::from(
            items
                .iter()
                .map(|item| slint::SharedString::from(item.as_str()))
                .collect::<Vec<_>>(),
        ))
    };
    let variant_options: Vec<String> = ANGLE_VARIANT_CHOICES.iter().map(usize::to_string).collect();
    let factor_options: Vec<String> = UPSCALE_FACTORS
        .iter()
        .map(|item| (*item).to_string())
        .collect();
    ui.set_image_ai_variant_options(strings(&variant_options));
    ui.set_image_ai_variant_labels(strings(&variant_options));
    ui.set_image_ai_factor_options(strings(&factor_options));
    ui.set_image_ai_factor_labels(strings(&factor_options));
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_ai_close(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| app.image_ai_close(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_ai_pick(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| {
                app.image_ai_pick(&ui, key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_ai_field(move |key, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| {
                app.image_ai_field(&ui, key.as_str(), value.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_ai_apply(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| app.image_ai_apply(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_image_ai_cancel(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| app.image_ai_cancel(&ui));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_video_mode(move |id, mode| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| {
                app.node_video_mode(&ui, id.as_str(), mode.as_str());
            });
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_node_video_slots(move |id, delta| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            crate::app::with_app(&app, |app| {
                app.node_video_slots(&ui, id.as_str(), delta);
            });
        });
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::ai_service::{
        collect_video_media, effective_video_mode, request_config, text_generation_count,
        validate_video_media, video_mode_id, TEXT_GENERATION_MAX,
    };
    use open_canvas_ai::video_request::{
        openrouter_video_body_media, ReferenceMedia, VideoMode, VideoRequestMedia,
    };
    use open_canvas_core::node::{CanvasConnection, CanvasVideoMode, CanvasVideoSlots, Position};
    use serde_json::json;

    fn meta(patch: impl FnOnce(&mut CanvasNodeMetadata)) -> CanvasNodeMetadata {
        let mut metadata = CanvasNodeMetadata::default();
        patch(&mut metadata);
        metadata
    }

    fn node(
        id: &str,
        kind: CanvasNodeType,
        patch: impl FnOnce(&mut CanvasNodeMetadata),
    ) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(0.0, 0.0),
            width: 200.0,
            height: 200.0,
            metadata: Some(meta(patch)),
        }
    }

    fn image(id: &str, content: &str) -> CanvasNodeData {
        node(id, CanvasNodeType::Image, |meta| {
            meta.content = Some(content.to_string());
            meta.mime_type = Some("image/png".to_string());
        })
    }

    fn video(id: &str, content: &str, duration_ms: f64) -> CanvasNodeData {
        node(id, CanvasNodeType::Video, |meta| {
            meta.content = Some(content.to_string());
            meta.mime_type = Some("video/mp4".to_string());
            meta.duration_ms = Some(duration_ms);
        })
    }

    fn prompt(id: &str, patch: impl FnOnce(&mut CanvasNodeMetadata)) -> CanvasNodeData {
        node(id, CanvasNodeType::VideoPrompt, patch)
    }

    fn generation(id: &str, patch: impl FnOnce(&mut CanvasNodeMetadata)) -> CanvasNodeData {
        node(id, CanvasNodeType::VideoGeneration, patch)
    }

    fn wire(from: &str, to: &str) -> CanvasConnection {
        CanvasConnection {
            id: format!("{from}-{to}"),
            from_node_id: from.into(),
            to_node_id: to.into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }
    }

    fn app_with(nodes: Vec<CanvasNodeData>, connections: Vec<CanvasConnection>) -> App {
        let mut app = App::new(crate::store::Store::at(std::env::temp_dir()));
        app.doc = crate::doc::Doc::assemble(nodes, connections);
        app
    }

    fn png_url(surface: &Surface) -> String {
        let (url, _bytes) = crate::image::tools::surface_png(surface)
            .unwrap_or_else(|error| unreachable!("fixture encode: {error}"));
        url
    }

    fn small_surface(width: usize, height: usize, rgba: [u8; 4]) -> Surface {
        let mut surface = Surface::new(width, height);
        surface.fill(rgba);
        surface
    }

    /// A 2x2 red PNG data URL (the reference fixtures).
    fn red_png() -> String {
        let mut surface = Surface::new(2, 2);
        surface.fill([220, 20, 20, 255]);
        png_url(&surface)
    }

    #[test]
    fn video_slots_map_to_frames_first_and_last() {
        // `acme/hailuo-3` is an OpenRouter video model without a capability
        // entry, so the fallback accepts both keyframes.
        let app = app_with(
            vec![
                image("first", &red_png()),
                image("last", &red_png()),
                prompt("vp", |meta| {
                    meta.video_mode = Some(CanvasVideoMode::Frames);
                    meta.video_slots = Some(CanvasVideoSlots {
                        first_frame: Some("first".into()),
                        last_frame: Some("last".into()),
                        references: None,
                    });
                }),
                generation("vg", |meta| meta.model = Some("acme/hailuo-3".into())),
            ],
            vec![wire("vp", "vg")],
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let (mode, media) = collect_video_media(&app, "vg", &node);
        assert_eq!(mode, VideoMode::Frames);
        assert!(media.first_frame.is_some(), "first frame keeps its slot");
        assert!(media.last_frame.is_some(), "last frame keeps its slot");
        assert_eq!(media.images.len(), 2);
        assert_eq!(media.videos.len(), 0, "frames mode carries no videos");

        let req = request_config(&app, &node, ModelCapability::Video);
        let body = openrouter_video_body_media(&req, "a wave", &media).expect("body");
        assert_eq!(body["frame_images"][0]["frame_type"], json!("first_frame"));
        assert_eq!(body["frame_images"][1]["frame_type"], json!("last_frame"));
        assert!(body.get("input_references").is_none());
    }

    #[test]
    fn single_keyframe_model_receives_one_frame() {
        let app = app_with(
            vec![
                image("first", &red_png()),
                image("last", &red_png()),
                prompt("vp", |meta| {
                    meta.video_mode = Some(CanvasVideoMode::Frames);
                    meta.video_slots = Some(CanvasVideoSlots {
                        first_frame: Some("first".into()),
                        last_frame: Some("last".into()),
                        references: None,
                    });
                }),
                generation("vg", |meta| meta.model = Some("minimax/hailuo-3".into())),
            ],
            vec![wire("vp", "vg")],
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let (mode, media) = collect_video_media(&app, "vg", &node);
        let req = request_config(&app, &node, ModelCapability::Video);
        let body = openrouter_video_body_media(&req, "a wave", &media).expect("body");
        let frames = body["frame_images"].as_array().cloned().unwrap_or_default();
        assert_eq!(mode, VideoMode::Frames);
        assert_eq!(frames.len(), 1, "the model accepts a single keyframe");
        assert_eq!(frames[0]["frame_type"], json!("first_frame"));
    }

    #[test]
    fn reference_mode_maps_references_by_kind() {
        let app = app_with(
            vec![
                image("img", &red_png()),
                video("vid", &red_png(), 3_000.0),
                prompt("vp", |meta| {
                    meta.video_mode = Some(CanvasVideoMode::Reference);
                    meta.video_slots = Some(CanvasVideoSlots {
                        first_frame: None,
                        last_frame: None,
                        references: Some(vec!["img".into(), "vid".into()]),
                    });
                }),
                generation("vg", |meta| meta.model = Some("minimax/hailuo-3".into())),
            ],
            vec![wire("vp", "vg")],
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let (mode, media) = collect_video_media(&app, "vg", &node);
        assert_eq!(mode, VideoMode::Reference);
        assert_eq!(media.images.len(), 1);
        assert_eq!(media.videos.len(), 1);
        assert_eq!(media.videos[0].duration_ms, Some(3_000));
        assert!(
            media.first_frame.is_none(),
            "reference mode keeps no frames"
        );

        // `ai_start` writes the effective mode into the request config.
        let mut req = request_config(&app, &node, ModelCapability::Video);
        req.video_mode = mode.as_str().to_string();
        let body = openrouter_video_body_media(&req, "a wave", &media).expect("body");
        assert!(body.get("frame_images").is_none());
        assert_eq!(body["input_references"][0]["type"], json!("image_url"));
        assert_eq!(body["input_references"][1]["type"], json!("video_url"));
    }

    #[test]
    fn reference_mode_deduplicates_slot_and_wired_nodes() {
        let app = app_with(
            vec![
                image("img", &red_png()),
                prompt("vp", |meta| {
                    meta.video_mode = Some(CanvasVideoMode::Reference);
                    meta.video_slots = Some(CanvasVideoSlots {
                        first_frame: None,
                        last_frame: None,
                        references: Some(vec!["img".into()]),
                    });
                }),
                generation("vg", |meta| meta.model = Some("minimax/hailuo-3".into())),
            ],
            vec![wire("img", "vg"), wire("vp", "vg")],
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let (mode, media) = collect_video_media(&app, "vg", &node);
        assert_eq!(mode, VideoMode::Reference);
        assert_eq!(media.images.len(), 1, "a wired slot node appears once");
    }

    #[test]
    fn illegal_reference_combinations_are_rejected_before_sending() {
        let app = app_with(
            vec![generation("vg", |meta| {
                meta.model = Some("minimax/hailuo-3".into());
            })],
            Vec::new(),
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let req = request_config(&app, &node, ModelCapability::Video);

        // Too many images for the provider count.
        let too_many = VideoRequestMedia {
            images: (0..10)
                .map(|index| ReferenceMedia::new(format!("i{index}.png"), "image/png", vec![1]))
                .collect(),
            ..VideoRequestMedia::default()
        };
        let error = validate_video_media(&req, &too_many).expect_err("count limit");
        assert!(error.contains("At most 9"), "got {error}");

        // Audio alone cannot be a reference.
        let audio_only = VideoRequestMedia {
            audios: vec![ReferenceMedia::new("a.mp3", "audio/mpeg", vec![1])],
            ..VideoRequestMedia::default()
        };
        let error = validate_video_media(&req, &audio_only).expect_err("audio needs visual");
        assert!(error.contains("alone"), "got {error}");

        // A reference outside the 2-15s window is rejected.
        let short = VideoRequestMedia {
            videos: vec![ReferenceMedia::new("v.mp4", "video/mp4", vec![1]).with_duration_ms(500)],
            ..VideoRequestMedia::default()
        };
        let error = validate_video_media(&req, &short).expect_err("duration window");
        assert!(error.contains("2-15"), "got {error}");

        // A per-kind byte ceiling is rejected.
        let oversized = VideoRequestMedia {
            audios: vec![ReferenceMedia::new(
                "a.mp3",
                "audio/mpeg",
                vec![0; open_canvas_ai::AUDIO_REFERENCE_MAX_BYTES + 1],
            )],
            images: vec![ReferenceMedia::new("i.png", "image/png", vec![1])],
            ..VideoRequestMedia::default()
        };
        let error = validate_video_media(&req, &oversized).expect_err("byte ceiling");
        assert!(error.contains("15MB"), "got {error}");

        // The same media passes for a model that accepts the kinds.
        let ok_media = VideoRequestMedia {
            images: vec![ReferenceMedia::new("i.png", "image/png", vec![1])],
            videos: vec![ReferenceMedia::new("v.mp4", "video/mp4", vec![1]).with_duration_ms(3_000)],
            ..VideoRequestMedia::default()
        };
        assert!(validate_video_media(&req, &ok_media).is_ok());
    }

    #[test]
    fn effective_video_mode_prefers_prompt_then_node_then_config() {
        let node = generation("vg", |meta| {
            meta.video_mode = Some(CanvasVideoMode::Reference);
        });
        let app = app_with(vec![node.clone()], Vec::new());
        assert_eq!(
            effective_video_mode(&app, &node, None),
            VideoMode::Reference
        );

        let frames_node = generation("vg", |meta| meta.video_mode = Some(CanvasVideoMode::Frames));
        let app = app_with(vec![frames_node.clone()], Vec::new());
        assert_eq!(
            effective_video_mode(&app, &frames_node, None),
            VideoMode::Frames
        );

        let plain = generation("vg", |_| {});
        let reference_prompt = prompt("vp", |meta| {
            meta.video_mode = Some(CanvasVideoMode::Reference);
        });
        let app = app_with(vec![plain.clone()], Vec::new());
        assert_eq!(
            effective_video_mode(&app, &plain, Some(&reference_prompt)),
            VideoMode::Reference,
            "the prompt's setting wins over the node's"
        );

        let mut app = app_with(vec![plain.clone()], Vec::new());
        app.config.video_mode = "reference".to_string();
        assert_eq!(
            effective_video_mode(&app, &plain, None),
            VideoMode::Reference
        );
    }

    #[test]
    fn request_config_applies_the_three_override_chains() {
        let app = app_with(
            vec![node("text", CanvasNodeType::Text, |meta| {
                meta.count = Some(4);
                meta.text_count = Some(2);
                meta.reasoning_effort = Some(open_canvas_core::node::CanvasReasoningEffort::High);
                meta.model = Some("deepseek-v4-pro".into());
            })],
            Vec::new(),
        );
        let node = app.doc.node("text").cloned().expect("fixture node");
        let req = request_config(&app, &node, ModelCapability::Text);
        assert_eq!(
            req.reasoning_effort,
            open_canvas_ai::ReasoningEffort::High,
            "the panel's reasoning effort reaches the request"
        );
        assert_eq!(req.count, 4, "metadata.count reaches the request for text");
        assert_eq!(
            req.model,
            open_canvas_ai::config::DEEPSEEK_TEXT_MODEL,
            "the locked text model wins over a stale metadata.model"
        );
    }

    #[test]
    fn request_config_passes_openrouter_video_models_through() {
        // A hailuo model that is not registered on any channel: the web's
        // `isOpenRouterVideoModel(nodeModel)` special case keeps it unresolved.
        let mut app = app_with(
            vec![generation("vg", |meta| {
                meta.model = Some("acme/hailuo-3".into());
            })],
            Vec::new(),
        );
        app.config.video_model = String::new();
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let req = request_config(&app, &node, ModelCapability::Video);
        assert_eq!(req.model, "acme/hailuo-3");
        assert_eq!(req.base_url, open_canvas_ai::OPENROUTER_BASE_URL);
    }

    #[test]
    fn request_config_applies_the_video_mode_and_count() {
        let app = app_with(
            vec![generation("vg", |meta| {
                meta.video_mode = Some(CanvasVideoMode::Reference);
                meta.count = Some(2);
                meta.model = Some("minimax/hailuo-3".into());
            })],
            Vec::new(),
        );
        let node = app.doc.node("vg").cloned().expect("fixture node");
        let req = request_config(&app, &node, ModelCapability::Video);
        assert_eq!(req.video_mode, "reference");
        assert_eq!(req.count, 2);
    }

    #[test]
    fn text_generation_count_prefers_text_count_then_count() {
        let both = node("t", CanvasNodeType::Text, |meta| {
            meta.count = Some(3);
            meta.text_count = Some(2);
        });
        assert_eq!(text_generation_count(&both), 2);
        let count_only = node("t", CanvasNodeType::Text, |meta| meta.count = Some(3));
        assert_eq!(text_generation_count(&count_only), 3);
        let none = node("t", CanvasNodeType::Text, |_| {});
        assert_eq!(text_generation_count(&none), 1);
        let huge = node("t", CanvasNodeType::Text, |meta| meta.text_count = Some(99));
        assert_eq!(text_generation_count(&huge), TEXT_GENERATION_MAX);
    }

    #[test]
    fn ocr_request_carries_the_prompt_and_image_url() {
        let image = image("i", &red_png());
        let request = ocr_request_for(&image).expect("request");
        assert_eq!(request.prompt, OCR_PROMPT);
        assert_eq!(request.image_urls(), vec![red_png()]);
        assert_eq!(request.messages[0].role, "user");
        assert!(ocr_request_for(&node("empty", CanvasNodeType::Image, |_| {})).is_none());
    }

    #[test]
    fn ocr_parse_strips_fences_and_rejects_empty() {
        assert_eq!(
            ocr_text("```\nHello world\n```").expect("parse"),
            "Hello world"
        );
        assert_eq!(
            ocr_text(r#"{"text":"Hi","blocks":[]}"#).expect("parse"),
            "Hi"
        );
        assert!(ocr_text("   ").is_err());
    }

    #[test]
    fn angle_requests_are_deterministic_and_variant_specific() {
        let reference = red_png();
        let plan = angle_variant_plan(&AngleParams::default(), 4);
        let first = angle_requests(&reference, &plan);
        let second = angle_requests(&reference, &plan);
        assert_eq!(first, second, "the same input yields the same requests");
        assert_eq!(first.len(), 4);
        assert!(first.iter().all(|request| request.reference == reference));
        for pair in first.windows(2) {
            assert_ne!(
                pair[0].prompt, pair[1].prompt,
                "each view has its own prompt"
            );
        }
        assert!(first[0].prompt.contains("AI multi-angle"));
    }

    #[test]
    fn upscale_job_tiles_and_stitches_back() {
        // 2048x1024 at 2x targets 4096x2048: two 1024px tiles, each one AI call.
        let mut source = Surface::new(2048, 1024);
        source.fill([10, 20, 30, 255]);
        let (plan, requests) = upscale_job(&source, "2x", "keep it sharp").expect("job");
        assert_eq!((plan.target_width, plan.target_height), (4096, 2048));
        assert_eq!(plan.tiles.len(), 2);
        assert_eq!(requests.len(), plan.tiles.len());
        assert!(requests[0]
            .prompt
            .starts_with("Enhance the image resolution to 2x."));
        assert!(requests[0].prompt.contains("keep it sharp"));
        // Each reference decodes to its own source tile.
        for (request, tile) in requests.iter().zip(&plan.tiles) {
            let tile_surface = crate::media::decode_surface(&request.reference).expect("tile");
            assert_eq!(
                (tile_surface.width(), tile_surface.height()),
                (tile.source.width, tile.source.height)
            );
        }
        // The provider's (locally resampled) tiles stitch to the target size.
        let results: Vec<Result<String, String>> = plan
            .tiles
            .iter()
            .map(|tile| {
                let cropped = crop_surface(&source, tile.source);
                let resized = cropped.resample_bilinear(tile.target.width, tile.target.height);
                Ok(png_url(&resized))
            })
            .collect();
        let stitched = stitch_ai_tiles(&plan, &results).expect("stitch");
        assert_eq!((stitched.width(), stitched.height()), (4096, 2048));
        assert_eq!(stitched.get(2047, 1023), [10, 20, 30, 255]);
        assert_eq!(stitched.get(2048, 1024), [10, 20, 30, 255]);

        // A failed tile fails the whole stitch.
        let mut failed = results.clone();
        failed[0] = Err("provider error".to_string());
        let error = stitch_ai_tiles(&plan, &failed).expect_err("failed tile");
        assert_eq!(error, "provider error");
        // A short result list is refused too.
        let error = stitch_ai_tiles(&plan, &results[..1]).expect_err("short list");
        assert!(error.contains("tiles"));
    }

    #[test]
    fn segment_path_reports_the_unavailable_runtime() {
        let surface = small_surface(4, 4, [1, 2, 3, 255]);
        let message = segment_unavailable(&surface);
        assert!(
            message.contains("No local segmentation runtime"),
            "got {message}"
        );
        assert!(message.contains("unavailable"), "got {message}");
        assert!(
            message.contains("instead of a fabricated mask"),
            "the state is honest, got {message}"
        );
    }

    #[test]
    fn video_mode_id_maps_the_metadata_enum() {
        assert_eq!(video_mode_id(CanvasVideoMode::Frames), "frames");
        assert_eq!(video_mode_id(CanvasVideoMode::Reference), "reference");
        assert_eq!(upscale_factor("4x"), 4);
        assert_eq!(upscale_factor("2x"), 2);
        assert_eq!(upscale_factor("nonsense"), 2);
    }
}

#[cfg(test)]
#[path = "ai_render_tests.rs"]
mod render_tests;
