//! Client-side generation-cost estimation, ported from
//! `web/src/lib/canvas/generation-cost.ts` (`MODEL_PRICES`,
//! `MODEL_TOKEN_PRICES`, `estimateGenerationCost`, `estimateTokenCost`,
//! `formatUsd`). Pure data + arithmetic with no dependencies, used when no
//! API/lookup price is available.

use crate::config::model_option_name;

/// `GenerationCostUnit` (`generation-cost.ts:3`), without the removed
/// audio/speech units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationCostUnit {
    Image,
    VideoSecond,
    Call,
}

impl GenerationCostUnit {
    /// The wire value (`"image" | "video-second" | "call"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::VideoSecond => "video-second",
            Self::Call => "call",
        }
    }
}

/// `GenerationCostSource` (`generation-cost.ts:5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationCostSource {
    Api,
    Lookup,
    Estimate,
}

/// `GenerationCost.reason` (`generation-cost.ts:7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationCostReason {
    UnpricedModel,
    UnpricedUnit,
}

/// `GenerationCost` (`generation-cost.ts:7`). Named `…Estimate` here to leave
/// [`crate::generation::GenerationCost`] (the API record) unambiguous.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenerationCostEstimate {
    pub usd: f64,
    pub priced: bool,
    pub source: GenerationCostSource,
    pub reason: Option<GenerationCostReason>,
}

/// `GENERATION_COST_RECORD_LIMIT` (`generation-cost.ts:29`).
pub const GENERATION_COST_RECORD_LIMIT: usize = 200;

/// `MODEL_PRICES` (`generation-cost.ts:13`): `(model id, unit, usd per unit)`.
const MODEL_PRICES: &[(&str, GenerationCostUnit, f64)] = &[
    ("gpt-image-1", GenerationCostUnit::Image, 0.04),
    ("dall-e-3", GenerationCostUnit::Image, 0.04),
    ("gpt-4o", GenerationCostUnit::Call, 0.005),
    ("gpt-4o-mini", GenerationCostUnit::Call, 0.0006),
    ("sora", GenerationCostUnit::VideoSecond, 0.1),
];

/// `MODEL_TOKEN_PRICES` (`generation-cost.ts:25`):
/// `(model id, inputText, inputImage, outputImage)`.
const MODEL_TOKEN_PRICES: &[(&str, f64, f64, f64)] = &[(
    "openai/gpt-image-2.5-sunburst",
    0.000_005,
    0.000_008,
    0.000_03,
)];

/// `priceModelId` (`generation-cost.ts:31`): the bare, trimmed, lowercased model.
pub fn price_model_id(model: &str) -> String {
    model_option_name(model).trim().to_ascii_lowercase()
}

fn model_price(model: &str) -> Option<(GenerationCostUnit, f64)> {
    let id = price_model_id(model);
    MODEL_PRICES
        .iter()
        .find(|(name, _, _)| *name == id.as_str())
        .map(|(_, unit, usd)| (*unit, *usd))
}

fn model_token_price(model: &str) -> Option<(f64, f64, f64)> {
    let id = price_model_id(model);
    MODEL_TOKEN_PRICES
        .iter()
        .find(|(name, _, _, _)| *name == id.as_str())
        .map(|(_, input_text, input_image, output_image)| {
            (*input_text, *input_image, *output_image)
        })
}

/// `Number((value).toFixed(6))` — the web's 6-decimal rounding.
fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn estimate(
    usd: f64,
    priced: bool,
    reason: Option<GenerationCostReason>,
) -> GenerationCostEstimate {
    GenerationCostEstimate {
        usd,
        priced,
        source: GenerationCostSource::Estimate,
        reason,
    }
}

/// `estimateGenerationCost` (`generation-cost.ts:39`).
pub fn estimate_generation_cost(
    model: &str,
    unit: GenerationCostUnit,
    quantity: f64,
) -> GenerationCostEstimate {
    let Some((price_unit, price_usd)) = model_price(model) else {
        return estimate(0.0, false, Some(GenerationCostReason::UnpricedModel));
    };
    if price_unit != unit || !quantity.is_finite() || quantity <= 0.0 {
        return estimate(0.0, false, Some(GenerationCostReason::UnpricedUnit));
    }
    estimate(round6(price_usd * quantity), true, None)
}

/// `estimateTokenCost`'s usage input (`generation-cost.ts:46`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenUsage {
    pub prompt_tokens: Option<f64>,
    pub completion_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub has_reference: bool,
}

/// The web's `Number(x) || fallback` truthiness: `0`, non-finite, and absent all
/// fall through to the fallback.
fn truthy(value: Option<f64>) -> Option<f64> {
    value.filter(|amount| amount.is_finite() && *amount != 0.0)
}

/// `estimateTokenCost` (`generation-cost.ts:46`); `None` when the model has no
/// token price or both token counts resolve to zero.
pub fn estimate_token_cost(model: &str, usage: TokenUsage) -> Option<GenerationCostEstimate> {
    let (input_text, input_image, output_image) = model_token_price(model)?;
    let completion = truthy(usage.completion_tokens).unwrap_or(0.0).max(0.0);
    let total = truthy(usage.total_tokens).unwrap_or(0.0);
    let prompt = truthy(usage.prompt_tokens)
        .unwrap_or(total - completion)
        .max(0.0);
    if prompt == 0.0 && completion == 0.0 {
        return None;
    }
    let input_rate = if usage.has_reference {
        input_image
    } else {
        input_text
    };
    Some(estimate(
        round6(prompt * input_rate + completion * output_image),
        true,
        None,
    ))
}

/// `formatUsd` (`generation-cost.ts:62`).
pub fn format_usd(usd: f64) -> String {
    if usd > 0.0 && usd < 0.001 {
        format!("${usd:.4}")
    } else if usd > 0.0 && usd < 0.01 {
        format!("${usd:.3}")
    } else {
        format!("${usd:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_model_id_strips_the_channel_and_lowercases() {
        assert_eq!(price_model_id("OpenRouter::GPT-Image-1"), "gpt-image-1");
        assert_eq!(price_model_id("  gpt-image-1 "), "gpt-image-1");
    }

    #[test]
    fn estimates_a_priced_image_call() {
        let cost = estimate_generation_cost("gpt-image-1", GenerationCostUnit::Image, 2.0);
        assert!(cost.priced);
        assert_eq!(cost.usd, 0.08);
        assert_eq!(cost.source, GenerationCostSource::Estimate);
        assert_eq!(cost.reason, None);
    }

    #[test]
    fn unpriced_model_and_unit_are_reported() {
        let unknown = estimate_generation_cost("nope", GenerationCostUnit::Image, 1.0);
        assert!(!unknown.priced);
        assert_eq!(unknown.reason, Some(GenerationCostReason::UnpricedModel));
        let mismatch = estimate_generation_cost("gpt-image-1", GenerationCostUnit::Call, 1.0);
        assert!(!mismatch.priced);
        assert_eq!(mismatch.reason, Some(GenerationCostReason::UnpricedUnit));
    }

    #[test]
    fn estimates_token_cost_from_rates() {
        let text = estimate_token_cost(
            "openai/gpt-image-2.5-sunburst",
            TokenUsage {
                prompt_tokens: Some(1000.0),
                completion_tokens: Some(500.0),
                total_tokens: None,
                has_reference: false,
            },
        )
        .unwrap_or_else(|| unreachable!("priced model"));
        assert_eq!(text.usd, 0.02);
        let reference = estimate_token_cost(
            "openai/gpt-image-2.5-sunburst",
            TokenUsage {
                prompt_tokens: Some(1000.0),
                completion_tokens: Some(500.0),
                total_tokens: None,
                has_reference: true,
            },
        )
        .unwrap_or_else(|| unreachable!("priced model"));
        assert_eq!(reference.usd, 0.023);
    }

    #[test]
    fn token_cost_is_none_for_unpriced_or_empty_usage() {
        assert_eq!(
            estimate_token_cost(
                "unknown-model",
                TokenUsage {
                    prompt_tokens: Some(10.0),
                    ..TokenUsage::default()
                }
            ),
            None
        );
        assert_eq!(
            estimate_token_cost("openai/gpt-image-2.5-sunburst", TokenUsage::default()),
            None
        );
    }

    #[test]
    fn token_cost_derives_prompt_tokens_from_total() {
        let cost = estimate_token_cost(
            "openai/gpt-image-2.5-sunburst",
            TokenUsage {
                completion_tokens: Some(100.0),
                total_tokens: Some(600.0),
                ..TokenUsage::default()
            },
        )
        .unwrap_or_else(|| unreachable!("priced model"));
        // prompt = 600 - 100 = 500 -> 500*0.000005 + 100*0.00003 = 0.0055
        assert_eq!(cost.usd, 0.0055);
    }

    #[test]
    fn format_usd_scales_precision_by_magnitude() {
        assert_eq!(format_usd(0.0), "$0.00");
        assert_eq!(format_usd(0.0005), "$0.0005");
        assert_eq!(format_usd(0.005), "$0.005");
        assert_eq!(format_usd(1.5), "$1.50");
    }
}
