//! Bridge to the vendored filter engine (`open-canvas-algo`).
//!
//! The engine works on [`open_canvas_raster::Surface`]s in any pixel format; the
//! Image Studio works on the RGBA8 [`Surface`] in [`crate::surface`]. This module
//! is the seam between the two:
//!
//! * [`available`] is the catalogue: one stable id, a display label, a group and a
//!   parameter schema per filter, so the UI can build a generic dialog without
//!   knowing the engine's Rust types.
//! * [`apply_filter`] converts the bitmap to an engine surface, runs the engine's
//!   [`open_canvas_algo::apply_in`] over the whole bitmap (edge pixels repeat, so
//!   neighbourhood filters do not pull in transparency from outside the canvas),
//!   and copies the result back.
//!
//! # Alpha
//!
//! Both surfaces store **straight (non-premultiplied) alpha**: the engine's
//! `RGBA8` format keeps encoded, straight-alpha samples, and [`Surface`] is the
//! Rust match for Canvas2D `ImageData`, which is straight-alpha too. The bytes
//! therefore convert 1:1 with no unpremultiply/premultiply step, and no rounding.
//! The engine treats alpha as the last colour channel of the filter kernel, so a
//! blur spreads alpha into transparent neighbours (as Photoshop's filters do on a
//! layer); per-pixel filters leave alpha untouched. Filters that create
//! transparency (`Color to Alpha`) are the only ones that deliberately change it.
//!
//! # Seeds
//!
//! Filters that are noisy or pattern-based take a `seed` (the engine's `u32`
//! seed fields). [`apply_filter`] takes a `u64` and uses its low 32 bits for every
//! such filter, so the same bitmap + spec + seed always produces the same pixels.
//!
//! # Not bridged
//!
//! `displace` moves pixels by a second image supplied by the caller; this
//! document model has no second-image parameter yet, so the catalogue lists it
//! with `supported: false` and [`apply_filter`] returns
//! [`PsError::UnsupportedFilter`]. Every other engine filter runs on the bitmap
//! alone. `Lens Blur` is exposed with a uniform depth (no depth map), which is the
//! engine's `DepthSource::None` mode.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use open_canvas_algo::{FilterParams, GalleryEffect, GalleryFilter, GalleryParamKind};
use open_canvas_color::PixelFormat;
use open_canvas_geom::Rect;
use open_canvas_raster::Surface as EngineSurface;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::PsError;
use crate::surface::Surface;

/// A value for one filter parameter.
///
/// The tagged representation keeps colors and numeric lists unambiguous on the
/// wire while staying readable in the project JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum ParamValue {
    /// A continuous number (radius, angle, percent, fraction).
    Number(f64),
    /// A whole number (blades, tile count, pixel offset).
    Integer(i64),
    /// A checkbox.
    Bool(bool),
    /// A choice's label (one of [`ParamKind::Choice::options`]) or free text.
    Text(String),
    /// Straight sRGB RGBA, components in `0..=1`.
    Color([f32; 4]),
    /// A fixed-length numeric list (screen angles).
    Numbers(Vec<f64>),
}

/// The UI control for a parameter and its constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ParamKind {
    /// A slider/spin box.
    Number {
        min: f64,
        max: f64,
        default: f64,
        step: f64,
    },
    /// An integer spin box.
    Integer { min: i64, max: i64, default: i64 },
    /// A checkbox.
    Toggle { default: bool },
    /// A drop-down; `default` is one of `options`.
    Choice {
        options: Vec<String>,
        default: String,
    },
    /// A colour picker; straight sRGB RGBA.
    Color { default: [f32; 4] },
    /// A text field (curve points, custom kernel).
    Text { default: String },
    /// A fixed-length numeric list; used for Color Halftone's screen angles.
    Numbers { default: Vec<f64>, len: usize },
}

impl ParamKind {
    /// A short name for the control, for tests and diagnostics.
    pub const fn control(&self) -> &'static str {
        match self {
            Self::Number { .. } => "number",
            Self::Integer { .. } => "integer",
            Self::Toggle { .. } => "toggle",
            Self::Choice { .. } => "choice",
            Self::Color { .. } => "color",
            Self::Text { .. } => "text",
            Self::Numbers { .. } => "numbers",
        }
    }
}

/// One parameter in a filter's schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Key in [`AlgoFilter::params`], matching the engine's field name.
    pub name: String,
    /// Human-readable label for the dialog.
    pub label: String,
    /// Control and constraints.
    pub kind: ParamKind,
}

impl ParamSpec {
    /// The value the UI starts with and [`apply_filter`] uses when a key is absent.
    pub fn default_value(&self) -> ParamValue {
        match &self.kind {
            ParamKind::Number { default, .. } => ParamValue::Number(*default),
            ParamKind::Integer { default, .. } => ParamValue::Integer(*default),
            ParamKind::Toggle { default } => ParamValue::Bool(*default),
            ParamKind::Choice { default, .. } | ParamKind::Text { default } => {
                ParamValue::Text(default.clone())
            }
            ParamKind::Color { default } => ParamValue::Color(*default),
            ParamKind::Numbers { default, .. } => ParamValue::Numbers(default.clone()),
        }
    }
}

/// One filter the engine can run on a bitmap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterEntry {
    /// Stable id used by menus, the CLI and project files (`twirl`, `oil-paint`,
    /// `gallery-colored-pencil`). Never reuse an id for another filter.
    pub id: String,
    /// Display label (`Twirl`, `Oil Paint`).
    pub label: String,
    /// Menu grouping (`blur`, `distort`, `stylize`, `gallery`, ...).
    pub group: String,
    /// The engine's serde variant tag, or `filterGallery` for gallery entries.
    pub tag: String,
    /// The gallery filter key for `gallery-*` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// Whether [`apply_filter`] can run it. `false` means it needs a second image.
    pub supported: bool,
    /// Why an unsupported filter is unsupported (shown by the UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
    /// The parameter schema, in dialog order.
    pub params: Vec<ParamSpec>,
}

/// A filter plus the values chosen for its parameters.
///
/// `params` maps [`ParamSpec::name`] to a [`ParamValue`]; omitted keys take the
/// catalogue default. Unknown keys are rejected rather than silently dropped, so
/// a stale UI cannot pretend a setting applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AlgoFilter {
    /// One of [`FilterEntry::id`].
    pub id: String,
    /// Supplied parameter values; defaults fill the rest.
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
}

impl AlgoFilter {
    /// A filter with every parameter at its default.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            params: BTreeMap::new(),
        }
    }

    /// A filter with one parameter overridden.
    #[must_use]
    pub fn with(mut self, name: &str, value: ParamValue) -> Self {
        self.params.insert(name.to_string(), value);
        self
    }
}

/// Every filter the vendored engine exposes that this document model does not
/// already implement in [`crate::filters`], in menu order.
///
/// The catalogue is static: the ids and schemas are part of the app's public
/// surface, so a UI can persist a filter by id and rebuild its dialog later.
pub fn available() -> &'static [FilterEntry] {
    ENTRIES.as_slice()
}

static ENTRIES: LazyLock<Vec<FilterEntry>> = LazyLock::new(build_catalogue);

/// Looks a catalogue entry up by id.
pub fn entry(id: &str) -> Option<&'static FilterEntry> {
    available().iter().find(|e| e.id == id)
}

/// The Filter Gallery folder for a `gallery-*` entry (`Artistic`, `Brush
/// Strokes`, `Distort`, `Sketch`, `Stylize`, `Texture`).
///
/// The catalogue keeps the engine's camel-case key in [`FilterEntry::gallery`];
/// the folder lives on the engine's [`GalleryFilter`], so the menu reads it from
/// here rather than duplicating the grouping.
pub fn gallery_category(entry: &FilterEntry) -> Option<&'static str> {
    let key = entry.gallery.as_deref()?;
    GalleryFilter::from_key(key).map(GalleryFilter::category)
}

/// The Filter Gallery's folders, in Photoshop's menu order.
pub fn gallery_categories() -> &'static [&'static str] {
    &open_canvas_algo::GALLERY_CATEGORIES
}

/// Runs `filter` on `bitmap` in place.
///
/// The whole bitmap is the filter's area and reference bounds; there is no
/// selection mask yet (the engine supports one, so a future revision can pass it
/// through without changing this signature's callers).
///
/// # Errors
///
/// [`PsError::UnknownFilter`] for an id outside [`available`],
/// [`PsError::UnsupportedFilter`] for a filter that needs a second image, and
/// [`PsError::InvalidFilter`] / [`PsError::InvalidFilterParameter`] when a value
/// has the wrong type or cannot be parsed. A bitmap larger than `i32::MAX` in
/// either direction is rejected with [`PsError::InvalidFilter`] (the engine's
/// geometry is `i32`).
pub fn apply_filter(bitmap: &mut Surface, filter: &AlgoFilter, seed: u64) -> Result<(), PsError> {
    let Some(found) = entry(&filter.id) else {
        return Err(PsError::UnknownFilter(filter.id.clone()));
    };
    if !found.supported {
        return Err(PsError::UnsupportedFilter {
            id: found.id.clone(),
            reason: found.unsupported_reason.clone().unwrap_or_else(|| {
                "needs an image this document model does not provide".to_string()
            }),
        });
    }
    let params = build_params(found, filter, (seed & u64::from(u32::MAX)) as u32)?;
    let (width, height) = (bitmap.width(), bitmap.height());
    if width == 0 || height == 0 {
        return Ok(());
    }
    let Ok(w) = i32::try_from(width) else {
        return Err(PsError::InvalidFilter {
            id: found.id.clone(),
            message: format!("bitmap width {width} exceeds the engine's i32 geometry"),
        });
    };
    let Ok(h) = i32::try_from(height) else {
        return Err(PsError::InvalidFilter {
            id: found.id.clone(),
            message: format!("bitmap height {height} exceeds the engine's i32 geometry"),
        });
    };
    let area = Rect::new(0, 0, w, h);
    let source = EngineSurface::from_interleaved(PixelFormat::RGBA8, area, bitmap.data());
    // `apply_in` repeats the bitmap's edge pixels for neighbourhood filters instead
    // of reading transparency around it (the engine's document semantics, matching
    // Photoshop at the canvas edge), and clips the output to the bitmap.
    let filtered = open_canvas_algo::apply_in(&source, &params, area, area, None, area);
    let bytes = filtered.to_interleaved(area);
    if bytes.len() != bitmap.len() {
        return Err(PsError::Dimension {
            expected: bitmap.len(),
            found: bytes.len(),
        });
    }
    bitmap.data_mut().copy_from_slice(&bytes);
    Ok(())
}

// ---------------------------------------------------------------------------
// Parameter resolution
// ---------------------------------------------------------------------------

/// Merges supplied values with the schema defaults, rejects unknown names and
/// wrong value types, and clamps numbers into their declared range.
fn resolved_values(
    found: &FilterEntry,
    filter: &AlgoFilter,
) -> Result<BTreeMap<String, ParamValue>, PsError> {
    for name in filter.params.keys() {
        if !found.params.iter().any(|p| &p.name == name) {
            return Err(PsError::InvalidFilterParameter {
                parameter: name.clone(),
                message: format!("is not a parameter of '{}'", found.id),
            });
        }
    }
    let mut out = BTreeMap::new();
    for spec in &found.params {
        let value = match filter.params.get(&spec.name) {
            Some(value) => resolve_value(spec, value.clone())?,
            None => spec.default_value(),
        };
        out.insert(spec.name.clone(), value);
    }
    Ok(out)
}

fn invalid_parameter(parameter: &str, message: impl Into<String>) -> PsError {
    PsError::InvalidFilterParameter {
        parameter: parameter.to_string(),
        message: message.into(),
    }
}

fn resolve_value(spec: &ParamSpec, value: ParamValue) -> Result<ParamValue, PsError> {
    match (&spec.kind, value) {
        (ParamKind::Number { min, max, .. }, ParamValue::Number(v)) if v.is_finite() => {
            Ok(ParamValue::Number(v.clamp(*min, *max)))
        }
        (ParamKind::Number { min, max, .. }, ParamValue::Integer(v)) => {
            Ok(ParamValue::Number((v as f64).clamp(*min, *max)))
        }
        (ParamKind::Integer { min, max, .. }, ParamValue::Integer(v)) => {
            Ok(ParamValue::Integer(v.clamp(*min, *max)))
        }
        (ParamKind::Integer { min, max, .. }, ParamValue::Number(v)) if v.is_finite() => {
            // Rounded first, then clamped, so 2.6 inside 0..=10 becomes 3 rather than 2.
            let rounded = v.round();
            let bounded = rounded.clamp(*min as f64, *max as f64);
            Ok(ParamValue::Integer(bounded as i64))
        }
        (ParamKind::Toggle { .. }, ParamValue::Bool(v)) => Ok(ParamValue::Bool(v)),
        (ParamKind::Choice { options, .. }, ParamValue::Text(v)) => {
            if options.iter().any(|o| o == &v) {
                Ok(ParamValue::Text(v))
            } else {
                Err(invalid_parameter(
                    &spec.name,
                    format!("'{v}' is not one of {options:?}"),
                ))
            }
        }
        (ParamKind::Color { .. }, ParamValue::Color(v)) => Ok(ParamValue::Color(v)),
        (ParamKind::Text { .. }, ParamValue::Text(v)) => Ok(ParamValue::Text(v)),
        (ParamKind::Numbers { len, .. }, ParamValue::Numbers(v)) => {
            if v.len() == *len && v.iter().all(|n| n.is_finite()) {
                Ok(ParamValue::Numbers(v))
            } else {
                Err(invalid_parameter(
                    &spec.name,
                    format!("expected {len} finite numbers"),
                ))
            }
        }
        (kind, _) => Err(invalid_parameter(
            &spec.name,
            format!("expected a {} value", kind.control()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Engine dispatch
// ---------------------------------------------------------------------------

fn build_params(
    found: &FilterEntry,
    filter: &AlgoFilter,
    seed: u32,
) -> Result<FilterParams, PsError> {
    if let Some(key) = &found.gallery {
        return build_gallery(found, filter, key);
    }
    let values = resolved_values(found, filter)?;
    let json = direct_json(found, &values, seed)?;
    serde_json::from_value(json).map_err(|error| PsError::InvalidFilter {
        id: found.id.clone(),
        message: error.to_string(),
    })
}

fn build_gallery(
    found: &FilterEntry,
    filter: &AlgoFilter,
    key: &str,
) -> Result<FilterParams, PsError> {
    let Some(gallery) = GalleryFilter::from_key(key) else {
        return Err(PsError::InvalidFilter {
            id: found.id.clone(),
            message: format!("the engine no longer knows gallery filter '{key}'"),
        });
    };
    let values = resolved_values(found, filter)?;
    let mut effect = GalleryEffect::new(gallery);
    for spec in &found.params {
        let Some(value) = values.get(&spec.name) else {
            return Err(invalid_parameter(&spec.name, "was not resolved"));
        };
        match value {
            ParamValue::Number(v) => effect.set(&spec.name, *v as f32),
            ParamValue::Integer(v) => effect.set(&spec.name, *v as f32),
            ParamValue::Bool(v) => effect.set(&spec.name, u8::from(*v) as f32),
            ParamValue::Text(label) => {
                let ParamKind::Choice { options, .. } = &spec.kind else {
                    return Err(invalid_parameter(&spec.name, "expected a numeric value"));
                };
                let Some(index) = options.iter().position(|o| o == label) else {
                    return Err(invalid_parameter(
                        &spec.name,
                        format!("'{label}' is not an option"),
                    ));
                };
                effect.set(&spec.name, index as f32);
            }
            ParamValue::Color(_) | ParamValue::Numbers(_) => {
                return Err(invalid_parameter(&spec.name, "is not a gallery parameter"));
            }
        }
    }
    Ok(FilterParams::FilterGallery {
        effects: vec![effect],
    })
}

/// Builds the engine's serde representation. Using serde means the engine's own
/// defaults and validation apply; the catalogue only supplies the fields the
/// dialog edits, and the variant tag selects the filter.
fn direct_json(
    found: &FilterEntry,
    values: &BTreeMap<String, ParamValue>,
    seed: u32,
) -> Result<Value, PsError> {
    let mut map = Map::new();
    map.insert("filter".to_string(), Value::String(found.tag.clone()));
    let mut skip: Vec<&str> = Vec::new();
    match found.tag.as_str() {
        "shear" => {
            let text = text_of(values, "points")?;
            map.insert("points".to_string(), parse_points("points", &text)?);
            skip.push("points");
        }
        "custom" => {
            let text = text_of(values, "kernel")?;
            map.insert("kernel".to_string(), parse_kernel(&text)?);
            skip.push("kernel");
        }
        "lightingEffects" => {
            map.insert(
                "lights".to_string(),
                Value::Array(vec![light_json(values)?]),
            );
            skip.extend([
                "lightKind",
                "x",
                "y",
                "z",
                "targetX",
                "targetY",
                "angle",
                "elevation",
                "lightColor",
                "intensity",
                "cone",
                "hotspot",
                "radius",
            ]);
        }
        "fieldBlur" => {
            let mut pin = Map::new();
            pin.insert("x".to_string(), number_value(values, "x")?);
            pin.insert("y".to_string(), number_value(values, "y")?);
            pin.insert("blur".to_string(), number_value(values, "blur")?);
            map.insert("pins".to_string(), Value::Array(vec![Value::Object(pin)]));
            skip.extend(["x", "y", "blur"]);
        }
        "spinBlur" => {
            let mut pin = Map::new();
            for name in ["x", "y", "radiusX", "radiusY", "angle", "blurAngle"] {
                pin.insert(name.to_string(), number_value(values, name)?);
            }
            map.insert("pins".to_string(), Value::Array(vec![Value::Object(pin)]));
            skip.extend(["x", "y", "radiusX", "radiusY", "angle", "blurAngle"]);
        }
        "pathBlur" => {
            let text = text_of(values, "points")?;
            let mut path = Map::new();
            path.insert("points".to_string(), parse_points("points", &text)?);
            path.insert("speed".to_string(), number_value(values, "speed")?);
            path.insert("taper".to_string(), number_value(values, "taper")?);
            map.insert("paths".to_string(), Value::Array(vec![Value::Object(path)]));
            skip.extend(["points", "speed", "taper"]);
        }
        "lensBlur" => {
            // The catalogue exposes a uniform depth; the engine's depth-map modes
            // need a second image this model does not carry.
            map.insert("depth".to_string(), Value::String("none".to_string()));
            map.insert("focalDistance".to_string(), Value::from(128));
            map.insert("invertDepth".to_string(), Value::Bool(false));
        }
        "clouds" if found.id == "difference-clouds" => {
            map.insert("difference".to_string(), Value::Bool(true));
        }
        _ => {}
    }
    for spec in &found.params {
        if skip.contains(&spec.name.as_str()) {
            continue;
        }
        let Some(value) = values.get(&spec.name) else {
            return Err(invalid_parameter(&spec.name, "was not resolved"));
        };
        map.insert(spec.name.clone(), param_json(value));
    }
    // Seeded variants read this; serde ignores it for the rest.
    map.insert("seed".to_string(), Value::from(seed));
    Ok(Value::Object(map))
}

fn light_json(values: &BTreeMap<String, ParamValue>) -> Result<Value, PsError> {
    let mut light = Map::new();
    light.insert("kind".to_string(), text_value(values, "lightKind")?);
    for name in [
        "x",
        "y",
        "z",
        "targetX",
        "targetY",
        "angle",
        "elevation",
        "intensity",
        "cone",
        "hotspot",
        "radius",
    ] {
        light.insert(name.to_string(), number_value(values, name)?);
    }
    let ParamValue::Color(color) = value_of(values, "lightColor")? else {
        return Err(invalid_parameter("lightColor", "expected a colour"));
    };
    light.insert(
        "color".to_string(),
        Value::Array(
            color[..3]
                .iter()
                .map(|c| Value::from(f64::from(*c)))
                .collect(),
        ),
    );
    Ok(Value::Object(light))
}

fn param_json(value: &ParamValue) -> Value {
    match value {
        ParamValue::Number(v) => Value::from(*v),
        ParamValue::Integer(v) => Value::from(*v),
        ParamValue::Bool(v) => Value::from(*v),
        ParamValue::Text(v) => Value::from(v.clone()),
        ParamValue::Color(c) => {
            Value::Array(c.iter().map(|v| Value::from(f64::from(*v))).collect())
        }
        ParamValue::Numbers(v) => Value::Array(v.iter().map(|v| Value::from(*v)).collect()),
    }
}

fn value_of<'a>(
    values: &'a BTreeMap<String, ParamValue>,
    name: &str,
) -> Result<&'a ParamValue, PsError> {
    values
        .get(name)
        .ok_or_else(|| invalid_parameter(name, "was not resolved"))
}

fn number_of(values: &BTreeMap<String, ParamValue>, name: &str) -> Result<f64, PsError> {
    match value_of(values, name)? {
        ParamValue::Number(v) => Ok(*v),
        _ => Err(invalid_parameter(name, "expected a number")),
    }
}

fn number_value(values: &BTreeMap<String, ParamValue>, name: &str) -> Result<Value, PsError> {
    number_of(values, name).map(Value::from)
}

fn text_of(values: &BTreeMap<String, ParamValue>, name: &str) -> Result<String, PsError> {
    match value_of(values, name)? {
        ParamValue::Text(v) => Ok(v.clone()),
        _ => Err(invalid_parameter(name, "expected text")),
    }
}

fn text_value(values: &BTreeMap<String, ParamValue>, name: &str) -> Result<Value, PsError> {
    text_of(values, name).map(Value::from)
}

/// Parses `x,y;x,y;...` (fractions of the bounds for blur paths, curve points for
/// Shear) into the engine's `Vec<[f32; 2]>`.
fn parse_points(name: &str, text: &str) -> Result<Value, PsError> {
    let mut points = Vec::new();
    for pair in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let Some((x, y)) = pair.split_once(',') else {
            return Err(invalid_parameter(
                name,
                format!("'{pair}' is not an 'x,y' pair"),
            ));
        };
        let parse = |part: &str| -> Result<f64, PsError> {
            part.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid_parameter(name, format!("'{part}' is not a finite number")))
        };
        let x = parse(x)?;
        let y = parse(y)?;
        points.push(Value::Array(vec![Value::from(x), Value::from(y)]));
    }
    if points.len() < 2 {
        return Err(invalid_parameter(name, "needs at least two points"));
    }
    Ok(Value::Array(points))
}

/// Parses a 5×5 kernel (row-major, centre last) of 25 comma/space separated numbers.
fn parse_kernel(text: &str) -> Result<Value, PsError> {
    let parts: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 25 {
        return Err(invalid_parameter(
            "kernel",
            format!("expected 25 numbers, found {}", parts.len()),
        ));
    }
    let mut out = Vec::with_capacity(25);
    for part in parts {
        let value = part
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                invalid_parameter("kernel", format!("'{part}' is not a finite number"))
            })?;
        out.push(Value::from(value));
    }
    Ok(Value::Array(out))
}

// ---------------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------------

fn spec(name: &str, label: &str, kind: ParamKind) -> ParamSpec {
    ParamSpec {
        name: name.to_string(),
        label: label.to_string(),
        kind,
    }
}

fn number(name: &str, label: &str, min: f64, max: f64, default: f64, step: f64) -> ParamSpec {
    spec(
        name,
        label,
        ParamKind::Number {
            min,
            max,
            default,
            step,
        },
    )
}

fn integer(name: &str, label: &str, min: i64, max: i64, default: i64) -> ParamSpec {
    spec(name, label, ParamKind::Integer { min, max, default })
}

fn toggle(name: &str, label: &str, default: bool) -> ParamSpec {
    spec(name, label, ParamKind::Toggle { default })
}

fn choice(name: &str, label: &str, options: &[&str], default: &str) -> ParamSpec {
    spec(
        name,
        label,
        ParamKind::Choice {
            options: options.iter().map(|o| (*o).to_string()).collect(),
            default: default.to_string(),
        },
    )
}

fn color(name: &str, label: &str, default: [f32; 4]) -> ParamSpec {
    spec(name, label, ParamKind::Color { default })
}

fn text(name: &str, label: &str, default: &str) -> ParamSpec {
    spec(
        name,
        label,
        ParamKind::Text {
            default: default.to_string(),
        },
    )
}

fn numbers(name: &str, label: &str, default: [f64; 4]) -> ParamSpec {
    spec(
        name,
        label,
        ParamKind::Numbers {
            default: default.to_vec(),
            len: default.len(),
        },
    )
}

fn filter(id: &str, label: &str, group: &str, tag: &str, params: Vec<ParamSpec>) -> FilterEntry {
    FilterEntry {
        id: id.to_string(),
        label: label.to_string(),
        group: group.to_string(),
        tag: tag.to_string(),
        gallery: None,
        supported: true,
        unsupported_reason: None,
        params,
    }
}

fn build_catalogue() -> Vec<FilterEntry> {
    let mut out = Vec::new();

    out.extend([filter(
        "surface-blur",
        "Surface Blur",
        "blur",
        "surfaceBlur",
        vec![
            number("radius", "Radius", 0.1, 100.0, 5.0, 0.1),
            number("threshold", "Threshold Levels", 0.0, 255.0, 15.0, 1.0),
        ],
    )]);
    out.push(filter(
        "lens-blur",
        "Lens Blur",
        "blur",
        "lensBlur",
        vec![
            number("radius", "Radius", 0.0, 100.0, 15.0, 0.5),
            integer("blades", "Blades", 3, 8, 6),
            number("curvature", "Blade Curvature", 0.0, 100.0, 50.0, 1.0),
            number("rotation", "Rotation", -180.0, 180.0, 0.0, 1.0),
            number("brightness", "Specular Highlights", 0.0, 100.0, 0.0, 1.0),
            number("threshold", "Highlight Threshold", 0.0, 255.0, 255.0, 1.0),
            number("noise", "Noise", 0.0, 100.0, 0.0, 1.0),
            choice(
                "distribution",
                "Noise Distribution",
                &["uniform", "gaussian"],
                "uniform",
            ),
            toggle("monochromatic", "Monochromatic", false),
        ],
    ));
    out.push(filter(
        "shape-blur",
        "Shape Blur",
        "blur",
        "shapeBlur",
        vec![
            number("radius", "Radius", 5.0, 1000.0, 15.0, 1.0),
            choice(
                "shape",
                "Shape",
                &[
                    "circle", "ring", "square", "diamond", "triangle", "hexagon", "star", "heart",
                    "cross",
                ],
                "circle",
            ),
        ],
    ));
    out.push(filter(
        "smart-blur",
        "Smart Blur",
        "blur",
        "smartBlur",
        vec![
            number("radius", "Radius", 0.1, 100.0, 3.0, 0.1),
            number("threshold", "Threshold", 0.1, 100.0, 10.0, 0.1),
            choice("quality", "Quality", &["low", "medium", "high"], "medium"),
            choice(
                "mode",
                "Mode",
                &["normal", "edgeOnly", "overlayEdge"],
                "normal",
            ),
        ],
    ));
    out.push(filter(
        "field-blur",
        "Field Blur",
        "blur",
        "fieldBlur",
        vec![
            number("x", "Center X", 0.0, 1.0, 0.5, 0.01),
            number("y", "Center Y", 0.0, 1.0, 0.5, 0.01),
            number("blur", "Blur", 0.0, 100.0, 15.0, 0.5),
        ],
    ));
    out.push(filter(
        "spin-blur",
        "Spin Blur",
        "blur",
        "spinBlur",
        vec![
            number("x", "Center X", 0.0, 1.0, 0.5, 0.01),
            number("y", "Center Y", 0.0, 1.0, 0.5, 0.01),
            number("radiusX", "Radius X", 0.0, 1.0, 0.3, 0.01),
            number("radiusY", "Radius Y", 0.0, 1.0, 0.3, 0.01),
            number("angle", "Ellipse Angle", -180.0, 180.0, 0.0, 1.0),
            number("blurAngle", "Blur Arc", 0.0, 360.0, 15.0, 1.0),
        ],
    ));
    out.push(filter(
        "path-blur",
        "Path Blur",
        "blur",
        "pathBlur",
        vec![
            text("points", "Path (x,y;x,y)", "0.2,0.5;0.8,0.5"),
            number("speed", "Speed", 0.0, 100.0, 50.0, 1.0),
            number("taper", "Taper", 0.0, 100.0, 0.0, 1.0),
        ],
    ));

    out.push(filter(
        "smart-sharpen",
        "Smart Sharpen",
        "sharpen",
        "smartSharpen",
        vec![
            number("amount", "Amount %", 1.0, 500.0, 100.0, 1.0),
            number("radius", "Radius", 0.1, 64.0, 1.0, 0.1),
            number("reduceNoise", "Reduce Noise %", 0.0, 100.0, 0.0, 1.0),
        ],
    ));
    out.push(filter(
        "high-pass",
        "High Pass",
        "sharpen",
        "highPass",
        vec![number("radius", "Radius", 0.1, 250.0, 10.0, 0.1)],
    ));

    out.push(filter(
        "median",
        "Median",
        "noise",
        "median",
        vec![number("radius", "Radius", 0.1, 100.0, 2.0, 0.1)],
    ));
    out.push(filter(
        "minimum",
        "Minimum",
        "noise",
        "minimum",
        vec![
            number("radius", "Radius", 0.1, 100.0, 2.0, 0.1),
            choice(
                "preserve",
                "Preserve",
                &["squareness", "roundness"],
                "squareness",
            ),
        ],
    ));
    out.push(filter(
        "maximum",
        "Maximum",
        "noise",
        "maximum",
        vec![
            number("radius", "Radius", 0.1, 100.0, 2.0, 0.1),
            choice(
                "preserve",
                "Preserve",
                &["squareness", "roundness"],
                "squareness",
            ),
        ],
    ));
    out.push(filter(
        "reduce-noise",
        "Reduce Noise",
        "noise",
        "reduceNoise",
        vec![
            number("strength", "Strength", 0.0, 10.0, 3.0, 0.1),
            number(
                "preserveDetails",
                "Preserve Details %",
                0.0,
                100.0,
                50.0,
                1.0,
            ),
            number(
                "reduceColorNoise",
                "Reduce Color Noise %",
                0.0,
                100.0,
                25.0,
                1.0,
            ),
            number("sharpenDetails", "Sharpen Details %", 0.0, 100.0, 0.0, 1.0),
            toggle("removeJpegArtifact", "Remove JPEG Artifact", false),
        ],
    ));

    out.push(filter(
        "mosaic",
        "Mosaic",
        "pixelate",
        "mosaic",
        vec![number("cellSize", "Cell Size", 2.0, 200.0, 10.0, 1.0)],
    ));
    out.push(filter(
        "color-halftone",
        "Color Halftone",
        "pixelate",
        "colorHalftone",
        vec![
            number("maxRadius", "Max Radius", 4.0, 127.0, 8.0, 1.0),
            numbers("angles", "Screen Angles", [108.0, 162.0, 90.0, 45.0]),
        ],
    ));
    out.push(filter("facet", "Facet", "pixelate", "facet", Vec::new()));
    out.push(filter(
        "fragment",
        "Fragment",
        "pixelate",
        "fragment",
        Vec::new(),
    ));
    out.push(filter(
        "mezzotint",
        "Mezzotint",
        "pixelate",
        "mezzotint",
        vec![choice(
            "kind",
            "Type",
            &[
                "fineDots",
                "mediumDots",
                "grainyDots",
                "coarseDots",
                "shortLines",
                "mediumLines",
                "longLines",
                "shortStrokes",
                "mediumStrokes",
                "longStrokes",
            ],
            "fineDots",
        )],
    ));
    out.push(filter(
        "pointillize",
        "Pointillize",
        "pixelate",
        "pointillize",
        vec![
            number("cellSize", "Cell Size", 3.0, 300.0, 5.0, 1.0),
            color("background", "Background", [1.0, 1.0, 1.0, 1.0]),
        ],
    ));

    out.push(filter("invert", "Invert", "stylize", "invert", Vec::new()));
    out.push(filter(
        "desaturate",
        "Desaturate",
        "stylize",
        "desaturate",
        Vec::new(),
    ));
    out.push(filter(
        "diffuse",
        "Diffuse",
        "stylize",
        "diffuse",
        vec![choice(
            "mode",
            "Mode",
            &["normal", "darkenOnly", "lightenOnly", "anisotropic"],
            "normal",
        )],
    ));
    out.push(filter(
        "extrude",
        "Extrude",
        "stylize",
        "extrude",
        vec![
            choice("kind", "Type", &["blocks", "pyramids"], "blocks"),
            number("size", "Size", 2.0, 255.0, 30.0, 1.0),
            number("depth", "Depth", 1.0, 255.0, 30.0, 1.0),
            toggle("levelBased", "Level-based", false),
            toggle("solidFront", "Solid Front Faces", true),
            toggle("maskIncomplete", "Mask Incomplete Blocks", false),
        ],
    ));
    out.push(filter(
        "oil-paint",
        "Oil Paint",
        "stylize",
        "oilPaint",
        vec![
            number("stylization", "Stylization", 0.1, 10.0, 2.5, 0.1),
            number("cleanliness", "Cleanliness", 0.0, 10.0, 5.0, 0.1),
            number("scale", "Scale", 0.1, 10.0, 1.0, 0.1),
            number("bristleDetail", "Bristle Detail", 0.0, 10.0, 5.0, 0.1),
            toggle("lighting", "Lighting", true),
            number("angle", "Angle", -180.0, 180.0, 60.0, 1.0),
            number("shine", "Shine", 0.0, 10.0, 3.0, 0.1),
        ],
    ));
    out.push(filter(
        "tiles",
        "Tiles",
        "stylize",
        "tiles",
        vec![
            integer("count", "Number of Tiles", 1, 99, 4),
            number("maxOffset", "Maximum Offset %", 1.0, 99.0, 10.0, 1.0),
            choice(
                "fill",
                "Fill Empty Area With",
                &["background", "foreground", "inverse", "unaltered"],
                "background",
            ),
            color("foreground", "Foreground", [0.0, 0.0, 0.0, 1.0]),
            color("background", "Background", [1.0, 1.0, 1.0, 1.0]),
        ],
    ));
    out.push(filter(
        "trace-contour",
        "Trace Contour",
        "stylize",
        "traceContour",
        vec![
            number("level", "Level", 0.0, 255.0, 128.0, 1.0),
            toggle("upper", "Edge: Upper", false),
        ],
    ));
    out.push(filter(
        "wind",
        "Wind",
        "stylize",
        "wind",
        vec![
            choice("method", "Method", &["wind", "blast", "stagger"], "wind"),
            toggle("fromRight", "Direction: From the Right", true),
        ],
    ));

    out.push(filter(
        "twirl",
        "Twirl",
        "distort",
        "twirl",
        vec![number("angle", "Angle", -999.0, 999.0, 90.0, 1.0)],
    ));
    out.push(filter(
        "wave",
        "Wave",
        "distort",
        "wave",
        vec![
            integer("generators", "Number of Generators", 1, 999, 5),
            number("wavelengthMin", "Wavelength Min", 1.0, 998.0, 10.0, 1.0),
            number("wavelengthMax", "Wavelength Max", 1.0, 998.0, 120.0, 1.0),
            number("amplitudeMin", "Amplitude Min", 0.0, 998.0, 5.0, 1.0),
            number("amplitudeMax", "Amplitude Max", 0.0, 998.0, 35.0, 1.0),
            choice("waveType", "Type", &["sine", "triangle", "square"], "sine"),
            choice(
                "undefined",
                "Undefined Areas",
                &["wrap", "repeat", "transparent"],
                "wrap",
            ),
        ],
    ));
    out.push(filter(
        "polar-coordinates",
        "Polar Coordinates",
        "distort",
        "polarCoordinates",
        vec![choice(
            "mode",
            "Mode",
            &["rectangularToPolar", "polarToRectangular"],
            "rectangularToPolar",
        )],
    ));
    out.push(filter(
        "shear",
        "Shear",
        "distort",
        "shear",
        vec![
            text("points", "Curve (x,y;x,y)", "0,0;0.5,0;1,0"),
            choice(
                "undefined",
                "Undefined Areas",
                &["wrap", "repeat", "transparent"],
                "wrap",
            ),
        ],
    ));
    out.push(filter(
        "zig-zag",
        "ZigZag",
        "distort",
        "zigZag",
        vec![
            number("amount", "Amount", -100.0, 100.0, 10.0, 1.0),
            number("ridges", "Ridges", 0.0, 20.0, 5.0, 0.1),
            choice(
                "style",
                "Style",
                &["aroundCenter", "outFromCenter", "pondRipples"],
                "pondRipples",
            ),
        ],
    ));
    out.push(FilterEntry {
        id: "displace".to_string(),
        label: "Displace".to_string(),
        group: "distort".to_string(),
        tag: "displace".to_string(),
        gallery: None,
        supported: false,
        unsupported_reason: Some(
            "needs a displacement map (a second image); the document model has no second-image parameter"
                .to_string(),
        ),
        params: vec![
            number("horizontal", "Horizontal Scale", -999.0, 999.0, 10.0, 1.0),
            number("vertical", "Vertical Scale", -999.0, 999.0, 10.0, 1.0),
            toggle("stretch", "Stretch to Fit", false),
            choice("undefined", "Undefined Areas", &["wrap", "repeat", "transparent"], "wrap"),
        ],
    });

    out.push(filter(
        "difference-clouds",
        "Difference Clouds",
        "render",
        "clouds",
        vec![
            color("foreground", "Foreground", [0.0, 0.0, 0.0, 1.0]),
            color("background", "Background", [1.0, 1.0, 1.0, 1.0]),
        ],
    ));
    out.push(filter(
        "fibers",
        "Fibers",
        "render",
        "fibers",
        vec![
            number("variance", "Variance", 1.0, 64.0, 16.0, 1.0),
            number("strength", "Strength", 1.0, 64.0, 4.0, 1.0),
            color("foreground", "Foreground", [0.0, 0.0, 0.0, 1.0]),
            color("background", "Background", [1.0, 1.0, 1.0, 1.0]),
        ],
    ));
    out.push(filter(
        "lighting-effects",
        "Lighting Effects",
        "render",
        "lightingEffects",
        vec![
            choice(
                "lightKind",
                "Light Type",
                &["spot", "point", "infinite"],
                "spot",
            ),
            number("x", "Light X", 0.0, 1.0, 0.25, 0.01),
            number("y", "Light Y", 0.0, 1.0, 0.2, 0.01),
            number("z", "Light Height", 0.0, 2.0, 0.6, 0.01),
            number("targetX", "Target X", 0.0, 1.0, 0.5, 0.01),
            number("targetY", "Target Y", 0.0, 1.0, 0.55, 0.01),
            number("angle", "Light Angle", -180.0, 180.0, 135.0, 1.0),
            number("elevation", "Elevation", 0.0, 90.0, 45.0, 1.0),
            color("lightColor", "Light Colour", [1.0, 1.0, 1.0, 1.0]),
            number("intensity", "Intensity", -100.0, 100.0, 75.0, 1.0),
            number("cone", "Cone", 0.0, 90.0, 45.0, 1.0),
            number("hotspot", "Hotspot", 0.0, 100.0, 50.0, 1.0),
            number("radius", "Point Light Reach", 0.0, 2.0, 0.8, 0.01),
            number("gloss", "Gloss", -100.0, 100.0, 0.0, 1.0),
            number("metallic", "Material", -100.0, 100.0, 0.0, 1.0),
            number("exposure", "Exposure", -100.0, 100.0, 0.0, 1.0),
            number("ambience", "Ambience", -100.0, 100.0, 0.0, 1.0),
            choice(
                "texture",
                "Texture Channel",
                &["none", "red", "green", "blue", "alpha", "luminance"],
                "none",
            ),
            number("height", "Bump Height", 0.0, 100.0, 0.0, 1.0),
            toggle("whiteIsHigh", "White is High", false),
        ],
    ));
    out.push(filter(
        "relight",
        "Relight",
        "render",
        "relight",
        vec![
            number("angle", "Light Angle", -180.0, 180.0, 45.0, 1.0),
            number("elevation", "Elevation", 0.0, 90.0, 45.0, 1.0),
            number("intensity", "Intensity", 0.0, 100.0, 50.0, 1.0),
            number("ambient", "Ambient", 0.0, 100.0, 25.0, 1.0),
            number("warmth", "Warmth", -100.0, 100.0, 0.0, 1.0),
            number("softness", "Softness", 1.0, 100.0, 30.0, 1.0),
        ],
    ));

    out.push(filter(
        "offset",
        "Offset",
        "other",
        "offset",
        vec![
            integer("horizontal", "Horizontal", -9999, 9999, 0),
            integer("vertical", "Vertical", -9999, 9999, 0),
            choice(
                "undefined",
                "Undefined Areas",
                &["wrap", "repeat", "transparent"],
                "wrap",
            ),
        ],
    ));
    out.push(filter(
        "custom",
        "Custom",
        "other",
        "custom",
        vec![
            text(
                "kernel",
                "5×5 Kernel",
                "0,0,0,0,0, 0,0,0,0,0, 0,0,1,0,0, 0,0,0,0,0, 0,0,0,0,0",
            ),
            number("scale", "Scale", 1.0, 100.0, 1.0, 0.01),
            number("offset", "Offset", -255.0, 255.0, 0.0, 1.0),
        ],
    ));
    out.push(filter(
        "hsb-hsl",
        "HSB/HSL",
        "other",
        "hsbHsl",
        vec![
            choice("input", "Input", &["rgb", "hsb", "hsl"], "rgb"),
            choice("output", "Output", &["rgb", "hsb", "hsl"], "rgb"),
        ],
    ));
    out.push(filter(
        "color-to-alpha",
        "Color to Alpha",
        "other",
        "colorToAlpha",
        vec![
            color("color", "Key Colour", [1.0, 1.0, 1.0, 1.0]),
            number(
                "transparencyThreshold",
                "Transparency Threshold",
                0.0,
                1.0,
                0.1,
                0.01,
            ),
            number("opacityThreshold", "Opacity Threshold", 0.0, 1.0, 0.9, 0.01),
        ],
    ));
    out.push(filter(
        "de-interlace",
        "De-Interlace",
        "other",
        "deInterlace",
        vec![
            toggle("eliminateEven", "Eliminate Even Fields", false),
            toggle("interpolate", "Interpolate", true),
        ],
    ));
    out.push(filter(
        "ntsc-colors",
        "NTSC Colors",
        "other",
        "ntscColors",
        Vec::new(),
    ));

    out.extend(gallery_entries());
    out
}

/// One entry per engine Filter Gallery effect, each with the engine's own
/// parameter ranges (`GalleryFilter::params`).
fn gallery_entries() -> Vec<FilterEntry> {
    GalleryFilter::ALL
        .iter()
        .map(|gallery| {
            let key = gallery.key();
            let params = gallery.params().into_iter().map(gallery_param).collect();
            FilterEntry {
                id: format!("gallery-{}", camel_to_kebab(key)),
                label: gallery.name().to_string(),
                group: "gallery".to_string(),
                tag: "filterGallery".to_string(),
                gallery: Some(key.to_string()),
                supported: true,
                unsupported_reason: None,
                params,
            }
        })
        .collect()
}

fn gallery_param(param: open_canvas_algo::GalleryParam) -> ParamSpec {
    let label = humanize(param.key);
    match param.kind {
        GalleryParamKind::Range { min, max, default } => number(
            param.key,
            &label,
            f64::from(min),
            f64::from(max),
            f64::from(default),
            1.0,
        ),
        GalleryParamKind::Choice(options) => {
            let default = options.first().copied().unwrap_or("");
            choice(param.key, &label, &options, default)
        }
        GalleryParamKind::Bool(default) => toggle(param.key, &label, default),
    }
}

/// `coloredPencil` → `colored-pencil`.
fn camel_to_kebab(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for (index, ch) in text.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// `pencilWidth` → `Pencil Width`.
fn humanize(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for (index, ch) in key.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push(' ');
            }
            out.push(ch);
        } else {
            if index == 0 {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: usize, height: usize) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let x = x as i64;
                let y = y as i64;
                let r = (x * 255 / width.max(1) as i64) as u8;
                let g = (y * 255 / height.max(1) as i64) as u8;
                let b = ((x + y) * 255 / (width + height).max(1) as i64) as u8;
                surface.set(x, y, [r, g, b, 255]);
            }
        }
        surface
    }

    fn checker(width: usize, height: usize) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let on = (x / 3 + y / 3) % 2 == 0;
                let value = if on {
                    [230, 40, 20, 255]
                } else {
                    [20, 60, 220, 255]
                };
                surface.set(x as i64, y as i64, value);
            }
        }
        surface
    }

    fn altered(before: &Surface, after: &Surface) -> bool {
        before.data() != after.data()
    }

    fn alpha_opaque(surface: &Surface) -> bool {
        surface
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| px[3] == 255)
    }

    #[test]
    fn bridged_filters_change_the_bitmap_and_keep_alpha() {
        // Twelve filters that are not in `crate::filters`, each on a small
        // synthetic image where its effect is unambiguous.
        let mut twirl = AlgoFilter::new("twirl");
        twirl = twirl.with("angle", ParamValue::Number(120.0));
        let mut mosaic = AlgoFilter::new("mosaic");
        mosaic = mosaic.with("cellSize", ParamValue::Number(8.0));
        let mut oil = AlgoFilter::new("oil-paint");
        oil = oil.with("stylization", ParamValue::Number(6.0));
        let mut median = AlgoFilter::new("median");
        median = median.with("radius", ParamValue::Number(2.0));
        let mut high_pass = AlgoFilter::new("high-pass");
        high_pass = high_pass.with("radius", ParamValue::Number(5.0));
        let mut wind = AlgoFilter::new("wind");
        wind = wind.with("method", ParamValue::Text("blast".to_string()));
        let mut mezzotint = AlgoFilter::new("mezzotint");
        mezzotint = mezzotint.with("kind", ParamValue::Text("coarseDots".to_string()));
        let mut halftone = AlgoFilter::new("color-halftone");
        halftone = halftone.with("maxRadius", ParamValue::Number(6.0));
        let mut diffuse = AlgoFilter::new("diffuse");
        diffuse = diffuse.with("mode", ParamValue::Text("anisotropic".to_string()));
        let mut lens = AlgoFilter::new("lens-blur");
        lens = lens.with("radius", ParamValue::Number(12.0));
        let mut relight = AlgoFilter::new("relight");
        relight = relight.with("intensity", ParamValue::Number(70.0));
        let mut pencil = AlgoFilter::new("gallery-colored-pencil");
        pencil = pencil.with("pencilWidth", ParamValue::Number(6.0));

        let cases: [(&str, &AlgoFilter); 12] = [
            ("twirl", &twirl),
            ("mosaic", &mosaic),
            ("oil-paint", &oil),
            ("median", &median),
            ("high-pass", &high_pass),
            ("wind", &wind),
            ("mezzotint", &mezzotint),
            ("color-halftone", &halftone),
            ("diffuse", &diffuse),
            ("lens-blur", &lens),
            ("relight", &relight),
            ("gallery-colored-pencil", &pencil),
        ];
        for (id, filter) in cases {
            let source = if id == "median" || id == "wind" {
                checker(24, 20)
            } else {
                gradient(24, 20)
            };
            let mut first = source.clone();
            apply_filter(&mut first, filter, 7).expect("filter runs");
            assert!(altered(&source, &first), "{id} should change the bitmap");
            assert!(
                alpha_opaque(&first),
                "{id} should keep an opaque bitmap opaque"
            );
            let mut second = source.clone();
            apply_filter(&mut second, filter, 7).expect("filter is deterministic");
            assert_eq!(first, second, "{id} must be deterministic for one seed");
        }
    }

    #[test]
    fn per_pixel_filters_leave_alpha_untouched() {
        let mut source = Surface::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let alpha = (x * 17) as u8;
                source.set(x, y, [(x * 16) as u8, (y * 16) as u8, 90, alpha]);
            }
        }
        let mut bitmap = source.clone();
        apply_filter(&mut bitmap, &AlgoFilter::new("invert"), 0).expect("runs");
        for y in 0..16 {
            for x in 0..16 {
                let before = source.get(x, y);
                let after = bitmap.get(x, y);
                assert_eq!(before[3], after[3], "alpha at ({x},{y})");
                assert_eq!(after[0], 255 - before[0]);
                assert_eq!(after[1], 255 - before[1]);
                assert_eq!(after[2], 255 - before[2]);
            }
        }
    }

    #[test]
    fn every_supported_entry_runs_with_its_defaults() {
        let source = gradient(12, 10);
        for found in available().iter().filter(|f| f.supported) {
            let mut bitmap = source.clone();
            let filter = AlgoFilter::new(&found.id);
            let result = apply_filter(&mut bitmap, &filter, 3);
            assert!(result.is_ok(), "{} failed: {:?}", found.id, result.err());
            assert_eq!(bitmap.width(), source.width());
            assert_eq!(bitmap.height(), source.height());
        }
    }

    #[test]
    fn seeded_filters_repeat_with_the_same_seed_and_differ_with_another() {
        let filter = AlgoFilter::new("diffuse");
        let source = gradient(20, 20);
        let mut a = source.clone();
        let mut b = source.clone();
        let mut c = source.clone();
        apply_filter(&mut a, &filter, 1).expect("runs");
        apply_filter(&mut b, &filter, 1).expect("runs");
        apply_filter(&mut c, &filter, 2).expect("runs");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn neutral_parameters_are_a_no_op() {
        // Twirl at 0 degrees maps every sample to itself.
        let source = gradient(32, 24);
        let mut bitmap = source.clone();
        let filter = AlgoFilter::new("twirl").with("angle", ParamValue::Number(0.0));
        apply_filter(&mut bitmap, &filter, 0).expect("runs");
        assert_eq!(bitmap, source, "twirl at 0 degrees must not move pixels");

        // Offset by (0, 0) is the identity whatever its wrap mode.
        let filter = AlgoFilter::new("offset");
        let mut bitmap = source.clone();
        apply_filter(&mut bitmap, &filter, 0).expect("runs");
        assert_eq!(bitmap, source, "offset by zero must not move pixels");

        // A straight Shear curve with a 1:1 scale is the identity.
        let filter = AlgoFilter::new("shear");
        let mut bitmap = source.clone();
        apply_filter(&mut bitmap, &filter, 0).expect("runs");
        assert_eq!(
            bitmap, source,
            "a straight shear curve must not move pixels"
        );
    }

    #[test]
    fn every_entry_round_trips_through_serde_with_a_valid_schema() {
        let mut seen = std::collections::BTreeSet::new();
        for found in available() {
            let json = serde_json::to_string(found).expect("entry serialises");
            let back: FilterEntry = serde_json::from_str(&json).expect("entry deserialises");
            assert_eq!(&back, found, "{} must round-trip through serde", found.id);
            assert!(
                seen.insert(found.id.clone()),
                "duplicate filter id {}",
                found.id
            );
            assert!(!found.id.is_empty() && !found.label.is_empty());
            for spec in &found.params {
                match &spec.kind {
                    ParamKind::Number {
                        min,
                        max,
                        default,
                        step,
                    } => {
                        assert!(min < max, "{}:{} min < max", found.id, spec.name);
                        assert!(
                            *min <= *default && *default <= *max,
                            "{}:{} default {default} outside {min}..={max}",
                            found.id,
                            spec.name
                        );
                        assert!(*step > 0.0, "{}:{} step > 0", found.id, spec.name);
                    }
                    ParamKind::Integer { min, max, default } => {
                        assert!(min < max, "{}:{} min < max", found.id, spec.name);
                        assert!(
                            *min <= *default && *default <= *max,
                            "{}:{} default {default} outside {min}..={max}",
                            found.id,
                            spec.name
                        );
                    }
                    ParamKind::Choice { options, default } => {
                        assert!(
                            !options.is_empty(),
                            "{}:{} has options",
                            found.id,
                            spec.name
                        );
                        assert!(
                            options.contains(default),
                            "{}:{} default is an option",
                            found.id,
                            spec.name
                        );
                    }
                    ParamKind::Color { default } => {
                        assert!(
                            default.iter().all(|c| (0.0..=1.0).contains(c)),
                            "{}:{} colour outside 0..=1",
                            found.id,
                            spec.name
                        );
                    }
                    ParamKind::Numbers { default, len } => {
                        assert_eq!(
                            default.len(),
                            *len,
                            "{}:{} list length",
                            found.id,
                            spec.name
                        );
                    }
                    ParamKind::Toggle { .. } | ParamKind::Text { .. } => {}
                }
            }
        }
    }

    #[test]
    fn the_catalogue_does_not_duplicate_the_ports_existing_filters() {
        for kind in [
            crate::filters::PsFilterType::GaussianBlur,
            crate::filters::PsFilterType::BoxBlur,
            crate::filters::PsFilterType::MotionBlur,
            crate::filters::PsFilterType::RadialBlur,
            crate::filters::PsFilterType::TiltShift,
            crate::filters::PsFilterType::IrisBlur,
            crate::filters::PsFilterType::Sharpen,
            crate::filters::PsFilterType::UnsharpMask,
            crate::filters::PsFilterType::AddNoise,
            crate::filters::PsFilterType::Despeckle,
            crate::filters::PsFilterType::DustScratches,
            crate::filters::PsFilterType::Pixelate,
            crate::filters::PsFilterType::Crystallize,
            crate::filters::PsFilterType::Emboss,
            crate::filters::PsFilterType::FindEdges,
            crate::filters::PsFilterType::Solarize,
            crate::filters::PsFilterType::Clouds,
            crate::filters::PsFilterType::LensFlare,
            crate::filters::PsFilterType::Ripple,
            crate::filters::PsFilterType::Pinch,
            crate::filters::PsFilterType::Spherize,
            crate::filters::PsFilterType::Liquify,
        ] {
            let id = crate::filters::ps_filter_id(kind);
            assert!(
                entry(id).is_none(),
                "'{id}' is already implemented in filters.rs"
            );
        }
    }

    #[test]
    fn displace_reports_unsupported_and_unknown_ids_are_errors() {
        let mut bitmap = gradient(8, 8);
        let filter = AlgoFilter::new("displace");
        let result = apply_filter(&mut bitmap, &filter, 0);
        assert!(matches!(result, Err(PsError::UnsupportedFilter { .. })));
        if let Err(PsError::UnsupportedFilter { id, .. }) = result {
            assert_eq!(id, "displace");
        }
        assert_eq!(
            bitmap,
            gradient(8, 8),
            "an unsupported filter must not touch the bitmap"
        );

        let filter = AlgoFilter::new("no-such-filter");
        assert!(matches!(
            apply_filter(&mut bitmap, &filter, 0),
            Err(PsError::UnknownFilter(_))
        ));
    }

    #[test]
    fn invalid_parameters_are_errors_not_panics() {
        let mut bitmap = gradient(8, 8);
        let wrong_type =
            AlgoFilter::new("twirl").with("angle", ParamValue::Text("a lot".to_string()));
        assert!(matches!(
            apply_filter(&mut bitmap, &wrong_type, 0),
            Err(PsError::InvalidFilterParameter { .. })
        ));

        let unknown = AlgoFilter::new("twirl").with("nope", ParamValue::Number(1.0));
        assert!(matches!(
            apply_filter(&mut bitmap, &unknown, 0),
            Err(PsError::InvalidFilterParameter { .. })
        ));

        let bad_choice =
            AlgoFilter::new("mezzotint").with("kind", ParamValue::Text("sparkles".to_string()));
        assert!(matches!(
            apply_filter(&mut bitmap, &bad_choice, 0),
            Err(PsError::InvalidFilterParameter { .. })
        ));

        let bad_kernel =
            AlgoFilter::new("custom").with("kernel", ParamValue::Text("1,2,3".to_string()));
        assert!(matches!(
            apply_filter(&mut bitmap, &bad_kernel, 0),
            Err(PsError::InvalidFilterParameter { .. })
        ));
    }

    #[test]
    fn zero_sized_bitmaps_are_a_no_op() {
        let mut bitmap = Surface::new(0, 0);
        let filter = AlgoFilter::new("mosaic");
        apply_filter(&mut bitmap, &filter, 0).expect("empty bitmap is a no-op");
        assert!(bitmap.is_empty());
    }

    #[test]
    fn camel_case_becomes_kebab_case_for_stable_ids() {
        assert_eq!(camel_to_kebab("coloredPencil"), "colored-pencil");
        assert_eq!(camel_to_kebab("watercolor"), "watercolor");
        assert_eq!(camel_to_kebab("angledStrokes"), "angled-strokes");
        assert_eq!(humanize("pencilWidth"), "Pencil Width");
    }
}
