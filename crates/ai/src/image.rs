//! Image generation/edit and text/vision requests, ported from
//! `web/src/services/api/image.ts`. Image ops force `IMAGE_MODEL`
//! (`requestGeneration`/`requestEdit`), text resolves the config's text model,
//! and the request/response JSON shapes match the web exactly.

use reqwest::Client;
use serde_json::{json, Map, Value};

use crate::client::{bearer, send_cancelable, Cancel, REQUEST_TIMEOUT};
use crate::config::{build_api_url, ModelRequestConfig, IMAGE_MODEL};
use crate::error::{map_reqwest_error, read_http_error, AiError};
use crate::json::value_as_f64;
use crate::response::{responses_body, stream_response, ToolResponse};

/// Bounds for the image count (`Math.max(1, Math.min(10, …))`, `image.ts:467`).
pub const IMAGE_COUNT_MIN: i64 = 1;
pub const IMAGE_COUNT_MAX: i64 = 10;
/// `IMAGE_OUTPUT_FORMAT` (`image.ts:136`).
pub const IMAGE_OUTPUT_FORMAT: &str = "png";

/// `QUALITY_BASE` (`image.ts:118`).
const QUALITY_BASE: [(&str, i64); 5] = [
    ("low", 1024),
    ("medium", 2048),
    ("high", 2880),
    ("standard", 1024),
    ("hd", 2048),
];
const IMAGE_SIZE_STEP: i64 = 16;
const IMAGE_MIN_PIXELS: i64 = 655_360;
const IMAGE_MAX_PIXELS: i64 = 8_294_400;
const IMAGE_MAX_EDGE: i64 = 3840;
const IMAGE_MAX_RATIO: f64 = 3.0;
const DEFAULT_IMAGE_SHORT_SIDE: i64 = 1024;

/// One generated image (`GeneratedImage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub data_url: String,
}

/// `ImageUsage` (`image.ts:55`): the `usage` block a `POST /images` response
/// may carry — the provider's own billed cost plus the token counts the
/// fallback estimate prices.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImageUsage {
    /// Billed amount in USD (`usage.cost`), when the provider reports one.
    pub cost: Option<f64>,
    pub prompt_tokens: Option<f64>,
    pub completion_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
}

impl ImageUsage {
    /// The token counts as `estimate_token_cost`'s input; `has_reference` says
    /// whether the request sent reference images (the image-input rate applies).
    pub fn token_usage(&self, has_reference: bool) -> crate::generation_cost::TokenUsage {
        crate::generation_cost::TokenUsage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            has_reference,
        }
    }
}

/// The `usage` block of a `POST /images` payload (an absent block is all-`None`).
pub fn parse_image_usage(payload: &Value) -> ImageUsage {
    let Some(usage) = payload.get("usage") else {
        return ImageUsage::default();
    };
    let number = |key: &str| usage.get(key).and_then(value_as_f64);
    ImageUsage {
        cost: number("cost"),
        prompt_tokens: number("prompt_tokens"),
        completion_tokens: number("completion_tokens"),
        total_tokens: number("total_tokens"),
    }
}

/// `ImageRequestResult` (`image.ts:67`): the generated images plus what the
/// response said about their cost — the `usage` block and the
/// `x-generation-id` header the `/generation` lookup needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageRequestResult {
    pub images: Vec<GeneratedImage>,
    pub usage: ImageUsage,
    pub generation_id: Option<String>,
}

/// `normalizeBackground` (`image.ts:145`): only `"transparent"` is forwarded.
pub fn normalize_background(background: &str) -> Option<String> {
    background
        .trim()
        .eq_ignore_ascii_case("transparent")
        .then(|| "transparent".to_string())
}

fn quality_base(quality: &str) -> Option<i64> {
    QUALITY_BASE
        .iter()
        .find(|(name, _)| *name == quality)
        .map(|(_, pixels)| *pixels)
}

/// `parseImageDimensions` (`image.ts:192`).
pub fn parse_image_dimensions(value: &str) -> Option<(i64, i64)> {
    let (width, height) = value.split_once(['x', 'X'])?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

/// `validateImageSize` (`image.ts:198`).
pub fn validate_image_size(width: i64, height: i64) -> Result<(), AiError> {
    if width <= 0 || height <= 0 {
        return Err(AiError::invalid("Image dimensions must be positive"));
    }
    if width % IMAGE_SIZE_STEP != 0 || height % IMAGE_SIZE_STEP != 0 {
        return Err(AiError::invalid("Image dimensions must be multiples of 16"));
    }
    if width.max(height) > IMAGE_MAX_EDGE {
        return Err(AiError::invalid("Image edge exceeds 3840px"));
    }
    let ratio = (width.max(height) as f64) / (width.min(height) as f64);
    if ratio > IMAGE_MAX_RATIO {
        return Err(AiError::invalid("Image ratio exceeds 3:1"));
    }
    let pixels = width * height;
    if !(IMAGE_MIN_PIXELS..=IMAGE_MAX_PIXELS).contains(&pixels) {
        return Err(AiError::invalid("Image pixel count is out of range"));
    }
    Ok(())
}

/// `resolveSize` (`image.ts:150`): map a quality + ratio string to pixels.
pub fn resolve_size(quality: Option<&str>, ratio: &str) -> Result<String, AiError> {
    let Some((ratio_w, ratio_h)) = ratio.split_once(':') else {
        return Err(AiError::invalid("Invalid image size format"));
    };
    let width_ratio: f64 = ratio_w
        .parse()
        .map_err(|_| AiError::invalid("Image ratio terms must be positive numbers"))?;
    let height_ratio: f64 = ratio_h
        .parse()
        .map_err(|_| AiError::invalid("Image ratio terms must be positive numbers"))?;
    if width_ratio <= 0.0 || height_ratio <= 0.0 {
        return Err(AiError::invalid(
            "Image ratio terms must be positive numbers",
        ));
    }
    let long_ratio = if width_ratio >= height_ratio {
        width_ratio / height_ratio
    } else {
        height_ratio / width_ratio
    };
    if long_ratio > IMAGE_MAX_RATIO {
        return Err(AiError::invalid("Image ratio exceeds 3:1"));
    }
    let scale = match quality {
        Some("high") => "4k",
        Some("medium" | "hd") => "2k",
        _ => "1k",
    };
    if let Some(preset) = preset_pixels(scale, ratio) {
        return Ok(preset);
    }
    let is_landscape = width_ratio >= height_ratio;
    let (long_side, short_side) = match quality.and_then(quality_base) {
        Some(base) => {
            let target = (base * base) as f64;
            let long_raw = (target * long_ratio).sqrt();
            let long = (long_raw / IMAGE_SIZE_STEP as f64).floor() * IMAGE_SIZE_STEP as f64;
            let short =
                (long / long_ratio / IMAGE_SIZE_STEP as f64).round() * IMAGE_SIZE_STEP as f64;
            (long, short)
        }
        None => {
            let short = DEFAULT_IMAGE_SHORT_SIDE as f64;
            let long =
                ((short * long_ratio) / IMAGE_SIZE_STEP as f64).round() * IMAGE_SIZE_STEP as f64;
            (long, short)
        }
    };
    let (width, height) = if is_landscape {
        (long_side, short_side)
    } else {
        (short_side, long_side)
    };
    let (width, height) = (width as i64, height as i64);
    validate_image_size(width, height)?;
    Ok(format!("{width}x{height}"))
}

/// `resolveRequestSize` (`image.ts:207`): a pixel size is validated and returned,
/// a ratio is expanded, `auto`/empty becomes `None`.
pub fn resolve_request_size(quality: Option<&str>, size: &str) -> Result<Option<String>, AiError> {
    let value = size.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    if let Some((width, height)) = parse_image_dimensions(value) {
        validate_image_size(width, height)?;
        return Ok(Some(format!("{width}x{height}")));
    }
    if value.contains(':') {
        return resolve_size(quality, value).map(Some);
    }
    Err(AiError::invalid("Invalid image size format"))
}

/// `resolveImageRequestOptions` (`image.ts:220`): the OpenRouter image fields.
/// The quality is forwarded **raw, lowercased** (matching the web), not normalized.
pub fn resolve_image_request_options(req: &ModelRequestConfig) -> Map<String, Value> {
    let quality = req.quality.trim().to_ascii_lowercase();
    let size = req.size.trim();
    let mut options = Map::new();
    if !quality.is_empty() && quality != "auto" {
        options.insert("quality".to_string(), json!(quality));
    }
    if parse_image_dimensions(size).is_some() {
        options.insert("size".to_string(), json!(size));
    } else if size == "auto" || is_ratio(size) {
        options.insert("aspect_ratio".to_string(), json!(size));
    }
    if let Some(background) = normalize_background(&req.background) {
        options.insert("background".to_string(), json!(background));
    }
    options
}

fn is_ratio(value: &str) -> bool {
    value.split_once(':').is_some_and(|(w, h)| {
        !w.is_empty() && !h.is_empty() && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit())
    })
}

/// `withSystemPrompt` (`image.ts:305`): the system prompt is prepended **into the
/// user prompt string** for image calls (not a real system role).
pub fn with_system_prompt(system_prompt: &str, prompt: &str) -> String {
    let system = system_prompt.trim();
    if system.is_empty() {
        prompt.to_string()
    } else {
        format!("{system}\n\n{prompt}")
    }
}

/// The `POST /images` body for generation (`requestGeneration`).
pub fn generation_body(req: &ModelRequestConfig, prompt: &str, count: i64) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), json!(IMAGE_MODEL));
    body.insert(
        "prompt".to_string(),
        json!(with_system_prompt(&req.system_prompt, prompt)),
    );
    body.insert("n".to_string(), json!(clamp_image_count(count)));
    for (key, value) in resolve_image_request_options(req) {
        body.insert(key, value);
    }
    body.insert("output_format".to_string(), json!(IMAGE_OUTPUT_FORMAT));
    Value::Object(body)
}

/// The `POST /images` body for edit (`requestEdit`): forces `n: 1` and adds the
/// reference images as `input_references`.
pub fn edit_body(req: &ModelRequestConfig, prompt: &str, references: &[String]) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), json!(IMAGE_MODEL));
    body.insert(
        "prompt".to_string(),
        json!(with_system_prompt(&req.system_prompt, prompt)),
    );
    body.insert("n".to_string(), json!(1));
    for (key, value) in resolve_image_request_options(req) {
        body.insert(key, value);
    }
    let parts: Vec<Value> = references
        .iter()
        .map(|url| json!({ "type": "image_url", "image_url": { "url": url } }))
        .collect();
    body.insert("input_references".to_string(), Value::Array(parts));
    body.insert("output_format".to_string(), json!(IMAGE_OUTPUT_FORMAT));
    Value::Object(body)
}

/// `clamp count 1..=10` (`image.ts:467`).
pub fn clamp_image_count(count: i64) -> i64 {
    count.clamp(IMAGE_COUNT_MIN, IMAGE_COUNT_MAX)
}

/// `resolveImageSource` (`image.ts:233`): `b64_json` → data URL, else `url`.
fn resolve_image_source(item: &Value) -> Option<String> {
    if let Some(b64) = item.get("b64_json").and_then(Value::as_str) {
        if !b64.is_empty() {
            let media_type = item
                .get("media_type")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("image/png");
            return Some(format!("data:{media_type};base64,{b64}"));
        }
    }
    item.get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `parseImagePayload` (`image.ts:244`): accept `data` | `images` | `results`.
pub fn parse_image_payload(payload: &Value) -> Result<Vec<GeneratedImage>, AiError> {
    if let Some(code) = payload.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let message = payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("Request failed");
            return Err(AiError::Api(message.to_string()));
        }
    }
    let list = ["data", "images", "results"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    let images: Vec<GeneratedImage> = list
        .iter()
        .filter_map(resolve_image_source)
        .map(|data_url| GeneratedImage { data_url })
        .collect();
    if images.is_empty() {
        let keys: Vec<&str> = payload
            .as_object()
            .map(|map| {
                map.keys()
                    .map(String::as_str)
                    .filter(|key| !matches!(*key, "code" | "msg" | "error"))
                    .collect()
            })
            .unwrap_or_default();
        return Err(AiError::Api(if keys.is_empty() {
            "The provider returned no image".to_string()
        } else {
            format!("Unrecognized image response (fields: {})", keys.join(", "))
        }));
    }
    Ok(images)
}

/// `readGenerationId` (`image.ts:271`): the `x-generation-id` response header.
fn read_generation_id(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("x-generation-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn post_images(
    client: &Client,
    req: &ModelRequestConfig,
    body: &Value,
    cancel: &Cancel,
) -> Result<ImageRequestResult, AiError> {
    let url = build_api_url(&req.base_url, "/images");
    let builder = client
        .post(&url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key))
        .json(body);
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    let generation_id = read_generation_id(response.headers());
    let text = response
        .text()
        .await
        .map_err(|error| map_reqwest_error(&error))?;
    if !status.is_success() {
        return Err(read_http_error(status.as_u16(), &text));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AiError::Parse(format!("image response: {error}")))?;
    Ok(ImageRequestResult {
        images: parse_image_payload(&value)?,
        usage: parse_image_usage(&value),
        generation_id,
    })
}

/// `requestGeneration` (`image.ts:465`): force `IMAGE_MODEL`, generate `n` images.
pub async fn request_generation(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    cancel: &Cancel,
) -> Result<ImageRequestResult, AiError> {
    let body = generation_body(req, prompt, req.count);
    post_images(client, req, &body, cancel).await
}

/// `requestEdit` (`image.ts:516`): force `IMAGE_MODEL`, edit from reference images
/// (data URLs).
pub async fn request_edit(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    references: &[String],
    cancel: &Cancel,
) -> Result<ImageRequestResult, AiError> {
    let body = edit_body(req, prompt, references);
    post_images(client, req, &body, cancel).await
}

/// `requestImageQuestion` (`image.ts:571`): text/vision over the streaming
/// `/responses` endpoint.
pub async fn request_image_question<F>(
    client: &Client,
    req: &ModelRequestConfig,
    prompt: &str,
    images: &[String],
    cancel: &Cancel,
    on_delta: &mut F,
) -> Result<ToolResponse, AiError>
where
    F: FnMut(&str),
{
    let body = responses_body(req, prompt, images);
    stream_response(client, req, &body, cancel, on_delta).await
}

/// Pixel preset lookup (`imageSizePresets`), reusing `open-canvas-core` so the
/// size math cannot drift between the app and the request layer.
fn preset_pixels(scale: &str, ratio: &str) -> Option<String> {
    let size = open_canvas_core::media_size::compute_media_size(scale, ratio);
    (size != ratio && size != "auto").then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReasoningEffort;

    fn request() -> ModelRequestConfig {
        ModelRequestConfig {
            model: "openai/gpt-image-2.5-sunburst".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: "sk-test".to_string(),
            system_prompt: String::new(),
            reasoning_effort: ReasoningEffort::Auto,
            quality: "auto".to_string(),
            size: "1:1".to_string(),
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
    fn generation_body_matches_the_web_shape() {
        let body = generation_body(&request(), "a cat", 1);
        assert_eq!(body["model"], json!(IMAGE_MODEL));
        assert_eq!(body["prompt"], json!("a cat"));
        assert_eq!(body["n"], json!(1));
        assert_eq!(body["aspect_ratio"], json!("1:1"));
        assert_eq!(body["output_format"], json!("png"));
        assert!(body.get("quality").is_none(), "auto quality is omitted");
    }

    #[test]
    fn edit_body_forces_n_one_and_includes_references() {
        let body = edit_body(
            &request(),
            "make it dark",
            &["data:image/png;base64,AA".to_string()],
        );
        assert_eq!(body["n"], json!(1));
        assert_eq!(body["input_references"][0]["type"], json!("image_url"));
        assert_eq!(
            body["input_references"][0]["image_url"]["url"],
            json!("data:image/png;base64,AA")
        );
    }

    #[test]
    fn quality_and_size_are_forwarded_like_the_web() {
        let mut req = request();
        req.quality = "High".to_string();
        req.size = "1024x1024".to_string();
        let body = generation_body(&req, "x", 3);
        assert_eq!(body["quality"], json!("high"));
        assert_eq!(body["size"], json!("1024x1024"));
        assert!(body.get("aspect_ratio").is_none());
    }

    #[test]
    fn count_clamps_to_the_web_window() {
        assert_eq!(clamp_image_count(0), 1);
        assert_eq!(clamp_image_count(42), 10);
        assert_eq!(clamp_image_count(4), 4);
    }

    #[test]
    fn parses_every_response_field_variant() {
        let data = ok(parse_image_payload(
            &json!({"data": [{"b64_json": "AA", "media_type": "image/png"}]}),
        ));
        assert_eq!(data[0].data_url, "data:image/png;base64,AA");
        let images = ok(parse_image_payload(
            &json!({"images": [{"url": "https://x/y.png"}]}),
        ));
        assert_eq!(images[0].data_url, "https://x/y.png");
        let results = ok(parse_image_payload(
            &json!({"results": [{"b64_json": "BB"}]}),
        ));
        assert_eq!(results[0].data_url, "data:image/png;base64,BB");
    }

    #[test]
    fn empty_image_response_is_an_error_naming_the_fields() {
        let error = parse_image_payload(&json!({"unexpected": 1}))
            .err()
            .unwrap_or_else(|| unreachable!("expected error"));
        assert!(matches!(error, AiError::Api(message) if message.contains("unexpected")));
        let error = parse_image_payload(&json!({}))
            .err()
            .unwrap_or_else(|| unreachable!("expected error"));
        assert!(matches!(error, AiError::Api(message) if message.contains("no image")));
    }

    #[test]
    fn code_nonzero_is_an_api_error() {
        let error = parse_image_payload(&json!({"code": 7, "msg": "quota"}))
            .err()
            .unwrap_or_else(|| unreachable!("expected error"));
        assert!(matches!(error, AiError::Api(message) if message == "quota"));
    }

    #[test]
    fn system_prompt_prepends_into_the_user_prompt() {
        assert_eq!(with_system_prompt("sys", "body"), "sys\n\nbody");
        assert_eq!(with_system_prompt("  ", "body"), "body");
    }

    #[test]
    fn size_resolution_validates_pixels() {
        assert_eq!(resolve_request_size(None, "auto").ok().flatten(), None);
        assert_eq!(
            resolve_request_size(None, "1024x1024").ok().flatten(),
            Some("1024x1024".to_string())
        );
        assert_eq!(
            resolve_request_size(Some("high"), "16:9").ok().flatten(),
            Some("3840x2160".to_string())
        );
        assert!(resolve_request_size(None, "1023x1024").is_err());
    }
}
