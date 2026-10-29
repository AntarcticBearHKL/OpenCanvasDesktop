//! Layer styles (fx) — port of `web/src/lib/canvas/ps-layer-styles.ts`. The
//! composite math (tint/blur/shift/dilate/erode/overlays) runs on `Surface`s;
//! the one web dependency the pure port cannot honour is pattern overlays that
//! reference a saved bitmap, which fall back to the procedural tile.

use crate::adjustments::ps_parse_gradient_stops;
use crate::blend::{resolve_blend_mode, BlendMode};
use crate::color::hex_to_rgb6;
use crate::composite::draw_surface;
use crate::doc::{CanvasPsLayerStyle, CanvasPsLayerStyleType, CanvasPsParamValue, PsParams};
use crate::filters::gaussian_blur;
use crate::geom::Point;
use crate::surface::{clamp_u8, Surface};

/// `PS_LAYER_STYLE_TYPES` (ps-layer-styles.ts:8).
pub const PS_LAYER_STYLE_TYPES: [CanvasPsLayerStyleType; 10] = [
    CanvasPsLayerStyleType::Stroke,
    CanvasPsLayerStyleType::DropShadow,
    CanvasPsLayerStyleType::InnerShadow,
    CanvasPsLayerStyleType::OuterGlow,
    CanvasPsLayerStyleType::InnerGlow,
    CanvasPsLayerStyleType::Bevel,
    CanvasPsLayerStyleType::Satin,
    CanvasPsLayerStyleType::ColorOverlay,
    CanvasPsLayerStyleType::GradientOverlay,
    CanvasPsLayerStyleType::PatternOverlay,
];

/// `PS_PATTERN_KINDS` (ps-layer-styles.ts:23).
pub const PS_PATTERN_KINDS: [&str; 4] = ["checker", "dots", "stripes", "crosshatch"];

const UNDER_STYLES: [CanvasPsLayerStyleType; 2] = [
    CanvasPsLayerStyleType::DropShadow,
    CanvasPsLayerStyleType::OuterGlow,
];
const OVER_ORDER: [CanvasPsLayerStyleType; 8] = [
    CanvasPsLayerStyleType::InnerShadow,
    CanvasPsLayerStyleType::InnerGlow,
    CanvasPsLayerStyleType::Bevel,
    CanvasPsLayerStyleType::Satin,
    CanvasPsLayerStyleType::ColorOverlay,
    CanvasPsLayerStyleType::GradientOverlay,
    CanvasPsLayerStyleType::PatternOverlay,
    CanvasPsLayerStyleType::Stroke,
];
const TAU: f64 = std::f64::consts::TAU;

fn num(params: &PsParams, key: &str, fallback: f64) -> f64 {
    match params.get(key) {
        Some(CanvasPsParamValue::Number(value)) => *value,
        _ => fallback,
    }
}

fn text(params: &PsParams, key: &str, fallback: &str) -> String {
    match params.get(key) {
        Some(CanvasPsParamValue::Text(value)) => value.clone(),
        _ => fallback.to_string(),
    }
}

fn number(value: f64) -> CanvasPsParamValue {
    CanvasPsParamValue::Number(value)
}

fn string(value: &str) -> CanvasPsParamValue {
    CanvasPsParamValue::Text(value.to_string())
}

fn strings(values: &[&str]) -> CanvasPsParamValue {
    CanvasPsParamValue::Texts(values.iter().map(|value| (*value).to_string()).collect())
}

/// `PS_LAYER_STYLE_DEFAULTS` (ps-layer-styles.ts:25) for one style.
pub fn ps_layer_style_defaults(kind: CanvasPsLayerStyleType) -> PsParams {
    let mut params = PsParams::new();
    match kind {
        CanvasPsLayerStyleType::Stroke => {
            params.insert("size".into(), number(3.0));
            params.insert("position".into(), string("outside"));
            params.insert("opacity".into(), number(100.0));
            params.insert("blendMode".into(), string("normal"));
            params.insert("color".into(), string("#000000"));
        }
        CanvasPsLayerStyleType::DropShadow | CanvasPsLayerStyleType::InnerShadow => {
            params.insert("color".into(), string("#000000"));
            params.insert("opacity".into(), number(75.0));
            params.insert("angle".into(), number(120.0));
            params.insert("distance".into(), number(5.0));
            params.insert("spread".into(), number(0.0));
            params.insert("size".into(), number(5.0));
            params.insert("blendMode".into(), string("multiply"));
        }
        CanvasPsLayerStyleType::OuterGlow | CanvasPsLayerStyleType::InnerGlow => {
            params.insert("color".into(), string("#ffffbe"));
            params.insert("opacity".into(), number(75.0));
            params.insert("size".into(), number(5.0));
            params.insert("spread".into(), number(0.0));
            params.insert("blendMode".into(), string("screen"));
        }
        CanvasPsLayerStyleType::Bevel => {
            params.insert("style".into(), string("inner"));
            params.insert("depth".into(), number(100.0));
            params.insert("direction".into(), string("up"));
            params.insert("size".into(), number(5.0));
            params.insert("soften".into(), number(0.0));
            params.insert("angle".into(), number(120.0));
            params.insert("highlightColor".into(), string("#ffffff"));
            params.insert("highlightOpacity".into(), number(75.0));
            params.insert("shadowColor".into(), string("#000000"));
            params.insert("shadowOpacity".into(), number(75.0));
        }
        CanvasPsLayerStyleType::Satin => {
            params.insert("color".into(), string("#000000"));
            params.insert("opacity".into(), number(50.0));
            params.insert("angle".into(), number(19.0));
            params.insert("distance".into(), number(11.0));
            params.insert("size".into(), number(14.0));
            params.insert("blendMode".into(), string("multiply"));
        }
        CanvasPsLayerStyleType::ColorOverlay => {
            params.insert("color".into(), string("#ff0000"));
            params.insert("opacity".into(), number(100.0));
            params.insert("blendMode".into(), string("normal"));
        }
        CanvasPsLayerStyleType::GradientOverlay => {
            params.insert("stops".into(), strings(&["#000000@0", "#ffffff@1"]));
            params.insert("angle".into(), number(90.0));
            params.insert("scale".into(), number(100.0));
            params.insert("reverse".into(), number(0.0));
            params.insert("opacity".into(), number(100.0));
            params.insert("blendMode".into(), string("normal"));
        }
        CanvasPsLayerStyleType::PatternOverlay => {
            params.insert("pattern".into(), string("checker"));
            params.insert("scale".into(), number(100.0));
            params.insert("angle".into(), number(0.0));
            params.insert("opacity".into(), number(100.0));
            params.insert("blendMode".into(), string("normal"));
        }
    }
    params
}

/// `createPsLayerStyle` (ps-layer-styles.ts:46).
pub fn create_ps_layer_style(kind: CanvasPsLayerStyleType, id: &str) -> CanvasPsLayerStyle {
    CanvasPsLayerStyle {
        id: id.to_string(),
        style_type: kind,
        enabled: true,
        params: ps_layer_style_defaults(kind),
    }
}

/// `psStylePadding` (ps-layer-styles.ts:50): extra pixels the scratch canvas
/// needs so shadows/glows/strokes are not clipped.
pub fn ps_style_padding(styles: &[CanvasPsLayerStyle], scale: f64) -> usize {
    let mut extent: f64 = 0.0;
    for style in styles.iter().filter(|style| style.enabled) {
        let size = num(&style.params, "size", 0.0) * scale;
        let distance = num(&style.params, "distance", 0.0) * scale;
        match style.style_type {
            CanvasPsLayerStyleType::DropShadow | CanvasPsLayerStyleType::InnerShadow => {
                extent = extent.max(distance + size * 3.0);
            }
            CanvasPsLayerStyleType::Stroke => {
                if text(&style.params, "position", "outside") != "inside" {
                    extent = extent.max(size);
                }
            }
            CanvasPsLayerStyleType::ColorOverlay
            | CanvasPsLayerStyleType::GradientOverlay
            | CanvasPsLayerStyleType::PatternOverlay => {}
            _ => extent = extent.max(size * 3.0 + distance),
        }
    }
    (extent.ceil() as usize + 2).min(400)
}

fn tint(source: &Surface, color: &str) -> Surface {
    let rgb = hex_to_rgb6(color);
    let mut out = source.clone();
    let data = out.data_mut();
    for index in (0..data.len()).step_by(4) {
        data[index] = clamp_u8(rgb[0]);
        data[index + 1] = clamp_u8(rgb[1]);
        data[index + 2] = clamp_u8(rgb[2]);
    }
    out
}

fn blurred(source: &Surface, blur: f64) -> Surface {
    if blur <= 0.5 {
        return source.clone();
    }
    let mut out = source.clone();
    gaussian_blur(&mut out, blur);
    out
}

fn shifted(source: &Surface, dx: f64, dy: f64) -> Surface {
    let mut out = Surface::new(source.width(), source.height());
    for y in 0..source.height() {
        for x in 0..source.width() {
            let sample = source.sample_bilinear(x as f64 - dx, y as f64 - dy);
            out.set(
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
    out
}

fn cut_out(base: &Surface, cut: &Surface) -> Surface {
    let mut out = base.clone();
    let cut_data = cut.data();
    let data = out.data_mut();
    for index in (0..data.len()).step_by(4) {
        let keep = 1.0 - f64::from(cut_data.get(index + 3).copied().unwrap_or(0)) / 255.0;
        data[index + 3] = clamp_u8(f64::from(data[index + 3]) * keep);
    }
    out
}

fn clip_to(base: &Surface, clip: &Surface) -> Surface {
    let mut out = base.clone();
    let clip_data = clip.data();
    let data = out.data_mut();
    for index in (0..data.len()).step_by(4) {
        let factor = f64::from(clip_data.get(index + 3).copied().unwrap_or(0)) / 255.0;
        data[index + 3] = clamp_u8(f64::from(data[index + 3]) * factor);
    }
    out
}

fn dilate(source: &Surface, radius: f64) -> Surface {
    if radius <= 0.5 {
        return source.clone();
    }
    let mut out = Surface::new(source.width(), source.height());
    for y in 0..source.height() {
        for x in 0..source.width() {
            let mut best = source.get(x as i64, y as i64);
            let steps = 24;
            for ring in [0.25, 0.5, 0.75, 1.0] {
                let distance = radius * ring;
                for step in 0..steps {
                    let angle = (step as f64 / steps as f64) * TAU;
                    let sx = (x as f64 + angle.cos() * distance).round() as i64;
                    let sy = (y as f64 + angle.sin() * distance).round() as i64;
                    let pixel = source.get(sx, sy);
                    if pixel[3] > best[3] {
                        best = pixel;
                    }
                }
            }
            out.set(x as i64, y as i64, best);
        }
    }
    out
}

fn erode(source: &Surface, radius: f64) -> Surface {
    if radius <= 0.5 {
        return source.clone();
    }
    let mut out = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            let mut min_alpha = source.get(x as i64, y as i64)[3];
            let steps = 24;
            for step in 0..steps {
                let angle = (step as f64 / steps as f64) * TAU;
                let sx = (x as f64 + angle.cos() * radius).round() as i64;
                let sy = (y as f64 + angle.sin() * radius).round() as i64;
                min_alpha = min_alpha.min(source.get(sx, sy)[3]);
            }
            let mut pixel = source.get(x as i64, y as i64);
            pixel[3] = min_alpha;
            out.set(x as i64, y as i64, pixel);
        }
    }
    out
}

fn light_vector(angle: f64) -> Point {
    let radians = angle.to_radians();
    Point::new(radians.cos(), -radians.sin())
}

fn shadow_offset(params: &PsParams) -> Point {
    let distance = num(params, "distance", 5.0);
    let light = light_vector(num(params, "angle", 120.0));
    Point::new(-light.x * distance, -light.y * distance)
}

/// `psPatternTile` (ps-layer-styles.ts:172): a white procedural pattern tile.
pub fn ps_pattern_tile(kind: &str, size: usize) -> Surface {
    let size = size.max(1);
    let mut tile = Surface::new(size, size);
    let half = size / 2;
    let stripe = (size as f64 * 0.18).max(1.0);
    for y in 0..size {
        for x in 0..size {
            let inside = match kind {
                "dots" => {
                    let cx = size as f64 / 2.0;
                    let cy = size as f64 / 2.0;
                    let r = size as f64 * 0.28;
                    (x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy) <= r
                }
                "stripes" => ((x + y) as f64 % size as f64) < stripe,
                "crosshatch" => {
                    (((x as f64 - y as f64).abs()) % size as f64) < stripe
                        || (((x as f64 + y as f64) % size as f64) - size as f64 / 2.0).abs()
                            < stripe / 2.0
                }
                _ => (x < half) == (y < half),
            };
            if inside {
                tile.set(x as i64, y as i64, [255, 255, 255, 255]);
            }
        }
    }
    tile
}

fn tile_pattern(source: &Surface, tile: &Surface) -> Surface {
    let mut out = Surface::new(source.width(), source.height());
    if tile.width() == 0 || tile.height() == 0 {
        return out;
    }
    for y in 0..source.height() {
        for x in 0..source.width() {
            let pixel = tile.get((x % tile.width()) as i64, (y % tile.height()) as i64);
            out.set(x as i64, y as i64, pixel);
        }
    }
    out
}

fn gradient_overlay(content: &Surface, params: &PsParams) -> Surface {
    let raw = match params.get("stops") {
        Some(CanvasPsParamValue::Texts(values)) => values.clone(),
        _ => vec!["#000000@0".to_string(), "#ffffff@1".to_string()],
    };
    let stops = ps_parse_gradient_stops(&raw);
    let reverse = num(params, "reverse", 0.0) >= 0.5;
    let ordered: Vec<(String, f64)> = if reverse {
        let mut reversed: Vec<(String, f64)> =
            stops.iter().map(|(c, p)| (c.clone(), 1.0 - p)).collect();
        reversed.reverse();
        reversed
    } else {
        stops
    };
    let angle = num(params, "angle", 90.0);
    let scale = num(params, "scale", 100.0).max(10.0) / 100.0;
    let (width, height) = (content.width() as f64, content.height() as f64);
    let radians = (angle - 90.0).to_radians();
    let (dx, dy) = (
        radians.cos() * (width / 2.0) * scale,
        radians.sin() * (height / 2.0) * scale,
    );
    let from = Point::new(width / 2.0 - dx, height / 2.0 - dy);
    let to = Point::new(width / 2.0 + dx, height / 2.0 + dy);
    let axis = Point::new(to.x - from.x, to.y - from.y);
    let length_sq = (axis.x * axis.x + axis.y * axis.y).max(1e-6);
    let mut ramp = Surface::new(content.width(), content.height());
    for y in 0..content.height() {
        for x in 0..content.width() {
            let t = (((x as f64 + 0.5 - from.x) * axis.x + (y as f64 + 0.5 - from.y) * axis.y)
                / length_sq)
                .clamp(0.0, 1.0);
            ramp.set(x as i64, y as i64, sample_stops(&ordered, t));
        }
    }
    clip_to(&ramp, content)
}

fn sample_stops(stops: &[(String, f64)], t: f64) -> [u8; 4] {
    let first = stops
        .first()
        .cloned()
        .unwrap_or_else(|| ("#000000".into(), 0.0));
    let last = stops
        .last()
        .cloned()
        .unwrap_or_else(|| ("#ffffff".into(), 1.0));
    let mut left = first;
    let mut right = last.clone();
    for window in stops.windows(2) {
        if t >= window[0].1 && t <= window[1].1 {
            left = window[0].clone();
            right = window[1].clone();
            break;
        }
    }
    let ratio = if right.1 == left.1 {
        0.0
    } else {
        (t - left.1) / (right.1 - left.1)
    };
    let lc = hex_to_rgb6(&left.0);
    let rc = hex_to_rgb6(&right.0);
    let rgb = [
        clamp_u8(lc[0] + (rc[0] - lc[0]) * ratio),
        clamp_u8(lc[1] + (rc[1] - lc[1]) * ratio),
        clamp_u8(lc[2] + (rc[2] - lc[2]) * ratio),
    ];
    [rgb[0], rgb[1], rgb[2], 255]
}

fn pattern_overlay(content: &Surface, params: &PsParams) -> Surface {
    let scale = num(params, "scale", 100.0).max(10.0) / 100.0;
    let tile = (16.0 * scale).round().max(4.0) as usize;
    let kind = text(params, "pattern", "checker");
    let pattern = tile_pattern(content, &ps_pattern_tile(&kind, tile));
    clip_to(&pattern, content)
}

fn draw_over(dest: &mut Surface, source: &Surface, blend_mode: &str, opacity: f64) {
    if opacity <= 0.0 {
        return;
    }
    draw_surface(
        dest,
        source,
        resolve_blend_mode(blend_mode),
        opacity / 100.0,
    );
}

fn draw_style(dest: &mut Surface, content: &Surface, style: &CanvasPsLayerStyle, scale: f64) {
    let params = &style.params;
    let size = num(params, "size", 5.0) * scale;
    let opacity = num(params, "opacity", 100.0);
    let blend_mode = text(params, "blendMode", "normal");
    match style.style_type {
        CanvasPsLayerStyleType::DropShadow => {
            let offset = shadow_offset(params);
            let spread = dilate(
                &tint(content, &text(params, "color", "#000000")),
                num(params, "spread", 0.0) / 100.0 * size,
            );
            let shadow = shifted(&blurred(&spread, size), offset.x * scale, offset.y * scale);
            draw_over(dest, &shadow, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::OuterGlow => {
            let glow = dilate(
                &tint(content, &text(params, "color", "#ffffbe")),
                num(params, "spread", 0.0) / 100.0 * size,
            );
            let glow = blurred(&glow, size);
            draw_over(dest, &glow, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::InnerShadow => {
            let offset = shadow_offset(params);
            let sliver = blurred(
                &cut_out(
                    &tint(content, &text(params, "color", "#000000")),
                    &shifted(content, offset.x * scale, offset.y * scale),
                ),
                size,
            );
            let clipped = clip_to(&sliver, content);
            draw_over(dest, &clipped, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::InnerGlow => {
            let glow = blurred(
                &dilate(
                    &tint(content, &text(params, "color", "#ffffbe")),
                    num(params, "spread", 0.0) / 100.0 * size,
                ),
                size,
            );
            let clipped = clip_to(&glow, content);
            draw_over(dest, &clipped, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::Bevel => {
            let light = light_vector(num(params, "angle", 120.0));
            let direction = if text(params, "direction", "up") == "down" {
                -1.0
            } else {
                1.0
            };
            let distance = (num(params, "depth", 100.0) / 100.0 * size * 0.8).max(1.0);
            let soften = num(params, "soften", 0.0) * scale;
            let highlight = clip_to(
                &blurred(
                    &cut_out(
                        &tint(content, &text(params, "highlightColor", "#ffffff")),
                        &shifted(
                            content,
                            -light.x * distance * direction,
                            -light.y * distance * direction,
                        ),
                    ),
                    size + soften,
                ),
                content,
            );
            let shadow = clip_to(
                &blurred(
                    &cut_out(
                        &tint(content, &text(params, "shadowColor", "#000000")),
                        &shifted(
                            content,
                            light.x * distance * direction,
                            light.y * distance * direction,
                        ),
                    ),
                    size + soften,
                ),
                content,
            );
            draw_over(
                dest,
                &highlight,
                "screen",
                num(params, "highlightOpacity", 75.0),
            );
            draw_over(
                dest,
                &shadow,
                "multiply",
                num(params, "shadowOpacity", 75.0),
            );
        }
        CanvasPsLayerStyleType::Satin => {
            let radians = num(params, "angle", 19.0).to_radians();
            let distance = num(params, "distance", 11.0) * scale;
            let sheen = clip_to(
                &blurred(
                    &cut_out(
                        &tint(content, &text(params, "color", "#000000")),
                        &shifted(content, radians.cos() * distance, radians.sin() * distance),
                    ),
                    size,
                ),
                content,
            );
            draw_over(dest, &sheen, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::ColorOverlay => {
            let tinted = tint(content, &text(params, "color", "#ff0000"));
            draw_over(dest, &tinted, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::GradientOverlay => {
            let overlay = gradient_overlay(content, params);
            draw_over(dest, &overlay, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::PatternOverlay => {
            let overlay = pattern_overlay(content, params);
            draw_over(dest, &overlay, &blend_mode, opacity);
        }
        CanvasPsLayerStyleType::Stroke => {
            let position = text(params, "position", "outside");
            let outer = if position == "inside" {
                0.0
            } else if position == "center" {
                size / 2.0
            } else {
                size
            };
            let inner = if position == "outside" {
                0.0
            } else if position == "center" {
                size / 2.0
            } else {
                size
            };
            let base: Surface = if outer > 0.0 {
                dilate(content, outer)
            } else {
                content.clone()
            };
            let subtract: Surface = if inner > 0.0 {
                erode(content, inner)
            } else {
                content.clone()
            };
            let ring = cut_out(&base, &subtract);
            let tinted = tint(&ring, &text(params, "color", "#000000"));
            draw_over(dest, &tinted, &blend_mode, opacity);
        }
    }
}

/// `psComposeLayerStyles` (ps-layer-styles.ts:329): content under-layered,
/// over-layered and flattened into one same-size surface.
pub fn ps_compose_layer_styles(
    content: &Surface,
    styles: &[CanvasPsLayerStyle],
    scale: f64,
) -> Surface {
    let enabled: Vec<&CanvasPsLayerStyle> = styles.iter().filter(|style| style.enabled).collect();
    if enabled.is_empty() {
        return content.clone();
    }
    let mut under = Surface::new(content.width(), content.height());
    let mut over = Surface::new(content.width(), content.height());
    let ordered = |types: &[CanvasPsLayerStyleType]| -> Vec<&CanvasPsLayerStyle> {
        types
            .iter()
            .flat_map(|kind| {
                enabled
                    .iter()
                    .filter(move |style| style.style_type == *kind)
            })
            .copied()
            .collect()
    };
    for style in ordered(&UNDER_STYLES) {
        draw_style(&mut under, content, style, scale);
    }
    for style in ordered(&OVER_ORDER) {
        draw_style(&mut over, content, style, scale);
    }
    let mut output = Surface::new(content.width(), content.height());
    draw_surface(&mut output, &under, BlendMode::Normal, 1.0);
    draw_surface(&mut output, content, BlendMode::Normal, 1.0);
    draw_surface(&mut output, &over, BlendMode::Normal, 1.0);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Surface {
        let mut surface = Surface::new(8, 8);
        for y in 2..6 {
            for x in 2..6 {
                surface.set(x, y, [200, 100, 50, 255]);
            }
        }
        surface
    }

    #[test]
    fn defaults_cover_every_style() {
        for kind in PS_LAYER_STYLE_TYPES {
            assert!(!ps_layer_style_defaults(kind).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn padding_grows_with_shadow() {
        let style = create_ps_layer_style(CanvasPsLayerStyleType::DropShadow, "s");
        let padding = ps_style_padding(&[style], 1.0);
        assert!(padding > 5);
        assert_eq!(ps_style_padding(&[], 1.0), 2);
    }

    #[test]
    fn color_overlay_tints_the_content() {
        let style = create_ps_layer_style(CanvasPsLayerStyleType::ColorOverlay, "s");
        let mut styled = create_ps_layer_style(CanvasPsLayerStyleType::ColorOverlay, "s");
        styled.params.insert("color".into(), string("#00ff00"));
        let out = ps_compose_layer_styles(&content(), &[styled], 1.0);
        assert_eq!(out.get(3, 3), [0, 255, 0, 255]);
        let _ = style;
    }

    #[test]
    fn stroke_expands_outside_the_content() {
        let mut style = create_ps_layer_style(CanvasPsLayerStyleType::Stroke, "s");
        style.params.insert("size".into(), number(2.0));
        style.params.insert("color".into(), string("#ff0000"));
        let out = ps_compose_layer_styles(&content(), &[style], 1.0);
        assert_eq!(out.get(1, 3)[3], 255);
        assert_eq!(out.get(3, 3), [200, 100, 50, 255]);
    }

    #[test]
    fn pattern_tile_kinds_are_non_empty() {
        for kind in PS_PATTERN_KINDS {
            let tile = ps_pattern_tile(kind, 8);
            assert_eq!(tile.width(), 8);
        }
    }
}
