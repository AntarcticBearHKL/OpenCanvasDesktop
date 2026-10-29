//! The 16 canvas blend modes + opacity clamp — port of
//! `web/src/lib/canvas/blend-modes.ts` (mode table, `clampLayerOpacity`,
//! `resolveBlendMode`) and the W3C compositing blend formulas those canvas
//! `GlobalCompositeOperation`s implement.

/// `CANVAS_BLEND_MODES` ids in table order (blend-modes.ts:7).
pub const BLEND_MODE_IDS: [&str; 16] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

/// `DEFAULT_BLEND_MODE` (blend-modes.ts:26).
pub const DEFAULT_BLEND_MODE: &str = "normal";

/// `LAYER_OPACITY_MIN`/`MAX`/`DEFAULT` (blend-modes.ts:28-30).
pub const LAYER_OPACITY_MIN: f64 = 0.0;
pub const LAYER_OPACITY_MAX: f64 = 1.0;
pub const LAYER_OPACITY_DEFAULT: f64 = 1.0;

/// One of the 16 `GlobalCompositeOperation` blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// The table id (also the layer `blendMode` string).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::ColorDodge => "color-dodge",
            Self::ColorBurn => "color-burn",
            Self::HardLight => "hard-light",
            Self::SoftLight => "soft-light",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
        }
    }
}

/// `resolveBlendMode` (blend-modes.ts:37): unknown ids fall back to `normal`.
pub fn resolve_blend_mode(id: &str) -> BlendMode {
    match id {
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "color-dodge" => BlendMode::ColorDodge,
        "color-burn" => BlendMode::ColorBurn,
        "hard-light" => BlendMode::HardLight,
        "soft-light" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

/// `clampLayerOpacity` (blend-modes.ts:32): non-finite -> default, rounded to
/// two decimals and clamped to `0..=1`.
pub fn clamp_layer_opacity(value: Option<f64>) -> f64 {
    match value {
        Some(v) if v.is_finite() => (v * 100.0).round() / 100.0,
        _ => LAYER_OPACITY_DEFAULT,
    }
    .clamp(LAYER_OPACITY_MIN, LAYER_OPACITY_MAX)
}

/// True for the twelve separable modes (the other four are HSL, whole-pixel).
fn is_separable(mode: BlendMode) -> bool {
    !matches!(
        mode,
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity
    )
}

/// Per-channel blend of two normalized channels for the separable modes
/// (`None` when the mode is one of the four non-separable HSL modes).
fn separable(mode: BlendMode, cb: f64, cs: f64) -> Option<f64> {
    let out = match mode {
        BlendMode::Normal => cs,
        BlendMode::Multiply => cb * cs,
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::Overlay => hard_light(cs, cb),
        BlendMode::Darken => cb.min(cs),
        BlendMode::Lighten => cb.max(cs),
        BlendMode::ColorDodge => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        BlendMode::HardLight => hard_light(cb, cs),
        BlendMode::SoftLight => soft_light(cb, cs),
        BlendMode::Difference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => {
            return None
        }
    };
    Some(out)
}

/// Hard-light (`multiply` for the lower half, `screen` for the upper half).
fn hard_light(cb: f64, cs: f64) -> f64 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        let d = 2.0 * cs - 1.0;
        cb + d - cb * d
    }
}

/// Soft-light per the W3C compositing spec.
fn soft_light(cb: f64, cs: f64) -> f64 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 {
            ((16.0 * cb - 12.0) * cb + 4.0) * cb
        } else {
            cb.sqrt()
        };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}

fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(mut c: [f64; 3]) -> [f64; 3] {
    let l = lum(c);
    let n = c.iter().copied().fold(f64::INFINITY, f64::min);
    let x = c.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if n < 0.0 {
        for slot in &mut c {
            *slot = l + (*slot - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for slot in &mut c {
            *slot = l + (*slot - l) * (1.0 - l) / (x - l);
        }
    }
    c
}

fn set_lum(c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn sat(c: [f64; 3]) -> f64 {
    let x = c.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let n = c.iter().copied().fold(f64::INFINITY, f64::min);
    x - n
}

/// `SetSat`: rescale the channel spread to `s`, keeping the min/mid/max order.
fn set_sat(c: [f64; 3], s: f64) -> [f64; 3] {
    let mut indices = [0_usize, 1, 2];
    indices.sort_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap_or(std::cmp::Ordering::Equal));
    let (min, mid, max) = (indices[0], indices[1], indices[2]);
    let mut out = [0.0_f64; 3];
    if c[max] > c[min] {
        out[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        out[max] = s;
    }
    out[min] = 0.0;
    out
}

/// W3C blend of two normalized RGB triples for all 16 modes.
pub fn blend_rgb(mode: BlendMode, cb: [f64; 3], cs: [f64; 3]) -> [f64; 3] {
    if is_separable(mode) {
        let mut out = [0.0; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            if let Some(value) = separable(mode, cb[i], cs[i]) {
                *slot = value;
            }
        }
        return out;
    }
    let lum_b = lum(cb);
    match mode {
        BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum_b),
        BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum_b),
        BlendMode::Color => set_lum(cs, lum_b),
        BlendMode::Luminosity => set_lum(cb, lum(cs)),
        // Separable modes handled above.
        _ => cs,
    }
}

/// `blendChannel`-style per-channel blend on `0..=255` inputs, returning a
/// `0..=255` float (kept for callers/tests that reason in bytes).
pub fn blend_channel(mode: BlendMode, backdrop: u8, source: u8) -> f64 {
    let cb = [f64::from(backdrop) / 255.0; 3];
    let cs = [f64::from(source) / 255.0; 3];
    blend_rgb(mode, cb, cs)[0] * 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_lists_all_sixteen_ids_in_order() {
        assert_eq!(BLEND_MODE_IDS.len(), 16);
        for id in BLEND_MODE_IDS {
            assert_eq!(resolve_blend_mode(id).id(), id);
        }
        assert_eq!(resolve_blend_mode("nope"), BlendMode::Normal);
    }

    #[test]
    fn opacity_clamp_rounds_and_clamps() {
        assert!((clamp_layer_opacity(None) - 1.0).abs() < f64::EPSILON);
        assert!((clamp_layer_opacity(Some(f64::NAN)) - 1.0).abs() < f64::EPSILON);
        assert!((clamp_layer_opacity(Some(0.444)) - 0.44).abs() < 1e-9);
        assert!((clamp_layer_opacity(Some(2.0)) - 1.0).abs() < f64::EPSILON);
        assert!((clamp_layer_opacity(Some(-1.0)) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn separable_modes_match_the_spec() {
        let c = |m: BlendMode, b: u8, s: u8| blend_channel(m, b, s);
        assert!((c(BlendMode::Normal, 10, 42) - 42.0).abs() < 1e-9);
        assert!((c(BlendMode::Multiply, 128, 255) - 128.0).abs() < 1e-9);
        assert!((c(BlendMode::Screen, 0, 128) - 128.0).abs() < 1e-9);
        assert!((c(BlendMode::Darken, 200, 55) - 55.0).abs() < 1e-9);
        assert!((c(BlendMode::Lighten, 200, 55) - 200.0).abs() < 1e-9);
        assert!((c(BlendMode::Difference, 200, 55) - 145.0).abs() < 1e-9);
        assert!((c(BlendMode::Exclusion, 255, 255) - 0.0).abs() < 1e-9);
        assert!((c(BlendMode::ColorDodge, 0, 0) - 0.0).abs() < 1e-9);
        assert!((c(BlendMode::ColorDodge, 128, 255) - 255.0).abs() < 1e-9);
        assert!((c(BlendMode::ColorBurn, 255, 0) - 255.0).abs() < 1e-9);
        // Overlay = hard-light with the operands swapped.
        assert!((c(BlendMode::Overlay, 0, 200) - 0.0).abs() < 1e-9);
        assert!((c(BlendMode::HardLight, 200, 0) - 0.0).abs() < 1e-9);
        // Soft-light of a mid gray on mid gray stays put.
        assert!((c(BlendMode::SoftLight, 128, 128) - 128.0).abs() < 1.0);
    }

    #[test]
    fn non_separable_modes_use_hsl() {
        // `Color` keeps the backdrop's luminance.
        let cb = [0.6, 0.4, 0.2];
        let cs = [0.2, 0.5, 0.8];
        let out = blend_rgb(BlendMode::Color, cb, cs);
        assert!((lum(out) - lum(cb)).abs() < 0.02);
        // `Saturation` keeps the source's saturation.
        let out = blend_rgb(BlendMode::Saturation, cb, cs);
        assert!((sat(out) - sat(cs)).abs() < 0.02);
    }
}
