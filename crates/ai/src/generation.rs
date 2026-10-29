//! Generation-cost lookup, ported from `fetchGenerationCost`
//! (`web/src/services/api/image.ts:277`).
//!
//! `GET {base}/generation?id=<id>` returns the OpenRouter generation record. The
//! web reads `response.data?.data || response.data`, then `total_cost`, falling
//! back to `usage`; this port keeps that resolution ([`GenerationCost::resolved_usd`])
//! and additionally exposes the typed cost fields the web's generic left as
//! `number | null`.
//!
//! Unlike the web (which swallows every error and returns `undefined`), a
//! transport/HTTP/parse failure here is a typed [`AiError`]; a well-formed
//! response that carries no cost yields an all-`None` record. A caller that wants
//! the web's fall-through-to-estimate behaviour uses `.ok()` and treats `None`
//! the same as `Err`. The function returns `Result<_, AiError>` so it composes
//! with [`crate::retry::run_with_retry`] exactly like the other request helpers.

use reqwest::Client;
use serde_json::Value;

use crate::client::{bearer, send_cancelable, Cancel, REQUEST_TIMEOUT};
use crate::config::{build_api_url, ModelRequestConfig};
use crate::error::{map_reqwest_error, read_http_error, AiError};
use crate::json::value_as_f64;

/// A `/generation` cost record (`fetchGenerationCost`'s payload).
///
/// Numeric fields are `Option<f64>` because OpenRouter returns `null` for
/// fields that do not apply (e.g. `cache_discount` without a cache hit). Field
/// names are the wire names (already `snake_case`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenerationCost {
    /// Billed total in USD; the web's primary field.
    pub total_cost: Option<f64>,
    /// Discount applied from a cache hit, when any.
    pub cache_discount: Option<f64>,
    /// BYOK upstream inference cost, when any.
    pub upstream_inference_cost: Option<f64>,
    /// Generic usage amount; the web's fallback when `total_cost` is absent.
    pub usage: Option<f64>,
    /// Prompt tokens billed by the provider.
    pub tokens_prompt: Option<f64>,
    /// Completion tokens billed by the provider.
    pub tokens_completion: Option<f64>,
    /// Native prompt tokens (provider-reported).
    pub native_tokens_prompt: Option<f64>,
    /// Native completion tokens (provider-reported).
    pub native_tokens_completion: Option<f64>,
}

impl GenerationCost {
    /// `total_cost ?? usage`, finite only — the number `resolveImageCost`
    /// (`image.ts:294`) turns into a `source: "lookup"` cost. `None` when the
    /// record carries no usable cost, so the caller falls back to an estimate.
    pub fn resolved_usd(&self) -> Option<f64> {
        self.total_cost
            .or(self.usage)
            .filter(|amount| amount.is_finite())
    }
}

/// `fetchGenerationCost`'s payload read (`image.ts:283`): `data` when it is an
/// object, else the response body itself. Never fails — a missing record is an
/// all-`None` [`GenerationCost`].
pub fn parse_generation_cost(payload: &Value) -> GenerationCost {
    let record = payload
        .get("data")
        .filter(|data| data.is_object())
        .unwrap_or(payload);
    GenerationCost {
        total_cost: field(record, "total_cost"),
        cache_discount: field(record, "cache_discount"),
        upstream_inference_cost: field(record, "upstream_inference_cost"),
        usage: field(record, "usage"),
        tokens_prompt: field(record, "tokens_prompt"),
        tokens_completion: field(record, "tokens_completion"),
        native_tokens_prompt: field(record, "native_tokens_prompt"),
        native_tokens_completion: field(record, "native_tokens_completion"),
    }
}

fn field(record: &Value, key: &str) -> Option<f64> {
    record.get(key).and_then(value_as_f64)
}

/// `GET {base}/generation?id=<id>` (`fetchGenerationCost`, `image.ts:277`) with
/// the same Bearer auth and error mapping as the other request helpers.
pub async fn fetch_generation_cost(
    client: &Client,
    req: &ModelRequestConfig,
    generation_id: &str,
    cancel: &Cancel,
) -> Result<GenerationCost, AiError> {
    let generation_id = generation_id.trim();
    if generation_id.is_empty() {
        return Err(AiError::invalid(
            "A generation id is required to look up its cost",
        ));
    }
    let base = build_api_url(&req.base_url, "/generation");
    let mut url = reqwest::Url::parse(&base)
        .map_err(|error| AiError::invalid(format!("Invalid base URL: {error}")))?;
    url.query_pairs_mut().append_pair("id", generation_id);
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
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AiError::Parse(format!("generation cost response: {error}")))?;
    Ok(parse_generation_cost(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_nested_and_flat_payloads() {
        let nested = parse_generation_cost(&json!({"data": {"total_cost": 1.0}}));
        assert_eq!(nested.total_cost, Some(1.0));
        // `data` absent or null falls back to the top-level body.
        let flat = parse_generation_cost(&json!({"total_cost": 3.0}));
        assert_eq!(flat.total_cost, Some(3.0));
        let null_data = parse_generation_cost(&json!({"data": null, "total_cost": 4.0}));
        assert_eq!(null_data.total_cost, Some(4.0));
    }

    #[test]
    fn keeps_nulls_as_none() {
        let cost = parse_generation_cost(&json!({
            "data": { "total_cost": null, "tokens_prompt": null, "usage": 0.9 }
        }));
        assert_eq!(cost.total_cost, None);
        assert_eq!(cost.tokens_prompt, None);
        assert_eq!(cost.native_tokens_completion, None);
        assert_eq!(cost.resolved_usd(), Some(0.9));
    }

    #[test]
    fn resolved_prefers_total_cost_over_usage() {
        let cost = parse_generation_cost(&json!({"total_cost": 0.1, "usage": 0.2}));
        assert_eq!(cost.resolved_usd(), Some(0.1));
    }

    #[test]
    fn empty_payload_is_unpriced() {
        let cost = parse_generation_cost(&json!({}));
        assert_eq!(cost, GenerationCost::default());
        assert_eq!(cost.resolved_usd(), None);
    }
}
