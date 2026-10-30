//! AI generation runtime: a background tokio runtime that performs the HTTP
//! requests and a `std::sync::mpsc` channel the Slint tick drains on the UI
//! thread. Nothing here borrows UI state across threads, so the event loop never
//! blocks: a task only owns owned data (config clone, prompt, references) and a
//! channel `Sender`; results are applied by [`App::poll_ai`] on the main thread.
// allow: SIZE_OK - service lifecycle + generation orchestration for three modes;
// splitting would scatter the spawn/apply seam across modules.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use open_canvas_ai::client::{build_client, Cancel};
use open_canvas_ai::config::{
    resolve_model_for_capability, resolve_model_request_config, resolve_model_script,
    ModelCapability, ModelRequestConfig, ReasoningEffort,
};
use open_canvas_ai::error::AiError;
use open_canvas_ai::generation_cost::{format_usd, GenerationCostUnit};
use open_canvas_ai::image::{request_edit, request_generation, request_image_question, ImageUsage};
use open_canvas_ai::now_millis;
use open_canvas_ai::plugin_video::{create_plugin_video_task, plugin_provider_from_script};
use open_canvas_ai::response::ResponseUsage;
use open_canvas_ai::retry::{run_with_retry, GENERATION_MAX_ATTEMPTS, GENERATION_RETRY_DELAY_MS};
use open_canvas_ai::runtime_log::RuntimeLogLevel;
use open_canvas_ai::video::{
    create_video_task_with_media, is_openrouter_video_model, wait_for_video_task, VideoResult,
};
use open_canvas_ai::video_request::{
    assert_reference_seconds, assert_reference_size, normalize_video_mode,
    validate_reference_kinds, validate_reference_limits, video_model_capability, ReferenceMedia,
    VideoFrameType, VideoMode, VideoReferenceKind, VideoRequestMedia, AUDIO_REFERENCE_MAX_BYTES,
    VIDEO_REFERENCE_MAX_BYTES,
};
use open_canvas_ai::Client;
use open_canvas_core::constants::get_node_spec;
use open_canvas_core::node::{
    CanvasAgentStep, CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata, CanvasNodeStatus,
    CanvasNodeType, CanvasNodeUsage, CanvasReasoningEffort, CanvasVideoMode,
    CanvasVideoTaskProvider,
};

use crate::app::App;
use crate::cost::{CostHint, GenerationCharge};
use crate::i18n;
use crate::AppWindow;

/// The panel's text-generation count window (`getGenerationCount`, 1..=15).
pub const TEXT_GENERATION_MAX: i64 = 15;

/// The number of text alternatives one generation runs (`getGenerationCount`):
/// the panel's text count (`metadata.textCount`), else the per-node count
/// (`metadata.count`, the web's `buildGenerationConfig` fallback), clamped
/// 1..=15.
pub fn text_generation_count(node: &CanvasNodeData) -> i64 {
    let meta = node.metadata.as_ref();
    meta.and_then(|meta| meta.text_count)
        .or_else(|| meta.and_then(|meta| meta.count))
        .unwrap_or(1)
        .clamp(1, TEXT_GENERATION_MAX)
}

/// The number of images one image generation produces (`getGenerationCount`,
/// 1..=15): the request's resolved count first, then the node's metadata.
pub(crate) fn image_generation_count(request_count: i64) -> usize {
    usize::try_from(request_count.clamp(1, 15)).unwrap_or(1)
}

/// Which request a node's Generate button issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenMode {
    Image,
    Text,
    Video,
}

impl GenMode {
    fn capability(self) -> ModelCapability {
        match self {
            Self::Image => ModelCapability::Image,
            Self::Text => ModelCapability::Text,
            Self::Video => ModelCapability::Video,
        }
    }
}

/// What a finished generation produced, plus what its response said about the
/// cost: the resolved model, the provider's usage block and (for the image
/// endpoint) the `x-generation-id` header a `/generation` lookup needs.
#[derive(Debug, Clone, PartialEq)]
pub enum AiOutcome {
    Image {
        urls: Vec<String>,
        model: String,
        usage: ImageUsage,
        generation_id: Option<String>,
        has_reference: bool,
    },
    Text {
        /// The primary text (the first of `texts`), kept for the node content.
        text: String,
        /// Every generated alternative (1..=15): the panel's generation count.
        texts: Vec<String>,
        model: String,
        usage: Option<ResponseUsage>,
        has_reference: bool,
    },
    Video {
        task_id: String,
        url: Option<String>,
        model: String,
        cost_usd: Option<f64>,
        /// Which backend owned the task (`metadata.videoTaskProvider`).
        provider: CanvasVideoTaskProvider,
    },
}

impl AiOutcome {
    /// The model the request actually ran with (the billed model, which may
    /// come from the config rather than the node's metadata).
    fn model(&self) -> &str {
        match self {
            Self::Image { model, .. } | Self::Text { model, .. } | Self::Video { model, .. } => {
                model
            }
        }
    }

    /// The client-side estimate input: the token usage the response reported
    /// (`usage.cost` aside), else nothing.
    fn cost_hint(&self) -> CostHint {
        match self {
            Self::Image {
                usage,
                has_reference,
                ..
            } => CostHint::Tokens(usage.token_usage(*has_reference)),
            Self::Text {
                usage,
                has_reference,
                ..
            } => match usage {
                Some(usage) => CostHint::Tokens(usage.token_usage(*has_reference)),
                None => CostHint::None,
            },
            Self::Video { cost_usd, .. } => match cost_usd {
                Some(usd) => CostHint::Api(*usd),
                None => CostHint::None,
            },
        }
    }

    /// The generation id to look the billed amount up by: only the image path
    /// reports one, and only when the response carried no `usage.cost` (the
    /// web's `resolveImageCost` order).
    fn lookup_id(&self) -> Option<&str> {
        match self {
            Self::Image {
                usage,
                generation_id,
                ..
            } if usage.cost.is_none() => generation_id.as_deref(),
            _ => None,
        }
    }

    /// The provider's generation id, kept on the produced node for a later
    /// `/generation` cost lookup.
    fn generation_id(&self) -> Option<&str> {
        match self {
            Self::Image { generation_id, .. } => generation_id.as_deref(),
            _ => None,
        }
    }

    /// The usage figures the produced node keeps in its metadata.
    fn usage(&self) -> Option<CanvasNodeUsage> {
        match self {
            Self::Image { usage, .. } => Some(CanvasNodeUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cost: usage.cost,
            }),
            Self::Text {
                usage: Some(usage), ..
            } => Some(CanvasNodeUsage {
                prompt_tokens: usage.input_tokens.map(|tokens| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "token counts stay far below 2^53"
                    )]
                    let tokens = tokens as f64;
                    tokens
                }),
                completion_tokens: usage.output_tokens.map(|tokens| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "token counts stay far below 2^53"
                    )]
                    let tokens = tokens as f64;
                    tokens
                }),
                total_tokens: usage.total_tokens.map(|tokens| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "token counts stay far below 2^53"
                    )]
                    let tokens = tokens as f64;
                    tokens
                }),
                cost: None,
            }),
            _ => None,
        }
    }
}

/// A message from a background task back to the UI thread.
#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    Delta {
        node_id: String,
        text: String,
    },
    Done {
        node_id: String,
        outcome: AiOutcome,
    },
    Cancelled {
        node_id: String,
    },
    Error {
        node_id: String,
        message: String,
    },
    /// `fetch_generation_cost` result for the Cost tab's lookup: the billed
    /// amount, or the provider/parse error text.
    CostFetched {
        model: String,
        unit: GenerationCostUnit,
        quantity: f64,
        result: Result<f64, String>,
    },
    /// `fetch_generation_cost` result for a finished generation: the record is
    /// written with the looked-up amount, or with the charge's fallback when the
    /// lookup had nothing to report.
    GenerationCost {
        node_id: String,
        charge: GenerationCharge,
        result: Result<f64, String>,
    },
    /// OCR (vision) result for an image node.
    OcrDone {
        node_id: String,
        text: String,
    },
    OcrError {
        node_id: String,
        message: String,
    },
    /// One finished AI image request of a job (multi-angle variant / upscale
    /// tile): the data URL, or the error text.
    ImageAiItem {
        job_id: u64,
        index: usize,
        result: Result<String, String>,
    },
    /// A whole AI image job finished: every item's result in order.
    ImageAiDone {
        job_id: u64,
        results: Vec<Result<String, String>>,
    },
}

pub(crate) struct TaskHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) cancel: Cancel,
}

/// One generation request: everything a background task needs, owned so it can
/// move across the thread boundary. Grouping the inputs keeps the per-mode
/// helpers within the small parameter budget.
struct Job {
    node_id: String,
    mode: GenMode,
    req: ModelRequestConfig,
    prompt: String,
    references: Vec<String>,
    /// Video reference media (frame slots + reference media); empty otherwise.
    video: VideoRequestMedia,
    /// Declarative plugin provider resolved from the model's script, when the
    /// selected video model carries one.
    plugin: Option<open_canvas_ai::PluginVideoProvider>,
    /// Where inline video bytes are saved (`file:` node content): the exports
    /// path computed on the UI thread.
    video_save: Option<std::path::PathBuf>,
    /// How many text alternatives the text path runs (`text_generation_count`).
    text_count: i64,
}

/// Owns the tokio runtime, the shared HTTP client and the event channel. `None`
/// when the runtime or client could not be built (the UI then reports it).
pub struct AiService {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) client: Client,
    pub(crate) tx: Sender<AiEvent>,
    rx: Receiver<AiEvent>,
    tasks: HashMap<String, TaskHandle>,
    /// Generation node id -> the placeholder media node its result lands in
    /// (an Image or Image Stack card, placed when the request starts).
    outputs: HashMap<String, String>,
    /// Background jobs of the AI image tools, keyed by job id.
    pub(crate) jobs: HashMap<u64, TaskHandle>,
    /// Allocator of those job ids.
    pub(crate) job_seq: u64,
}

impl AiService {
    pub fn new() -> Option<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .ok()?;
        let client = build_client().ok()?;
        let (tx, rx) = mpsc::channel();
        Some(Self {
            runtime,
            client,
            tx,
            rx,
            tasks: HashMap::new(),
            outputs: HashMap::new(),
            jobs: HashMap::new(),
            job_seq: 0,
        })
    }

    /// The media node a generation node's result was last placed in.
    fn output_node(&self, generation_id: &str) -> Option<&str> {
        self.outputs.get(generation_id).map(String::as_str)
    }

    /// The generation that owns a produced media node (its card's Retry reruns
    /// that generation instead of merely clearing the media's error).
    pub(crate) fn generation_for_output(&self, output_id: &str) -> Option<&str> {
        self.outputs
            .iter()
            .find(|(_, id)| id.as_str() == output_id)
            .map(|(generation, _)| generation.as_str())
    }

    fn remember_output(&mut self, generation_id: &str, output_id: &str) {
        self.outputs
            .insert(generation_id.to_string(), output_id.to_string());
    }

    /// Cost tab: resolves a generation id's billed amount through
    /// `fetch_generation_cost` on the shared runtime; the result arrives as
    /// [`AiEvent::CostFetched`] and is applied by `poll_ai`.
    pub fn spawn_cost_lookup(
        &self,
        req: ModelRequestConfig,
        generation_id: String,
        capability: ModelCapability,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let cancel = Cancel::new();
        let model = req.model.clone();
        let unit = match capability {
            ModelCapability::Video => GenerationCostUnit::VideoSecond,
            _ => GenerationCostUnit::Image,
        };
        self.runtime.spawn(async move {
            let result =
                open_canvas_ai::fetch_generation_cost(&client, &req, &generation_id, &cancel)
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|cost| {
                        cost.resolved_usd().ok_or_else(|| {
                            "the API returned no cost for this generation".to_string()
                        })
                    });
            let _sent = tx.send(AiEvent::CostFetched {
                model,
                unit,
                quantity: 1.0,
                result,
            });
        });
    }

    /// A finished generation's `/generation` lookup (the `x-generation-id` the
    /// image response carried): resolves the billed amount in the background and
    /// hands it back as [`AiEvent::GenerationCost`]; the record lands either
    /// way, with the charge's fallback when the lookup has nothing to report.
    pub fn spawn_generation_cost_lookup(
        &self,
        req: ModelRequestConfig,
        node_id: String,
        generation_id: String,
        charge: GenerationCharge,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let cancel = Cancel::new();
        self.runtime.spawn(async move {
            let result =
                open_canvas_ai::fetch_generation_cost(&client, &req, &generation_id, &cancel)
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|cost| {
                        cost.resolved_usd().ok_or_else(|| {
                            "the API returned no cost for this generation".to_string()
                        })
                    });
            let _sent = tx.send(AiEvent::GenerationCost {
                node_id,
                charge,
                result,
            });
        });
    }
}

pub(crate) fn gen_mode(_app: &App, node: &CanvasNodeData) -> Option<GenMode> {
    match node.node_type {
        CanvasNodeType::Text => Some(GenMode::Text),
        CanvasNodeType::VideoPrompt | CanvasNodeType::VideoGeneration => Some(GenMode::Video),
        CanvasNodeType::ImageGeneration | CanvasNodeType::ImageAttempt => Some(GenMode::Image),
        _ => None,
    }
}

/// The prompt card wired into a generation node: the first connected `Prompt`
/// wins, else the first `VideoPrompt` (the same order `source_content` used).
fn connected_prompt_node<'a>(app: &'a App, node_id: &str) -> Option<&'a CanvasNodeData> {
    let mut video = None;
    for connection in app.doc.connections() {
        if connection.to_node_id != node_id {
            continue;
        }
        let Some(source) = app.doc.node(&connection.from_node_id) else {
            continue;
        };
        match source.node_type {
            CanvasNodeType::Prompt => return Some(source),
            CanvasNodeType::VideoPrompt if video.is_none() => video = Some(source),
            _ => {}
        }
    }
    video
}

/// The stored bitmap of an image reference node: an Image's `content`, or an
/// Image Stack's primary/first image entry (the stack itself has no `content`).
fn image_reference_content(source: &CanvasNodeData) -> Option<String> {
    let meta = source.metadata.as_ref()?;
    if let Some(content) = meta.content.clone().filter(|text| !text.is_empty()) {
        return Some(content);
    }
    let images = meta.images.as_ref()?;
    let primary = meta.primary_image_id.as_deref();
    images
        .iter()
        .find(|image| primary.is_some_and(|id| image.id == id))
        .or_else(|| images.first())
        .map(|image| image.content.clone())
        .filter(|text| !text.is_empty())
}

/// Appends one image/image-stack node's stored content as a reference, keyed by
/// node id so a direct wire and a prompt wire cannot send the same node twice.
fn push_image_reference(
    app: &App,
    references: &mut Vec<String>,
    seen: &mut Vec<String>,
    source_id: &str,
) {
    if seen.iter().any(|id| id == source_id) {
        return;
    }
    let Some(source) = app.doc.node(source_id) else {
        return;
    };
    if !matches!(
        source.node_type,
        CanvasNodeType::Image | CanvasNodeType::ImageStack
    ) {
        return;
    }
    let Some(content) = image_reference_content(source) else {
        return;
    };
    seen.push(source_id.to_string());
    references.push(content);
}

/// The prompt and reference data-URLs a node generates from: the live composer
/// text (composer nodes), else a connected prompt node, else its own metadata.
/// References are the prompt node's own incoming image/image-stack wires first,
/// then the generation node's, de-duplicated by node id (the web's
/// `buildNodeGenerationInputs(promptNode)`).
fn collect_inputs(app: &App, node_id: &str, node: &CanvasNodeData) -> (String, Vec<String>) {
    if node.node_type == CanvasNodeType::ImageAttempt {
        let prompt = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.prompt.clone())
            .unwrap_or_default();
        let mut references = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for connection in app.doc.connections() {
            if connection.to_node_id == node_id {
                push_image_reference(app, &mut references, &mut seen, &connection.from_node_id);
            }
        }
        return (prompt, references);
    }
    let prompt_node = connected_prompt_node(app, node_id);
    let prompt =
        if app.panel.node.as_deref() == Some(node_id) && !app.panel.prompt.trim().is_empty() {
            app.panel.prompt.clone()
        } else {
            prompt_node
                .and_then(|prompt| {
                    prompt.metadata.as_ref().and_then(|meta| {
                        meta.prompt
                            .clone()
                            .or_else(|| meta.content.clone())
                            .filter(|text| !text.trim().is_empty())
                    })
                })
                .or_else(|| node.metadata.as_ref().and_then(|meta| meta.prompt.clone()))
                .unwrap_or_default()
        };
    let mut references = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    if let Some(prompt) = prompt_node {
        for connection in app.doc.connections() {
            if connection.to_node_id == prompt.id {
                push_image_reference(app, &mut references, &mut seen, &connection.from_node_id);
            }
        }
    }
    for connection in app.doc.connections() {
        if connection.to_node_id == node_id {
            push_image_reference(app, &mut references, &mut seen, &connection.from_node_id);
        }
    }
    (prompt, references)
}

/// A node's stored media as reference bytes: `data:` URLs decode, `file:` video
/// nodes read from disk (the desktop's imported-video convention). The kind
/// comes from the node type; only Image and Video nodes are references.
fn node_reference_media(node: &CanvasNodeData) -> Option<(VideoReferenceKind, ReferenceMedia)> {
    let meta = node.metadata.as_ref()?;
    let content = meta
        .content
        .as_deref()
        .filter(|content| !content.is_empty())?;
    let kind = match node.node_type {
        CanvasNodeType::Image => VideoReferenceKind::Image,
        CanvasNodeType::Video => VideoReferenceKind::Video,
        _ => return None,
    };
    let bytes = match content.strip_prefix("file:") {
        Some(path) => std::fs::read(path).ok()?,
        None => crate::image::tools::data_url_bytes(content)?,
    };
    if bytes.is_empty() {
        return None;
    }
    let (fallback_mime, extension) = match kind {
        VideoReferenceKind::Image => ("image/png", "png"),
        VideoReferenceKind::Video => ("video/mp4", "mp4"),
        VideoReferenceKind::Audio => ("audio/mpeg", "mp3"),
    };
    let mime = meta
        .mime_type
        .clone()
        .filter(|mime| !mime.trim().is_empty())
        .unwrap_or_else(|| fallback_mime.to_string());
    let mut media = ReferenceMedia::new(format!("{}.{extension}", node.title), mime, bytes);
    if kind == VideoReferenceKind::Video {
        media.duration_ms = duration_ms_of(meta.duration_ms);
    }
    Some((kind, media))
}

/// A finite positive millisecond duration as `u64` (the reference window check
/// consumes it; a bogus value is simply absent).
fn duration_ms_of(value: Option<f64>) -> Option<u64> {
    let value = value.filter(|value| value.is_finite() && *value > 0.0)?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is finite and positive; media durations stay far below u64::MAX"
    )]
    let millis = value as u64;
    Some(millis)
}

/// The effective video mode of a generation node: the prompt node's setting,
/// else the generation node's, else the config's (`project.tsx:630`).
pub(crate) fn effective_video_mode(
    app: &App,
    node: &CanvasNodeData,
    prompt: Option<&CanvasNodeData>,
) -> VideoMode {
    let wire = prompt
        .and_then(|prompt| prompt.metadata.as_ref())
        .and_then(|meta| meta.video_mode)
        .or_else(|| node.metadata.as_ref().and_then(|meta| meta.video_mode))
        .map_or_else(
            || app.config.video_mode.clone(),
            |mode| video_mode_id(mode).to_string(),
        );
    normalize_video_mode(&wire)
}

/// The video reference media of a generation node: the web's
/// `readVideoSlotReferences` (`canvas-node-generation.ts:173`) merged with the
/// connected resource nodes. In frames mode only images travel (a frame request
/// is image-only); in reference mode the slot references keep their kind and
/// the connected images/videos follow, de-duplicated by node id. The first
/// value is the effective mode (the prompt's setting wins, then the node's,
/// then the config), which the caller writes into the request config.
pub(crate) fn collect_video_media(
    app: &App,
    node_id: &str,
    node: &CanvasNodeData,
) -> (VideoMode, VideoRequestMedia) {
    let prompt = app.doc.connections().iter().find_map(|connection| {
        if connection.to_node_id != node_id {
            return None;
        }
        let source = app.doc.node(&connection.from_node_id)?;
        (source.node_type == CanvasNodeType::VideoPrompt).then(|| source.clone())
    });
    let mode = effective_video_mode(app, node, prompt.as_ref());
    let mut media = VideoRequestMedia::default();
    let mut seen: Vec<String> = Vec::new();
    let slots = prompt
        .as_ref()
        .and_then(|prompt| prompt.metadata.as_ref())
        .and_then(|meta| meta.video_slots.clone());
    if let Some(slots) = slots {
        if mode == VideoMode::Reference {
            for id in slots.references.iter().flatten() {
                push_reference(app, &mut media, &mut seen, id);
            }
        } else {
            let frame_slots = [
                (slots.first_frame.clone(), VideoFrameType::First),
                (slots.last_frame.clone(), VideoFrameType::Last),
            ];
            for (id, frame_type) in frame_slots {
                let Some(source) = id.as_deref().and_then(|id| app.doc.node(id)) else {
                    continue;
                };
                let Some((VideoReferenceKind::Image, item)) = node_reference_media(source) else {
                    continue;
                };
                match frame_type {
                    VideoFrameType::First => media.first_frame = Some(item.clone()),
                    VideoFrameType::Last => media.last_frame = Some(item.clone()),
                }
                seen.push(source.id.clone());
                media.images.push(item);
            }
        }
    }
    // Connected resource nodes: the generation node's own wires plus the video
    // prompt's incoming wires (the web reads `buildNodeGenerationInputs(promptNode)`).
    let prompt_id = prompt.as_ref().map(|prompt| prompt.id.as_str());
    for connection in app.doc.connections() {
        if connection.to_node_id != node_id && Some(connection.to_node_id.as_str()) != prompt_id {
            continue;
        }
        let Some(source) = app.doc.node(&connection.from_node_id) else {
            continue;
        };
        if seen.contains(&source.id) {
            continue;
        }
        if mode == VideoMode::Frames {
            // A frame request is image-only: connected videos never join it.
            if let Some((VideoReferenceKind::Image, item)) = node_reference_media(source) {
                seen.push(source.id.clone());
                media.images.push(item);
            }
        } else {
            push_reference(app, &mut media, &mut seen, &connection.from_node_id);
        }
    }
    (mode, media)
}

/// Appends a node id's reference media by its kind, skipping nodes already
/// consumed and kinds the video request has no slot for.
fn push_reference(app: &App, media: &mut VideoRequestMedia, seen: &mut Vec<String>, id: &str) {
    if seen.iter().any(|existing| existing == id) {
        return;
    }
    let Some(source) = app.doc.node(id) else {
        return;
    };
    let Some((kind, item)) = node_reference_media(source) else {
        return;
    };
    seen.push(id.to_string());
    match kind {
        VideoReferenceKind::Image => media.images.push(item),
        VideoReferenceKind::Video => media.videos.push(item),
        VideoReferenceKind::Audio => media.audios.push(item),
    }
}

/// The web's pre-flight reference validation (`video.ts:327-349` + the provider
/// counts): every failure is surfaced on the node before a request is sent.
pub(crate) fn validate_video_media(
    req: &ModelRequestConfig,
    media: &VideoRequestMedia,
) -> Result<(), String> {
    validate_reference_limits(media).map_err(|error| error.to_string())?;
    let capability = video_model_capability(&req.model);
    validate_reference_kinds(&capability, media).map_err(|error| error.to_string())?;
    for video in &media.videos {
        assert_reference_seconds(video, VideoReferenceKind::Video)
            .map_err(|error| error.to_string())?;
        assert_reference_size(video, VideoReferenceKind::Video, VIDEO_REFERENCE_MAX_BYTES)
            .map_err(|error| error.to_string())?;
    }
    for audio in &media.audios {
        assert_reference_seconds(audio, VideoReferenceKind::Audio)
            .map_err(|error| error.to_string())?;
        assert_reference_size(audio, VideoReferenceKind::Audio, AUDIO_REFERENCE_MAX_BYTES)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// `Metadata.reasoning_effort` -> the request's [`ReasoningEffort`].
fn reasoning_effort(effort: CanvasReasoningEffort) -> ReasoningEffort {
    match effort {
        CanvasReasoningEffort::Auto => ReasoningEffort::Auto,
        CanvasReasoningEffort::Low => ReasoningEffort::Low,
        CanvasReasoningEffort::Medium => ReasoningEffort::Medium,
        CanvasReasoningEffort::High => ReasoningEffort::High,
        CanvasReasoningEffort::Xhigh => ReasoningEffort::Xhigh,
    }
}

/// `Metadata.video_mode` -> the request's wire string.
pub(crate) fn video_mode_id(mode: CanvasVideoMode) -> &'static str {
    match mode {
        CanvasVideoMode::Frames => "frames",
        CanvasVideoMode::Reference => "reference",
    }
}

/// `buildGenerationConfig` (`canvas-generation-helpers.ts:71`): node metadata
/// overrides the config defaults, then the model resolves to its owner channel.
/// `count` applies in every mode (the web's `String(node.metadata.count || 1)`),
/// and an `OpenRouter` video model on the node passes through unresolved (the
/// web's `isOpenRouterVideoModel(nodeModel)` special case).
pub(crate) fn request_config(
    app: &App,
    node: &CanvasNodeData,
    capability: ModelCapability,
) -> ModelRequestConfig {
    let meta = node.metadata.as_ref();
    let mut config = app.config.clone();
    if let Some(value) = meta.and_then(|m| m.quality.clone()) {
        config.quality = value;
    }
    if let Some(value) = meta.and_then(|m| m.size.clone()) {
        config.size = value;
    }
    if let Some(value) = meta.and_then(|m| m.background.clone()) {
        config.background = value;
    }
    if let Some(value) = meta.and_then(|m| m.seconds.clone()) {
        config.video_seconds = value;
    }
    if let Some(value) = meta.and_then(|m| m.vquality.clone()) {
        config.vquality = value;
    }
    if let Some(value) = meta.and_then(|m| m.generate_audio.clone()) {
        config.video_generate_audio = value;
    }
    if let Some(value) = meta.and_then(|m| m.watermark.clone()) {
        config.video_watermark = value;
    }
    if let Some(value) = meta.and_then(|m| m.video_mode) {
        config.video_mode = video_mode_id(value).to_string();
    }
    if let Some(value) = meta.and_then(|m| m.reasoning_effort) {
        config.reasoning_effort = reasoning_effort(value);
    }
    config.count = match capability {
        ModelCapability::Image => meta.and_then(|m| m.count).map_or_else(
            || config.canvas_image_count.clone(),
            |count| count.to_string(),
        ),
        _ => meta
            .and_then(|m| m.count)
            .map_or_else(|| config.count.clone(), |count| count.to_string()),
    };
    let model = meta.and_then(|m| m.model.clone());
    let resolved = match model {
        Some(model)
            if !model.is_empty()
                && capability == ModelCapability::Video
                && is_openrouter_video_model(&model) =>
        {
            model
        }
        other => resolve_model_for_capability(&config, other.as_deref(), capability),
    };
    resolve_model_request_config(&config, &resolved)
}

async fn image_outcome(client: Client, job: Job, cancel: Cancel) -> Result<AiOutcome, AiError> {
    let Job {
        req,
        prompt,
        references,
        ..
    } = job;
    let has_reference = !references.is_empty();
    let result = if has_reference {
        run_with_retry(
            GENERATION_MAX_ATTEMPTS,
            GENERATION_RETRY_DELAY_MS,
            &cancel,
            |_attempt| {
                let client = client.clone();
                let req = req.clone();
                let prompt = prompt.clone();
                let references = references.clone();
                let cancel = cancel.clone();
                async move { request_edit(&client, &req, &prompt, &references, &cancel).await }
            },
        )
        .await?
    } else {
        run_with_retry(
            GENERATION_MAX_ATTEMPTS,
            GENERATION_RETRY_DELAY_MS,
            &cancel,
            |_attempt| {
                let client = client.clone();
                let req = req.clone();
                let prompt = prompt.clone();
                let cancel = cancel.clone();
                async move { request_generation(&client, &req, &prompt, &cancel).await }
            },
        )
        .await?
    };
    Ok(AiOutcome::Image {
        urls: result
            .images
            .into_iter()
            .map(|image| image.data_url)
            .collect(),
        model: req.model.clone(),
        usage: result.usage,
        generation_id: result.generation_id,
        has_reference,
    })
}

async fn text_outcome(
    client: Client,
    job: Job,
    cancel: Cancel,
    tx: Sender<AiEvent>,
) -> Result<AiOutcome, AiError> {
    let Job {
        node_id,
        req,
        prompt,
        references,
        text_count,
        ..
    } = job;
    let has_reference = !references.is_empty();
    // The panel's "generations" count (`text_generation_count`): one streaming
    // request per alternative, the first one streaming into the node content.
    let count = text_count.clamp(1, TEXT_GENERATION_MAX);
    let mut texts: Vec<String> = Vec::new();
    let mut usage: Option<ResponseUsage> = None;
    for index in 0..count {
        let response = run_with_retry(
            GENERATION_MAX_ATTEMPTS,
            GENERATION_RETRY_DELAY_MS,
            &cancel,
            |_attempt| {
                let client = client.clone();
                let req = req.clone();
                let prompt = prompt.clone();
                let references = references.clone();
                let cancel = cancel.clone();
                let tx = tx.clone();
                let node_id = node_id.clone();
                let stream_primary = index == 0;
                async move {
                    let mut on_delta = move |text: &str| {
                        // Only the primary alternative streams into the node
                        // content; the others arrive with the finished event.
                        if stream_primary {
                            let _sent = tx.send(AiEvent::Delta {
                                node_id: node_id.clone(),
                                text: text.to_string(),
                            });
                        }
                    };
                    request_image_question(
                        &client,
                        &req,
                        &prompt,
                        &references,
                        &cancel,
                        &mut on_delta,
                    )
                    .await
                }
            },
        )
        .await?;
        if usage.is_none() {
            usage = response.usage;
        }
        texts.push(response.content);
    }
    let text = texts.first().cloned().unwrap_or_default();
    Ok(AiOutcome::Text {
        text,
        texts,
        model: req.model.clone(),
        usage,
        has_reference,
    })
}

async fn video_outcome(client: Client, job: Job, cancel: Cancel) -> Result<AiOutcome, AiError> {
    let Job {
        req,
        prompt,
        video,
        plugin,
        video_save,
        ..
    } = job;
    if let Some(provider) = &plugin {
        // The declarative plugin provider (`model-plugin.ts`): the create POST
        // and the poll loop are data-driven, so the task completes inline.
        let result =
            create_plugin_video_task(&client, &req, provider, &prompt, &video, &cancel).await?;
        return Ok(AiOutcome::Video {
            task_id: format!("plugin-{}", now_millis()),
            url: video_content(result, video_save.as_deref())?,
            model: req.model.clone(),
            cost_usd: None,
            provider: CanvasVideoTaskProvider::Plugin,
        });
    }
    let task = create_video_task_with_media(&client, &req, &prompt, &video, &cancel).await?;
    let result = wait_for_video_task(&client, &req, &task, &cancel).await?;
    let provider = match task.provider {
        open_canvas_ai::video::VideoProvider::OpenRouter => CanvasVideoTaskProvider::Openrouter,
        open_canvas_ai::video::VideoProvider::OpenAi => CanvasVideoTaskProvider::Openai,
    };
    let cost_usd = result.cost_usd;
    Ok(AiOutcome::Video {
        task_id: task.id,
        url: video_content(result, video_save.as_deref())?,
        model: req.model.clone(),
        cost_usd,
        provider,
    })
}

/// A finished video as node content: its download URL, else the inline bytes
/// written to the exports path (the desktop has no upload storage).
fn video_content(
    result: VideoResult,
    save_path: Option<&std::path::Path>,
) -> Result<Option<String>, AiError> {
    if let Some(url) = result.url {
        return Ok(Some(url));
    }
    let Some(bytes) = result.bytes else {
        return Ok(None);
    };
    let Some(path) = save_path else {
        return Err(AiError::Api(
            "The generated video could not be stored".to_string(),
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            AiError::Api(format!("The video folder could not be created: {error}"))
        })?;
    }
    std::fs::write(path, &bytes)
        .map_err(|error| AiError::Api(format!("The video could not be saved: {error}")))?;
    Ok(Some(format!("file:{}", path.display())))
}

fn spawn_generation(app: &mut App, job: Job) {
    let Some(service) = app.ai.as_ref() else {
        return;
    };
    let client = service.client.clone();
    let tx = service.tx.clone();
    let cancel = Cancel::new();
    let mode = job.mode;
    let node_id = job.node_id.clone();
    let task_cancel = cancel.clone();
    let task_node = job.node_id.clone();
    let task_tx = tx.clone();
    let handle = service.runtime.spawn(async move {
        let outcome = match mode {
            GenMode::Image => image_outcome(client, job, task_cancel).await,
            GenMode::Text => text_outcome(client, job, task_cancel, task_tx.clone()).await,
            GenMode::Video => video_outcome(client, job, task_cancel).await,
        };
        let event = match outcome {
            Ok(outcome) => AiEvent::Done {
                node_id: task_node,
                outcome,
            },
            Err(error) if error.is_canceled() => AiEvent::Cancelled { node_id: task_node },
            Err(error) => AiEvent::Error {
                node_id: task_node,
                message: error.to_string(),
            },
        };
        let _sent = task_tx.send(event);
    });
    if let Some(service) = app.ai.as_mut() {
        service.tasks.insert(
            node_id,
            TaskHandle {
                join: handle,
                cancel,
            },
        );
    }
}

impl App {
    /// Records the visible part of an agent run before the request leaves the
    /// app. This is deliberately a short, inspectable activity trace rather
    /// than hidden chain-of-thought; an external agent can still supply its
    /// own `agentTrace`/`agentSteps` through the bridge, which are appended.
    fn begin_attempt_trace(
        &mut self,
        node_id: &str,
        prompt: &str,
        req: &open_canvas_ai::config::ModelRequestConfig,
    ) {
        let now = self.now_ms();
        let run_id = format!("agent-{now}");
        let at = crate::store::now_iso();
        let excerpt = trace_excerpt(prompt);
        let plan = format!(
            "{} image(s) · {} · {} · quality {}",
            req.count, req.model, req.size, req.quality
        );
        let raw = format!(
            "Agent run {run_id}\nObservation: The attempt prompt asks for: {excerpt}\nPlan: Generate {plan}.\nAction: Dispatched the provider request."
        );
        let steps = vec![
            CanvasAgentStep {
                kind: "observe".into(),
                title: "Read the attempt prompt".into(),
                detail: Some(excerpt),
                at: Some(at.clone()),
            },
            CanvasAgentStep {
                kind: "plan".into(),
                title: "Prepared the image request".into(),
                detail: Some(plan),
                at: Some(at.clone()),
            },
            CanvasAgentStep {
                kind: "act".into(),
                title: "Started generation".into(),
                detail: Some("Waiting for the provider".into()),
                at: Some(at),
            },
        ];
        if let Some(node) = self
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)
        {
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.agent_run_id = Some(run_id);
            meta.agent_steps.get_or_insert_with(Vec::new).extend(steps);
            let trace = meta.agent_raw_trace.get_or_insert_with(String::new);
            if !trace.is_empty() {
                trace.push_str("\n\n");
            }
            trace.push_str(&raw);
        }
        self.doc.commit(now);
        self.note_doc_change();
    }

    /// Generate/stop for the focused panel node: stop when it is already
    /// running, otherwise build the request and start it on the AI runtime.
    pub(crate) fn ai_start(&mut self, ui: &AppWindow, node_id: &str) {
        if self.ai.is_none() {
            self.set_notice(ui, "The AI runtime could not start".to_string(), true);
            return;
        }
        let Some(node) = self.doc.node(node_id).cloned() else {
            return;
        };
        let Some(mode) = gen_mode(self, &node) else {
            return;
        };
        let (prompt, references) = collect_inputs(self, node_id, &node);
        // A plugin's built-in panel may declare a prompt prefix (panorama's
        // equirectangular constraint): the host applies it before generation.
        let prompt = match self.plugin.prompt_prefix(&node) {
            Some(prefix) => format!("{prefix}{prompt}"),
            None => prompt,
        };
        if prompt.trim().is_empty() {
            self.set_notice(ui, "Add a prompt before generating".to_string(), true);
            return;
        }
        let mut req = request_config(self, &node, mode.capability());
        if req.api_key.trim().is_empty() {
            self.set_notice(
                ui,
                "Set a provider API key in Settings before generating".to_string(),
                true,
            );
            return;
        }
        let (video, plugin, video_save) = if mode == GenMode::Video {
            let (video_mode, media) = collect_video_media(self, node_id, &node);
            // The prompt node's mode wins over the generation node's/config's
            // (`project.tsx:630`), so the request builder resolves the same mode
            // the media collection did.
            req.video_mode = video_mode.as_str().to_string();
            if let Err(message) = validate_video_media(&req, &media) {
                // A reference the provider would reject never reaches the wire:
                // the failure lands on the node like any request error.
                self.set_node_state(node_id, CanvasNodeStatus::Error, Some(message.clone()));
                self.log(
                    RuntimeLogLevel::Error,
                    "generation",
                    message.clone(),
                    Some(node_id.to_string()),
                );
                self.set_notice(ui, message, true);
                self.sync_rows_of(ui, &[node_id.to_string()]);
                self.sync_panel(ui);
                self.note_doc_change();
                return;
            }
            let model_value = node
                .metadata
                .as_ref()
                .and_then(|meta| meta.model.clone())
                .filter(|model| !model.trim().is_empty())
                .unwrap_or_else(|| {
                    resolve_model_for_capability(&self.config, None, ModelCapability::Video)
                });
            let plugin = resolve_model_script(&self.config, &model_value)
                .and_then(|script| plugin_provider_from_script(&script));
            let save = self
                .store
                .export_path_for(&node.title)
                .with_extension("mp4");
            (media, plugin, Some(save))
        } else {
            (VideoRequestMedia::default(), None, None)
        };
        if node.node_type == CanvasNodeType::ImageAttempt {
            self.begin_attempt_trace(node_id, &prompt, &req);
        }
        // The producing UX lives on the result card: an image generation
        // places its placeholder media node in the loading state now, and the
        // generation card itself shows no card-level spinner. The text and
        // video paths keep their existing loading marker on the card.
        let output_id = if mode == GenMode::Image && node.node_type == CanvasNodeType::ImageAttempt
        {
            self.set_node_state(node_id, CanvasNodeStatus::Loading, None);
            None
        } else if mode == GenMode::Image {
            let output = self.prepare_image_output(node_id, &prompt, &req);
            self.set_node_state(node_id, CanvasNodeStatus::Idle, None);
            output
        } else {
            self.set_node_state(node_id, CanvasNodeStatus::Loading, None);
            None
        };
        self.panel.running = Some(node_id.to_string());
        self.panel.popover = None;
        self.log(
            RuntimeLogLevel::Info,
            "generation",
            format!("Generation started with {}", req.model),
            Some(node_id.to_string()),
        );
        // The image path just placed a brand-new node in the document. A
        // per-id refresh can only rewrite rows that already exist, so the new
        // placeholder would stay invisible until the next full sync (leaving
        // and re-entering the canvas). Rebuild the row model here so the
        // loading image node appears on the same frame as the click.
        if output_id.is_some() {
            self.sync_rows_full(ui);
        } else {
            self.sync_rows_of(ui, &[node_id.to_string()]);
        }
        self.sync_panel(ui);
        spawn_generation(
            self,
            Job {
                node_id: node_id.to_string(),
                mode,
                req,
                prompt,
                references,
                video,
                plugin,
                video_save,
                text_count: text_generation_count(&node),
            },
        );
    }

    pub(crate) fn ai_cancel(&mut self, ui: &AppWindow, node_id: &str) {
        if let Some(service) = self.ai.as_mut() {
            if let Some(task) = service.tasks.remove(node_id) {
                task.cancel.cancel();
                task.join.abort();
            }
        }
        self.panel.running = None;
        self.set_node_state(node_id, CanvasNodeStatus::Idle, None);
        self.sync_rows_of(ui, &[node_id.to_string()]);
        self.sync_panel(ui);
        self.note_doc_change();
    }

    /// Cancels the generation that owns a node before that node is deleted.
    ///
    /// Deleting a loading image node is the stop gesture: the placeholder is
    /// in the loading state, so its generation task is aborted first and the
    /// caller then removes the node. Deleting the generation card itself also
    /// drops its still-loading placeholder so no orphan spinner is left behind.
    pub(crate) fn cancel_generation_for_node(&mut self, ui: &AppWindow, node_id: &str) {
        let running = self.panel.running.as_deref() == Some(node_id);
        let loading = self
            .doc
            .node(node_id)
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.status)
            .is_some_and(|status| status == CanvasNodeStatus::Loading);
        if !running && !loading {
            return;
        }
        let generation = if running {
            node_id.to_string()
        } else {
            let Some(generation) = self
                .ai
                .as_ref()
                .and_then(|service| service.generation_for_output(node_id))
                .map(str::to_string)
            else {
                return;
            };
            generation
        };
        if generation == node_id {
            let output = self
                .ai
                .as_ref()
                .and_then(|service| service.output_node(node_id))
                .map(str::to_string);
            if let Some(output) = output {
                let output_loading = self
                    .doc
                    .node(&output)
                    .and_then(|node| node.metadata.as_ref())
                    .and_then(|meta| meta.status)
                    .is_some_and(|status| status == CanvasNodeStatus::Loading);
                if output_loading {
                    self.doc.remove_node(&output, self.now_ms());
                }
            }
        }
        self.ai_cancel(ui, &generation);
    }

    /// Drains background work on the app tick (called from the 60 ms app tick):
    /// the AI generation events plus the archive worker's export/import result
    /// (`library_export`). Nothing here blocks the UI thread.
    #[expect(
        clippy::too_many_lines,
        reason = "the event drain dispatches every AI event variant in one match"
    )]
    pub fn poll_ai(&mut self, ui: &AppWindow) {
        self.poll_archive(ui);
        let Some(service) = self.ai.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = service.rx.try_recv() {
            events.push(event);
        }
        if events.is_empty() {
            return;
        }
        for event in events {
            // The two cost events (the Cost tab's own lookup and a finished
            // generation's) write records and repaint the settings modal.
            if self.apply_cost_event(ui, &event) {
                continue;
            }
            match event {
                AiEvent::Delta { node_id, text } => {
                    self.set_node_content(&node_id, &text);
                    self.sync_rows_of(ui, &[node_id]);
                    self.sync_panel(ui);
                }
                AiEvent::Done { node_id, outcome } => {
                    self.forget_task(&node_id);
                    self.panel.running = None;
                    let auto_export = self.attempt_auto_export.as_deref() == Some(node_id.as_str());
                    if auto_export {
                        self.attempt_auto_export = None;
                    }
                    let produced = apply_outcome(self, &node_id, &outcome);
                    self.set_node_state(&node_id, CanvasNodeStatus::Success, None);
                    // An image result owns its generation's cost record; text
                    // and video keep the record on the generating card.
                    let cost_node = produced.as_deref().unwrap_or(&node_id);
                    self.record_generation_cost(ui, cost_node, &outcome);
                    self.log(
                        RuntimeLogLevel::Info,
                        "generation",
                        "Generation finished",
                        Some(node_id.clone()),
                    );
                    let mut dirty = vec![node_id.clone()];
                    if let Some(produced) = produced {
                        dirty.push(produced);
                    }
                    if auto_export {
                        if let Some(image_id) =
                            self.attempt_export_image(ui, &node_id, "Generated image")
                        {
                            dirty.push(image_id);
                        }
                    }
                    self.sync_rows_of(ui, &dirty);
                    self.sync_panel(ui);
                }
                AiEvent::Cancelled { node_id } => {
                    self.forget_task(&node_id);
                    self.panel.running = None;
                    if self.attempt_auto_export.as_deref() == Some(node_id.as_str()) {
                        self.attempt_auto_export = None;
                    }
                    let output_id = self
                        .ai
                        .as_ref()
                        .and_then(|service| service.output_node(&node_id))
                        .map(str::to_string);
                    if let Some(output) = output_id.as_deref() {
                        self.fail_output_node(output, "Generation cancelled");
                    }
                    self.set_node_state(&node_id, CanvasNodeStatus::Idle, None);
                    self.log(
                        RuntimeLogLevel::Info,
                        "generation",
                        "Generation cancelled",
                        Some(node_id.clone()),
                    );
                    let mut dirty = vec![node_id];
                    if let Some(output) = output_id {
                        dirty.push(output);
                    }
                    self.sync_rows_of(ui, &dirty);
                    self.sync_panel(ui);
                }
                AiEvent::Error { node_id, message } => {
                    self.forget_task(&node_id);
                    self.panel.running = None;
                    if self.attempt_auto_export.as_deref() == Some(node_id.as_str()) {
                        self.attempt_auto_export = None;
                    }
                    let output_id = self
                        .ai
                        .as_ref()
                        .and_then(|service| service.output_node(&node_id))
                        .map(str::to_string);
                    if let Some(output) = output_id.as_deref() {
                        self.fail_output_node(output, &message);
                    }
                    // The generation card keeps its error + Retry surface (the
                    // retry reruns the request and reuses the failed card).
                    append_attempt_trace(
                        self,
                        &node_id,
                        "error",
                        "Generation failed",
                        Some(message.clone()),
                        format!("Error: {message}"),
                    );
                    self.set_node_state(&node_id, CanvasNodeStatus::Error, Some(message.clone()));
                    self.log(
                        RuntimeLogLevel::Error,
                        "generation",
                        message.clone(),
                        Some(node_id.clone()),
                    );
                    let mut dirty = vec![node_id];
                    if let Some(output) = output_id {
                        dirty.push(output);
                    }
                    self.sync_rows_of(ui, &dirty);
                    self.sync_panel(ui);
                    self.set_notice(ui, message, true);
                }
                AiEvent::CostFetched { .. } | AiEvent::GenerationCost { .. } => {}
                // AI-backed image tools (`image_ai`): each event applies to its
                // own dialog/state.
                AiEvent::OcrDone { node_id, text } => {
                    self.apply_ocr_done(ui, &node_id, &text);
                }
                AiEvent::OcrError { node_id, message } => {
                    self.apply_ocr_error(ui, &node_id, &message);
                }
                AiEvent::ImageAiItem {
                    job_id,
                    index,
                    result,
                } => {
                    self.apply_image_ai_item(ui, job_id, index, result);
                }
                AiEvent::ImageAiDone { job_id, results } => {
                    self.apply_image_ai_done(ui, job_id, &results);
                }
            }
        }
        self.note_doc_change();
    }

    /// Applies one background cost event; `false` when the event is not a cost
    /// result. Both arms write `costs.json` and refresh the settings modal.
    fn apply_cost_event(&mut self, ui: &AppWindow, event: &AiEvent) -> bool {
        match event {
            AiEvent::CostFetched {
                model,
                unit,
                quantity,
                result,
            } => {
                match result {
                    Ok(usd) => {
                        self.costs
                            .record_lookup(model, *unit, *quantity, *usd, now_millis());
                        self.persist_costs(ui);
                        let message = i18n::tf(
                            self.locale,
                            "config.cost.lookup-ok",
                            &[("usd", &format_usd(*usd))],
                        );
                        self.log(RuntimeLogLevel::Info, "cost", message.clone(), None);
                        self.set_notice(ui, message, false);
                    }
                    Err(error) => {
                        let message = i18n::tf(
                            self.locale,
                            "config.cost.lookup-failed",
                            &[("error", error)],
                        );
                        self.log(RuntimeLogLevel::Error, "cost", message.clone(), None);
                        self.set_notice(ui, message, true);
                    }
                }
                self.sync_config(ui);
                true
            }
            AiEvent::GenerationCost {
                node_id,
                charge,
                result,
            } => {
                let at = now_millis();
                match result {
                    Ok(usd) => {
                        self.costs.record_generation_lookup(
                            node_id,
                            &charge.model,
                            charge.unit,
                            charge.quantity,
                            *usd,
                            at,
                        );
                        self.log(
                            RuntimeLogLevel::Info,
                            "cost",
                            format!(
                                "Recorded looked-up generation cost for {} ({})",
                                charge.model,
                                format_usd(*usd)
                            ),
                            Some(node_id.clone()),
                        );
                    }
                    Err(error) => {
                        let source = self.costs.record_generation(node_id, charge, at);
                        self.log(
                            RuntimeLogLevel::Error,
                            "cost",
                            format!(
                                "Generation cost lookup failed ({error}); recorded a {source} cost for {}",
                                charge.model
                            ),
                            Some(node_id.clone()),
                        );
                    }
                }
                // The produced media node records the final priced amount too
                // (the lookup arrives after the completion handler).
                if self.doc.node(node_id).is_some_and(|node| {
                    matches!(
                        node.node_type,
                        CanvasNodeType::Image | CanvasNodeType::ImageStack
                    )
                }) {
                    let usd = self.costs.records.first().map(|record| record.usd);
                    self.set_generation_meta(node_id, None, None, usd);
                    self.sync_rows_of(ui, std::slice::from_ref(node_id));
                }
                self.persist_costs(ui);
                self.sync_config(ui);
                true
            }
            _ => false,
        }
    }

    /// Records a finished generation in `costs.json` with the response's own
    /// cost when it reported one (`usage.cost` -> `api`, a generation id ->
    /// `/generation` lookup -> `lookup`), else the client-side estimate
    /// (`estimate_token_cost` first, then the model price table).
    fn record_generation_cost(&mut self, ui: &AppWindow, node_id: &str, outcome: &AiOutcome) {
        let Some(node) = self.doc.node(node_id).cloned() else {
            return;
        };
        let Some((unit, quantity)) = crate::cost::generation_charge(&node) else {
            return;
        };
        // A produced image/stack node owns the generation's id and usage; the
        // generating card keeps none of it.
        let media = matches!(
            node.node_type,
            CanvasNodeType::Image | CanvasNodeType::ImageStack
        );
        if media {
            let usage = outcome.usage();
            self.set_generation_meta(
                node_id,
                outcome.generation_id().map(str::to_string),
                usage,
                None,
            );
        }
        let charge = GenerationCharge {
            model: outcome.model().to_string(),
            unit,
            quantity,
            fallback: outcome.cost_hint(),
        };
        if let Some(generation_id) = outcome.lookup_id() {
            let req = request_config(self, &node, ModelCapability::Image);
            let spawned = match self.ai.as_ref() {
                Some(service) => {
                    service.spawn_generation_cost_lookup(
                        req,
                        node_id.to_string(),
                        generation_id.to_string(),
                        charge.clone(),
                    );
                    true
                }
                None => false,
            };
            if spawned {
                self.log(
                    RuntimeLogLevel::Info,
                    "cost",
                    format!("Looking up the billed cost for {}", charge.model),
                    Some(node_id.to_string()),
                );
                return;
            }
        }
        let source = self.costs.record_generation(node_id, &charge, now_millis());
        if media {
            let usd = self.costs.records.first().map(|record| record.usd);
            self.set_generation_meta(node_id, None, None, usd);
        }
        self.persist_costs(ui);
        self.log(
            RuntimeLogLevel::Info,
            "cost",
            format!("Recorded {source} generation cost for {}", charge.model),
            None,
        );
        self.sync_config(ui);
    }

    fn forget_task(&mut self, node_id: &str) {
        if let Some(service) = self.ai.as_mut() {
            service.tasks.remove(node_id);
        }
    }

    fn set_node_state(&mut self, node_id: &str, status: CanvasNodeStatus, error: Option<String>) {
        let now = self.now_ms();
        {
            let Some(node) = self
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == node_id)
            else {
                return;
            };
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.status = Some(status);
            meta.error_details = error;
        }
        self.doc.commit(now);
    }

    fn set_node_content(&mut self, node_id: &str, content: &str) {
        let now = self.now_ms();
        {
            let Some(node) = self
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == node_id)
            else {
                return;
            };
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.content = Some(content.to_string());
        }
        self.doc.commit(now);
    }

    /// Metadata of a not-yet-finished image result: the card is already on the
    /// canvas in the loading state so the generating UX belongs to it. A
    /// multi-image result is one Image Stack carrying loading entries.
    fn image_placeholder_metadata(
        kind: &CanvasNodeType,
        count: usize,
        prompt: &str,
        model: &str,
    ) -> CanvasNodeMetadata {
        let stack = *kind == CanvasNodeType::ImageStack;
        let images = if stack {
            Some(
                (0..count)
                    .map(|index| CanvasNodeImage {
                        id: format!("image-{index}"),
                        status: CanvasNodeStatus::Loading,
                        error_details: None,
                        content: String::new(),
                        natural_width: 0.0,
                        natural_height: 0.0,
                        bytes: 0,
                        mime_type: "image/png".to_string(),
                        ..CanvasNodeImage::default()
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let primary_image_id = images
            .as_ref()
            .and_then(|images| images.first())
            .map(|image| image.id.clone());
        CanvasNodeMetadata {
            content: (!stack).then(String::new),
            status: Some(CanvasNodeStatus::Loading),
            error_details: None,
            prompt: Some(prompt.to_string()),
            model: Some(model.to_string()),
            count: Some(i64::try_from(count).unwrap_or(i64::MAX)),
            images,
            primary_image_id,
            mime_type: Some("image/png".to_string()),
            ..CanvasNodeMetadata::default()
        }
    }

    /// Places (or resets) the image result card for a generation that is about
    /// to run. A failed placeholder from the previous run is reused, so a
    /// retry never piles cards on the canvas; a successful result is left in
    /// place and the new generation gets its own card.
    fn prepare_image_output(
        &mut self,
        generation_id: &str,
        prompt: &str,
        req: &ModelRequestConfig,
    ) -> Option<String> {
        self.doc.node(generation_id)?;
        let count = image_generation_count(req.count);
        let kind = if count > 1 {
            CanvasNodeType::ImageStack
        } else {
            CanvasNodeType::Image
        };
        let now = self.now_ms();
        let previous = self
            .ai
            .as_ref()
            .and_then(|service| service.output_node(generation_id))
            .map(str::to_string);
        if let Some(previous) = previous {
            let reusable = self.doc.node(&previous).is_some_and(|existing| {
                existing.node_type == kind
                    && existing
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.status)
                        .is_some_and(|status| {
                            matches!(status, CanvasNodeStatus::Error | CanvasNodeStatus::Loading)
                        })
            });
            if reusable {
                let meta = Self::image_placeholder_metadata(&kind, count, prompt, &req.model);
                if let Some(existing) = self
                    .doc
                    .nodes_mut()
                    .iter_mut()
                    .find(|node| node.id == previous)
                {
                    existing.metadata = Some(meta);
                }
                self.doc.commit(now);
                if let Some(service) = self.ai.as_mut() {
                    service.remember_output(generation_id, &previous);
                }
                return Some(previous);
            }
            // A stale placeholder of the other kind is replaced; a finished
            // result stays where it is.
            let stale = self.doc.node(&previous).is_some_and(|existing| {
                existing
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.status)
                    .is_some_and(|status| {
                        matches!(status, CanvasNodeStatus::Error | CanvasNodeStatus::Loading)
                    })
            });
            if stale {
                self.doc.remove_node(&previous, now);
            }
        }
        let id = self
            .doc
            .place_output_node(generation_id, kind.clone(), now)?;
        let meta = Self::image_placeholder_metadata(&kind, count, prompt, &req.model);
        if let Some(placed) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
            placed.metadata = Some(meta);
        }
        self.doc.commit(now);
        if let Some(service) = self.ai.as_mut() {
            service.remember_output(generation_id, &id);
        }
        Some(id)
    }

    /// Marks a media placeholder failed with the provider's error and keeps it
    /// on the canvas (the user can retry from the generation card or delete it).
    fn fail_output_node(&mut self, node_id: &str, message: &str) {
        let now = self.now_ms();
        {
            let Some(node) = self
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == node_id)
            else {
                return;
            };
            let stack = node.node_type == CanvasNodeType::ImageStack;
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            meta.status = Some(CanvasNodeStatus::Error);
            meta.error_details = Some(message.to_string());
            if stack {
                if let Some(images) = meta.images.as_mut() {
                    for image in images {
                        image.status = CanvasNodeStatus::Error;
                        image.error_details = Some(message.to_string());
                    }
                }
            }
        }
        self.doc.commit(now);
    }

    /// Writes the producing generation's id / usage / priced amount onto the
    /// media node, so the Cost tab and the node's own info agree on ownership.
    fn set_generation_meta(
        &mut self,
        node_id: &str,
        generation_id: Option<String>,
        usage: Option<CanvasNodeUsage>,
        cost_usd: Option<f64>,
    ) {
        let now = self.now_ms();
        {
            let Some(node) = self
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == node_id)
            else {
                return;
            };
            let meta = node
                .metadata
                .get_or_insert_with(CanvasNodeMetadata::default);
            if generation_id.is_some() {
                meta.generation_id = generation_id;
            }
            if usage.is_some() {
                meta.usage = usage;
            }
            if cost_usd.is_some() {
                meta.cost_usd = cost_usd;
            }
        }
        self.doc.commit(now);
    }
}

/// Applies a finished generation. An image result fills the placeholder card
/// placed when the request started (looked up through the AI service), or
/// places a new one when there is none; the return value is the media node the
/// result landed in, so the cost record can be attributed to it.
#[expect(
    clippy::too_many_lines,
    reason = "the three outcome arms materialise their own node kind; the order mirrors the web apply path"
)]
fn trace_excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let excerpt: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        format!("{excerpt}...")
    } else {
        excerpt
    }
}

fn append_attempt_trace(
    app: &mut App,
    node_id: &str,
    kind: &str,
    title: &str,
    detail: Option<String>,
    raw: String,
) {
    let at = crate::store::now_iso();
    if let Some(node) = app
        .doc
        .nodes_mut()
        .iter_mut()
        .find(|node| node.id == node_id)
    {
        if node.node_type != CanvasNodeType::ImageAttempt {
            return;
        }
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.agent_steps
            .get_or_insert_with(Vec::new)
            .push(CanvasAgentStep {
                kind: kind.to_string(),
                title: title.to_string(),
                detail,
                at: Some(at),
            });
        let trace = meta.agent_raw_trace.get_or_insert_with(String::new);
        if !trace.is_empty() {
            trace.push_str("\n\n");
        }
        trace.push_str(&raw);
    }
}

fn apply_outcome(app: &mut App, node_id: &str, outcome: &AiOutcome) -> Option<String> {
    let now = app.now_ms();
    let output_id = app
        .ai
        .as_ref()
        .and_then(|service| service.output_node(node_id))
        .map(str::to_string);
    let output_id = output_id.as_deref();
    if let AiOutcome::Text { text, texts, .. } = outcome {
        let node = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == node_id)?;
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.content = Some(text.clone());
        let entries: Vec<open_canvas_core::node::CanvasNodeText> = texts
            .iter()
            .enumerate()
            .map(|(index, content)| open_canvas_core::node::CanvasNodeText {
                id: format!("text-{index}"),
                status: CanvasNodeStatus::Success,
                error_details: None,
                content: content.clone(),
            })
            .collect();
        if !entries.is_empty() {
            meta.primary_text_id = entries.first().map(|entry| entry.id.clone());
            meta.texts = Some(entries);
        }
        app.doc.commit(now);
        return None;
    }

    match outcome {
        AiOutcome::Image { urls, model, .. } => {
            // An Image Attempt owns its output directly: append a run to the
            // version instead of creating a downstream placeholder node.
            if app
                .doc
                .node(node_id)
                .is_some_and(|node| node.node_type == CanvasNodeType::ImageAttempt)
            {
                let images: Vec<CanvasNodeImage> = urls
                    .iter()
                    .enumerate()
                    .map(|(index, url)| CanvasNodeImage {
                        id: format!("image-{now}-{index}"),
                        status: CanvasNodeStatus::Success,
                        error_details: None,
                        content: url.clone(),
                        natural_width: 0.0,
                        natural_height: 0.0,
                        bytes: 0,
                        mime_type: "image/png".to_string(),
                        ..CanvasNodeImage::default()
                    })
                    .collect();
                let primary = images.first().map(|image| image.id.clone());
                let run = open_canvas_core::node::CanvasAttemptRun {
                    id: format!("run-{now}"),
                    status: CanvasNodeStatus::Success,
                    error_details: None,
                    images: images.clone(),
                    primary_image_id: primary.clone(),
                    generation_id: None,
                    usage: open_canvas_core::node::CanvasNodeUsage::default(),
                    agent_run_id: None,
                    created_at: Some(crate::store::now_iso()),
                };
                if let Some(node) = app
                    .doc
                    .nodes_mut()
                    .iter_mut()
                    .find(|node| node.id == node_id)
                {
                    let meta = node
                        .metadata
                        .get_or_insert_with(CanvasNodeMetadata::default);
                    meta.images = Some(images);
                    meta.primary_image_id = primary;
                    meta.content = None;
                    meta.status = Some(CanvasNodeStatus::Success);
                    meta.error_details = None;
                    meta.model = Some(model.clone());
                    meta.attempt_frozen = Some(true);
                    meta.attempt_runs.get_or_insert_with(Vec::new).push(run);
                }
                append_attempt_trace(
                    app,
                    node_id,
                    "observe",
                    "Generation completed",
                    Some(format!("{} image(s) returned", urls.len())),
                    format!(
                        "Observation: The provider returned {} image(s).",
                        urls.len()
                    ),
                );
                app.doc.commit(now);
                return Some(node_id.to_string());
            }
            let requested = app
                .doc
                .node(node_id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.count)
                .unwrap_or(1)
                .max(1);
            let requested = usize::try_from(requested).unwrap_or(1);
            let stack = urls.len() > 1 || requested > 1;
            let kind = if stack {
                CanvasNodeType::ImageStack
            } else {
                CanvasNodeType::Image
            };
            let existing = output_id
                .and_then(|id| app.doc.node(id))
                .map(|node| node.id.clone());
            let id = if let Some(id) = existing {
                if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                    if node.node_type != kind {
                        let spec = get_node_spec(&kind);
                        node.node_type = kind.clone();
                        node.title = spec.title;
                        node.width = spec.width;
                        node.height = spec.height;
                    }
                }
                id
            } else {
                app.doc.place_output_node(node_id, kind.clone(), now)?
            };

            let images = stack.then(|| {
                urls.iter()
                    .enumerate()
                    .map(|(index, url)| CanvasNodeImage {
                        id: format!("image-{index}"),
                        status: CanvasNodeStatus::Success,
                        error_details: None,
                        content: url.clone(),
                        natural_width: 0.0,
                        natural_height: 0.0,
                        bytes: 0,
                        mime_type: "image/png".to_string(),
                        ..CanvasNodeImage::default()
                    })
                    .collect::<Vec<_>>()
            });
            let primary_image_id = images
                .as_ref()
                .and_then(|images| images.first())
                .map(|image| image.id.clone());
            let produced_count = urls.len().max(1);
            if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                let meta = node
                    .metadata
                    .get_or_insert_with(CanvasNodeMetadata::default);
                if stack {
                    meta.content = None;
                    meta.images = images;
                    meta.primary_image_id = primary_image_id;
                } else {
                    meta.content = urls.first().cloned();
                    meta.images = None;
                    meta.primary_image_id = None;
                }
                meta.mime_type = Some("image/png".to_string());
                meta.status = Some(CanvasNodeStatus::Success);
                meta.error_details = None;
                meta.model = Some(model.clone());
                meta.count = Some(i64::try_from(produced_count).unwrap_or(i64::MAX));
            }
            app.doc.commit(now);
            Some(id)
        }
        AiOutcome::Video {
            task_id,
            url,
            provider,
            ..
        } => {
            if let Some(id) = app
                .doc
                .create_output_node(node_id, CanvasNodeType::Video, 0, now)
            {
                if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                    let meta = node
                        .metadata
                        .get_or_insert_with(CanvasNodeMetadata::default);
                    meta.content.clone_from(url);
                    meta.mime_type = Some("video/mp4".to_string());
                    meta.status = Some(CanvasNodeStatus::Success);
                    meta.video_task_id = Some(task_id.clone());
                    meta.video_task_provider = Some(*provider);
                }
            }
            app.doc.commit(now);
            None
        }
        AiOutcome::Text { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::CanvasNodeMetadata;
    use open_canvas_core::node::{CanvasNodeData, CanvasNodeType, Position};
    use slint::Model;

    fn generation_node() -> CanvasNodeData {
        CanvasNodeData {
            id: "gen".into(),
            node_type: CanvasNodeType::ImageGeneration,
            title: "Image Generation".into(),
            position: Position::new(100.0, 100.0),
            width: 320.0,
            height: 360.0,
            metadata: Some(CanvasNodeMetadata {
                generation_mode: Some(open_canvas_core::node::CanvasGenerationMode::Image),
                ..Default::default()
            }),
        }
    }

    fn test_app() -> Result<App, Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        Ok(App::new(crate::store::Store::at(dir.path().to_path_buf())))
    }

    fn image_node(id: &str, content: &str) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: CanvasNodeType::Image,
            title: id.into(),
            position: Position::new(0.0, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: Some(CanvasNodeMetadata {
                content: Some(content.into()),
                ..Default::default()
            }),
        }
    }

    fn wire(from: &str, to: &str) -> open_canvas_core::CanvasConnection {
        open_canvas_core::CanvasConnection {
            id: format!("wire-{from}-{to}"),
            from_node_id: from.into(),
            to_node_id: to.into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }
    }

    /// An image wired into the prompt node (not the generation) is a reference;
    /// the prompt's own wires come first and direct + prompt wires de-duplicate
    /// by node id.
    #[test]
    fn prompt_wired_references_reach_the_generation_exactly_once(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        let prompt = CanvasNodeData {
            id: "prompt".into(),
            node_type: CanvasNodeType::Prompt,
            title: "Image Prompt".into(),
            position: Position::new(0.0, 0.0),
            width: 320.0,
            height: 240.0,
            metadata: Some(CanvasNodeMetadata {
                prompt: Some("a red fox in snow".into()),
                ..Default::default()
            }),
        };
        app.doc = crate::doc::Doc::assemble(
            vec![
                prompt,
                image_node("prompt-ref", "data:image/png;base64,PROMPT"),
                image_node("direct-ref", "data:image/png;base64,DIRECT"),
                image_node("both-ref", "data:image/png;base64,BOTH"),
                image_node("last-ref", "data:image/png;base64,LAST"),
                generation_node(),
            ],
            vec![
                wire("prompt-ref", "prompt"),
                wire("both-ref", "prompt"),
                wire("both-ref", "gen"),
                wire("direct-ref", "gen"),
                wire("prompt", "gen"),
                wire("last-ref", "prompt"),
            ],
        );

        let node = app.doc.node("gen").cloned().ok_or("generation node")?;
        let (text, references) = collect_inputs(&app, "gen", &node);
        assert_eq!(text, "a red fox in snow");
        assert_eq!(
            references,
            vec![
                "data:image/png;base64,PROMPT".to_string(),
                "data:image/png;base64,BOTH".to_string(),
                "data:image/png;base64,LAST".to_string(),
                "data:image/png;base64,DIRECT".to_string(),
            ],
            "prompt wires first, then direct wires, each node at most once"
        );

        // The same prompt-owned reference also appears once when it is the only
        // input (an image stack is a reference node too, read through its
        // primary image entry).
        let mut stack = image_node("stack-ref", "");
        stack.node_type = CanvasNodeType::ImageStack;
        if let Some(meta) = stack.metadata.as_mut() {
            meta.content = None;
            meta.primary_image_id = Some("stack-image".into());
            meta.images = Some(vec![open_canvas_core::node::CanvasNodeImage {
                id: "stack-image".into(),
                content: "data:image/png;base64,STACK".into(),
                ..open_canvas_core::node::CanvasNodeImage::default()
            }]);
        }
        app.doc = crate::doc::Doc::assemble(
            vec![
                CanvasNodeData {
                    id: "prompt".into(),
                    node_type: CanvasNodeType::Prompt,
                    title: "Image Prompt".into(),
                    position: Position::new(0.0, 0.0),
                    width: 320.0,
                    height: 240.0,
                    metadata: Some(CanvasNodeMetadata {
                        prompt: Some("a red fox".into()),
                        ..Default::default()
                    }),
                },
                stack,
                generation_node(),
            ],
            vec![wire("stack-ref", "prompt"), wire("prompt", "gen")],
        );
        let node = app.doc.node("gen").cloned().ok_or("generation node")?;
        let (_, references) = collect_inputs(&app, "gen", &node);
        assert_eq!(references, vec!["data:image/png;base64,STACK".to_string()]);
        Ok(())
    }

    /// A wired Image Prompt reaches the generation node: its text lives in
    /// `metadata.prompt` (written by the inline editor), not `metadata.content`,
    /// and reading only the latter made Generate report a missing prompt.
    #[test]
    fn a_wired_image_prompt_reaches_the_generation() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        let prompt = CanvasNodeData {
            id: "prompt".into(),
            node_type: CanvasNodeType::Prompt,
            title: "Image Prompt".into(),
            position: Position::new(0.0, 0.0),
            width: 320.0,
            height: 240.0,
            metadata: Some(CanvasNodeMetadata {
                prompt: Some("a red fox in snow".into()),
                // Inline reference spans are presentation data: the request text
                // is still exactly the prompt body.
                mentions: Some(vec![open_canvas_core::node::CanvasNodeMention {
                    start: 2,
                    end: 8,
                    node_id: "image-1".into(),
                }]),
                ..Default::default()
            }),
        };
        let connection = open_canvas_core::CanvasConnection {
            id: "wire".into(),
            from_node_id: "prompt".into(),
            to_node_id: "gen".into(),
            relation: Some("prompt".into()),
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        };
        app.doc = crate::doc::Doc::assemble(vec![prompt, generation_node()], vec![connection]);

        let node = app.doc.node("gen").cloned().ok_or("generation node")?;
        let (text, references) = collect_inputs(&app, "gen", &node);
        assert_eq!(text, "a red fox in snow");
        assert!(references.is_empty(), "a prompt is not a reference image");
        Ok(())
    }

    fn image_outcome(urls: Vec<String>) -> AiOutcome {
        AiOutcome::Image {
            urls,
            model: "gpt-image-1".into(),
            usage: crate::ai_service::ImageUsage::default(),
            generation_id: Some("gen-xyz".into()),
            has_reference: false,
        }
    }

    fn request(count: i64) -> ModelRequestConfig {
        ModelRequestConfig {
            model: "gpt-image-1".into(),
            base_url: String::new(),
            api_key: String::new(),
            system_prompt: String::new(),
            reasoning_effort: ReasoningEffort::Auto,
            quality: String::new(),
            size: String::new(),
            background: String::new(),
            count,
            video_seconds: "6".into(),
            vquality: String::new(),
            video_generate_audio: String::new(),
            video_watermark: String::new(),
            video_mode: "frames".into(),
        }
    }

    fn overlaps(a: &CanvasNodeData, b: &CanvasNodeData, margin: f64) -> bool {
        a.position.x < b.position.x + b.width + margin
            && b.position.x < a.position.x + a.width + margin
            && a.position.y < b.position.y + b.height + margin
            && b.position.y < a.position.y + a.height + margin
    }

    /// A finished single-image generation becomes a sibling Image node to the
    /// right of the generation, carrying the bitmap. Images are display-only,
    /// so there is no generation -> image wire.
    #[test]
    fn image_generation_creates_a_placed_image_node() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        app.doc = crate::doc::Doc::assemble(vec![generation_node()], Vec::new());
        let url = "data:image/png;base64,AAAA".to_string();
        let produced = apply_outcome(&mut app, "gen", &image_outcome(vec![url.clone()]))
            .ok_or("produced node")?;

        let nodes = app.doc.nodes();
        assert_eq!(nodes.len(), 2, "the result is its own node");
        let generation = app.doc.node("gen").ok_or("generation node")?;
        assert!(
            generation
                .metadata
                .as_ref()
                .and_then(|meta| meta.content.clone())
                .is_none(),
            "the generation node no longer hosts the output"
        );
        let output = app.doc.node(&produced).ok_or("downstream image node")?;
        assert_eq!(output.node_type, CanvasNodeType::Image);
        assert_eq!(
            output
                .metadata
                .as_ref()
                .and_then(|meta| meta.content.clone()),
            Some(url),
            "the image node carries the bitmap"
        );
        assert!(
            output.position.x >= generation.position.x + generation.width + 48.0,
            "it sits to the right of the generation"
        );
        assert_eq!(
            app.doc.connections().len(),
            0,
            "an image result is placed, not wired"
        );
        Ok(())
    }

    /// A multi-image result becomes ONE Image Stack node: `metadata.images`
    /// holds every result and the first one is the primary image.
    #[test]
    fn multi_image_generation_creates_one_image_stack() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        let mut gen = generation_node();
        gen.metadata.as_mut().ok_or("metadata")?.count = Some(3);
        app.doc = crate::doc::Doc::assemble(vec![gen], Vec::new());
        let urls = vec![
            "data:image/png;base64,AAA".to_string(),
            "data:image/png;base64,BBB".to_string(),
            "data:image/png;base64,CCC".to_string(),
        ];
        let produced =
            apply_outcome(&mut app, "gen", &image_outcome(urls.clone())).ok_or("produced node")?;

        assert_eq!(
            app.doc
                .nodes()
                .iter()
                .filter(|node| {
                    matches!(
                        node.node_type,
                        CanvasNodeType::Image | CanvasNodeType::ImageStack
                    )
                })
                .count(),
            1,
            "three images are one stack card"
        );
        let stack = app.doc.node(&produced).ok_or("stack node")?;
        assert_eq!(stack.node_type, CanvasNodeType::ImageStack);
        let meta = stack.metadata.as_ref().ok_or("stack metadata")?;
        let images = meta.images.as_ref().ok_or("stack images")?;
        assert_eq!(images.len(), 3);
        assert_eq!(
            images
                .iter()
                .map(|image| image.content.clone())
                .collect::<Vec<_>>(),
            urls
        );
        assert_eq!(meta.primary_image_id.as_deref(), Some("image-0"));
        assert_eq!(
            meta.status,
            Some(CanvasNodeStatus::Success),
            "the placeholder is cleared on completion"
        );
        assert_eq!(app.doc.connections().len(), 0);
        Ok(())
    }

    /// The default spot to the right is taken: the result lands on the next
    /// free rectangle instead of overlapping the existing node.
    #[test]
    fn image_placement_avoids_an_occupied_default_spot() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        let gen = generation_node();
        let blocker = CanvasNodeData {
            id: "blocker".into(),
            node_type: CanvasNodeType::Text,
            title: "Blocker".into(),
            position: Position::new(gen.position.x + gen.width + 48.0, gen.position.y),
            width: 340.0,
            height: 240.0,
            metadata: Some(CanvasNodeMetadata::default()),
        };
        app.doc = crate::doc::Doc::assemble(vec![gen, blocker], Vec::new());
        let produced = apply_outcome(
            &mut app,
            "gen",
            &image_outcome(vec!["data:image/png;base64,AAAA".into()]),
        )
        .ok_or("produced node")?;
        let output = app.doc.node(&produced).ok_or("output")?;
        let blocker = app.doc.node("blocker").ok_or("blocker")?;
        assert!(
            !overlaps(output, blocker, 8.0),
            "the result never overlaps an existing node"
        );
        assert!(
            output.position.x > blocker.position.x,
            "the search moves further right before stacking on top"
        );
        Ok(())
    }

    /// The generating state lives on the new card: the placeholder exists in
    /// the loading state while the request runs, is filled on success and is
    /// marked failed (with the provider's error) when the request fails.
    #[test]
    fn the_placeholder_carries_the_generating_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        let mut gen = generation_node();
        gen.metadata.as_mut().ok_or("metadata")?.count = Some(3);
        app.doc = crate::doc::Doc::assemble(vec![gen], Vec::new());
        let placeholder = app
            .prepare_image_output("gen", "a red fox", &request(3))
            .ok_or("placeholder")?;
        assert_eq!(
            app.ai
                .as_ref()
                .and_then(|service| service.generation_for_output(&placeholder)),
            Some("gen"),
            "the placeholder's Retry can rerun the generation"
        );
        {
            let node = app.doc.node(&placeholder).ok_or("placeholder node")?;
            let meta = node.metadata.as_ref().ok_or("placeholder metadata")?;
            assert_eq!(node.node_type, CanvasNodeType::ImageStack);
            assert_eq!(meta.status, Some(CanvasNodeStatus::Loading));
            let images = meta.images.as_ref().ok_or("loading entries")?;
            assert_eq!(images.len(), 3, "one loading entry per requested image");
            assert!(images
                .iter()
                .all(|image| image.status == CanvasNodeStatus::Loading));
            assert_eq!(app.doc.connections().len(), 0);
        }

        let urls = vec![
            "data:image/png;base64,AAA".into(),
            "data:image/png;base64,BBB".into(),
            "data:image/png;base64,CCC".into(),
        ];
        apply_outcome(&mut app, "gen", &image_outcome(urls));
        {
            let meta = app
                .doc
                .node(&placeholder)
                .and_then(|node| node.metadata.as_ref())
                .ok_or("completed metadata")?;
            assert_eq!(meta.status, Some(CanvasNodeStatus::Success));
            assert!(meta.images.as_ref().is_some_and(|images| images
                .iter()
                .all(|image| image.status == CanvasNodeStatus::Success)));
        }

        // A second (failed) request reuses the failed card and records the
        // provider error on it.
        let failed = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("failed placeholder")?;
        app.fail_output_node(&failed, "provider exploded");
        let meta = app
            .doc
            .node(&failed)
            .and_then(|node| node.metadata.as_ref())
            .ok_or("failed metadata")?;
        assert_eq!(meta.status, Some(CanvasNodeStatus::Error));
        assert_eq!(meta.error_details.as_deref(), Some("provider exploded"));
        Ok(())
    }

    /// Deleting the loading placeholder is the stop gesture: its owning task is
    /// cancelled first, then the caller removes the node.
    #[test]
    fn deleting_a_loading_placeholder_cancels_its_generation(
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        let mut app = app.borrow_mut();
        app.doc = crate::doc::Doc::assemble(vec![generation_node()], Vec::new());
        let placeholder = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("placeholder")?;
        app.panel.running = Some("gen".into());
        assert_eq!(
            app.ai
                .as_ref()
                .and_then(|service| service.generation_for_output(&placeholder)),
            Some("gen"),
            "the loading image node knows which generation owns it"
        );

        app.cancel_generation_for_node(&ui, &placeholder);
        assert!(
            app.panel.running.is_none(),
            "the owning task is cancelled before the delete"
        );
        assert!(
            app.doc.node(&placeholder).is_some(),
            "the placeholder is still present for the delete path to remove"
        );
        let now = app.now_ms();
        app.doc.remove_node(&placeholder, now);
        assert!(app.doc.node(&placeholder).is_none());
        Ok(())
    }

    /// Starting an image generation must rebuild the row model, not just
    /// refresh existing ids: the placeholder is a brand-new node and would
    /// otherwise stay invisible until a full sync (leaving the canvas).
    #[test]
    fn starting_an_image_generation_adds_the_placeholder_row_immediately(
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        let mut app = app.borrow_mut();
        app.doc = crate::doc::Doc::assemble(vec![generation_node()], Vec::new());
        let placeholder = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("placeholder")?;
        app.sync_rows_full(&ui);
        assert!(
            ui.get_nodes().iter().any(|row| row.id == placeholder),
            "the loading image node is in the row model on the same frame"
        );

        let src = include_str!("ai_service.rs");
        let start = src.find("pub(crate) fn ai_start").ok_or("ai_start")?;
        let end = src.find("pub(crate) fn ai_cancel").ok_or("ai_cancel")?;
        let start_block = &src[start..end];
        assert!(
            start_block.contains("if output_id.is_some() {")
                && start_block.contains("self.sync_rows_full(ui);"),
            "ai_start uses the full row rebuild for a new image placeholder"
        );
        Ok(())
    }

    /// A retry reuses the failed card but clears every bitmap it carried: the
    /// card is an empty loading placeholder until the result arrives.
    #[test]
    fn a_regeneration_clears_the_reused_placeholder_bitmap(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = test_app()?;
        app.doc = crate::doc::Doc::assemble(vec![generation_node()], Vec::new());
        let placeholder = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("placeholder")?;
        {
            let node = app
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == placeholder)
                .ok_or("placeholder node")?;
            let meta = node.metadata.as_mut().ok_or("metadata")?;
            meta.status = Some(CanvasNodeStatus::Error);
            meta.content = Some("data:image/png;base64,STALE".into());
            meta.thumbnail = Some("data:image/png;base64,STALE".into());
            meta.natural_width = Some(512.0);
            meta.natural_height = Some(512.0);
            meta.bytes = Some(4096);
        }
        let again = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("reused placeholder")?;
        assert_eq!(again, placeholder, "the retry reuses the failed card");
        let node = app.doc.node(&again).ok_or("placeholder node")?;
        let meta = node.metadata.as_ref().ok_or("metadata")?;
        assert_eq!(meta.status, Some(CanvasNodeStatus::Loading));
        assert_eq!(
            meta.content.as_deref(),
            Some(""),
            "the old bitmap is cleared before the request starts"
        );
        assert!(meta.thumbnail.is_none());
        assert!(meta.natural_width.is_none());
        assert!(meta.bytes.is_none());

        // The same clearing holds for a multi-image stack: every entry is a
        // blank loading entry, never the previous run's bitmap.
        let mut gen = generation_node();
        gen.metadata.as_mut().ok_or("metadata")?.count = Some(3);
        app.doc = crate::doc::Doc::assemble(vec![gen], Vec::new());
        let stack = app
            .prepare_image_output("gen", "three foxes", &request(3))
            .ok_or("stack placeholder")?;
        {
            let node = app
                .doc
                .nodes_mut()
                .iter_mut()
                .find(|node| node.id == stack)
                .ok_or("stack node")?;
            let meta = node.metadata.as_mut().ok_or("metadata")?;
            meta.status = Some(CanvasNodeStatus::Error);
            for image in meta.images.as_mut().ok_or("images")? {
                image.status = CanvasNodeStatus::Success;
                image.content = "data:image/png;base64,STALE".into();
            }
        }
        let again = app
            .prepare_image_output("gen", "three foxes", &request(3))
            .ok_or("reused stack")?;
        assert_eq!(again, stack);
        let images = app
            .doc
            .node(&again)
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.images.as_ref())
            .ok_or("images")?;
        assert_eq!(images.len(), 3);
        assert!(images.iter().all(|image| {
            image.status == CanvasNodeStatus::Loading && image.content.is_empty()
        }));
        Ok(())
    }

    /// The generation's cost record belongs to the produced image node, and
    /// the node's metadata keeps the generation id, usage and priced amount.
    #[test]
    fn generation_cost_is_attributed_to_the_produced_node() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        let mut app = app.borrow_mut();
        app.doc = crate::doc::Doc::assemble(vec![generation_node()], Vec::new());
        let placeholder = app
            .prepare_image_output("gen", "a red fox", &request(1))
            .ok_or("placeholder")?;
        // A provider-reported amount keeps the record synchronous (no lookup).
        let outcome = AiOutcome::Image {
            urls: vec!["data:image/png;base64,AAAA".into()],
            model: "gpt-image-1".into(),
            usage: crate::ai_service::ImageUsage {
                cost: Some(0.04),
                ..crate::ai_service::ImageUsage::default()
            },
            generation_id: Some("gen-xyz".into()),
            has_reference: false,
        };
        apply_outcome(&mut app, "gen", &outcome);
        app.record_generation_cost(&ui, &placeholder, &outcome);

        let record = app.costs.records.first().ok_or("cost record")?;
        assert_eq!(record.node_id, placeholder);
        assert_ne!(record.node_id, "gen");
        let meta = app
            .doc
            .node(&placeholder)
            .and_then(|node| node.metadata.as_ref())
            .ok_or("produced metadata")?;
        assert_eq!(meta.generation_id.as_deref(), Some("gen-xyz"));
        assert!(meta.cost_usd.is_some(), "the priced amount is on the node");
        assert!(meta.usage.is_some(), "the usage block is on the node");
        Ok(())
    }

    /// A video result becomes a downstream video node holding the task
    /// reference and the poster/url, not content on the generation card.
    #[test]
    fn video_generation_creates_a_downstream_video_node() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut app = test_app()?;
        let mut node = generation_node();
        node.node_type = CanvasNodeType::VideoGeneration;
        app.doc = crate::doc::Doc::assemble(vec![node], Vec::new());
        apply_outcome(
            &mut app,
            "gen",
            &AiOutcome::Video {
                task_id: "task-7".into(),
                url: Some("https://example.test/v.mp4".into()),
                model: "test/video".into(),
                cost_usd: None,
                provider: open_canvas_core::node::CanvasVideoTaskProvider::Openai,
            },
        );
        let output = app
            .doc
            .nodes()
            .iter()
            .find(|node| node.node_type == CanvasNodeType::Video)
            .ok_or("downstream video node")?;
        let meta = output.metadata.as_ref().ok_or("video metadata")?;
        assert_eq!(meta.video_task_id.as_deref(), Some("task-7"));
        assert_eq!(meta.content.as_deref(), Some("https://example.test/v.mp4"));
        assert_eq!(app.doc.connections().len(), 1);
        assert!(
            app.doc
                .node("gen")
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.video_task_id.clone())
                .is_none(),
            "the generation node keeps no task payload"
        );
        Ok(())
    }
}
