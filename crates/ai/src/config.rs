//! Provider / model configuration, ported from `web/src/stores/use-config-store.ts`
//! and `web/src/lib/provider-presets.ts`.
//!
//! The on-disk shape matches the web's localStorage `AiConfig` (camelCase, same
//! field names) so a config stays recognisably compatible, and the two addressing
//! rules the web depends on are preserved verbatim:
//!
//! * a model option is encoded `"<channelId>::<modelName>"` (`encode_channel_model`), and
//! * a base URL is normalized to end in `/v1` before a path is appended (`build_api_url`).
//!
//! The provider/model assignment is fixed: exactly two providers (OpenRouter and
//! DeepSeek) and one text, one image and one video model. [`AiConfig::normalize`]
//! enforces that lock on every load, repairing older saved configs silently.
//!
//! The API key lives in this config on disk and is sent by the client directly to
//! the configured endpoint (`Authorization: Bearer …`); see the crate docs.

use serde::{Deserialize, Deserializer, Serialize};

/// `CHANNEL_MODEL_SEPARATOR` (`use-config-store.ts:70`).
pub const CHANNEL_MODEL_SEPARATOR: &str = "::";
/// `OPENROUTER_BASE_URL` (`use-config-store.ts:71`).
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// DeepSeek's OpenAI-compatible base URL (`build_api_url` appends `/v1`).
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
/// The one DeepSeek text model served by DeepSeek-V4.1-Flash-1017.
pub const DEEPSEEK_TEXT_MODEL: &str = "deepseek-flash";
/// `IMAGE_MODEL` (`use-config-store.ts:72`) — image endpoints force this model.
pub const IMAGE_MODEL: &str = "openai/gpt-image-2.5-sunburst";
/// The one video model.
pub const VIDEO_MODEL: &str = "minimax/hailuo-3-max";

/// `ModelCapability` (`use-config-store.ts:11`), reduced to the three capabilities
/// this build supports. Older configs may still carry `"audio"`/`"speech"`; the
/// lenient [`Deserialize`] maps any unknown value to `Text`, and
/// [`AiConfig::normalize`] then drops those channels' models entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCapability {
    Text,
    Image,
    Video,
}

impl<'de> Deserialize<'de> for ModelCapability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "image" => Self::Image,
            "video" => Self::Video,
            _ => Self::Text,
        })
    }
}

impl ModelCapability {
    /// The three capabilities in the web's tab order.
    pub const ALL: [ModelCapability; 3] = [
        ModelCapability::Text,
        ModelCapability::Image,
        ModelCapability::Video,
    ];

    /// Wire id (serde lowercase name).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

/// `ReasoningEffort` (`use-config-store.ts:12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    #[default]
    Auto,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

/// `ChannelModel` (`use-config-store.ts:14`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModel {
    pub name: String,
    pub capability: ModelCapability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

/// `ModelChannel` (`use-config-store.ts:20`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChannel {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    /// Always `"openai"` today (the only `ApiCallFormat`).
    #[serde(default = "default_api_format")]
    pub api_format: String,
    #[serde(default)]
    pub models: Vec<ChannelModel>,
}

/// `AiConfig` (`use-config-store.ts:29`): the full persisted config. Every field
/// defaults, so a partial or older JSON file still loads. Unknown fields (the
/// removed audio/speech and canvas-background settings) are ignored on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub channel_mode: String,
    pub api_format: String,
    pub channels: Vec<ModelChannel>,
    pub model: String,
    pub image_model: String,
    pub video_model: String,
    pub text_model: String,
    pub video_seconds: String,
    pub vquality: String,
    pub video_generate_audio: String,
    pub video_watermark: String,
    pub video_mode: String,
    pub system_prompt: String,
    pub reasoning_effort: ReasoningEffort,
    pub models: Vec<String>,
    pub quality: String,
    pub size: String,
    pub background: String,
    pub count: String,
    pub canvas_image_count: String,
}

/// The resolved request target: the model value decoded to its owner channel, so
/// `base_url`/`api_key` always travel with the provider (`resolveModelRequestConfig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequestConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub system_prompt: String,
    pub reasoning_effort: ReasoningEffort,
    pub quality: String,
    pub size: String,
    pub background: String,
    pub count: i64,
    pub video_seconds: String,
    pub vquality: String,
    pub video_generate_audio: String,
    pub video_watermark: String,
    pub video_mode: String,
}

/// One provider preset (`provider-presets.ts:6`).
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub text_only: bool,
    pub models: &'static [(&'static str, ModelCapability)],
}

/// `PROVIDER_PRESETS`: the fixed provider table, in display order.
pub const PROVIDER_PRESETS: [ProviderPreset; 2] = [
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: OPENROUTER_BASE_URL,
        text_only: false,
        models: &[
            (IMAGE_MODEL, ModelCapability::Image),
            ("minimax/hailuo-3-max", ModelCapability::Video),
            ("minimax/hailuo-3", ModelCapability::Video),
        ],
    },
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: DEEPSEEK_BASE_URL,
        text_only: true,
        models: &[(DEEPSEEK_TEXT_MODEL, ModelCapability::Text)],
    },
];

/// `VIDEO_KEYWORDS` (`use-config-store.ts:125`).
const VIDEO_KEYWORDS: [&str; 6] = ["video", "sora", "veo", "kling", "wan", "hailuo"];
/// `IMAGE_KEYWORDS` (`use-config-store.ts:132`).
const IMAGE_KEYWORDS: [&str; 10] = [
    "seedream",
    "gpt-image",
    "image",
    "dall-e",
    "dalle",
    "imagen",
    "flux",
    "sdxl",
    "stable-diffusion",
    "midjourney",
];

fn default_api_format() -> String {
    "openai".to_string()
}

impl Default for AiConfig {
    /// `defaultConfig` (`use-config-store.ts:74`), with the fixed model lock.
    fn default() -> Self {
        Self {
            channel_mode: "local".to_string(),
            api_format: default_api_format(),
            channels: PROVIDER_PRESETS
                .iter()
                .map(|preset| preset_channel(preset, String::new()))
                .collect(),
            model: String::new(),
            image_model: fixed_model_for_capability(ModelCapability::Image),
            video_model: fixed_model_for_capability(ModelCapability::Video),
            text_model: fixed_model_for_capability(ModelCapability::Text),
            video_seconds: "6".to_string(),
            vquality: "720".to_string(),
            video_generate_audio: "true".to_string(),
            video_watermark: "false".to_string(),
            video_mode: "frames".to_string(),
            system_prompt: String::new(),
            reasoning_effort: ReasoningEffort::Auto,
            models: Vec::new(),
            quality: "auto".to_string(),
            size: "1:1".to_string(),
            background: String::new(),
            count: "1".to_string(),
            canvas_image_count: "1".to_string(),
        }
    }
}

/// The model name fixed for a capability.
pub fn fixed_model_name(capability: ModelCapability) -> &'static str {
    match capability {
        ModelCapability::Text => DEEPSEEK_TEXT_MODEL,
        ModelCapability::Image => IMAGE_MODEL,
        ModelCapability::Video => VIDEO_MODEL,
    }
}

/// The encoded (`<channelId>::<model>`) fixed model option for a capability.
pub fn fixed_model_for_capability(capability: ModelCapability) -> String {
    let channel = match capability {
        ModelCapability::Text => "deepseek",
        ModelCapability::Image | ModelCapability::Video => "openrouter",
    };
    encode_channel_model(channel, fixed_model_name(capability))
}

/// Builds a channel from a preset, carrying the stored API key over.
fn preset_channel(preset: &ProviderPreset, api_key: String) -> ModelChannel {
    ModelChannel {
        id: preset.id.to_string(),
        name: preset.name.to_string(),
        base_url: preset.base_url.to_string(),
        api_key,
        api_format: default_api_format(),
        models: preset
            .models
            .iter()
            .map(|(name, capability)| ChannelModel {
                name: (*name).to_string(),
                capability: *capability,
                script: None,
            })
            .collect(),
    }
}

/// `encodeChannelModel` (`use-config-store.ts:319`).
pub fn encode_channel_model(channel_id: &str, model: &str) -> String {
    format!("{channel_id}{CHANNEL_MODEL_SEPARATOR}{}", model.trim())
}

/// `decodeChannelModel` (`use-config-store.ts:323`): split on the first `::`.
pub fn decode_channel_model(value: &str) -> Option<(String, String)> {
    let index = value.find(CHANNEL_MODEL_SEPARATOR)?;
    let channel_id = value.get(..index)?.to_string();
    let model = value
        .get(index + CHANNEL_MODEL_SEPARATOR.len()..)?
        .to_string();
    Some((channel_id, model))
}

/// `modelOptionName` (`use-config-store.ts:329`): the bare model name of an option.
pub fn model_option_name(value: &str) -> String {
    decode_channel_model(value).map_or_else(|| value.to_string(), |(_, model)| model)
}

/// `isTextOnlyBaseUrl` (`provider-presets.ts:52`).
pub fn is_text_only_base_url(base_url: &str) -> bool {
    let normalized = normalize_base_url(base_url);
    PROVIDER_PRESETS
        .iter()
        .any(|preset| preset.text_only && normalize_base_url(preset.base_url) == normalized)
}

/// `guessCapability` (`use-config-store.ts:135`).
pub fn guess_capability(name: &str) -> ModelCapability {
    let value = name.to_ascii_lowercase();
    if VIDEO_KEYWORDS.iter().any(|keyword| value.contains(keyword)) {
        return ModelCapability::Video;
    }
    if IMAGE_KEYWORDS.iter().any(|keyword| value.contains(keyword)) {
        return ModelCapability::Image;
    }
    ModelCapability::Text
}

/// `buildApiUrl` (`use-config-store.ts:406`): append `/v1` unless the base
/// already ends in it.
pub fn build_api_url(base_url: &str, path: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    let api_base = if normalized.to_ascii_lowercase().ends_with("/v1") {
        normalized.to_string()
    } else {
        format!("{normalized}/v1")
    };
    format!("{api_base}{path}")
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// `resolveModelChannel` (`use-config-store.ts:356`): the decoded channel, else the
/// first channel whose models contain the bare name, else the first channel.
pub fn resolve_model_channel<'a>(config: &'a AiConfig, value: &str) -> Option<&'a ModelChannel> {
    if let Some((channel_id, _)) = decode_channel_model(value) {
        return config.channels.iter().find(|item| item.id == channel_id);
    }
    let name = value.trim();
    let owner = if name.is_empty() {
        None
    } else {
        config
            .channels
            .iter()
            .find(|channel| channel.models.iter().any(|model| model.name == name))
    };
    owner.or_else(|| config.channels.first())
}

/// `resolveModelRequestConfig` (`use-config-store.ts:364`).
pub fn resolve_model_request_config(config: &AiConfig, value: &str) -> ModelRequestConfig {
    let channel = resolve_model_channel(config, value);
    let raw = if value.is_empty() {
        &config.model
    } else {
        value
    };
    ModelRequestConfig {
        model: model_option_name(raw),
        base_url: channel
            .map(|channel| channel.base_url.trim().to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| OPENROUTER_BASE_URL.to_string()),
        api_key: channel.map_or_else(String::new, |channel| channel.api_key.trim().to_string()),
        system_prompt: config.system_prompt.clone(),
        reasoning_effort: config.reasoning_effort,
        quality: config.quality.clone(),
        size: config.size.clone(),
        background: config.background.clone(),
        count: config.count.trim().parse().unwrap_or(1),
        video_seconds: config.video_seconds.clone(),
        vquality: config.vquality.clone(),
        video_generate_audio: config.video_generate_audio.clone(),
        video_watermark: config.video_watermark.clone(),
        video_mode: config.video_mode.clone(),
    }
}

/// `findChannelModel` (`use-config-store.ts:144`): the channel + model entry a
/// model option (encoded or bare) resolves to.
pub fn find_channel_model<'a>(
    config: &'a AiConfig,
    value: &str,
) -> Option<(&'a ModelChannel, &'a ChannelModel)> {
    let (channel, name) = match decode_channel_model(value) {
        Some((channel_id, name)) => (
            config.channels.iter().find(|item| item.id == channel_id),
            name,
        ),
        None => {
            let name = value.trim().to_string();
            let channel = config
                .channels
                .iter()
                .find(|item| item.models.iter().any(|model| model.name == name));
            (channel, name)
        }
    };
    let channel = channel?;
    let model = channel.models.iter().find(|item| item.name == name)?;
    Some((channel, model))
}

/// `resolveModelScript` (`use-config-store.ts:175`): the trimmed user script of
/// the model option, when it carries one.
pub fn resolve_model_script(config: &AiConfig, value: &str) -> Option<String> {
    find_channel_model(config, value)?
        .1
        .script
        .as_ref()
        .map(|script| script.trim().to_string())
        .filter(|script| !script.is_empty())
}

/// `resolveModelForCapability` (`use-config-store.ts:161`).
pub fn resolve_model_for_capability(
    config: &AiConfig,
    current: Option<&str>,
    capability: ModelCapability,
) -> String {
    let default_model = config.model_for_capability(capability);
    let fallback = AiConfig::default().model_for_capability(capability);
    if let Some(current) = current {
        if !current.is_empty() && model_matches_capability(config, current, capability) {
            return current.to_string();
        }
    }
    if !default_model.is_empty() && model_matches_capability(config, &default_model, capability) {
        return default_model;
    }
    fallback
}

/// `selectableModelsByCapability` (`use-config-store.ts:169`).
pub fn selectable_models_by_capability(
    config: &AiConfig,
    capability: ModelCapability,
) -> Vec<String> {
    config
        .channels
        .iter()
        .flat_map(|channel| {
            channel
                .models
                .iter()
                .filter(|model| model.capability == capability)
                .map(|model| encode_channel_model(&channel.id, &model.name))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `resolveModelChannel(...).apiKey` readiness (`isAiConfigReady`).
pub fn is_ai_config_ready(config: &AiConfig, model: &str) -> bool {
    !model.trim().is_empty()
        && resolve_model_channel(config, model)
            .is_some_and(|channel| !channel.api_key.trim().is_empty())
}

fn model_capability_of(config: &AiConfig, value: &str) -> Option<ModelCapability> {
    let (channel_id, name) = match decode_channel_model(value) {
        Some(decoded) => decoded,
        None => {
            let name = value.trim();
            let channel = config
                .channels
                .iter()
                .find(|item| item.models.iter().any(|model| model.name == name))?;
            return channel
                .models
                .iter()
                .find(|model| model.name == name)
                .map(|model| model.capability);
        }
    };
    let channel = config.channels.iter().find(|item| item.id == channel_id)?;
    channel
        .models
        .iter()
        .find(|model| model.name == name)
        .map(|model| model.capability)
}

fn model_matches_capability(config: &AiConfig, value: &str, capability: ModelCapability) -> bool {
    model_capability_of(config, value) == Some(capability)
}

impl AiConfig {
    /// The per-capability fixed model field.
    pub fn model_for_capability(&self, capability: ModelCapability) -> String {
        match capability {
            ModelCapability::Text => self.text_model.clone(),
            ModelCapability::Image => self.image_model.clone(),
            ModelCapability::Video => self.video_model.clone(),
        }
    }

    /// `modelOptionsFromChannels` + `uniqueModelOptions`: the flat, order-preserving,
    /// deduped list of encodable model options.
    pub fn model_options(&self) -> Vec<String> {
        let mut options: Vec<String> = Vec::new();
        for channel in &self.channels {
            for model in &channel.models {
                let option = encode_channel_model(&channel.id, &model.name);
                if !options.contains(&option) {
                    options.push(option);
                }
            }
        }
        options
    }

    /// Post-load normalization: rebuilds the two canonical channels from
    /// [`PROVIDER_PRESETS`] (keeping each stored API key), drops every other
    /// channel and forces the three fixed model assignments. Older configs are
    /// repaired silently.
    pub fn normalize(mut self) -> Self {
        let stored = std::mem::take(&mut self.channels);
        self.channels = PROVIDER_PRESETS
            .iter()
            .map(|preset| {
                let api_key = stored
                    .iter()
                    .find(|channel| channel.id == preset.id)
                    .map_or_else(String::new, |channel| channel.api_key.clone());
                preset_channel(preset, api_key)
            })
            .collect();
        self.channel_mode = "local".to_string();
        self.api_format = default_api_format();
        self.text_model = fixed_model_for_capability(ModelCapability::Text);
        self.image_model = fixed_model_for_capability(ModelCapability::Image);
        self.video_model = fixed_model_for_capability(ModelCapability::Video);
        self.models = self.model_options();
        if self.count.trim().is_empty() {
            self.count = "1".to_string();
        }
        if self.quality.trim().is_empty() {
            self.quality = "auto".to_string();
        }
        if self.size.trim().is_empty() {
            self.size = "1:1".to_string();
        }
        if self.canvas_image_count.trim().is_empty() {
            self.canvas_image_count = "1".to_string();
        }
        if self.video_mode != "reference" {
            self.video_mode = "frames".to_string();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some<T>(value: Option<T>) -> T {
        match value {
            Some(value) => value,
            None => unreachable!("expected a value"),
        }
    }

    fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
        match value {
            Ok(value) => value,
            Err(error) => unreachable!("unexpected error: {error:?}"),
        }
    }

    fn channel<'a>(config: &'a AiConfig, id: &str) -> &'a ModelChannel {
        config
            .channels
            .iter()
            .find(|channel| channel.id == id)
            .unwrap_or_else(|| unreachable!("channel {id}"))
    }

    #[test]
    fn encode_decode_round_trip_preserves_the_separator_rule() {
        let encoded = encode_channel_model("openrouter", "openai/gpt-image-2.5-sunburst");
        assert_eq!(encoded, "openrouter::openai/gpt-image-2.5-sunburst");
        let (channel, model) = some(decode_channel_model(&encoded));
        assert_eq!(channel, "openrouter");
        assert_eq!(model, "openai/gpt-image-2.5-sunburst");
        assert_eq!(model_option_name(&encoded), "openai/gpt-image-2.5-sunburst");
        // A bare model name has no separator and decodes to itself.
        assert_eq!(decode_channel_model("bare-model"), None);
        assert_eq!(model_option_name("bare-model"), "bare-model");
    }

    #[test]
    fn build_api_url_normalizes_v1_once() {
        assert_eq!(
            build_api_url("https://openrouter.ai/api/v1", "/images"),
            "https://openrouter.ai/api/v1/images"
        );
        // DeepSeek's official base URL carries no `/v1`; it is added exactly once.
        assert_eq!(
            build_api_url(DEEPSEEK_BASE_URL, "/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            build_api_url("https://api.deepseek.com/", "/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        // Trailing slashes are collapsed and an existing /v1 is not duplicated.
        assert_eq!(
            build_api_url("https://example.com/v1/", "/videos"),
            "https://example.com/v1/videos"
        );
    }

    #[test]
    fn default_config_seeds_the_two_fixed_channels_and_models() {
        let config = AiConfig::default();
        assert_eq!(config.channels.len(), 2);
        assert_eq!(config.channels[0].id, "openrouter");
        assert_eq!(config.channels[0].name, "OpenRouter");
        assert_eq!(config.channels[0].base_url, OPENROUTER_BASE_URL);
        assert_eq!(config.channels[1].id, "deepseek");
        assert_eq!(config.channels[1].name, "DeepSeek");
        assert_eq!(config.channels[1].base_url, DEEPSEEK_BASE_URL);
        assert_eq!(
            config.channels[0]
                .models
                .iter()
                .map(|model| (model.name.as_str(), model.capability))
                .collect::<Vec<_>>(),
            vec![
                (IMAGE_MODEL, ModelCapability::Image),
                ("minimax/hailuo-3-max", ModelCapability::Video),
                ("minimax/hailuo-3", ModelCapability::Video),
            ]
        );
        assert_eq!(
            config.channels[1]
                .models
                .iter()
                .map(|model| (model.name.as_str(), model.capability))
                .collect::<Vec<_>>(),
            vec![(DEEPSEEK_TEXT_MODEL, ModelCapability::Text)]
        );
        assert_eq!(config.text_model, "deepseek::deepseek-flash");
        assert_eq!(
            config.image_model,
            "openrouter::openai/gpt-image-2.5-sunburst"
        );
        assert_eq!(config.video_model, "openrouter::minimax/hailuo-3-max");
        assert_eq!(PROVIDER_PRESETS.len(), 2);
        assert_eq!(ModelCapability::ALL.len(), 3);
    }

    #[test]
    fn guess_capability_matches_the_keyword_lists() {
        assert_eq!(guess_capability("minimax/hailuo-3"), ModelCapability::Video);
        assert_eq!(
            guess_capability("openai/gpt-image-2.5"),
            ModelCapability::Image
        );
        assert_eq!(guess_capability("deepseek-flash"), ModelCapability::Text);
    }

    #[test]
    fn resolve_request_config_uses_the_owning_channel_credentials() {
        let mut config = AiConfig::default();
        config.channels[1].api_key = "sk-deepseek".to_string();
        let value = encode_channel_model("deepseek", DEEPSEEK_TEXT_MODEL);
        let resolved = resolve_model_request_config(&config, &value);
        assert_eq!(resolved.model, DEEPSEEK_TEXT_MODEL);
        assert_eq!(resolved.base_url, "https://api.deepseek.com");
        assert_eq!(resolved.api_key, "sk-deepseek");
        assert_eq!(
            build_api_url(&resolved.base_url, "/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert!(is_ai_config_ready(&config, &value));
    }

    #[test]
    fn resolve_request_config_falls_back_to_openrouter_when_no_channel() {
        let config = AiConfig {
            channels: Vec::new(),
            ..AiConfig::default()
        };
        let resolved = resolve_model_request_config(&config, "some-model");
        assert_eq!(resolved.base_url, OPENROUTER_BASE_URL);
        assert_eq!(resolved.model, "some-model");
        assert!(resolved.api_key.is_empty());
    }

    #[test]
    fn resolve_for_capability_prefers_current_then_default_then_fallback() {
        let mut config = AiConfig::default();
        let current = encode_channel_model("openrouter", VIDEO_MODEL);
        assert_eq!(
            resolve_model_for_capability(&config, Some(&current), ModelCapability::Video),
            current
        );
        config.text_model = encode_channel_model("deepseek", DEEPSEEK_TEXT_MODEL);
        assert_eq!(
            resolve_model_for_capability(&config, None, ModelCapability::Text),
            config.text_model
        );
        // Image default with no override resolves to the forced IMAGE_MODEL.
        assert_eq!(
            resolve_model_for_capability(&config, None, ModelCapability::Image),
            "openrouter::openai/gpt-image-2.5-sunburst"
        );
    }

    #[test]
    fn selectable_models_are_namespaced_per_capability() {
        let config = AiConfig::default();
        let videos = selectable_models_by_capability(&config, ModelCapability::Video);
        assert_eq!(
            videos,
            vec![
                "openrouter::minimax/hailuo-3-max".to_string(),
                "openrouter::minimax/hailuo-3".to_string(),
            ]
        );
        assert_eq!(
            selectable_models_by_capability(&config, ModelCapability::Text),
            vec!["deepseek::deepseek-flash".to_string()]
        );
    }

    #[test]
    fn normalize_repairs_an_old_matilda_audio_speech_config() {
        // An older saved config: a Matilda channel, extra OpenRouter models, the
        // retired DeepSeek models, audio/speech models and the removed audio
        // fields. Unknown fields must be ignored and the structure repaired.
        let saved = r#"{
            "channelMode": "local",
            "channels": [
                {
                    "id": "matilda",
                    "name": "Matilda (Maincode)",
                    "baseUrl": "/matilda/v1",
                    "apiKey": "sk-matilda",
                    "models": [{ "name": "matilda", "capability": "text" }]
                },
                {
                    "id": "openrouter",
                    "name": "OpenRouter",
                    "baseUrl": "https://openrouter.ai/api/v1",
                    "apiKey": "sk-or",
                    "models": [
                        { "name": "z-ai/glm-5.3", "capability": "text" },
                        { "name": "fish-audio/s2.1-pro", "capability": "speech" },
                        { "name": "google/lyria-3-pro-preview", "capability": "audio" }
                    ]
                },
                {
                    "id": "deepseek",
                    "name": "DeepSeek",
                    "baseUrl": "https://api.deepseek.com/v1",
                    "apiKey": "sk-ds",
                    "models": [{ "name": "deepseek-v4-pro", "capability": "text" }]
                }
            ],
            "textModel": "deepseek::deepseek-v4-pro",
            "imageModel": "openrouter::google/lyria-3-pro-preview",
            "videoModel": "openrouter::minimax/hailuo-3",
            "audioModel": "openai/tts-1",
            "speechModel": "fish-audio/s2.1-pro",
            "audioVoice": "nova",
            "audioFormat": "opus",
            "audioSpeed": "1.5",
            "audioInstructions": "calm"
        }"#;
        let loaded: AiConfig = ok(serde_json::from_str(saved));
        let config = loaded.normalize();
        assert_eq!(config.channels.len(), 2);
        assert_eq!(config.channels[0].id, "openrouter");
        assert_eq!(config.channels[1].id, "deepseek");
        assert_eq!(channel(&config, "openrouter").name, "OpenRouter");
        assert_eq!(channel(&config, "openrouter").base_url, OPENROUTER_BASE_URL);
        assert_eq!(channel(&config, "deepseek").name, "DeepSeek");
        assert_eq!(channel(&config, "deepseek").base_url, DEEPSEEK_BASE_URL);
        assert_eq!(
            channel(&config, "openrouter").api_key,
            "sk-or",
            "the stored API key survives"
        );
        assert_eq!(channel(&config, "deepseek").api_key, "sk-ds");
        assert_eq!(
            channel(&config, "openrouter")
                .models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            vec![IMAGE_MODEL, "minimax/hailuo-3-max", "minimax/hailuo-3"]
        );
        assert_eq!(
            channel(&config, "deepseek")
                .models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            vec![DEEPSEEK_TEXT_MODEL]
        );
        assert_eq!(config.text_model, "deepseek::deepseek-flash");
        assert_eq!(
            config.image_model,
            "openrouter::openai/gpt-image-2.5-sunburst"
        );
        assert_eq!(config.video_model, "openrouter::minimax/hailuo-3-max");
        assert!(!format!("{config:?}").contains("audio_model"));
        assert!(!format!("{config:?}").contains("speech_model"));
        assert!(!format!("{config:?}").contains("matilda"));
        assert_eq!(
            config.models,
            vec![
                "openrouter::openai/gpt-image-2.5-sunburst".to_string(),
                "openrouter::minimax/hailuo-3-max".to_string(),
                "openrouter::minimax/hailuo-3".to_string(),
                "deepseek::deepseek-flash".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_drops_extra_channels_and_keeps_the_known_keys() {
        let mut config = AiConfig::default();
        config.channels.push(ModelChannel {
            id: "extra".to_string(),
            name: "Extra".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "sk-extra".to_string(),
            api_format: "openai".to_string(),
            models: Vec::new(),
        });
        config.channels[0].api_key = "sk-or".to_string();
        let normalized = config.normalize();
        assert_eq!(normalized.channels.len(), 2);
        assert_eq!(normalized.channels[0].api_key, "sk-or");
        assert!(normalized
            .channels
            .iter()
            .all(|channel| channel.id != "extra"));
    }

    #[test]
    fn normalize_seeds_the_channels_when_missing() {
        let empty = AiConfig {
            channels: Vec::new(),
            ..AiConfig::default()
        }
        .normalize();
        assert_eq!(empty.channels.len(), 2);
        assert_eq!(empty.channels[0].id, "openrouter");
        assert_eq!(empty.channels[1].id, "deepseek");
        assert!(empty.channels[0].api_key.is_empty());
    }

    #[test]
    fn config_round_trips_through_json_with_camel_case() {
        let config = AiConfig::default();
        let json = ok(serde_json::to_string(&config));
        assert!(json.contains("\"imageModel\":"));
        assert!(json.contains("\"channelMode\":\"local\""));
        assert!(!json.contains("audioModel"));
        assert!(!json.contains("speechModel"));
        let back: AiConfig = ok(serde_json::from_str(&json));
        assert_eq!(back, config);
    }

    #[test]
    fn partial_json_loads_with_defaults() {
        let back: AiConfig = ok(serde_json::from_str(r#"{"imageModel":"custom"}"#));
        assert_eq!(back.image_model, "custom");
        assert_eq!(back.channels.len(), 2);
        // Loading normalizes the lock even when the file overrode a model.
        let normalized = back.normalize();
        assert_eq!(
            normalized.image_model,
            "openrouter::openai/gpt-image-2.5-sunburst"
        );
    }

    #[test]
    fn old_json_with_canvas_background_mode_still_loads() {
        // The canvas-background setting is gone; an `aiConfig` written by an
        // older build still carries its key. It must be ignored, not rejected.
        let saved = r#"{
            "channelMode": "local",
            "imageModel": "openrouter::openai/gpt-image-2.5-sunburst",
            "canvasImageCount": "2",
            "canvasBackgroundMode": "lines"
        }"#;
        let loaded: AiConfig = ok(serde_json::from_str(saved));
        assert_eq!(loaded.canvas_image_count, "2");
        let config = loaded.normalize();
        assert_eq!(config.channels.len(), 2);
        assert_eq!(config.canvas_image_count, "2");
        // The removed field is neither kept nor re-serialized.
        let json = ok(serde_json::to_string(&config));
        assert!(!json.contains("canvasBackgroundMode"));
    }

    #[test]
    fn model_script_resolves_by_encoded_and_bare_name() {
        let mut config = AiConfig::default();
        config.channels[0].models.push(ChannelModel {
            name: "my/video-model".to_string(),
            capability: ModelCapability::Video,
            script: Some("  {\"endpoint\":\"/v\"}  ".to_string()),
        });
        let encoded = encode_channel_model("openrouter", "my/video-model");
        assert_eq!(
            resolve_model_script(&config, &encoded).as_deref(),
            Some("{\"endpoint\":\"/v\"}")
        );
        assert_eq!(
            resolve_model_script(&config, "my/video-model").as_deref(),
            Some("{\"endpoint\":\"/v\"}")
        );
        assert_eq!(resolve_model_script(&config, VIDEO_MODEL), None);
        assert_eq!(resolve_model_script(&config, "unknown"), None);
    }
}
