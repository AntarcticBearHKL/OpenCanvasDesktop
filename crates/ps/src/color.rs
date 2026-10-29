//! Color conversions — port of `web/src/lib/canvas/ps-color.ts` (hex/HSB/RGB)
//! plus `psHexToRgb` (ps-adjustments.ts:54).

use crate::surface::clamp_u8;

/// `#rrggbb` only, else black — `psHexToRgb` (ps-adjustments.ts:54).
pub fn hex_to_rgb6(color: &str) -> [f64; 3] {
    let bytes = color.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' && color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        let parse = |range: std::ops::Range<usize>| -> f64 {
            u8::from_str_radix(&color[range], 16).map_or(0.0, f64::from)
        };
        [parse(1..3), parse(3..5), parse(5..7)]
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// `psNormalizeHex` (ps-color.ts:7): `""` when not `#rgb`/`#rrggbb`.
pub fn normalize_hex(value: &str) -> String {
    let trimmed = value.trim();
    let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if !(body.len() == 3 || body.len() == 6) || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return String::new();
    }
    let expanded = if body.len() == 3 {
        body.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        body.to_string()
    };
    format!("#{}", expanded.to_ascii_lowercase())
}

/// `psRgbToHex` (ps-color.ts:14).
pub fn rgb_to_hex(rgb: [f64; 3]) -> String {
    let [r, g, b] = rgb.map(clamp_u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `psRgbToHsb` (ps-color.ts:18): hue `0..360`, saturation/value `0..100`.
pub fn rgb_to_hsb(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|value| value / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut h = 0.0;
    if delta > 0.0 {
        h = if max == r {
            ((g - b) / delta) % 6.0
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        } * 60.0;
        if h < 0.0 {
            h += 360.0;
        }
    }
    let s = if max > 0.0 {
        (delta / max) * 100.0
    } else {
        0.0
    };
    [h, s, max * 100.0]
}

/// `psHsbToRgb` (ps-color.ts:34).
pub fn hsb_to_rgb(hsb: [f64; 3]) -> [f64; 3] {
    let [hue, s, v] = hsb;
    let h = ((hue % 360.0) + 360.0) % 360.0;
    let s = (s / 100.0).clamp(0.0, 1.0);
    let v = (v / 100.0).clamp(0.0, 1.0);
    let chroma = v * s;
    let secondary = chroma * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let offset = v - chroma;
    let table = if h < 60.0 {
        [chroma, secondary, 0.0]
    } else if h < 120.0 {
        [secondary, chroma, 0.0]
    } else if h < 180.0 {
        [0.0, chroma, secondary]
    } else if h < 240.0 {
        [0.0, secondary, chroma]
    } else if h < 300.0 {
        [secondary, 0.0, chroma]
    } else {
        [chroma, 0.0, secondary]
    };
    table.map(|value| ((value + offset) * 255.0).round())
}

/// `psHexToHsb` (ps-color.ts:45).
pub fn hex_to_hsb(hex: &str) -> [f64; 3] {
    rgb_to_hsb(hex_to_rgb_triple(hex))
}

/// `psHsbToHex` (ps-color.ts:49).
pub fn hsb_to_hex(hsb: [f64; 3]) -> String {
    rgb_to_hex(hsb_to_rgb(hsb))
}

/// `psHexToRgbTriple` (ps-color.ts:53).
pub fn hex_to_rgb_triple(hex: &str) -> [f64; 3] {
    let normalized = normalize_hex(hex);
    hex_to_rgb6(if normalized.is_empty() {
        "#000000"
    } else {
        &normalized
    })
}

/// `psReadableText` (ps-color.ts:59): black or white label on a swatch.
pub fn readable_text(hex: &str) -> &'static str {
    let [r, g, b] = hex_to_rgb_triple(hex);
    if (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0 > 0.55 {
        "#000000"
    } else {
        "#ffffff"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_expands_shorthand_and_rejects_junk() {
        assert_eq!(normalize_hex("#ABC"), "#aabbcc");
        assert_eq!(normalize_hex("fff"), "#ffffff");
        assert_eq!(normalize_hex("#12"), "");
        assert_eq!(normalize_hex("zzz"), "");
    }

    #[test]
    fn hsb_round_trips_primaries() {
        assert_eq!(hex_to_rgb_triple("#ff0000"), [255.0, 0.0, 0.0]);
        let hsb = hex_to_hsb("#ff0000");
        assert!(hsb[0].abs() < 1e-9);
        assert!((hsb[1] - 100.0).abs() < 1e-9);
        assert_eq!(hsb_to_hex([120.0, 100.0, 100.0]), "#00ff00");
        assert_eq!(rgb_to_hex(hsb_to_rgb([240.0, 100.0, 100.0])), "#0000ff");
    }

    #[test]
    fn readable_text_picks_contrast() {
        assert_eq!(readable_text("#ffffff"), "#000000");
        assert_eq!(readable_text("#000000"), "#ffffff");
    }

    #[test]
    fn hex_to_rgb6_requires_six_digits() {
        assert_eq!(hex_to_rgb6("#ec8a00"), [236.0, 138.0, 0.0]);
        assert_eq!(hex_to_rgb6("#fff"), [0.0, 0.0, 0.0]);
    }
}
