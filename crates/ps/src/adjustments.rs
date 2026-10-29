//! The 15 adjustment layers — port of `web/src/lib/canvas/ps-adjustments.ts`:
//! the type list, per-type defaults, parameter helpers and `applyPsAdjustment`.
//! Everything works in place on a `Surface`'s RGB channels (alpha untouched).

use crate::color::hex_to_rgb6;
use crate::doc::{CanvasPsAdjustmentType, CanvasPsParamValue, PsParams};
use crate::surface::{clamp_u8, Surface};

/// `PS_ADJUSTMENT_TYPES` (ps-adjustments.ts:3).
pub const PS_ADJUSTMENT_TYPES: [CanvasPsAdjustmentType; 15] = [
    CanvasPsAdjustmentType::BrightnessContrast,
    CanvasPsAdjustmentType::Levels,
    CanvasPsAdjustmentType::Curves,
    CanvasPsAdjustmentType::Exposure,
    CanvasPsAdjustmentType::Vibrance,
    CanvasPsAdjustmentType::HueSaturation,
    CanvasPsAdjustmentType::ColorBalance,
    CanvasPsAdjustmentType::BlackWhite,
    CanvasPsAdjustmentType::PhotoFilter,
    CanvasPsAdjustmentType::ChannelMixer,
    CanvasPsAdjustmentType::GradientMap,
    CanvasPsAdjustmentType::Invert,
    CanvasPsAdjustmentType::Posterize,
    CanvasPsAdjustmentType::Threshold,
    CanvasPsAdjustmentType::SelectiveColor,
];

/// `PS_CURVE_IDENTITY` (ps-adjustments.ts:23).
pub const PS_CURVE_IDENTITY: [f64; 4] = [0.0, 0.0, 255.0, 255.0];
/// `PS_GRADIENT_MAP_DEFAULT` (ps-adjustments.ts:24).
pub const PS_GRADIENT_MAP_DEFAULT: [&str; 2] = ["#000000@0", "#ffffff@1"];
/// `PS_CHANNEL_MIXER_DEFAULT` (ps-adjustments.ts:25).
pub const PS_CHANNEL_MIXER_DEFAULT: [f64; 12] = [
    100.0, 0.0, 0.0, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0, 0.0, 100.0, 0.0,
];

/// The stable id of `kind`, matching its kebab-case serde name (the id the
/// adjustment panel and the headless CLI use).
pub fn ps_adjustment_id(kind: CanvasPsAdjustmentType) -> &'static str {
    match kind {
        CanvasPsAdjustmentType::BrightnessContrast => "brightness-contrast",
        CanvasPsAdjustmentType::Levels => "levels",
        CanvasPsAdjustmentType::Curves => "curves",
        CanvasPsAdjustmentType::Exposure => "exposure",
        CanvasPsAdjustmentType::Vibrance => "vibrance",
        CanvasPsAdjustmentType::HueSaturation => "hue-saturation",
        CanvasPsAdjustmentType::ColorBalance => "color-balance",
        CanvasPsAdjustmentType::BlackWhite => "black-white",
        CanvasPsAdjustmentType::PhotoFilter => "photo-filter",
        CanvasPsAdjustmentType::ChannelMixer => "channel-mixer",
        CanvasPsAdjustmentType::GradientMap => "gradient-map",
        CanvasPsAdjustmentType::Invert => "invert",
        CanvasPsAdjustmentType::Posterize => "posterize",
        CanvasPsAdjustmentType::Threshold => "threshold",
        CanvasPsAdjustmentType::SelectiveColor => "selective-color",
    }
}

/// Resolves an adjustment id (inverse of [`ps_adjustment_id`]); `None` for an
/// unknown id.
pub fn ps_adjustment_from_id(id: &str) -> Option<CanvasPsAdjustmentType> {
    Some(match id {
        "brightness-contrast" => CanvasPsAdjustmentType::BrightnessContrast,
        "levels" => CanvasPsAdjustmentType::Levels,
        "curves" => CanvasPsAdjustmentType::Curves,
        "exposure" => CanvasPsAdjustmentType::Exposure,
        "vibrance" => CanvasPsAdjustmentType::Vibrance,
        "hue-saturation" => CanvasPsAdjustmentType::HueSaturation,
        "color-balance" => CanvasPsAdjustmentType::ColorBalance,
        "black-white" => CanvasPsAdjustmentType::BlackWhite,
        "photo-filter" => CanvasPsAdjustmentType::PhotoFilter,
        "channel-mixer" => CanvasPsAdjustmentType::ChannelMixer,
        "gradient-map" => CanvasPsAdjustmentType::GradientMap,
        "invert" => CanvasPsAdjustmentType::Invert,
        "posterize" => CanvasPsAdjustmentType::Posterize,
        "threshold" => CanvasPsAdjustmentType::Threshold,
        "selective-color" => CanvasPsAdjustmentType::SelectiveColor,
        _ => return None,
    })
}

fn num(value: f64) -> CanvasPsParamValue {
    CanvasPsParamValue::Number(value)
}

fn text(value: &str) -> CanvasPsParamValue {
    CanvasPsParamValue::Text(value.to_string())
}

fn numbers(values: &[f64]) -> CanvasPsParamValue {
    CanvasPsParamValue::Numbers(values.to_vec())
}

fn texts(values: &[&str]) -> CanvasPsParamValue {
    CanvasPsParamValue::Texts(values.iter().map(|value| (*value).to_string()).collect())
}

/// `PS_ADJUSTMENT_DEFAULTS` (ps-adjustments.ts:27) for one type.
pub fn ps_adjustment_defaults(kind: CanvasPsAdjustmentType) -> PsParams {
    let mut params = PsParams::new();
    match kind {
        CanvasPsAdjustmentType::BrightnessContrast => {
            params.insert("brightness".into(), num(0.0));
            params.insert("contrast".into(), num(0.0));
        }
        CanvasPsAdjustmentType::Levels => {
            params.insert("channel".into(), text("rgb"));
            params.insert("inBlack".into(), num(0.0));
            params.insert("inWhite".into(), num(255.0));
            params.insert("gamma".into(), num(1.0));
            params.insert("outBlack".into(), num(0.0));
            params.insert("outWhite".into(), num(255.0));
        }
        CanvasPsAdjustmentType::Curves => {
            params.insert("channel".into(), text("rgb"));
            params.insert("points".into(), numbers(&PS_CURVE_IDENTITY));
        }
        CanvasPsAdjustmentType::Exposure => {
            params.insert("exposure".into(), num(0.0));
            params.insert("offset".into(), num(0.0));
            params.insert("gamma".into(), num(1.0));
        }
        CanvasPsAdjustmentType::Vibrance => {
            params.insert("vibrance".into(), num(0.0));
            params.insert("saturation".into(), num(0.0));
        }
        CanvasPsAdjustmentType::HueSaturation => {
            params.insert("channel".into(), text("master"));
            params.insert("hue".into(), num(0.0));
            params.insert("saturation".into(), num(0.0));
            params.insert("lightness".into(), num(0.0));
        }
        CanvasPsAdjustmentType::ColorBalance => {
            params.insert("tone".into(), text("midtones"));
            params.insert("shadows".into(), numbers(&[0.0, 0.0, 0.0]));
            params.insert("midtones".into(), numbers(&[0.0, 0.0, 0.0]));
            params.insert("highlights".into(), numbers(&[0.0, 0.0, 0.0]));
            params.insert("preserve".into(), num(1.0));
        }
        CanvasPsAdjustmentType::BlackWhite => {
            params.insert("reds".into(), num(40.0));
            params.insert("yellows".into(), num(60.0));
            params.insert("greens".into(), num(40.0));
            params.insert("cyans".into(), num(60.0));
            params.insert("blues".into(), num(20.0));
            params.insert("magentas".into(), num(80.0));
        }
        CanvasPsAdjustmentType::PhotoFilter => {
            params.insert("color".into(), text("#ec8a00"));
            params.insert("density".into(), num(25.0));
            params.insert("preserve".into(), num(1.0));
        }
        CanvasPsAdjustmentType::ChannelMixer => {
            params.insert("output".into(), text("red"));
            params.insert("monochrome".into(), num(0.0));
            params.insert("values".into(), numbers(&PS_CHANNEL_MIXER_DEFAULT));
        }
        CanvasPsAdjustmentType::GradientMap => {
            params.insert("stops".into(), texts(&PS_GRADIENT_MAP_DEFAULT));
            params.insert("reverse".into(), num(0.0));
        }
        CanvasPsAdjustmentType::Invert => {
            params.insert("channel".into(), text("rgb"));
        }
        CanvasPsAdjustmentType::Posterize => {
            params.insert("levels".into(), num(4.0));
        }
        CanvasPsAdjustmentType::Threshold => {
            params.insert("level".into(), num(128.0));
        }
        CanvasPsAdjustmentType::SelectiveColor => {
            params.insert("range".into(), num(0.0));
            params.insert("method".into(), text("relative"));
            params.insert("values".into(), numbers(&vec![0.0; 36]));
        }
    }
    params
}

/// `psCopyParams` (ps-adjustments.ts:45): a deep copy of the parameter bag.
fn param(params: &PsParams, key: &str, fallback: f64) -> f64 {
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

fn numbers_param(params: &PsParams, key: &str, fallback: &[f64]) -> Vec<f64> {
    match params.get(key) {
        Some(CanvasPsParamValue::Numbers(values)) => values.clone(),
        _ => fallback.to_vec(),
    }
}

fn texts_param(params: &PsParams, key: &str, fallback: &[&str]) -> Vec<String> {
    match params.get(key) {
        Some(CanvasPsParamValue::Texts(values)) => values.clone(),
        _ => fallback.iter().map(|value| (*value).to_string()).collect(),
    }
}

fn ps_lut(f: impl Fn(f64) -> f64) -> [u8; 256] {
    let mut table = [0_u8; 256];
    for (value, slot) in table.iter_mut().enumerate() {
        *slot = clamp_u8(f(value as f64));
    }
    table
}

fn ps_apply_lut(surface: &mut Surface, table: &[u8; 256], channel: Option<usize>) {
    let offsets: &[usize] = match channel {
        Some(index) => &[index],
        None => &[0, 1, 2],
    };
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        for offset in offsets {
            let value = data[index + offset] as usize;
            data[index + offset] = table[value];
        }
    }
}

fn ps_curve_lut(points: &[f64]) -> [u8; 256] {
    let mut control: Vec<(f64, f64)> = Vec::new();
    let mut index = 0;
    while index + 1 < points.len() {
        control.push((
            points[index].clamp(0.0, 255.0),
            points[index + 1].clamp(0.0, 255.0),
        ));
        index += 2;
    }
    control.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if control.len() < 2 {
        return ps_lut(|value| value);
    }
    let mut table = [0_u8; 256];
    for (value, slot) in table.iter_mut().enumerate() {
        let value = value as f64;
        let mut left = control[0];
        let mut right = control[control.len() - 1];
        for window in control.windows(2) {
            if value >= window[0].0 && value <= window[1].0 {
                left = window[0];
                right = window[1];
                break;
            }
        }
        *slot = if right.0 == left.0 {
            clamp_u8(right.1)
        } else {
            clamp_u8(left.1 + (value - left.0) / (right.0 - left.0) * (right.1 - left.1))
        };
    }
    table
}

fn hsl_from_rgb(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (rn, gn, bn) = (r / 255.0, g / 255.0, b / 255.0);
    let max = rn.max(gn).max(bn);
    let min = rn.min(gn).min(bn);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let delta = max - min;
    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == rn {
        (gn - bn) / delta + if gn < bn { 6.0 } else { 0.0 }
    } else if max == gn {
        (bn - rn) / delta + 2.0
    } else {
        (rn - gn) / delta + 4.0
    };
    ((hue / 6.0) * 360.0, s, l)
}

fn rgb_from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s <= 0.0 {
        return [l * 255.0; 3];
    }
    let hue = (((h % 360.0) + 360.0) % 360.0) / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |offset: f64| {
        let t = (hue + offset).rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [
        channel(1.0 / 3.0) * 255.0,
        channel(0.0) * 255.0,
        channel(-1.0 / 3.0) * 255.0,
    ]
}

fn hue_weight(hue: f64, centre: f64) -> f64 {
    let distance = (((hue - centre + 540.0) % 360.0) - 180.0).abs();
    if distance >= 60.0 {
        0.0
    } else {
        1.0 - distance / 60.0
    }
}

fn channel_of(params: &PsParams) -> Option<usize> {
    match text_param(params, "channel", "rgb").as_str() {
        "red" => Some(0),
        "green" => Some(1),
        "blue" => Some(2),
        _ => None,
    }
}

/// `psParseGradientStops` (ps-adjustments.ts:263).
pub fn ps_parse_gradient_stops(stops: &[String]) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = stops
        .iter()
        .map(|stop| {
            let (color, position) = stop.split_once('@').unwrap_or((stop.as_str(), "0"));
            let color = if color.len() == 7 && color.starts_with('#') {
                color.to_string()
            } else {
                "#000000".to_string()
            };
            (
                color,
                position.parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0),
            )
        })
        .collect();
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// `psFormatGradientStop` (ps-adjustments.ts:272).
pub fn ps_format_gradient_stop(color: &str, position: f64) -> String {
    format!(
        "{color}@{}",
        (position.clamp(0.0, 1.0) * 100.0).round() / 100.0
    )
}

fn apply_hue_saturation(surface: &mut Surface, params: &PsParams) {
    let channel = text_param(params, "channel", "master");
    let hue_shift = param(params, "hue", 0.0);
    let saturation = param(params, "saturation", 0.0);
    let lightness = param(params, "lightness", 0.0);
    if hue_shift == 0.0 && saturation == 0.0 && lightness == 0.0 {
        return;
    }
    let centre: Option<f64> = match channel.as_str() {
        "reds" => Some(0.0),
        "yellows" => Some(60.0),
        "greens" => Some(120.0),
        "cyans" => Some(180.0),
        "blues" => Some(240.0),
        "magentas" => Some(300.0),
        _ => None,
    };
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let (hue, sat, luma) = hsl_from_rgb(
            f64::from(data[index]),
            f64::from(data[index + 1]),
            f64::from(data[index + 2]),
        );
        let weight = centre.map_or(1.0, |centre| hue_weight(hue, centre));
        if weight == 0.0 {
            continue;
        }
        let next_sat =
            sat + (saturation / 100.0) * weight * (if saturation >= 0.0 { 1.0 - sat } else { sat });
        let next_light = (luma + (lightness / 200.0) * weight).clamp(0.0, 1.0);
        let rgb = rgb_from_hsl(
            hue + hue_shift * weight,
            next_sat.clamp(0.0, 1.0),
            next_light,
        );
        data[index] = clamp_u8(rgb[0]);
        data[index + 1] = clamp_u8(rgb[1]);
        data[index + 2] = clamp_u8(rgb[2]);
    }
}

fn apply_color_balance(surface: &mut Surface, params: &PsParams) {
    let shadows = numbers_param(params, "shadows", &[0.0, 0.0, 0.0]);
    let midtones = numbers_param(params, "midtones", &[0.0, 0.0, 0.0]);
    let highlights = numbers_param(params, "highlights", &[0.0, 0.0, 0.0]);
    let preserve = param(params, "preserve", 1.0) >= 0.5;
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let r = f64::from(data[index]);
        let g = f64::from(data[index + 1]);
        let b = f64::from(data[index + 2]);
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        let shadow_weight = ((128.0 - luma) / 128.0).clamp(0.0, 1.0);
        let highlight_weight = ((luma - 128.0) / 128.0).clamp(0.0, 1.0);
        let mid_weight = 1.0 - (luma - 128.0).abs() / 128.0;
        let mut next = [0.0_f64; 3];
        for (channel, slot) in next.iter_mut().enumerate() {
            let shift = shadows[channel] * shadow_weight
                + midtones[channel] * mid_weight
                + highlights[channel] * highlight_weight;
            *slot = [r, g, b][channel] + shift * 0.4;
        }
        if preserve {
            let next_luma = 0.299 * next[0] + 0.587 * next[1] + 0.114 * next[2];
            let scale = if next_luma > 1.0 {
                luma / next_luma
            } else {
                1.0
            };
            for slot in &mut next {
                *slot *= scale;
            }
        }
        data[index] = clamp_u8(next[0]);
        data[index + 1] = clamp_u8(next[1]);
        data[index + 2] = clamp_u8(next[2]);
    }
}

fn apply_black_white(surface: &mut Surface, params: &PsParams) {
    let reds = param(params, "reds", 40.0);
    let yellows = param(params, "yellows", 60.0);
    let greens = param(params, "greens", 40.0);
    let cyans = param(params, "cyans", 60.0);
    let blues = param(params, "blues", 20.0);
    let magentas = param(params, "magentas", 80.0);
    let boost = |base: f64, source: f64, amount: f64| base + (source - base) * (amount / 100.0);
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let r = f64::from(data[index]);
        let g = f64::from(data[index + 1]);
        let b = f64::from(data[index + 2]);
        let mut gray = 0.299 * r + 0.587 * g + 0.114 * b;
        gray = boost(gray, r, reds);
        gray = boost(gray, (r + g) / 2.0, yellows);
        gray = boost(gray, g, greens);
        gray = boost(gray, (g + b) / 2.0, cyans);
        gray = boost(gray, b, blues);
        gray = boost(gray, (r + b) / 2.0, magentas);
        let value = clamp_u8(gray);
        data[index] = value;
        data[index + 1] = value;
        data[index + 2] = value;
    }
}

fn apply_photo_filter(surface: &mut Surface, params: &PsParams) {
    let filter = hex_to_rgb6(&text_param(params, "color", "#ec8a00"));
    let density = param(params, "density", 25.0) / 100.0;
    let preserve = param(params, "preserve", 1.0) >= 0.5;
    let filter_luma = 0.299 * filter[0] + 0.587 * filter[1] + 0.114 * filter[2];
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let rgb = [
            f64::from(data[index]),
            f64::from(data[index + 1]),
            f64::from(data[index + 2]),
        ];
        let mut blend = [0.0_f64; 3];
        for (channel, slot) in blend.iter_mut().enumerate() {
            let value = rgb[channel];
            let filter_value = filter[channel];
            *slot = if filter_luma > 128.0 {
                255.0 - ((255.0 - value) * (255.0 - filter_value)) / 255.0
            } else {
                (value * filter_value) / 255.0
            };
        }
        let mut mix = [0.0_f64; 3];
        for (channel, slot) in mix.iter_mut().enumerate() {
            *slot = rgb[channel] * (1.0 - density) + blend[channel] * density;
        }
        if preserve {
            let luma = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
            let next_luma = 0.299 * mix[0] + 0.587 * mix[1] + 0.114 * mix[2];
            let scale = if next_luma > 1.0 {
                luma / next_luma
            } else {
                1.0
            };
            for slot in &mut mix {
                *slot *= scale;
            }
        }
        data[index] = clamp_u8(mix[0]);
        data[index + 1] = clamp_u8(mix[1]);
        data[index + 2] = clamp_u8(mix[2]);
    }
}

fn apply_channel_mixer(surface: &mut Surface, params: &PsParams) {
    let values = numbers_param(params, "values", &PS_CHANNEL_MIXER_DEFAULT);
    let monochrome = param(params, "monochrome", 0.0) >= 0.5;
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let r = f64::from(data[index]);
        let g = f64::from(data[index + 1]);
        let b = f64::from(data[index + 2]);
        if monochrome {
            let gray = (r * values[0] + g * values[1] + b * values[2]) / 100.0 + values[3] * 2.55;
            let value = clamp_u8(gray);
            data[index] = value;
            data[index + 1] = value;
            data[index + 2] = value;
            continue;
        }
        let next_r = (r * values[0] + g * values[1] + b * values[2]) / 100.0 + values[3] * 2.55;
        let next_g = (r * values[4] + g * values[5] + b * values[6]) / 100.0 + values[7] * 2.55;
        let next_b = (r * values[8] + g * values[9] + b * values[10]) / 100.0 + values[11] * 2.55;
        data[index] = clamp_u8(next_r);
        data[index + 1] = clamp_u8(next_g);
        data[index + 2] = clamp_u8(next_b);
    }
}

fn apply_gradient_map(surface: &mut Surface, params: &PsParams) {
    let raw = texts_param(params, "stops", &PS_GRADIENT_MAP_DEFAULT);
    let stops = ps_parse_gradient_stops(&raw);
    if stops.len() < 2 {
        return;
    }
    let reverse = param(params, "reverse", 0.0) >= 0.5;
    let mut table = [[0_u8; 3]; 256];
    for (value, slot) in table.iter_mut().enumerate() {
        let target = if reverse {
            1.0 - value as f64 / 255.0
        } else {
            value as f64 / 255.0
        };
        let mut left = stops[0].clone();
        let mut right = stops[stops.len() - 1].clone();
        for window in stops.windows(2) {
            if target >= window[0].1 && target <= window[1].1 {
                left = window[0].clone();
                right = window[1].clone();
                break;
            }
        }
        let ratio = if right.1 == left.1 {
            0.0
        } else {
            (target - left.1) / (right.1 - left.1)
        };
        let lc = hex_to_rgb6(&left.0);
        let rc = hex_to_rgb6(&right.0);
        for channel in 0..3 {
            slot[channel] = clamp_u8(lc[channel] + (rc[channel] - lc[channel]) * ratio);
        }
    }
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let luma = (0.299 * f64::from(data[index])
            + 0.587 * f64::from(data[index + 1])
            + 0.114 * f64::from(data[index + 2]))
        .round() as usize;
        let rgb = table[luma.min(255)];
        data[index] = rgb[0];
        data[index + 1] = rgb[1];
        data[index + 2] = rgb[2];
    }
}

fn selective_weight(range: usize, r: f64, g: f64, b: f64) -> f64 {
    let (hue, sat, luma) = hsl_from_rgb(r, g, b);
    let luma255 = luma * 255.0;
    let sat_weight = (sat * 3.0).min(1.0);
    let centres = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0];
    if range < 6 {
        return hue_weight(hue, centres[range]) * sat_weight;
    }
    let neutral = 1.0 - sat_weight;
    if range == 6 {
        return ((luma255 - 128.0) / 127.0).clamp(0.0, 1.0) * neutral;
    }
    if range == 7 {
        return (1.0 - (luma255 - 128.0).abs() / 128.0) * neutral;
    }
    ((128.0 - luma255) / 128.0).clamp(0.0, 1.0) * neutral
}

fn apply_selective_color(surface: &mut Surface, params: &PsParams) {
    let values = numbers_param(params, "values", &vec![0.0; 36]);
    let relative = text_param(params, "method", "relative") == "relative";
    if values.iter().all(|value| *value == 0.0) {
        return;
    }
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let r = f64::from(data[index]);
        let g = f64::from(data[index + 1]);
        let b = f64::from(data[index + 2]);
        let mut cyan = 1.0 - r / 255.0;
        let mut magenta = 1.0 - g / 255.0;
        let mut yellow = 1.0 - b / 255.0;
        let mut black = 0.0;
        for range in 0..9 {
            let weight = selective_weight(range, r, g, b);
            if weight == 0.0 {
                continue;
            }
            let base = range * 4;
            let apply = |value: f64, amount: f64| {
                value
                    + (if relative {
                        value * amount * weight
                    } else {
                        amount * weight
                    })
            };
            cyan = apply(cyan, values[base] / 100.0);
            magenta = apply(magenta, values[base + 1] / 100.0);
            yellow = apply(yellow, values[base + 2] / 100.0);
            black = apply(black, values[base + 3] / 100.0);
        }
        let clamp_unit = |value: f64| (value + black).clamp(0.0, 1.0);
        data[index] = clamp_u8(255.0 * (1.0 - clamp_unit(cyan)));
        data[index + 1] = clamp_u8(255.0 * (1.0 - clamp_unit(magenta)));
        data[index + 2] = clamp_u8(255.0 * (1.0 - clamp_unit(yellow)));
    }
}

/// `applyPsAdjustment` (ps-adjustments.ts:352): apply one adjustment in place.
pub fn apply_ps_adjustment(surface: &mut Surface, kind: CanvasPsAdjustmentType, params: &PsParams) {
    match kind {
        CanvasPsAdjustmentType::BrightnessContrast => {
            let brightness = param(params, "brightness", 0.0);
            let contrast = param(params, "contrast", 0.0);
            let table =
                ps_lut(|value| 128.0 + (value + brightness - 128.0) * (1.0 + contrast / 100.0));
            ps_apply_lut(surface, &table, None);
        }
        CanvasPsAdjustmentType::Levels => {
            let in_black = param(params, "inBlack", 0.0);
            let in_white = param(params, "inWhite", 255.0);
            let gamma = param(params, "gamma", 1.0).max(0.01);
            let out_black = param(params, "outBlack", 0.0);
            let out_white = param(params, "outWhite", 255.0);
            let span = (in_white - in_black).max(1.0);
            let table = ps_lut(|value| {
                out_black
                    + ((value - in_black) / span)
                        .clamp(0.0, 1.0)
                        .powf(1.0 / gamma)
                        * (out_white - out_black)
            });
            ps_apply_lut(surface, &table, channel_of(params));
        }
        CanvasPsAdjustmentType::Curves => {
            let table = ps_curve_lut(&numbers_param(params, "points", &PS_CURVE_IDENTITY));
            ps_apply_lut(surface, &table, channel_of(params));
        }
        CanvasPsAdjustmentType::Exposure => {
            let exposure = param(params, "exposure", 0.0);
            let offset = param(params, "offset", 0.0);
            let gamma = param(params, "gamma", 1.0).max(0.01);
            let factor = 2_f64.powf(exposure);
            let table = ps_lut(|value| {
                ((value * factor) / 255.0).clamp(0.0, 1.0).powf(1.0 / gamma) * 255.0
                    + offset * 255.0
            });
            ps_apply_lut(surface, &table, None);
        }
        CanvasPsAdjustmentType::Vibrance => {
            let vibrance = param(params, "vibrance", 0.0);
            let saturation = param(params, "saturation", 0.0);
            let data = surface.data_mut();
            for index in (0..data.len()).step_by(4) {
                let r = f64::from(data[index]);
                let g = f64::from(data[index + 1]);
                let b = f64::from(data[index + 2]);
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
                let factor = (1.0 + saturation / 100.0 + (vibrance / 100.0) * (1.0 - sat)).max(0.0);
                let luma = 0.299 * r + 0.587 * g + 0.114 * b;
                data[index] = clamp_u8(luma + (r - luma) * factor);
                data[index + 1] = clamp_u8(luma + (g - luma) * factor);
                data[index + 2] = clamp_u8(luma + (b - luma) * factor);
            }
        }
        CanvasPsAdjustmentType::HueSaturation => apply_hue_saturation(surface, params),
        CanvasPsAdjustmentType::ColorBalance => apply_color_balance(surface, params),
        CanvasPsAdjustmentType::BlackWhite => apply_black_white(surface, params),
        CanvasPsAdjustmentType::PhotoFilter => apply_photo_filter(surface, params),
        CanvasPsAdjustmentType::ChannelMixer => apply_channel_mixer(surface, params),
        CanvasPsAdjustmentType::GradientMap => apply_gradient_map(surface, params),
        CanvasPsAdjustmentType::Invert => {
            let table = ps_lut(|value| 255.0 - value);
            ps_apply_lut(surface, &table, channel_of(params));
        }
        CanvasPsAdjustmentType::Posterize => {
            let levels = param(params, "levels", 4.0).round().clamp(2.0, 255.0);
            let step = 255.0 / (levels - 1.0);
            let table = ps_lut(|value| (value / step).round() * step);
            ps_apply_lut(surface, &table, None);
        }
        CanvasPsAdjustmentType::Threshold => {
            let level = param(params, "level", 128.0);
            let data = surface.data_mut();
            for index in (0..data.len()).step_by(4) {
                let luma = 0.299 * f64::from(data[index])
                    + 0.587 * f64::from(data[index + 1])
                    + 0.114 * f64::from(data[index + 2]);
                let value = if luma >= level { 255 } else { 0 };
                data[index] = value;
                data[index + 1] = value;
                data[index + 2] = value;
            }
        }
        CanvasPsAdjustmentType::SelectiveColor => apply_selective_color(surface, params),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustment_ids_round_trip_and_are_unique() {
        let mut ids = std::collections::BTreeSet::new();
        for kind in PS_ADJUSTMENT_TYPES {
            let id = ps_adjustment_id(kind);
            assert!(ids.insert(id), "duplicate adjustment id {id}");
            assert_eq!(ps_adjustment_from_id(id), Some(kind));
        }
        assert_eq!(ids.len(), 15);
        assert_eq!(ps_adjustment_from_id("no-such-adjustment"), None);
    }

    fn surface_with(rgb: [u8; 3]) -> Surface {
        let mut surface = Surface::new(1, 1);
        surface.set(0, 0, [rgb[0], rgb[1], rgb[2], 255]);
        surface
    }

    fn param_map(pairs: &[(&str, CanvasPsParamValue)]) -> PsParams {
        let mut params = PsParams::new();
        for (key, value) in pairs {
            params.insert((*key).to_string(), value.clone());
        }
        params
    }

    #[test]
    fn defaults_cover_every_type() {
        for kind in PS_ADJUSTMENT_TYPES {
            assert!(!ps_adjustment_defaults(kind).is_empty(), "{kind:?}");
        }
        let defaults = ps_adjustment_defaults(CanvasPsAdjustmentType::Curves);
        assert_eq!(
            defaults.get("points"),
            Some(&CanvasPsParamValue::Numbers(vec![0.0, 0.0, 255.0, 255.0]))
        );
    }

    #[test]
    fn brightness_contrast_luts_the_pixel() {
        let mut surface = surface_with([100, 100, 100]);
        let params = param_map(&[("brightness", CanvasPsParamValue::Number(28.0))]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::BrightnessContrast,
            &params,
        );
        assert_eq!(surface.get(0, 0), [128, 128, 128, 255]);
    }

    #[test]
    fn invert_flips_channels() {
        let mut surface = surface_with([10, 20, 30]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::Invert,
            &PsParams::new(),
        );
        assert_eq!(surface.get(0, 0), [245, 235, 225, 255]);
    }

    #[test]
    fn threshold_is_black_or_white() {
        let mut surface = surface_with([200, 200, 200]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::Threshold,
            &PsParams::new(),
        );
        assert_eq!(surface.get(0, 0), [255, 255, 255, 255]);
        let mut dark = surface_with([10, 10, 10]);
        apply_ps_adjustment(
            &mut dark,
            CanvasPsAdjustmentType::Threshold,
            &PsParams::new(),
        );
        assert_eq!(dark.get(0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn posterize_quantises() {
        let mut surface = surface_with([90, 90, 90]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::Posterize,
            &PsParams::new(),
        );
        // 4 levels -> step 85; round(90/85)=1 -> 85.
        assert_eq!(surface.get(0, 0), [85, 85, 85, 255]);
    }

    #[test]
    fn black_white_with_defaults_produces_grey() {
        let mut surface = surface_with([200, 100, 50]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::BlackWhite,
            &PsParams::new(),
        );
        let pixel = surface.get(0, 0);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }

    #[test]
    fn channel_mixer_default_is_identity() {
        let mut surface = surface_with([10, 20, 30]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::ChannelMixer,
            &PsParams::new(),
        );
        assert_eq!(surface.get(0, 0), [10, 20, 30, 255]);
    }

    #[test]
    fn gradient_map_with_defaults_maps_luma() {
        let mut surface = surface_with([0, 0, 0]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::GradientMap,
            &PsParams::new(),
        );
        assert_eq!(surface.get(0, 0), [0, 0, 0, 255]);
        let mut white = surface_with([255, 255, 255]);
        apply_ps_adjustment(
            &mut white,
            CanvasPsAdjustmentType::GradientMap,
            &PsParams::new(),
        );
        assert_eq!(white.get(0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn alpha_is_never_touched() {
        let mut surface = Surface::new(1, 1);
        surface.set(0, 0, [10, 10, 10, 77]);
        apply_ps_adjustment(
            &mut surface,
            CanvasPsAdjustmentType::Invert,
            &PsParams::new(),
        );
        assert_eq!(surface.get(0, 0)[3], 77);
    }
}
