// allow: SIZE_OK — 1:1 mirror of the 51-field CanvasNodeMetadata wire schema from
// web/src/types/canvas.ts; this is data declarations (all Option, serde-attributed)
// plus the field list the merge macro needs, not grown logic.
//! UI-agnostic domain types for the canvas, mirroring
//! `web/src/types/canvas.ts` field-for-field so JSON round-trips with the web app.
//!
//! Node `type` is an open string: the ten built-ins are the [`CanvasNodeType`]
//! variants, everything else (plugin ids of the form `"<pluginId>:<name>"`, or any
//! future type) is captured losslessly by [`CanvasNodeType::Plugin`] instead of
//! failing to deserialize.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 2D point in canvas (world) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Workspace tab of a project. Mirrors `CanvasWorkspace` (canvas.ts:6-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasWorkspace {
    Canvas,
    Image,
    Pixel,
}

impl CanvasWorkspace {
    pub const ALL: [CanvasWorkspace; 3] = [
        CanvasWorkspace::Canvas,
        CanvasWorkspace::Image,
        CanvasWorkspace::Pixel,
    ];
}

/// Built-in node kinds plus an open fallback for plugin / unknown types.
///
/// The web type is `CanvasNodeType | (string & {})` (canvas.ts:16-29); the
/// [`CanvasNodeType::Plugin`] variant is that open string. Serialization is
/// custom so built-ins emit their exact kebab-case ids and plugin ids are kept
/// verbatim, and deserialization never fails on an unknown type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasNodeType {
    Image,
    ImageStack,
    /// One prompt/config version with its generated images.
    ImageAttempt,
    Text,
    Prompt,
    VideoPrompt,
    ImageGeneration,
    VideoGeneration,
    Video,
    Assets,
    /// A card that owns (and previews) one Image Studio project.
    SmartCanvas,
    /// Any non-built-in type, e.g. `"my-plugin:custom-node"`.
    Plugin(String),
}

impl CanvasNodeType {
    /// The ten built-in kinds, in declaration order.
    pub const BUILTINS: [CanvasNodeType; 5] = [
        CanvasNodeType::Image,
        CanvasNodeType::ImageStack,
        CanvasNodeType::ImageAttempt,
        CanvasNodeType::Assets,
        CanvasNodeType::SmartCanvas,
    ];

    /// Wire id of the type (built-in kebab-case id or the raw plugin id).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Image => "image",
            Self::ImageStack => "image-stack",
            Self::ImageAttempt => "image-attempt",
            Self::Text => "text",
            Self::Prompt => "prompt",
            Self::VideoPrompt => "video-prompt",
            Self::ImageGeneration => "image-generation",
            Self::VideoGeneration => "video-generation",
            Self::Video => "video",
            Self::Assets => "assets",
            Self::SmartCanvas => "smart-canvas",
            Self::Plugin(id) => id,
        }
    }

    /// Parse a wire id, falling back to [`CanvasNodeType::Plugin`].
    pub fn from_id(id: &str) -> Self {
        match id {
            "image" => Self::Image,
            "image-stack" => Self::ImageStack,
            "image-attempt" => Self::ImageAttempt,
            "text" => Self::Text,
            "prompt" => Self::Prompt,
            "video-prompt" => Self::VideoPrompt,
            "image-generation" => Self::ImageGeneration,
            "video-generation" => Self::VideoGeneration,
            "video" => Self::Video,
            "assets" => Self::Assets,
            "smart-canvas" => Self::SmartCanvas,
            other => Self::Plugin(other.to_string()),
        }
    }

    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Plugin(_))
    }

    /// True for the two generator kinds (`image-generation`, `video-generation`).
    pub fn is_generation(&self) -> bool {
        matches!(
            self,
            Self::ImageAttempt | Self::ImageGeneration | Self::VideoGeneration
        )
    }
}

impl Serialize for CanvasNodeType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CanvasNodeType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(CanvasNodeType::from_id(&raw))
    }
}

impl std::fmt::Display for CanvasNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `CanvasNodeStatus` (canvas.ts:31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasNodeStatus {
    #[default]
    Idle,
    Success,
    Loading,
    Error,
}

/// `CanvasGenerationMode` (canvas.ts:32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasGenerationMode {
    Text,
    Image,
    Video,
}

/// `CanvasImageGenerationType` (canvas.ts:33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasImageGenerationType {
    Generation,
    Edit,
}

/// `CanvasVideoMode` (canvas.ts:56).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasVideoMode {
    Frames,
    Reference,
}

/// `CanvasAssetSource` (canvas.ts:67).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasAssetSource {
    Folder,
    Cache,
    Studio,
}

/// `CanvasNodeMetadata.fontWeight` (canvas.ts:218).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasFontWeight {
    Normal,
    Bold,
}

/// `CanvasNodeMetadata.textAlign` (canvas.ts:220).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasTextAlign {
    Left,
    Center,
    Right,
}

/// `CanvasNodeMetadata.reasoningEffort` (canvas.ts:225).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasReasoningEffort {
    Auto,
    Low,
    Medium,
    High,
    Xhigh,
}

/// `CanvasNodeMetadata.videoTaskProvider` (canvas.ts:252).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasVideoTaskProvider {
    Openai,
    Plugin,
    Openrouter,
}

/// `CanvasNodeImage` (canvas.ts:35-47) — one image entry of an image-stack node.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeImage {
    pub id: String,
    #[serde(default)]
    pub status: CanvasNodeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_key: Option<String>,
    pub natural_width: f64,
    pub natural_height: f64,
    pub bytes: u64,
    pub mime_type: String,
}

/// Token/cost figures a produced media node keeps from the generation that
/// created it: the provider usage block plus the amount the app billed. Stored
/// on the produced node (not the generation card) so the media's own info bar
/// and the Cost tab agree on which node owns the generation.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<f64>,
    /// `usage.cost` the provider reported, when it reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

/// `CanvasNodeText` (canvas.ts:49-54) — one text entry of a batch text node.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeText {
    pub id: String,
    #[serde(default)]
    pub status: CanvasNodeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
    pub content: String,
}

/// `CanvasVideoSlots` (canvas.ts:61-65) — bound media node ids, never payloads.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasVideoSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_frame: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,
}

/// Legacy inline reference token from older documents: the byte range of a
/// reference label inside `metadata.prompt` plus the referenced node id.
///
/// The desktop app no longer renders, creates or writes mention spans: prompt
/// text is plain text and references are the wired chips on the card. The type
/// survives only so documents that still carry a `metadata.mentions` array keep
/// deserializing; [`CanvasNodeMetadata::mentions`] is never serialized again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeMention {
    pub start: usize,
    pub end: usize,
    pub node_id: String,
}

/// `CanvasAssetStudioProjects` (canvas.ts:70-73).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasAssetStudioProjects {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel: Option<Vec<String>>,
}

/// One generation run inside an image attempt. A frozen attempt can keep
/// appending runs with the same prompt/config, but never changes its version.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasAttemptRun {
    pub id: String,
    #[serde(default)]
    pub status: CanvasNodeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
    #[serde(default)]
    pub images: Vec<CanvasNodeImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    #[serde(default)]
    pub usage: CanvasNodeUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// One visible agent step. `raw_text` on the attempt keeps the full original
/// trace; these steps are the navigable timeline.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasAgentStep {
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

/// `CanvasNodeMetadata` (canvas.ts:209-261): the single wide struct holding every
/// per-kind field. All fields are optional; unknown keys are ignored by serde so a
/// plugin that adds metadata keys cannot make a load fail.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composer_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CanvasNodeStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_frozen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_runs: Option<Vec<CanvasAttemptRun>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_raw_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_steps: Option<Vec<CanvasAgentStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<CanvasFontWeight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<CanvasTextAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_mode: Option<CanvasGenerationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_type: Option<CanvasImageGenerationType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<CanvasReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texts: Option<Vec<CanvasNodeText>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_text_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vquality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_audio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_mode: Option<CanvasVideoMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_slots: Option<CanvasVideoSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,
    /// Legacy `metadata.mentions` input from older documents. Accepted on load
    /// and then ignored: the prompt body is the plain prompt text and references
    /// come from the wired chips, so this field is never serialized or rendered
    /// again. Kept in the struct so the existing document reader remains
    /// compatible with payloads written before the span model was retired.
    #[serde(default, skip_serializing)]
    #[doc(hidden)]
    pub mentions: Option<Vec<CanvasNodeMention>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_resize: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<CanvasNodeImage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_task_provider: Option<CanvasVideoTaskProvider>,
    /// The provider's `x-generation-id` for the generation that produced this
    /// media node (a `/generation` cost lookup uses it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    /// The amount billed/priced for the generation that produced this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The provider usage block of the producing generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<CanvasNodeUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_source: Option<CanvasAssetSource>,
    /// Absolute path of the local folder an assets node is bound to (the web
    /// keeps this in `useAssetFolderStore`; on the desktop it is project data,
    /// so it persists with the node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_studio_projects: Option<CanvasAssetStudioProjects>,
    /// The Image Studio project a `smart-canvas` card owns and previews. Kept
    /// in node metadata so the link survives save/reload; a missing project
    /// only makes the card fall back to its empty placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_canvas_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas_title: Option<String>,
}

macro_rules! merge_metadata_fields {
    ($base:expr, $patch:expr, $($field:ident),+ $(,)?) => {
        CanvasNodeMetadata {
            $(
                $field: $patch.$field.clone().or_else(|| $base.$field.clone()),
            )+
        }
    };
}

/// Overlay `patch` on top of `base` (`{ ...base, ...patch }` in the web factory):
/// a `Some` in `patch` wins, otherwise the `base` value is kept.
pub fn merge_metadata(base: &CanvasNodeMetadata, patch: &CanvasNodeMetadata) -> CanvasNodeMetadata {
    merge_metadata_fields!(
        base,
        patch,
        content,
        composer_content,
        prompt,
        status,
        attempt_version,
        attempt_title,
        attempt_frozen,
        attempt_runs,
        agent_raw_trace,
        agent_steps,
        agent_run_id,
        error_details,
        font_size,
        line_height,
        font_family,
        font_weight,
        italic,
        text_align,
        text_color,
        generation_mode,
        generation_type,
        model,
        reasoning_effort,
        size,
        quality,
        background,
        count,
        text_count,
        texts,
        primary_text_id,
        seconds,
        vquality,
        generate_audio,
        watermark,
        video_mode,
        video_slots,
        references,
        mentions,
        natural_width,
        natural_height,
        free_resize,
        images,
        primary_image_id,
        storage_key,
        thumbnail,
        thumbnail_key,
        mime_type,
        bytes,
        duration_ms,
        video_task_id,
        video_task_provider,
        generation_id,
        cost_usd,
        usage,
        blend_mode,
        opacity,
        interactive,
        locked,
        hidden,
        asset_source,
        asset_folder,
        asset_studio_projects,
        smart_canvas_project_id,
        canvas_title,
    )
}

/// `CanvasNodeData` (canvas.ts:263-271).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeData {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: CanvasNodeType,
    pub title: String,
    pub position: Position,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CanvasNodeMetadata>,
}

/// `CanvasConnection` (canvas.ts:273-278). Z-order of connections is array order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasConnection {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    /// `derive` | `duplicate` | `reference` | `extract` (new attempt graph).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The agent-written explanation for this transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Human/agent-readable change summary (prompt/config/reference diff).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_run_id: Option<String>,
}

/// `CanvasScope` role (canvas.ts:281-285).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasScopeRole {
    Character,
    Location,
    Plot,
    Graph,
}

/// `CanvasScope` (canvas.ts:281-285) — marks a project owned by a writing project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasScope {
    pub write_project_id: String,
    pub role: CanvasScopeRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}

/// `ConnectionHandle.handleType` (canvas.ts:294-297).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionHandleType {
    Source,
    Target,
}

/// `ConnectionHandle` (canvas.ts:294-297).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHandle {
    pub node_id: String,
    pub handle_type: ConnectionHandleType,
}

/// `SelectionBox` (canvas.ts:299-306).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionBox {
    pub start_world_x: f64,
    pub start_world_y: f64,
    pub current_world_x: f64,
    pub current_world_y: f64,
    pub additive: bool,
    pub initial_selected_node_ids: Vec<String>,
}

/// Build the node id the factory uses: `` `${type}-${Date.now()}-${rand}` ``.
pub fn create_node_id(node_type: &CanvasNodeType, timestamp_ms: u64, nonce: &str) -> String {
    format!("{}-{}-{}", node_type.as_str(), timestamp_ms, nonce)
}

/// Create a node from its type, mirroring `createCanvasNode` (canvas-node-factory.ts:9-25):
/// the given `position` is the node's **center** (the returned node is centered on it),
/// and `metadata` is merged over the type's default metadata.
pub fn create_canvas_node(
    node_type: CanvasNodeType,
    position: Position,
    metadata: Option<CanvasNodeMetadata>,
    id: String,
) -> CanvasNodeData {
    let spec = crate::constants::get_node_spec(&node_type);
    let merged = match metadata {
        Some(patch) => merge_metadata(&spec.metadata, &patch),
        None => spec.metadata,
    };
    CanvasNodeData {
        id,
        title: spec.title,
        position: Position::new(
            position.x - spec.width / 2.0,
            position.y - spec.height / 2.0,
        ),
        width: spec.width,
        height: spec.height,
        node_type,
        metadata: Some(merged),
    }
}

/// Apply a metadata patch, mirroring `applyNodeConfigPatch` (canvas-node-factory.ts:51-57):
/// when a size string is patched onto an Image/Video node that has no content yet, the
/// node is re-sized from the ratio and re-centered on its old center.
pub fn apply_node_config_patch(
    node: &CanvasNodeData,
    patch: &CanvasNodeMetadata,
) -> CanvasNodeData {
    let base = node.metadata.clone().unwrap_or_default();
    let mut next = node.clone();
    next.metadata = Some(merge_metadata(&base, patch));

    let spec = crate::constants::get_node_spec(&node.node_type);
    let has_content = base.content.is_some();
    let size = if patch.size.is_some() && !has_content {
        patch
            .size
            .as_deref()
            .and_then(|s| crate::geometry::node_size_from_ratio(s, spec.width, spec.height))
    } else {
        None
    };

    if let Some((width, height)) = size {
        if matches!(
            node.node_type,
            CanvasNodeType::Image | CanvasNodeType::Video
        ) {
            let center_x = node.position.x + node.width / 2.0;
            let center_y = node.position.y + node.height / 2.0;
            next.width = width;
            next.height = height;
            next.position = Position::new(center_x - width / 2.0, center_y - height / 2.0);
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, node_type: CanvasNodeType) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type,
            title: id.to_string(),
            position: Position::new(0.0, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: None,
        }
    }

    #[test]
    fn builtin_ids_round_trip_exactly() {
        for node_type in CanvasNodeType::BUILTINS {
            assert_eq!(CanvasNodeType::from_id(node_type.as_str()), node_type);
        }
    }

    #[test]
    fn unknown_and_plugin_node_types_do_not_fail() {
        assert_eq!(
            CanvasNodeType::from_id("my-plugin:custom"),
            CanvasNodeType::Plugin("my-plugin:custom".to_string())
        );
        // Round-trips a plugin type through JSON without loss.
        let raw = serde_json::to_string(&CanvasNodeType::Plugin("acme:thing".into())).unwrap();
        assert_eq!(raw, "\"acme:thing\"");
        let back: CanvasNodeType = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, CanvasNodeType::Plugin("acme:thing".to_string()));
    }

    #[test]
    fn node_data_round_trips_field_for_field() {
        let meta = CanvasNodeMetadata {
            status: Some(CanvasNodeStatus::Success),
            prompt: Some("a cat".into()),
            video_mode: Some(CanvasVideoMode::Frames),
            count: Some(4),
            references: Some(vec!["image:a".into()]),
            ..Default::default()
        };
        let original = CanvasNodeData {
            metadata: Some(meta),
            ..node("image-1", CanvasNodeType::Image)
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"type\":\"image\""));
        assert!(json.contains("\"videoMode\":\"frames\""));
        assert!(json.contains("\"count\":4"));
        assert!(!json.contains("\"mentions\""));
        let back: CanvasNodeData = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn legacy_mentions_load_without_being_rendered_or_written_back() {
        // Documents written by an older build may carry an inline span list.
        // They still load, the prompt text is exactly `metadata.prompt`, and a
        // save drops the inert span list instead of resurrecting the model.
        let json = r#"{
            "prompt": "a red fox",
            "mentions": [{ "start": 2, "end": 10, "nodeId": "image:a" }]
        }"#;
        let parsed: CanvasNodeMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.prompt.as_deref(), Some("a red fox"));
        assert_eq!(
            parsed.mentions.as_ref().map(Vec::len),
            Some(1),
            "the legacy payload is accepted"
        );
        let written = serde_json::to_value(&parsed).unwrap();
        assert!(
            written.get("mentions").is_none(),
            "legacy spans are never written back"
        );
        assert_eq!(written["prompt"], "a red fox");
    }

    #[test]
    fn unknown_metadata_keys_are_ignored() {
        let json = r#"{
            "id": "n1",
            "type": "prompt",
            "title": "Prompt",
            "position": { "x": 1.0, "y": 2.0 },
            "width": 340.0,
            "height": 240.0,
            "metadata": { "prompt": "hi", "pluginSecretFlag": true, "future": [1,2,3] }
        }"#;
        let parsed: CanvasNodeData = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.metadata.unwrap().prompt.as_deref(), Some("hi"));
    }

    #[test]
    fn absent_optional_fields_are_not_serialized() {
        let n = node("text-1", CanvasNodeType::Text);
        let value = serde_json::to_value(&n).unwrap();
        assert!(value.get("metadata").is_none());
    }

    #[test]
    fn create_canvas_node_centers_on_position_and_merges_metadata() {
        let patch = CanvasNodeMetadata {
            prompt: Some("hello".into()),
            ..Default::default()
        };
        let created = create_canvas_node(
            CanvasNodeType::Prompt,
            Position::new(500.0, 400.0),
            Some(patch),
            "prompt-1".to_string(),
        );
        // Prompt spec is 340x240 -> centered: x = 500-170, y = 400-120.
        assert_eq!(created.position, Position::new(330.0, 280.0));
        assert_eq!(created.width, 340.0);
        assert_eq!(created.height, 240.0);
        let meta = created.metadata.unwrap();
        assert_eq!(meta.prompt.as_deref(), Some("hello"));
        // Default spec status survives the merge.
        assert_eq!(meta.status, Some(CanvasNodeStatus::Idle));
    }

    #[test]
    fn apply_node_config_patch_resizes_and_recenters_image() {
        let mut current = node("img-1", CanvasNodeType::Image);
        current.width = 340.0;
        current.height = 240.0;
        current.position = Position::new(100.0, 100.0);
        let patch = CanvasNodeMetadata {
            size: Some("1024x1024".into()),
            ..Default::default()
        };
        let next = apply_node_config_patch(&current, &patch);
        // 1:1 ratio vs 340/240 base -> 240x240, re-centered on the old center (270, 220).
        assert_eq!(next.width, 240.0);
        assert_eq!(next.height, 240.0);
        assert_eq!(next.position, Position::new(150.0, 100.0));
    }

    #[test]
    fn apply_node_config_patch_keeps_size_when_content_present() {
        let mut current = node("img-2", CanvasNodeType::Image);
        current.metadata = Some(CanvasNodeMetadata {
            content: Some("blob".into()),
            ..Default::default()
        });
        let patch = CanvasNodeMetadata {
            size: Some("1024x1024".into()),
            ..Default::default()
        };
        let next = apply_node_config_patch(&current, &patch);
        assert_eq!(next.width, 340.0);
        assert_eq!(next.height, 240.0);
    }

    #[test]
    fn smart_canvas_is_a_builtin_with_its_own_id_and_default_title() {
        assert_eq!(
            CanvasNodeType::from_id("smart-canvas"),
            CanvasNodeType::SmartCanvas
        );
        assert!(CanvasNodeType::SmartCanvas.is_builtin());
        assert_eq!(CanvasNodeType::SmartCanvas.as_str(), "smart-canvas");
        assert_eq!(
            crate::constants::get_node_spec(&CanvasNodeType::SmartCanvas).title,
            "Smart Canvas"
        );
    }

    #[test]
    fn smart_canvas_project_link_round_trips_and_defaults_when_absent() {
        let original = CanvasNodeData {
            metadata: Some(CanvasNodeMetadata {
                smart_canvas_project_id: Some("image-1-1".into()),
                ..Default::default()
            }),
            ..node("smart-canvas-1", CanvasNodeType::SmartCanvas)
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"type\":\"smart-canvas\""));
        assert!(json.contains("\"smartCanvasProjectId\":\"image-1-1\""));
        let back: CanvasNodeData = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);

        // A document written before the field existed still loads, with no link.
        let legacy = r#"{
            "id": "smart-canvas-old",
            "type": "smart-canvas",
            "title": "Smart Canvas",
            "position": { "x": 0.0, "y": 0.0 },
            "width": 420.0,
            "height": 300.0,
            "metadata": {}
        }"#;
        let parsed: CanvasNodeData = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            parsed
                .metadata
                .and_then(|meta| meta.smart_canvas_project_id),
            None
        );
    }
}
