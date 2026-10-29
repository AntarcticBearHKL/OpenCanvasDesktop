//! OpenRouter account inspection, ported from
//! `web/src/services/api/openrouter-account.ts` (`fetchOpenRouterKeyUsage`) plus
//! the sibling `/credits` endpoint.
//!
//! Both requests carry the same `Authorization: Bearer <key>` header and reuse
//! the crate's error mapping ([`read_http_error`] / [`map_reqwest_error`]); an
//! empty key fails before any request is made (the web's `account.missingKey`).
//! The web binds the base to the `OPENROUTER_BASE_URL` constant, so the primary
//! functions do too; the `…_from` variants take an explicit base (used by the
//! tests and by a self-hosted/relayed OpenRouter endpoint).

use reqwest::Client;
use serde_json::Value;

use crate::client::{bearer, send_cancelable, Cancel, REQUEST_TIMEOUT};
use crate::config::OPENROUTER_BASE_URL;
use crate::error::{map_reqwest_error, read_http_error, AiError};
use crate::json::value_as_f64;

/// `OpenRouterKeyUsage` (`openrouter-account.ts:6`): per-key usage and limits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenRouterKeyUsage {
    pub usage: f64,
    pub usage_daily: f64,
    pub usage_weekly: f64,
    pub usage_monthly: f64,
    pub limit: Option<f64>,
    pub limit_remaining: Option<f64>,
    pub limit_reset: Option<String>,
    pub is_free_tier: bool,
}

impl OpenRouterKeyUsage {
    /// `fetchOpenRouterKeyUsage`'s payload read (`openrouter-account.ts:37`):
    /// `response.data?.data || {}` with `toAmount` / `toNullableAmount` coercion.
    pub fn from_payload(payload: &Value) -> Self {
        let data = payload.get("data").filter(|data| data.is_object());
        let amount = |key: &str| {
            data.and_then(|data| data.get(key))
                .and_then(value_as_f64)
                .unwrap_or(0.0)
        };
        let nullable = |key: &str| data.and_then(|data| data.get(key)).and_then(value_as_f64);
        Self {
            usage: amount("usage"),
            usage_daily: amount("usage_daily"),
            usage_weekly: amount("usage_weekly"),
            usage_monthly: amount("usage_monthly"),
            limit: nullable("limit"),
            limit_remaining: nullable("limit_remaining"),
            limit_reset: data
                .and_then(|data| data.get("limit_reset"))
                .and_then(Value::as_str)
                .map(str::to_string),
            is_free_tier: data
                .and_then(|data| data.get("is_free_tier"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// OpenRouter `/credits` (`{ data: { total_credits, total_usage } }`, USD).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenRouterCredits {
    pub total_credits: f64,
    pub total_usage: f64,
}

impl OpenRouterCredits {
    /// The `/credits` `data` envelope with the crate's amount coercion.
    pub fn from_payload(payload: &Value) -> Self {
        let data = payload.get("data").filter(|data| data.is_object());
        let amount = |key: &str| {
            data.and_then(|data| data.get(key))
                .and_then(value_as_f64)
                .unwrap_or(0.0)
        };
        Self {
            total_credits: amount("total_credits"),
            total_usage: amount("total_usage"),
        }
    }

    /// Credits not yet spent (`total_credits - total_usage`).
    pub fn remaining(&self) -> f64 {
        self.total_credits - self.total_usage
    }
}

/// `fetchOpenRouterKeyUsage` (`openrouter-account.ts:30`) against
/// [`OPENROUTER_BASE_URL`].
pub async fn fetch_openrouter_key_usage(
    client: &Client,
    api_key: &str,
    cancel: &Cancel,
) -> Result<OpenRouterKeyUsage, AiError> {
    fetch_openrouter_key_usage_from(client, OPENROUTER_BASE_URL, api_key, cancel).await
}

/// `/key` against an explicit base URL.
pub async fn fetch_openrouter_key_usage_from(
    client: &Client,
    base_url: &str,
    api_key: &str,
    cancel: &Cancel,
) -> Result<OpenRouterKeyUsage, AiError> {
    let value = get_openrouter(client, base_url, "/key", api_key, cancel).await?;
    Ok(OpenRouterKeyUsage::from_payload(&value))
}

/// `/credits` against [`OPENROUTER_BASE_URL`].
pub async fn fetch_openrouter_credits(
    client: &Client,
    api_key: &str,
    cancel: &Cancel,
) -> Result<OpenRouterCredits, AiError> {
    fetch_openrouter_credits_from(client, OPENROUTER_BASE_URL, api_key, cancel).await
}

/// `/credits` against an explicit base URL.
pub async fn fetch_openrouter_credits_from(
    client: &Client,
    base_url: &str,
    api_key: &str,
    cancel: &Cancel,
) -> Result<OpenRouterCredits, AiError> {
    let value = get_openrouter(client, base_url, "/credits", api_key, cancel).await?;
    Ok(OpenRouterCredits::from_payload(&value))
}

async fn get_openrouter(
    client: &Client,
    base_url: &str,
    path: &str,
    api_key: &str,
    cancel: &Cancel,
) -> Result<Value, AiError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AiError::invalid("Missing API key"));
    }
    let base = base_url.trim().trim_end_matches('/');
    let url = format!("{base}{path}");
    let builder = client
        .get(&url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, bearer(key));
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
        .map_err(|error| AiError::Parse(format!("account response: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_usage_defaults_when_data_is_missing() {
        let usage = OpenRouterKeyUsage::from_payload(&json!({}));
        assert_eq!(usage, OpenRouterKeyUsage::default());
        assert_eq!(usage.limit, None);
        assert!(!usage.is_free_tier);
    }

    #[test]
    fn key_usage_coerces_numbers_and_keeps_nulls() {
        let usage = OpenRouterKeyUsage::from_payload(&json!({
            "data": {
                "usage": 1.5,
                "usage_daily": "0.25",
                "usage_weekly": null,
                "limit": null,
                "limit_remaining": 3,
                "limit_reset": "2026-01-01T00:00:00Z",
                "is_free_tier": true
            }
        }));
        assert_eq!(usage.usage, 1.5);
        assert_eq!(usage.usage_daily, 0.25);
        assert_eq!(usage.usage_weekly, 0.0);
        assert_eq!(usage.usage_monthly, 0.0);
        assert_eq!(usage.limit, None);
        assert_eq!(usage.limit_remaining, Some(3.0));
        assert_eq!(usage.limit_reset.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert!(usage.is_free_tier);
    }

    #[test]
    fn credits_parse_and_report_remaining() {
        let credits = OpenRouterCredits::from_payload(&json!({
            "data": { "total_credits": 5.0, "total_usage": 1.25 }
        }));
        assert_eq!(credits.total_credits, 5.0);
        assert_eq!(credits.total_usage, 1.25);
        assert_eq!(credits.remaining(), 3.75);
    }
}
