//! The typed manifest error, one variant per failing field.

use thiserror::Error;

/// A manifest that failed to parse or validate. Every variant names the exact
/// field so the installer UI can point at it.
#[derive(Debug, Error, PartialEq)]
pub enum ManifestError {
    #[error("manifest is not valid JSON: {message}")]
    Syntax { message: String },
    #[error("manifest is missing required field `{field}`")]
    MissingField { field: &'static str },
    #[error("node at index {index} is missing required field `{field}`")]
    MissingNodeField { index: usize, field: &'static str },
    #[error("plugin declares no nodes")]
    NoNodes,
    #[error("plugin id `{id}` is invalid: {reason}")]
    InvalidId { id: String, reason: &'static str },
    #[error("version `{version}` is not a valid semantic version")]
    InvalidVersion { version: String },
    #[error("node type id `{type_id}` (index {index}) is invalid: {reason}")]
    InvalidNodeTypeId {
        index: usize,
        type_id: String,
        reason: &'static str,
    },
    #[error("duplicate node type id `{type_id}`")]
    DuplicateTypeId { type_id: String },
    #[error("node `{type_id}` declares unknown render kind `{render_kind}`")]
    UnknownRenderKind {
        type_id: String,
        render_kind: String,
    },
    #[error("node `{type_id}` declares a render kind this host cannot draw: `{render_kind}`")]
    UnsupportedRenderKind {
        type_id: String,
        render_kind: String,
    },
    #[error("node `{type_id}` declares unknown icon key `{icon}`")]
    UnknownIconKey { type_id: String, icon: String },
    #[error("node `{type_id}` size {width}x{height} is outside {min}..{max}")]
    SizeOutOfBounds {
        type_id: String,
        width: f64,
        height: f64,
        min: f64,
        max: f64,
    },
    #[error("node `{type_id}` has an invalid minimap colour `{color}`")]
    InvalidMinimapColor { type_id: String, color: String },
    #[error("plugin setting `{key}` is invalid: {reason}")]
    InvalidSetting { key: String, reason: &'static str },
}
