//! Filter kernels + `applyPsFilter` — port of `web/src/lib/canvas/ps/filters.ts`.
//! Every filter is a pure `Surface` mutation; the two filters that used
//! `Math.random` (add-noise, crystallize) use a fixed-seed PRNG so headless
//! output is reproducible.

use serde::{Deserialize, Serialize};

use crate::doc::{CanvasPsParamValue, PsParams};
use crate::geom::Point;
use crate::surface::{clamp_u8, Surface};

/// `PsFilterType` (filters.ts:8): the 22 filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PsFilterType {
    GaussianBlur,
    BoxBlur,
    MotionBlur,
    RadialBlur,
    TiltShift,
    IrisBlur,
    Sharpen,
    UnsharpMask,
    AddNoise,
    Despeckle,
    DustScratches,
    Pixelate,
    Crystallize,
    Emboss,
    FindEdges,
    Solarize,
    Clouds,
    LensFlare,
    Ripple,
    Pinch,
    Spherize,
    Liquify,
}

/// `PS_FILTER_GROUPS` (filters.ts:39).
pub const PS_FILTER_GROUPS: [&str; 6] =
    ["blur", "sharpen", "noise", "pixelate", "stylize", "distort"];

/// The stable id of `kind`, matching its kebab-case serde name (the id the UI
/// menus and the headless CLI use to name a filter).
pub fn ps_filter_id(kind: PsFilterType) -> &'static str {
    match kind {
        PsFilterType::GaussianBlur => "gaussian-blur",
        PsFilterType::BoxBlur => "box-blur",
        PsFilterType::MotionBlur => "motion-blur",
        PsFilterType::RadialBlur => "radial-blur",
        PsFilterType::TiltShift => "tilt-shift",
        PsFilterType::IrisBlur => "iris-blur",
        PsFilterType::Sharpen => "sharpen",
        PsFilterType::UnsharpMask => "unsharp-mask",
        PsFilterType::AddNoise => "add-noise",
        PsFilterType::Despeckle => "despeckle",
        PsFilterType::DustScratches => "dust-scratches",
        PsFilterType::Pixelate => "pixelate",
        PsFilterType::Crystallize => "crystallize",
        PsFilterType::Emboss => "emboss",
        PsFilterType::FindEdges => "find-edges",
        PsFilterType::Solarize => "solarize",
        PsFilterType::Clouds => "clouds",
        PsFilterType::LensFlare => "lens-flare",
        PsFilterType::Ripple => "ripple",
        PsFilterType::Pinch => "pinch",
        PsFilterType::Spherize => "spherize",
        PsFilterType::Liquify => "liquify",
    }
}

/// Resolves a filter id (inverse of [`ps_filter_id`]); `None` for an unknown id.
pub fn ps_filter_from_id(id: &str) -> Option<PsFilterType> {
    Some(match id {
        "gaussian-blur" => PsFilterType::GaussianBlur,
        "box-blur" => PsFilterType::BoxBlur,
        "motion-blur" => PsFilterType::MotionBlur,
        "radial-blur" => PsFilterType::RadialBlur,
        "tilt-shift" => PsFilterType::TiltShift,
        "iris-blur" => PsFilterType::IrisBlur,
        "sharpen" => PsFilterType::Sharpen,
        "unsharp-mask" => PsFilterType::UnsharpMask,
        "add-noise" => PsFilterType::AddNoise,
        "despeckle" => PsFilterType::Despeckle,
        "dust-scratches" => PsFilterType::DustScratches,
        "pixelate" => PsFilterType::Pixelate,
        "crystallize" => PsFilterType::Crystallize,
        "emboss" => PsFilterType::Emboss,
        "find-edges" => PsFilterType::FindEdges,
        "solarize" => PsFilterType::Solarize,
        "clouds" => PsFilterType::Clouds,
        "lens-flare" => PsFilterType::LensFlare,
        "ripple" => PsFilterType::Ripple,
        "pinch" => PsFilterType::Pinch,
        "spherize" => PsFilterType::Spherize,
        "liquify" => PsFilterType::Liquify,
        _ => return None,
    })
}

const SEED: u32 = 0x9e37_79b9;

fn num(params: &PsParams, key: &str, fallback: f64) -> f64 {
    match params.get(key) {
        Some(CanvasPsParamValue::Number(value)) => *value,
        _ => fallback,
    }
}

fn str_param(params: &PsParams, key: &str, fallback: &str) -> String {
    match params.get(key) {
        Some(CanvasPsParamValue::Text(value)) => value.clone(),
        _ => fallback.to_string(),
    }
}

/// `psFilterParams` (filters.ts:74) for one filter.
pub fn ps_filter_params(kind: PsFilterType) -> PsParams {
    let mut params = PsParams::new();
    let mut put = |key: &str, value: CanvasPsParamValue| {
        params.insert(key.to_string(), value);
    };
    match kind {
        PsFilterType::GaussianBlur | PsFilterType::BoxBlur => {
            put("radius", CanvasPsParamValue::Number(4.0))
        }
        PsFilterType::MotionBlur => {
            put("angle", CanvasPsParamValue::Number(0.0));
            put("distance", CanvasPsParamValue::Number(12.0));
        }
        PsFilterType::RadialBlur => {
            put("amount", CanvasPsParamValue::Number(10.0));
            put("method", CanvasPsParamValue::Text("zoom".into()));
        }
        PsFilterType::TiltShift => {
            put("blur", CanvasPsParamValue::Number(12.0));
            put("angle", CanvasPsParamValue::Number(0.0));
            put("position", CanvasPsParamValue::Number(50.0));
            put("width", CanvasPsParamValue::Number(30.0));
        }
        PsFilterType::IrisBlur => {
            put("blur", CanvasPsParamValue::Number(12.0));
            put("x", CanvasPsParamValue::Number(50.0));
            put("y", CanvasPsParamValue::Number(50.0));
            put("radius", CanvasPsParamValue::Number(35.0));
            put("feather", CanvasPsParamValue::Number(40.0));
        }
        PsFilterType::Sharpen => put("amount", CanvasPsParamValue::Number(100.0)),
        PsFilterType::UnsharpMask => {
            put("amount", CanvasPsParamValue::Number(100.0));
            put("radius", CanvasPsParamValue::Number(2.0));
            put("threshold", CanvasPsParamValue::Number(0.0));
        }
        PsFilterType::AddNoise => {
            put("amount", CanvasPsParamValue::Number(12.0));
            put("distribution", CanvasPsParamValue::Text("uniform".into()));
            put("monochrome", CanvasPsParamValue::Number(1.0));
        }
        PsFilterType::DustScratches => {
            put("radius", CanvasPsParamValue::Number(1.0));
            put("threshold", CanvasPsParamValue::Number(16.0));
        }
        PsFilterType::Pixelate => put("size", CanvasPsParamValue::Number(12.0)),
        PsFilterType::Crystallize => put("size", CanvasPsParamValue::Number(16.0)),
        PsFilterType::Emboss => {
            put("angle", CanvasPsParamValue::Number(135.0));
            put("height", CanvasPsParamValue::Number(3.0));
            put("amount", CanvasPsParamValue::Number(100.0));
        }
        PsFilterType::Solarize => put("threshold", CanvasPsParamValue::Number(128.0)),
        PsFilterType::Clouds => {
            put("size", CanvasPsParamValue::Number(64.0));
            put("seed", CanvasPsParamValue::Number(7.0));
        }
        PsFilterType::LensFlare => {
            put("brightness", CanvasPsParamValue::Number(100.0));
            put("x", CanvasPsParamValue::Number(30.0));
            put("y", CanvasPsParamValue::Number(30.0));
        }
        PsFilterType::Ripple => {
            put("amount", CanvasPsParamValue::Number(10.0));
            put("size", CanvasPsParamValue::Number(60.0));
        }
        PsFilterType::Pinch => put("amount", CanvasPsParamValue::Number(50.0)),
        PsFilterType::Spherize => {
            put("amount", CanvasPsParamValue::Number(50.0));
            put("mode", CanvasPsParamValue::Text("normal".into()));
        }
        PsFilterType::Liquify => {
            put("size", CanvasPsParamValue::Number(80.0));
            put("pressure", CanvasPsParamValue::Number(60.0));
        }
        // Despeckle and find-edges have no parameters.
        PsFilterType::Despeckle | PsFilterType::FindEdges => {}
    }
    params
}

fn clamp_index(value: i64, limit: usize) -> usize {
    value.clamp(0, limit as i64 - 1) as usize
}

fn clone_data(surface: &Surface) -> Vec<u8> {
    surface.data().to_vec()
}

/// `psBoxBlur` (filters.ts:88): separable sliding-window box blur.
pub fn box_blur(surface: &mut Surface, radius: f64) {
    let r = radius.round() as i64;
    if r < 1 {
        return;
    }
    let (width, height) = (surface.width(), surface.height());
    let data = surface.data_mut();
    let mut temp = vec![0_u8; data.len()];
    let span = (2 * r + 1) as f64;
    for y in 0..height {
        for channel in 0..4 {
            let mut sum = 0.0_f64;
            for offset in -r..=r {
                sum += f64::from(data[(y * width + clamp_index(offset, width)) * 4 + channel]);
            }
            for x in 0..width {
                temp[(y * width + x) * 4 + channel] = clamp_u8(sum / span);
                sum += f64::from(
                    data[(y * width + clamp_index(x as i64 + r + 1, width)) * 4 + channel],
                ) - f64::from(
                    data[(y * width + clamp_index(x as i64 - r, width)) * 4 + channel],
                );
            }
        }
    }
    for x in 0..width {
        for channel in 0..4 {
            let mut sum = 0.0_f64;
            for offset in -r..=r {
                sum += f64::from(temp[(clamp_index(offset, height) * width + x) * 4 + channel]);
            }
            for y in 0..height {
                data[(y * width + x) * 4 + channel] = clamp_u8(sum / span);
                sum += f64::from(
                    temp[(clamp_index(y as i64 + r + 1, height) * width + x) * 4 + channel],
                ) - f64::from(
                    temp[(clamp_index(y as i64 - r, height) * width + x) * 4 + channel],
                );
            }
        }
    }
}

/// `psGaussianBlur` (filters.ts:116): three box passes.
pub fn gaussian_blur(surface: &mut Surface, radius: f64) {
    let r = ((4.0 * radius * radius + 1.0).sqrt() - 1.0) / 2.0;
    let r = r.round().max(1.0);
    for _ in 0..3 {
        box_blur(surface, r);
    }
}

/// `psConvolve` (filters.ts:121): RGB only, alpha preserved.
pub fn convolve(surface: &mut Surface, kernel: &[f64], size: usize, divisor: f64, offset: f64) {
    let (width, height) = (surface.width(), surface.height());
    let source = clone_data(surface);
    let half = (size as i64 - 1) / 2;
    let total = if divisor != 0.0 {
        divisor
    } else {
        let sum: f64 = kernel.iter().sum();
        if sum == 0.0 {
            1.0
        } else {
            sum
        }
    };
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let mut rgb = [0.0_f64; 3];
            for ky in 0..size {
                for kx in 0..size {
                    let weight = kernel.get(ky * size + kx).copied().unwrap_or(0.0);
                    if weight == 0.0 {
                        continue;
                    }
                    let index = (clamp_index(y as i64 + ky as i64 - half, height) * width
                        + clamp_index(x as i64 + kx as i64 - half, width))
                        * 4;
                    for channel in 0..3 {
                        rgb[channel] += f64::from(source[index + channel]) * weight;
                    }
                }
            }
            let index = (y * width + x) * 4;
            for channel in 0..3 {
                data[index + channel] = clamp_u8(rgb[channel] / total + offset);
            }
        }
    }
}

/// `psMedian` (filters.ts:149).
fn median(surface: &mut Surface, radius: i64, threshold: f64) {
    let (width, height) = (surface.width(), surface.height());
    let source = clone_data(surface);
    let size = (radius * 2 + 1) as usize;
    let mut window = vec![0.0_f64; size * size];
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            for channel in 0..3 {
                let mut count = 0;
                for ky in -radius..=radius {
                    for kx in -radius..=radius {
                        window[count] = f64::from(
                            source[(clamp_index(y as i64 + ky, height) * width
                                + clamp_index(x as i64 + kx, width))
                                * 4
                                + channel],
                        );
                        count += 1;
                    }
                }
                window[..count]
                    .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let value = window[count >> 1];
                let index = (y * width + x) * 4 + channel;
                if threshold <= 0.0 || (f64::from(data[index]) - value).abs() > threshold {
                    data[index] = clamp_u8(value);
                }
            }
        }
    }
}

/// `psRemap` (filters.ts:198): resample each pixel through a coordinate map.
fn remap(surface: &mut Surface, map: impl Fn(f64, f64) -> (f64, f64)) {
    let (width, height) = (surface.width(), surface.height());
    let source = surface.clone();
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = map(x as f64, y as f64);
            let sample = source.sample_bilinear(sx, sy);
            let index = (y * width + x) * 4;
            for channel in 0..4 {
                data[index + channel] = clamp_u8(sample[channel]);
            }
        }
    }
}

/// `psValueNoise` (filters.ts:215): deterministic fbm value noise.
fn value_noise(width: usize, height: usize, cell: f64, seed: f64) -> Vec<f64> {
    let random = |x: f64, y: f64| {
        let value = ((x * 127.1 + y * 311.7 + seed * 74.7).sin()) * 43758.5453;
        value - value.floor()
    };
    let mut grid = vec![0.0_f64; width * height];
    let scale = cell.max(1.0);
    let mut amplitude = 1.0;
    let mut total = 0.0;
    for octave in 0..4 {
        let step = (scale / 2_f64.powi(octave)).max(1.0);
        for y in 0..height {
            for x in 0..width {
                let gx = (x as f64 / step).floor();
                let gy = (y as f64 / step).floor();
                let fx = x as f64 / step - gx;
                let fy = y as f64 / step - gy;
                let sx = fx * fx * (3.0 - 2.0 * fx);
                let sy = fy * fy * (3.0 - 2.0 * fy);
                let top = random(gx, gy) * (1.0 - sx) + random(gx + 1.0, gy) * sx;
                let bottom = random(gx, gy + 1.0) * (1.0 - sx) + random(gx + 1.0, gy + 1.0) * sx;
                grid[y * width + x] += (top * (1.0 - sy) + bottom * sy) * amplitude;
            }
        }
        total += amplitude;
        amplitude *= 0.5;
    }
    for value in &mut grid {
        *value = *value / total * 255.0;
    }
    grid
}

/// `psApplyBlurMix` (filters.ts:247).
fn apply_blur_mix(surface: &mut Surface, blurred: &Surface, mask_at: impl Fn(f64, f64) -> f64) {
    let width = surface.width();
    let data = surface.data_mut();
    let source = blurred.data();
    for index in (0..data.len()).step_by(4) {
        let pixel = index / 4;
        let weight = mask_at((pixel % width) as f64, (pixel / width) as f64).clamp(0.0, 1.0);
        for channel in 0..4 {
            data[index + channel] = clamp_u8(
                f64::from(data[index + channel])
                    + (f64::from(source[index + channel]) - f64::from(data[index + channel]))
                        * weight,
            );
        }
    }
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from((t ^ (t >> 14)) & 0x00ff_ffff) / f64::from(0x0100_0000_u32)
    }
}

fn lens_flare(surface: &mut Surface, params: &PsParams) {
    let brightness = num(params, "brightness", 100.0) / 100.0;
    let (width, height) = (surface.width(), surface.height());
    let cx = (num(params, "x", 30.0) / 100.0) * width as f64;
    let cy = (num(params, "y", 30.0) / 100.0) * height as f64;
    let limit = width.min(height) as f64;
    let dx = width as f64 / 2.0 - cx;
    let dy = height as f64 / 2.0 - cy;
    let spots = [
        (-0.3, 0.08, [255.0, 220.0, 180.0]),
        (0.4, 0.05, [200.0, 230.0, 255.0]),
        (0.7, 0.09, [255.0, 240.0, 200.0]),
        (1.2, 0.12, [255.0, 190.0, 150.0]),
    ];
    let mut flare = Surface::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let radius = (limit * 0.9).max(1.0);
            let t = ((px - cx).hypot(py - cy) / radius).min(1.0);
            let glow_alpha = if t <= 0.12 {
                brightness * (1.0 - t / 0.12 * 0.3)
            } else if t <= 0.35 {
                brightness * (0.7 - (t - 0.12) / 0.23 * 0.45)
            } else {
                brightness * (0.25 * (1.0 - (t - 0.35) / 0.65))
            };
            let mut rgb = [255.0, 200.0, 140.0];
            let mut alpha = glow_alpha.max(0.0);
            if t <= 0.12 {
                rgb = [255.0, 238.0, 200.0];
            } else if t <= 0.35 {
                rgb = [255.0, 200.0, 140.0];
            }
            for (spot_t, spot_r, color) in spots {
                let sxp = cx + dx * spot_t;
                let syp = cy + dy * spot_t;
                let sr = (limit * spot_r).max(1.0);
                let d = (px - sxp).hypot(py - syp);
                if d < sr {
                    let a = (brightness * 0.5 * (1.0 - d / sr)).max(0.0);
                    rgb = color;
                    alpha = alpha.max(a);
                }
            }
            flare.set(
                x as i64,
                y as i64,
                [
                    rgb[0] as u8,
                    rgb[1] as u8,
                    rgb[2] as u8,
                    clamp_u8(alpha * 255.0),
                ],
            );
        }
    }
    let data = surface.data_mut();
    let source = flare.data();
    for index in (0..data.len()).step_by(4) {
        for channel in 0..3 {
            let base = f64::from(data[index + channel]);
            let top = f64::from(source[index + channel]);
            let sa = f64::from(source[index + 3]) / 255.0;
            let blended = 255.0 - (255.0 - base) * (255.0 - top) / 255.0;
            data[index + channel] = clamp_u8(base * (1.0 - sa) + blended * sa);
        }
    }
}

/// `psLiquifyPush` (filters.ts:636): push pixels forward under the brush.
pub fn ps_liquify_push(
    surface: &mut Surface,
    point: Point,
    delta: Point,
    radius: f64,
    pressure: f64,
) {
    let (width, height) = (surface.width(), surface.height());
    let limit = radius.clamp(1.0, 600.0);
    let source = surface.clone();
    let strength = (pressure / 100.0).clamp(0.01, 1.0);
    let x0 = (point.x - limit).floor().max(0.0) as usize;
    let y0 = (point.y - limit).floor().max(0.0) as usize;
    let x1 = ((point.x + limit).ceil() as usize).min(width);
    let y1 = ((point.y + limit).ceil() as usize).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let distance = (x as f64 - point.x).hypot(y as f64 - point.y);
            if distance > limit {
                continue;
            }
            let weight = (1.0 - distance / limit) * strength;
            let sample =
                source.sample_bilinear(x as f64 - delta.x * weight, y as f64 - delta.y * weight);
            surface.set(
                x as i64,
                y as i64,
                [
                    clamp_u8(sample[0]),
                    clamp_u8(sample[1]),
                    clamp_u8(sample[2]),
                    clamp_u8(sample[3]),
                ],
            );
        }
    }
}

/// `psMixFiltered` (filters.ts:708): lerp base toward filtered by mask weight.
pub fn ps_mix_filtered(base: &mut Surface, filtered: &Surface, mask: &[f64]) {
    let data = base.data_mut();
    let source = filtered.data();
    for (pixel, weight) in mask.iter().enumerate() {
        if *weight >= 1.0 {
            continue;
        }
        let index = pixel * 4;
        for channel in 0..4 {
            if index + channel >= data.len() || index + channel >= source.len() {
                continue;
            }
            let base_value = f64::from(data[index + channel]);
            data[index + channel] = clamp_u8(
                base_value
                    + (f64::from(source[index + channel]) - base_value) * weight.clamp(0.0, 1.0),
            );
        }
    }
}

/// `applyPsFilter` (filters.ts:261): one destructive filter, in place.
pub fn apply_ps_filter(surface: &mut Surface, kind: PsFilterType, params: &PsParams) {
    match kind {
        PsFilterType::GaussianBlur => gaussian_blur(surface, num(params, "radius", 4.0)),
        PsFilterType::BoxBlur => box_blur(surface, num(params, "radius", 4.0)),
        PsFilterType::Sharpen => {
            let amount = num(params, "amount", 100.0) / 100.0;
            convolve(
                surface,
                &[
                    0.0,
                    -amount,
                    0.0,
                    -amount,
                    1.0 + 4.0 * amount,
                    -amount,
                    0.0,
                    -amount,
                    0.0,
                ],
                3,
                1.0,
                0.0,
            );
        }
        PsFilterType::UnsharpMask => {
            let amount = num(params, "amount", 100.0) / 100.0;
            let threshold = num(params, "threshold", 0.0);
            let mut blurred = surface.clone();
            gaussian_blur(&mut blurred, num(params, "radius", 2.0));
            let data = surface.data_mut();
            let source = blurred.data();
            for index in (0..data.len()).step_by(4) {
                for channel in 0..3 {
                    let difference =
                        f64::from(data[index + channel]) - f64::from(source[index + channel]);
                    if difference.abs() > threshold {
                        data[index + channel] =
                            clamp_u8(f64::from(data[index + channel]) + difference * amount);
                    }
                }
            }
        }
        PsFilterType::Despeckle => median(surface, 1, 0.0),
        PsFilterType::DustScratches => {
            median(
                surface,
                num(params, "radius", 1.0).round() as i64,
                num(params, "threshold", 16.0),
            );
        }
        PsFilterType::Solarize => {
            let threshold = num(params, "threshold", 128.0);
            let data = surface.data_mut();
            for index in (0..data.len()).step_by(4) {
                let luma = 0.299 * f64::from(data[index])
                    + 0.587 * f64::from(data[index + 1])
                    + 0.114 * f64::from(data[index + 2]);
                if luma < threshold {
                    continue;
                }
                for channel in 0..3 {
                    data[index + channel] = 255 - data[index + channel];
                }
            }
        }
        PsFilterType::Pixelate => {
            let size = num(params, "size", 12.0).round().max(2.0) as usize;
            pixelate(surface, size);
        }
        PsFilterType::Crystallize => {
            let size = num(params, "size", 16.0).round().max(3.0);
            crystallize(surface, size);
        }
        PsFilterType::MotionBlur => motion_blur(surface, params),
        PsFilterType::RadialBlur => radial_blur(surface, params),
        PsFilterType::TiltShift => tilt_shift(surface, params),
        PsFilterType::IrisBlur => iris_blur(surface, params),
        PsFilterType::AddNoise => add_noise(surface, params),
        PsFilterType::Emboss => emboss(surface, params),
        PsFilterType::FindEdges => find_edges(surface),
        PsFilterType::Clouds => {
            let grid = value_noise(
                surface.width(),
                surface.height(),
                num(params, "size", 64.0),
                num(params, "seed", 7.0),
            );
            let data = surface.data_mut();
            for (pixel, value) in grid.iter().enumerate() {
                let index = pixel * 4;
                let byte = clamp_u8(*value);
                data[index] = byte;
                data[index + 1] = byte;
                data[index + 2] = byte;
                data[index + 3] = 255;
            }
        }
        PsFilterType::LensFlare => lens_flare(surface, params),
        PsFilterType::Ripple => {
            let amount = num(params, "amount", 10.0);
            let size = num(params, "size", 60.0).max(2.0);
            let tau = std::f64::consts::TAU;
            remap(surface, |x, y| {
                (
                    x + amount * (y / size * tau).sin(),
                    y + amount * (x / size * tau).sin(),
                )
            });
        }
        PsFilterType::Pinch => {
            let amount = num(params, "amount", 50.0) / 100.0;
            let cx = surface.width() as f64 / 2.0;
            let cy = surface.height() as f64 / 2.0;
            let limit = cx.min(cy);
            remap(surface, |x, y| {
                let (dx, dy) = (x - cx, y - cy);
                let distance = dx.hypot(dy);
                if distance >= limit {
                    return (x, y);
                }
                let scale = 1.0 + amount * (1.0 - distance / limit) * 0.9;
                (cx + dx * scale, cy + dy * scale)
            });
        }
        PsFilterType::Spherize => {
            let amount = num(params, "amount", 50.0) / 100.0;
            let mode = str_param(params, "mode", "normal");
            let cx = surface.width() as f64 / 2.0;
            let cy = surface.height() as f64 / 2.0;
            let limit = cx.min(cy);
            remap(surface, |x, y| {
                let (dx, dy) = (x - cx, y - cy);
                let distance = dx.hypot(dy);
                if distance >= limit {
                    return (x, y);
                }
                let factor = 1.0 + amount * (1.0 - (distance / limit).powi(2));
                match mode.as_str() {
                    "horizontal" => (cx + dx * factor, y),
                    "vertical" => (x, cy + dy * factor),
                    _ => (cx + dx * factor, cy + dy * factor),
                }
            });
        }
        // Liquify is driven by its dialog through `ps_liquify_push`.
        PsFilterType::Liquify => {}
    }
}

fn pixelate(surface: &mut Surface, size: usize) {
    let (width, height) = (surface.width(), surface.height());
    let data = surface.data_mut();
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let mut sum = [0.0_f64; 4];
            let mut count = 0.0_f64;
            for py in y..(y + size).min(height) {
                for px in x..(x + size).min(width) {
                    let index = (py * width + px) * 4;
                    for channel in 0..4 {
                        sum[channel] += f64::from(data[index + channel]);
                    }
                    count += 1.0;
                }
            }
            let fill = sum.map(|value| clamp_u8(value / count));
            for py in y..(y + size).min(height) {
                for px in x..(x + size).min(width) {
                    let index = (py * width + px) * 4;
                    data[index..index + 4].copy_from_slice(&fill);
                }
            }
            x += size;
        }
        y += size;
    }
}

fn crystallize(surface: &mut Surface, size: f64) {
    let (width, height) = (surface.width(), surface.height());
    let cols = (width as f64 / size).ceil() as usize + 2;
    let rows = (height as f64 / size).ceil() as usize + 2;
    let mut rng = Rng(SEED);
    let mut centres: Vec<(f64, f64, [f64; 4], f64)> = (0..cols * rows)
        .map(|index| {
            let col = (index % cols) as f64;
            let row = (index / cols) as f64;
            (
                (col - 0.5) * size + rng.next() * size,
                (row - 0.5) * size + rng.next() * size,
                [0.0; 4],
                0.0,
            )
        })
        .collect();
    let nearest = |centres: &[(f64, f64, [f64; 4], f64)], x: f64, y: f64| -> usize {
        let col = (x / size).floor() as i64 + 1;
        let row = (y / size).floor() as i64 + 1;
        let mut best = 0;
        let mut best_distance = f64::INFINITY;
        for oy in -1..=1 {
            for ox in -1..=1 {
                let idx = ((row + oy) * cols as i64 + col + ox).max(0) as usize;
                if let Some(centre) = centres.get(idx) {
                    let distance = (centre.0 - x).powi(2) + (centre.1 - y).powi(2);
                    if distance < best_distance {
                        best_distance = distance;
                        best = idx;
                    }
                }
            }
        }
        best
    };
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let target = nearest(&centres, x as f64, y as f64);
            let centre = &mut centres[target];
            for channel in 0..4 {
                centre.2[channel] += f64::from(data[index + channel]);
            }
            centre.3 += 1.0;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let target = nearest(&centres, x as f64, y as f64);
            let centre = centres[target];
            for channel in 0..4 {
                data[index + channel] = clamp_u8(centre.2[channel] / centre.3.max(1.0));
            }
        }
    }
}

fn motion_blur(surface: &mut Surface, params: &PsParams) {
    let distance = num(params, "distance", 12.0).round().max(1.0);
    let stride = (distance / 48.0).ceil().max(1.0);
    let radians = num(params, "angle", 0.0).to_radians();
    let (dx, dy) = (radians.cos(), radians.sin());
    let (width, height) = (surface.width(), surface.height());
    let source = clone_data(surface);
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0_f64; 4];
            let mut count = 0.0_f64;
            let mut offset = -distance / 2.0;
            while offset <= distance / 2.0 {
                let index = (clamp_index((y as f64 + dy * offset).round() as i64, height) * width
                    + clamp_index((x as f64 + dx * offset).round() as i64, width))
                    * 4;
                for channel in 0..4 {
                    sum[channel] += f64::from(source[index + channel]);
                }
                count += 1.0;
                offset += stride;
            }
            let index = (y * width + x) * 4;
            for channel in 0..4 {
                data[index + channel] = clamp_u8(sum[channel] / count.max(1.0));
            }
        }
    }
}

fn radial_blur(surface: &mut Surface, params: &PsParams) {
    let amount = num(params, "amount", 10.0);
    let method = str_param(params, "method", "zoom");
    let source = surface.clone();
    let (width, height) = (surface.width(), surface.height());
    let (cx, cy) = (width as f64 / 2.0, height as f64 / 2.0);
    let samples = 12;
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0_f64; 4];
            for step in 0..samples {
                let t = ((step as f64 - samples as f64 / 2.0) / samples as f64) * (amount / 100.0);
                let (sample_x, sample_y) = if method == "spin" {
                    (
                        cx + (x as f64 - cx) * t.cos() - (y as f64 - cy) * t.sin(),
                        cy + (x as f64 - cx) * t.sin() + (y as f64 - cy) * t.cos(),
                    )
                } else {
                    (
                        cx + (x as f64 - cx) * (1.0 + t),
                        cy + (y as f64 - cy) * (1.0 + t),
                    )
                };
                let sample = source.sample_bilinear(sample_x, sample_y);
                for channel in 0..4 {
                    sum[channel] += sample[channel];
                }
            }
            let index = (y * width + x) * 4;
            for channel in 0..4 {
                data[index + channel] = clamp_u8(sum[channel] / samples as f64);
            }
        }
    }
}

fn tilt_shift(surface: &mut Surface, params: &PsParams) {
    let blur = num(params, "blur", 12.0);
    let angle = num(params, "angle", 0.0).to_radians();
    let position = num(params, "position", 50.0) / 100.0;
    let width_band = (num(params, "width", 30.0) / 100.0).max(0.05);
    let mut blurred = surface.clone();
    gaussian_blur(&mut blurred, blur);
    let cx = surface.width() as f64 / 2.0;
    let cy = surface.height() as f64 * position;
    let extent = surface.height() as f64 / 2.0;
    let (nx, ny) = (angle.sin(), -angle.cos());
    apply_blur_mix(surface, &blurred, |x, y| {
        let distance = ((x - cx) * nx + (y - cy) * ny).abs() / extent;
        ((distance - width_band) / width_band.max(0.05)).clamp(0.0, 1.0)
    });
}

fn iris_blur(surface: &mut Surface, params: &PsParams) {
    let blur = num(params, "blur", 12.0);
    let mut blurred = surface.clone();
    gaussian_blur(&mut blurred, blur);
    let (width, height) = (surface.width() as f64, surface.height() as f64);
    let cx = (num(params, "x", 50.0) / 100.0) * width;
    let cy = (num(params, "y", 50.0) / 100.0) * height;
    let radius = (num(params, "radius", 35.0) / 100.0) * width.min(height);
    let feather = ((num(params, "feather", 40.0) / 100.0) * width.min(height)).max(1.0);
    apply_blur_mix(surface, &blurred, |x, y| {
        (((x - cx).hypot(y - cy) - radius) / feather).clamp(0.0, 1.0)
    });
}

fn add_noise(surface: &mut Surface, params: &PsParams) {
    let amount = num(params, "amount", 12.0) * 2.55;
    let gaussian = str_param(params, "distribution", "uniform") == "gaussian";
    let monochrome = num(params, "monochrome", 1.0) >= 0.5;
    let mut rng = Rng(SEED);
    let random = |rng: &mut Rng| {
        if gaussian {
            ((rng.next() + rng.next() + rng.next() + rng.next() - 2.0) / 2.0) * amount
        } else {
            (rng.next() * 2.0 - 1.0) * amount
        }
    };
    let data = surface.data_mut();
    for index in (0..data.len()).step_by(4) {
        let shared = random(&mut rng);
        for channel in 0..3 {
            let delta = if monochrome { shared } else { random(&mut rng) };
            data[index + channel] = clamp_u8(f64::from(data[index + channel]) + delta);
        }
    }
}

fn emboss(surface: &mut Surface, params: &PsParams) {
    let angle = num(params, "angle", 135.0).to_radians();
    let height = num(params, "height", 3.0);
    let amount = num(params, "amount", 100.0) / 100.0;
    let (dx, dy) = (angle.cos() * height, -angle.sin() * height);
    let (width, height_px) = (surface.width(), surface.height());
    let source = surface.clone();
    let data = surface.data_mut();
    for y in 0..height_px {
        for x in 0..width {
            let light = source.sample_bilinear(x as f64 + dx, y as f64 + dy);
            let shade = source.sample_bilinear(x as f64 - dx, y as f64 - dy);
            let index = (y * width + x) * 4;
            for channel in 0..3 {
                data[index + channel] =
                    clamp_u8(128.0 + (light[channel] - shade[channel]) * amount);
            }
        }
    }
}

fn find_edges(surface: &mut Surface) {
    let (width, height) = (surface.width(), surface.height());
    let source = clone_data(surface);
    let luma = |x: i64, y: i64| -> f64 {
        let index = (clamp_index(y, height) * width + clamp_index(x, width)) * 4;
        0.299 * f64::from(source[index])
            + 0.587 * f64::from(source[index + 1])
            + 0.114 * f64::from(source[index + 2])
    };
    let data = surface.data_mut();
    for y in 0..height {
        for x in 0..width {
            let (xf, yf) = (x as i64, y as i64);
            let gx = luma(xf + 1, yf - 1) + 2.0 * luma(xf + 1, yf) + luma(xf + 1, yf + 1)
                - luma(xf - 1, yf - 1)
                - 2.0 * luma(xf - 1, yf)
                - luma(xf - 1, yf + 1);
            let gy = luma(xf - 1, yf + 1) + 2.0 * luma(xf, yf + 1) + luma(xf + 1, yf + 1)
                - luma(xf - 1, yf - 1)
                - 2.0 * luma(xf, yf - 1)
                - luma(xf + 1, yf - 1);
            let value = clamp_u8(255.0 - gx.hypot(gy).min(255.0));
            let index = (y * width + x) * 4;
            data[index] = value;
            data[index + 1] = value;
            data[index + 2] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_ids_round_trip_and_are_unique() {
        let kinds = [
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
        let mut ids = std::collections::BTreeSet::new();
        for kind in kinds {
            let id = ps_filter_id(kind);
            assert!(ids.insert(id), "duplicate filter id {id}");
            assert_eq!(ps_filter_from_id(id), Some(kind));
        }
        assert_eq!(ids.len(), 22);
        assert_eq!(ps_filter_from_id("no-such-filter"), None);
    }

    fn solid(width: usize, height: usize, rgba: [u8; 4]) -> Surface {
        let mut surface = Surface::new(width, height);
        surface.fill(rgba);
        surface
    }

    #[test]
    fn every_filter_has_a_default_param_set() {
        for kind in [
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
        ] {
            let _ = ps_filter_params(kind);
        }
        assert_eq!(PS_FILTER_GROUPS.len(), 6);
    }

    #[test]
    fn box_blur_on_solid_keeps_the_colour() {
        let mut surface = solid(3, 3, [40, 80, 120, 255]);
        box_blur(&mut surface, 1.0);
        assert_eq!(surface.get(1, 1), [40, 80, 120, 255]);
    }

    #[test]
    fn box_blur_spreads_an_edge_pixel() {
        let mut surface = Surface::new(3, 1);
        surface.set(0, 0, [0, 0, 0, 255]);
        surface.set(1, 0, [0, 0, 0, 255]);
        surface.set(2, 0, [90, 0, 0, 255]);
        box_blur(&mut surface, 1.0);
        assert!(surface.get(1, 0)[0] > 0);
        assert!(surface.get(1, 0)[0] < 90);
    }

    #[test]
    fn convolve_with_identity_kernel_keeps_pixels() {
        let mut surface = solid(2, 2, [10, 20, 30, 255]);
        convolve(
            &mut surface,
            &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            3,
            1.0,
            0.0,
        );
        assert_eq!(surface.get(1, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn pixelate_averages_a_cell() {
        let mut surface = Surface::new(2, 2);
        surface.set(0, 0, [0, 0, 0, 255]);
        surface.set(1, 0, [100, 0, 0, 255]);
        surface.set(0, 1, [0, 0, 0, 255]);
        surface.set(1, 1, [0, 0, 0, 255]);
        apply_ps_filter(&mut surface, PsFilterType::Pixelate, &{
            let mut params = PsParams::new();
            params.insert("size".into(), CanvasPsParamValue::Number(2.0));
            params
        });
        assert_eq!(surface.get(0, 0)[0], 25);
        assert_eq!(surface.get(1, 0)[0], 25);
    }

    #[test]
    fn invert_like_solarize_flips_bright_pixels() {
        let mut surface = solid(1, 1, [200, 200, 200, 255]);
        apply_ps_filter(&mut surface, PsFilterType::Solarize, &PsParams::new());
        assert_eq!(surface.get(0, 0), [55, 55, 55, 255]);
    }

    #[test]
    fn clouds_are_deterministic_and_opaque() {
        let mut surface = Surface::new(8, 8);
        apply_ps_filter(&mut surface, PsFilterType::Clouds, &PsParams::new());
        let first = surface.get(0, 0);
        assert_eq!(first[3], 255);
        let mut again = Surface::new(8, 8);
        apply_ps_filter(&mut again, PsFilterType::Clouds, &PsParams::new());
        assert_eq!(again.get(0, 0), first);
    }

    #[test]
    fn liquify_push_moves_pixels() {
        let mut surface = Surface::new(5, 5);
        surface.set(2, 2, [255, 0, 0, 255]);
        ps_liquify_push(
            &mut surface,
            Point::new(2.0, 2.0),
            Point::new(1.0, 0.0),
            3.0,
            100.0,
        );
        assert!(surface.get(2, 2)[0] < 255);
    }

    #[test]
    fn mix_filtered_lerps_by_mask() {
        let mut base = solid(2, 1, [0, 0, 0, 255]);
        let filtered = solid(2, 1, [100, 100, 100, 255]);
        ps_mix_filtered(&mut base, &filtered, &[1.0, 0.5]);
        assert_eq!(base.get(0, 0), [0, 0, 0, 255]);
        assert_eq!(base.get(1, 0)[0], 50);
    }
}
