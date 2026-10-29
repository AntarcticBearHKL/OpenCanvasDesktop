//! Video generation create/poll, ported from `web/src/services/api/video.ts`.
//!
//! Two providers share one entry point: `openai` (multipart `POST /videos` +
//! `GET /videos/{id}`) and `openrouter` (JSON `POST /videos` with a `polling_url`,
//! `GET {polling_url}` / `/videos/{id}`). The poll loop is faithful to the web:
//! 120 attempts × 2500 ms ≈ 300 s, abortable at every step.
//!
//! The full request surface (first/last frames, reference images/videos/audios,
//! per-model capability resolution and validation) lives in
//! [`crate::video_request`]; the scriptable `plugin` provider is modelled as a
//! caller-supplied declarative config in [`crate::plugin_video`].

use std::time::Duration;

#[cfg(test)]
use base64::Engine as _;
use reqwest::Client;
use serde_json::{json, Value};

use crate::client::{bearer, send_cancelable, Cancel, REQUEST_TIMEOUT};
use crate::config::{build_api_url, model_option_name, ModelRequestConfig};
use crate::error::{map_reqwest_error, read_api_error_message, read_http_error, AiError};
use crate::video_request::{self, VideoRequestMedia};

/// `waitForVideoGenerationTask` attempt budget (`video.ts:98`).
pub const VIDEO_POLL_ATTEMPTS: u32 = 120;
/// `waitForVideoGenerationTask` delay (`video.ts:121`).
pub const VIDEO_POLL_DELAY_MS: u64 = 2500;

/// Which backend owns a video task (`createVideoGenerationTask`, `video.ts:140`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoProvider {
    OpenAi,
    OpenRouter,
}

/// A created video job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTask {
    pub id: String,
    pub provider: VideoProvider,
    pub model: String,
    pub polling_url: Option<String>,
}

/// A terminal or pending poll result (`VideoGenerationTaskState`, `video.ts:29`).
#[derive(Debug, Clone, PartialEq)]
pub enum VideoState {
    Pending,
    Completed(VideoResult),
    Failed(String),
}

/// A finished video (`VideoGenerationResult`, `video.ts:27`): inline bytes when the
/// content endpoint answered, else a download URL.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoResult {
    pub bytes: Option<Vec<u8>>,
    pub url: Option<String>,
    pub mime: Option<String>,
    pub cost_usd: Option<f64>,
}

/// `isOpenRouterVideoModel` (`video-generation.ts:96`): the OpenRouter video models
/// are the hailuo-3 pair.
pub fn is_openrouter_video_model(model: &str) -> bool {
    let model = model.trim();
    model.ends_with("hailuo-3-max") || model.ends_with("hailuo-3")
}

/// `boolConfig` (`use-config-store.ts:127`).
pub fn bool_config(value: &str, fallback: bool) -> bool {
    if value.is_empty() {
        fallback
    } else {
        value == "true"
    }
}

/// `normalizeVideoResolution` (`video.ts:468`).
pub fn normalize_video_resolution(value: &str) -> String {
    match value {
        "low" => "480p".to_string(),
        "auto" | "high" | "medium" => "720p".to_string(),
        other => {
            let stripped = other.trim_end_matches(['p', 'P']);
            if stripped.is_empty() {
                "720p".to_string()
            } else {
                format!("{stripped}p")
            }
        }
    }
}

/// `normalizeVideoSize` (`video.ts:460`).
pub fn normalize_video_size(value: &str, resolution: &str) -> Option<String> {
    if value == "auto" {
        return None;
    }
    if is_pixel_size(value) {
        return Some(value.to_string());
    }
    let ratio = open_canvas_core::media_size::infer_video_ratio(value);
    if ratio == "auto" {
        return None;
    }
    Some(open_canvas_core::media_size::compute_video_size(
        resolution, ratio,
    ))
}

/// `videoAspectRatio` (`video.ts:430`).
pub fn video_aspect_ratio(size: &str) -> String {
    let ratio = open_canvas_core::media_size::infer_video_ratio(size);
    if ratio == "auto" {
        "16:9".to_string()
    } else {
        ratio.to_string()
    }
}

fn is_pixel_size(value: &str) -> bool {
    value.split_once(['x', 'X']).is_some_and(|(w, h)| {
        !w.is_empty()
            && !h.is_empty()
            && w.chars().all(|c| c.is_ascii_digit())
            && h.chars().all(|c| c.is_ascii_digit())
    })
}

/// `unwrapEnvelope` (`video.ts:479`): an `{code, data}` wrapper unwraps to `data`
/// (a non-zero code is an error); a bare object is returned as-is.
pub fn unwrap_envelope(payload: &Value, empty_message: &str) -> Result<Value, AiError> {
    if payload.is_null() {
        return Err(AiError::Api(empty_message.to_string()));
    }
    if let Some(code) = payload.get("code") {
        let ok = code.as_i64() == Some(0) || code.as_str() == Some("0");
        if !ok {
            let message = read_api_error_message(payload);
            return Err(AiError::Api(if message.is_empty() {
                "Request failed".to_string()
            } else {
                message
            }));
        }
        return payload
            .get("data")
            .filter(|data| !data.is_null())
            .cloned()
            .ok_or_else(|| AiError::Api(empty_message.to_string()));
    }
    Ok(payload.clone())
}

/// `videoResultUrl` (`video.ts:489`): the first playable media URL of a poll body.
pub fn video_result_url(payload: &Value) -> Option<String> {
    let candidates = [
        payload.get("video_url"),
        payload.get("result_url"),
        payload.get("url"),
        payload
            .get("content")
            .and_then(|content| content.get("video_url")),
        payload
            .get("content")
            .and_then(|content| content.get("url")),
    ];
    candidates.into_iter().flatten().find_map(|value| {
        value
            .as_str()
            .filter(|url| {
                url.starts_with("http://") || url.starts_with("https://") || url.contains(".mp4")
            })
            .map(str::to_string)
    })
}

/// `createOpenRouterVideoTask` body (`video.ts:284`).
pub fn openrouter_video_body(
    req: &ModelRequestConfig,
    prompt: &str,
    references: &[String],
) -> Value {
    let mut body = json!({
        "model": model_option_name(&req.model),
        "prompt": prompt,
        "duration": open_canvas_core::media_size::clamp_video_seconds(&req.video_seconds),
        "resolution": normalize_video_resolution(&req.vquality),
        "aspect_ratio": video_aspect_ratio(&req.size),
        "generate_audio": bool_config(&req.video_generate_audio, true),
    });
    if !references.is_empty() {
        let parts: Vec<Value> = references
            .iter()
            .map(|url| json!({ "type": "image_url", "image_url": { "url": url } }))
            .collect();
        body["input_references"] = Value::Array(parts);
    }
    body
}

#[cfg(test)]
fn data_url_bytes(url: &str) -> Option<(Vec<u8>, String)> {
    let (meta, payload) = url.strip_prefix("data:")?.split_once(',')?;
    let mime = meta.split(';').next().unwrap_or("image/png").to_string();
    if !meta.contains(";base64") {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    Some((bytes, mime))
}

/// `createVideoGenerationTask` (`video.ts:136`) for the legacy reference-image
/// entry point: the data URLs become [`ReferenceMedia`](crate::video_request::ReferenceMedia)
/// images and flow through the full media-aware path.
pub async fn create_video_task(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    references: &[String],
    cancel: &Cancel,
) -> Result<VideoTask, AiError> {
    let media = VideoRequestMedia::from_reference_data_urls(references)?;
    create_video_task_with_media(client, req, prompt, &media, cancel).await
}

/// `createVideoGenerationTask` (`video.ts:136`) with the full media surface:
/// first/last frames, reference images, videos and audios.
pub async fn create_video_task_with_media(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    media: &VideoRequestMedia,
    cancel: &Cancel,
) -> Result<VideoTask, AiError> {
    assert_video_config(req)?;
    if is_openrouter_video_model(&req.model) {
        create_openrouter_task(client, req, prompt, media, cancel).await
    } else {
        create_openai_task(client, req, prompt, media, cancel).await
    }
}

async fn create_openrouter_task(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    media: &VideoRequestMedia,
    cancel: &Cancel,
) -> Result<VideoTask, AiError> {
    let body = video_request::openrouter_video_body_media(req, prompt, media)?;
    let url = build_api_url(&req.base_url, "/videos");
    let builder = client
        .post(&url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key))
        .json(&body);
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| map_reqwest_error(&error))?;
    if !status.is_success() {
        return Err(read_http_error(status.as_u16(), &text));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AiError::Parse(format!("video create response: {error}")))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| AiError::Api("The provider returned no video task id".to_string()))?;
    Ok(VideoTask {
        id: id.to_string(),
        provider: VideoProvider::OpenRouter,
        model: req.model.clone(),
        polling_url: value
            .get("polling_url")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

async fn create_openai_task(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    media: &VideoRequestMedia,
    cancel: &Cancel,
) -> Result<VideoTask, AiError> {
    let form = video_request::openai_video_form(req, prompt, media)?;
    let url = build_api_url(&req.base_url, "/videos");
    let builder = client
        .post(&url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key))
        .multipart(form);
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| map_reqwest_error(&error))?;
    if !status.is_success() {
        return Err(read_http_error(status.as_u16(), &text));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AiError::Parse(format!("video create response: {error}")))?;
    let created = unwrap_envelope(&value, "The provider returned no video task")?;
    let id = created
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| AiError::Api("The provider returned no video task id".to_string()))?;
    Ok(VideoTask {
        id: id.to_string(),
        provider: VideoProvider::OpenAi,
        model: req.model.clone(),
        polling_url: None,
    })
}

/// `pollVideoGenerationTask` (`video.ts:158`): one poll attempt.
pub async fn poll_video_task(
    client: &Client,
    req: &ModelRequestConfig,
    task: &VideoTask,
    cancel: &Cancel,
) -> Result<VideoState, AiError> {
    match task.provider {
        VideoProvider::OpenRouter => poll_openrouter(client, req, task, cancel).await,
        VideoProvider::OpenAi => poll_openai(client, req, task, cancel).await,
    }
}

async fn get_json(
    client: &Client,
    url: &str,
    req: &ModelRequestConfig,
    cancel: &Cancel,
) -> Result<Value, AiError> {
    let builder = client
        .get(url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key));
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| map_reqwest_error(&error))?;
    if !status.is_success() {
        return Err(read_http_error(status.as_u16(), &text));
    }
    serde_json::from_str(&text)
        .map_err(|error| AiError::Parse(format!("video poll response: {error}")))
}

pub(crate) async fn download(
    client: &Client,
    url: &str,
    req: &ModelRequestConfig,
    cancel: &Cancel,
) -> Result<Vec<u8>, AiError> {
    let builder = client
        .get(url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key));
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(read_http_error(status.as_u16(), &text));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| map_reqwest_error(&error))?;
    Ok(bytes.to_vec())
}

async fn poll_openai(
    client: &Client,
    req: &ModelRequestConfig,
    task: &VideoTask,
    cancel: &Cancel,
) -> Result<VideoState, AiError> {
    let url = build_api_url(&req.base_url, &format!("/videos/{}", task.id));
    let value = get_json(client, &url, req, cancel).await?;
    let video = unwrap_envelope(&value, "The provider returned no video task")?;
    if let Some(url) = video_result_url(&video) {
        return Ok(VideoState::Completed(VideoResult {
            url: Some(url),
            mime: Some("video/mp4".to_string()),
            ..VideoResult::default()
        }));
    }
    match video.get("status").and_then(Value::as_str) {
        Some("completed") => {
            let content_url = build_api_url(&req.base_url, &format!("/videos/{}/content", task.id));
            let bytes = download(client, &content_url, req, cancel).await?;
            Ok(VideoState::Completed(VideoResult {
                bytes: Some(bytes),
                mime: Some("video/mp4".to_string()),
                ..VideoResult::default()
            }))
        }
        Some("failed" | "cancelled") => Ok(VideoState::Failed(
            read_api_error_message(&video).into_empty_then("Video generation failed"),
        )),
        _ => Ok(VideoState::Pending),
    }
}

async fn poll_openrouter(
    client: &Client,
    req: &ModelRequestConfig,
    task: &VideoTask,
    cancel: &Cancel,
) -> Result<VideoState, AiError> {
    let url = resolve_polling_url(req, task);
    let video = get_json(client, &url, req, cancel).await?;
    match video.get("status").and_then(Value::as_str) {
        Some("completed") => {
            let cost = video
                .get("usage")
                .and_then(|usage| usage.get("cost"))
                .and_then(Value::as_f64);
            let content_url = build_api_url(
                &req.base_url,
                &format!("/videos/{}/content?index=0", task.id),
            );
            match download(client, &content_url, req, cancel).await {
                Ok(bytes) => Ok(VideoState::Completed(VideoResult {
                    bytes: Some(bytes),
                    mime: Some("video/mp4".to_string()),
                    cost_usd: cost,
                    ..VideoResult::default()
                })),
                Err(error) if error.is_canceled() => Err(error),
                Err(_) => {
                    let fallback = video
                        .get("unsigned_urls")
                        .and_then(Value::as_array)
                        .and_then(|urls| urls.first())
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    match fallback {
                        Some(url) => Ok(VideoState::Completed(VideoResult {
                            url: Some(url),
                            mime: Some("video/mp4".to_string()),
                            cost_usd: cost,
                            ..VideoResult::default()
                        })),
                        None => Err(AiError::Api("Video download failed".to_string())),
                    }
                }
            }
        }
        Some("failed" | "cancelled" | "expired") => {
            let message = read_api_error_message(&video);
            Ok(VideoState::Failed(
                message.into_empty_then("Video generation failed"),
            ))
        }
        _ => Ok(VideoState::Pending),
    }
}

fn resolve_polling_url(req: &ModelRequestConfig, task: &VideoTask) -> String {
    match task.polling_url.as_deref() {
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => url.to_string(),
        // A leading-slash path resolves against the origin (the web's
        // `new URL(pollingUrl, base` semantics), a bare path against `/v1/`.
        Some(relative) if relative.starts_with('/') => {
            format!("{}{relative}", origin_of(&req.base_url))
        }
        Some(relative) => format!(
            "{}{}",
            build_api_url(&req.base_url, "/"),
            relative.trim_start_matches('/')
        ),
        None => build_api_url(&req.base_url, &format!("/videos/{}", task.id)),
    }
}

/// `scheme://host[:port]` of a base URL, dropping any path.
pub(crate) fn origin_of(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if let Some(scheme_end) = trimmed.find("://") {
        let after = trimmed.get(scheme_end + 3..).unwrap_or_default();
        if let Some(slash) = after.find('/') {
            return trimmed
                .get(..scheme_end + 3 + slash)
                .unwrap_or(trimmed)
                .to_string();
        }
    }
    trimmed.trim_end_matches('/').to_string()
}

/// `waitForVideoGenerationTask` (`video.ts:97`): poll until terminal, up to
/// [`VIDEO_POLL_ATTEMPTS`] × [`VIDEO_POLL_DELAY_MS`].
pub async fn wait_for_video_task(
    client: &Client,
    req: &ModelRequestConfig,
    task: &VideoTask,
    cancel: &Cancel,
) -> Result<VideoResult, AiError> {
    for attempt in 0..VIDEO_POLL_ATTEMPTS {
        if cancel.is_canceled() {
            return Err(AiError::Canceled);
        }
        match poll_video_task(client, req, task, cancel).await? {
            VideoState::Completed(result) => return Ok(result),
            VideoState::Failed(error) => return Err(AiError::VideoFailed(error)),
            VideoState::Pending => {}
        }
        if attempt + 1 >= VIDEO_POLL_ATTEMPTS {
            return Err(AiError::Timeout("The video task timed out".to_string()));
        }
        tokio::select! {
            () = cancel.cancelled() => return Err(AiError::Canceled),
            () = tokio::time::sleep(Duration::from_millis(VIDEO_POLL_DELAY_MS)) => {}
        }
    }
    Err(AiError::Timeout("The video task timed out".to_string()))
}

/// `assertVideoConfig` (`video.ts:424`).
pub(crate) fn assert_video_config(req: &ModelRequestConfig) -> Result<(), AiError> {
    if req.model.trim().is_empty() {
        return Err(AiError::invalid("Select a video model first"));
    }
    if req.base_url.trim().is_empty() {
        return Err(AiError::invalid("Set a provider base URL first"));
    }
    if req.api_key.trim().is_empty() {
        return Err(AiError::invalid("Set a provider API key first"));
    }
    Ok(())
}

trait EmptyThen {
    fn into_empty_then(self, fallback: &str) -> String;
}

impl EmptyThen for String {
    fn into_empty_then(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReasoningEffort;

    fn request() -> ModelRequestConfig {
        ModelRequestConfig {
            model: "minimax/hailuo-3".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: "sk-test".to_string(),
            system_prompt: String::new(),
            reasoning_effort: ReasoningEffort::Auto,
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

    fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
        match value {
            Ok(value) => value,
            Err(error) => unreachable!("unexpected error: {error:?}"),
        }
    }

    #[test]
    fn openrouter_models_are_the_hailuo_pair() {
        assert!(is_openrouter_video_model("minimax/hailuo-3"));
        assert!(is_openrouter_video_model("minimax/hailuo-3-max"));
        assert!(!is_openrouter_video_model("sora-2"));
        assert!(!is_openrouter_video_model(""));
    }

    #[test]
    fn resolution_and_size_normalization_match_the_web() {
        assert_eq!(normalize_video_resolution("low"), "480p");
        assert_eq!(normalize_video_resolution("auto"), "720p");
        assert_eq!(normalize_video_resolution("1080"), "1080p");
        assert_eq!(normalize_video_size("auto", "720"), None);
        assert_eq!(
            normalize_video_size("1280x720", "720"),
            Some("1280x720".to_string())
        );
        assert_eq!(
            normalize_video_size("16:9", "720"),
            Some("1280x720".to_string())
        );
    }

    #[test]
    fn bool_config_defaults_on_empty() {
        assert!(bool_config("", true));
        assert!(!bool_config("", false));
        assert!(bool_config("true", false));
        assert!(!bool_config("false", true));
    }

    #[test]
    fn openrouter_body_carries_the_core_fields_and_references() {
        let body = openrouter_video_body(
            &request(),
            "a wave",
            &["data:image/png;base64,AA".to_string()],
        );
        assert_eq!(body["model"], json!("minimax/hailuo-3"));
        assert_eq!(body["duration"], json!(6));
        assert_eq!(body["resolution"], json!("720p"));
        assert_eq!(body["aspect_ratio"], json!("16:9"));
        assert_eq!(body["input_references"][0]["type"], json!("image_url"));
    }

    #[test]
    fn envelope_unwrap_handles_bare_and_wrapped_payloads() {
        let bare = ok(unwrap_envelope(&json!({"id": "t1"}), "empty"));
        assert_eq!(bare["id"], json!("t1"));
        let wrapped = ok(unwrap_envelope(
            &json!({"code": 0, "data": {"id": "t2"}}),
            "empty",
        ));
        assert_eq!(wrapped["id"], json!("t2"));
        assert!(
            matches!(unwrap_envelope(&json!({"code": 5, "msg": "bad"}), "empty"), Err(AiError::Api(message)) if message == "bad")
        );
        assert!(
            matches!(unwrap_envelope(&json!({"code": 0, "data": null}), "empty"), Err(AiError::Api(message)) if message == "empty")
        );
    }

    #[test]
    fn video_result_url_prefers_mp4_shapes() {
        assert_eq!(
            video_result_url(&json!({"video_url": "https://x/v.mp4"})),
            Some("https://x/v.mp4".to_string())
        );
        assert_eq!(
            video_result_url(&json!({"content": {"url": "https://x/c.mp4"}})),
            Some("https://x/c.mp4".to_string())
        );
        assert_eq!(video_result_url(&json!({"status": "pending"})), None);
    }

    #[test]
    fn data_url_bytes_decodes_base64() {
        let decoded = data_url_bytes("data:image/png;base64,AAEC");
        let (bytes, mime) = decoded.unwrap_or_else(|| unreachable!("valid data url"));
        assert_eq!(bytes, vec![0, 1, 2]);
        assert_eq!(mime, "image/png");
        assert!(data_url_bytes("https://x/y.png").is_none());
    }
}
