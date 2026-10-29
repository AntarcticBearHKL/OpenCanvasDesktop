//! AI service layer for the OpenCanvas desktop port: an OpenAI-compatible HTTP
//! client (image generation/edit, streaming text/vision, video create + poll with
//! frames/reference media), generation-cost lookup + estimation, OpenRouter
//! account inspection, the provider/model configuration model, a host-agnostic
//! runtime log, and the retry/concurrency helpers.
//!
//! Ported from `web/src/services/api/{image,video,api-error,openrouter-account}.ts`,
//! `web/src/lib/{runtime-log.ts,canvas/generation-cost.ts}`,
//! `web/src/stores/use-config-store.ts` and `web/src/lib/provider-presets.ts`.
//!
//! # API key handling (by design, matching the web)
//! The API key is stored locally in the app's config file (next to
//! `library.json`) and is sent by this client **directly to the endpoint the user
//! configured** as an `Authorization: Bearer …` header. There is no proxy and no
//! cloud backend: the key never leaves the machine except toward that endpoint.

pub mod account;
pub mod client;
pub mod config;
pub mod error;
pub mod generation;
pub mod generation_cost;
pub mod image;
mod json;
pub mod plugin_video;
pub mod response;
pub mod retry;
pub mod runtime_log;
pub mod video;
pub mod video_request;

pub use account::{
    fetch_openrouter_credits, fetch_openrouter_credits_from, fetch_openrouter_key_usage,
    fetch_openrouter_key_usage_from, OpenRouterCredits, OpenRouterKeyUsage,
};
pub use client::{build_client, Cancel, Client};
pub use config::{
    build_api_url, decode_channel_model, encode_channel_model, find_channel_model,
    fixed_model_for_capability, fixed_model_name, is_ai_config_ready, resolve_model_request_config,
    resolve_model_script, AiConfig, ChannelModel, ModelCapability, ModelChannel,
    ModelRequestConfig, ReasoningEffort, DEEPSEEK_BASE_URL, DEEPSEEK_TEXT_MODEL, IMAGE_MODEL,
    OPENROUTER_BASE_URL, VIDEO_MODEL,
};
pub use error::AiError;
pub use generation::{fetch_generation_cost, parse_generation_cost, GenerationCost};
pub use generation_cost::{
    estimate_generation_cost, estimate_token_cost, format_usd, price_model_id,
    GenerationCostEstimate, GenerationCostReason, GenerationCostSource, GenerationCostUnit,
    TokenUsage, GENERATION_COST_RECORD_LIMIT,
};
pub use plugin_video::{
    create_plugin_video_task, plugin_provider_from_script, plugin_video_vars, render_json_template,
    render_template, PluginVar, PluginVideoPoll, PluginVideoProvider,
};
pub use retry::{
    run_with_retry, GenerationQueue, GENERATION_CONCURRENCY, GENERATION_MAX_ATTEMPTS,
    GENERATION_RETRY_DELAY_MS,
};
pub use runtime_log::{
    now_millis, truncate_log_text, RuntimeLogBuffer, RuntimeLogEntry, RuntimeLogLevel,
    MAX_LOG_TEXT, RUNTIME_LOG_LIMIT,
};
pub use video::{create_video_task_with_media, is_openrouter_video_model};
pub use video_request::{
    assert_reference_seconds, assert_reference_size, normalize_video_mode, openai_video_form,
    openrouter_frame_images, openrouter_video_body_media, resolve_video_mode,
    supported_video_resolution, validate_reference_kinds, validate_reference_limits,
    video_model_capability, video_model_duration, ReferenceMedia, VideoFrameType, VideoMode,
    VideoModelCapability, VideoReferenceKind, VideoRequestMedia, AUDIO_REFERENCE_MAX_BYTES,
    VIDEO_REFERENCE_AUDIO_LIMIT, VIDEO_REFERENCE_IMAGE_LIMIT, VIDEO_REFERENCE_MAX_BYTES,
    VIDEO_REFERENCE_REQUEST_MAX_BYTES, VIDEO_REFERENCE_SECONDS_MAX, VIDEO_REFERENCE_SECONDS_MIN,
    VIDEO_REFERENCE_TOTAL_LIMIT, VIDEO_REFERENCE_VIDEO_LIMIT,
};
