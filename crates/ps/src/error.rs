//! Error type for the Image Studio domain. Used at the I/O boundary (PNG
//! decode/encode) and by the image-AI plumbing (`ocr`, `segmentation`), whose
//! pure math is infallible but whose host-provided runtimes can be absent.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PsError {
    /// The encoded image bytes could not be decoded.
    #[error("failed to decode image: {0}")]
    Decode(String),
    /// A surface could not be encoded.
    #[error("failed to encode image: {0}")]
    Encode(String),
    /// A decoded buffer's length did not match `width * height * 4`.
    #[error("dimension mismatch: expected {expected} bytes, found {found}")]
    Dimension { expected: usize, found: usize },
    /// OCR returned no usable text (`canvas.imageTools.ocrEmpty`).
    #[error("no text was recognised in the image")]
    OcrEmpty,
    /// A local segmenter was requested but the host supplies no model runtime.
    #[error("segmentation runtime is unavailable: {0}")]
    SegmenterUnavailable(String),
    /// A filter id is not in [`crate::algo_bridge::available`].
    #[error("unknown filter: {0}")]
    UnknownFilter(String),
    /// A filter exists but needs an input this document model cannot supply.
    #[error("filter '{id}' is unsupported: {reason}")]
    UnsupportedFilter { id: String, reason: String },
    /// The engine rejected the filter's geometry or a deserialized parameter set.
    #[error("invalid filter '{id}': {message}")]
    InvalidFilter { id: String, message: String },
    /// A parameter value had the wrong type or could not be parsed.
    #[error("invalid filter parameter '{parameter}': {message}")]
    InvalidFilterParameter { parameter: String, message: String },
    /// A pure helper was handed a value it cannot operate on.
    #[error("invalid {context}: {value}")]
    Invalid { context: String, value: String },
}
