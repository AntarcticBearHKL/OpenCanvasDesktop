//! Text layout + warp math — port of `web/src/lib/canvas/ps-text.ts`. The
//! glyph rasteriser needs `measureText`, so the pure parts (font metrics,
//! paragraphs, line breaking, text-on-path placement, warp mesh) take the width
//! measurer as a callback and stay fully headless-testable.

use crate::doc::{
    CanvasPsLayer, CanvasPsPath, CanvasPsTextAlign, CanvasPsTextCase, CanvasPsTextParagraph,
    CanvasPsTextWarp, CanvasPsTextWarpStyle,
};
use crate::geom::Point;
use crate::path::PathSampler;

/// `PS_FONT_FAMILIES` (ps-text.ts:8).
pub const PS_FONT_FAMILIES: [&str; 11] = [
    "sans-serif",
    "serif",
    "monospace",
    "Arial",
    "Helvetica",
    "Georgia",
    "Times New Roman",
    "Verdana",
    "Trebuchet MS",
    "Courier New",
    "Impact",
];

/// `PS_TEXT_CASES` (ps-text.ts:9).
pub const PS_TEXT_CASES: [CanvasPsTextCase; 4] = [
    CanvasPsTextCase::None,
    CanvasPsTextCase::Upper,
    CanvasPsTextCase::Lower,
    CanvasPsTextCase::SmallCaps,
];

/// `PS_TEXT_ALIGNS` (ps-text.ts:10).
pub const PS_TEXT_ALIGNS: [CanvasPsTextAlign; 4] = [
    CanvasPsTextAlign::Left,
    CanvasPsTextAlign::Center,
    CanvasPsTextAlign::Right,
    CanvasPsTextAlign::Justify,
];

/// `PS_TEXT_WARP_STYLES` (ps-text.ts:11).
pub const PS_TEXT_WARP_STYLES: [CanvasPsTextWarpStyle; 10] = [
    CanvasPsTextWarpStyle::Arc,
    CanvasPsTextWarpStyle::ArcLower,
    CanvasPsTextWarpStyle::ArcUpper,
    CanvasPsTextWarpStyle::Flag,
    CanvasPsTextWarpStyle::Wave,
    CanvasPsTextWarpStyle::Fish,
    CanvasPsTextWarpStyle::Rise,
    CanvasPsTextWarpStyle::Bulge,
    CanvasPsTextWarpStyle::Shell,
    CanvasPsTextWarpStyle::Squeeze,
];

const DEFAULT_FONT_SIZE: f64 = 32.0;
const DEFAULT_LINE_HEIGHT: f64 = 1.2;

/// `PsTextRun` (ps-text.ts:5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRun {
    pub ch: char,
    pub small: bool,
}

/// `PsTextLine` (ps-text.ts:6).
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub runs: Vec<TextRun>,
    pub paragraph_start: bool,
}

/// `psTextFontSize` (ps-text.ts:32).
pub fn text_font_size(layer: &CanvasPsLayer) -> f64 {
    layer.font_size.unwrap_or(DEFAULT_FONT_SIZE).max(1.0)
}

/// `psTextLineHeight` (ps-text.ts:36).
pub fn text_line_height(layer: &CanvasPsLayer) -> f64 {
    match layer.leading {
        Some(leading) if leading > 0.0 => leading.max(1.0),
        _ => text_font_size(layer) * DEFAULT_LINE_HEIGHT,
    }
}

/// `psTextScaleX` (ps-text.ts:51).
pub fn text_scale_x(layer: &CanvasPsLayer) -> f64 {
    layer.text_scale_x.unwrap_or(100.0).max(1.0) / 100.0
}

/// `psTextScaleY` (ps-text.ts:55).
pub fn text_scale_y(layer: &CanvasPsLayer) -> f64 {
    layer.text_scale_y.unwrap_or(100.0).max(1.0) / 100.0
}

/// `psTextNeedsRaster` (ps-text.ts:59).
pub fn text_needs_raster(layer: &CanvasPsLayer) -> bool {
    fn truthy(value: Option<f64>) -> bool {
        value.is_some_and(|value| value != 0.0)
    }
    truthy(layer.tracking)
        || truthy(layer.kerning)
        || truthy(layer.leading)
        || truthy(layer.baseline_shift)
        || layer.text_scale_x.is_some_and(|value| value != 100.0)
        || layer.text_scale_y.is_some_and(|value| value != 100.0)
        || layer.faux_bold == Some(true)
        || layer.faux_italic == Some(true)
        || layer.underline == Some(true)
        || layer.strikethrough == Some(true)
        || layer
            .text_case
            .is_some_and(|value| value != CanvasPsTextCase::None)
        || layer.paragraph.is_some()
        || layer.text_path_id.is_some()
        || layer.text_warp.is_some()
}

/// `psTextParagraph` (ps-text.ts:78).
pub fn text_paragraph(layer: &CanvasPsLayer) -> CanvasPsTextParagraph {
    layer.paragraph.clone().unwrap_or(CanvasPsTextParagraph {
        align: CanvasPsTextAlign::Left,
        indent_left: 0.0,
        indent_right: 0.0,
        indent_first: 0.0,
        space_before: 0.0,
        space_after: 0.0,
        hyphenate: false,
    })
}

/// `psCaseRuns` (ps-text.ts:82).
pub fn case_runs(text: &str, text_case: CanvasPsTextCase) -> Vec<TextRun> {
    text.chars()
        .map(|ch| match text_case {
            CanvasPsTextCase::Upper => TextRun {
                ch: ch.to_uppercase().next().unwrap_or(ch),
                small: false,
            },
            CanvasPsTextCase::Lower => TextRun {
                ch: ch.to_lowercase().next().unwrap_or(ch),
                small: false,
            },
            CanvasPsTextCase::SmallCaps => TextRun {
                ch: ch.to_uppercase().next().unwrap_or(ch),
                small: ch.to_uppercase().next() != Some(ch) && ch.to_lowercase().next() == Some(ch),
            },
            CanvasPsTextCase::None => TextRun { ch, small: false },
        })
        .collect()
}

fn run_width(measure: &dyn Fn(bool, char) -> f64, run: TextRun) -> f64 {
    measure(run.small, run.ch)
}

/// `psLayoutText` (ps-text.ts:95): one measured pass with wrap + paragraph rules.
pub fn layout_text(
    layer: &CanvasPsLayer,
    max_width: f64,
    scale: f64,
    measure: &dyn Fn(bool, char) -> f64,
) -> Vec<TextLine> {
    let paragraph = text_paragraph(layer);
    let mut lines: Vec<TextLine> = Vec::new();
    for raw in layer.text.clone().unwrap_or_default().split('\n') {
        let runs = case_runs(raw, layer.text_case.unwrap_or(CanvasPsTextCase::None));
        let available =
            (max_width - (paragraph.indent_left + paragraph.indent_right) * scale).max(1.0);
        let mut current: Vec<TextRun> = Vec::new();
        let mut width = 0.0_f64;
        for run in runs {
            let width_of = run_width(measure, run);
            if run.ch != ' ' && width + width_of > available && !current.is_empty() {
                lines.push(TextLine {
                    runs: std::mem::take(&mut current),
                    paragraph_start: false,
                });
                width = 0.0;
                current.push(run);
                if paragraph.hyphenate && width_of > available {
                    lines.push(TextLine {
                        runs: std::mem::take(&mut current),
                        paragraph_start: false,
                    });
                } else {
                    width = width_of;
                }
                continue;
            }
            current.push(run);
            width += width_of;
        }
        lines.push(TextLine {
            runs: current,
            paragraph_start: true,
        });
    }
    lines
}

/// `psLineWidth` (ps-text.ts:135).
pub fn line_width(
    measure: &dyn Fn(bool, char) -> f64,
    line: &TextLine,
    tracking: f64,
    kerning: f64,
) -> f64 {
    line.runs
        .iter()
        .enumerate()
        .map(|(index, run)| {
            run_width(measure, *run) + if index > 0 { tracking + kerning } else { 0.0 }
        })
        .sum()
}

/// One glyph placed along a path (`psDrawTextOnPath`, ps-text.ts:215).
#[derive(Debug, Clone, PartialEq)]
pub struct PathGlyph {
    pub ch: char,
    pub point: Point,
    pub angle: f64,
    pub width: f64,
    pub advance: f64,
}

/// `psDrawTextOnPath` geometry: walks the first line's glyphs along the path.
pub fn text_on_path_placements(
    layer: &CanvasPsLayer,
    path: &CanvasPsPath,
    scale: f64,
    measure: &dyn Fn(bool, char) -> f64,
) -> Vec<PathGlyph> {
    let sampler = PathSampler::new(path);
    if sampler.total <= 0.0 {
        return Vec::new();
    }
    let font_size = (text_font_size(layer) * scale).max(1.0);
    let tracking = layer.tracking.unwrap_or(0.0) * (font_size / 1000.0);
    let kerning = layer.kerning.unwrap_or(0.0) * scale;
    let first_line = layer
        .text
        .clone()
        .unwrap_or_default()
        .split('\n')
        .next()
        .unwrap_or_default()
        .to_string();
    let mut cursor = 0.0_f64;
    let mut out = Vec::new();
    for (index, ch) in first_line.chars().enumerate() {
        let run = TextRun { ch, small: false };
        let width = run_width(measure, run);
        if cursor + width > sampler.total {
            break;
        }
        let middle = cursor + width / 2.0;
        let Some(point) = sampler.point_at(middle) else {
            break;
        };
        out.push(PathGlyph {
            ch,
            point,
            angle: sampler.angle_at(middle),
            width,
            advance: width + tracking + if index > 0 { kerning } else { 0.0 },
        });
        cursor += width + tracking + if index > 0 { kerning } else { 0.0 };
    }
    out
}

/// `psWarpOffset` (ps-text.ts:246).
pub fn warp_offset(warp: &CanvasPsTextWarp, u: f64, v: f64) -> Point {
    let bend = warp.bend.clamp(-100.0, 100.0) / 100.0;
    let horizontal = warp.horizontal.clamp(-100.0, 100.0) / 100.0;
    let vertical = warp.vertical.clamp(-100.0, 100.0) / 100.0;
    let middle = (std::f64::consts::PI * u).sin();
    let across = (std::f64::consts::PI * v).sin();
    let (mut x, mut y) = (0.0, 0.0);
    match warp.style {
        CanvasPsTextWarpStyle::Arc => y = -bend * middle * 0.5,
        CanvasPsTextWarpStyle::ArcLower => y = bend * middle * 0.5 * v,
        CanvasPsTextWarpStyle::ArcUpper => y = -bend * middle * 0.5 * (1.0 - v),
        CanvasPsTextWarpStyle::Flag => {
            y = bend * (2.0 * std::f64::consts::PI * u).sin() * 0.25 * (0.4 + v)
        }
        CanvasPsTextWarpStyle::Wave => y = bend * (3.0 * std::f64::consts::PI * u).sin() * 0.2,
        CanvasPsTextWarpStyle::Fish => x = -bend * across * (u - 0.5) * 0.6,
        CanvasPsTextWarpStyle::Rise => y = -bend * u * 0.5,
        CanvasPsTextWarpStyle::Bulge => x = bend * (u - 0.5) * across * 0.6,
        CanvasPsTextWarpStyle::Shell => y = -bend * (1.0 - (2.0 * u - 1.0).abs()) * 0.4,
        CanvasPsTextWarpStyle::Squeeze => {
            x = bend * (u - 0.5) * across * 0.6;
            y = bend * (v - 0.5) * middle * 0.4;
        }
    }
    Point::new(
        x + horizontal * (u - 0.5) * (1.0 - (2.0 * v - 1.0).abs()) * 0.5,
        y + vertical * (v - 0.5) * (1.0 - (2.0 * u - 1.0).abs()) * 0.5,
    )
}

/// `psTextWarpMesh` (ps-text.ts:291): the warp as the shared 4x4 mesh.
pub fn text_warp_mesh(warp: &CanvasPsTextWarp) -> Vec<Point> {
    (0..16)
        .map(|index| {
            let u = (index % 4) as f64 / 3.0;
            let v = (index / 4) as f64 / 3.0;
            let offset = warp_offset(warp, u, v);
            Point::new(u + offset.x, v + offset.y)
        })
        .collect()
}

/// `psTextWarpDefault` (ps-text.ts:300).
pub fn text_warp_default() -> CanvasPsTextWarp {
    CanvasPsTextWarp {
        style: CanvasPsTextWarpStyle::Arc,
        bend: 0.0,
        horizontal: 0.0,
        vertical: 0.0,
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::doc::CanvasPsLayerKind;
    use crate::path::{create_path, path_anchor};

    fn text_layer(text: &str) -> CanvasPsLayer {
        let mut layer = crate::board::base_layer(
            "l".into(),
            "T",
            CanvasPsLayerKind::Text,
            crate::geom::Bounds::new(0.0, 0.0, 100.0, 40.0),
        );
        layer.text = Some(text.to_string());
        layer.font_size = Some(20.0);
        layer
    }

    fn monospace(_small: bool, _ch: char) -> f64 {
        10.0
    }

    #[test]
    fn metrics_and_needs_raster() {
        let layer = text_layer("hi");
        assert!((text_font_size(&layer) - 20.0).abs() < 1e-9);
        assert!((text_line_height(&layer) - 24.0).abs() < 1e-9);
        assert!(!text_needs_raster(&layer));
        let mut tracked = layer.clone();
        tracked.tracking = Some(5.0);
        assert!(text_needs_raster(&tracked));
    }

    #[test]
    fn case_runs_upper_lower_small_caps() {
        let upper = case_runs("ab", CanvasPsTextCase::Upper);
        assert_eq!(upper[0].ch, 'A');
        assert!(!upper[0].small);
        let small = case_runs("aB", CanvasPsTextCase::SmallCaps);
        assert_eq!(small[0].ch, 'A');
        assert!(small[0].small);
        assert!(!small[1].small);
    }

    #[test]
    fn layout_wraps_on_the_measurer() {
        let layer = text_layer("abcde");
        let lines = layout_text(&layer, 25.0, 1.0, &monospace);
        let text: Vec<String> = lines
            .iter()
            .map(|line| line.runs.iter().map(|run| run.ch).collect())
            .collect();
        assert_eq!(text, vec!["ab", "cd", "e"]);
        assert!(lines.last().expect("line").paragraph_start);
    }

    #[test]
    fn line_width_counts_tracking_between_glyphs() {
        let line = TextLine {
            runs: vec![
                TextRun {
                    ch: 'a',
                    small: false,
                },
                TextRun {
                    ch: 'b',
                    small: false,
                },
            ],
            paragraph_start: true,
        };
        assert!((line_width(&monospace, &line, 2.0, 1.0) - 23.0).abs() < 1e-9);
    }

    #[test]
    fn warp_mesh_identity_when_bend_is_zero() {
        let mesh = text_warp_mesh(&text_warp_default());
        assert_eq!(mesh.len(), 16);
        for (index, point) in mesh.iter().enumerate() {
            assert!((point.x - (index % 4) as f64 / 3.0).abs() < 1e-9);
            assert!((point.y - (index / 4) as f64 / 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn arc_warp_lifts_the_middle() {
        let warp = CanvasPsTextWarp {
            style: CanvasPsTextWarpStyle::Arc,
            bend: 100.0,
            horizontal: 0.0,
            vertical: 0.0,
        };
        let offset = warp_offset(&warp, 0.5, 0.5);
        assert!(offset.y < -0.4);
    }

    #[test]
    fn text_on_path_places_glyphs_along_the_line() {
        let layer = text_layer("ab");
        let path = create_path(
            "p",
            "line",
            vec![path_anchor(0.0, 0.0), path_anchor(100.0, 0.0)],
            false,
        );
        let placements = text_on_path_placements(&layer, &path, 1.0, &monospace);
        assert_eq!(placements.len(), 2);
        assert!((placements[0].point.x - 5.0).abs() < 1e-6);
        assert!((placements[0].angle).abs() < 1e-6);
    }
}
