//! Full-fidelity video request shapes, ported from the media-carrying paths of
//! `web/src/services/api/video.ts` and `web/src/lib/video-generation.ts`.
//!
//! The original port ([`crate::video`]) shipped only the reference-image path
//! with a hardcoded OpenAI `mode`. This module models the rest of the web's
//! request surface without changing those public functions:
//!
//! * **first/last frame slots** (`frame_type: first_frame | last_frame`) and the
//!   positional fallback the web uses when no slot list exists;
//! * **reference images, videos, and audios** with the per-kind / total limits
//!   and the OpenRouter `input_references` shape (`image_url` / `video_url` /
//!   `audio_url`);
//! * the **mode** semantics (`resolveVideoMode`: an explicit `reference` mode or
//!   more than two images means reference; otherwise frames);
//! * the per-model **capability table** (resolutions, duration bounds, audio,
//!   frame slots, accepted reference kinds) the OpenRouter body resolves against;
//! * the web's **validation rules**: unsupported reference kind, audio-only
//!   references, reference duration window (2–15 s), per-kind byte ceilings
//!   (video 50 MB, audio 15 MB), the 64 MB total request cap, and the 9/3/3/12
//!   provider counts.
//!
//! Media is carried as owned bytes plus its MIME type: bytes feed the OpenAI
//! multipart form, while [`ReferenceMedia::data_url`] produces the base64 data
//! URL the OpenRouter JSON body needs. Both come from the same value, so the two
//! providers stay in lockstep without a file system or browser `Blob`.

use base64::Engine as _;
use serde_json::{json, Value};

use crate::config::{model_option_name, ModelRequestConfig};
use crate::error::AiError;
use crate::video::{
    bool_config, normalize_video_resolution, normalize_video_size, video_aspect_ratio,
};

/// `VIDEO_REFERENCE_SECONDS_MIN` (`video-generation.ts:34`).
pub const VIDEO_REFERENCE_SECONDS_MIN: f64 = 2.0;
/// `VIDEO_REFERENCE_SECONDS_MAX` (`video-generation.ts:35`).
pub const VIDEO_REFERENCE_SECONDS_MAX: f64 = 15.0;
/// `VIDEO_REFERENCE_LIMITS.image` (`video-generation.ts:32`).
pub const VIDEO_REFERENCE_IMAGE_LIMIT: usize = 9;
/// `VIDEO_REFERENCE_LIMITS.video` (`video-generation.ts:32`).
pub const VIDEO_REFERENCE_VIDEO_LIMIT: usize = 3;
/// `VIDEO_REFERENCE_LIMITS.audio` (`video-generation.ts:32`).
pub const VIDEO_REFERENCE_AUDIO_LIMIT: usize = 3;
/// `VIDEO_REFERENCE_TOTAL_LIMIT` (`video-generation.ts:33`).
pub const VIDEO_REFERENCE_TOTAL_LIMIT: usize = 12;
/// `VIDEO_REFERENCE_MAX_BYTES` (`video.ts:36`).
pub const VIDEO_REFERENCE_MAX_BYTES: usize = 50 * 1024 * 1024;
/// `AUDIO_REFERENCE_MAX_BYTES` (`video.ts:37`).
pub const AUDIO_REFERENCE_MAX_BYTES: usize = 15 * 1024 * 1024;
/// `VIDEO_REFERENCE_REQUEST_MAX_BYTES` (`video.ts:38`).
pub const VIDEO_REFERENCE_REQUEST_MAX_BYTES: usize = 64 * 1024 * 1024;

/// `VideoFrameReference.frameType` (`video-generation.ts:10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFrameType {
    First,
    Last,
}

impl VideoFrameType {
    /// The wire value (`first_frame` / `last_frame`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::First => "first_frame",
            Self::Last => "last_frame",
        }
    }
}

/// `VideoReferenceKind` (`video-generation.ts:6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoReferenceKind {
    Image,
    Video,
    Audio,
}

impl VideoReferenceKind {
    /// The wire / label value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }
}

/// `VideoMode` (`video-generation.ts:3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMode {
    Frames,
    Reference,
}

impl VideoMode {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frames => "frames",
            Self::Reference => "reference",
        }
    }
}

/// `resolveVideoMode` (`video.ts:455`): an explicit `reference` mode or more than
/// two images means reference; anything else is frames.
pub fn resolve_video_mode(mode: &str, image_count: usize) -> VideoMode {
    if mode == "reference" || image_count > 2 {
        VideoMode::Reference
    } else {
        VideoMode::Frames
    }
}

/// `normalizeVideoMode` (`video-generation.ts:12`).
pub fn normalize_video_mode(value: &str) -> VideoMode {
    if value == "reference" {
        VideoMode::Reference
    } else {
        VideoMode::Frames
    }
}

/// A single reference image, video, or audio clip. `bytes` power the multipart
/// form; [`ReferenceMedia::data_url`] derives the OpenRouter JSON URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceMedia {
    /// Display / multipart file name.
    pub name: String,
    /// MIME type (`image/png`, `video/mp4`, `audio/mpeg`, …).
    pub mime: String,
    /// Raw bytes (also the source of the derived data URL).
    pub bytes: Vec<u8>,
    /// Optional duration, enabling the web's 2–15 s reference check.
    pub duration_ms: Option<u64>,
}

impl ReferenceMedia {
    /// A named media value.
    pub fn new(name: impl Into<String>, mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            mime: mime.into(),
            bytes,
            duration_ms: None,
        }
    }

    /// Attach a duration (milliseconds) so [`assert_reference_seconds`] applies.
    #[must_use]
    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Decode a `data:<mime>[;base64],<payload>` URL into owned bytes.
    pub fn from_data_url(url: &str) -> Result<Self, AiError> {
        let (meta, payload) = url
            .strip_prefix("data:")
            .and_then(|rest| rest.split_once(','))
            .ok_or_else(|| AiError::invalid("Reference media must be a data URL"))?;
        if !meta.contains(";base64") {
            return Err(AiError::invalid("Reference media must be base64 encoded"));
        }
        let mime = meta
            .split(';')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .map_err(|_| AiError::invalid("Reference media is not valid base64"))?;
        Ok(Self::new(String::new(), mime, bytes))
    }

    /// `data:<mime>;base64,<payload>` for JSON bodies.
    pub fn data_url(&self) -> String {
        let payload = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{payload}", self.mime)
    }

    /// Byte length (used by the per-kind / total ceilings).
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

/// The reference media for one video request (`VideoMediaOptions`, `video.ts:24`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoRequestMedia {
    /// Reference images (OpenAI `image[]`, OpenRouter `image_url` parts).
    pub images: Vec<ReferenceMedia>,
    /// Explicit first-frame slot (OpenRouter `frame_images`).
    pub first_frame: Option<ReferenceMedia>,
    /// Explicit last-frame slot (OpenRouter `frame_images`).
    pub last_frame: Option<ReferenceMedia>,
    /// Reference videos (OpenAI `video[]`, OpenRouter `video_url` parts).
    pub videos: Vec<ReferenceMedia>,
    /// Reference audios (OpenAI `audio[]`, OpenRouter `audio_url` parts).
    pub audios: Vec<ReferenceMedia>,
}

impl VideoRequestMedia {
    /// Reference images only, decoded from data URLs (the legacy
    /// [`crate::video::create_video_task`] shape).
    pub fn from_reference_data_urls(urls: &[String]) -> Result<Self, AiError> {
        let images = urls
            .iter()
            .map(|url| ReferenceMedia::from_data_url(url))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            images,
            ..Self::default()
        })
    }

    /// True when either explicit frame slot is set.
    pub fn has_explicit_frames(&self) -> bool {
        self.first_frame.is_some() || self.last_frame.is_some()
    }
}

/// `VideoModelCapability` (`video-generation.ts:18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoModelCapability {
    pub resolutions: Vec<String>,
    pub duration_min: i64,
    pub duration_max: i64,
    pub aspect_ratios: Vec<String>,
    pub supports_size: bool,
    pub supports_audio: bool,
    pub supports_watermark: bool,
    pub supports_seed: bool,
    pub frame_images: Vec<VideoFrameType>,
    pub max_frame_images: usize,
    pub input_references: bool,
    pub reference_kinds: Vec<VideoReferenceKind>,
}

/// `openRouterVideoModels` (`video-generation.ts:37`): the hailuo pair.
fn model_capability(model: &str) -> Option<VideoModelCapability> {
    let references = vec![
        VideoReferenceKind::Image,
        VideoReferenceKind::Video,
        VideoReferenceKind::Audio,
    ];
    let aspect_ratios = ["21:9", "16:9", "4:3", "1:1", "3:4", "9:16"]
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    match model.trim() {
        "minimax/hailuo-3-max" => Some(VideoModelCapability {
            resolutions: vec!["480p".to_string(), "768p".to_string()],
            duration_min: 5,
            duration_max: 15,
            aspect_ratios,
            supports_size: false,
            supports_audio: false,
            supports_watermark: false,
            supports_seed: false,
            frame_images: vec![VideoFrameType::First, VideoFrameType::Last],
            max_frame_images: 1,
            input_references: true,
            reference_kinds: references,
        }),
        "minimax/hailuo-3" => Some(VideoModelCapability {
            resolutions: vec!["2K".to_string()],
            duration_min: 5,
            duration_max: 15,
            aspect_ratios,
            supports_size: false,
            supports_audio: true,
            supports_watermark: false,
            supports_seed: false,
            frame_images: vec![VideoFrameType::First, VideoFrameType::Last],
            max_frame_images: 1,
            input_references: true,
            reference_kinds: references,
        }),
        _ => None,
    }
}

/// `fallbackVideoModelCapability` (`video-generation.ts:79`): permissive default
/// that keeps plugin and legacy video models behaving as before.
fn fallback_capability() -> VideoModelCapability {
    VideoModelCapability {
        resolutions: Vec::new(),
        duration_min: 4,
        duration_max: 30,
        aspect_ratios: Vec::new(),
        supports_size: true,
        supports_audio: true,
        supports_watermark: true,
        supports_seed: true,
        frame_images: vec![VideoFrameType::First, VideoFrameType::Last],
        max_frame_images: 2,
        input_references: true,
        reference_kinds: vec![
            VideoReferenceKind::Image,
            VideoReferenceKind::Video,
            VideoReferenceKind::Audio,
        ],
    }
}

/// `videoModelCapability` (`video-generation.ts:102`).
pub fn video_model_capability(model: &str) -> VideoModelCapability {
    model_capability(model).unwrap_or_else(fallback_capability)
}

fn resolution_rank(value: &str) -> i64 {
    open_canvas_core::media_size::parse_video_resolution(value)
        .parse()
        .unwrap_or(720)
}

/// `supportedVideoResolution` (`video-generation.ts:115`): the model's exact
/// resolution, else the closest supported tier.
pub fn supported_video_resolution(
    value: &str,
    capability: &VideoModelCapability,
) -> Option<String> {
    if capability.resolutions.is_empty() {
        return None;
    }
    let current = resolution_rank(value);
    if let Some(exact) = capability
        .resolutions
        .iter()
        .find(|item| resolution_rank(item) == current)
    {
        return Some(exact.clone());
    }
    capability
        .resolutions
        .iter()
        .reduce(|best, item| {
            let best_gap = (resolution_rank(best) - current).abs();
            let item_gap = (resolution_rank(item) - current).abs();
            if item_gap < best_gap {
                item
            } else {
                best
            }
        })
        .cloned()
}

/// `videoModelDuration` (`video-generation.ts:122`): clamp seconds to the model's
/// window.
pub fn video_model_duration(value: &str, capability: &VideoModelCapability) -> i64 {
    let seconds = value
        .trim()
        .parse::<f64>()
        .map_or(6.0, |number| number.floor());
    let seconds = if seconds.is_finite() && seconds > 0.0 {
        seconds
    } else {
        6.0
    };
    let clamped = seconds.clamp(
        capability.duration_min as f64,
        capability.duration_max as f64,
    );
    clamped as i64
}

/// `openRouterInputReferences` kind check (`video.ts:331`).
pub fn validate_reference_kinds(
    capability: &VideoModelCapability,
    media: &VideoRequestMedia,
) -> Result<(), AiError> {
    let counts = [
        (VideoReferenceKind::Image, media.images.len()),
        (VideoReferenceKind::Video, media.videos.len()),
        (VideoReferenceKind::Audio, media.audios.len()),
    ];
    for (kind, count) in counts {
        if count > 0 && !capability.reference_kinds.contains(&kind) {
            return Err(AiError::invalid(format!(
                "This model does not accept {} references. Change the reference or the model.",
                kind.as_str()
            )));
        }
    }
    if !media.audios.is_empty() && media.images.is_empty() && media.videos.is_empty() {
        return Err(AiError::invalid(
            "Audio references cannot be used alone. Add at least one image or video reference.",
        ));
    }
    Ok(())
}

/// `assertReferenceSeconds` (`video.ts:344`).
pub fn assert_reference_seconds(
    item: &ReferenceMedia,
    kind: VideoReferenceKind,
) -> Result<(), AiError> {
    let Some(duration_ms) = item.duration_ms else {
        return Ok(());
    };
    let seconds = duration_ms as f64 / 1000.0;
    if !(VIDEO_REFERENCE_SECONDS_MIN..=VIDEO_REFERENCE_SECONDS_MAX).contains(&seconds) {
        return Err(AiError::invalid(format!(
            "The {} reference \"{}\" must be 2-15 seconds long.",
            kind.as_str(),
            item.name
        )));
    }
    Ok(())
}

/// `referenceMediaDataUrl` byte ceilings (`video.ts:360`).
pub fn assert_reference_size(
    item: &ReferenceMedia,
    kind: VideoReferenceKind,
    max_bytes: usize,
) -> Result<(), AiError> {
    if item.byte_len() > max_bytes {
        return Err(AiError::invalid(format!(
            "The {} reference \"{}\" exceeds the {}MB limit. Compress or replace it.",
            kind.as_str(),
            item.name,
            max_bytes / (1024 * 1024)
        )));
    }
    Ok(())
}

/// The canvas provider counts (`canvas-video-slots.ts:30`): at most 9 images,
/// 3 videos, 3 audios and 12 references in total.
pub fn validate_reference_limits(media: &VideoRequestMedia) -> Result<(), AiError> {
    let checks = [
        (
            VideoReferenceKind::Image,
            media.images.len(),
            VIDEO_REFERENCE_IMAGE_LIMIT,
        ),
        (
            VideoReferenceKind::Video,
            media.videos.len(),
            VIDEO_REFERENCE_VIDEO_LIMIT,
        ),
        (
            VideoReferenceKind::Audio,
            media.audios.len(),
            VIDEO_REFERENCE_AUDIO_LIMIT,
        ),
    ];
    for (kind, count, limit) in checks {
        if count > limit {
            return Err(AiError::invalid(format!(
                "At most {limit} {} references are allowed (got {count}).",
                kind.as_str()
            )));
        }
    }
    let total = media.images.len() + media.videos.len() + media.audios.len();
    if total > VIDEO_REFERENCE_TOTAL_LIMIT {
        return Err(AiError::invalid(format!(
            "At most {VIDEO_REFERENCE_TOTAL_LIMIT} references are allowed (got {total})."
        )));
    }
    Ok(())
}

fn openrouter_input_references(
    capability: &VideoModelCapability,
    media: &VideoRequestMedia,
) -> Result<Vec<Value>, AiError> {
    validate_reference_kinds(capability, media)?;
    for video in &media.videos {
        assert_reference_seconds(video, VideoReferenceKind::Video)?;
        assert_reference_size(video, VideoReferenceKind::Video, VIDEO_REFERENCE_MAX_BYTES)?;
    }
    for audio in &media.audios {
        assert_reference_seconds(audio, VideoReferenceKind::Audio)?;
        assert_reference_size(audio, VideoReferenceKind::Audio, AUDIO_REFERENCE_MAX_BYTES)?;
    }
    let mut parts: Vec<Value> = Vec::new();
    let mut request_bytes = 0usize;
    let mut push = |key: &str, url: String| {
        request_bytes += url.len();
        let mut part = serde_json::Map::new();
        part.insert("type".to_string(), json!(key));
        part.insert(key.to_string(), json!({ "url": url }));
        parts.push(Value::Object(part));
    };
    for image in &media.images {
        push("image_url", image.data_url());
    }
    for video in &media.videos {
        push("video_url", video.data_url());
    }
    for audio in &media.audios {
        push("audio_url", audio.data_url());
    }
    if request_bytes > VIDEO_REFERENCE_REQUEST_MAX_BYTES {
        return Err(AiError::invalid(
            "References exceed the 64MB request limit (base64 adds about 33%). Remove or compress some references.",
        ));
    }
    Ok(parts)
}

/// `openRouterFrameImages` (`video.ts:307`): explicit slots keep their identity;
/// a lone last frame is never sent as the first frame, and the result is capped
/// at `capability.max_frame_images`.
pub fn openrouter_frame_images(
    capability: &VideoModelCapability,
    media: &VideoRequestMedia,
) -> Vec<Value> {
    if capability.frame_images.is_empty() {
        return Vec::new();
    }
    let frame_value = |item: &ReferenceMedia, frame_type: VideoFrameType| {
        json!({
            "type": "image_url",
            "image_url": { "url": item.data_url() },
            "frame_type": frame_type.as_str(),
        })
    };
    let mut entries: Vec<Value> = Vec::new();
    if media.has_explicit_frames() {
        if let Some(item) = &media.first_frame {
            if capability.frame_images.contains(&VideoFrameType::First) {
                entries.push(frame_value(item, VideoFrameType::First));
            }
        }
        if let Some(item) = &media.last_frame {
            if capability.frame_images.contains(&VideoFrameType::Last) {
                entries.push(frame_value(item, VideoFrameType::Last));
            }
        }
    } else {
        for (index, item) in media.images.iter().take(2).enumerate() {
            if let Some(frame_type) = capability.frame_images.get(index) {
                entries.push(frame_value(item, *frame_type));
            }
        }
    }
    entries.truncate(capability.max_frame_images);
    entries
}

/// `createOpenRouterVideoTask` body (`video.ts:284`), media-aware.
pub fn openrouter_video_body_media(
    req: &ModelRequestConfig,
    prompt: &str,
    media: &VideoRequestMedia,
) -> Result<Value, AiError> {
    validate_reference_limits(media)?;
    let capability = video_model_capability(&req.model);
    let mode = if media.has_explicit_frames() {
        VideoMode::Frames
    } else {
        resolve_video_mode(&req.video_mode, media.images.len())
    };
    let frame_images = if mode == VideoMode::Frames {
        openrouter_frame_images(&capability, media)
    } else {
        Vec::new()
    };
    let input_references = if mode == VideoMode::Reference {
        openrouter_input_references(&capability, media)?
    } else {
        Vec::new()
    };
    let mut body = json!({
        "model": model_option_name(&req.model),
        "prompt": prompt,
        "duration": video_model_duration(&req.video_seconds, &capability),
        "resolution": supported_video_resolution(&req.vquality, &capability)
            .unwrap_or_else(|| normalize_video_resolution(&req.vquality)),
        "aspect_ratio": video_aspect_ratio(&req.size),
    });
    if !frame_images.is_empty() {
        body["frame_images"] = Value::Array(frame_images);
    }
    if !input_references.is_empty() {
        body["input_references"] = Value::Array(input_references);
    }
    if capability.supports_audio {
        body["generate_audio"] = json!(bool_config(&req.video_generate_audio, true));
    }
    Ok(body)
}

fn media_part(
    media: &ReferenceMedia,
    fallback_name: &str,
) -> Result<reqwest::multipart::Part, AiError> {
    let name = if media.name.is_empty() {
        fallback_name.to_string()
    } else {
        media.name.clone()
    };
    reqwest::multipart::Part::bytes(media.bytes.clone())
        .file_name(name)
        .mime_str(&media.mime)
        .map_err(|error| AiError::invalid(format!("Invalid reference MIME type: {error}")))
}

/// `createOpenAIVideoTask` multipart form (`video.ts:243`), media-aware: frames
/// mode sends `first_frame` / `last_frame`; reference mode sends `image[]`, and
/// videos/audios always append as `video[]` / `audio[]`.
pub fn openai_video_form(
    req: &ModelRequestConfig,
    prompt: &str,
    media: &VideoRequestMedia,
) -> Result<reqwest::multipart::Form, AiError> {
    validate_reference_limits(media)?;
    let mode = resolve_video_mode(&req.video_mode, media.images.len());
    let size =
        normalize_video_size(&req.size, &req.vquality).unwrap_or_else(|| "1280x720".to_string());
    let mut form = reqwest::multipart::Form::new()
        .text("model", model_option_name(&req.model))
        .text("prompt", prompt.to_string())
        .text(
            "seconds",
            open_canvas_core::media_size::clamp_video_seconds(&req.video_seconds).to_string(),
        )
        .text("size", size)
        .text("resolution_name", normalize_video_resolution(&req.vquality))
        .text(
            "generate_audio",
            bool_config(&req.video_generate_audio, true).to_string(),
        )
        .text(
            "watermark",
            bool_config(&req.video_watermark, false).to_string(),
        )
        .text("mode", mode.as_str());
    if mode == VideoMode::Frames {
        // Explicit slots win; otherwise fall back to the web's positional images.
        if let Some(first) = media.first_frame.as_ref().or_else(|| media.images.first()) {
            form = form.part("first_frame", media_part(first, "first.png")?);
        }
        if let Some(last) = media.last_frame.as_ref().or_else(|| media.images.get(1)) {
            form = form.part("last_frame", media_part(last, "last.png")?);
        }
    } else {
        for (index, image) in media.images.iter().enumerate() {
            let named = ReferenceMedia {
                name: if image.name.is_empty() {
                    format!("ref-{index}.png")
                } else {
                    image.name.clone()
                },
                ..image.clone()
            };
            form = form.part("image[]", media_part(&named, "ref.png")?);
        }
    }
    for video in &media.videos {
        form = form.part("video[]", media_part(video, "ref.mp4")?);
    }
    for audio in &media.audios {
        form = form.part("audio[]", media_part(audio, "ref.mp3")?);
    }
    Ok(form)
}

/// Resolve a JSON dot path (`a.b.0.c`) against a value, mirroring the web's
/// bracket-free property reads (`record.video_url`, `record.usage.cost`).
pub(crate) fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ModelRequestConfig {
        ModelRequestConfig {
            model: "minimax/hailuo-3".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: "sk-test".to_string(),
            system_prompt: String::new(),
            reasoning_effort: crate::config::ReasoningEffort::Auto,
            quality: String::new(),
            size: "16:9".to_string(),
            background: String::new(),
            count: 1,
            video_seconds: "6".to_string(),
            vquality: "720".to_string(),
            video_generate_audio: "true".to_string(),
            video_watermark: "false".to_string(),
            video_mode: "frames".to_string(),
        }
    }

    fn media(name: &str, mime: &str, bytes: &[u8]) -> ReferenceMedia {
        ReferenceMedia::new(name, mime, bytes.to_vec())
    }

    fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
        match value {
            Ok(value) => value,
            Err(error) => unreachable!("unexpected error: {error:?}"),
        }
    }

    #[test]
    fn mode_resolution_matches_the_web() {
        assert_eq!(resolve_video_mode("reference", 1), VideoMode::Reference);
        assert_eq!(resolve_video_mode("frames", 3), VideoMode::Reference);
        assert_eq!(resolve_video_mode("frames", 2), VideoMode::Frames);
        assert_eq!(normalize_video_mode("reference"), VideoMode::Reference);
        assert_eq!(normalize_video_mode("anything"), VideoMode::Frames);
    }

    #[test]
    fn capability_lookup_selects_the_hailuo_pair_else_fallback() {
        let max = video_model_capability("minimax/hailuo-3-max");
        assert!(!max.supports_audio);
        assert_eq!(max.resolutions, vec!["480p", "768p"]);
        let base = video_model_capability("minimax/hailuo-3");
        assert!(base.supports_audio);
        assert_eq!(base.max_frame_images, 1);
        let fallback = video_model_capability("sora-2");
        assert_eq!(fallback.max_frame_images, 2);
        assert!(fallback.supports_size);
    }

    #[test]
    fn supported_resolution_matches_exactly_or_closest() {
        let cap = video_model_capability("minimax/hailuo-3");
        assert_eq!(
            supported_video_resolution("2k", &cap),
            Some("2K".to_string())
        );
        assert_eq!(
            supported_video_resolution("720", &cap),
            Some("2K".to_string())
        );
        let none = video_model_capability("sora-2");
        assert_eq!(supported_video_resolution("720", &none), None);
    }

    #[test]
    fn duration_clamps_to_capability_window() {
        let cap = video_model_capability("minimax/hailuo-3-max");
        assert_eq!(video_model_duration("2", &cap), 5);
        assert_eq!(video_model_duration("40", &cap), 15);
        assert_eq!(video_model_duration("7", &cap), 7);
    }

    #[test]
    fn reference_media_round_trips_through_a_data_url() {
        let item = media("ref.png", "image/png", &[0, 1, 2, 3]);
        let decoded = ok(ReferenceMedia::from_data_url(&item.data_url()));
        assert_eq!(decoded.mime, "image/png");
        assert_eq!(decoded.bytes, vec![0, 1, 2, 3]);
        assert!(ReferenceMedia::from_data_url("https://x/y.png").is_err());
    }

    #[test]
    fn reference_kind_and_audio_only_are_rejected() {
        let mut cap = video_model_capability("minimax/hailuo-3");
        cap.reference_kinds = vec![VideoReferenceKind::Image];
        let with_video = VideoRequestMedia {
            videos: vec![media("v.mp4", "video/mp4", &[1])],
            ..VideoRequestMedia::default()
        };
        assert!(validate_reference_kinds(&cap, &with_video).is_err());
        let audio_only = VideoRequestMedia {
            audios: vec![media("a.mp3", "audio/mpeg", &[1])],
            ..VideoRequestMedia::default()
        };
        let full = video_model_capability("minimax/hailuo-3");
        let error = validate_reference_kinds(&full, &audio_only).err();
        assert!(matches!(error, Some(AiError::Invalid(message)) if message.contains("alone")));
    }

    #[test]
    fn reference_seconds_window_is_enforced() {
        let short = media("a.mp3", "audio/mpeg", &[1]).with_duration_ms(1_000);
        assert!(assert_reference_seconds(&short, VideoReferenceKind::Audio).is_err());
        let good = media("a.mp3", "audio/mpeg", &[1]).with_duration_ms(3_000);
        assert!(assert_reference_seconds(&good, VideoReferenceKind::Audio).is_ok());
        let none = media("a.mp3", "audio/mpeg", &[1]);
        assert!(assert_reference_seconds(&none, VideoReferenceKind::Audio).is_ok());
    }

    #[test]
    fn provider_counts_are_enforced() {
        let too_many = VideoRequestMedia {
            images: (0..10)
                .map(|i| media(&format!("i{i}"), "image/png", &[1]))
                .collect(),
            ..VideoRequestMedia::default()
        };
        assert!(validate_reference_limits(&too_many).is_err());
        let total = VideoRequestMedia {
            images: (0..9)
                .map(|i| media(&format!("i{i}"), "image/png", &[1]))
                .collect(),
            videos: (0..3)
                .map(|i| media(&format!("v{i}"), "video/mp4", &[1]))
                .collect(),
            audios: vec![media("a", "audio/mpeg", &[1])],
            ..VideoRequestMedia::default()
        };
        assert!(validate_reference_limits(&total).is_err());
    }

    #[test]
    fn frame_images_keep_slot_identity_and_respect_max() {
        let cap = video_model_capability("minimax/hailuo-3");
        let explicit = VideoRequestMedia {
            first_frame: Some(media("l.png", "image/png", &[9])),
            ..VideoRequestMedia::default()
        };
        let entries = openrouter_frame_images(&cap, &explicit);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["frame_type"], json!("first_frame"));
        // Two explicit frames collapse to one for a single-keyframe model.
        let both = VideoRequestMedia {
            first_frame: Some(media("f.png", "image/png", &[1])),
            last_frame: Some(media("l.png", "image/png", &[2])),
            ..VideoRequestMedia::default()
        };
        assert_eq!(openrouter_frame_images(&cap, &both).len(), 1);
        // Positional fallback binds slots in capability order.
        let positional = VideoRequestMedia {
            images: vec![media("a.png", "image/png", &[1])],
            ..VideoRequestMedia::default()
        };
        let entries = openrouter_frame_images(&cap, &positional);
        assert_eq!(entries[0]["frame_type"], json!("first_frame"));
    }

    #[test]
    fn openrouter_body_carries_frames_or_references() {
        let explicit = VideoRequestMedia {
            first_frame: Some(media("f.png", "image/png", &[1])),
            ..VideoRequestMedia::default()
        };
        let body = ok(openrouter_video_body_media(&request(), "a wave", &explicit));
        assert_eq!(body["frame_images"][0]["type"], json!("image_url"));
        assert!(body.get("input_references").is_none());

        let reference = VideoRequestMedia {
            images: vec![media("i.png", "image/png", &[1])],
            videos: vec![media("v.mp4", "video/mp4", &[2]).with_duration_ms(3_000)],
            ..VideoRequestMedia::default()
        };
        let mut reference_request = request();
        reference_request.video_mode = "reference".to_string();
        let body = ok(openrouter_video_body_media(
            &reference_request,
            "a wave",
            &reference,
        ));
        assert!(body.get("frame_images").is_none());
        assert_eq!(body["input_references"][0]["type"], json!("image_url"));
        assert_eq!(body["input_references"][1]["type"], json!("video_url"));
    }

    #[test]
    fn json_path_reads_nested_arrays() {
        let value = json!({"a": {"b": [{"c": 7}]}});
        assert_eq!(json_path(&value, "a.b.0.c"), Some(&json!(7)));
        assert_eq!(json_path(&value, "a.missing"), None);
    }
}
