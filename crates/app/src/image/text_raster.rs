//! Text glyph rasterisation for the Image Studio board composite — the app half
//! of `drawPsTextLayer`/`psDrawTextLines` (`web/src/lib/canvas/ps-text.ts`),
//! plugged into `open-canvas-ps` through its [`TextRasterizer`] hook.
//!
//! Layout, case, paragraph and warp math come from `open_canvas_ps::text`; this
//! module owns the font face (the embedded Baloo 2 variable TTF the UI already
//! ships) and stamps `ab_glyph` coverage into a [`Surface`]. Canvas exposes many
//! families, the desktop port renders all of them with that one face until a
//! family-specific font pack exists — the same documented deviation as the CJK
//! fallback in `main.rs`.
//! Owns the embedded font face and the `TextRasterizer` hook wiring.

use std::sync::OnceLock;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use open_canvas_ps::compositor::TextRasterizer;
use open_canvas_ps::doc::{CanvasPsLayer, CanvasPsPath, CanvasPsTextAlign, CanvasPsTextParagraph};
use open_canvas_ps::text::{
    layout_text, line_width, text_font_size, text_line_height, text_on_path_placements,
    text_paragraph, text_scale_x, text_scale_y, text_warp_mesh,
};
use open_canvas_ps::{clamp_u8, hex_to_rgba, Point, Rgba, Surface};

use crate::vm;

const EMBEDDED_FONT: &[u8] = include_bytes!("../../assets/fonts/Baloo2-Variable.ttf");
const SMALL_CAPS_SCALE: f64 = 0.8;
const DEFAULT_TEXT_COLOR: &str = "#000000";
const UNDERLINE_RATIO: f64 = 1.05;
const STRIKETHROUGH_RATIO: f64 = 0.55;

/// The app's [`TextRasterizer`]: one embedded font face + glyph stamping.
pub struct GlyphRasterizer {
    font: FontArc,
}

impl GlyphRasterizer {
    /// Loads the embedded face; `None` when the bytes are not a valid font.
    pub fn embedded() -> Option<Self> {
        FontArc::try_from_slice(EMBEDDED_FONT)
            .ok()
            .map(|font| Self { font })
    }

    fn scaled(&self, size: f64) -> impl ScaleFont<&FontArc> + '_ {
        self.font.as_scaled(self.px_scale(size))
    }

    /// CSS `font-size` is the em size; `PxScale` is the font's ascent-descent
    /// height, so convert through `height_unscaled / units_per_em`.
    fn px_scale(&self, size: f64) -> PxScale {
        let upem = self.font.units_per_em().unwrap_or(1.0);
        let height = self.font.height_unscaled();
        let factor = if upem > 0.0 { height / upem } else { 1.0 };
        PxScale::from(vm::len(size.max(1.0)) * factor)
    }

    fn advance(&self, size: f64, ch: char) -> f64 {
        let font = self.scaled(size);
        f64::from(font.h_advance(font.glyph_id(ch)))
    }

    fn ascent(&self, size: f64) -> f64 {
        f64::from(self.scaled(size).ascent())
    }

    fn render(
        &self,
        layer: &CanvasPsLayer,
        paths: &[CanvasPsPath],
        width: usize,
        height: usize,
        scale: f64,
    ) -> Surface {
        if let Some(path) = layer
            .text_path_id
            .as_deref()
            .and_then(|id| paths.iter().find(|path| path.id == id))
        {
            return self.render_on_path(layer, path, width, height, scale);
        }
        if let Some(warp) = layer.text_warp.as_ref() {
            let mut scratch = Surface::new(width, height);
            self.draw_lines(&mut scratch, layer, width, scale, Point::new(0.0, 0.0));
            let mesh = text_warp_mesh(warp);
            return open_canvas_ps::warp_mesh(&scratch, &mesh, width, height);
        }
        let mut surface = Surface::new(width, height);
        self.draw_lines(&mut surface, layer, width, scale, Point::new(0.0, 0.0));
        surface
    }

    fn draw_lines(
        &self,
        surface: &mut Surface,
        layer: &CanvasPsLayer,
        box_width: usize,
        scale: f64,
        origin: Point,
    ) {
        let context = TextContext::new(self, layer, scale);
        let measure = |small: bool, ch: char| {
            self.advance(
                if small {
                    context.small_size
                } else {
                    context.normal_size
                },
                ch,
            )
        };
        let lines = layout_text(layer, vm::i2f(vm::u2i(box_width)), scale, &measure);
        let paragraph = text_paragraph(layer);
        let box_width = vm::i2f(vm::u2i(box_width));
        let mut y = origin.y;
        for (index, line) in lines.iter().enumerate() {
            if index > 0 && line.paragraph_start {
                y += paragraph.space_before * scale;
            }
            let indent = paragraph.indent_left
                + if line.paragraph_start {
                    paragraph.indent_first
                } else {
                    0.0
                };
            let width = line_width(&measure, line, context.tracking, context.kerning);
            let available =
                (box_width - (paragraph.indent_left + paragraph.indent_right) * scale).max(1.0);
            let gaps = vm::i2f(vm::u2i(line.runs.len().saturating_sub(1)));
            let justify = paragraph.align == CanvasPsTextAlign::Justify && !line.paragraph_start;
            let extra = if justify && gaps > 0.0 {
                (available - indent * scale - width).max(0.0) / gaps
            } else {
                0.0
            };
            let left = line_left(origin.x, &paragraph, indent, available, width, scale);
            let baseline = y + context.baseline_shift + context.ascent;
            let mut x = left;
            for (slot, run) in line.runs.iter().enumerate() {
                if slot > 0 {
                    x += (context.tracking + context.kerning + extra) * context.scale_x;
                }
                self.draw_glyph(surface, &context, run.ch, run.small, x, baseline);
                x += self.advance(
                    if run.small {
                        context.small_size
                    } else {
                        context.normal_size
                    },
                    run.ch,
                ) * context.scale_x;
            }
            if context.underline || context.strikethrough {
                draw_decorations(surface, &context, width, origin.x, y);
            }
            y += context.line_height
                + if line.paragraph_start {
                    paragraph.space_after * scale
                } else {
                    0.0
                };
        }
    }

    fn draw_glyph(
        &self,
        surface: &mut Surface,
        context: &TextContext,
        ch: char,
        small: bool,
        pen_x: f64,
        baseline: f64,
    ) {
        let size = if small {
            context.small_size
        } else {
            context.normal_size
        };
        let glyph_scale = PxScale {
            x: self.px_scale(size * context.scale_x).x,
            y: self.px_scale(size * context.scale_y).y,
        };
        let glyph = self.font.glyph_id(ch).with_scale_and_position(
            glyph_scale,
            ab_glyph::point(vm::len(pen_x), vm::len(baseline)),
        );
        let Some(outlined) = self.font.outline_glyph(glyph) else {
            return;
        };
        let bounds = outlined.px_bounds();
        let (origin_x, origin_y) = (bounds.min.x, bounds.min.y);
        let color = context.color;
        outlined.draw(|x, y, coverage| {
            let x = f64::from(x) + f64::from(origin_x);
            let y = f64::from(y) + f64::from(origin_y);
            let shear = if context.faux_italic { 0.25 * y } else { 0.0 };
            stamp(surface, vm::len(x - shear), vm::len(y), coverage, color);
        });
        if context.faux_bold {
            let offset = vm::len((size * 0.03).max(0.5));
            outlined.draw(|x, y, coverage| {
                let x = vm::len(f64::from(x) + f64::from(origin_x));
                let y = vm::len(f64::from(y) + f64::from(origin_y));
                stamp(surface, x + offset, y, coverage * 0.6, color);
                stamp(surface, x - offset, y, coverage * 0.6, color);
            });
        }
    }

    fn render_on_path(
        &self,
        layer: &CanvasPsLayer,
        path: &CanvasPsPath,
        width: usize,
        height: usize,
        scale: f64,
    ) -> Surface {
        let context = TextContext::new(self, layer, scale);
        let measure = |small: bool, ch: char| {
            self.advance(
                if small {
                    context.small_size
                } else {
                    context.normal_size
                },
                ch,
            )
        };
        let mut surface = Surface::new(width, height);
        for glyph in text_on_path_placements(layer, path, scale, &measure) {
            self.stamp_rotated(&mut surface, &context, glyph.ch, glyph.point, glyph.angle);
        }
        surface
    }

    fn stamp_rotated(
        &self,
        target: &mut Surface,
        context: &TextContext,
        ch: char,
        centre: Point,
        angle: f64,
    ) {
        let extent = context.normal_size * context.scale_x.max(context.scale_y) * 1.6;
        let side = vm::i2u(vm::f2i(extent.max(8.0))).saturating_mul(2).max(8);
        let pivot = vm::i2f(vm::u2i(side)) / 2.0;
        let mut glyph = Surface::new(side, side);
        let advance = self.advance(context.normal_size, ch);
        self.draw_glyph(
            &mut glyph,
            context,
            ch,
            false,
            pivot - advance * context.scale_x / 2.0,
            pivot,
        );
        let radians = angle.to_radians();
        let (cos, sin) = (radians.cos(), radians.sin());
        let reach = vm::i2f(vm::u2i(side)) / 2.0 + 2.0;
        let start_x = vm::i2u(vm::f2i(centre.x - reach));
        let start_y = vm::i2u(vm::f2i(centre.y - reach));
        let end_x = vm::i2u(vm::f2i(centre.x + reach)).min(target.width().saturating_sub(1));
        let end_y = vm::i2u(vm::f2i(centre.y + reach)).min(target.height().saturating_sub(1));
        for py in start_y..=end_y {
            for px in start_x..=end_x {
                let dx = vm::i2f(vm::u2i(px)) + 0.5 - centre.x;
                let dy = vm::i2f(vm::u2i(py)) + 0.5 - centre.y;
                let lx = dx * cos + dy * sin + pivot;
                let ly = -dx * sin + dy * cos + pivot;
                if lx < 0.0
                    || ly < 0.0
                    || lx >= vm::i2f(vm::u2i(side))
                    || ly >= vm::i2f(vm::u2i(side))
                {
                    continue;
                }
                let sample = glyph.sample_bilinear(lx, ly);
                if sample[3] <= 0.0 {
                    continue;
                }
                blend_sample(target, px, py, sample);
            }
        }
    }
}

impl TextRasterizer for GlyphRasterizer {
    fn rasterize(
        &self,
        layer: &CanvasPsLayer,
        paths: &[CanvasPsPath],
        width: usize,
        height: usize,
        scale: f64,
    ) -> Option<Surface> {
        if width == 0 || height == 0 || scale <= 0.0 {
            return None;
        }
        Some(self.render(layer, paths, width, height, scale))
    }
}

/// The process-wide rasterizer (`board_surface` + PNG export share one face).
pub fn shared() -> Option<&'static GlyphRasterizer> {
    static SHARED: OnceLock<Option<GlyphRasterizer>> = OnceLock::new();
    SHARED.get_or_init(GlyphRasterizer::embedded).as_ref()
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "the four text decorations are independent canvas attributes, not a state machine"
)]
struct TextContext {
    color: Rgba,
    normal_size: f64,
    small_size: f64,
    line_height: f64,
    tracking: f64,
    kerning: f64,
    baseline_shift: f64,
    scale_x: f64,
    scale_y: f64,
    ascent: f64,
    faux_bold: bool,
    faux_italic: bool,
    underline: bool,
    strikethrough: bool,
}

impl TextContext {
    fn new(rasterizer: &GlyphRasterizer, layer: &CanvasPsLayer, scale: f64) -> Self {
        let normal_size = (text_font_size(layer) * scale).max(1.0);
        let color = layer
            .color
            .as_deref()
            .filter(|color| !color.is_empty())
            .unwrap_or(DEFAULT_TEXT_COLOR);
        Self {
            color: hex_to_rgba(color),
            normal_size,
            small_size: normal_size * SMALL_CAPS_SCALE,
            line_height: (text_line_height(layer) * scale).max(1.0),
            tracking: layer.tracking.unwrap_or(0.0) * (normal_size / 1000.0),
            kerning: layer.kerning.unwrap_or(0.0) * scale,
            baseline_shift: layer.baseline_shift.unwrap_or(0.0) * scale,
            scale_x: text_scale_x(layer),
            scale_y: text_scale_y(layer),
            ascent: rasterizer.ascent(normal_size),
            faux_bold: layer.faux_bold == Some(true),
            faux_italic: layer.faux_italic == Some(true),
            underline: layer.underline == Some(true),
            strikethrough: layer.strikethrough == Some(true),
        }
    }
}

fn line_left(
    origin_x: f64,
    paragraph: &CanvasPsTextParagraph,
    indent: f64,
    available: f64,
    width: f64,
    scale: f64,
) -> f64 {
    let left = origin_x + indent * scale;
    match paragraph.align {
        CanvasPsTextAlign::Center => left + (available - width).max(0.0) / 2.0,
        CanvasPsTextAlign::Right => left + (available - width).max(0.0),
        CanvasPsTextAlign::Left | CanvasPsTextAlign::Justify => left,
    }
}

fn draw_decorations(
    surface: &mut Surface,
    context: &TextContext,
    width: f64,
    origin_x: f64,
    line_top: f64,
) {
    let thickness = vm::i2u(vm::f2i((context.normal_size * 0.05).max(1.0))).max(1);
    if context.underline {
        let y = line_top + context.normal_size * UNDERLINE_RATIO;
        fill_band(
            surface,
            origin_x,
            origin_x + width,
            y,
            thickness,
            context.color,
        );
    }
    if context.strikethrough {
        let y = line_top + context.normal_size * STRIKETHROUGH_RATIO;
        fill_band(
            surface,
            origin_x,
            origin_x + width,
            y,
            thickness,
            context.color,
        );
    }
}

fn stamp(surface: &mut Surface, x: f32, y: f32, coverage: f32, color: Rgba) {
    let px = vm::f2i(f64::from(x));
    let py = vm::f2i(f64::from(y));
    let cov = f64::from(coverage).clamp(0.0, 1.0);
    let alpha = f64::from(color[3]) * cov;
    if alpha <= 0.0 {
        return;
    }
    let dst = surface.get(px, py);
    let source = alpha / 255.0;
    let backdrop = f64::from(dst[3]) / 255.0;
    let out_alpha = source + backdrop * (1.0 - source);
    if out_alpha <= 0.0 {
        return;
    }
    let mix = |from: u8, to: u8| {
        clamp_u8((f64::from(from) * source + f64::from(to) * backdrop * (1.0 - source)) / out_alpha)
    };
    surface.set(
        px,
        py,
        [
            mix(color[0], dst[0]),
            mix(color[1], dst[1]),
            mix(color[2], dst[2]),
            clamp_u8(out_alpha * 255.0),
        ],
    );
}

fn blend_sample(surface: &mut Surface, x: usize, y: usize, sample: [f64; 4]) {
    let dst = surface.get(vm::u2i(x), vm::u2i(y));
    let source = (sample[3] / 255.0).clamp(0.0, 1.0);
    let backdrop = f64::from(dst[3]) / 255.0;
    let out_alpha = source + backdrop * (1.0 - source);
    if out_alpha <= 0.0 {
        return;
    }
    let mix = |from: f64, to: u8| {
        clamp_u8((from * source + f64::from(to) * backdrop * (1.0 - source)) / out_alpha)
    };
    surface.set(
        vm::u2i(x),
        vm::u2i(y),
        [
            mix(sample[0], dst[0]),
            mix(sample[1], dst[1]),
            mix(sample[2], dst[2]),
            clamp_u8(out_alpha * 255.0),
        ],
    );
}

fn fill_band(surface: &mut Surface, x0: f64, x1: f64, y: f64, thickness: usize, color: Rgba) {
    let start_x = vm::i2u(vm::f2i(x0)).min(surface.width().saturating_sub(1));
    let end_x = vm::i2u(vm::f2i(x1)).min(surface.width());
    let top = vm::i2u(vm::f2i(y));
    for py in top..top.saturating_add(thickness).min(surface.height()) {
        for px in start_x..end_x {
            surface.set(vm::u2i(px), vm::u2i(py), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_ps::board::base_layer;
    use open_canvas_ps::doc::CanvasPsLayerKind;
    use open_canvas_ps::path::{create_path, path_anchor};
    use open_canvas_ps::{Bounds, CanvasPsTextCase};

    fn text_layer(text: &str) -> CanvasPsLayer {
        let mut layer = base_layer(
            "t".into(),
            "Text",
            CanvasPsLayerKind::Text,
            Bounds::new(0.0, 0.0, 96.0, 40.0),
        );
        layer.text = Some(text.to_string());
        layer.font_size = Some(24.0);
        layer.color = Some("#ff0000".to_string());
        layer
    }

    fn rasterizer() -> Result<GlyphRasterizer, Box<dyn std::error::Error>> {
        GlyphRasterizer::embedded().ok_or_else(|| "embedded font".into())
    }

    fn render(
        rasterizer: &GlyphRasterizer,
        layer: &CanvasPsLayer,
        width: usize,
        height: usize,
        paths: &[CanvasPsPath],
    ) -> Surface {
        rasterizer
            .rasterize(layer, paths, width, height, 1.0)
            .unwrap_or_else(|| Surface::new(width, height))
    }

    fn opaque_pixels(surface: &Surface) -> usize {
        surface
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    }

    fn red_pixels(surface: &Surface) -> usize {
        surface
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 80)
            .count()
    }

    #[test]
    fn embedded_face_renders_visible_coloured_glyphs() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let layer = text_layer("A");
        let surface = render(&rasterizer, &layer, 96, 40, &[]);
        assert!(opaque_pixels(&surface) > 20, "the glyph covers pixels");
        assert!(red_pixels(&surface) > 20, "the glyph uses the layer colour");
        let invisible = render(&rasterizer, &text_layer(""), 96, 40, &[]);
        assert_eq!(
            opaque_pixels(&invisible),
            0,
            "empty text rasterises nothing"
        );
        Ok(())
    }

    #[test]
    fn case_tracking_and_decorations_change_the_raster() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let plain = text_layer("ab");
        let plain_surface = render(&rasterizer, &plain, 96, 40, &[]);
        let mut upper = plain.clone();
        upper.text_case = Some(CanvasPsTextCase::Upper);
        let upper_surface = render(&rasterizer, &upper, 96, 40, &[]);
        assert_ne!(
            plain_surface.data(),
            upper_surface.data(),
            "case changes glyphs"
        );

        let mut tracked = plain.clone();
        tracked.tracking = Some(500.0);
        let tracked_surface = render(&rasterizer, &tracked, 128, 40, &[]);
        assert!(rightmost_ink(&tracked_surface) > rightmost_ink(&plain_surface));

        let mut underlined = plain.clone();
        underlined.underline = Some(true);
        let underlined_surface = render(&rasterizer, &underlined, 96, 40, &[]);
        assert!(opaque_pixels(&underlined_surface) > opaque_pixels(&plain_surface));
        Ok(())
    }

    #[test]
    fn paragraph_alignment_moves_the_line() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let mut layer = text_layer("ab");
        let left = render(&rasterizer, &layer, 96, 40, &[]);
        layer.paragraph = Some(CanvasPsTextParagraph {
            align: CanvasPsTextAlign::Right,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            hyphenate: false,
        });
        let right = render(&rasterizer, &layer, 96, 40, &[]);
        assert!(
            leftmost_ink(&right) > leftmost_ink(&left),
            "right align shifts ink"
        );
        Ok(())
    }

    #[test]
    fn faux_style_and_scale_change_the_raster() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let plain = text_layer("W");
        let base = render(&rasterizer, &plain, 96, 40, &[]);
        let mut bold = plain.clone();
        bold.faux_bold = Some(true);
        let bold_surface = render(&rasterizer, &bold, 96, 40, &[]);
        assert!(
            opaque_pixels(&bold_surface) > opaque_pixels(&base),
            "bold thickens"
        );

        let mut italic = plain.clone();
        italic.faux_italic = Some(true);
        let italic_surface = render(&rasterizer, &italic, 96, 40, &[]);
        assert_ne!(base.data(), italic_surface.data(), "italic shears");

        let mut scaled = plain.clone();
        scaled.text_scale_x = Some(150.0);
        let scaled_surface = render(&rasterizer, &scaled, 128, 40, &[]);
        assert!(rightmost_ink(&scaled_surface) > rightmost_ink(&base));
        Ok(())
    }

    #[test]
    fn warp_and_text_on_path_place_glyphs() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let mut warped = text_layer("arc");
        let mut warp = open_canvas_ps::text::text_warp_default();
        warp.bend = 100.0;
        warped.text_warp = Some(warp);
        let warped_surface = render(&rasterizer, &warped, 96, 48, &[]);
        assert!(opaque_pixels(&warped_surface) > 0, "warped text draws");
        let plain = render(&rasterizer, &text_layer("arc"), 96, 48, &[]);
        assert!(plain.data() != warped_surface.data(), "bend moves pixels");

        let path = create_path(
            "p",
            "line",
            vec![path_anchor(4.0, 30.0), path_anchor(90.0, 30.0)],
            false,
        );
        let mut on_path = text_layer("ab");
        on_path.text_path_id = Some("p".to_string());
        let on_path_surface = render(&rasterizer, &on_path, 96, 48, &[path]);
        assert!(opaque_pixels(&on_path_surface) > 0, "path text draws");
        Ok(())
    }

    #[test]
    fn trailing_whitespace_does_not_add_ink() -> Result<(), Box<dyn std::error::Error>> {
        let rasterizer = rasterizer()?;
        let mut layer = text_layer("A   ");
        let spaced = render(&rasterizer, &layer, 128, 40, &[]);
        layer.text = Some("A".to_string());
        let tight = render(&rasterizer, &layer, 128, 40, &[]);
        assert_eq!(opaque_pixels(&spaced), opaque_pixels(&tight));
        Ok(())
    }

    fn leftmost_ink(surface: &Surface) -> usize {
        (0..surface.width())
            .find(|x| (0..surface.height()).any(|y| surface.get(vm::u2i(*x), vm::u2i(y))[3] > 0))
            .unwrap_or(surface.width())
    }

    fn rightmost_ink(surface: &Surface) -> usize {
        (0..surface.width())
            .rev()
            .find(|x| (0..surface.height()).any(|y| surface.get(vm::u2i(*x), vm::u2i(y))[3] > 0))
            .unwrap_or(0)
    }
}
