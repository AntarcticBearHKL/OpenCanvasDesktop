//! Image Studio asset store + the gradient, pattern and brush panels — the
//! Rust half of `web/src/stores/use-ps-asset-store.ts` and
//! `web/src/components/canvas/workspace/ps-asset-panels.tsx`.
//!
//! Presets (swatches, gradients, patterns, brushes, actions) are app state, not
//! document state, so they persist inside `images\<id>.json` next to the board
//! (the web keeps them in localforage under `open-canvas:ps_asset_store`).
//! Pattern tiles reuse the image file's embedded bitmap map under
//! `image:pattern-<id>` keys, so a saved pattern travels with the project.
//! Owns the preset store (`ImageAssets`) and its persistence.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use open_canvas_ps::{hex_to_rgba, Rgba, Surface};

use crate::app::App;
use crate::image::ImageTool;
use crate::vm;
use crate::AppWindow;

/// `PsGradientStopPreset` (use-ps-asset-store.ts:8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsGradientStop {
    pub color: String,
    pub position: f64,
    pub midpoint: f64,
}

impl PsGradientStop {
    pub fn new(color: &str, position: f64, midpoint: f64) -> Self {
        Self {
            color: color.to_string(),
            position,
            midpoint,
        }
    }
}

/// `PsGradientPreset` (use-ps-asset-store.ts:9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsGradientPreset {
    pub id: String,
    pub name: String,
    pub stops: Vec<PsGradientStop>,
}

/// `PsPatternPreset` (use-ps-asset-store.ts:10); `key` names the tile bitmap in
/// the image file's bitmap map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsPatternPreset {
    pub id: String,
    pub name: String,
    pub key: String,
    pub width: usize,
    pub height: usize,
}

/// `PsBrushPreset` (use-ps-asset-store.ts:11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsBrushPreset {
    pub id: String,
    pub name: String,
    pub size: f64,
    pub hardness: f64,
    pub opacity: f64,
    pub spacing: f64,
    pub scatter: f64,
    pub angle: f64,
    pub roundness: f64,
    pub dynamics: f64,
    pub texture: f64,
}

/// `PsActionStep` (use-ps-asset-store.ts:12), flattened to `kind`/`command`/`arg`
/// (the desktop port records menu and layer commands).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsActionStep {
    pub kind: String,
    pub command: String,
    pub arg: String,
}

/// `PsAction` (use-ps-asset-store.ts:17).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsAction {
    pub id: String,
    pub name: String,
    pub steps: Vec<PsActionStep>,
    pub created_at: String,
}

/// The persisted asset store (`PsAssetStore` minus the transient API).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageAssets {
    pub swatches: Vec<String>,
    pub gradients: Vec<PsGradientPreset>,
    pub patterns: Vec<PsPatternPreset>,
    pub brushes: Vec<PsBrushPreset>,
    pub actions: Vec<PsAction>,
}

/// `DEFAULT_SWATCHES` (use-ps-asset-store.ts:42).
pub fn default_swatches() -> Vec<String> {
    [
        "#000000", "#ffffff", "#ff0000", "#ff7f00", "#ffff00", "#00ff00", "#00ffff", "#0000ff",
        "#7f00ff", "#ff00ff", "#7f7f7f", "#c0c0c0",
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect()
}

impl Default for ImageAssets {
    /// `DEFAULT_GRADIENTS` + `DEFAULT_BRUSHES` (use-ps-asset-store.ts:44-48);
    /// the ids are deterministic so a default store round-trips unchanged.
    fn default() -> Self {
        let gradient = |id: &str, name: &str, stops: &[(&str, f64, f64)]| PsGradientPreset {
            id: id.to_string(),
            name: name.to_string(),
            stops: stops
                .iter()
                .map(|(color, position, midpoint)| PsGradientStop::new(color, *position, *midpoint))
                .collect(),
        };
        let brush = |id: &str, name: &str, size: f64, hardness: f64, opacity: f64, spacing: f64| {
            PsBrushPreset {
                id: id.to_string(),
                name: name.to_string(),
                size,
                hardness,
                opacity,
                spacing,
                scatter: 0.0,
                angle: 0.0,
                roundness: 1.0,
                dynamics: 0.0,
                texture: 0.0,
            }
        };
        Self {
            swatches: default_swatches(),
            gradients: vec![
                gradient(
                    "ps-gradient-1",
                    "Black & White",
                    &[("#000000", 0.0, 0.5), ("#ffffff", 1.0, 0.5)],
                ),
                gradient(
                    "ps-gradient-2",
                    "Rainbow",
                    &[
                        ("#ff0000", 0.0, 0.5),
                        ("#ffff00", 0.33, 0.5),
                        ("#00ff00", 0.66, 0.5),
                        ("#0000ff", 1.0, 0.5),
                    ],
                ),
                gradient(
                    "ps-gradient-3",
                    "Sunset",
                    &[
                        ("#2b1055", 0.0, 0.4),
                        ("#ff6b6b", 0.55, 0.5),
                        ("#ffd93d", 1.0, 0.5),
                    ],
                ),
                gradient(
                    "ps-gradient-4",
                    "Steel",
                    &[
                        ("#0f2027", 0.0, 0.5),
                        ("#2c5364", 0.5, 0.5),
                        ("#a8c0cc", 1.0, 0.5),
                    ],
                ),
            ],
            patterns: Vec::new(),
            brushes: vec![
                {
                    let mut preset = brush("ps-brush-1", "Hard Round", 24.0, 1.0, 1.0, 0.15);
                    preset.hardness = 1.0;
                    preset
                },
                brush("ps-brush-2", "Soft Round", 60.0, 0.25, 0.9, 0.12),
                {
                    let mut preset = brush("ps-brush-3", "Calligraphy", 32.0, 0.8, 1.0, 0.08);
                    preset.angle = 45.0;
                    preset.roundness = 0.25;
                    preset
                },
                {
                    let mut preset = brush("ps-brush-4", "Spray", 48.0, 0.4, 0.6, 0.25);
                    preset.scatter = 0.6;
                    preset.dynamics = 0.4;
                    preset.texture = 0.5;
                    preset
                },
            ],
            actions: Vec::new(),
        }
    }
}

/// `PS_BRUSH_DEFAULT` (use-ps-asset-store.ts:156): the fields beyond the tool
/// options bar (size/hardness/opacity live in [`crate::image::ImageState`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushTool {
    pub spacing: f64,
    pub scatter: f64,
    pub angle: f64,
    pub roundness: f64,
    pub dynamics: f64,
    pub texture: f64,
}

impl Default for BrushTool {
    fn default() -> Self {
        Self {
            spacing: 0.15,
            scatter: 0.0,
            angle: 0.0,
            roundness: 1.0,
            dynamics: 0.0,
            texture: 0.0,
        }
    }
}

/// One resource-pool row: an image (or pixel) layer of another image project.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAssetRow {
    pub project_id: String,
    pub project_title: String,
    pub layer_id: String,
    pub layer_name: String,
    pub key: String,
    pub thumb: Surface,
}

/// The recorder state (`usePsActionRecorder`): recording flag + captured steps.
#[derive(Debug, Clone, Default)]
pub struct ActionRecorder {
    pub recording: bool,
    pub steps: Vec<PsActionStep>,
    /// Set while a saved action replays so playback never re-records.
    pub replaying: bool,
}

impl ActionRecorder {
    /// Captures `step` while recording (the web's `record`).
    pub fn record(&mut self, kind: &str, command: &str, arg: &str) {
        if self.recording && !self.replaying {
            self.steps.push(PsActionStep {
                kind: kind.to_string(),
                command: command.to_string(),
                arg: arg.to_string(),
            });
        }
    }
}

/// The gradient editor's in-progress stops (`PsGradientsPanel` state).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientEdit {
    pub preset_id: String,
    pub name: String,
    pub stops: Vec<PsGradientStop>,
    pub selected: usize,
}

impl Default for GradientEdit {
    fn default() -> Self {
        Self {
            preset_id: String::new(),
            name: String::new(),
            stops: vec![
                PsGradientStop::new("#000000", 0.0, 0.5),
                PsGradientStop::new("#ffffff", 1.0, 0.5),
            ],
            selected: 0,
        }
    }
}

impl GradientEdit {
    pub fn to_preset(&self, id: &str, name: &str) -> PsGradientPreset {
        PsGradientPreset {
            id: id.to_string(),
            name: name.to_string(),
            stops: self.stops.clone(),
        }
    }

    /// The stop the editor's colour/position/midpoint rows address.
    pub fn selected(&self) -> Option<&PsGradientStop> {
        self.stops
            .get(self.selected.min(self.stops.len().saturating_sub(1)))
    }
}

/// `psMixColor` (use-ps-asset-store.ts:69): a straight RGB mix of two hex
/// colours (alpha is dropped, as in the web).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "channel mixes are clamped to 0..=255 before the u8 casts"
)]
pub fn ps_mix_hex(from: &str, to: &str, ratio: f64) -> String {
    let parse = |hex: &str| -> [f64; 3] {
        let [r, g, b, _] = hex_to_rgba(hex);
        [f64::from(r), f64::from(g), f64::from(b)]
    };
    let a = parse(from);
    let b = parse(to);
    let ratio = ratio.clamp(0.0, 1.0);
    let channel = |index: usize| {
        let mixed = a[index] + (b[index] - a[index]) * ratio;
        format!("{:02x}", mixed.round().clamp(0.0, 255.0) as u8)
    };
    format!("#{}{}{}", channel(0), channel(1), channel(2))
}

/// `psExpandGradientStops` (use-ps-asset-store.ts:82): midpoints become real
/// stops so the fill, gradient tool and gradient overlay keep taking plain stops.
pub fn ps_expand_gradient_stops(stops: &[PsGradientStop]) -> Vec<(String, f64)> {
    let mut sorted: Vec<&PsGradientStop> = stops.iter().collect();
    sorted.sort_by(|a, b| {
        a.position
            .partial_cmp(&b.position)
            .unwrap_or(Ordering::Equal)
    });
    let mut out: Vec<(String, f64)> = Vec::new();
    for (index, stop) in sorted.iter().enumerate() {
        out.push((stop.color.clone(), stop.position.clamp(0.0, 1.0)));
        if let Some(next) = sorted.get(index + 1) {
            let midpoint = stop.midpoint.clamp(0.0, 1.0);
            if midpoint > 0.0 && midpoint < 1.0 && next.position > stop.position {
                out.push((
                    ps_mix_hex(&stop.color, &next.color, midpoint),
                    stop.position + (next.position - stop.position) * midpoint,
                ));
            }
        }
    }
    out
}

/// Samples an expanded stop list at `t` (0..1) with a straight RGBA lerp;
/// `hex_to_rgba` carries the 8-digit `#rrggbb00` fade the dynamic presets use.
pub fn sample_gradient_stops(stops: &[(String, f64)], t: f64) -> Rgba {
    if stops.is_empty() {
        return [0, 0, 0, 0];
    }
    let t = t.clamp(0.0, 1.0);
    let mut lower = &stops[0];
    let mut upper = stops.last().unwrap_or(&stops[0]);
    for stop in stops {
        if stop.1 <= t {
            lower = stop;
        }
        if stop.1 >= t {
            upper = stop;
            break;
        }
    }
    let span = upper.1 - lower.1;
    let local = if span > 1e-9 {
        (t - lower.1) / span
    } else {
        0.0
    };
    let a = hex_to_rgba(&lower.0);
    let b = hex_to_rgba(&upper.0);
    lerp_rgba(a, b, local)
}

/// Straight RGBA lerp with rounding (the shared gradient sampler).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "channel lerps are clamped to 0..=255 before the u8 casts"
)]
pub fn lerp_rgba(from: Rgba, to: Rgba, t: f64) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let mix =
        |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).clamp(0.0, 255.0) as u8;
    [
        mix(from[0], to[0]),
        mix(from[1], to[1]),
        mix(from[2], to[2]),
        mix(from[3], to[3]),
    ]
}

/// A `width x height` strip of the expanded gradient (the panel's preview bar).
pub fn gradient_strip(stops: &[PsGradientStop], width: usize, height: usize) -> Surface {
    let expanded = ps_expand_gradient_stops(stops);
    let mut strip = Surface::new(width.max(1), height.max(1));
    for y in 0..strip.height() {
        for x in 0..strip.width() {
            let t = if strip.width() <= 1 {
                0.0
            } else {
                vm::i2f(vm::u2i(x)) / vm::i2f(vm::u2i(strip.width() - 1))
            };
            strip.set(vm::u2i(x), vm::u2i(y), sample_gradient_stops(&expanded, t));
        }
    }
    strip
}

/// Crops `source` to `(x, y, width, height)` clamped to the surface bounds.
pub fn crop_surface(source: &Surface, x: i64, y: i64, width: i64, height: i64) -> Surface {
    let max_x = vm::u2i(source.width());
    let max_y = vm::u2i(source.height());
    let x0 = x.clamp(0, max_x);
    let y0 = y.clamp(0, max_y);
    let x1 = (x + width.max(0)).clamp(x0, max_x);
    let y1 = (y + height.max(0)).clamp(y0, max_y);
    let out_w = vm::i2u(x1 - x0).max(1);
    let out_h = vm::i2u(y1 - y0).max(1);
    let mut out = Surface::new(out_w, out_h);
    for oy in 0..out_h {
        for ox in 0..out_w {
            out.set(
                vm::u2i(ox),
                vm::u2i(oy),
                source.get(x0 + vm::u2i(ox), y0 + vm::u2i(oy)),
            );
        }
    }
    out
}

/// Tiles `tile` across `target` from `(0, 0)` (`ps-paint.ts` pattern fill).
pub fn tile_fill(target: &mut Surface, tile: &Surface) {
    if tile.width() == 0 || tile.height() == 0 {
        return;
    }
    let tw = vm::u2i(tile.width());
    let th = vm::u2i(tile.height());
    for y in 0..target.height() {
        for x in 0..target.width() {
            let tx = vm::i2u(vm::u2i(x) % tw);
            let ty = vm::i2u(vm::u2i(y) % th);
            let source = tile.get(vm::u2i(tx), vm::u2i(ty));
            if source[3] > 0 {
                target.set(vm::u2i(x), vm::u2i(y), source);
            }
        }
    }
}

/// Scales `source` into a `max`-bounded thumbnail (resource-pool rows).
pub fn thumbnail(source: &Surface, max: usize) -> Surface {
    let longest = source.width().max(source.height()).max(1);
    if longest <= max {
        return source.clone();
    }
    let scale = vm::i2f(vm::u2i(max)) / vm::i2f(vm::u2i(longest));
    let width = vm::i2u(vm::f2i(vm::i2f(vm::u2i(source.width())) * scale)).max(1);
    let height = vm::i2u(vm::f2i(vm::i2f(vm::u2i(source.height())) * scale)).max(1);
    source.resample_bilinear(width, height)
}

/// `psBrushPreview` (paint.ts): a dab stroke of the current brush settings.
pub fn brush_preview(
    size: f64,
    hardness: f64,
    opacity: f64,
    tool: BrushTool,
    color: &str,
    width: usize,
    height: usize,
) -> Surface {
    let mut preview = Surface::new(width.max(1), height.max(1));
    let radius = (size / 2.0).clamp(0.75, vm::i2f(vm::u2i(height)) / 2.0);
    let rgba = hex_to_rgba(color);
    let steps = 18;
    for step in 0..steps {
        let t = vm::i2f(step) / vm::i2f(steps - 1);
        let cx = radius + t * (vm::i2f(vm::u2i(width)) - radius * 2.0);
        let cy = vm::i2f(vm::u2i(height)) / 2.0;
        stamp_preview_dab(
            &mut preview,
            cx,
            cy,
            radius,
            hardness,
            opacity,
            tool,
            rgba,
            step,
        );
    }
    preview
}

/// One preview dab: elliptical (roundness/angle), scattered, texture-modulated.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_arguments,
    reason = "the dab parameters mirror the brush record; coverage channels are clamped before casting"
)]
fn stamp_preview_dab(
    target: &mut Surface,
    cx: f64,
    cy: f64,
    radius: f64,
    hardness: f64,
    opacity: f64,
    tool: BrushTool,
    rgba: Rgba,
    index: i64,
) {
    let angle = tool.angle.to_radians();
    let (sin, cos) = angle.sin_cos();
    let roundness = tool.roundness.clamp(0.05, 1.0);
    let dynamics = tool.dynamics.clamp(0.0, 1.0);
    let wobble = 1.0 + dynamics * 0.6 * ((vm::i2f(index) * 1.7).sin());
    let radius = radius * wobble;
    let scatter = tool.scatter.clamp(0.0, 1.0) * radius;
    let hash = (vm::i2f(index) * 12.9898).sin() * 43_758.545_312_3;
    let offset_x = scatter * (hash - hash.floor() - 0.5) * 2.0;
    let offset_y = scatter * (((hash * 1.37).sin() * 0.5) * 2.0 - 0.5);
    let (cx, cy) = (cx + offset_x, cy + offset_y);
    let hard = radius * hardness.clamp(0.0, 1.0);
    let texture = tool.texture.clamp(0.0, 1.0);
    let x0 = vm::f2i((cx - radius).floor()).max(0);
    let y0 = vm::f2i((cy - radius).floor()).max(0);
    let x1 = vm::f2i((cx + radius).ceil()).min(vm::u2i(target.width()));
    let y1 = vm::f2i((cy + radius).ceil()).min(vm::u2i(target.height()));
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = vm::i2f(x) + 0.5 - cx;
            let dy = vm::i2f(y) + 0.5 - cy;
            let rx = (cos * dx + sin * dy) / roundness.max(0.05);
            let ry = -sin * dx + cos * dy;
            let distance = (rx * rx + ry * ry).sqrt();
            if distance > radius {
                continue;
            }
            let mut coverage = if distance <= hard || radius <= hard {
                1.0
            } else {
                1.0 - (distance - hard) / (radius - hard).max(1e-6)
            };
            if texture > 0.0 {
                let grain = 0.55 + 0.45 * ((vm::i2f(x) * 0.7 + vm::i2f(y) * 0.9).sin() * 0.5 + 0.5);
                coverage *= 1.0 - texture + texture * grain;
            }
            let coverage = coverage.clamp(0.0, 1.0) * opacity.clamp(0.0, 1.0);
            let existing = target.get(x, y);
            let out_a = coverage + f64::from(existing[3]) / 255.0 * (1.0 - coverage);
            if out_a <= 0.0 {
                continue;
            }
            let mix = |index: usize| {
                let src = f64::from(rgba[index]) * coverage;
                let dst = f64::from(existing[index])
                    * (f64::from(existing[3]) / 255.0)
                    * (1.0 - coverage);
                ((src + dst) / out_a).clamp(0.0, 255.0) as u8
            };
            target.set(
                x,
                y,
                [mix(0), mix(1), mix(2), (out_a * 255.0).round() as u8],
            );
        }
    }
}

/// The brush preset for the current tool options (the panel's "save" source).
pub fn current_brush_preset(
    id: &str,
    name: &str,
    size: f64,
    hardness: f64,
    opacity: f64,
    tool: BrushTool,
) -> PsBrushPreset {
    PsBrushPreset {
        id: id.to_string(),
        name: name.to_string(),
        size,
        hardness,
        opacity,
        spacing: tool.spacing,
        scatter: tool.scatter,
        angle: tool.angle,
        roundness: tool.roundness,
        dynamics: tool.dynamics,
        texture: tool.texture,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "flat panel dispatch mirroring ps-asset-panels.tsx handlers"
)]
impl App {
    /// Gradient panel actions (`PsGradientsPanel` + the gradient tool pick).
    pub fn image_gradient_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_gradient_action_core(action, id);
        self.sync_image(ui);
    }

    /// The gradient dispatch without a window (unit tests drive this directly).
    pub fn image_gradient_action_core(&mut self, action: &str, id: &str) {
        match action {
            "pick" => {
                let Some(preset) = self
                    .image
                    .assets
                    .gradients
                    .iter()
                    .find(|preset| preset.id == id)
                    .cloned()
                else {
                    return;
                };
                self.image.gradient_edit = GradientEdit {
                    preset_id: preset.id.clone(),
                    name: preset.name,
                    stops: preset.stops,
                    selected: 0,
                };
                self.image.gradient_picked_id = preset.id;
                self.sync_gradient_pick();
            }
            "pick-dynamic" => {
                let fg = self.image.color.clone();
                let bg = self.image.bg_color.clone();
                let (name, stops) = if id == "transparent" {
                    let faded = format!("{fg}00");
                    (
                        "Foreground to Transparent",
                        vec![
                            PsGradientStop::new(&fg, 0.0, 0.5),
                            PsGradientStop::new(&faded, 1.0, 0.5),
                        ],
                    )
                } else {
                    (
                        "Foreground to Background",
                        vec![
                            PsGradientStop::new(&fg, 0.0, 0.5),
                            PsGradientStop::new(&bg, 1.0, 0.5),
                        ],
                    )
                };
                self.image.gradient_edit = GradientEdit {
                    preset_id: String::new(),
                    name: name.to_string(),
                    stops,
                    selected: 0,
                };
                self.image.gradient_picked_id = id.to_string();
                self.sync_gradient_pick();
            }
            "stop-select" => {
                if let Ok(index) = id.parse::<usize>() {
                    if index < self.image.gradient_edit.stops.len() {
                        self.image.gradient_edit.selected = index;
                    }
                }
            }
            "stop-add" => {
                let position = id.parse::<f64>().unwrap_or(0.5).clamp(0.0, 1.0);
                let color = self
                    .image
                    .gradient_edit
                    .selected()
                    .map_or_else(|| self.image.color.clone(), |stop| stop.color.clone());
                self.image
                    .gradient_edit
                    .stops
                    .push(PsGradientStop::new(&color, position, 0.5));
                self.image.gradient_edit.selected = self.image.gradient_edit.stops.len() - 1;
                self.sync_gradient_pick();
            }
            "stop-remove" => {
                if self.image.gradient_edit.stops.len() > 2 {
                    let edit = &mut self.image.gradient_edit;
                    let index = edit.selected.min(edit.stops.len() - 1);
                    edit.stops.remove(index);
                    edit.selected = index.min(edit.stops.len() - 1);
                    self.sync_gradient_pick();
                }
            }
            "stop-color" => {
                let color = if id.is_empty() {
                    self.image.color.clone()
                } else {
                    open_canvas_ps::color::normalize_hex(id)
                };
                if color.is_empty() {
                    return;
                }
                let selected = self.image.gradient_edit.selected;
                if let Some(stop) = self.image.gradient_edit.stops.get_mut(selected) {
                    stop.color = color;
                }
                self.sync_gradient_pick();
            }
            "stop-position" => {
                if let Ok(value) = id.parse::<f64>() {
                    let selected = self.image.gradient_edit.selected;
                    if let Some(stop) = self.image.gradient_edit.stops.get_mut(selected) {
                        stop.position = (value / 100.0).clamp(0.0, 1.0);
                    }
                    self.sync_gradient_pick();
                }
            }
            "stop-midpoint" => {
                if let Ok(value) = id.parse::<f64>() {
                    let selected = self.image.gradient_edit.selected;
                    if let Some(stop) = self.image.gradient_edit.stops.get_mut(selected) {
                        stop.midpoint = (value / 100.0).clamp(0.01, 0.99);
                    }
                    self.sync_gradient_pick();
                }
            }
            "name" => {
                self.image.gradient_edit.name = id.to_string();
            }
            "save" => {
                let name = self.image.gradient_edit.name.trim().to_string();
                let name = if name.is_empty() {
                    format!("Gradient {}", self.image.assets.gradients.len() + 1)
                } else {
                    name
                };
                let id = if self.image.gradient_edit.preset_id.is_empty() {
                    self.mint_image_id("ps-gradient")
                } else {
                    self.image.gradient_edit.preset_id.clone()
                };
                let preset = self.image.gradient_edit.to_preset(&id, &name);
                let gradients = &mut self.image.assets.gradients;
                match gradients.iter_mut().find(|item| item.id == id) {
                    Some(existing) => *existing = preset,
                    None => gradients.push(preset),
                }
                self.image.gradient_edit.preset_id = id.clone();
                self.image.gradient_edit.name = name;
                self.image.gradient_picked_id = id;
                self.sync_gradient_pick();
                self.note_doc_change();
            }
            "delete" => {
                self.image.assets.gradients.retain(|preset| preset.id != id);
                if self.image.gradient_picked_id == id {
                    self.image.gradient_picked_id.clear();
                    self.image.gradient_picked.clear();
                }
                self.note_doc_change();
            }
            _ => {}
        }
    }

    /// Recomputes the picked gradient's expanded stops from the editor state.
    fn sync_gradient_pick(&mut self) {
        self.image.gradient_picked = ps_expand_gradient_stops(&self.image.gradient_edit.stops);
    }

    /// Pattern panel actions (`PsPatternsPanel`).
    pub fn image_pattern_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_pattern_action_core(action, id);
        self.sync_image(ui);
    }

    /// The pattern dispatch without a window (unit tests drive this directly);
    /// returns `false` when "define" had no usable target.
    pub fn image_pattern_action_core(&mut self, action: &str, id: &str) -> bool {
        match action {
            "define" => {
                if !self.image_pattern_define() {
                    return false;
                }
            }
            "pick" => {
                if self
                    .image
                    .assets
                    .patterns
                    .iter()
                    .any(|preset| preset.id == id)
                {
                    self.image.pattern_picked = id.to_string();
                    self.image.tool = ImageTool::Bucket;
                }
            }
            "fill" => self.image_pattern_fill(),
            "delete" => {
                let key = self
                    .image
                    .assets
                    .patterns
                    .iter()
                    .find(|preset| preset.id == id)
                    .map(|preset| preset.key.clone());
                self.image.assets.patterns.retain(|preset| preset.id != id);
                if let Some(key) = key {
                    self.image.bitmaps.remove(&key);
                }
                if self.image.pattern_picked == id {
                    self.image.pattern_picked.clear();
                }
                self.note_doc_change();
            }
            _ => return false,
        }
        true
    }

    /// `PsPatternsPanel.define`: crop the selection (composite) or the active
    /// layer bitmap into a new tile preset. Returns `false` when the board has
    /// no usable target (no selection and no pixel/image layer).
    pub fn image_pattern_define(&mut self) -> bool {
        let selection_bounds = self
            .image
            .selection
            .as_ref()
            .and_then(open_canvas_ps::selection_bounds);
        let (tile, from_selection) = if let Some(bounds) = selection_bounds {
            let Some(composite) = self.image_composite() else {
                return false;
            };
            (
                crop_surface(
                    &composite,
                    vm::f2i(bounds.x.floor()),
                    vm::f2i(bounds.y.floor()),
                    vm::f2i(bounds.width.ceil()),
                    vm::f2i(bounds.height.ceil()),
                ),
                true,
            )
        } else {
            let active = self.image.active_layer_id();
            let Some(key) = self.image.ensure_layer_bitmap(&active) else {
                return false;
            };
            let Some(bitmap) = self.image.bitmaps.get(&key).cloned() else {
                return false;
            };
            (bitmap, false)
        };
        if tile.is_empty() {
            return false;
        }
        let id = self.mint_image_id("ps-pattern");
        let key = format!("image:pattern-{id}");
        let name = if from_selection {
            format!("Selection {}", self.image.assets.patterns.len() + 1)
        } else {
            format!("Pattern {}", self.image.assets.patterns.len() + 1)
        };
        let preset = PsPatternPreset {
            id,
            name,
            key: key.clone(),
            width: tile.width(),
            height: tile.height(),
        };
        self.image.assets.patterns.insert(0, preset);
        self.image.bitmaps.insert(key, tile);
        self.image
            .pattern_picked
            .clone_from(&self.image.assets.patterns[0].id);
        self.note_doc_change();
        true
    }

    /// `PsPatternsPanel.onFill`: tile the picked pattern over the active layer.
    fn image_pattern_fill(&mut self) {
        let Some(preset) = self
            .image
            .assets
            .patterns
            .iter()
            .find(|preset| preset.id == self.image.pattern_picked)
            .cloned()
        else {
            return;
        };
        let Some(tile) = self.image.bitmaps.get(&preset.key).cloned() else {
            return;
        };
        let active = self.image.active_layer_id();
        let Some(key) = self.image.ensure_layer_bitmap(&active) else {
            return;
        };
        self.image.begin_edit(std::slice::from_ref(&key));
        if let Some(mut bitmap) = self.image.bitmaps.remove(&key) {
            tile_fill(&mut bitmap, &tile);
            self.image.bitmaps.insert(key, bitmap);
        }
        self.finish_image("Pattern Fill");
    }

    /// Brush panel actions (`PsBrushesPanel`).
    pub fn image_brush_action(&mut self, ui: &AppWindow, action: &str, id: &str) {
        self.image_brush_action_core(action, id);
        self.sync_image(ui);
    }

    /// The brush dispatch without a window (unit tests drive this directly).
    pub fn image_brush_action_core(&mut self, action: &str, id: &str) {
        match action {
            "pick" => {
                let Some(preset) = self
                    .image
                    .assets
                    .brushes
                    .iter()
                    .find(|preset| preset.id == id)
                    .cloned()
                else {
                    return;
                };
                self.apply_brush_preset(&preset);
                self.image.brush_picked = preset.id;
            }
            "save" => {
                let name = if id.trim().is_empty() {
                    format!("Brush {}", self.image.assets.brushes.len() + 1)
                } else {
                    id.trim().to_string()
                };
                let preset_id = self.mint_image_id("ps-brush-preset");
                let preset = current_brush_preset(
                    &preset_id,
                    &name,
                    self.image.brush_size,
                    self.image.brush_hardness,
                    self.image.brush_opacity,
                    self.image.brush_tool,
                );
                self.image.assets.brushes.push(preset);
                self.image.brush_picked = preset_id;
                self.note_doc_change();
            }
            "delete" => {
                self.image.assets.brushes.retain(|preset| preset.id != id);
                if self.image.brush_picked == id {
                    self.image.brush_picked.clear();
                }
                self.note_doc_change();
            }
            "param" => {
                if let Some((key, value)) = id.split_once('=') {
                    let Ok(value) = value.parse::<f64>() else {
                        return;
                    };
                    self.set_brush_param(key, value);
                }
            }
            _ => {}
        }
    }

    /// Brush panel sliders (`key=value` pushed through the brush dispatch).
    pub fn image_brush_slider(&mut self, ui: &AppWindow, key: &str, value: f64) {
        self.image_brush_action_core("param", &format!("{key}={value}"));
        self.sync_image(ui);
    }

    /// Applies a brush preset to the live tool options (`PsBrushesPanel.onPick`).
    pub fn apply_brush_preset(&mut self, preset: &PsBrushPreset) {
        self.image.brush_size = preset.size;
        self.image.brush_hardness = preset.hardness;
        self.image.brush_opacity = preset.opacity;
        self.image.brush_tool = BrushTool {
            spacing: preset.spacing,
            scatter: preset.scatter,
            angle: preset.angle,
            roundness: preset.roundness,
            dynamics: preset.dynamics,
            texture: preset.texture,
        };
    }

    /// One brush slider: `size`, `hardness`, `opacity`, `spacing`, `scatter`,
    /// `angle`, `roundness`, `dynamics`, `texture` (values in UI units).
    fn set_brush_param(&mut self, key: &str, value: f64) {
        match key {
            "size" => self.image.brush_size = value.round().clamp(1.0, 500.0),
            "hardness" => self.image.brush_hardness = (value / 100.0).clamp(0.0, 1.0),
            "opacity" => self.image.brush_opacity = (value / 100.0).clamp(0.0, 1.0),
            "spacing" => self.image.brush_tool.spacing = (value / 100.0).clamp(0.01, 2.0),
            "scatter" => self.image.brush_tool.scatter = (value / 100.0).clamp(0.0, 1.0),
            "angle" => self.image.brush_tool.angle = value.clamp(-180.0, 180.0),
            "roundness" => self.image.brush_tool.roundness = (value / 100.0).clamp(0.05, 1.0),
            "dynamics" => self.image.brush_tool.dynamics = (value / 100.0).clamp(0.0, 1.0),
            "texture" => self.image.brush_tool.texture = (value / 100.0).clamp(0.0, 1.0),
            _ => return,
        }
        // Editing while a preset is picked syncs the preset too (the web panel
        // edits the preset's own fields).
        let picked = self.image.brush_picked.clone();
        if !picked.is_empty() {
            if let Some(preset) = self
                .image
                .assets
                .brushes
                .iter_mut()
                .find(|preset| preset.id == picked)
            {
                preset.size = self.image.brush_size;
                preset.hardness = self.image.brush_hardness;
                preset.opacity = self.image.brush_opacity;
                preset.spacing = self.image.brush_tool.spacing;
                preset.scatter = self.image.brush_tool.scatter;
                preset.angle = self.image.brush_tool.angle;
                preset.roundness = self.image.brush_tool.roundness;
                preset.dynamics = self.image.brush_tool.dynamics;
                preset.texture = self.image.brush_tool.texture;
            }
        }
    }

    /// The resource pool's search text (`PsResourcePoolPanel`).
    pub fn image_asset_search(&mut self, ui: &AppWindow, text: &str) {
        self.image_asset_search_core(text);
        self.sync_image(ui);
    }

    /// The search field without a window (unit tests drive this directly).
    pub fn image_asset_search_core(&mut self, text: &str) {
        self.image.asset_search = text.to_string();
    }

    /// Scans every other image project for image/pixel layers (the web walks all
    /// canvases' image nodes; here those are the library's image projects).
    pub fn image_asset_refresh(&mut self, ui: &AppWindow) {
        self.image_asset_refresh_core();
        self.sync_image(ui);
    }

    /// The resource-pool scan without a window (unit tests drive this directly).
    pub fn image_asset_refresh_core(&mut self) {
        let current = self
            .image
            .meta
            .as_ref()
            .map_or_else(String::new, |meta| meta.id.clone());
        let mut rows: Vec<ImageAssetRow> = Vec::new();
        for entry in &self.library.index.projects {
            if !crate::image::is_image_kind(&entry.kind) || entry.id == current || rows.len() >= 60
            {
                continue;
            }
            let Ok(file) = self.store.load_image(&entry.id) else {
                continue;
            };
            let Ok((project, bitmaps, _assets)) = file.into_state() else {
                continue;
            };
            for layer in &project.board_layers {
                if rows.len() >= 60 {
                    break;
                }
                if !matches!(
                    layer.kind,
                    open_canvas_ps::CanvasPsLayerKind::Image
                        | open_canvas_ps::CanvasPsLayerKind::Pixel
                ) {
                    continue;
                }
                let Some(key) = layer.storage_key.clone() else {
                    continue;
                };
                let Some(bitmap) = bitmaps.get(&key) else {
                    continue;
                };
                rows.push(ImageAssetRow {
                    project_id: entry.id.clone(),
                    project_title: project.title.clone(),
                    layer_id: layer.id.clone(),
                    layer_name: layer.name.clone(),
                    key,
                    thumb: thumbnail(bitmap, 96),
                });
            }
        }
        self.image.asset_rows = rows;
        self.image.asset_loaded = true;
    }

    /// Adds a resource-pool layer to the board as a new image layer (a copy, so
    /// editing it never touches the source project).
    pub fn image_asset_add(&mut self, ui: &AppWindow, project_id: &str, layer_id: &str) {
        if let Err(message) = self.image_asset_add_core(project_id, layer_id) {
            self.set_notice(ui, message, true);
            return;
        }
        self.sync_image(ui);
    }

    /// The resource-pool add without a window (unit tests drive this directly).
    pub fn image_asset_add_core(&mut self, project_id: &str, layer_id: &str) -> Result<(), String> {
        self.insert_asset(project_id, layer_id, None)
    }

    /// Drops a resource-pool row at board point `(x, y)`: a new image layer,
    /// decoded from its source project, sized to its aspect ratio, centred on
    /// the drop point and selected.
    pub fn image_asset_drop(
        &mut self,
        ui: &AppWindow,
        project_id: &str,
        layer_id: &str,
        x: f64,
        y: f64,
    ) {
        let point = self.image_doc_point(x, y);
        let (board_w, board_h) = self.image.board();
        if point.x < 0.0 || point.y < 0.0 || point.x > board_w || point.y > board_h {
            return;
        }
        if let Err(message) = self.image_asset_drop_core(project_id, layer_id, point.x, point.y) {
            self.set_notice(ui, message, true);
            return;
        }
        self.sync_image(ui);
    }

    /// The drop without a window (unit tests drive this directly).
    pub fn image_asset_drop_core(
        &mut self,
        project_id: &str,
        layer_id: &str,
        x: f64,
        y: f64,
    ) -> Result<(), String> {
        self.insert_asset(project_id, layer_id, Some((x, y)))
    }

    fn insert_asset(
        &mut self,
        project_id: &str,
        layer_id: &str,
        at: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let Some((name, source)) = self.load_asset_source(project_id, layer_id) else {
            return Err("The source image project could not be read".to_string());
        };
        let board = self.image.board();
        let mut gen = self.image_gen("ps-image");
        let layer = open_canvas_ps::create_ps_image_layer(
            &mut gen,
            board,
            &name,
            (
                vm::i2f(vm::u2i(source.width())),
                vm::i2f(vm::u2i(source.height())),
            ),
        );
        let new_id = layer.id.clone();
        let layer_w = layer.width.max(1.0);
        let layer_h = layer.height.max(1.0);
        let new_key = format!("image:{new_id}");
        self.image.bitmaps.insert(new_key.clone(), source);
        let layers = open_canvas_ps::add_ps_layer(&self.image.project.board_layers, layer, None);
        self.image.project.board_layers =
            open_canvas_ps::patch_ps_layer(&layers, &new_id, |layer| {
                layer.storage_key = Some(new_key.clone());
                if let Some((x, y)) = at {
                    // Centre the image on the drop point, keeping it on the board.
                    layer.x = (x - layer_w / 2.0).clamp(0.0, (board.0 - layer_w).max(0.0));
                    layer.y = (y - layer_h / 2.0).clamp(0.0, (board.1 - layer_h).max(0.0));
                }
            });
        self.image.active_layer = new_id;
        self.finish_image("Add Image Layer");
        Ok(())
    }

    /// Loads one resource-pool layer's bitmap out of its source project file.
    fn load_asset_source(&self, project_id: &str, layer_id: &str) -> Option<(String, Surface)> {
        let file = self.store.load_image(project_id).ok()?;
        let (project, bitmaps, _assets) = file.into_state().ok()?;
        let layer = project
            .board_layers
            .iter()
            .find(|layer| layer.id == layer_id)?;
        let key = layer.storage_key.clone()?;
        let source = bitmaps.get(&key)?.clone();
        Some((layer.name.clone(), source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use open_canvas_ps::{Bounds, CanvasPsLayerKind, IdGen, ImageProject};

    fn checker_board() -> (ImageProject, HashMap<String, Surface>) {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.title = "Board".to_string();
        let mut gen = IdGen::new("t");
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (8.0, 8.0), "Layer 1");
        layer.storage_key = Some("image:t-1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(8, 8);
        for y in 0..4 {
            for x in 0..4 {
                let red = (x / 2 + y / 2) % 2 == 0;
                surface.set(
                    x,
                    y,
                    if red {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    },
                );
            }
        }
        bitmaps.insert("image:t-1".to_string(), surface);
        (project, bitmaps)
    }

    fn assets_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let (project, bitmaps) = checker_board();
        crate::test_support::app_with_image(project, bitmaps)
    }

    #[test]
    fn gradient_name_edited_then_saved_uses_the_name() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        app.image_gradient_action_core("pick-dynamic", "pair");
        app.image_panel_name_edited_core("Sunset");
        assert_eq!(app.image.gradient_edit.name, "Sunset");
        app.image_gradient_action_core("save", "");
        let saved = app.image.assets.gradients.last().ok_or("saved gradient")?;
        assert_eq!(saved.name, "Sunset", "the panel name field feeds the save");
        Ok(())
    }

    #[test]
    fn gradient_expansion_inserts_midpoints_with_mixed_colors() {
        let stops = vec![
            PsGradientStop::new("#000000", 0.0, 0.5),
            PsGradientStop::new("#ffffff", 1.0, 0.5),
        ];
        let expanded = ps_expand_gradient_stops(&stops);
        assert_eq!(expanded.len(), 3, "one midpoint becomes a real stop");
        assert_eq!(expanded[0], ("#000000".to_string(), 0.0));
        assert_eq!(expanded[1], ("#808080".to_string(), 0.5));
        assert_eq!(expanded[2], ("#ffffff".to_string(), 1.0));

        // A midpoint at 0 or 1 disappears (the web's `midpoint > 0 && < 1`).
        let flat = vec![
            PsGradientStop::new("#ff0000", 0.0, 0.0),
            PsGradientStop::new("#00ff00", 0.5, 0.5),
        ];
        assert_eq!(ps_expand_gradient_stops(&flat).len(), 2);
    }

    #[test]
    fn picking_a_saved_gradient_sets_the_expanded_tool_stops(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        app.image_gradient_action_core("pick", "ps-gradient-2");
        assert_eq!(
            app.image.gradient_edit.stops.len(),
            4,
            "rainbow has 4 stops"
        );
        assert_eq!(app.image.gradient_picked.len(), 7, "3 midpoints expand");
        assert_eq!(app.image.gradient_picked_id, "ps-gradient-2");
        Ok(())
    }

    #[test]
    fn gradient_editor_edits_and_save_update_the_store() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        app.image_gradient_action_core("pick", "ps-gradient-3");
        let before = app.image.assets.gradients.len();

        app.image_gradient_action_core("stop-add", "0.5");
        assert_eq!(app.image.gradient_edit.stops.len(), 4);
        app.image_gradient_action_core("stop-position", "75");
        app.image_gradient_action_core("stop-midpoint", "25");
        app.image_gradient_action_core("stop-color", "#112233");
        let selected = app.image.gradient_edit.selected;
        let stop = app.image.gradient_edit.stops.get(selected).cloned();
        let stop = stop.ok_or("selected stop")?;
        assert!((stop.position - 0.75).abs() < 1e-9, "{stop:?}");
        assert!((stop.midpoint - 0.25).abs() < 1e-9, "{stop:?}");
        assert_eq!(stop.color, "#112233");
        assert!(
            !app.image.gradient_picked.is_empty(),
            "edits refresh the pick"
        );

        app.image_gradient_action_core("stop-remove", "");
        assert_eq!(app.image.gradient_edit.stops.len(), 3);
        // A gradient never drops below two stops.
        app.image_gradient_action_core("stop-remove", "");
        assert_eq!(app.image.gradient_edit.stops.len(), 2);

        // Saving a loaded preset updates it in place (the web's
        // `saveGradient({ id: editingId })`).
        app.image_gradient_action_core("save", "");
        assert_eq!(app.image.assets.gradients.len(), before, "update in place");
        let saved = app
            .image
            .assets
            .gradients
            .iter()
            .find(|preset| preset.id == "ps-gradient-3");
        assert!(
            saved.is_some_and(|preset| preset.stops.len() == 2),
            "edited stops saved"
        );

        // Saving a dynamic pick (no preset id) appends a new preset.
        app.image_gradient_action_core("pick-dynamic", "pair");
        app.image_gradient_action_core("save", "");
        assert_eq!(app.image.assets.gradients.len(), before + 1);
        assert!(app.image.gradient_picked_id.starts_with("ps-gradient"));
        Ok(())
    }

    #[test]
    fn gradient_tool_fill_samples_the_picked_multi_stop_gradient(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        app.image.gradient_picked = vec![
            ("#ff0000".to_string(), 0.0),
            ("#00ff00".to_string(), 0.5),
            ("#0000ff".to_string(), 1.0),
        ];
        app.image.tool = ImageTool::Gradient;
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        app.image_pointer_core(
            crate::input::PointerKind::Down,
            0.0,
            0.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Move,
            7.0,
            0.0,
            crate::input::Button::Left,
            false,
            false,
        );
        app.image_pointer_core(
            crate::input::PointerKind::Up,
            7.0,
            0.0,
            crate::input::Button::Left,
            false,
            false,
        );
        let bitmap = app.image.bitmaps.get("image:t-1");
        let mid = bitmap.map(|bitmap| bitmap.get(3, 0)).unwrap_or_default();
        assert_eq!(mid, [0, 255, 0, 255], "t = 0.5 hits the middle stop");
        let start = bitmap.map(|bitmap| bitmap.get(0, 0)).unwrap_or_default();
        assert!(
            start[0] > 200 && start[1] < 60,
            "the first pixel is still red-dominant: {start:?}"
        );
        let end = bitmap.map(|bitmap| bitmap.get(7, 0)).unwrap_or_default();
        assert!(
            end[2] > 200 && end[1] < 60,
            "the last pixel is blue-dominant: {end:?}"
        );
        assert_eq!(app.image.history.label(1), Some("Gradient"));
        Ok(())
    }

    #[test]
    fn pattern_defined_from_a_layer_tiles_and_round_trips() -> Result<(), Box<dyn std::error::Error>>
    {
        let (mut app, _dir) = assets_board_app()?;
        assert!(app.image_pattern_action_core("define", ""));
        let preset = app.image.assets.patterns.first().cloned();
        let preset = preset.ok_or("pattern preset")?;
        assert_eq!(
            (preset.width, preset.height),
            (8, 8),
            "the layer is the tile"
        );
        assert!(app.image.bitmaps.contains_key(&preset.key));

        // Pick + fill a fresh layer, then assert the tiled pixels.
        app.image_layer_action_core("add-pixel", "");
        let fill_id = app.image.active_layer.clone();
        assert!(app.image_pattern_action_core("pick", &preset.id.clone()));
        assert_eq!(app.image.tool, ImageTool::Bucket, "pick arms the bucket");
        assert!(app.image_pattern_action_core("fill", ""));
        let key = format!("image:{fill_id}");
        let bitmap = app.image.bitmaps.get(&key);
        assert_eq!(
            bitmap.map(|bitmap| bitmap.get(0, 0)),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            bitmap.map(|bitmap| bitmap.get(2, 0)),
            Some([0, 0, 255, 255])
        );
        assert_eq!(app.image.history.label(2), Some("Pattern Fill"));

        assert!(app.image_pattern_action_core("delete", &preset.id.clone()));
        assert!(app.image.assets.patterns.is_empty(), "preset removed");
        assert!(
            !app.image.bitmaps.contains_key(&preset.key),
            "tile bitmap dropped"
        );
        assert!(app.image.pattern_picked.is_empty(), "pick cleared");
        Ok(())
    }

    #[test]
    fn pattern_define_from_a_selection_crops_the_composite(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        app.with_selection(|selection| {
            open_canvas_ps::selection_rect(
                selection,
                Bounds::new(2.0, 2.0, 2.0, 2.0),
                false,
                0.0,
                false,
                open_canvas_ps::SelectionMode::Replace,
            );
        });
        assert!(app.image_pattern_action_core("define", ""));
        let preset = app.image.assets.patterns.first().cloned();
        let preset = preset.ok_or("pattern preset")?;
        assert_eq!((preset.width, preset.height), (2, 2));
        assert!(preset.name.starts_with("Selection"), "{}", preset.name);
        Ok(())
    }

    #[test]
    fn brush_presets_save_pick_and_edit_through_sliders() -> Result<(), Box<dyn std::error::Error>>
    {
        let (mut app, _dir) = assets_board_app()?;
        app.image_brush_action_core("pick", "ps-brush-3");
        assert!((app.image.brush_size - 32.0).abs() < 1e-9);
        assert!((app.image.brush_tool.angle - 45.0).abs() < 1e-9);
        assert!((app.image.brush_tool.roundness - 0.25).abs() < 1e-9);

        app.image_brush_action_core("param", "size=100");
        app.image_brush_action_core("param", "roundness=50");
        app.image_brush_action_core("param", "spacing=30");
        assert!((app.image.brush_size - 100.0).abs() < 1e-9);
        assert!((app.image.brush_tool.roundness - 0.5).abs() < 1e-9);
        assert!((app.image.brush_tool.spacing - 0.3).abs() < 1e-9);
        let picked = app
            .image
            .assets
            .brushes
            .iter()
            .find(|preset| preset.id == "ps-brush-3");
        assert!(
            picked.is_some_and(|preset| (preset.roundness - 0.5).abs() < 1e-9),
            "slider edits sync the picked preset"
        );

        let before = app.image.assets.brushes.len();
        app.image_brush_action_core("save", "My Brush");
        assert_eq!(app.image.assets.brushes.len(), before + 1);
        assert!(app
            .image
            .assets
            .brushes
            .iter()
            .any(|preset| preset.name == "My Brush"));
        app.image_brush_action_core("delete", &app.image.brush_picked.clone());
        assert_eq!(app.image.assets.brushes.len(), before);
        Ok(())
    }

    #[test]
    fn brush_preview_paints_a_dab_stroke_in_the_brush_color() {
        let preview = brush_preview(24.0, 1.0, 1.0, BrushTool::default(), "#ff0000", 64, 24);
        assert_eq!((preview.width(), preview.height()), (64, 24));
        let centre = preview.get(20, 12);
        assert_eq!(centre, [255, 0, 0, 255], "{centre:?}");
        assert_eq!(
            preview.get(0, 0)[3],
            0,
            "outside the stroke stays transparent"
        );

        let soft = brush_preview(24.0, 0.0, 1.0, BrushTool::default(), "#ff0000", 64, 24);
        let hard = brush_preview(24.0, 1.0, 1.0, BrushTool::default(), "#ff0000", 64, 24);
        let soft_edge = soft.get(20, 13)[3];
        let hard_edge = hard.get(20, 13)[3];
        assert!(
            hard_edge >= soft_edge,
            "hardness keeps the core opaque: {hard_edge} vs {soft_edge}"
        );
    }

    #[test]
    fn resource_pool_lists_another_project_and_adds_a_copy(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-2".to_string();
        project.title = "Other".to_string();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            CanvasPsLayerKind::Pixel,
            Bounds::new(0.0, 0.0, 4.0, 4.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        let mut surface = Surface::new(4, 4);
        surface.set(1, 1, [12, 200, 30, 255]);
        bitmaps.insert("image:l1".to_string(), surface);
        let file = crate::image::ImageFile::from_state(&project, &bitmaps, &ImageAssets::default())
            .map_err(std::io::Error::other)?;
        app.store.save_image(&file).map_err(std::io::Error::other)?;
        app.library
            .index
            .upsert_entry(open_canvas_core::document::ProjectIndexEntry {
                id: "img-2".to_string(),
                title: "Other".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
                group_id: None,
                kind: crate::image::PROJECT_KIND_IMAGE.to_string(),
            });

        app.image_asset_refresh_core();
        assert_eq!(
            app.image.asset_rows.len(),
            1,
            "the other project's layer is listed"
        );
        assert_eq!(app.image.asset_rows[0].layer_id, "l1");
        app.image_asset_search_core("other");
        assert_eq!(app.image.asset_search, "other");

        let before = app.image.project.board_layers.len();
        app.image_asset_add_core("img-2", "l1")?;
        assert_eq!(app.image.project.board_layers.len(), before + 1);
        let added = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("added layer")?;
        assert_eq!(added.kind, CanvasPsLayerKind::Image);
        let key = added.storage_key.clone().ok_or("storage key")?;
        assert_eq!(
            app.image.bitmaps.get(&key).map(|bitmap| bitmap.get(1, 1)),
            Some([12, 200, 30, 255]),
            "the source bitmap is copied under the new layer key"
        );
        assert_eq!(app.image.history.label(1), Some("Add Image Layer"));
        Ok(())
    }

    #[test]
    fn a_pool_drop_centres_the_new_layer_on_the_pointer() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 64.0;
        project.height = 32.0;
        app.enter_image(project, HashMap::new(), ImageAssets::default());

        let mut source_project = crate::image::ImageState::default().project;
        source_project.id = "img-2".to_string();
        source_project.title = "Other".to_string();
        let mut layer = open_canvas_ps::base_layer(
            "l1".to_string(),
            "Paint",
            CanvasPsLayerKind::Pixel,
            Bounds::new(0.0, 0.0, 10.0, 5.0),
        );
        layer.storage_key = Some("image:l1".to_string());
        source_project.board_layers = vec![layer];
        let mut bitmaps = HashMap::new();
        bitmaps.insert("image:l1".to_string(), Surface::new(10, 5));
        let file =
            crate::image::ImageFile::from_state(&source_project, &bitmaps, &ImageAssets::default())
                .map_err(std::io::Error::other)?;
        app.store.save_image(&file).map_err(std::io::Error::other)?;

        app.image_asset_drop_core("img-2", "l1", 32.0, 16.0)?;
        let added = app
            .image
            .project
            .board_layers
            .last()
            .cloned()
            .ok_or("dropped layer")?;
        assert_eq!(added.kind, CanvasPsLayerKind::Image);
        assert_eq!(
            app.image.active_layer, added.id,
            "the drop selects the layer"
        );
        assert!(
            (added.width / added.height - 2.0).abs() < 0.01,
            "the layer keeps the source aspect ({added:?})"
        );
        assert!(
            (added.x + added.width / 2.0 - 32.0).abs() < 0.5,
            "centred on x"
        );
        assert!(
            (added.y + added.height / 2.0 - 16.0).abs() < 0.5,
            "centred on y"
        );
        Ok(())
    }

    #[test]
    fn asset_store_persists_with_the_image_file() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = assets_board_app()?;
        assert!(app.image_pattern_action_core("define", ""));
        app.image_gradient_action_core("pick", "ps-gradient-1");
        app.image_gradient_action_core("save", "Mine");
        app.image_brush_action_core("save", "Custom");
        let gradients = app.image.assets.gradients.len();
        let brushes = app.image.assets.brushes.len();

        app.image_save_now().map_err(std::io::Error::other)?;
        let file = app
            .store
            .load_image("img-1")
            .map_err(std::io::Error::other)?;
        let (_project, bitmaps, assets) = file.into_state().map_err(std::io::Error::other)?;
        assert_eq!(assets.gradients.len(), gradients);
        assert_eq!(assets.brushes.len(), brushes);
        assert_eq!(assets.patterns.len(), 1);
        let pattern = assets.patterns.first().ok_or("pattern")?;
        assert!(
            bitmaps.contains_key(&pattern.key),
            "the pattern tile travels in the bitmap map"
        );
        Ok(())
    }
}
