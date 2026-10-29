//! Streaming text/vision responses (`/v1/responses`), ported from
//! `web/src/services/api/image.ts:389-463` (`consumeResponseStreamBlock`,
//! `consumeResponseStreamText`, `requestStreamingResponse`).
//!
//! The parser is deliberately dependency-free and pure so the SSE block framing
//! and event handling can be unit-tested without a socket; the async wrapper
//! feeds it decoded chunks from a `reqwest` byte stream.

use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};

use crate::client::{bearer, send_cancelable, Cancel};
use crate::config::{build_api_url, ModelRequestConfig, ReasoningEffort};
use crate::error::{map_reqwest_error, read_api_error_message, AiError};

/// One function call the model requested (`parseToolResponse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// `ToolResponseResult` (`image.ts:31`) plus the token usage the completed
/// response reported (the Responses API `usage` object).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<ResponseUsage>,
}

/// Token usage from a completed `/responses` payload (`usage`): the counts the
/// provider actually billed, for [`crate::generation_cost::estimate_token_cost`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

impl ResponseUsage {
    /// The counts as `estimate_token_cost`'s input; `has_reference` says whether
    /// the request carried reference images (the image-input rate applies).
    pub fn token_usage(&self, has_reference: bool) -> crate::generation_cost::TokenUsage {
        crate::generation_cost::TokenUsage {
            prompt_tokens: self.input_tokens.map(|value| value as f64),
            completion_tokens: self.output_tokens.map(|value| value as f64),
            total_tokens: self.total_tokens.map(|value| value as f64),
            has_reference,
        }
    }
}

/// The `usage` block of a `/responses` payload; `None` when it carries none.
fn parse_response_usage(payload: &Value) -> Option<ResponseUsage> {
    let usage = payload.get("usage")?;
    let count = |key: &str| usage.get(key).and_then(Value::as_u64);
    Some(ResponseUsage {
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
        total_tokens: count("total_tokens"),
    })
}

/// `ResponseStreamState` (`image.ts:53`).
#[derive(Debug, Default, Clone)]
pub struct StreamState {
    pub buffer: String,
    pub text: String,
    pub payload: Option<Value>,
    pub error: Option<String>,
}

/// `consumeResponseStreamText` (`image.ts:416`): feed a decoded chunk (or a final
/// flush) and return whether the accumulated text changed.
pub fn consume_stream_text(state: &mut StreamState, chunk: &str, flush: bool) -> bool {
    state.buffer.push_str(chunk);
    let before = state.text.clone();
    while let Some((index, separator_len)) = find_block_break(&state.buffer) {
        let block = state.buffer.get(..index).unwrap_or_default().to_string();
        state.buffer = state
            .buffer
            .get(index + separator_len..)
            .unwrap_or_default()
            .to_string();
        consume_stream_block(&block, state);
    }
    if flush {
        let trailing = state.buffer.trim().to_string();
        if !trailing.is_empty() {
            let block = std::mem::take(&mut state.buffer);
            consume_stream_block(&block, state);
        }
    }
    state.text != before
}

/// Finds the next `\r?\n\r?\n` block separator, returning its byte index and length.
fn find_block_break(buffer: &str) -> Option<(usize, usize)> {
    let bytes = buffer.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'\n' {
            if bytes[index + 1] == b'\n' {
                return Some((index, 2));
            }
            if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') {
                return Some((index, 3));
            }
        }
        index += 1;
    }
    None
}

/// `consumeResponseStreamBlock` (`image.ts:389`): parse one SSE block's `data:`
/// lines and fold the event into the state.
fn consume_stream_block(block: &str, state: &mut StreamState) {
    let data: String = block
        .lines()
        .filter(|line| line.starts_with("data:"))
        .map(|line| line.get(5..).unwrap_or_default().trim_start_matches(' '))
        .collect::<Vec<_>>()
        .join("\n");
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return;
    }
    let Ok(event) = serde_json::from_str::<Value>(data) else {
        return;
    };
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let error_message = error_message_of(&event);
    if !error_message.is_empty() {
        state.error = Some(error_message);
    }
    if event_type == "response.output_text.delta" {
        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
            state.text.push_str(delta);
        }
    }
    if event_type == "response.output_text.done" && state.text.is_empty() {
        if let Some(text) = event.get("text").and_then(Value::as_str) {
            state.text = text.to_string();
        }
    }
    if event_type == "response.completed" {
        if let Some(response) = event.get("response") {
            if response.is_object() {
                state.payload = Some(response.clone());
            }
        }
    } else if event.get("output").is_some_and(Value::is_array) {
        state.payload = Some(event.clone());
    }
}

/// `responseErrorMessage` (`image.ts:362`).
fn error_message_of(value: &Value) -> String {
    read_api_error_message(value)
}

/// `parseToolResponse` (`image.ts:339`).
pub fn parse_tool_response(payload: &Value) -> ToolResponse {
    let output = payload
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text_from_output: String = output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
                .into()
        })
        .collect();
    let output_text = payload
        .get("output_text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content = if output_text.is_empty() {
        text_from_output
    } else {
        output_text.to_string()
    };
    let tool_calls = output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|item| {
            let id = item
                .get("call_id")
                .and_then(Value::as_str)
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or_default();
            let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            Some(ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}")
                    .to_string(),
            })
        })
        .collect();
    ToolResponse {
        content,
        tool_calls,
        usage: parse_response_usage(payload),
    }
}

/// `validateResponsePayload` (`image.ts:374`).
fn validate_payload(payload: &Value) -> Result<(), AiError> {
    if let Some(code) = payload.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let message = payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("Request failed");
            return Err(AiError::Api(message.to_string()));
        }
    }
    if let Some(message) = payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return Err(AiError::Api(message.to_string()));
    }
    Ok(())
}

/// Builds a Responses-API input array from a prompt and optional reference image
/// URLs (the desktop port of `withSystemMessage` + `toResponseInput`): a real
/// `system` role when the config carries a system prompt, then one `user` turn.
pub fn responses_input(req: &ModelRequestConfig, prompt: &str, images: &[String]) -> Vec<Value> {
    let mut input: Vec<Value> = Vec::new();
    let system = req.system_prompt.trim();
    if !system.is_empty() {
        input.push(json!({ "role": "system", "content": system }));
    }
    let mut content: Vec<Value> = vec![json!({ "type": "input_text", "text": prompt })];
    for url in images {
        content.push(json!({ "type": "input_image", "image_url": url }));
    }
    input.push(json!({ "role": "user", "content": content }));
    input
}

/// The JSON body for `POST /responses` (`requestImageQuestion`'s `toResponseInput`).
pub fn responses_body(req: &ModelRequestConfig, prompt: &str, images: &[String]) -> Value {
    let mut body = json!({
        "model": req.model,
        "input": responses_input(req, prompt, images),
        "stream": true,
    });
    if req.reasoning_effort != ReasoningEffort::Auto {
        body["reasoning"] = json!({ "effort": req.reasoning_effort.as_str() });
    }
    body
}

/// `requestStreamingResponse` (`image.ts:431`): POST the body to `/responses`,
/// parse the SSE stream, and call `on_delta` with the growing text.
pub async fn stream_response<F>(
    client: &Client,
    req: &ModelRequestConfig,
    body: &Value,
    cancel: &Cancel,
    on_delta: &mut F,
) -> Result<ToolResponse, AiError>
where
    F: FnMut(&str),
{
    let url = build_api_url(&req.base_url, "/responses");
    let builder = client
        .post(&url)
        .header(reqwest::header::AUTHORIZATION, bearer(&req.api_key))
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body);
    let response = send_cancelable(builder, cancel).await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(crate::error::read_http_error(status.as_u16(), &text));
    }

    let mut state = StreamState::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(AiError::Canceled),
        item = stream.next() => item,
    } {
        let chunk = chunk.map_err(|error| map_reqwest_error(&error))?;
        let text = String::from_utf8_lossy(&chunk);
        if consume_stream_text(&mut state, &text, false) {
            on_delta(state.text.as_str());
        }
        if let Some(error) = state.error.clone() {
            return Err(AiError::Api(error));
        }
    }
    if consume_stream_text(&mut state, "", true) {
        on_delta(state.text.as_str());
    }
    if let Some(error) = state.error {
        return Err(AiError::Api(error));
    }
    let Some(payload) = state.payload else {
        return Ok(ToolResponse {
            content: state.text,
            tool_calls: Vec::new(),
            usage: None,
        });
    };
    validate_payload(&payload)?;
    let mut result = parse_tool_response(&payload);
    if !state.text.is_empty() {
        result.content = state.text;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&str]) -> StreamState {
        let mut state = StreamState::default();
        for chunk in chunks {
            consume_stream_text(&mut state, chunk, false);
        }
        consume_stream_text(&mut state, "", true);
        state
    }

    #[test]
    fn accumulates_output_text_deltas_across_block_boundaries() {
        let state = feed(&[
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\ndata: [DONE]\n\n",
        ]);
        assert_eq!(state.text, "Hello");
        assert!(state.error.is_none());
    }

    #[test]
    fn splits_a_chunk_that_carries_a_partial_block() {
        let mut state = StreamState::default();
        consume_stream_text(&mut state, "data: {\"type\":\"response.output", false);
        consume_stream_text(&mut state, "_text.delta\",\"delta\":\"A\"}\n\n", false);
        assert_eq!(state.text, "A");
    }

    #[test]
    fn prefers_done_text_only_when_no_deltas_arrived() {
        let state =
            feed(&["data: {\"type\":\"response.output_text.done\",\"text\":\"final\"}\n\n"]);
        assert_eq!(state.text, "final");
    }

    #[test]
    fn captures_the_completed_response_payload() {
        let state = feed(&[
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"output_text\":\"hi\",\"output\":[]}}\n\n",
        ]);
        assert!(state.payload.is_some());
    }

    #[test]
    fn surfaces_stream_error_events() {
        let state = feed(&["data: {\"type\":\"error\",\"msg\":\"boom\"}\n\n"]);
        assert_eq!(state.error.as_deref(), Some("boom"));
    }

    #[test]
    fn parses_output_text_and_function_calls() {
        let payload = json!({
            "output": [
                {"type": "message", "content": [{"type": "output_text", "text": "answer"}]},
                {"type": "function_call", "call_id": "c1", "name": "do_thing", "arguments": "{}"}
            ]
        });
        let parsed = parse_tool_response(&payload);
        assert_eq!(parsed.content, "answer");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "do_thing");
    }

    #[test]
    fn responses_body_adds_system_role_and_reasoning() {
        let req = ModelRequestConfig {
            model: "m".to_string(),
            base_url: "https://x/v1".to_string(),
            api_key: String::new(),
            system_prompt: "be terse".to_string(),
            reasoning_effort: ReasoningEffort::High,
            quality: String::new(),
            size: String::new(),
            background: String::new(),
            count: 1,
            video_seconds: String::new(),
            vquality: String::new(),
            video_generate_audio: String::new(),
            video_watermark: String::new(),
            video_mode: String::new(),
        };
        let body = responses_body(&req, "hi", &["data:image/png;base64,AA".to_string()]);
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["reasoning"]["effort"], json!("high"));
        let input = body["input"].as_array().cloned().unwrap_or_default();
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["role"], json!("system"));
        assert_eq!(input[1]["content"][1]["type"], json!("input_image"));
    }
}
