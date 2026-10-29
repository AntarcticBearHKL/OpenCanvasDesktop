//! Error mapping, ported from `web/src/services/api/api-error.ts`.
//!
//! The web unwraps the many shapes an OpenAI-compatible gateway returns an error
//! in (plain string, serialized JSON string, HTML error page, or an object with
//! `msg`/`message`/`error`/`detail`) and maps HTTP statuses to localized text.
//! This module keeps that behaviour and adds the typed [`AiError`] the app
//! surfaces as a toast. The i18n lookups become literal English strings (the
//! desktop app is en-US only today).

use serde_json::Value;

/// A typed AI-service failure. Every variant carries a UI-ready message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    /// The request was aborted by the user (Stop).
    #[error("Request canceled")]
    Canceled,
    /// Transport failure (DNS, connection refused, TLS, …).
    #[error("{0}")]
    Network(String),
    /// The upstream endpoint returned an error envelope / non-2xx status.
    #[error("{0}")]
    Api(String),
    /// The config or request inputs are unusable (missing model/key/base URL, bad size).
    #[error("{0}")]
    Invalid(String),
    /// The polling loop exhausted its attempt budget.
    #[error("{0}")]
    Timeout(String),
    /// A response could not be decoded into the expected shape.
    #[error("{0}")]
    Parse(String),
    /// A video task reached a terminal `failed`/`cancelled`/`expired` state.
    #[error("{0}")]
    VideoFailed(String),
}

impl AiError {
    /// True when the failure is a user-initiated cancellation.
    pub fn is_canceled(&self) -> bool {
        matches!(self, Self::Canceled)
    }

    /// Convenience for building an [`AiError::Invalid`] from anything stringy.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

/// `statusMessage` (`api-error.ts:45`): an HTTP status → a short message.
pub fn status_message(status: u16) -> String {
    match status {
        401 | 403 => "Authentication failed".to_string(),
        429 => "Rate limited by the provider".to_string(),
        404 => "Endpoint not found".to_string(),
        502 => "Bad gateway".to_string(),
        503 => "Service busy".to_string(),
        other => format!("Request failed (HTTP {other})"),
    }
}

/// `readApiErrorMessage` (`api-error.ts:13`): unwrap the error envelope shapes.
pub fn read_api_error_message(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => read_api_error_string(text),
        Value::Object(map) => {
            // `msg` || `message` || (`error` as string or `.message`) || `detail`.
            for key in ["msg", "message", "error", "detail"] {
                let message = map
                    .get(key)
                    .map_or_else(String::new, read_api_error_message);
                if !message.is_empty() {
                    return message;
                }
            }
            map.get("response")
                .and_then(|response| response.get("error"))
                .map_or_else(String::new, read_api_error_message)
        }
        _ => String::new(),
    }
}

fn read_api_error_string(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    match serde_json::from_str::<Value>(text) {
        Ok(parsed) => {
            let inner = read_api_error_message(&parsed);
            let inner = if inner.is_empty() {
                text.to_string()
            } else {
                inner
            };
            let empty_object = parsed.as_object().is_some_and(serde_json::Map::is_empty);
            // "{}"-style payloads carry no useful message.
            if empty_object && inner == text {
                String::new()
            } else {
                inner
            }
        }
        Err(_) if looks_like_html(text) => {
            let preview: String = text.chars().take(80).collect();
            format!("Provider returned an HTML error page: {preview}...")
        }
        Err(_) => text.to_string(),
    }
}

fn looks_like_html(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.first() == Some(&b'<') && bytes.get(1).is_some_and(u8::is_ascii_alphabetic)
}

/// `readAxiosError` (non-axios part): turn an HTTP status + body into an
/// [`AiError::Api`], preferring the body's own message over the status text.
pub fn read_http_error(status: u16, body: &str) -> AiError {
    let from_body = if body.trim().is_empty() {
        String::new()
    } else {
        match serde_json::from_str::<Value>(body) {
            Ok(value) => read_api_error_message(&value),
            Err(_) => read_api_error_string(body),
        }
    };
    let message = if from_body.is_empty() {
        status_message(status)
    } else {
        from_body
    };
    AiError::Api(message)
}

/// Map a `reqwest` transport error: an explicit timeout becomes
/// [`AiError::Timeout`], a dropped connection becomes [`AiError::Canceled`].
pub fn map_reqwest_error(error: &reqwest::Error) -> AiError {
    if error.is_timeout() {
        return AiError::Timeout("Request timed out".to_string());
    }
    if error.is_connect() {
        return AiError::Network(format!("Could not reach the provider: {error}"));
    }
    AiError::Network(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn api_message(error: AiError) -> String {
        match error {
            AiError::Api(message) => message,
            other => unreachable!("expected Api variant, got {other:?}"),
        }
    }

    #[test]
    fn status_message_maps_the_known_statuses() {
        assert_eq!(status_message(401), "Authentication failed");
        assert_eq!(status_message(403), "Authentication failed");
        assert_eq!(status_message(429), "Rate limited by the provider");
        assert_eq!(status_message(404), "Endpoint not found");
        assert_eq!(status_message(502), "Bad gateway");
        assert_eq!(status_message(503), "Service busy");
        assert_eq!(status_message(418), "Request failed (HTTP 418)");
    }

    #[test]
    fn unwraps_object_envelopes() {
        assert_eq!(read_api_error_message(&json!({"msg": "boom"})), "boom");
        assert_eq!(read_api_error_message(&json!({"error": "nope"})), "nope");
        assert_eq!(
            read_api_error_message(&json!({"error": {"message": "deep"}})),
            "deep"
        );
        assert_eq!(read_api_error_message(&json!({"detail": "d1"})), "d1");
        assert_eq!(
            read_api_error_message(&json!({"response": {"error": {"message": "inner"}}})),
            "inner"
        );
        assert_eq!(read_api_error_message(&json!({"unrelated": 1})), "");
    }

    #[test]
    fn unwraps_string_and_json_string_shapes() {
        assert_eq!(
            read_api_error_message(&json!("plain failure")),
            "plain failure"
        );
        assert_eq!(
            read_api_error_message(&json!("{\"message\":\"nested\"}")),
            "nested"
        );
        // An empty JSON object string carries no message.
        assert_eq!(read_api_error_message(&json!("{}")), "");
    }

    #[test]
    fn detects_html_error_pages() {
        let message = read_api_error_message(&json!("<html><body>500</body></html>"));
        assert!(message.contains("HTML error page"));
    }

    #[test]
    fn read_http_error_prefers_the_body_then_the_status() {
        assert_eq!(
            api_message(read_http_error(
                400,
                "{\"error\":{\"message\":\"bad size\"}}"
            )),
            "bad size"
        );
        assert_eq!(api_message(read_http_error(503, "")), "Service busy");
    }
}
