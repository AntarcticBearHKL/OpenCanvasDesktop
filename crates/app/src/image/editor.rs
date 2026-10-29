//! Adjustment-layer creation + the per-type parameter editors (`ps-adjustments-panel.tsx`,
//! `ps-adjustment-editor.tsx`, `ps-param-editors.tsx`) and the layer-style (fx)
//! parameter editing that shares the same row model (`ps-fx-panel.tsx`).
//! Owns the adjustment/style editor rows and their parameter edits.

use open_canvas_ps::adjustments::{ps_adjustment_defaults, PS_ADJUSTMENT_TYPES};
use open_canvas_ps::doc::PsParams;
use open_canvas_ps::layer_ops::patch_ps_layer_style;
use open_canvas_ps::layer_styles::ps_layer_style_defaults;
use open_canvas_ps::{
    add_ps_layer_above, create_ps_adjustment_layer, layer_ops::find_ps_layer, patch_ps_layer,
    CanvasPsAdjustmentType, CanvasPsLayerStyle, CanvasPsParamValue,
};

use crate::app::App;
use crate::image::{EditorTarget, ImageState};
use crate::vm;
use crate::AppWindow;

/// Display label + kebab id per adjustment type (the panel grid).
pub const ADJUSTMENT_LABELS: [(&str, &str); 15] = [
    ("brightness-contrast", "Brightness/Contrast"),
    ("levels", "Levels"),
    ("curves", "Curves"),
    ("exposure", "Exposure"),
    ("vibrance", "Vibrance"),
    ("hue-saturation", "Hue/Saturation"),
    ("color-balance", "Color Balance"),
    ("black-white", "Black & White"),
    ("photo-filter", "Photo Filter"),
    ("channel-mixer", "Channel Mixer"),
    ("gradient-map", "Gradient Map"),
    ("invert", "Invert"),
    ("posterize", "Posterize"),
    ("threshold", "Threshold"),
    ("selective-color", "Selective Color"),
];

/// Kebab id -> adjustment enum (the panel grid + serde names).
pub fn adjustment_from_id(id: &str) -> Option<CanvasPsAdjustmentType> {
    ADJUSTMENT_LABELS
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, label)| label)
        .and_then(|_| {
            PS_ADJUSTMENT_TYPES
                .iter()
                .copied()
                .find(|kind| crate::image::render::adjustment_id(*kind) == id)
        })
}

/// Label of an adjustment type (panel + editor titles).
pub fn adjustment_label(kind: CanvasPsAdjustmentType) -> &'static str {
    let id = crate::image::render::adjustment_id(kind);
    ADJUSTMENT_LABELS
        .iter()
        .find(|(key, _)| *key == id)
        .map_or("Adjustment", |(_, label)| *label)
}

/// One editor row (`ImageEditorRow`): `kind` picks the widget the panel renders.
pub struct EditorRow {
    pub key: String,
    pub label: String,
    pub kind: &'static str,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub text: String,
    pub options: Vec<String>,
    pub checked: bool,
}

impl EditorRow {
    pub(crate) fn number(key: &str, label: &str, value: f64, min: f64, max: f64) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind: "number",
            value,
            min,
            max,
            text: String::new(),
            options: Vec::new(),
            checked: false,
        }
    }

    pub(crate) fn select(key: &str, label: &str, options: &[&str], current: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind: "select",
            value: 0.0,
            min: 0.0,
            max: 0.0,
            text: current.to_string(),
            options: options.iter().map(|option| (*option).to_string()).collect(),
            checked: false,
        }
    }

    pub(crate) fn check(key: &str, label: &str, checked: bool) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind: "check",
            value: if checked { 1.0 } else { 0.0 },
            min: 0.0,
            max: 1.0,
            text: String::new(),
            options: Vec::new(),
            checked,
        }
    }

    pub(crate) fn color(key: &str, label: &str, hex: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind: "color",
            value: 0.0,
            min: 0.0,
            max: 0.0,
            text: hex.to_string(),
            options: Vec::new(),
            checked: false,
        }
    }
}

fn num(params: &PsParams, key: &str, fallback: f64) -> f64 {
    match params.get(key) {
        Some(CanvasPsParamValue::Number(value)) => *value,
        _ => fallback,
    }
}

fn text_param(params: &PsParams, key: &str, fallback: &str) -> String {
    match params.get(key) {
        Some(CanvasPsParamValue::Text(value)) => value.clone(),
        _ => fallback.to_string(),
    }
}

fn numbers(params: &PsParams, key: &str, len: usize) -> Vec<f64> {
    match params.get(key) {
        Some(CanvasPsParamValue::Numbers(values)) => values.clone(),
        _ => vec![0.0; len],
    }
}

fn texts(params: &PsParams, key: &str) -> Vec<String> {
    match params.get(key) {
        Some(CanvasPsParamValue::Texts(values)) => values.clone(),
        _ => Vec::new(),
    }
}

/// `PS_ADJUSTMENT_NAME_KEYS` order: one row list per type
/// (`ps-adjustment-editor.tsx`).
#[expect(
    clippy::too_many_lines,
    reason = "one flat row table per adjustment type, mirroring the web editor's dispatch"
)]
pub fn adjustment_rows(kind: CanvasPsAdjustmentType, params: &PsParams) -> Vec<EditorRow> {
    use CanvasPsAdjustmentType as Adjustment;
    let mut rows: Vec<EditorRow> = Vec::new();
    match kind {
        Adjustment::BrightnessContrast => {
            rows.push(EditorRow::number(
                "brightness",
                "Brightness",
                num(params, "brightness", 0.0),
                -150.0,
                150.0,
            ));
            rows.push(EditorRow::number(
                "contrast",
                "Contrast",
                num(params, "contrast", 0.0),
                -100.0,
                100.0,
            ));
        }
        Adjustment::Levels => {
            rows.push(EditorRow::select(
                "channel",
                "Channel",
                &["rgb", "red", "green", "blue"],
                &text_param(params, "channel", "rgb"),
            ));
            rows.push(EditorRow::number(
                "inBlack",
                "In Black",
                num(params, "inBlack", 0.0),
                0.0,
                254.0,
            ));
            rows.push(EditorRow::number(
                "inWhite",
                "In White",
                num(params, "inWhite", 255.0),
                1.0,
                255.0,
            ));
            rows.push(EditorRow::number(
                "gamma",
                "Gamma",
                num(params, "gamma", 1.0),
                0.1,
                9.99,
            ));
            rows.push(EditorRow::number(
                "outBlack",
                "Out Black",
                num(params, "outBlack", 0.0),
                0.0,
                255.0,
            ));
            rows.push(EditorRow::number(
                "outWhite",
                "Out White",
                num(params, "outWhite", 255.0),
                0.0,
                255.0,
            ));
        }
        Adjustment::Curves => {
            rows.push(EditorRow::select(
                "channel",
                "Channel",
                &["rgb", "red", "green", "blue"],
                &text_param(params, "channel", "rgb"),
            ));
            rows.push(EditorRow {
                key: "points".to_string(),
                label: "Curve".to_string(),
                kind: "curve",
                value: 0.0,
                min: 0.0,
                max: 255.0,
                text: String::new(),
                options: Vec::new(),
                checked: false,
            });
        }
        Adjustment::Exposure => {
            rows.push(EditorRow::number(
                "exposure",
                "Exposure",
                num(params, "exposure", 0.0),
                -5.0,
                5.0,
            ));
            rows.push(EditorRow::number(
                "offset",
                "Offset",
                num(params, "offset", 0.0),
                -0.5,
                0.5,
            ));
            rows.push(EditorRow::number(
                "gamma",
                "Gamma",
                num(params, "gamma", 1.0),
                0.1,
                9.99,
            ));
        }
        Adjustment::Vibrance => {
            rows.push(EditorRow::number(
                "vibrance",
                "Vibrance",
                num(params, "vibrance", 0.0),
                -100.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "saturation",
                "Saturation",
                num(params, "saturation", 0.0),
                -100.0,
                100.0,
            ));
        }
        Adjustment::HueSaturation => {
            rows.push(EditorRow::select(
                "channel",
                "Channel",
                &[
                    "master", "reds", "yellows", "greens", "cyans", "blues", "magentas",
                ],
                &text_param(params, "channel", "master"),
            ));
            rows.push(EditorRow::number(
                "hue",
                "Hue",
                num(params, "hue", 0.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::number(
                "saturation",
                "Saturation",
                num(params, "saturation", 0.0),
                -100.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "lightness",
                "Lightness",
                num(params, "lightness", 0.0),
                -100.0,
                100.0,
            ));
        }
        Adjustment::ColorBalance => {
            rows.push(EditorRow::select(
                "tone",
                "Tone",
                &["shadows", "midtones", "highlights"],
                &text_param(params, "tone", "midtones"),
            ));
            let tone = text_param(params, "tone", "midtones");
            let values = numbers(params, &tone, 3);
            for (index, channel) in ["Red", "Green", "Blue"].iter().enumerate() {
                rows.push(EditorRow::number(
                    &format!("{tone}:{index}"),
                    channel,
                    values.get(index).copied().unwrap_or(0.0),
                    -100.0,
                    100.0,
                ));
            }
            rows.push(EditorRow::check(
                "preserve",
                "Preserve Luminosity",
                num(params, "preserve", 1.0) >= 0.5,
            ));
        }
        Adjustment::BlackWhite => {
            for key in ["reds", "yellows", "greens", "cyans", "blues", "magentas"] {
                let mut label = key.to_string();
                if let Some(first) = label.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                rows.push(EditorRow::number(
                    key,
                    &label,
                    num(params, key, 40.0),
                    -200.0,
                    300.0,
                ));
            }
        }
        Adjustment::PhotoFilter => {
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#ec8a00"),
            ));
            rows.push(EditorRow::number(
                "density",
                "Density",
                num(params, "density", 25.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::check(
                "preserve",
                "Preserve Luminosity",
                num(params, "preserve", 1.0) >= 0.5,
            ));
        }
        Adjustment::ChannelMixer => {
            rows.push(EditorRow::select(
                "output",
                "Output",
                &["red", "green", "blue"],
                &text_param(params, "output", "red"),
            ));
            rows.push(EditorRow::check(
                "monochrome",
                "Monochrome",
                num(params, "monochrome", 0.0) >= 0.5,
            ));
            let values = numbers(params, "values", 12);
            for (index, label) in CHANNEL_MIXER_LABELS.iter().enumerate() {
                rows.push(EditorRow::number(
                    &format!("values:{index}"),
                    label,
                    values.get(index).copied().unwrap_or(0.0),
                    -200.0,
                    200.0,
                ));
            }
        }
        Adjustment::GradientMap => {
            rows.push(EditorRow {
                key: "stops".to_string(),
                label: "Stops".to_string(),
                kind: "stops",
                value: 0.0,
                min: 0.0,
                max: 1.0,
                text: String::new(),
                options: Vec::new(),
                checked: false,
            });
            rows.push(EditorRow::check(
                "reverse",
                "Reverse",
                num(params, "reverse", 0.0) >= 0.5,
            ));
        }
        Adjustment::Invert => {
            rows.push(EditorRow::select(
                "channel",
                "Channel",
                &["rgb", "red", "green", "blue"],
                &text_param(params, "channel", "rgb"),
            ));
        }
        Adjustment::Posterize => {
            rows.push(EditorRow::number(
                "levels",
                "Levels",
                num(params, "levels", 4.0),
                2.0,
                255.0,
            ));
        }
        Adjustment::Threshold => {
            rows.push(EditorRow::number(
                "level",
                "Level",
                num(params, "level", 128.0),
                1.0,
                255.0,
            ));
        }
        Adjustment::SelectiveColor => {
            let range = num(params, "range", 0.0).round().clamp(0.0, 8.0);
            let range_name = SELECTIVE_RANGES
                .get(vm::i2u(vm::f2i(range)))
                .copied()
                .unwrap_or("reds");
            rows.push(EditorRow::select(
                "range",
                "Colors",
                &SELECTIVE_RANGES,
                range_name,
            ));
            rows.push(EditorRow::select(
                "method",
                "Method",
                &["relative", "absolute"],
                &text_param(params, "method", "relative"),
            ));
            let values = numbers(params, "values", 36);
            let base = vm::f2i(range) * 4;
            for (slot, label) in ["Cyan", "Magenta", "Yellow", "Black"].iter().enumerate() {
                let index = vm::i2u(base + i64::try_from(slot).unwrap_or(0));
                rows.push(EditorRow::number(
                    &format!("values:{index}"),
                    label,
                    values.get(index).copied().unwrap_or(0.0),
                    -100.0,
                    100.0,
                ));
            }
        }
    }
    rows
}

/// The channel-mixer row labels (output x source).
pub const CHANNEL_MIXER_LABELS: [&str; 12] = [
    "R from R",
    "R from G",
    "R from B",
    "R constant",
    "G from R",
    "G from G",
    "G from B",
    "G constant",
    "B from R",
    "B from G",
    "B from B",
    "B constant",
];

/// The nine selective-colour ranges (`ps-adjustments.ts` order).
pub const SELECTIVE_RANGES: [&str; 9] = [
    "reds", "yellows", "greens", "cyans", "blues", "magentas", "whites", "neutrals", "blacks",
];

/// The fx panel's add chips (`ps-fx-panel.tsx`): kebab id + label.
pub const FX_STYLE_LABELS: [(&str, &str); 10] = [
    ("stroke", "Stroke"),
    ("drop-shadow", "Drop Shadow"),
    ("inner-shadow", "Inner Shadow"),
    ("outer-glow", "Outer Glow"),
    ("inner-glow", "Inner Glow"),
    ("bevel", "Bevel"),
    ("satin", "Satin"),
    ("color-overlay", "Color Overlay"),
    ("gradient-overlay", "Gradient Overlay"),
    ("pattern-overlay", "Pattern Overlay"),
];

/// Label of one fx style (`FX_STYLE_LABELS`, keyed by the kebab id).
pub fn style_label(kind: open_canvas_ps::CanvasPsLayerStyleType) -> &'static str {
    let id = crate::image::actions::layer_style_id(kind);
    FX_STYLE_LABELS
        .iter()
        .find(|(key, _)| *key == id)
        .map_or("Layer Style", |(_, label)| *label)
}

/// One layer-style parameter row list (`PS_LAYER_STYLE_DEFAULTS` keys plus the
/// web's `STYLE_CONTROLS` order). `patterns` are the saved pattern presets the
/// `PatternOverlay`'s `patternKey` select offers (name shown, storage key
/// stored).
#[expect(
    clippy::too_many_lines,
    reason = "the style panel's control rows are a flat transcribed table; splitting scatters one lookup order"
)]
pub fn style_rows(
    style: &CanvasPsLayerStyle,
    patterns: &[crate::image::assets::PsPatternPreset],
) -> Vec<EditorRow> {
    use open_canvas_ps::CanvasPsLayerStyleType as Style;
    let params = &style.params;
    let blend = |rows: &mut Vec<EditorRow>| {
        rows.push(EditorRow::select(
            "blendMode",
            "Blend",
            &open_canvas_ps::BLEND_MODE_IDS,
            &text_param(params, "blendMode", "normal"),
        ));
    };
    let mut rows: Vec<EditorRow> = Vec::new();
    match style.style_type {
        Style::Stroke => {
            rows.push(EditorRow::number(
                "size",
                "Size",
                num(params, "size", 3.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::select(
                "position",
                "Position",
                &["outside", "inside", "center"],
                &text_param(params, "position", "outside"),
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 100.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#000000"),
            ));
            blend(&mut rows);
        }
        Style::DropShadow | Style::InnerShadow => {
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#000000"),
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 75.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "angle",
                "Angle",
                num(params, "angle", 120.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::number(
                "distance",
                "Distance",
                num(params, "distance", 5.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "spread",
                "Spread",
                num(params, "spread", 0.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "size",
                "Size",
                num(params, "size", 5.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
        Style::OuterGlow | Style::InnerGlow => {
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#ffffbe"),
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 75.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "size",
                "Size",
                num(params, "size", 5.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "spread",
                "Spread",
                num(params, "spread", 0.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
        Style::Bevel => {
            rows.push(EditorRow::select(
                "style",
                "Style",
                &BEVEL_STYLES,
                &text_param(params, "style", "inner"),
            ));
            rows.push(EditorRow::select(
                "direction",
                "Direction",
                &BEVEL_DIRECTIONS,
                &text_param(params, "direction", "up"),
            ));
            rows.push(EditorRow::number(
                "depth",
                "Depth",
                num(params, "depth", 100.0),
                0.0,
                1000.0,
            ));
            rows.push(EditorRow::number(
                "size",
                "Size",
                num(params, "size", 5.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "soften",
                "Soften",
                num(params, "soften", 0.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "angle",
                "Angle",
                num(params, "angle", 120.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::color(
                "highlightColor",
                "Highlight",
                &text_param(params, "highlightColor", "#ffffff"),
            ));
            rows.push(EditorRow::number(
                "highlightOpacity",
                "Hi Opacity",
                num(params, "highlightOpacity", 75.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::color(
                "shadowColor",
                "Shadow",
                &text_param(params, "shadowColor", "#000000"),
            ));
            rows.push(EditorRow::number(
                "shadowOpacity",
                "Sh Opacity",
                num(params, "shadowOpacity", 75.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
        Style::Satin => {
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#000000"),
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 50.0),
                0.0,
                100.0,
            ));
            rows.push(EditorRow::number(
                "angle",
                "Angle",
                num(params, "angle", 19.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::number(
                "distance",
                "Distance",
                num(params, "distance", 11.0),
                0.0,
                200.0,
            ));
            rows.push(EditorRow::number(
                "size",
                "Size",
                num(params, "size", 14.0),
                0.0,
                200.0,
            ));
            blend(&mut rows);
        }
        Style::ColorOverlay => {
            rows.push(EditorRow::color(
                "color",
                "Color",
                &text_param(params, "color", "#ff0000"),
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 100.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
        Style::GradientOverlay => {
            rows.push(EditorRow {
                key: "stops".to_string(),
                label: "Stops".to_string(),
                kind: "stops",
                value: 0.0,
                min: 0.0,
                max: 1.0,
                text: String::new(),
                options: Vec::new(),
                checked: false,
            });
            rows.push(EditorRow::number(
                "angle",
                "Angle",
                num(params, "angle", 90.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::number(
                "scale",
                "Scale",
                num(params, "scale", 100.0),
                1.0,
                500.0,
            ));
            rows.push(EditorRow::check(
                "reverse",
                "Reverse",
                num(params, "reverse", 0.0) >= 0.5,
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 100.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
        Style::PatternOverlay => {
            rows.push(EditorRow::select(
                "pattern",
                "Pattern",
                &open_canvas_ps::layer_styles::PS_PATTERN_KINDS,
                &text_param(params, "pattern", "checker"),
            ));
            rows.push(saved_pattern_row(params, patterns));
            rows.push(EditorRow::number(
                "scale",
                "Scale",
                num(params, "scale", 100.0),
                1.0,
                500.0,
            ));
            rows.push(EditorRow::number(
                "angle",
                "Angle",
                num(params, "angle", 0.0),
                -180.0,
                180.0,
            ));
            rows.push(EditorRow::number(
                "opacity",
                "Opacity",
                num(params, "opacity", 100.0),
                0.0,
                100.0,
            ));
            blend(&mut rows);
        }
    }
    rows
}

/// The editor colour-row keys whose text must be normalised to a hex colour
/// (the fx panel's `color` plus the Bevel highlight/shadow colours).
pub fn is_editor_color_key(key: &str) -> bool {
    matches!(key, "color" | "highlightColor" | "shadowColor")
}

/// The Bevel `style` select options (`ps-fx-panel.tsx` `STYLE_CONTROLS.bevel`).
pub const BEVEL_STYLES: [&str; 3] = ["inner", "emboss", "outer"];
/// The Bevel `direction` select options.
pub const BEVEL_DIRECTIONS: [&str; 2] = ["up", "down"];

/// The `PatternOverlay` `patternKey` select row: saved preset names, with
/// "None" first, showing the preset whose storage key the style points at.
fn saved_pattern_row(
    params: &PsParams,
    patterns: &[crate::image::assets::PsPatternPreset],
) -> EditorRow {
    let names = saved_pattern_names(patterns);
    let current = saved_pattern_name(patterns, &text_param(params, "patternKey", ""));
    EditorRow::select("pattern-key", "Saved", &names, &current)
}

/// The saved pattern preset names, with "None" first (the `patternKey` select).
fn saved_pattern_names(patterns: &[crate::image::assets::PsPatternPreset]) -> Vec<&str> {
    let mut names = vec!["None"];
    names.extend(patterns.iter().map(|preset| preset.name.as_str()));
    names
}

/// The display name of the preset whose storage key is `key` ("None" when unset).
fn saved_pattern_name(patterns: &[crate::image::assets::PsPatternPreset], key: &str) -> String {
    patterns
        .iter()
        .find(|preset| preset.key == key)
        .map_or_else(|| "None".to_string(), |preset| preset.name.clone())
}

/// The curve's control points as `(x, y)` pairs sorted by x (`psSortPairs`).
pub fn curve_points(params: &PsParams) -> Vec<(f64, f64)> {
    let flat = numbers(params, "points", 4);
    let mut points: Vec<(f64, f64)> = flat
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0].clamp(0.0, 255.0), pair[1].clamp(0.0, 255.0)))
        .collect();
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if points.len() < 2 {
        points = vec![(0.0, 0.0), (255.0, 255.0)];
    }
    points
}

/// A 126px curve path through the point list (a straight polyline of the pairs).
pub fn curve_path(points: &[(f64, f64)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (index, (x, y)) in points.iter().enumerate() {
        let px = x.clamp(0.0, 255.0) / 255.0 * 126.0;
        let py = (1.0 - y.clamp(0.0, 255.0) / 255.0) * 126.0;
        let command = if index == 0 { "M" } else { "L" };
        if write!(out, "{command} {px:.2} {py:.2} ").is_err() {
            return out;
        }
    }
    out
}

/// Parses the `"#hex@pos"` stop strings (`psParseGradientStops`).
pub fn editor_stops(params: &PsParams) -> Vec<(String, f64)> {
    let strings = texts(params, "stops");
    open_canvas_ps::adjustments::ps_parse_gradient_stops(&strings)
}

impl ImageState {
    /// The params the editor currently addresses: the active adjustment layer's
    /// or the picked style's.
    pub fn editor_params(&self) -> Option<PsParams> {
        let layer = find_ps_layer(&self.project.board_layers, &self.active_layer_id())?;
        match &self.editor_target {
            EditorTarget::Adjustment => layer
                .adjustment_params
                .clone()
                .or_else(|| layer.adjustment.map(ps_adjustment_defaults)),
            EditorTarget::Style(id) => layer
                .styles
                .as_ref()
                .and_then(|styles| styles.iter().find(|style| style.id == *id))
                .map(|style| style.params.clone())
                .or_else(|| {
                    layer
                        .styles
                        .as_ref()
                        .and_then(|styles| styles.first())
                        .map(|style| style.params.clone())
                }),
        }
    }

    /// The adjustment type the editor addresses (adjustment target only).
    pub fn editor_adjustment(&self) -> Option<CanvasPsAdjustmentType> {
        if !matches!(self.editor_target, EditorTarget::Adjustment) {
            return None;
        }
        find_ps_layer(&self.project.board_layers, &self.active_layer_id())
            .and_then(|layer| layer.adjustment)
    }

    /// The style the editor addresses (fx target only).
    pub fn editor_style(&self) -> Option<CanvasPsLayerStyle> {
        let EditorTarget::Style(id) = &self.editor_target else {
            return None;
        };
        find_ps_layer(&self.project.board_layers, &self.active_layer_id())
            .and_then(|layer| layer.styles.clone())
            .and_then(|styles| styles.into_iter().find(|style| style.id == *id))
    }

    /// The editor's row model (`adjustment_rows` / `style_rows`).
    pub fn editor_rows(&self) -> Vec<EditorRow> {
        match &self.editor_target {
            EditorTarget::Adjustment => match (self.editor_adjustment(), self.editor_params()) {
                (Some(kind), Some(params)) => adjustment_rows(kind, &params),
                _ => Vec::new(),
            },
            EditorTarget::Style(_) => self
                .editor_style()
                .map(|style| style_rows(&style, &self.assets.patterns))
                .unwrap_or_default(),
        }
    }
}

impl App {
    /// Adjustments panel click (`ps-adjustments-panel.tsx` `pick`): replaces the
    /// selected adjustment layer's type or creates a new layer above it.
    pub fn image_adjustment_create(&mut self, ui: &AppWindow, id: &str) {
        self.image_adjustment_create_core(id);
        self.sync_image(ui);
    }

    /// The adjustment pick without a window (unit tests drive this directly).
    pub fn image_adjustment_create_core(&mut self, id: &str) {
        let Some(kind) = adjustment_from_id(id) else {
            return;
        };
        let label = adjustment_label(kind);
        let active = self.image.active_layer_id();
        let selected = find_ps_layer(&self.image.project.board_layers, &active);
        let is_adjustment = selected
            .is_some_and(|layer| layer.kind == open_canvas_ps::CanvasPsLayerKind::Adjustment);
        if is_adjustment {
            let params = ps_adjustment_defaults(kind);
            self.image.project.board_layers =
                patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                    layer.adjustment = Some(kind);
                    layer.adjustment_params = Some(params.clone());
                    layer.name = label.to_string();
                });
        } else {
            let board = self.image.board();
            let mut gen = self.image_gen("ps-adjustment");
            let layer = create_ps_adjustment_layer(&mut gen, board, kind, label);
            let id = layer.id.clone();
            self.image.project.board_layers =
                add_ps_layer_above(&self.image.project.board_layers, layer, Some(&active));
            self.image.active_layer = id;
        }
        self.image.editor_target = EditorTarget::Adjustment;
        self.image.editor_stop = 0;
        self.commit_image_core("Adjustment");
    }

    /// One editor slider: number params, including `array:index` keys.
    pub fn image_editor_set(&mut self, ui: &AppWindow, key: &str, value: f64) {
        self.image_editor_set_core(key, value);
        self.sync_image(ui);
    }

    /// The editor slider without a window (unit tests drive this directly).
    pub fn image_editor_set_core(&mut self, key: &str, value: f64) {
        self.patch_editor_params(|params| {
            if let Some((array, index)) = key.split_once(':') {
                if let Some(CanvasPsParamValue::Numbers(values)) = params.get_mut(array) {
                    if let Ok(index) = index.parse::<usize>() {
                        if let Some(slot) = values.get_mut(index) {
                            *slot = value;
                        }
                    }
                }
                return;
            }
            if let Some(CanvasPsParamValue::Number(slot)) = params.get_mut(key) {
                *slot = value;
            }
        });
        self.commit_image_core("Adjustment Parameter");
    }

    /// Editor select rows (`channel`, `tone`, `range`, `method`, `pattern`).
    pub fn image_editor_option(&mut self, ui: &AppWindow, key: &str, option: &str) {
        self.image_editor_option_core(key, option);
        self.sync_image(ui);
    }

    /// The editor select without a window (unit tests drive this directly).
    pub fn image_editor_option_core(&mut self, key: &str, option: &str) {
        // The PatternOverlay `patternKey` select shows preset names but stores
        // the preset's bitmap storage key ("None" clears it), matching the web's
        // saved-pattern control. Its widget row key is kebab (`pattern-key`)
        // while the style param is camel (`patternKey`).
        let (param_key, option) = if key == "pattern-key" {
            let stored = if option == "None" {
                String::new()
            } else {
                self.image
                    .assets
                    .patterns
                    .iter()
                    .find(|preset| preset.name == option)
                    .map_or_else(String::new, |preset| preset.key.clone())
            };
            ("patternKey", stored)
        } else {
            (key, option.to_string())
        };
        self.patch_editor_params(|params| {
            params.insert(
                param_key.to_string(),
                CanvasPsParamValue::Text(option.clone()),
            );
        });
        self.commit_image_core("Adjustment Parameter");
    }

    /// Editor text/colour rows (`color`, plus free text values).
    pub fn image_editor_text(&mut self, ui: &AppWindow, key: &str, text: &str) {
        self.image_editor_text_core(key, text);
        self.sync_image(ui);
    }

    /// The editor text field without a window (unit tests drive this directly).
    pub fn image_editor_text_core(&mut self, key: &str, text: &str) {
        let value = if is_editor_color_key(key) {
            let hex = open_canvas_ps::color::normalize_hex(text);
            if hex.is_empty() {
                return;
            }
            hex
        } else {
            text.to_string()
        };
        self.patch_editor_params(|params| {
            params.insert(key.to_string(), CanvasPsParamValue::Text(value.clone()));
        });
        self.commit_image_core("Adjustment Parameter");
    }

    /// Editor checkboxes (`preserve`, `monochrome`, `reverse`, ...).
    pub fn image_editor_check(&mut self, ui: &AppWindow, key: &str, checked: bool) {
        self.image_editor_check_core(key, checked);
        self.sync_image(ui);
    }

    /// The editor checkbox without a window (unit tests drive this directly).
    pub fn image_editor_check_core(&mut self, key: &str, checked: bool) {
        self.patch_editor_params(|params| {
            params.insert(
                key.to_string(),
                CanvasPsParamValue::Number(if checked { 1.0 } else { 0.0 }),
            );
        });
        self.commit_image_core("Adjustment Parameter");
    }

    /// `PsCurveEditor` click: appends a control point (min 2 points kept).
    pub fn image_curve_add(&mut self, ui: &AppWindow, x: f64, y: f64) {
        self.image_curve_add_core(x, y);
        self.sync_image(ui);
    }

    /// The curve add without a window (unit tests drive this directly).
    pub fn image_curve_add_core(&mut self, x: f64, y: f64) {
        self.patch_editor_params(|params| {
            let mut points = curve_points(params);
            points.push((x.clamp(0.0, 255.0), y.clamp(0.0, 255.0)));
            points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let flat: Vec<f64> = points.into_iter().flat_map(|(x, y)| [x, y]).collect();
            params.insert("points".to_string(), CanvasPsParamValue::Numbers(flat));
        });
        self.commit_image_core("Edit Curve");
    }

    /// `PsCurveEditor` drag: moves one control point (x re-sorts).
    pub fn image_curve_move(&mut self, ui: &AppWindow, index: usize, x: f64, y: f64) {
        self.image_curve_move_core(index, x, y);
        self.sync_image(ui);
    }

    /// The curve move without a window (unit tests drive this directly).
    pub fn image_curve_move_core(&mut self, index: usize, x: f64, y: f64) {
        self.patch_editor_params(|params| {
            let mut points = curve_points(params);
            if let Some(point) = points.get_mut(index) {
                *point = (x.clamp(0.0, 255.0), y.clamp(0.0, 255.0));
            }
            points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let flat: Vec<f64> = points.into_iter().flat_map(|(x, y)| [x, y]).collect();
            params.insert("points".to_string(), CanvasPsParamValue::Numbers(flat));
        });
        self.commit_image_core("Edit Curve");
    }

    /// `PsCurveEditor` double-click: removes a control point (min 2 kept).
    pub fn image_curve_delete(&mut self, ui: &AppWindow, index: usize) {
        self.image_curve_delete_core(index);
        self.sync_image(ui);
    }

    /// The curve delete without a window (unit tests drive this directly).
    pub fn image_curve_delete_core(&mut self, index: usize) {
        self.patch_editor_params(|params| {
            let mut points = curve_points(params);
            if points.len() > 2 && index < points.len() {
                points.remove(index);
            }
            let flat: Vec<f64> = points.into_iter().flat_map(|(x, y)| [x, y]).collect();
            params.insert("points".to_string(), CanvasPsParamValue::Numbers(flat));
        });
        self.commit_image_core("Edit Curve");
    }

    /// `PsGradientStopsEditor`: adds a stop halfway from the picked stop's colour.
    pub fn image_editor_stop_add(&mut self, ui: &AppWindow) {
        self.image_editor_stop_add_core();
        self.sync_image(ui);
    }

    /// The stop add without a window (unit tests drive this directly).
    pub fn image_editor_stop_add_core(&mut self) {
        let hex = self.image.color.clone();
        self.patch_editor_params(|params| {
            let mut stops = editor_stops(params);
            stops.push((hex.clone(), 0.5));
            stops.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let strings: Vec<String> = stops
                .into_iter()
                .map(|(color, position)| {
                    open_canvas_ps::adjustments::ps_format_gradient_stop(&color, position)
                })
                .collect();
            params.insert("stops".to_string(), CanvasPsParamValue::Texts(strings));
        });
        // The editor selects the stop it just added (stops sort by position).
        let selected = self.image.editor_params().map_or(0, |params| {
            editor_stops(&params)
                .iter()
                .position(|(color, position)| color == &hex && (*position - 0.5).abs() < 1e-9)
                .unwrap_or(0)
        });
        self.image.editor_stop = selected;
        self.commit_image_core("Edit Gradient Stops");
    }

    /// Picks the stop the colour/position rows address.
    pub fn image_editor_stop_select(&mut self, ui: &AppWindow, index: usize) {
        self.image.editor_stop = index;
        self.sync_image(ui);
    }

    /// Double-click delete (min 2 stops kept).
    pub fn image_editor_stop_remove(&mut self, ui: &AppWindow, index: usize) {
        self.image_editor_stop_remove_core(index);
        self.sync_image(ui);
    }

    /// The stop remove without a window (unit tests drive this directly).
    pub fn image_editor_stop_remove_core(&mut self, index: usize) {
        self.patch_editor_params(|params| {
            let mut stops = editor_stops(params);
            if stops.len() > 2 && index < stops.len() {
                stops.remove(index);
            }
            let strings: Vec<String> = stops
                .into_iter()
                .map(|(color, position)| {
                    open_canvas_ps::adjustments::ps_format_gradient_stop(&color, position)
                })
                .collect();
            params.insert("stops".to_string(), CanvasPsParamValue::Texts(strings));
        });
        let count = self
            .image
            .editor_params()
            .map_or(0, |params| editor_stops(&params).len());
        self.image.editor_stop = self.image.editor_stop.min(count.saturating_sub(1));
        self.commit_image_core("Edit Gradient Stops");
    }

    /// Stop colour: the picked stop takes `hex` (empty = the foreground).
    pub fn image_editor_stop_color(&mut self, ui: &AppWindow, index: usize, hex: &str) {
        self.image_editor_stop_color_core(index, hex);
        self.sync_image(ui);
    }

    /// The stop colour without a window (unit tests drive this directly).
    pub fn image_editor_stop_color_core(&mut self, index: usize, hex: &str) {
        let color = if hex.is_empty() {
            self.image.color.clone()
        } else {
            open_canvas_ps::color::normalize_hex(hex)
        };
        if color.is_empty() {
            return;
        }
        self.patch_editor_params(|params| {
            let mut stops = editor_stops(params);
            if let Some(stop) = stops.get_mut(index) {
                stop.0.clone_from(&color);
            }
            let strings: Vec<String> = stops
                .into_iter()
                .map(|(color, position)| {
                    open_canvas_ps::adjustments::ps_format_gradient_stop(&color, position)
                })
                .collect();
            params.insert("stops".to_string(), CanvasPsParamValue::Texts(strings));
        });
        self.commit_image_core("Edit Gradient Stops");
    }

    /// Stop position (UI percent 0..100).
    pub fn image_editor_stop_pos(&mut self, ui: &AppWindow, index: usize, value: f64) {
        self.image_editor_stop_pos_core(index, value);
        self.sync_image(ui);
    }

    /// The stop position without a window (unit tests drive this directly).
    pub fn image_editor_stop_pos_core(&mut self, index: usize, value: f64) {
        self.patch_editor_params(|params| {
            let mut stops = editor_stops(params);
            if let Some(stop) = stops.get_mut(index) {
                stop.1 = (value / 100.0).clamp(0.0, 1.0);
            }
            let strings: Vec<String> = stops
                .into_iter()
                .map(|(color, position)| {
                    open_canvas_ps::adjustments::ps_format_gradient_stop(&color, position)
                })
                .collect();
            params.insert("stops".to_string(), CanvasPsParamValue::Texts(strings));
        });
        self.commit_image_core("Edit Gradient Stops");
    }

    /// Runs `edit` on the editor target's params and writes them back.
    fn patch_editor_params(&mut self, edit: impl FnOnce(&mut PsParams)) {
        let active = self.image.active_layer_id();
        let mut params = self.image.editor_params().unwrap_or_default();
        edit(&mut params);
        match self.image.editor_target.clone() {
            EditorTarget::Adjustment => {
                self.image.project.board_layers =
                    patch_ps_layer(&self.image.project.board_layers, &active, |layer| {
                        layer.adjustment_params = Some(params.clone());
                    });
            }
            EditorTarget::Style(id) => {
                self.image.project.board_layers = patch_ps_layer_style(
                    &self.image.project.board_layers,
                    &active,
                    &id,
                    None,
                    Some(params.clone()),
                )
                .into_iter()
                .map(|layer| {
                    if layer.id != active {
                        return layer;
                    }
                    let mut layer = layer;
                    let styles = layer.styles.get_or_insert_with(Vec::new);
                    if styles.is_empty() {
                        let fallback = open_canvas_ps::CanvasPsLayerStyleType::Stroke;
                        styles.push(open_canvas_ps::create_ps_layer_style(
                            fallback,
                            "ps-style-0",
                        ));
                        if let Some(style) = styles.last_mut() {
                            style.params = ps_layer_style_defaults(fallback);
                        }
                    }
                    layer
                })
                .collect();
            }
        }
    }

    /// fx panel: opens one style's parameter editor.
    pub fn image_fx_edit_core(&mut self, style_id: &str) {
        let active = self.image.active_layer_id();
        let exists = find_ps_layer(&self.image.project.board_layers, &active)
            .and_then(|layer| layer.styles.clone())
            .is_some_and(|styles| styles.iter().any(|style| style.id == style_id));
        if exists {
            self.image.editor_target = EditorTarget::Style(style_id.to_string());
            self.image.editor_stop = 0;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::composite;
    use std::collections::HashMap;

    use open_canvas_ps::{CanvasPsLayerStyleType, IdGen, ImageProject, Surface};

    fn rich_board() -> (ImageProject, HashMap<String, Surface>) {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        project.width = 16.0;
        project.height = 16.0;
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(16, 16);
        let palette = [
            [200, 100, 50, 255],
            [40, 180, 90, 255],
            [60, 60, 200, 255],
            [30, 30, 30, 255],
            [230, 230, 230, 255],
            [128, 128, 128, 255],
        ];
        for y in 0..16 {
            for x in 0..16 {
                let slot = usize::try_from((x + y) % 6).unwrap_or(0);
                let color = palette.get(slot).copied().unwrap_or([0, 0, 0, 255]);
                surface.set(x, y, color);
            }
        }
        bitmaps.insert("image:t-1".to_string(), surface);
        (project, bitmaps)
    }

    fn editor_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let (project, bitmaps) = rich_board();
        crate::test_support::app_with_image(project, bitmaps)
    }

    /// One editor action that visibly changes the composite, per type
    /// (`ps-adjustment-editor.tsx`).
    fn tweak(app: &mut App, id: &str) {
        match id {
            "brightness-contrast" => app.image_editor_set_core("brightness", 100.0),
            "levels" => app.image_editor_set_core("inBlack", 110.0),
            "curves" => app.image_curve_add_core(128.0, 20.0),
            "exposure" => app.image_editor_set_core("exposure", 1.5),
            "vibrance" => app.image_editor_set_core("vibrance", 80.0),
            "hue-saturation" => app.image_editor_set_core("hue", 120.0),
            "color-balance" => {
                app.image_editor_option_core("tone", "midtones");
                app.image_editor_set_core("midtones:0", 80.0);
            }
            "black-white" => app.image_editor_set_core("reds", 250.0),
            "photo-filter" => app.image_editor_set_core("density", 90.0),
            "channel-mixer" => app.image_editor_set_core("values:0", 200.0),
            "gradient-map" => app.image_editor_stop_color_core(0, "#ff0000"),
            "invert" => app.image_editor_option_core("channel", "rgb"),
            "posterize" => app.image_editor_set_core("levels", 2.0),
            "threshold" => app.image_editor_set_core("level", 128.0),
            "selective-color" => {
                app.image_editor_option_core("range", "reds");
                app.image_editor_set_core("values:0", 90.0);
            }
            _ => {}
        }
    }

    #[test]
    fn every_adjustment_type_edits_through_the_ui_path_and_changes_pixels(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (id, label) in ADJUSTMENT_LABELS {
            let (mut app, _dir) = editor_board_app()?;
            let before = composite(&app);
            app.image_adjustment_create_core(id);
            let layer = app
                .image
                .project
                .board_layers
                .last()
                .cloned()
                .ok_or("adjustment layer")?;
            assert!(
                matches!(layer.kind, open_canvas_ps::CanvasPsLayerKind::Adjustment),
                "{id} creates an adjustment layer"
            );
            assert_eq!(layer.name, label, "{id} names the layer");
            assert!(
                layer.adjustment_params.is_some(),
                "{id} carries its default params"
            );
            assert_eq!(
                app.image.editor_target,
                crate::image::EditorTarget::Adjustment
            );
            let rows = app.image.editor_rows();
            assert!(!rows.is_empty(), "{id} has editor rows");

            tweak(&mut app, id);
            let after = composite(&app);
            assert!(
                after != before,
                "{id} must change the composite through the editor"
            );
        }
        Ok(())
    }

    #[test]
    fn adjustment_pick_replaces_an_existing_adjustment_layer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        app.image_adjustment_create_core("invert");
        let layers = app.image.project.board_layers.len();
        app.image_adjustment_create_core("threshold");
        assert_eq!(
            app.image.project.board_layers.len(),
            layers,
            "picking again replaces the type"
        );
        let layer = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("layer")?;
        assert_eq!(
            layer.adjustment,
            Some(open_canvas_ps::CanvasPsAdjustmentType::Threshold)
        );
        Ok(())
    }

    #[test]
    fn curve_editor_adds_moves_and_deletes_points() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        app.image_adjustment_create_core("curves");
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(curve_points(&params), vec![(0.0, 0.0), (255.0, 255.0)]);

        app.image_curve_add_core(100.0, 50.0);
        let params = app.image.editor_params().ok_or("params")?;
        let points = curve_points(&params);
        assert_eq!(points.len(), 3);
        assert_eq!(points[1], (100.0, 50.0));
        assert_ne!(curve_path(&points), "");

        app.image_curve_move_core(1, 120.0, 200.0);
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(curve_points(&params)[1], (120.0, 200.0));

        app.image_curve_delete_core(1);
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(curve_points(&params).len(), 2, "min two points kept");
        Ok(())
    }

    #[test]
    fn gradient_stops_editor_edits_the_stop_list() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        app.image_adjustment_create_core("gradient-map");
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(editor_stops(&params).len(), 2);

        app.image.color = "#00ff00".to_string();
        app.image_editor_stop_add_core();
        let params = app.image.editor_params().ok_or("params")?;
        let stops = editor_stops(&params);
        assert_eq!(stops.len(), 3);
        assert_eq!(
            stops
                .get(1)
                .map(|(color, position)| (color.as_str(), *position)),
            Some(("#00ff00", 0.5)),
            "the new stop sorts between the defaults"
        );
        assert_eq!(app.image.editor_stop, 1, "the new stop is selected");

        app.image_editor_stop_pos_core(1, 80.0);
        let params = app.image.editor_params().ok_or("params")?;
        let stops = editor_stops(&params);
        assert!((stops.get(1).map_or(0.0, |(_, position)| *position) - 0.8).abs() < 1e-9);

        app.image_editor_stop_color_core(0, "#0000ff");
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(
            editor_stops(&params).first().map(|(c, _)| c.as_str()),
            Some("#0000ff")
        );

        app.image_editor_stop_remove_core(1);
        let params = app.image.editor_params().ok_or("params")?;
        assert_eq!(editor_stops(&params).len(), 2);
        assert_eq!(app.image.editor_stop, 1, "selection clamps");
        Ok(())
    }

    #[test]
    fn fx_style_params_edit_through_the_shared_editor() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        app.image_layer_action_core("fx-add", "color-overlay");
        let layer = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("layer")?;
        let style = layer
            .styles
            .as_ref()
            .and_then(|styles| styles.first())
            .cloned()
            .ok_or("style")?;

        app.image_fx_edit_core(&style.id);
        assert_eq!(
            app.image.editor_target,
            crate::image::EditorTarget::Style(style.id.clone())
        );
        let rows = app.image.editor_rows();
        assert!(
            rows.iter().any(|row| row.key == "color"),
            "the style editor exposes its color row"
        );

        app.image_editor_text_core("color", "#00ff00");
        let layer = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("layer")?;
        let color = layer
            .styles
            .as_ref()
            .and_then(|styles| styles.first())
            .and_then(|style| style.params.get("color"))
            .cloned();
        assert_eq!(color, Some(CanvasPsParamValue::Text("#00ff00".to_string())));
        let rows = app.image.editor_rows();
        let color_row = rows
            .iter()
            .find(|row| row.key == "color")
            .ok_or("color row")?;
        assert_eq!(color_row.text, "#00ff00", "the editor reflects the edit");
        assert_eq!(app.image.history.label(2), Some("Adjustment Parameter"));
        Ok(())
    }

    /// Adds one style of `style_id` to the board's layer and opens its editor.
    fn open_style(app: &mut App, style_id: &str) -> Result<CanvasPsLayerStyle, String> {
        app.image_layer_action_core("fx-add", style_id);
        let layer = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("layer")?;
        let style = layer
            .styles
            .as_ref()
            .and_then(|styles| styles.first())
            .cloned()
            .ok_or("style")?;
        app.image_fx_edit_core(&style.id);
        Ok(style)
    }

    fn style_param(app: &App, key: &str) -> Option<CanvasPsParamValue> {
        app.image
            .project
            .board_layers
            .last()
            .and_then(|layer| layer.styles.as_ref())
            .and_then(|styles| styles.first())
            .and_then(|style| style.params.get(key))
            .cloned()
    }

    #[test]
    fn every_layer_style_exposes_a_blend_mode_row() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        for style_id in [
            "stroke",
            "drop-shadow",
            "inner-shadow",
            "outer-glow",
            "inner-glow",
            "bevel",
            "satin",
            "color-overlay",
            "gradient-overlay",
            "pattern-overlay",
        ] {
            open_style(&mut app, style_id)?;
            let rows = app.image.editor_rows();
            let blend = rows
                .iter()
                .find(|row| row.key == "blendMode")
                .ok_or_else(|| format!("{style_id} has no blendMode row"))?;
            assert_eq!(blend.kind, "select", "{style_id}");
            assert_eq!(
                blend.options.len(),
                open_canvas_ps::BLEND_MODE_IDS.len(),
                "{style_id} offers every blend mode"
            );
            assert!(
                !blend.text.is_empty(),
                "{style_id} shows the current blend mode"
            );
        }
        Ok(())
    }

    #[test]
    fn bevel_exposes_style_direction_and_highlight_shadow_colours(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        open_style(&mut app, "bevel")?;
        let rows = app.image.editor_rows();
        for (key, kind) in [
            ("style", "select"),
            ("direction", "select"),
            ("highlightColor", "color"),
            ("shadowColor", "color"),
        ] {
            let row = rows
                .iter()
                .find(|row| row.key == key)
                .ok_or_else(|| format!("missing {key}"))?;
            assert_eq!(row.kind, kind, "{key} widget");
        }
        let style_row = rows.iter().find(|row| row.key == "style").ok_or("style")?;
        assert_eq!(style_row.text, "inner");
        let names = |values: &[&str]| values.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        assert_eq!(style_row.options, names(&BEVEL_STYLES));
        let direction_row = rows
            .iter()
            .find(|row| row.key == "direction")
            .ok_or("dir")?;
        assert_eq!(direction_row.options, names(&BEVEL_DIRECTIONS));

        // The selects and colour fields write the style params.
        app.image_editor_option_core("style", "emboss");
        app.image_editor_option_core("direction", "down");
        assert_eq!(
            style_param(&app, "style"),
            Some(CanvasPsParamValue::Text("emboss".to_string()))
        );
        assert_eq!(
            style_param(&app, "direction"),
            Some(CanvasPsParamValue::Text("down".to_string()))
        );
        app.image_editor_text_core("highlightColor", "#112233");
        app.image_editor_text_core("shadowColor", "#445566");
        assert_eq!(
            style_param(&app, "highlightColor"),
            Some(CanvasPsParamValue::Text("#112233".to_string()))
        );
        assert_eq!(
            style_param(&app, "shadowColor"),
            Some(CanvasPsParamValue::Text("#445566".to_string()))
        );
        // A junk colour is ignored (silently, like the shared editor).
        app.image_editor_text_core("highlightColor", "junk");
        assert_eq!(
            style_param(&app, "highlightColor"),
            Some(CanvasPsParamValue::Text("#112233".to_string()))
        );
        Ok(())
    }

    #[test]
    fn the_bevel_direction_and_colours_reach_the_style_renderer() {
        // The layer-style compositor reads `direction` / `highlightColor` /
        // `shadowColor` / `blendMode`, so a pixel assertion is possible at that
        // level even though the board preview does not composite fx yet.
        let mut content = Surface::new(12, 12);
        for y in 3..9 {
            for x in 3..9 {
                content.set(x, y, [120, 120, 120, 255]);
            }
        }
        let render = |mut style: CanvasPsLayerStyle| -> Vec<u8> {
            style.enabled = true;
            open_canvas_ps::ps_compose_layer_styles(&content, &[style], 1.0)
                .data()
                .to_vec()
        };
        let base = open_canvas_ps::create_ps_layer_style(CanvasPsLayerStyleType::Bevel, "b");
        let up = render(base.clone());
        let mut down = base.clone();
        down.params.insert(
            "direction".to_string(),
            CanvasPsParamValue::Text("down".to_string()),
        );
        assert_ne!(up, render(down), "the direction flips the bevel light");

        let mut warm = base.clone();
        warm.params.insert(
            "highlightColor".to_string(),
            CanvasPsParamValue::Text("#ff0000".to_string()),
        );
        assert_ne!(up, render(warm), "the highlight colour tints the bevel");

        let mut dark = base.clone();
        dark.params.insert(
            "shadowColor".to_string(),
            CanvasPsParamValue::Text("#ff0000".to_string()),
        );
        assert_ne!(up, render(dark), "the shadow colour tints the bevel");

        let mut bevel_depth = base.clone();
        bevel_depth
            .params
            .insert("depth".to_string(), CanvasPsParamValue::Number(0.0));
        assert_ne!(up, render(bevel_depth), "the depth changes the bevel");
    }

    #[test]
    fn pattern_overlay_offers_the_saved_pattern_key() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = editor_board_app()?;
        let preset = crate::image::assets::PsPatternPreset {
            id: "ps-pattern-1".to_string(),
            name: "Selection 1".to_string(),
            key: "image:pattern-ps-pattern-1".to_string(),
            width: 8,
            height: 8,
        };
        app.image.assets.patterns = vec![preset.clone()];
        open_style(&mut app, "pattern-overlay")?;
        let rows = app.image.editor_rows();
        let row = rows
            .iter()
            .find(|row| row.key == "pattern-key")
            .ok_or("pattern-key row")?;
        assert_eq!(row.kind, "select");
        assert_eq!(
            row.options,
            ["None", "Selection 1"]
                .iter()
                .map(|v| (*v).to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(row.text, "None", "unset by default");

        // Picking the preset stores its bitmap storage key.
        app.image_editor_option_core("pattern-key", "Selection 1");
        assert_eq!(
            style_param(&app, "patternKey"),
            Some(CanvasPsParamValue::Text(preset.key.clone()))
        );
        let rows = app.image.editor_rows();
        assert_eq!(
            rows.iter()
                .find(|row| row.key == "pattern-key")
                .map(|row| row.text.as_str()),
            Some("Selection 1"),
            "the row shows the picked preset"
        );
        // "None" clears it.
        app.image_editor_option_core("pattern-key", "None");
        assert_eq!(
            style_param(&app, "patternKey"),
            Some(CanvasPsParamValue::Text(String::new()))
        );
        Ok(())
    }

    #[test]
    fn every_layer_style_type_adds_through_the_fx_panel() -> Result<(), Box<dyn std::error::Error>>
    {
        let (mut app, _dir) = editor_board_app()?;
        let style_ids = [
            "stroke",
            "drop-shadow",
            "inner-shadow",
            "outer-glow",
            "inner-glow",
            "bevel",
            "satin",
            "color-overlay",
            "gradient-overlay",
            "pattern-overlay",
        ];
        let layer_id = app.image.active_layer_id();
        for (index, style_id) in style_ids.iter().enumerate() {
            app.image_layer_action_core("fx-add", style_id);
            let layer = app
                .image
                .project
                .board_layers
                .iter()
                .find(|layer| layer.id == layer_id)
                .cloned()
                .ok_or("layer")?;
            let styles = layer.styles.as_ref().ok_or("styles")?;
            assert_eq!(styles.len(), index + 1, "{style_id} appended");
            let style = styles.get(index).ok_or("style")?;
            assert_eq!(
                crate::image::actions::layer_style_id(style.style_type),
                *style_id,
                "{style_id} keeps its kind"
            );

            let id = style.id.clone();
            app.image_fx_edit_core(&id);
            assert!(
                !app.image.editor_rows().is_empty(),
                "{style_id} exposes editor rows"
            );
        }
        Ok(())
    }
}
