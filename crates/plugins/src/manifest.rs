//! Declarative plugin manifest: the JSON a native plugin ships instead of a
//! JavaScript bundle.
//!
//! The web contract (`web/src/types/canvas-plugin.ts`) is React/DOM-bound: a
//! plugin exports `CanvasNodeDefinition`s whose `Content`/`Panel` are React
//! components, icons are `ReactNode`, and `setup` injects CSS. None of that can
//! run under Slint. A **manifest** replaces it: it declares the plugin identity,
//! its node definitions (each mapped onto a [`crate::render_kind::PluginRenderKind`]
//! the host already draws), a settings schema, and i18n strings.
//!
//! [`parse_manifest`] is the trust boundary: raw bytes are parsed once into a
//! typed [`PluginManifest`] with per-field validation errors, and everything
//! downstream receives typed values. [`emit_manifest`] writes the canonical form
//! (declaration-order keys, sorted maps) so a round-trip is byte-stable.
//!
//! - `model` — the manifest/node/setting data types and [`ManifestError`].
//! - `validate` — parse-time field validation.
//! - `convert` — manifest → [`crate::abi::CanvasNodeDefinition`] / [`crate::plugin::CanvasPlugin`].

mod convert;
mod error;
mod model;
mod validate;

#[cfg(test)]
mod tests;

pub use error::ManifestError;
pub use model::{
    PluginBuiltinPanelManifest, PluginManifest, PluginNodeManifest, PluginPanelManifest,
    PluginSettingKind, PluginSettingSpec, PluginSize, DEFAULT_LOCALE, KNOWN_ICON_KEYS,
    MAX_NODE_SIDE, MIN_NODE_SIDE, RENDER_KIND_KEY,
};

/// Parse raw manifest bytes into a validated, normalised [`PluginManifest`].
///
/// The host fetches the bytes; this function owns trust. Every failure is a
/// typed [`ManifestError`] naming the offending field.
pub fn parse_manifest(bytes: &[u8]) -> Result<PluginManifest, ManifestError> {
    let mut manifest: PluginManifest =
        serde_json::from_slice(bytes).map_err(|err| ManifestError::Syntax {
            message: err.to_string(),
        })?;
    manifest.validate()?;
    manifest.normalize();
    Ok(manifest)
}

/// Emit a manifest in its canonical form: declaration-order object keys and
/// sorted maps, so [`parse_manifest`] of the output equals the input.
pub fn emit_manifest(manifest: &PluginManifest) -> String {
    // Infallible for this model: no non-string map keys and no fallible
    // serialisers, so the only failure mode is unreachable.
    serde_json::to_string_pretty(manifest).unwrap_or_default()
}
