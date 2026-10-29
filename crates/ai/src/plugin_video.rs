//! Declarative replacement for the web's scriptable video provider
//! (`web/src/services/api/model-plugin.ts` + the `plugin` branch of
//! `video.ts`).
//!
//! The web runs a user-authored JavaScript function (`new Function(...)`) with an
//! `http`/`request`/`poll` helper surface. The desktop has no JS runtime, so the
//! same capability is modelled as data the caller supplies:
//!
//! * an **endpoint template** (`/videos`, `https://host/v1/jobs`, `{{...}}` placeholders);
//! * a **JSON body template** where `{{name}}` placeholders are substituted from
//!   [`plugin_video_vars`] (a placeholder that is the whole string takes the raw
//!   JSON value, so arrays/objects embed correctly);
//! * a **poll shape** (endpoint template, `id`/status/result dot paths, terminal
//!   statuses, interval, timeout).
//!
//! [`create_plugin_video_task`] then performs the create POST and the poll loop
//! itself, returning a [`VideoResult`] exactly like the other providers.
//!
//! What this covers versus the web script: the templated create request, the
//! create→poll flow, and result download. What it deliberately does **not**
//! cover: arbitrary branching, request signing/crypto, custom auth per request,
//! multipart bodies, and multi-request fan-out — those required the JS engine.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};

use crate::client::{bearer, send_cancelable, Cancel, REQUEST_TIMEOUT};
use crate::config::{build_api_url, ModelRequestConfig};
use crate::error::{map_reqwest_error, read_api_error_message, read_http_error, AiError};
use crate::video::{
    assert_video_config, bool_config, download, normalize_video_resolution, normalize_video_size,
    origin_of, video_aspect_ratio, VideoResult,
};
use crate::video_request::{
    json_path, resolve_video_mode, video_model_capability, video_model_duration, VideoMode,
    VideoRequestMedia,
};

/// A template variable: plain text, or a JSON value for exact-placeholder
/// substitution inside a body template.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginVar {
    Text(String),
    Json(Value),
}

impl PluginVar {
    /// The textual form (JSON renders compactly; a JSON string renders bare).
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Json(Value::String(text)) => text.clone(),
            Self::Json(value) => value.to_string(),
        }
    }
}

/// `poll` shape (`model-plugin.ts:createPoll`): where to poll, which statuses are
/// terminal, and which dot path yields the result URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginVideoPoll {
    /// Poll endpoint template (`{{id}}` is substituted with the created task id).
    pub endpoint: String,
    /// Dot path to the status string in the poll body.
    pub status_path: String,
    /// Statuses that mean success.
    pub completed: Vec<String>,
    /// Statuses that mean failure.
    pub failed: Vec<String>,
    /// Dot path to the result URL in the poll body.
    pub result_url_path: String,
    /// Dot path to the task id in the create response.
    pub id_path: String,
    /// Poll interval (default 2500 ms, matching the web script default).
    pub interval_ms: u64,
    /// Overall poll deadline (default 300000 ms, matching the web script default).
    pub timeout_ms: u64,
}

impl Default for PluginVideoPoll {
    fn default() -> Self {
        Self {
            endpoint: "/videos/{{id}}".to_string(),
            status_path: "status".to_string(),
            completed: vec!["completed".to_string()],
            failed: vec!["failed".to_string(), "cancelled".to_string()],
            result_url_path: "video_url".to_string(),
            id_path: "id".to_string(),
            interval_ms: 2500,
            timeout_ms: 300_000,
        }
    }
}

/// A caller-supplied declarative video provider (the data equivalent of the
/// web's per-model `script`).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginVideoProvider {
    /// Create endpoint template.
    pub endpoint: String,
    /// Create JSON body template.
    pub body: Value,
    /// Poll shape.
    pub poll: PluginVideoPoll,
}

/// Parse the declarative JSON script a channel model carries in place of the
/// web's JavaScript (`resolveModelScript`): `{ "endpoint", "body", "poll" }`.
/// The endpoint is required; `body` defaults to an empty object and `poll` to
/// [`PluginVideoPoll::default`] with only the fields the script provides
/// overridden. A script that is not this JSON shape yields `None`, so the
/// caller falls back to the built-in providers.
pub fn plugin_provider_from_script(script: &str) -> Option<PluginVideoProvider> {
    let value: Value = serde_json::from_str(script.trim()).ok()?;
    if !value.is_object() {
        return None;
    }
    let endpoint = value
        .get("endpoint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())?
        .to_string();
    let body = match value.get("body") {
        Some(body) if body.is_object() => body.clone(),
        _ => json!({}),
    };
    let mut poll = PluginVideoPoll::default();
    if let Some(poll_value) = value.get("poll").filter(|poll| poll.is_object()) {
        let string_field = |key: &str, fallback: &str| {
            poll_value
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| fallback.to_string())
        };
        let string_list = |key: &str, fallback: &[String]| {
            poll_value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|items| !items.is_empty())
                .unwrap_or_else(|| fallback.to_vec())
        };
        poll.endpoint = string_field("endpoint", &poll.endpoint);
        poll.status_path = string_field("statusPath", &poll.status_path);
        poll.completed = string_list("completed", &poll.completed);
        poll.failed = string_list("failed", &poll.failed);
        poll.result_url_path = string_field("resultUrlPath", &poll.result_url_path);
        poll.id_path = string_field("idPath", &poll.id_path);
        if let Some(interval) = poll_value.get("intervalMs").and_then(Value::as_u64) {
            poll.interval_ms = interval.max(1);
        }
        if let Some(timeout) = poll_value.get("timeoutMs").and_then(Value::as_u64) {
            poll.timeout_ms = timeout.max(1);
        }
    }
    Some(PluginVideoProvider {
        endpoint,
        body,
        poll,
    })
}

/// Substitute `{{name}}` placeholders in a string from `vars`; unknown keys
/// become empty and a dangling `{{` is left verbatim.
pub fn render_template(template: &str, vars: &BTreeMap<String, PluginVar>) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            break;
        };
        output.push_str(&rest[..start]);
        let key = after_open[..end].trim();
        if let Some(var) = vars.get(key) {
            output.push_str(&var.as_text());
        }
        rest = &after_open[end + 2..];
    }
    output.push_str(rest);
    output
}

fn exact_placeholder(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{{")?.strip_suffix("}}")?;
    let key = inner.trim();
    (!key.is_empty() && !key.contains('{') && !key.contains('}')).then_some(key)
}

/// Recursively substitute placeholders in a JSON template. A string that is
/// exactly one placeholder takes the raw JSON value of that variable (so arrays
/// and objects embed), otherwise inline `{{name}}` occurrences are text-replaced.
pub fn render_json_template(template: &Value, vars: &BTreeMap<String, PluginVar>) -> Value {
    match template {
        Value::String(text) => {
            if let Some(key) = exact_placeholder(text) {
                return vars.get(key).map_or_else(
                    || Value::String(String::new()),
                    |var| match var {
                        PluginVar::Json(value) => value.clone(),
                        PluginVar::Text(text) => Value::String(text.clone()),
                    },
                );
            }
            Value::String(render_template(text, vars))
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| render_json_template(item, vars))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), render_json_template(value, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_endpoint(base_url: &str, endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else if endpoint.starts_with('/') {
        build_api_url(base_url, endpoint)
    } else {
        build_api_url(base_url, &format!("/{endpoint}"))
    }
}

fn resolve_media_url(base_url: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with('/') {
        format!("{}{url}", origin_of(base_url))
    } else {
        build_api_url(base_url, &format!("/{url}"))
    }
}

/// `runModelPlugin` params for a video call (`video.ts:createPluginVideoTask`):
/// `prompt`, `model`, `seconds`, `size`, `resolution`, `ratio`, `generateAudio`,
/// `watermark`, `mode`, plus the reference media as data URL arrays.
pub fn plugin_video_vars(
    prompt: &str,
    req: &ModelRequestConfig,
    media: &VideoRequestMedia,
) -> BTreeMap<String, PluginVar> {
    let capability = video_model_capability(&req.model);
    let mode = if media.has_explicit_frames() {
        VideoMode::Frames
    } else {
        resolve_video_mode(&req.video_mode, media.images.len())
    };
    let size = normalize_video_size(&req.size, &req.vquality).map_or(Value::Null, Value::String);
    let data_urls = |items: &[crate::video_request::ReferenceMedia]| {
        Value::Array(
            items
                .iter()
                .map(|item| Value::String(item.data_url()))
                .collect(),
        )
    };
    let frame = |item: &Option<crate::video_request::ReferenceMedia>| {
        item.as_ref()
            .map_or(Value::Null, |media| Value::String(media.data_url()))
    };
    BTreeMap::from([
        ("prompt".to_string(), PluginVar::Text(prompt.to_string())),
        ("model".to_string(), PluginVar::Text(req.model.clone())),
        (
            "seconds".to_string(),
            PluginVar::Text(video_model_duration(&req.video_seconds, &capability).to_string()),
        ),
        ("size".to_string(), PluginVar::Json(size)),
        (
            "resolution".to_string(),
            PluginVar::Text(normalize_video_resolution(&req.vquality)),
        ),
        (
            "ratio".to_string(),
            PluginVar::Text(video_aspect_ratio(&req.size)),
        ),
        (
            "mode".to_string(),
            PluginVar::Text(mode.as_str().to_string()),
        ),
        (
            "generateAudio".to_string(),
            PluginVar::Json(json!(bool_config(&req.video_generate_audio, true))),
        ),
        (
            "watermark".to_string(),
            PluginVar::Json(json!(bool_config(&req.video_watermark, false))),
        ),
        (
            "firstFrame".to_string(),
            PluginVar::Json(frame(&media.first_frame)),
        ),
        (
            "lastFrame".to_string(),
            PluginVar::Json(frame(&media.last_frame)),
        ),
        (
            "images".to_string(),
            PluginVar::Json(data_urls(&media.images)),
        ),
        (
            "videos".to_string(),
            PluginVar::Json(data_urls(&media.videos)),
        ),
        (
            "audios".to_string(),
            PluginVar::Json(data_urls(&media.audios)),
        ),
    ])
}

async fn post_json(
    client: &Client,
    url: &str,
    req: &ModelRequestConfig,
    body: &Value,
    cancel: &Cancel,
) -> Result<Value, AiError> {
    let builder = client
        .post(url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key))
        .json(body);
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
        .map_err(|error| AiError::Parse(format!("plugin video response: {error}")))
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
        .map_err(|error| AiError::Parse(format!("plugin video poll response: {error}")))
}

async fn finish_result(
    client: &Client,
    req: &ModelRequestConfig,
    url: Option<String>,
    cancel: &Cancel,
) -> Result<VideoResult, AiError> {
    let Some(url) = url.filter(|value| !value.is_empty()) else {
        return Err(AiError::Api(
            "The model request script did not return a video".to_string(),
        ));
    };
    let media_url = resolve_media_url(&req.base_url, &url);
    match download(client, &media_url, req, cancel).await {
        Ok(bytes) => Ok(VideoResult {
            bytes: Some(bytes),
            mime: Some("video/mp4".to_string()),
            ..VideoResult::default()
        }),
        Err(error) if error.is_canceled() => Err(error),
        Err(_) => Ok(VideoResult {
            url: Some(url),
            mime: Some("video/mp4".to_string()),
            ..VideoResult::default()
        }),
    }
}

/// The declarative `create_plugin_video_task` (`video.ts:194`): render the
/// configured body, POST it, then poll per the configured shape until terminal,
/// returning the downloaded result.
pub async fn create_plugin_video_task(
    client: &Client,
    req: &ModelRequestConfig,
    provider: &PluginVideoProvider,
    prompt: &str,
    media: &VideoRequestMedia,
    cancel: &Cancel,
) -> Result<VideoResult, AiError> {
    assert_video_config(req)?;
    crate::video_request::validate_reference_limits(media)?;
    let vars = plugin_video_vars(prompt, req, media);
    let create_url = resolve_endpoint(&req.base_url, &render_template(&provider.endpoint, &vars));
    let body = render_json_template(&provider.body, &vars);
    let created = post_json(client, &create_url, req, &body, cancel).await?;

    let created_status = json_path(&created, &provider.poll.status_path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if provider.poll.completed.contains(&created_status) {
        let url = json_path(&created, &provider.poll.result_url_path)
            .and_then(Value::as_str)
            .map(str::to_string);
        return finish_result(client, req, url, cancel).await;
    }

    let id = json_path(&created, &provider.poll.id_path)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AiError::Api("The provider returned no video task id".to_string()))?;
    let mut poll_vars = vars;
    poll_vars.insert("id".to_string(), PluginVar::Text(id.to_string()));
    let poll_url = resolve_endpoint(
        &req.base_url,
        &render_template(&provider.poll.endpoint, &poll_vars),
    );

    let interval = provider.poll.interval_ms.max(1);
    let attempts = (provider.poll.timeout_ms / interval).max(1);
    for _ in 0..attempts {
        if cancel.is_canceled() {
            return Err(AiError::Canceled);
        }
        let value = get_json(client, &poll_url, req, cancel).await?;
        let status = json_path(&value, &provider.poll.status_path)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if provider.poll.completed.contains(&status) {
            let url = json_path(&value, &provider.poll.result_url_path)
                .and_then(Value::as_str)
                .map(str::to_string);
            return finish_result(client, req, url, cancel).await;
        }
        if provider.poll.failed.contains(&status) {
            let message = read_api_error_message(&value);
            return Err(AiError::VideoFailed(if message.is_empty() {
                "Video generation failed".to_string()
            } else {
                message
            }));
        }
        tokio::select! {
            () = cancel.cancelled() => return Err(AiError::Canceled),
            () = tokio::time::sleep(Duration::from_millis(interval)) => {}
        }
    }
    Err(AiError::Timeout("The video task timed out".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> BTreeMap<String, PluginVar> {
        BTreeMap::from([
            ("model".to_string(), PluginVar::Text("m".to_string())),
            ("prompt".to_string(), PluginVar::Text("a cat".to_string())),
            (
                "images".to_string(),
                PluginVar::Json(json!(["data:image/png;base64,AA"])),
            ),
            ("generateAudio".to_string(), PluginVar::Json(json!(true))),
            ("size".to_string(), PluginVar::Json(Value::Null)),
        ])
    }

    #[test]
    fn renders_inline_and_exact_placeholders() {
        assert_eq!(render_template("/v/{{model}}", &vars()), "/v/m");
        let body = render_json_template(
            &json!({
                "model": "{{model}}",
                "prompt": "make: {{prompt}}!",
                "refs": "{{images}}",
                "audio": "{{generateAudio}}",
                "size": "{{size}}",
            }),
            &vars(),
        );
        assert_eq!(body["model"], json!("m"));
        assert_eq!(body["prompt"], json!("make: a cat!"));
        assert_eq!(body["refs"], json!(["data:image/png;base64,AA"]));
        assert_eq!(body["audio"], json!(true));
        assert_eq!(body["size"], json!(null));
    }

    #[test]
    fn unknown_placeholders_render_empty_and_dangling_is_kept() {
        assert_eq!(render_template("{{nope}}", &vars()), "");
        let body = render_json_template(&json!({"x": "{{nope}}"}), &vars());
        assert_eq!(body["x"], json!(""));
    }

    #[test]
    fn media_url_resolution_handles_absolute_origin_and_relative() {
        assert_eq!(
            resolve_media_url("https://x/api/v1", "https://cdn/a.mp4"),
            "https://cdn/a.mp4"
        );
        assert_eq!(
            resolve_media_url("https://x/api/v1", "/v1/asset/1"),
            "https://x/v1/asset/1"
        );
        assert_eq!(
            resolve_media_url("https://x/api/v1", "asset/1"),
            "https://x/api/v1/asset/1"
        );
    }

    #[test]
    fn script_parses_into_a_declarative_provider() {
        let script = r#"{
            "endpoint": "/videos",
            "body": {"model": "{{model}}", "prompt": "{{prompt}}", "refs": "{{images}}"},
            "poll": {"endpoint": "/jobs/{{id}}", "statusPath": "state", "completed": ["done"],
                     "failed": ["error"], "resultUrlPath": "output.url", "idPath": "jobId",
                     "intervalMs": 1000, "timeoutMs": 60000}
        }"#;
        let provider = plugin_provider_from_script(script)
            .unwrap_or_else(|| unreachable!("a well-formed script must parse"));
        assert_eq!(provider.endpoint, "/videos");
        assert_eq!(provider.body["model"], json!("{{model}}"));
        assert_eq!(provider.poll.endpoint, "/jobs/{{id}}");
        assert_eq!(provider.poll.status_path, "state");
        assert_eq!(provider.poll.completed, vec!["done".to_string()]);
        assert_eq!(provider.poll.failed, vec!["error".to_string()]);
        assert_eq!(provider.poll.result_url_path, "output.url");
        assert_eq!(provider.poll.id_path, "jobId");
        assert_eq!(provider.poll.interval_ms, 1000);
        assert_eq!(provider.poll.timeout_ms, 60_000);
    }

    #[test]
    fn script_defaults_and_rejections() {
        let minimal = plugin_provider_from_script(r#"{"endpoint": "/v"}"#)
            .unwrap_or_else(|| unreachable!("an endpoint is enough"));
        assert_eq!(minimal.body, json!({}));
        assert_eq!(minimal.poll, PluginVideoPoll::default());
        assert!(plugin_provider_from_script("not json").is_none());
        assert!(plugin_provider_from_script("[]").is_none());
        assert!(plugin_provider_from_script(r#"{"body": {}}"#).is_none());
        assert!(plugin_provider_from_script(r#"{"endpoint": "  "}"#).is_none());
        let zero_interval = plugin_provider_from_script(
            r#"{"endpoint": "/v", "poll": {"intervalMs": 0, "timeoutMs": 0}}"#,
        )
        .unwrap_or_else(|| unreachable!("zero values clamp"));
        assert_eq!(zero_interval.poll.interval_ms, 1);
        assert_eq!(zero_interval.poll.timeout_ms, 1);
    }
}
