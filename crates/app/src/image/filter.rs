//! The filter dialog (`ps-filter-dialog.tsx`): 320px preview, per-filter
//! parameters, liquify drag, reset and the rasterize warning. Applying goes
//! through `App::image_filter_apply` so the recorder captures the same step.
//! Owns the open filter dialog, its preview, and the data-driven Filter menu.
//!
//! Two filter families share one dialog:
//!
//! * [`PsFilterType`] — the 22 native kernels in `open_canvas_ps::filters`;
//! * [`FilterEntry`] — the 90 filters of the vendored engine, reached through
//!   `open_canvas_ps::algo_bridge`.
//!
//! [`FilterChoice`] is the unified identity, [`FilterParams`] the unified
//! parameter bag, and [`filter_param_rows`] builds an identical Slint model from
//! either schema.

use std::collections::BTreeMap;

use open_canvas_ps::filters::{
    ps_filter_from_id, ps_filter_id, ps_filter_params, ps_liquify_push, PsFilterType,
};
use open_canvas_ps::{
    algo_filter_entry, apply_filter as apply_algo_filter, available_algo_filters,
    gallery_categories, gallery_category, AlgoFilter, CanvasPsLayerKind, CanvasPsParamValue,
    ParamKind, ParamValue, PsParams, Surface,
};

use crate::app::App;
use crate::image::actions::filter_id;
use crate::vm;
use crate::AppWindow;

/// `ps-filter-dialog.tsx`: the preview's longest side.
const PREVIEW_MAX: usize = 320;

/// The seed used for engine filters that take one but whose schema has no
/// `seed` parameter, so a preview and an apply always agree.
const FIXED_SEED: u64 = 0x5EED;

/// Unified filter identity: a native kernel or a vendored-engine filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterChoice {
    Native(PsFilterType),
    Algo(&'static open_canvas_ps::FilterEntry),
}

impl FilterChoice {
    /// Resolves a stable id: native first, then the engine catalogue.
    pub fn from_id(id: &str) -> Option<Self> {
        ps_filter_from_id(id)
            .map(Self::Native)
            .or_else(|| algo_filter_entry(id).map(Self::Algo))
    }

    /// The stable id recorded in actions and used by `filter.<id>` actions.
    pub fn id(self) -> &'static str {
        match self {
            Self::Native(kind) => ps_filter_id(kind),
            Self::Algo(entry) => entry.id.as_str(),
        }
    }

    /// The dialog title (`Filter: <label>`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Native(kind) => filter_label(kind),
            Self::Algo(entry) => entry.label.as_str(),
        }
    }

    /// Whether the filter can run on this document model.
    pub fn supported(self) -> bool {
        match self {
            Self::Native(_) => true,
            Self::Algo(entry) => entry.supported,
        }
    }

    /// Why an engine filter cannot run (`displace` needs a second image).
    pub fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            Self::Native(_) => None,
            Self::Algo(entry) => entry.unsupported_reason.as_deref(),
        }
    }

    /// Liquify keeps its native drag path; no engine filter uses it.
    pub fn is_liquify(self) -> bool {
        matches!(self, Self::Native(PsFilterType::Liquify))
    }
}

/// The dialog-level parameter bag for either filter family.
///
/// Native filters keep their [`PsParams`] map (numbers, text and text lists);
/// engine filters keep a [`BTreeMap`] of tagged [`ParamValue`]s whose order is
/// stable, which is also the recorder's parameter encoding.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterParams {
    Native(PsParams),
    Algo(BTreeMap<String, ParamValue>),
}

impl FilterParams {
    /// Every parameter at its schema default.
    pub fn defaults(choice: FilterChoice) -> Self {
        match choice {
            FilterChoice::Native(kind) => Self::native(ps_filter_params(kind)),
            FilterChoice::Algo(entry) => Self::Algo(
                entry
                    .params
                    .iter()
                    .map(|spec| (spec.name.clone(), spec.default_value()))
                    .collect(),
            ),
        }
    }

    /// A native parameter bag (the recorder's replay path).
    pub fn native(params: PsParams) -> Self {
        Self::Native(params)
    }

    /// A numeric parameter's current value (engine numbers, integers and 0/1
    /// checkboxes all answer here).
    pub fn number(&self, key: &str) -> Option<f64> {
        match self {
            Self::Native(params) => match params.get(key) {
                Some(CanvasPsParamValue::Number(value)) => Some(*value),
                _ => None,
            },
            Self::Algo(params) => params.get(key).and_then(number_of),
        }
    }

    /// A text parameter's current value (choices store their label).
    pub fn text(&self, key: &str) -> Option<&str> {
        match self {
            Self::Native(params) => match params.get(key) {
                Some(CanvasPsParamValue::Text(value)) => Some(value.as_str()),
                _ => None,
            },
            Self::Algo(params) => match params.get(key) {
                Some(ParamValue::Text(value)) => Some(value.as_str()),
                _ => None,
            },
        }
    }

    /// A checkbox's current value.
    pub fn flag(&self, key: &str) -> bool {
        match self {
            Self::Native(params) => matches!(
                params.get(key),
                Some(CanvasPsParamValue::Number(value)) if *value > 0.5
            ),
            Self::Algo(params) => matches!(params.get(key), Some(ParamValue::Bool(true))),
        }
    }

    /// Sets a native number (no-op for an engine bag, which is typed).
    pub fn set_native_number(&mut self, key: &str, value: f64) {
        if let Self::Native(params) = self {
            params.insert(key.to_string(), CanvasPsParamValue::Number(value));
        }
    }

    /// Sets a native text value (selects and checkbox state are numbers).
    pub fn set_native_text(&mut self, key: &str, value: &str) {
        if let Self::Native(params) = self {
            params.insert(key.to_string(), CanvasPsParamValue::Text(value.to_string()));
        }
    }

    /// Inserts a typed engine value.
    pub fn set_algo(&mut self, key: &str, value: ParamValue) {
        if let Self::Algo(params) = self {
            params.insert(key.to_string(), value);
        }
    }

    /// The engine bag, when this is one.
    pub fn algo(&self) -> Option<&BTreeMap<String, ParamValue>> {
        match self {
            Self::Native(_) => None,
            Self::Algo(params) => Some(params),
        }
    }

    /// The recorder's argument string: the native `key=value;...` encoding, or
    /// a JSON object for an engine bag (sorted keys, tagged values).
    pub fn record_string(&self) -> String {
        match self {
            Self::Native(params) => crate::image::channels::param_string(params),
            Self::Algo(params) => serde_json::to_string(params).unwrap_or_default(),
        }
    }

    /// Decodes [`Self::record_string`] for the given filter (action replay).
    pub fn parse_for(choice: FilterChoice, text: &str) -> Self {
        match choice {
            FilterChoice::Native(_) => Self::Native(crate::image::channels::parse_params(text)),
            FilterChoice::Algo(_) => Self::Algo(serde_json::from_str(text).unwrap_or_default()),
        }
    }
}

/// A number from an engine value (numbers and integers).
fn number_of(value: &ParamValue) -> Option<f64> {
    match value {
        ParamValue::Number(number) => Some(*number),
        ParamValue::Integer(integer) => Some(vm::i2f(*integer)),
        _ => None,
    }
}

/// One open filter dialog.
#[derive(Debug, Clone)]
pub struct FilterDialog {
    pub choice: FilterChoice,
    pub params: FilterParams,
    /// The active layer's bitmap at full size (the apply source).
    pub source: Surface,
    /// The 320px preview of the current result.
    pub preview: Surface,
    /// Liquify's full-size working copy (None until the first push).
    pub working: Option<Surface>,
    pub warning: String,
    pub last: Option<(f64, f64)>,
}

/// Display label per native filter (`PS_FILTERS[].labelKey`).
pub fn filter_label(kind: PsFilterType) -> &'static str {
    use PsFilterType as Filter;
    match kind {
        Filter::GaussianBlur => "Gaussian Blur",
        Filter::BoxBlur => "Box Blur",
        Filter::MotionBlur => "Motion Blur",
        Filter::RadialBlur => "Radial Blur",
        Filter::TiltShift => "Tilt-Shift",
        Filter::IrisBlur => "Iris Blur",
        Filter::Sharpen => "Sharpen",
        Filter::UnsharpMask => "Unsharp Mask",
        Filter::AddNoise => "Add Noise",
        Filter::Despeckle => "Despeckle",
        Filter::DustScratches => "Dust & Scratches",
        Filter::Pixelate => "Pixelate",
        Filter::Crystallize => "Crystallize",
        Filter::Emboss => "Emboss",
        Filter::FindEdges => "Find Edges",
        Filter::Solarize => "Solarize",
        Filter::Clouds => "Clouds",
        Filter::LensFlare => "Lens Flare",
        Filter::Ripple => "Ripple",
        Filter::Pinch => "Pinch",
        Filter::Spherize => "Spherize",
        Filter::Liquify => "Liquify",
    }
}

/// One native filter parameter's control spec (range + select options).
struct ControlSpec {
    key: &'static str,
    label: &'static str,
    kind: &'static str,
    min: f64,
    max: f64,
    options: &'static [&'static str],
}

const fn number(key: &'static str, label: &'static str, min: f64, max: f64) -> ControlSpec {
    ControlSpec {
        key,
        label,
        kind: "number",
        min,
        max,
        options: &[],
    }
}

const fn select(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
) -> ControlSpec {
    ControlSpec {
        key,
        label,
        kind: "select",
        min: 0.0,
        max: 0.0,
        options,
    }
}

const fn check(key: &'static str, label: &'static str) -> ControlSpec {
    ControlSpec {
        key,
        label,
        kind: "check",
        min: 0.0,
        max: 1.0,
        options: &[],
    }
}

/// The control list for one native filter (`PS_FILTERS[].controls` + `defaults`).
fn filter_specs(kind: PsFilterType) -> Vec<ControlSpec> {
    use PsFilterType as Filter;
    match kind {
        Filter::GaussianBlur | Filter::BoxBlur => vec![number("radius", "Radius", 0.0, 100.0)],
        Filter::MotionBlur => vec![
            number("angle", "Angle", -180.0, 180.0),
            number("distance", "Distance", 1.0, 100.0),
        ],
        Filter::RadialBlur => vec![
            number("amount", "Amount", 1.0, 100.0),
            select("method", "Method", &["zoom", "spin"]),
        ],
        Filter::TiltShift => vec![
            number("blur", "Blur", 0.0, 100.0),
            number("angle", "Angle", -180.0, 180.0),
            number("position", "Position", 0.0, 100.0),
            number("width", "Width", 0.0, 100.0),
        ],
        Filter::IrisBlur => vec![
            number("blur", "Blur", 0.0, 100.0),
            number("x", "X", 0.0, 100.0),
            number("y", "Y", 0.0, 100.0),
            number("radius", "Radius", 1.0, 100.0),
            number("feather", "Feather", 0.0, 100.0),
        ],
        Filter::Sharpen => vec![number("amount", "Amount", 1.0, 500.0)],
        Filter::UnsharpMask => vec![
            number("amount", "Amount", 1.0, 500.0),
            number("radius", "Radius", 1.0, 100.0),
            number("threshold", "Threshold", 0.0, 255.0),
        ],
        Filter::AddNoise => vec![
            number("amount", "Amount", 1.0, 100.0),
            select("distribution", "Distribution", &["uniform", "gaussian"]),
            check("monochrome", "Monochrome"),
        ],
        Filter::Despeckle | Filter::FindEdges => Vec::new(),
        Filter::DustScratches => vec![
            number("radius", "Radius", 1.0, 3.0),
            number("threshold", "Threshold", 0.0, 255.0),
        ],
        Filter::Pixelate => vec![number("size", "Size", 2.0, 200.0)],
        Filter::Crystallize => vec![number("size", "Size", 3.0, 200.0)],
        Filter::Emboss => vec![
            number("angle", "Angle", -180.0, 180.0),
            number("height", "Height", 1.0, 20.0),
            number("amount", "Amount", 1.0, 500.0),
        ],
        Filter::Solarize => vec![number("threshold", "Threshold", 0.0, 255.0)],
        Filter::Clouds => vec![
            number("size", "Size", 2.0, 400.0),
            number("seed", "Seed", 0.0, 999.0),
        ],
        Filter::LensFlare => vec![
            number("brightness", "Brightness", 10.0, 300.0),
            number("x", "X", 0.0, 100.0),
            number("y", "Y", 0.0, 100.0),
        ],
        Filter::Ripple => vec![
            number("amount", "Amount", 1.0, 100.0),
            number("size", "Size", 2.0, 400.0),
        ],
        Filter::Pinch => vec![number("amount", "Amount", -100.0, 100.0)],
        Filter::Spherize => vec![
            number("amount", "Amount", -100.0, 100.0),
            select("mode", "Mode", &["normal", "horizontal", "vertical"]),
        ],
        Filter::Liquify => vec![
            number("size", "Size", 10.0, 300.0),
            number("pressure", "Pressure", 1.0, 100.0),
        ],
    }
}

/// The dialog's parameter rows as the Slint model.
pub fn filter_param_rows(
    choice: FilterChoice,
    params: &FilterParams,
) -> Vec<crate::ImageFilterParam> {
    match choice {
        FilterChoice::Native(kind) => native_param_rows(kind, params),
        FilterChoice::Algo(entry) => entry
            .params
            .iter()
            .map(|spec| algo_param_row(spec, params))
            .collect(),
    }
}

/// Native rows keep their historical control kinds: sliders, chip selects and
/// checkboxes.
fn native_param_rows(kind: PsFilterType, params: &FilterParams) -> Vec<crate::ImageFilterParam> {
    let number_bag = match params {
        FilterParams::Native(bag) => Some(bag),
        FilterParams::Algo(_) => None,
    };
    filter_specs(kind)
        .iter()
        .map(|spec| {
            let number = number_bag
                .and_then(|bag| match bag.get(spec.key) {
                    Some(CanvasPsParamValue::Number(value)) => Some(*value),
                    _ => None,
                })
                .unwrap_or(0.0);
            let text = number_bag
                .and_then(|bag| match bag.get(spec.key) {
                    Some(CanvasPsParamValue::Text(value)) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| spec.options.first().copied().unwrap_or("").to_string());
            crate::ImageFilterParam {
                key: spec.key.into(),
                label: spec.label.into(),
                kind: spec.kind.into(),
                value: crate::vm::len(number),
                text: text.as_str().into(),
                min: crate::vm::len(spec.min),
                max: crate::vm::len(spec.max),
                options: slint::ModelRc::new(slint::VecModel::from(
                    spec.options
                        .iter()
                        .map(|option| slint::SharedString::from(*option))
                        .collect::<Vec<_>>(),
                )),
            }
        })
        .collect()
}

/// One engine parameter mapped onto the dialog's generic controls.
fn algo_param_row(
    spec: &open_canvas_ps::ParamSpec,
    params: &FilterParams,
) -> crate::ImageFilterParam {
    let current = params
        .algo()
        .and_then(|bag| bag.get(&spec.name))
        .cloned()
        .unwrap_or_else(|| spec.default_value());
    let mut row = crate::ImageFilterParam {
        key: spec.name.as_str().into(),
        label: spec.label.as_str().into(),
        kind: "text".into(),
        value: 0.0,
        text: "".into(),
        min: 0.0,
        max: 0.0,
        options: slint::ModelRc::new(slint::VecModel::from(Vec::new())),
    };
    match &spec.kind {
        ParamKind::Number { min, max, .. } => {
            row.kind = "number".into();
            row.value = crate::vm::len(number_of(&current).unwrap_or(*min));
            row.min = crate::vm::len(*min);
            row.max = crate::vm::len(*max);
        }
        ParamKind::Integer { min, max, .. } => {
            row.kind = "number".into();
            row.value = crate::vm::len(number_of(&current).unwrap_or(0.0));
            row.min = crate::vm::len(crate::vm::i2f(*min));
            row.max = crate::vm::len(crate::vm::i2f(*max));
        }
        ParamKind::Toggle { .. } => {
            row.kind = "check".into();
            row.value = if params.flag(&spec.name) { 1.0 } else { 0.0 };
            row.max = 1.0;
        }
        ParamKind::Choice { options, .. } => {
            row.kind = "choice".into();
            row.text = params.text(&spec.name).map_or_else(
                || {
                    options
                        .first()
                        .map_or("".into(), |option| option.as_str().into())
                },
                slint::SharedString::from,
            );
            row.options = slint::ModelRc::new(slint::VecModel::from(
                options
                    .iter()
                    .map(|option| slint::SharedString::from(option.as_str()))
                    .collect::<Vec<_>>(),
            ));
        }
        ParamKind::Color { .. } => {
            row.kind = "color".into();
            if let ParamValue::Color(color) = current {
                row.text = color_hex(&color).into();
            }
        }
        ParamKind::Text { .. } => {
            row.kind = "text".into();
            if let ParamValue::Text(value) = current {
                row.text = value.as_str().into();
            }
        }
        ParamKind::Numbers { .. } => {
            row.kind = "numbers".into();
            if let ParamValue::Numbers(values) = current {
                let parts: Vec<String> = values.iter().map(|value| format!("{value}")).collect();
                row.text = parts.join(", ").into();
            }
        }
    }
    row
}

/// `#rrggbb` for a straight sRGB colour (alpha 1 is implied by the editor).
fn color_hex(color: &[f32; 4]) -> String {
    let channel = |value: f32| crate::vm::f2u8((f64::from(value.clamp(0.0, 1.0)) * 255.0).round());
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color[0]),
        channel(color[1]),
        channel(color[2])
    )
}

/// Parses `#rrggbb` / `#rrggbbaa` into a straight sRGB colour.
fn parse_color(text: &str) -> Option<[f32; 4]> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.is_ascii() || (hex.len() != 6 && hex.len() != 8) {
        return None;
    }
    let mut color = [1.0_f32; 4];
    for (index, slot) in color.iter_mut().take(hex.len() / 2).enumerate() {
        let byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
        *slot = f32::from(byte) / 255.0;
    }
    Some(color)
}

/// Parses a comma/space separated numeric list (`108, 162, 90, 45`).
fn parse_numbers(text: &str, expected: usize) -> Option<Vec<f64>> {
    let values: Vec<f64> = text
        .split([',', ';', ' '])
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<_, _>>()
        .ok()?;
    (values.len() == expected && values.iter().all(|value| value.is_finite())).then_some(values)
}

/// Downscales to the dialog preview size (never upscales).
pub fn filter_preview_of(source: &Surface) -> Surface {
    let longest = source.width().max(source.height()).max(1);
    if longest <= PREVIEW_MAX {
        return source.clone();
    }
    let scale = vm::i2f(vm::u2i(PREVIEW_MAX)) / vm::i2f(vm::u2i(longest));
    let width = vm::i2u(vm::f2i(vm::i2f(vm::u2i(source.width())) * scale)).max(1);
    let height = vm::i2u(vm::f2i(vm::i2f(vm::u2i(source.height())) * scale)).max(1);
    source.resample_bilinear(width, height)
}

/// The seed an engine filter runs with: its own `seed` parameter when the
/// schema has one, [`FIXED_SEED`] otherwise (previews and applies agree).
fn seed_for(entry: &open_canvas_ps::FilterEntry, params: &FilterParams) -> u64 {
    if !entry.params.iter().any(|spec| spec.name == "seed") {
        return FIXED_SEED;
    }
    match params.number("seed") {
        Some(value) if value.is_finite() => {
            u64::try_from(crate::vm::f2i(value.abs())).unwrap_or(FIXED_SEED)
        }
        _ => FIXED_SEED,
    }
}

/// Runs one filter over a surface in place (the one call the dialog, apply and
/// repeat share).
pub fn apply_choice(
    choice: FilterChoice,
    params: &FilterParams,
    surface: &mut Surface,
) -> Result<(), String> {
    match choice {
        FilterChoice::Native(kind) => {
            let empty = PsParams::new();
            let bag = match params {
                FilterParams::Native(bag) => bag,
                FilterParams::Algo(_) => &empty,
            };
            open_canvas_ps::filters::apply_ps_filter(surface, kind, bag);
            Ok(())
        }
        FilterChoice::Algo(entry) => {
            if !entry.supported {
                return Err(entry.unsupported_reason.clone().unwrap_or_else(|| {
                    format!(
                        "{} needs an input this document model does not provide",
                        entry.label
                    )
                }));
            }
            let mut filter = AlgoFilter::new(entry.id.as_str());
            if let Some(bag) = params.algo() {
                filter.params = bag.clone();
            }
            apply_algo_filter(surface, &filter, seed_for(entry, params))
                .map_err(|error| error.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Filter menu (data-driven; built once per session)
// ---------------------------------------------------------------------------

/// Photoshop's Filter-menu groups, in menu order. The engine's catalogue uses
/// these keys for its `group` field; native filters are mapped onto them here.
pub const PHOTOSHOP_GROUPS: [(&str, &str); 9] = [
    ("blur", "Blur"),
    ("distort", "Distort"),
    ("noise", "Noise"),
    ("pixelate", "Pixelate"),
    ("render", "Render"),
    ("sharpen", "Sharpen"),
    ("stylize", "Stylize"),
    ("other", "Other"),
    ("video", "Video"),
];

/// Native filters in their menu order (the order of `PS_FILTERS`).
const NATIVE_FILTER_ORDER: [PsFilterType; 22] = [
    PsFilterType::GaussianBlur,
    PsFilterType::BoxBlur,
    PsFilterType::MotionBlur,
    PsFilterType::RadialBlur,
    PsFilterType::TiltShift,
    PsFilterType::IrisBlur,
    PsFilterType::Sharpen,
    PsFilterType::UnsharpMask,
    PsFilterType::AddNoise,
    PsFilterType::Despeckle,
    PsFilterType::DustScratches,
    PsFilterType::Pixelate,
    PsFilterType::Crystallize,
    PsFilterType::Emboss,
    PsFilterType::FindEdges,
    PsFilterType::Solarize,
    PsFilterType::Clouds,
    PsFilterType::LensFlare,
    PsFilterType::Ripple,
    PsFilterType::Pinch,
    PsFilterType::Spherize,
    PsFilterType::Liquify,
];

/// The Photoshop group a native filter belongs to.
fn native_group(kind: PsFilterType) -> &'static str {
    use PsFilterType as Filter;
    match kind {
        Filter::GaussianBlur
        | Filter::BoxBlur
        | Filter::MotionBlur
        | Filter::RadialBlur
        | Filter::TiltShift
        | Filter::IrisBlur => "blur",
        Filter::Ripple | Filter::Pinch | Filter::Spherize | Filter::Liquify => "distort",
        Filter::AddNoise | Filter::Despeckle | Filter::DustScratches => "noise",
        Filter::Pixelate | Filter::Crystallize => "pixelate",
        Filter::Sharpen | Filter::UnsharpMask => "sharpen",
        Filter::Emboss
        | Filter::FindEdges
        | Filter::Solarize
        | Filter::Clouds
        | Filter::LensFlare => "stylize",
    }
}

/// The flyout key of the Filter Gallery's own submenu.
const GALLERY_GROUP: &str = "gallery";

/// Engine filters Photoshop files under Filter ▸ Video. The engine's catalogue
/// tags them `other`, so the menu files them here (keeping their ids).
const VIDEO_FILTER_IDS: [&str; 2] = ["de-interlace", "ntsc-colors"];

/// The menu group of one engine entry: Video wins over the catalogue's tag.
fn algo_group(entry: &open_canvas_ps::FilterEntry) -> &str {
    if VIDEO_FILTER_IDS.contains(&entry.id.as_str()) {
        "video"
    } else {
        entry.group.as_str()
    }
}

/// The flyout key of one gallery folder (`gallery/brush-strokes`).
fn gallery_folder_key(category: &str) -> String {
    format!(
        "{GALLERY_GROUP}/{}",
        category.to_lowercase().replace(' ', "-")
    )
}

/// Builds the Filter menu's rows as a nested menu model: the top level is one
/// group row per non-empty Photoshop group, then Filter Gallery — whose flyout
/// holds one group row per gallery folder.
///
/// Every row names the flyout it lives in (`parent`, `""` = top level) and a
/// group row names the flyout it opens (`group`), so the UI can render each
/// flyout from the same flat, static model.
pub fn filter_menu_rows() -> Vec<crate::FilterMenuRow> {
    let mut rows = Vec::new();
    for (group, label) in PHOTOSHOP_GROUPS {
        let native: Vec<PsFilterType> = NATIVE_FILTER_ORDER
            .iter()
            .copied()
            .filter(|kind| native_group(*kind) == group)
            .collect();
        let algo: Vec<&open_canvas_ps::FilterEntry> = available_algo_filters()
            .iter()
            .filter(|entry| algo_group(entry) == group)
            .collect();
        if native.is_empty() && algo.is_empty() {
            continue;
        }
        rows.push(group_row("", label, group));
        for kind in native {
            rows.push(choice_row(FilterChoice::Native(kind), group));
        }
        for entry in algo {
            rows.push(choice_row(FilterChoice::Algo(entry), group));
        }
    }

    let gallery: Vec<&open_canvas_ps::FilterEntry> = available_algo_filters()
        .iter()
        .filter(|entry| entry.group == GALLERY_GROUP)
        .collect();
    if !gallery.is_empty() {
        rows.push(group_row("", "Filter Gallery", GALLERY_GROUP));
        for category in gallery_categories() {
            let folder = gallery_folder_key(category);
            let in_category: Vec<&open_canvas_ps::FilterEntry> = gallery
                .iter()
                .copied()
                .filter(|entry| gallery_category(entry) == Some(*category))
                .collect();
            if in_category.is_empty() {
                continue;
            }
            rows.push(group_row(GALLERY_GROUP, category, &folder));
            for entry in in_category {
                rows.push(choice_row(FilterChoice::Algo(entry), &folder));
            }
        }
    }
    rows
}

/// A menu row from a filter choice, inside the flyout `parent`.
fn choice_row(choice: FilterChoice, parent: &str) -> crate::FilterMenuRow {
    menu_row(
        "item",
        choice.label(),
        choice.id(),
        choice.supported(),
        choice.unsupported_reason().unwrap_or(""),
        parent,
        "",
    )
}

/// A row that opens the submenu `group` from inside `parent`.
fn group_row(parent: &str, label: &str, group: &str) -> crate::FilterMenuRow {
    menu_row("group", label, group, true, "", parent, group)
}

/// One Slint menu row.
fn menu_row(
    kind: &str,
    label: &str,
    id: &str,
    enabled: bool,
    note: &str,
    parent: &str,
    group: &str,
) -> crate::FilterMenuRow {
    crate::FilterMenuRow {
        kind: kind.into(),
        label: label.into(),
        id: id.into(),
        enabled,
        note: note.into(),
        parent: parent.into(),
        group: group.into(),
    }
}

// ---------------------------------------------------------------------------
// Dialog lifecycle
// ---------------------------------------------------------------------------

impl App {
    /// The dialog open without a window (unit tests drive this directly).
    pub fn image_filter_open_core(&mut self, id: &str) -> Result<(), String> {
        let choice = FilterChoice::from_id(id).ok_or_else(|| format!("Unknown filter: {id}"))?;
        if !choice.supported() {
            return Err(choice
                .unsupported_reason()
                .unwrap_or("This filter needs an input the document model does not provide")
                .to_string());
        }
        let active = self.image.active_layer_id();
        let layer =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &active)
                .cloned();
        let bitmap_kind = layer.as_ref().is_some_and(|layer| {
            matches!(
                layer.kind,
                CanvasPsLayerKind::Pixel | CanvasPsLayerKind::Image
            )
        });
        let warning = if bitmap_kind {
            String::new()
        } else {
            "This layer is not a pixel layer: applying rasterizes it first.".to_string()
        };
        let source = if bitmap_kind {
            self.image
                .ensure_layer_bitmap(&active)
                .and_then(|key| self.image.bitmaps.get(&key).cloned())
        } else {
            self.rasterize_layer_surface(&active)
        };
        let Some(source) = source else {
            return Err("Filters need a pixel, image, text or shape layer".to_string());
        };
        let params = FilterParams::defaults(choice);
        let preview = filter_preview_of(&source);
        let mut dialog = FilterDialog {
            choice,
            params,
            source,
            preview,
            working: None,
            warning,
            last: None,
        };
        refresh_preview(&mut dialog);
        self.image.filter_dialog = Some(dialog);
        Ok(())
    }

    /// A layer's own raster, rendered through the ps compositor at its box size
    /// (`renderPsLayerBitmap` for the app layer; text gains glyphs in P5c-3's
    /// glyph pass).
    pub fn rasterize_layer_surface(&mut self, layer_id: &str) -> Option<Surface> {
        let layer =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, layer_id)?
                .clone();
        let width = vm::f2i(layer.width.round().max(1.0));
        let height = vm::f2i(layer.height.round().max(1.0));
        if width <= 0 || height <= 0 || width.checked_mul(height)? > 16_777_216 {
            return None;
        }
        let mut project = self.image.project.clone();
        let mut solo = layer;
        solo.x = 0.0;
        solo.y = 0.0;
        solo.width = vm::i2f(width);
        solo.height = vm::i2f(height);
        solo.rotation = 0.0;
        project.board_layers = vec![solo];
        project.width = vm::i2f(width);
        project.height = vm::i2f(height);
        project.board_background = "transparent".to_string();
        crate::image::render::board_surface(&project, &self.image.bitmaps)
    }

    /// One filter parameter (number / check).
    pub fn image_filter_param(&mut self, ui: &AppWindow, key: &str, value: f64) {
        self.image_filter_param_core(key, value);
        self.sync_image(ui);
    }

    /// The parameter edit without a window (unit tests drive this directly).
    pub fn image_filter_param_core(&mut self, key: &str, value: f64) {
        let Some(dialog) = self.image.filter_dialog.as_mut() else {
            return;
        };
        match dialog.choice {
            FilterChoice::Native(_) => dialog.params.set_native_number(key, value),
            FilterChoice::Algo(entry) => {
                let typed = entry.params.iter().find(|spec| spec.name == key).map_or(
                    ParamValue::Number(value),
                    |spec| match &spec.kind {
                        ParamKind::Integer { .. } => {
                            ParamValue::Integer(crate::vm::f2i(value.round()))
                        }
                        ParamKind::Toggle { .. } => ParamValue::Bool(value > 0.5),
                        _ => ParamValue::Number(value),
                    },
                );
                dialog.params.set_algo(key, typed);
            }
        }
        refresh_preview(dialog);
    }

    /// One filter select / text / colour / numeric-list edit.
    pub fn image_filter_param_text(&mut self, ui: &AppWindow, key: &str, text: &str) {
        self.image_filter_param_text_core(key, text);
        self.sync_image(ui);
    }

    /// The text edit without a window (unit tests drive this directly).
    pub fn image_filter_param_text_core(&mut self, key: &str, text: &str) {
        let Some(dialog) = self.image.filter_dialog.as_mut() else {
            return;
        };
        match dialog.choice {
            FilterChoice::Native(_) => dialog.params.set_native_text(key, text),
            FilterChoice::Algo(entry) => {
                let Some(spec) = entry.params.iter().find(|spec| spec.name == key) else {
                    return;
                };
                let typed = match &spec.kind {
                    ParamKind::Color { .. } => match parse_color(text) {
                        Some(color) => ParamValue::Color(color),
                        None => return,
                    },
                    ParamKind::Numbers { len, .. } => match parse_numbers(text, *len) {
                        Some(values) => ParamValue::Numbers(values),
                        None => return,
                    },
                    _ => ParamValue::Text(text.to_string()),
                };
                dialog.params.set_algo(key, typed);
            }
        }
        refresh_preview(dialog);
    }

    /// Reset to the filter's schema defaults (`ps-filter-dialog.tsx` Reset).
    pub fn image_filter_reset(&mut self, ui: &AppWindow) {
        self.image_filter_reset_core();
        self.sync_image(ui);
    }

    /// The reset without a window (unit tests drive this directly).
    pub fn image_filter_reset_core(&mut self) {
        let Some(dialog) = self.image.filter_dialog.as_mut() else {
            return;
        };
        dialog.params = FilterParams::defaults(dialog.choice);
        dialog.working = None;
        dialog.last = None;
        refresh_preview(dialog);
    }

    /// Cancel: closes without applying (`ps-filter-dialog.tsx` Cancel).
    pub fn image_filter_cancel(&mut self, ui: &AppWindow) {
        self.image_filter_cancel_core();
        self.sync_image(ui);
    }

    /// The cancel without a window (unit tests drive this directly).
    pub fn image_filter_cancel_core(&mut self) {
        self.image.filter_dialog = None;
    }

    /// Liquify drag on the preview: normalized `(x, y)` -> a layer-space push.
    pub fn image_filter_push(&mut self, ui: &AppWindow, x: f64, y: f64) {
        self.image_filter_push_core(x, y);
        self.sync_image(ui);
    }

    /// The liquify push without a window (unit tests drive this directly).
    pub fn image_filter_push_core(&mut self, x: f64, y: f64) {
        let Some(dialog) = self.image.filter_dialog.as_mut() else {
            return;
        };
        if !dialog.choice.is_liquify() {
            return;
        }
        let (width, height) = (
            vm::i2f(vm::u2i(dialog.source.width())),
            vm::i2f(vm::u2i(dialog.source.height())),
        );
        let point =
            open_canvas_ps::Point::new(x.clamp(0.0, 1.0) * width, y.clamp(0.0, 1.0) * height);
        let (delta_x, delta_y) = match dialog.last {
            Some((lx, ly)) => (point.x - lx, point.y - ly),
            None => (0.0, 0.0),
        };
        dialog.last = Some((point.x, point.y));
        let radius = dialog.params.number("size").map_or(40.0, |size| size / 2.0);
        let pressure = dialog
            .params
            .number("pressure")
            .map_or(0.6, |pressure| pressure / 100.0);
        if dialog.working.is_none() {
            dialog.working = Some(dialog.source.clone());
        }
        if let Some(working) = dialog.working.as_mut() {
            ps_liquify_push(
                working,
                point,
                open_canvas_ps::Point::new(delta_x, delta_y),
                radius,
                pressure,
            );
        }
        refresh_preview(dialog);
    }

    /// Apply: filter the layer bitmap (rasterizing non-pixel layers first),
    /// push history and close (`ps-filter-dialog.tsx` Apply).
    pub fn image_filter_apply_dialog(&mut self, ui: &AppWindow) {
        if let Err(message) = self.image_filter_apply_dialog_core() {
            self.image.filter_dialog = None;
            self.set_notice(ui, message, true);
            return;
        }
        self.image.filter_dialog = None;
        self.sync_image(ui);
    }

    /// The dialog apply without a window (unit tests drive this directly).
    pub fn image_filter_apply_dialog_core(&mut self) -> Result<(), String> {
        let Some(dialog) = self.image.filter_dialog.clone() else {
            return Err("No filter dialog is open".to_string());
        };
        let active = self.image.active_layer_id();
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &active)
                .cloned()
        else {
            return Err("Filters need an active layer".to_string());
        };
        let bitmap_kind = matches!(
            layer.kind,
            CanvasPsLayerKind::Pixel | CanvasPsLayerKind::Image
        );
        let key = if bitmap_kind {
            self.image
                .ensure_layer_bitmap(&active)
                .ok_or_else(|| "Filters need a pixel or image layer".to_string())?
        } else {
            // Rasterize first (`rasterizePsLayer` + the bitmap the dialog read).
            let key = format!("image:{active}");
            self.image
                .bitmaps
                .insert(key.clone(), dialog.source.clone());
            let layers =
                open_canvas_ps::rasterize_ps_layer(&self.image.project.board_layers, &active, &key);
            self.image.project.board_layers = layers;
            key
        };
        let mut result = dialog
            .working
            .clone()
            .unwrap_or_else(|| dialog.source.clone());
        apply_choice(dialog.choice, &dialog.params, &mut result)?;
        self.image.begin_edit(std::slice::from_ref(&key));
        self.image.bitmaps.insert(key, result);
        self.image.recorder.record(
            "filter",
            filter_id(dialog.choice),
            &dialog.params.record_string(),
        );
        self.commit_image_core("Filter");
        self.image.filter_dialog = None;
        Ok(())
    }
}

/// Recomputes the preview from the working copy (or the source) + parameters.
fn refresh_preview(dialog: &mut FilterDialog) {
    let mut source = dialog
        .working
        .clone()
        .unwrap_or_else(|| dialog.source.clone());
    if apply_choice(dialog.choice, &dialog.params, &mut source).is_ok() {
        dialog.preview = filter_preview_of(&source);
    }
}

/// Pushes the filter dialog + Filter menu view models (`sync_image_extra`).
pub fn sync_filter_ui(app: &App, ui: &AppWindow) {
    use slint::Model;

    // The menu is static: build its rows once, then keep the model bound.
    if ui.get_filter_menu_rows().row_count() == 0 {
        ui.set_filter_menu_rows(slint::ModelRc::new(slint::VecModel::from(
            filter_menu_rows(),
        )));
    }
    if let Some(dialog) = &app.image.filter_dialog {
        ui.set_image_filter_open(true);
        ui.set_image_filter_label(dialog.choice.label().into());
        ui.set_image_filter_params(slint::ModelRc::new(slint::VecModel::from(
            filter_param_rows(dialog.choice, &dialog.params),
        )));
        ui.set_image_filter_preview(crate::image::render::image_of(&dialog.preview));
        ui.set_image_filter_warning(dialog.warning.as_str().into());
        ui.set_image_filter_liquify(dialog.choice.is_liquify());
    } else {
        ui.set_image_filter_open(false);
        ui.set_image_filter_warning("".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{CanvasPsLayerKind, IdGen, ImageProject};

    fn rich_board() -> (ImageProject, HashMap<String, Surface>) {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        project.width = 24.0;
        project.height = 24.0;
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (24.0, 24.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(24, 24);
        for y in 0..24 {
            for x in 0..24 {
                let checker = (x / 3 + y / 3) % 2 == 0;
                surface.set(
                    x,
                    y,
                    if checker {
                        [200, 100, 50, 255]
                    } else {
                        [40, 60, 220, 255]
                    },
                );
            }
        }
        // Isolated speckles so Despeckle has outliers to remove.
        surface.set(2, 2, [255, 255, 255, 255]);
        surface.set(11, 5, [0, 0, 0, 255]);
        surface.set(17, 17, [255, 255, 255, 255]);
        bitmaps.insert("image:t-1".to_string(), surface);
        (project, bitmaps)
    }

    fn filter_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let (project, bitmaps) = rich_board();
        crate::test_support::app_with_image(project, bitmaps)
    }

    fn bitmap(app: &App) -> Vec<u8> {
        app.image
            .bitmaps
            .get("image:t-1")
            .map(|bitmap| bitmap.data().to_vec())
            .unwrap_or_default()
    }

    const ALL_NATIVE: [&str; 22] = [
        "gaussian-blur",
        "box-blur",
        "motion-blur",
        "radial-blur",
        "tilt-shift",
        "iris-blur",
        "sharpen",
        "unsharp-mask",
        "add-noise",
        "despeckle",
        "dust-scratches",
        "pixelate",
        "crystallize",
        "emboss",
        "find-edges",
        "solarize",
        "clouds",
        "lens-flare",
        "ripple",
        "pinch",
        "spherize",
        "liquify",
    ];

    /// Every native id plus every supported engine id.
    fn all_supported_ids() -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = ALL_NATIVE.to_vec();
        ids.extend(
            available_algo_filters()
                .iter()
                .filter(|entry| entry.supported)
                .map(|entry| entry.id.as_str()),
        );
        ids
    }

    /// A few engine filters are neutral at their schema defaults (`offset`
    /// 0/0, an identity `custom` kernel, `hsb-hsl` rgb->rgb, a flat `shear`
    /// curve, `smart-blur` on flat blocks, `ntsc-colors` on in-gamut pixels).
    /// Nudge them so the apply has pixels to change; everything else runs at
    /// its defaults.
    fn nudge_neutral_default(app: &mut App, id: &str) {
        match id {
            "smart-blur" => {
                app.image_filter_param_core("radius", 30.0);
                app.image_filter_param_text_core("mode", "edgeOnly");
            }
            "shear" => app.image_filter_param_text_core("points", "0,0;0.5,0.25;1,0"),
            "offset" => {
                app.image_filter_param_core("horizontal", 5.0);
                app.image_filter_param_core("vertical", 5.0);
            }
            "custom" => app.image_filter_param_core("offset", 60.0),
            "hsb-hsl" => app.image_filter_param_text_core("output", "hsb"),
            "ntsc-colors" => {
                if let Some(surface) = app.image.bitmaps.get_mut("image:t-1") {
                    surface.set(0, 0, [255, 0, 0, 255]);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn every_filter_opens_in_the_dialog_with_a_preview_and_applies(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for id in all_supported_ids() {
            let (mut app, _dir) = filter_board_app()?;
            app.image_filter_open_core(id)
                .map_err(|error| format!("{id}: {error}"))?;
            let dialog = app.image.filter_dialog.clone().ok_or("dialog")?;
            assert!(
                dialog.preview.width() <= PREVIEW_MAX && dialog.preview.height() <= PREVIEW_MAX,
                "{id} preview is bounded"
            );
            assert!(
                dialog.preview.width() > 0 && dialog.preview.height() > 0,
                "{id} has a preview"
            );
            assert!(dialog.warning.is_empty(), "{id} targets a pixel layer");
            assert_eq!(dialog.choice.id(), id, "{id} keeps its identity");
            if dialog.choice.is_liquify() {
                // Liquify only changes pixels through drags (`psLiquifyPush`).
                app.image_filter_push_core(0.4, 0.4);
                app.image_filter_push_core(0.6, 0.6);
            } else {
                nudge_neutral_default(&mut app, id);
            }
            let before = bitmap(&app);

            app.image_filter_apply_dialog_core()
                .map_err(|error| format!("{id}: {error}"))?;
            assert!(app.image.filter_dialog.is_none(), "{id} closes on apply");
            let after = bitmap(&app);
            assert!(after != before, "{id} must filter the bitmap");
            assert_eq!(app.image.history.label(1), Some("Filter"), "{id}");
        }
        Ok(())
    }

    #[test]
    fn every_filter_exposes_its_schema_controls() -> Result<(), Box<dyn std::error::Error>> {
        for id in ALL_NATIVE {
            let choice = FilterChoice::from_id(id)
                .ok_or_else(|| format!("{id} decodes to a filter kind"))?;
            let params = FilterParams::defaults(choice);
            let rows = filter_param_rows(choice, &params);
            let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
            for row in &rows {
                assert!(
                    ["number", "select", "check"].contains(&row.kind.as_str()),
                    "{id} control {} renders",
                    row.key
                );
            }
            let expected: &[&str] = match id {
                "gaussian-blur" | "box-blur" => &["radius"],
                "motion-blur" => &["angle", "distance"],
                "radial-blur" => &["amount", "method"],
                "tilt-shift" => &["blur", "angle", "position", "width"],
                "iris-blur" => &["blur", "x", "y", "radius", "feather"],
                "sharpen" | "pinch" => &["amount"],
                "unsharp-mask" => &["amount", "radius", "threshold"],
                "add-noise" => &["amount", "distribution", "monochrome"],
                "despeckle" | "find-edges" => &[],
                "dust-scratches" => &["radius", "threshold"],
                "pixelate" | "crystallize" => &["size"],
                "emboss" => &["angle", "height", "amount"],
                "solarize" => &["threshold"],
                "clouds" => &["size", "seed"],
                "lens-flare" => &["brightness", "x", "y"],
                "ripple" => &["amount", "size"],
                "spherize" => &["amount", "mode"],
                "liquify" => &["size", "pressure"],
                other => return Err(format!("{other} has no control expectation").into()),
            };
            assert_eq!(keys, expected, "{id} controls (PS_FILTERS[].controls)");
        }

        // Engine rows mirror the catalogue schema one-for-one, and every kind
        // maps onto a control the dialog renders.
        for entry in available_algo_filters() {
            let choice = FilterChoice::Algo(entry);
            let rows = filter_param_rows(choice, &FilterParams::defaults(choice));
            let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
            let expected: Vec<&str> = entry.params.iter().map(|spec| spec.name.as_str()).collect();
            assert_eq!(keys, expected, "{} schema order", entry.id);
            for row in &rows {
                assert!(
                    ["number", "select", "check", "text", "color", "choice", "numbers"]
                        .contains(&row.kind.as_str()),
                    "{} control {} renders",
                    entry.id,
                    row.key
                );
            }
        }
        Ok(())
    }

    #[test]
    fn filter_parameters_rerender_the_preview_and_reset_restores_defaults(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        app.image_filter_open_core("gaussian-blur")?;
        let default_preview = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.preview.data().to_vec())
            .unwrap_or_default();

        app.image_filter_param_core("radius", 40.0);
        let heavy = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.preview.data().to_vec())
            .unwrap_or_default();
        assert!(
            heavy != default_preview,
            "the parameter changes the preview"
        );
        let params = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| filter_param_rows(dialog.choice, &dialog.params))
            .unwrap_or_default();
        let radius = params.iter().find(|row| row.key == "radius");
        assert!(radius.is_some_and(|row| (row.value - 40.0).abs() < 1e-3));

        app.image_filter_reset_core();
        let reset = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.preview.data().to_vec())
            .unwrap_or_default();
        assert_eq!(reset, default_preview, "reset restores the default result");

        // Selects route through the same param bag (spherize mode).
        app.image_filter_cancel_core();
        app.image_filter_open_core("spherize")?;
        app.image_filter_param_text_core("mode", "horizontal");
        let params = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.params.clone())
            .ok_or("dialog")?;
        assert_eq!(
            params.text("mode"),
            Some("horizontal"),
            "the native select stores its text"
        );

        app.image_filter_cancel_core();
        assert!(
            app.image.filter_dialog.is_none(),
            "cancel closes the dialog"
        );
        Ok(())
    }

    #[test]
    fn engine_number_parameters_rerender_and_reset_restores_defaults(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        app.image_filter_open_core("oil-paint")?;
        let default_preview = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.preview.data().to_vec())
            .unwrap_or_default();
        let defaults = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.params.clone())
            .ok_or("dialog")?;

        app.image_filter_param_core("stylization", 9.0);
        assert_eq!(
            app.image
                .filter_dialog
                .as_ref()
                .and_then(|dialog| dialog.params.number("stylization")),
            Some(9.0),
            "the engine number lands in the bag"
        );
        let heavy = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.preview.data().to_vec())
            .unwrap_or_default();
        assert!(heavy != default_preview, "the engine preview rerenders");

        app.image_filter_reset_core();
        let reset = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| (dialog.preview.data().to_vec(), dialog.params.clone()))
            .ok_or("dialog")?;
        assert_eq!(reset.0, default_preview, "reset restores the preview");
        assert_eq!(reset.1, defaults, "reset restores the schema defaults");
        Ok(())
    }

    #[test]
    fn engine_choices_and_toggles_route_through_the_dialog(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        app.image_filter_open_core("wind")?;
        app.image_filter_param_text_core("method", "blast");
        app.image_filter_param_core("fromRight", 0.0);
        let params = app
            .image
            .filter_dialog
            .as_ref()
            .map(|dialog| dialog.params.clone())
            .ok_or("dialog")?;
        assert_eq!(params.text("method"), Some("blast"));
        assert!(!params.flag("fromRight"));
        Ok(())
    }

    #[test]
    fn liquify_drag_pushes_the_working_copy_and_apply_commits_it(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        let before = bitmap(&app);
        app.image_filter_open_core("liquify")?;
        assert!(app
            .image
            .filter_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.choice.is_liquify()));

        app.image_filter_push_core(0.4, 0.4);
        app.image_filter_push_core(0.6, 0.6);
        let dialog = app.image.filter_dialog.clone().ok_or("dialog")?;
        assert!(dialog.working.is_some(), "the drag builds a working copy");

        app.image_filter_apply_dialog_core()?;
        let after = bitmap(&app);
        assert!(after != before, "the liquify result is committed");
        Ok(())
    }

    #[test]
    fn non_pixel_layers_carry_the_rasterize_warning_and_apply_rasterizes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        let mut gen = IdGen::new("s");
        let shape = open_canvas_ps::create_ps_shape_layer(
            &mut gen,
            open_canvas_ps::CanvasPsShapeKind::Rectangle,
            open_canvas_ps::Bounds::new(2.0, 2.0, 12.0, 12.0),
            "Shape",
            "#ff0000",
            "",
            0.0,
        );
        let shape_id = shape.id.clone();
        app.image.project.board_layers.push(shape);
        app.image.active_layer = shape_id.clone();

        app.image_filter_open_core("gaussian-blur")?;
        let dialog = app.image.filter_dialog.clone().ok_or("dialog")?;
        assert!(!dialog.warning.is_empty(), "the rasterize warning shows");
        app.image_filter_apply_dialog_core()?;
        let layer = app
            .image
            .project
            .board_layers
            .iter()
            .find(|layer| layer.id == shape_id)
            .cloned()
            .ok_or("layer")?;
        assert_eq!(
            layer.kind,
            CanvasPsLayerKind::Pixel,
            "apply rasterizes the layer"
        );
        assert!(layer.storage_key.is_some());
        Ok(())
    }

    #[test]
    fn a_gallery_filter_opens_previews_and_applies() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        let before = bitmap(&app);
        app.image_filter_open_core("gallery-watercolor")?;
        let dialog = app.image.filter_dialog.clone().ok_or("dialog")?;
        assert_eq!(dialog.choice.label(), "Watercolor");
        assert!(!dialog.preview.data().is_empty(), "the preview is rendered");
        assert!(
            !filter_param_rows(dialog.choice, &dialog.params).is_empty(),
            "the gallery schema has controls"
        );
        app.image_filter_apply_dialog_core()?;
        assert!(bitmap(&app) != before, "the gallery filter applies");
        assert_eq!(app.image.history.label(1), Some("Filter"));
        Ok(())
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one assertion sweep over the whole nested menu model (ids, groups, flyouts, disabled reasons)"
    )]
    fn the_filter_menu_lists_every_filter_once_in_photoshop_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let rows = filter_menu_rows();
        assert_eq!(rows.first().map(|row| row.kind.as_str()), Some("group"));
        assert_eq!(rows.first().map(|row| row.label.as_str()), Some("Blur"));
        assert_eq!(rows.first().map(|row| row.parent.as_str()), Some(""));

        let item_ids: Vec<&str> = rows
            .iter()
            .filter(|row| row.kind == "item")
            .map(|row| row.id.as_str())
            .collect();
        for id in ALL_NATIVE {
            assert_eq!(
                item_ids.iter().filter(|found| **found == id).count(),
                1,
                "{id} appears exactly once in the menu"
            );
        }
        for entry in available_algo_filters() {
            let count = item_ids.iter().filter(|found| **found == entry.id).count();
            if entry.supported {
                assert_eq!(count, 1, "{} appears exactly once", entry.id);
            } else {
                assert_eq!(count, 1, "{} is still listed (disabled)", entry.id);
            }
        }
        assert_eq!(
            item_ids.len(),
            ALL_NATIVE.len() + available_algo_filters().len(),
            "the menu reaches every filter, no duplicates"
        );

        // The top level is one submenu per non-empty group, then Filter Gallery.
        let top: Vec<&crate::FilterMenuRow> =
            rows.iter().filter(|row| row.parent.is_empty()).collect();
        assert_eq!(
            top.iter()
                .map(|row| (row.kind.as_str(), row.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("group", "Blur"),
                ("group", "Distort"),
                ("group", "Noise"),
                ("group", "Pixelate"),
                ("group", "Render"),
                ("group", "Sharpen"),
                ("group", "Stylize"),
                ("group", "Other"),
                ("group", "Video"),
                ("group", "Filter Gallery"),
            ],
            "Photoshop group order, each opening a flyout, then the gallery"
        );

        // Filter Gallery opens the six folders, each opening its own flyout.
        let folders: Vec<&crate::FilterMenuRow> = rows
            .iter()
            .filter(|row| row.parent == "gallery" && row.kind == "group")
            .collect();
        assert_eq!(
            folders
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Artistic",
                "Brush Strokes",
                "Distort",
                "Sketch",
                "Stylize",
                "Texture",
            ],
            "the gallery's folders stay in Photoshop's order"
        );
        assert!(
            folders
                .iter()
                .all(|folder| folder.group.starts_with("gallery/")),
            "each folder names its own flyout"
        );

        // Photoshop's Video flyout holds the two engine video filters.
        assert_eq!(
            rows.iter()
                .filter(|row| row.parent == "video" && row.kind == "item")
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["de-interlace", "ntsc-colors"],
            "Video opens with its own filters"
        );

        // Every flyout is reachable: each non-top-level row hangs off a group
        // row that opens exactly that flyout, and every group has items.
        let flyouts: Vec<&str> = rows
            .iter()
            .filter(|row| !row.parent.is_empty())
            .map(|row| row.parent.as_str())
            .collect();
        for row in rows.iter().filter(|row| !row.parent.is_empty()) {
            assert!(
                flyouts.contains(&row.parent.as_str()),
                "{} is inside a known flyout",
                row.label
            );
        }
        let groups: Vec<&str> = rows
            .iter()
            .filter(|row| row.kind == "group")
            .map(|row| row.group.as_str())
            .collect();
        for row in rows.iter().filter(|row| !row.parent.is_empty()) {
            assert!(
                groups.contains(&row.parent.as_str()),
                "{} belongs to a flyout some group row opens",
                row.label
            );
        }
        for group in groups.iter().filter(|group| **group != GALLERY_GROUP) {
            assert!(
                rows.iter()
                    .any(|row| row.parent == group && row.kind != "group"),
                "{group} holds at least one filter"
            );
        }
        assert!(
            rows.iter()
                .any(|row| row.parent == GALLERY_GROUP && row.kind == "group"),
            "the gallery flyout holds its folders"
        );

        // Disabled entries carry their reason; every id resolves.
        for row in rows.iter().filter(|row| row.kind == "item") {
            let choice = FilterChoice::from_id(&row.id)
                .ok_or_else(|| format!("{} is unknown to filter_from_id", row.id))?;
            assert_eq!(row.enabled, choice.supported(), "{} enabled state", row.id);
            if row.enabled {
                assert!(row.note.is_empty(), "{} has no disabled note", row.id);
            } else {
                assert!(
                    !row.note.is_empty(),
                    "{} explains why it is disabled",
                    row.id
                );
            }
        }
        let displace = rows
            .iter()
            .find(|row| row.id == "displace")
            .ok_or("displace is listed")?;
        assert!(!displace.enabled, "displace is disabled");
        assert!(
            displace.note.contains("displacement map"),
            "the reason is shown"
        );
        Ok(())
    }

    #[test]
    fn unsupported_filters_do_not_open() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = filter_board_app()?;
        let error = app
            .image_filter_open_core("displace")
            .err()
            .ok_or("error")?;
        assert!(error.contains("displacement map"), "the reason is reported");
        assert!(app.image.filter_dialog.is_none());
        Ok(())
    }
}
