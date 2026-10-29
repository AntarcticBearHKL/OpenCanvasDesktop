//! Parse-time field validation. Every rule returns the first failure as a typed
//! [`ManifestError`]; a manifest that passes here is safe for [`super::convert`].

use super::error::ManifestError;
use super::model::{
    PluginManifest, PluginNodeManifest, PluginSettingKind, PluginSettingSpec, PluginSize,
    KNOWN_ICON_KEYS, MAX_NODE_SIDE, MIN_NODE_SIDE,
};
use crate::render_kind::PluginRenderKind;

impl PluginManifest {
    /// Validate every field, returning the first error found.
    pub fn validate(&self) -> Result<(), ManifestError> {
        validate_plugin_id(&self.id)?;
        if !self.version.trim().is_empty() {
            validate_semver(&self.version)?;
        }
        if self.nodes.is_empty() {
            return Err(ManifestError::NoNodes);
        }
        let mut seen: Vec<&str> = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            node.validate(index)?;
            if seen.contains(&node.type_id.as_str()) {
                return Err(ManifestError::DuplicateTypeId {
                    type_id: node.type_id.clone(),
                });
            }
            seen.push(node.type_id.as_str());
        }
        for (key, spec) in &self.settings {
            spec.validate(key)?;
        }
        Ok(())
    }
}

impl PluginNodeManifest {
    fn validate(&self, index: usize) -> Result<(), ManifestError> {
        if self.type_id.trim().is_empty() {
            return Err(ManifestError::MissingNodeField {
                index,
                field: "type",
            });
        }
        validate_node_type_id(&self.type_id, index)?;
        if self.title.trim().is_empty() {
            return Err(ManifestError::MissingNodeField {
                index,
                field: "title",
            });
        }
        if PluginRenderKind::from_key(&self.render_kind).is_none() {
            return Err(ManifestError::UnknownRenderKind {
                type_id: self.type_id.clone(),
                render_kind: self.render_kind.clone(),
            });
        }
        validate_icon(&self.type_id, &self.icon)?;
        if let Some(size) = &self.default_size {
            validate_size(&self.type_id, size)?;
        }
        if let Some(color) = &self.minimap_color {
            validate_color(&self.type_id, color)?;
        }
        Ok(())
    }
}

impl PluginSettingSpec {
    fn validate(&self, key: &str) -> Result<(), ManifestError> {
        if self.kind == PluginSettingKind::Select && self.options.is_empty() {
            return Err(ManifestError::InvalidSetting {
                key: key.to_string(),
                reason: "a select setting must declare at least one option",
            });
        }
        if let Some(default) = &self.default {
            let matches = match self.kind {
                PluginSettingKind::String => default.is_string(),
                PluginSettingKind::Number => default.is_number(),
                PluginSettingKind::Boolean => default.is_boolean(),
                PluginSettingKind::Select => default
                    .as_str()
                    .is_some_and(|value| self.options.iter().any(|option| option == value)),
            };
            if !matches {
                return Err(ManifestError::InvalidSetting {
                    key: key.to_string(),
                    reason: "the default value does not match the declared kind",
                });
            }
        }
        Ok(())
    }
}

fn validate_plugin_id(id: &str) -> Result<(), ManifestError> {
    if id.trim().is_empty() {
        return Err(ManifestError::MissingField { field: "id" });
    }
    let ok = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        return Err(ManifestError::InvalidId {
            id: id.to_string(),
            reason: "only letters, digits, '-', '_' and '.' are allowed",
        });
    }
    Ok(())
}

fn validate_node_type_id(type_id: &str, index: usize) -> Result<(), ManifestError> {
    let invalid = |reason: &'static str| ManifestError::InvalidNodeTypeId {
        index,
        type_id: type_id.to_string(),
        reason,
    };
    let Some((plugin, name)) = type_id.split_once(':') else {
        return Err(invalid("expected `<pluginId>:<name>`"));
    };
    if plugin.is_empty() || name.is_empty() {
        return Err(invalid("both the plugin id and the name must be non-empty"));
    }
    let ok = |part: &str| {
        part.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    };
    if !ok(plugin) || !ok(name) {
        return Err(invalid(
            "id parts may only contain letters, digits, '-' and '_'",
        ));
    }
    Ok(())
}

fn validate_icon(type_id: &str, icon: &str) -> Result<(), ManifestError> {
    let emoji = !icon.is_ascii();
    if icon.trim().is_empty() || !(KNOWN_ICON_KEYS.contains(&icon) || emoji) {
        return Err(ManifestError::UnknownIconKey {
            type_id: type_id.to_string(),
            icon: icon.to_string(),
        });
    }
    Ok(())
}

fn validate_size(type_id: &str, size: &PluginSize) -> Result<(), ManifestError> {
    let within = |value: f64| value.is_finite() && (MIN_NODE_SIDE..=MAX_NODE_SIDE).contains(&value);
    if !within(size.width) || !within(size.height) {
        return Err(ManifestError::SizeOutOfBounds {
            type_id: type_id.to_string(),
            width: size.width,
            height: size.height,
            min: MIN_NODE_SIDE,
            max: MAX_NODE_SIDE,
        });
    }
    Ok(())
}

fn validate_color(type_id: &str, color: &str) -> Result<(), ManifestError> {
    let digits = color.strip_prefix('#');
    let valid = digits.is_some_and(|hex| {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    });
    if !valid {
        return Err(ManifestError::InvalidMinimapColor {
            type_id: type_id.to_string(),
            color: color.to_string(),
        });
    }
    Ok(())
}

fn validate_semver(version: &str) -> Result<(), ManifestError> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    let valid = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    if !valid {
        return Err(ManifestError::InvalidVersion {
            version: version.to_string(),
        });
    }
    Ok(())
}
