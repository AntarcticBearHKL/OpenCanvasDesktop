//! OCR request/response plumbing — port of `web/src/lib/canvas/canvas-ocr.ts`.
//!
//! The HTTP call stays in `open-canvas-ai`: this module builds the
//! `buildNodeResponseMessages` vision payload and parses the model's
//! transcription back into text plus optional layout blocks.

use serde::{Deserialize, Serialize};

use crate::error::PsError;

/// `canvas.projectPage.ocrPrompt` (en-US) — the vision instruction.
pub const OCR_PROMPT: &str = "Transcribe every piece of visible text in this image exactly as it appears, preserving the original line breaks and order. Do not translate, summarise, explain or add any commentary. Output only the transcribed text.";

/// `ocrPrompt()` (canvas-ocr.ts:7).
pub const fn ocr_prompt() -> &'static str {
    OCR_PROMPT
}

/// The subset of the web's `ReferenceImage` the OCR request needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrImageRef {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub data_url: String,
}

impl OcrImageRef {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        mime_type: impl Into<String>,
        data_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            mime_type: mime_type.into(),
            data_url: data_url.into(),
        }
    }
}

/// `{ image_url: { url } }` (canvas-node-generation.ts:130).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrImageUrl {
    pub url: String,
}

/// One content part of a `buildNodeResponseMessages` user turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OcrContentPart {
    Text { text: String },
    ImageUrl { image_url: OcrImageUrl },
}

/// A user turn: a text prompt plus the reference image, or bare text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrMessage {
    pub role: String,
    pub content: Vec<OcrContentPart>,
}

/// The `buildNodeResponseMessages` payload the app hands to `crates/ai`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrRequest {
    pub prompt: String,
    pub messages: Vec<OcrMessage>,
}

impl OcrRequest {
    /// The reference image data URLs carried on the user turn, in order.
    pub fn image_urls(&self) -> Vec<String> {
        self.messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(|part| match part {
                OcrContentPart::ImageUrl { image_url } => Some(image_url.url.clone()),
                OcrContentPart::Text { .. } => None,
            })
            .collect()
    }
}

/// `buildNodeResponseMessages({ prompt, referenceImages: [image] })`
/// (canvas-node-generation.ts:122): a single user turn with text then image.
pub fn build_ocr_request(prompt: &str, image: &OcrImageRef) -> OcrRequest {
    OcrRequest {
        prompt: prompt.to_string(),
        messages: vec![OcrMessage {
            role: "user".to_string(),
            content: vec![
                OcrContentPart::Text {
                    text: prompt.to_string(),
                },
                OcrContentPart::ImageUrl {
                    image_url: OcrImageUrl {
                        url: image.data_url.clone(),
                    },
                },
            ],
        }],
    }
}

/// `extractImageText` request with the default OCR instruction.
pub fn build_ocr_request_default(image: &OcrImageRef) -> OcrRequest {
    build_ocr_request(ocr_prompt(), image)
}

/// `normaliseOcrText` (canvas-ocr.ts:11): strip an optional ``` fence, then
/// trailing whitespace per line, then the whole body.
pub fn normalise_ocr_text(value: &str) -> String {
    let trimmed = value.trim();
    let body = strip_code_fence(trimmed);
    body.split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// `^```[^\n]*\n([\s\S]*?)\n?```$` (canvas-ocr.ts:12).
fn strip_code_fence(trimmed: &str) -> &str {
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let Some(header_end) = rest.find('\n') else {
        return trimmed;
    };
    let inner_and_close = &rest[header_end + 1..];
    let Some(inner) = inner_and_close.strip_suffix("```") else {
        return trimmed;
    };
    inner.strip_suffix('\n').unwrap_or(inner)
}

/// One optional layout block (bounding box in source pixels) a capable model
/// may return alongside its transcription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrLayoutBlock {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Parsed OCR output: the normalised text plus any layout blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrResult {
    pub text: String,
    pub blocks: Vec<OcrLayoutBlock>,
}

#[derive(Deserialize)]
struct OcrStructured {
    text: String,
    #[serde(default)]
    blocks: Vec<OcrLayoutBlock>,
}

/// Result parser: normalise the model text; when the body is the structured
/// `{ "text", "blocks" }` shape (a layout-capable model), split out the blocks,
/// otherwise the whole body is the transcription. Empty text is [`PsError::OcrEmpty`]
/// (`canvas-ocr.ts:34`).
pub fn parse_ocr_response(raw: &str) -> Result<OcrResult, PsError> {
    let body = normalise_ocr_text(raw);
    let (text, blocks) = match serde_json::from_str::<OcrStructured>(&body) {
        Ok(parsed) => (normalise_ocr_text(&parsed.text), parsed.blocks),
        Err(_) => (body, Vec::new()),
    };
    if text.is_empty() {
        return Err(PsError::OcrEmpty);
    }
    Ok(OcrResult { text, blocks })
}

#[cfg(test)]
mod tests {

    use super::*;

    fn image() -> OcrImageRef {
        OcrImageRef::new("n1", "photo.png", "image/png", "data:image/png;base64,AA")
    }

    #[test]
    fn request_matches_the_web_messages_shape() {
        let request = build_ocr_request_default(&image());
        assert_eq!(request.prompt, OCR_PROMPT);
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, "user");
        assert_eq!(request.messages[0].content.len(), 2);
        assert_eq!(
            request.messages[0].content[0],
            OcrContentPart::Text {
                text: OCR_PROMPT.to_string()
            }
        );
        assert_eq!(
            request.image_urls(),
            vec!["data:image/png;base64,AA".to_string()]
        );
    }

    #[test]
    fn request_serialises_to_openai_content_parts() {
        let value = serde_json::to_value(build_ocr_request_default(&image())).expect("to_value");
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"][0]["type"], "text");
        assert_eq!(value["messages"][0]["content"][1]["type"], "image_url");
        assert_eq!(
            value["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AA"
        );
    }

    #[test]
    fn normalise_strips_fences_and_trailing_whitespace() {
        assert_eq!(normalise_ocr_text("  hello  \nworld\t "), "hello\nworld");
        assert_eq!(
            normalise_ocr_text("```\nline one\nline two\n```"),
            "line one\nline two"
        );
        assert_eq!(normalise_ocr_text("```text\n{\"a\": 1}```"), "{\"a\": 1}");
        assert_eq!(normalise_ocr_text("```"), "```");
        assert_eq!(normalise_ocr_text("no fence"), "no fence");
    }

    #[test]
    fn parse_plain_text_transcription() {
        let result = parse_ocr_response("```\nA B C\n```").expect("parse");
        assert_eq!(result.text, "A B C");
        assert!(result.blocks.is_empty());
    }

    #[test]
    fn parse_structured_layout_blocks() {
        let raw =
            r#"{"text":"Hi","blocks":[{"text":"Hi","x":1.0,"y":2.0,"width":3.0,"height":4.0}]}"#;
        let result = parse_ocr_response(raw).expect("parse");
        assert_eq!(result.text, "Hi");
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].height, 4.0);
    }

    #[test]
    fn empty_transcription_is_ocr_empty() {
        assert!(matches!(parse_ocr_response("   "), Err(PsError::OcrEmpty)));
        assert!(matches!(
            parse_ocr_response("```\n\n```"),
            Err(PsError::OcrEmpty)
        ));
    }
}
