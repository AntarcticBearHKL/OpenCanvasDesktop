//! Filter/adjustment chains: resolve ids from `open-canvas-ps`, apply the same
//! `--param key=value` overrides to every operator that declares the key, and
//! run the chain over a project's raster layers.

use std::collections::BTreeSet;

use open_canvas_ps::adjustments::{
    apply_ps_adjustment, ps_adjustment_defaults, ps_adjustment_from_id,
};
use open_canvas_ps::doc::{CanvasPsParamValue, PsParams};
use open_canvas_ps::filters::{apply_ps_filter, ps_filter_from_id, ps_filter_params, PsFilterType};
use open_canvas_ps::{CanvasPsAdjustmentType, Surface};

use crate::project::Loaded;

/// One resolved operator in a chain.
#[derive(Debug)]
pub struct Op {
    kind: Kind,
    params: PsParams,
}

#[derive(Debug)]
enum Kind {
    Filter(PsFilterType),
    Adjustment(CanvasPsAdjustmentType),
}

impl Op {
    /// Applies this operator in place; the maths lives in `open-canvas-ps`.
    pub fn apply(&self, surface: &mut Surface) {
        match self.kind {
            Kind::Filter(kind) => apply_ps_filter(surface, kind, &self.params),
            Kind::Adjustment(kind) => apply_ps_adjustment(surface, kind, &self.params),
        }
    }
}

/// Resolves `--filter` ids in order, with `--param` overrides on top of each
/// operator's defaults. An unknown id or a parameter no operator knows is an
/// error, so a typo never silently does nothing.
pub fn resolve(filters: &[String], params: &[String]) -> Result<Vec<Op>, String> {
    let overrides = parse_overrides(params)?;
    let mut used = BTreeSet::new();
    let mut chain = Vec::with_capacity(filters.len());
    for id in filters {
        let (kind, mut bag) = match (ps_filter_from_id(id), ps_adjustment_from_id(id)) {
            (Some(kind), _) => (Kind::Filter(kind), ps_filter_params(kind)),
            (None, Some(kind)) => (Kind::Adjustment(kind), ps_adjustment_defaults(kind)),
            (None, None) => {
                return Err(format!(
                    "unknown filter or adjustment id \"{id}\" (see --help for the ids)"
                ));
            }
        };
        for (key, raw) in &overrides {
            if bag.contains_key(key) {
                bag.insert(key.clone(), parse_value(raw));
                used.insert(key.clone());
            }
        }
        chain.push(Op { kind, params: bag });
    }
    for (key, _) in &overrides {
        if !used.contains(key) {
            return Err(format!(
                "unknown parameter \"{key}\" for filter chain [{}]",
                filters.join(", ")
            ));
        }
    }
    Ok(chain)
}

/// Applies `chain` to every raster layer that has a stored bitmap, returning
/// how many bitmaps were changed. Masks and adjustment layers are left alone.
pub fn apply_all(loaded: &mut Loaded, chain: &[Op]) -> Result<usize, String> {
    if chain.is_empty() {
        return Ok(0);
    }
    let mut keys: Vec<String> = loaded
        .project
        .board_layers
        .iter()
        .filter_map(|layer| layer.storage_key.clone())
        .collect();
    keys.sort();
    keys.dedup();
    let mut applied = 0;
    for key in keys {
        let Some(surface) = loaded.bitmaps.get_mut(&key) else {
            continue;
        };
        for op in chain {
            op.apply(surface);
        }
        applied += 1;
    }
    if applied == 0 {
        return Err("no raster layer to filter".to_string());
    }
    Ok(applied)
}

fn parse_overrides(params: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut overrides = Vec::with_capacity(params.len());
    for raw in params {
        let Some((key, value)) = raw.split_once('=') else {
            return Err(format!("--param expects key=value, got \"{raw}\""));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("--param has an empty key: \"{raw}\""));
        }
        overrides.push((key.to_string(), value.to_string()));
    }
    Ok(overrides)
}

/// `8` -> number, `0,0,255,255` -> number list, `#000000@0,#ffffff@1` -> text
/// list, anything else -> text.
fn parse_value(raw: &str) -> CanvasPsParamValue {
    if let Ok(number) = raw.parse::<f64>() {
        return CanvasPsParamValue::Number(number);
    }
    if raw.contains(',') {
        let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
        let numbers: Option<Vec<f64>> = parts.iter().map(|part| part.parse::<f64>().ok()).collect();
        if let Some(numbers) = numbers {
            return CanvasPsParamValue::Numbers(numbers);
        }
        return CanvasPsParamValue::Texts(parts.iter().map(|part| (*part).to_string()).collect());
    }
    CanvasPsParamValue::Text(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_typed_from_their_text() {
        assert!(matches!(
            parse_value("8.5"),
            CanvasPsParamValue::Number(value) if (value - 8.5).abs() < f64::EPSILON
        ));
        assert!(matches!(
            parse_value("0,0,255,255"),
            CanvasPsParamValue::Numbers(values) if values == vec![0.0, 0.0, 255.0, 255.0]
        ));
        assert!(matches!(
            parse_value("#000000@0,#ffffff@1"),
            CanvasPsParamValue::Texts(values) if values.len() == 2
        ));
        assert!(matches!(
            parse_value("zoom"),
            CanvasPsParamValue::Text(value) if value == "zoom"
        ));
    }

    #[test]
    fn unknown_ids_and_parameters_are_rejected() {
        let error = resolve(&["no-such-filter".to_string()], &[])
            .err()
            .unwrap_or_default();
        assert!(error.contains("unknown filter"), "{error}");

        let error = resolve(
            &["gaussian-blur".to_string()],
            &["no-such-param=1".to_string()],
        )
        .err()
        .unwrap_or_default();
        assert!(error.contains("unknown parameter"), "{error}");

        let error = resolve(&["gaussian-blur".to_string()], &["radius".to_string()])
            .err()
            .unwrap_or_default();
        assert!(error.contains("key=value"), "{error}");
    }

    #[test]
    fn chains_take_parameter_overrides_and_apply_them() {
        let chain = resolve(
            &["gaussian-blur".to_string(), "invert".to_string()],
            &["radius=1".to_string()],
        )
        .unwrap_or_default();
        assert_eq!(chain.len(), 2);

        let mut surface = Surface::new(2, 2);
        surface.fill([10, 20, 30, 255]);
        let mut loaded = crate::project::raster_loaded(std::path::Path::new("test.png"), surface);
        let applied = apply_all(&mut loaded, &chain).unwrap_or_default();
        assert_eq!(applied, 1);
        // The blur cannot change a uniform 2x2 surface's centre value; invert
        // must.
        let pixel = loaded
            .bitmaps
            .values()
            .next()
            .map(|bitmap| bitmap.get(0, 0));
        assert_eq!(pixel, Some([245, 235, 225, 255]));
    }
}
