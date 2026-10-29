//! Text tool panel params: character (tracking/kerning/leading/baseline
//! shift/scales/case), paragraph (align/indents/spacing/hyphenate),
//! text-on-path and text warp - the Rust half of `ps-text-panel.tsx`.
//! Owns the text tool parameters and their panel rows.

use open_canvas_ps::doc::{CanvasPsTextAlign, CanvasPsTextCase, CanvasPsTextParagraph};
use open_canvas_ps::text::text_warp_default;
use open_canvas_ps::{patch_ps_layer, CanvasPsLayer, CanvasPsLayerKind, CanvasPsTextWarpStyle};

use crate::app::App;
use crate::AppWindow;

/// The layer's paragraph, defaulted the way `psTextParagraph` does.
pub fn paragraph_of(layer: &CanvasPsLayer) -> CanvasPsTextParagraph {
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

impl App {
    /// Double-click on the board: with the polygon lasso it closes the polygon
    /// (the existing gesture); otherwise it opens the in-canvas editor for the
    /// topmost text layer under the pointer.
    pub fn image_canvas_double_click(&mut self, ui: &AppWindow, x: f64, y: f64) {
        if self.image.tool == crate::image::ImageTool::PolygonLasso {
            self.image_finish_polygon(ui);
            return;
        }
        self.image_text_edit_begin(ui, x, y);
    }

    /// Opens the text overlay for the topmost visible, unlocked text layer at
    /// screen point `(x, y)`; returns whether an editor opened.
    pub fn image_text_edit_begin(&mut self, ui: &AppWindow, x: f64, y: f64) -> bool {
        if self.image.meta.is_none() || self.image.mask_view {
            return false;
        }
        let point = self.image_doc_point(x, y);
        let target = self
            .image
            .project
            .board_layers
            .iter()
            .rev()
            .find(|layer| {
                if layer.kind != CanvasPsLayerKind::Text || layer.hidden || layer.locked {
                    return false;
                }
                let box_ = open_canvas_ps::layer_box(&self.image.project.board_layers, layer);
                point.x >= box_.x
                    && point.x <= box_.x + box_.width
                    && point.y >= box_.y
                    && point.y <= box_.y + box_.height
            })
            .cloned();
        let Some(layer) = target else {
            return false;
        };
        if self.image.text_edit.is_editing_layer(&layer.id) {
            return false;
        }
        let content = layer.text.clone().unwrap_or_default();
        self.image.active_layer.clone_from(&layer.id);
        if !self.image.text_edit.begin(&layer.id, &content) {
            return false;
        }
        self.sync_image(ui);
        true
    }

    /// One keystroke in the overlay (kept off the document until commit).
    pub fn image_text_edit_changed(&mut self, text: &str) {
        if let Some(layer_id) = self
            .image
            .text_edit
            .open()
            .map(|edit| edit.layer_id.clone())
        {
            let _changed = self.image.text_edit.change(&layer_id, text);
        }
    }

    /// Commits the open overlay through the properties panel's own writer.
    pub fn image_text_edit_commit(&mut self, ui: &AppWindow, text: Option<&str>) {
        let Some(committed) = self.image.text_edit.commit(text) else {
            self.sync_image(ui);
            return;
        };
        self.image.active_layer = committed.layer_id;
        let _written = self.image_text_row_text_core("text-content", &committed.text);
        self.sync_image(ui);
    }

    /// Escape / view switch: closes the overlay without touching the document.
    pub fn image_text_edit_cancel(&mut self, ui: &AppWindow) {
        if self.image.text_edit.cancel().is_some() {
            self.sync_image(ui);
        }
    }

    /// The overlay's board-area rectangle for the edited layer at the current
    /// view: `(x, y, width, height)` in screen pixels.
    pub fn image_text_edit_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let edit = self.image.text_edit.open()?;
        let layer = open_canvas_ps::layer_ops::find_ps_layer(
            &self.image.project.board_layers,
            &edit.layer_id,
        )?;
        let box_ = open_canvas_ps::layer_box(&self.image.project.board_layers, layer);
        let view = self.image.view;
        Some((
            view.x + box_.x * view.k,
            view.y + box_.y * view.k,
            (box_.width * view.k).max(24.0),
            (box_.height * view.k).max(24.0),
        ))
    }

    /// One select-style field: text case, paragraph align, text path, warp, the
    /// text panel's tab and the shape layer's kind.
    pub fn image_text_option(&mut self, ui: &AppWindow, key: &str, value: &str) {
        if self.image_text_option_core(key, value) {
            self.sync_image(ui);
        }
    }

    /// The select without a window (unit tests drive this directly).
    pub fn image_text_option_core(&mut self, key: &str, value: &str) -> bool {
        match key {
            "text-tab" => {
                let Some(tab) = crate::image::TextPanelTab::from_id(value) else {
                    return false;
                };
                self.image.text_tab = tab;
                true
            }
            "shape-kind" => {
                let Some(kind) = shape_kind_from_label(value) else {
                    return false;
                };
                self.patch_active_shape(move |layer| layer.shape = Some(kind))
            }
            "text-case" => {
                let case = labelled(&TEXT_CASE_LABELS, value).unwrap_or(CanvasPsTextCase::None);
                self.patch_active_text(move |layer| layer.text_case = Some(case))
            }
            "align" => {
                let align = labelled(&TEXT_ALIGN_LABELS, value).unwrap_or(CanvasPsTextAlign::Left);
                self.patch_active_text(move |layer| {
                    let mut paragraph = paragraph_of(layer);
                    paragraph.align = align;
                    layer.paragraph = Some(paragraph);
                })
            }
            "text-path" => {
                let path = (!value.is_empty()).then(|| value.to_string());
                self.patch_active_text(move |layer| layer.text_path_id.clone_from(&path))
            }
            "warp" => {
                if value.is_empty() {
                    return self.patch_active_text(|layer| layer.text_warp = None);
                }
                let Some(style) = labelled(&TEXT_WARP_LABELS, value) else {
                    return false;
                };
                self.patch_active_text(move |layer| {
                    let mut warp = layer.text_warp.clone().unwrap_or_else(text_warp_default);
                    warp.style = style;
                    layer.text_warp = Some(warp);
                })
            }
            _ => false,
        }
    }

    /// One shared editor-row slider (number rows and the choice rows, whose
    /// value arrives as the picked index).
    pub fn image_text_row_set(&mut self, ui: &AppWindow, key: &str, value: f64) {
        if self.image_text_row_set_core(key, value) {
            self.sync_image(ui);
        }
    }

    /// The row slider without a window (unit tests drive this directly).
    pub fn image_text_row_set_core(&mut self, key: &str, value: f64) -> bool {
        match key {
            "warp" => {
                let index = i32::try_from(crate::vm::f2i(value.round())).unwrap_or(i32::MAX);
                match warp_choice(index) {
                    Some(style) => {
                        let label = TEXT_WARP_LABELS
                            .iter()
                            .find(|(_, entry)| *entry == style)
                            .map_or("", |(label, _)| *label);
                        self.image_text_option_core("warp", label)
                    }
                    None => self.image_text_option_core("warp", ""),
                }
            }
            "text-path" => {
                let index = i32::try_from(crate::vm::f2i(value.round())).unwrap_or(i32::MAX);
                let paths = self.image.project.board_paths.clone().unwrap_or_default();
                let id = path_choice(&paths, index);
                self.patch_active_text(move |layer| layer.text_path_id.clone_from(&id))
            }
            "warp-bend" | "warp-horizontal" | "warp-vertical" => {
                self.image_text_warp_core(key, value)
            }
            "shape-stroke-width" => self.patch_active_shape(move |layer| {
                layer.shape_stroke_width = Some(value.clamp(0.0, 64.0));
            }),
            "shape-radius" => self.patch_active_shape(move |layer| {
                layer.shape_radius = Some(value.clamp(0.0, 200.0));
            }),
            "shape-sides" => self.patch_active_shape(move |layer| {
                layer.shape_sides = Some(value.round().clamp(3.0, 24.0));
            }),
            _ => {
                let Some(patch) = parse_number_row(key, value) else {
                    return false;
                };
                match patch {
                    TextPatch::Number(slot, value) => {
                        self.patch_active_text(move |layer| apply_number(layer, slot, value))
                    }
                    TextPatch::FontSize(size) => {
                        self.patch_active_text(move |layer| layer.font_size = Some(size))
                    }
                    TextPatch::Paragraph(slot, value) => self.patch_active_text(move |layer| {
                        let mut paragraph = paragraph_of(layer);
                        slot.apply(&mut paragraph, value);
                        layer.paragraph = Some(paragraph);
                    }),
                }
            }
        }
    }

    /// One shared editor-row text field (`text-color` hex, the content).
    pub fn image_text_row_text(&mut self, ui: &AppWindow, key: &str, text: &str) {
        if self.image_text_row_text_core(key, text) {
            self.sync_image(ui);
        }
    }

    /// The row text field without a window (unit tests drive this directly).
    pub fn image_text_row_text_core(&mut self, key: &str, text: &str) -> bool {
        match key {
            "shape-fill" | "shape-stroke" => {
                let hex = open_canvas_ps::color::normalize_hex(text);
                if hex.is_empty() {
                    return false;
                }
                let fill = key == "shape-fill";
                self.patch_active_shape(move |layer| {
                    if fill {
                        layer.shape_fill = Some(hex.clone());
                    } else {
                        layer.shape_stroke = Some(hex.clone());
                    }
                })
            }
            "text-color" => {
                let hex = open_canvas_ps::color::normalize_hex(text);
                if hex.is_empty() {
                    return false;
                }
                self.patch_active_text(move |layer| layer.color = Some(hex.clone()))
            }
            "text-content" => {
                let content = text.to_string();
                self.patch_active_text(move |layer| layer.text = Some(content.clone()))
            }
            _ => false,
        }
    }

    /// One shared editor-row checkbox (the character toggles).
    pub fn image_text_row_check(&mut self, ui: &AppWindow, key: &str, checked: bool) {
        if self.image_text_row_check_core(key, checked) {
            self.sync_image(ui);
        }
    }

    /// The row checkbox without a window (unit tests drive this directly).
    pub fn image_text_row_check_core(&mut self, key: &str, checked: bool) -> bool {
        let toggle = match key {
            "faux-bold" => ToggleSlot::FauxBold,
            "faux-italic" => ToggleSlot::FauxItalic,
            "underline" => ToggleSlot::Underline,
            "strikethrough" => ToggleSlot::Strikethrough,
            "hyphenate" => ToggleSlot::Hyphenate,
            _ => return false,
        };
        self.patch_active_text(move |layer| match toggle {
            ToggleSlot::FauxBold => layer.faux_bold = Some(checked),
            ToggleSlot::FauxItalic => layer.faux_italic = Some(checked),
            ToggleSlot::Underline => layer.underline = Some(checked),
            ToggleSlot::Strikethrough => layer.strikethrough = Some(checked),
            ToggleSlot::Hyphenate => {
                let mut paragraph = paragraph_of(layer);
                paragraph.hyphenate = checked;
                layer.paragraph = Some(paragraph);
            }
        })
    }

    /// The warp slider without a window (unit tests drive this directly).
    pub fn image_text_warp_core(&mut self, key: &str, value: f64) -> bool {
        let value = value.clamp(-100.0, 100.0);
        let slot = match key {
            "warp-bend" => TextWarpSlot::Bend,
            "warp-horizontal" => TextWarpSlot::Horizontal,
            "warp-vertical" => TextWarpSlot::Vertical,
            _ => return false,
        };
        self.patch_active_text(move |layer| {
            let mut warp = layer.text_warp.clone().unwrap_or_else(text_warp_default);
            match slot {
                TextWarpSlot::Bend => warp.bend = value,
                TextWarpSlot::Horizontal => warp.horizontal = value,
                TextWarpSlot::Vertical => warp.vertical = value,
            }
            layer.text_warp = Some(warp);
        })
    }

    /// Patches the active shape layer and commits a "Shape Properties" history
    /// entry. Only shape layers are accepted (the panel's shape section).
    fn patch_active_shape(&mut self, edit: impl FnOnce(&mut CanvasPsLayer)) -> bool {
        let active = self.image.active_layer_id();
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &active)
        else {
            return false;
        };
        if layer.kind != CanvasPsLayerKind::Shape {
            return false;
        }
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, edit);
        self.commit_image_core("Shape Properties");
        true
    }

    /// Patches the active text layer and commits an "Edit Text" history entry.
    fn patch_active_text(&mut self, edit: impl FnOnce(&mut CanvasPsLayer)) -> bool {
        let active = self.image.active_layer_id();
        let Some(layer) =
            open_canvas_ps::layer_ops::find_ps_layer(&self.image.project.board_layers, &active)
        else {
            return false;
        };
        if layer.kind != CanvasPsLayerKind::Text {
            return false;
        }
        self.image.project.board_layers =
            patch_ps_layer(&self.image.project.board_layers, &active, edit);
        self.commit_image_core("Edit Text");
        true
    }
}

/// The shape layer's type select options (display label -> kind), matching the
/// tool options bar's shape chips.
pub const SHAPE_KIND_LABELS: [(&str, open_canvas_ps::CanvasPsShapeKind); 5] = [
    ("Rectangle", open_canvas_ps::CanvasPsShapeKind::Rectangle),
    (
        "Rounded",
        open_canvas_ps::CanvasPsShapeKind::RoundedRectangle,
    ),
    ("Ellipse", open_canvas_ps::CanvasPsShapeKind::Ellipse),
    ("Polygon", open_canvas_ps::CanvasPsShapeKind::Polygon),
    ("Line", open_canvas_ps::CanvasPsShapeKind::Line),
];

/// The shape kind a type-row label selects (case/separator insensitive).
pub fn shape_kind_from_label(label: &str) -> Option<open_canvas_ps::CanvasPsShapeKind> {
    let normalize = |text: &str| text.to_ascii_lowercase().replace(['-', '_', ' '], "");
    let needle = normalize(label);
    SHAPE_KIND_LABELS
        .iter()
        .find_map(|(entry, kind)| (normalize(entry) == needle).then_some(*kind))
}

/// The character-tab numeric slots.
#[derive(Debug, Clone, Copy)]
enum TextNumber {
    Tracking,
    Kerning,
    Leading,
    BaselineShift,
    ScaleX,
    ScaleY,
}

fn apply_number(layer: &mut CanvasPsLayer, slot: TextNumber, value: f64) {
    match slot {
        TextNumber::Tracking => layer.tracking = Some(value),
        TextNumber::Kerning => layer.kerning = Some(value),
        TextNumber::Leading => layer.leading = Some(value),
        TextNumber::BaselineShift => layer.baseline_shift = Some(value),
        TextNumber::ScaleX => layer.text_scale_x = Some(value),
        TextNumber::ScaleY => layer.text_scale_y = Some(value),
    }
}

/// The warp-tab slider slots.
#[derive(Debug, Clone, Copy)]
enum TextWarpSlot {
    Bend,
    Horizontal,
    Vertical,
}

/// Display labels of the case select (`ps-text-panel.tsx`).
pub const TEXT_CASE_LABELS: [(&str, CanvasPsTextCase); 4] = [
    ("None", CanvasPsTextCase::None),
    ("Upper", CanvasPsTextCase::Upper),
    ("Lower", CanvasPsTextCase::Lower),
    ("Small Caps", CanvasPsTextCase::SmallCaps),
];

/// Display labels of the paragraph-align segmented control.
pub const TEXT_ALIGN_LABELS: [(&str, CanvasPsTextAlign); 4] = [
    ("Left", CanvasPsTextAlign::Left),
    ("Center", CanvasPsTextAlign::Center),
    ("Right", CanvasPsTextAlign::Right),
    ("Justify", CanvasPsTextAlign::Justify),
];

/// Display labels of the warp-style select (`PS_TEXT_WARP_NAME_KEYS`).
pub const TEXT_WARP_LABELS: [(&str, CanvasPsTextWarpStyle); 10] = [
    ("Arc", CanvasPsTextWarpStyle::Arc),
    ("Arc Lower", CanvasPsTextWarpStyle::ArcLower),
    ("Arc Upper", CanvasPsTextWarpStyle::ArcUpper),
    ("Flag", CanvasPsTextWarpStyle::Flag),
    ("Wave", CanvasPsTextWarpStyle::Wave),
    ("Fish", CanvasPsTextWarpStyle::Fish),
    ("Rise", CanvasPsTextWarpStyle::Rise),
    ("Bulge", CanvasPsTextWarpStyle::Bulge),
    ("Shell", CanvasPsTextWarpStyle::Shell),
    ("Squeeze", CanvasPsTextWarpStyle::Squeeze),
];

/// The label the select row shows for `value`; accepts the display label or
/// the kebab id, ignoring case and separators.
fn labelled<T: Copy + PartialEq>(table: &[(&'static str, T)], value: &str) -> Option<T> {
    let normalize = |text: &str| text.to_ascii_lowercase().replace(['-', '_', ' '], "");
    let needle = normalize(value);
    table
        .iter()
        .find_map(|(label, entry)| (normalize(label) == needle).then_some(*entry))
}

/// The properties panel's rows for the active layer: the text panel's rows for
/// a text layer (filtered by `tab`), the shape layer's property rows for a
/// shape, and nothing for other kinds.
pub fn layer_panel_rows(
    layer: Option<&CanvasPsLayer>,
    paths: &[open_canvas_ps::CanvasPsPath],
    tab: crate::image::TextPanelTab,
) -> Vec<crate::image::editor::EditorRow> {
    match layer.map(|layer| layer.kind) {
        Some(CanvasPsLayerKind::Text) => text_panel_rows(layer, paths, tab),
        Some(CanvasPsLayerKind::Shape) => layer.map_or_else(Vec::new, shape_panel_rows),
        _ => Vec::new(),
    }
}

/// The shape layer's editable properties (`ps-properties-panel.tsx`'s shape
/// section): type, fill, stroke, stroke width, corner radius and polygon sides.
pub fn shape_panel_rows(layer: &CanvasPsLayer) -> Vec<crate::image::editor::EditorRow> {
    use crate::image::editor::EditorRow;
    let kind_label = SHAPE_KIND_LABELS
        .iter()
        .find(|(_, kind)| Some(*kind) == layer.shape)
        .map_or("Rectangle", |(label, _)| *label);
    let options: Vec<&str> = SHAPE_KIND_LABELS.iter().map(|(label, _)| *label).collect();
    vec![
        EditorRow::select("shape-kind", "Type", &options, kind_label),
        EditorRow::color(
            "shape-fill",
            "Fill",
            layer.shape_fill.as_deref().unwrap_or("#000000"),
        ),
        EditorRow::color(
            "shape-stroke",
            "Stroke",
            layer.shape_stroke.as_deref().unwrap_or("#000000"),
        ),
        EditorRow::number(
            "shape-stroke-width",
            "Stroke W",
            layer.shape_stroke_width.unwrap_or(2.0),
            0.0,
            64.0,
        ),
        EditorRow::number(
            "shape-radius",
            "Radius",
            layer.shape_radius.unwrap_or(24.0),
            0.0,
            200.0,
        ),
        EditorRow::number(
            "shape-sides",
            "Sides",
            layer.shape_sides.unwrap_or(6.0),
            3.0,
            24.0,
        ),
    ]
}

/// The rows of the text tool panel (`ps-text-panel.tsx`): the Character /
/// Paragraph tab row, that tab's rows, then text-on-path and the text-warp
/// select with its sliders (always shown, like the web).
#[expect(
    clippy::too_many_lines,
    reason = "one flat row table for the text panel, mirroring the web panel's control list"
)]
pub fn text_panel_rows(
    layer: Option<&CanvasPsLayer>,
    paths: &[open_canvas_ps::CanvasPsPath],
    tab: crate::image::TextPanelTab,
) -> Vec<crate::image::editor::EditorRow> {
    let Some(layer) = layer.filter(|layer| layer.kind == CanvasPsLayerKind::Text) else {
        return Vec::new();
    };
    let paragraph = paragraph_of(layer);
    let number =
        |key: &str, label: &str, value: f64, min: f64, max: f64| crate::image::editor::EditorRow {
            key: key.to_string(),
            label: label.to_string(),
            kind: "number",
            value,
            min,
            max,
            text: String::new(),
            options: Vec::new(),
            checked: false,
        };
    let check = |key: &str, label: &str, checked: bool| crate::image::editor::EditorRow {
        key: key.to_string(),
        label: label.to_string(),
        kind: "check",
        value: if checked { 1.0 } else { 0.0 },
        min: 0.0,
        max: 1.0,
        text: String::new(),
        options: Vec::new(),
        checked,
    };
    let select =
        |key: &str, label: &str, options: &[&str], current: &str| crate::image::editor::EditorRow {
            key: key.to_string(),
            label: label.to_string(),
            kind: "select",
            value: 0.0,
            min: 0.0,
            max: 0.0,
            text: current.to_string(),
            options: options.iter().map(|option| (*option).to_string()).collect(),
            checked: false,
        };
    let choice = |key: &str, label: &str, value: f64, max: f64, text: &str| {
        crate::image::editor::EditorRow {
            key: key.to_string(),
            label: label.to_string(),
            kind: "choice",
            value,
            min: 0.0,
            max,
            text: text.to_string(),
            options: Vec::new(),
            checked: false,
        }
    };

    let case_options: Vec<&str> = TEXT_CASE_LABELS.iter().map(|(label, _)| *label).collect();
    let case_label = TEXT_CASE_LABELS
        .iter()
        .find(|(_, entry)| *entry == layer.text_case.unwrap_or(CanvasPsTextCase::None))
        .map_or("None", |(label, _)| *label);
    let align_options: Vec<&str> = TEXT_ALIGN_LABELS.iter().map(|(label, _)| *label).collect();
    let align_label = TEXT_ALIGN_LABELS
        .iter()
        .find(|(_, entry)| *entry == paragraph.align)
        .map_or("Left", |(label, _)| *label);

    let character = vec![
        crate::image::editor::EditorRow {
            key: "text-color".to_string(),
            label: "Color".to_string(),
            kind: "color",
            value: 0.0,
            min: 0.0,
            max: 0.0,
            text: layer.color.clone().unwrap_or_else(|| "#000000".to_string()),
            options: Vec::new(),
            checked: false,
        },
        number(
            "text-size",
            "Size",
            layer.font_size.unwrap_or(32.0),
            1.0,
            512.0,
        ),
        number(
            "tracking",
            "Tracking",
            layer.tracking.unwrap_or(0.0),
            -1000.0,
            5000.0,
        ),
        number(
            "kerning",
            "Kerning",
            layer.kerning.unwrap_or(0.0),
            -200.0,
            500.0,
        ),
        number(
            "leading",
            "Leading",
            layer.leading.unwrap_or(0.0),
            0.0,
            800.0,
        ),
        number(
            "baseline-shift",
            "Baseline",
            layer.baseline_shift.unwrap_or(0.0),
            -400.0,
            400.0,
        ),
        number(
            "scale-x",
            "Scale X",
            layer.text_scale_x.unwrap_or(100.0),
            1.0,
            1000.0,
        ),
        number(
            "scale-y",
            "Scale Y",
            layer.text_scale_y.unwrap_or(100.0),
            1.0,
            1000.0,
        ),
        check("faux-bold", "Bold", layer.faux_bold.unwrap_or(false)),
        check("faux-italic", "Italic", layer.faux_italic.unwrap_or(false)),
        check("underline", "Underline", layer.underline.unwrap_or(false)),
        check(
            "strikethrough",
            "Strike",
            layer.strikethrough.unwrap_or(false),
        ),
        select("text-case", "Case", &case_options, case_label),
    ];
    let paragraph_rows = vec![
        select("align", "Align", &align_options, align_label),
        number("indent-left", "Indent L", paragraph.indent_left, 0.0, 800.0),
        number(
            "indent-right",
            "Indent R",
            paragraph.indent_right,
            0.0,
            800.0,
        ),
        number(
            "indent-first",
            "First Line",
            paragraph.indent_first,
            0.0,
            800.0,
        ),
        number(
            "space-before",
            "Space Before",
            paragraph.space_before,
            0.0,
            800.0,
        ),
        number(
            "space-after",
            "Space After",
            paragraph.space_after,
            0.0,
            800.0,
        ),
        check("hyphenate", "Hyphenate", paragraph.hyphenate),
    ];
    // The Character / Paragraph tab row, then only the active tab's rows.
    let mut rows = vec![select(
        "text-tab",
        "",
        &["Character", "Paragraph"],
        match tab {
            crate::image::TextPanelTab::Character => "Character",
            crate::image::TextPanelTab::Paragraph => "Paragraph",
        },
    )];
    rows.extend(match tab {
        crate::image::TextPanelTab::Character => character,
        crate::image::TextPanelTab::Paragraph => paragraph_rows,
    });

    // Text on path: index 0 is "None", then every board path in order.
    let path_index = layer
        .text_path_id
        .as_deref()
        .and_then(|id| paths.iter().position(|path| path.id == id));
    let path_label = path_index
        .and_then(|index| paths.get(index))
        .map_or("None", |path| path.name.as_str());
    rows.push(choice(
        "text-path",
        "On Path",
        crate::vm::i2f(crate::vm::u2i(path_index.map_or(0, |index| index + 1))),
        crate::vm::i2f(crate::vm::u2i(paths.len())),
        path_label,
    ));

    // Text warp: index 0 is "None", then the ten styles; the sliders below
    // always edit (moving one arms the default Arc warp).
    let warp = layer.text_warp.clone().unwrap_or_else(text_warp_default);
    let warp_index = layer
        .text_warp
        .as_ref()
        .and_then(|_| {
            TEXT_WARP_LABELS
                .iter()
                .position(|(_, style)| *style == warp.style)
        })
        .map_or(0, |index| index + 1);
    let warp_label = if layer.text_warp.is_some() {
        TEXT_WARP_LABELS
            .iter()
            .find(|(_, style)| *style == warp.style)
            .map_or("None", |(label, _)| *label)
    } else {
        "None"
    };
    rows.push(choice(
        "warp",
        "Warp",
        crate::vm::i2f(crate::vm::u2i(warp_index)),
        crate::vm::i2f(crate::vm::u2i(TEXT_WARP_LABELS.len())),
        warp_label,
    ));
    rows.push(number("warp-bend", "Bend", warp.bend, -100.0, 100.0));
    rows.push(number(
        "warp-horizontal",
        "H Distort",
        warp.horizontal,
        -100.0,
        100.0,
    ));
    rows.push(number(
        "warp-vertical",
        "V Distort",
        warp.vertical,
        -100.0,
        100.0,
    ));
    rows
}

/// One panel row event from the shared editor-row widget: number sliders,
/// choice sliders (which arrive as an index) and paragraph spacing.
fn parse_number_row(key: &str, value: f64) -> Option<TextPatch> {
    let number = |slot: TextNumber, value: f64| Some(TextPatch::Number(slot, value));
    match key {
        "tracking" => number(TextNumber::Tracking, value.clamp(-1000.0, 5000.0)),
        "kerning" => number(TextNumber::Kerning, value.clamp(-200.0, 500.0)),
        "leading" => number(TextNumber::Leading, value.clamp(0.0, 800.0)),
        "baseline-shift" => number(TextNumber::BaselineShift, value.clamp(-400.0, 400.0)),
        "scale-x" => number(TextNumber::ScaleX, value.clamp(1.0, 1000.0)),
        "scale-y" => number(TextNumber::ScaleY, value.clamp(1.0, 1000.0)),
        "text-size" => Some(TextPatch::FontSize(value.clamp(1.0, 512.0))),
        "indent-left" => Some(TextPatch::Paragraph(
            ParagraphSlot::IndentLeft,
            value.clamp(0.0, 800.0),
        )),
        "indent-right" => Some(TextPatch::Paragraph(
            ParagraphSlot::IndentRight,
            value.clamp(0.0, 800.0),
        )),
        "indent-first" => Some(TextPatch::Paragraph(
            ParagraphSlot::IndentFirst,
            value.clamp(0.0, 800.0),
        )),
        "space-before" => Some(TextPatch::Paragraph(
            ParagraphSlot::SpaceBefore,
            value.clamp(0.0, 800.0),
        )),
        "space-after" => Some(TextPatch::Paragraph(
            ParagraphSlot::SpaceAfter,
            value.clamp(0.0, 800.0),
        )),
        _ => None,
    }
}

/// What a numeric text-panel row patches.
#[derive(Debug, Clone, Copy)]
enum TextPatch {
    Number(TextNumber, f64),
    FontSize(f64),
    Paragraph(ParagraphSlot, f64),
}

/// The paragraph fields the text panel edits.
#[derive(Debug, Clone, Copy)]
enum ParagraphSlot {
    IndentLeft,
    IndentRight,
    IndentFirst,
    SpaceBefore,
    SpaceAfter,
}

/// The boolean text-panel rows.
#[derive(Debug, Clone, Copy)]
enum ToggleSlot {
    FauxBold,
    FauxItalic,
    Underline,
    Strikethrough,
    Hyphenate,
}

impl ParagraphSlot {
    fn apply(self, paragraph: &mut CanvasPsTextParagraph, value: f64) {
        match self {
            Self::IndentLeft => paragraph.indent_left = value,
            Self::IndentRight => paragraph.indent_right = value,
            Self::IndentFirst => paragraph.indent_first = value,
            Self::SpaceBefore => paragraph.space_before = value,
            Self::SpaceAfter => paragraph.space_after = value,
        }
    }
}

/// The path id a `text-path` choice index selects (0 = none).
pub fn path_choice(paths: &[open_canvas_ps::CanvasPsPath], index: i32) -> Option<String> {
    if index <= 0 {
        return None;
    }
    let slot = crate::vm::i2u(crate::vm::f2i(f64::from(index))).saturating_sub(1);
    paths.get(slot).map(|path| path.id.clone())
}

/// The warp style a `warp` choice index selects (0 = none).
pub fn warp_choice(index: i32) -> Option<CanvasPsTextWarpStyle> {
    if index <= 0 {
        return None;
    }
    let slot = crate::vm::i2u(crate::vm::f2i(f64::from(index))).saturating_sub(1);
    TEXT_WARP_LABELS.get(slot).map(|(_, style)| *style)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::composite;
    use std::collections::HashMap;

    use open_canvas_ps::IdGen;

    fn app_with_text() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 64.0;
        project.height = 32.0;
        let mut gen = IdGen::new("t");
        let text = open_canvas_ps::create_ps_text_layer(&mut gen, (64.0, 32.0), "Hello", "Text");
        project.board_layers = vec![text];
        app.enter_image(
            project,
            HashMap::new(),
            crate::image::assets::ImageAssets::default(),
        );
        Ok((app, dir))
    }

    fn layer(app: &App) -> Result<CanvasPsLayer, Box<dyn std::error::Error>> {
        app.image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or_else(|| "layer".into())
    }

    #[test]
    fn double_click_opens_the_overlay_and_commits_through_the_panel_writer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        crate::app::with_app(&app, |app| {
            let mut project = crate::image::ImageState::default().project;
            project.id = "img-1".to_string();
            project.width = 64.0;
            project.height = 32.0;
            let mut gen = IdGen::new("t");
            project.board_layers = vec![open_canvas_ps::create_ps_text_layer(
                &mut gen,
                (64.0, 32.0),
                "Hello",
                "Text",
            )];
            app.enter_image(
                project,
                HashMap::new(),
                crate::image::assets::ImageAssets::default(),
            );
            app.image.view = crate::image::ImageView {
                x: 0.0,
                y: 0.0,
                k: 1.0,
            };
            app.sync_image(&ui);

            assert!(
                app.image_text_edit_begin(&ui, 32.0, 16.0),
                "the double-click hits the text layer"
            );
        });
        assert!(ui.get_image_text_edit_on());
        assert_eq!(ui.get_image_text_edit_draft(), "Hello");
        assert!(ui.get_image_text_edit_w() >= 24.0);

        crate::app::with_app(&app, |app| {
            app.image_text_edit_changed("Hello world");
            app.image_text_edit_commit(&ui, Some("Hello world"));
        });
        assert!(!ui.get_image_text_edit_on(), "commit closes the overlay");
        crate::app::with_app(&app, |app| {
            assert_eq!(
                layer(app).ok().and_then(|layer| layer.text),
                Some("Hello world".to_string())
            );
            assert_eq!(app.image.history.label(1), Some("Edit Text"));
        });
        Ok(())
    }

    #[test]
    fn escape_cancels_the_overlay_without_dirtying_the_layer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        crate::app::with_app(&app, |app| {
            let mut project = crate::image::ImageState::default().project;
            project.id = "img-1".to_string();
            project.width = 64.0;
            project.height = 32.0;
            let mut gen = IdGen::new("t");
            project.board_layers = vec![open_canvas_ps::create_ps_text_layer(
                &mut gen,
                (64.0, 32.0),
                "Hello",
                "Text",
            )];
            app.enter_image(
                project,
                HashMap::new(),
                crate::image::assets::ImageAssets::default(),
            );
            app.image.view = crate::image::ImageView {
                x: 0.0,
                y: 0.0,
                k: 1.0,
            };
            assert!(app.image_text_edit_begin(&ui, 32.0, 16.0));
            app.image_text_edit_changed("discard me");
            app.image_text_edit_cancel(&ui);
            assert_eq!(
                layer(app).ok().and_then(|layer| layer.text),
                Some("Hello".to_string())
            );
            assert_eq!(app.image.history.label(1), None, "cancel writes no history");
        });
        Ok(())
    }

    #[test]
    fn character_rows_round_trip_through_the_panel() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        assert!(app.image_text_row_set_core("tracking", 120.0));
        assert!(app.image_text_row_set_core("kerning", -40.0));
        assert!(app.image_text_row_set_core("leading", 48.0));
        assert!(app.image_text_row_set_core("baseline-shift", 6.0));
        assert!(app.image_text_row_set_core("scale-x", 150.0));
        assert!(app.image_text_row_set_core("scale-y", 75.0));
        let row_layer = layer(&app)?;
        assert_eq!(row_layer.tracking, Some(120.0));
        assert_eq!(row_layer.kerning, Some(-40.0));
        assert_eq!(row_layer.leading, Some(48.0));
        assert_eq!(row_layer.baseline_shift, Some(6.0));
        assert_eq!(row_layer.text_scale_x, Some(150.0));
        assert_eq!(row_layer.text_scale_y, Some(75.0));
        assert_eq!(app.image.history.label(1), Some("Edit Text"));

        assert!(
            app.image_text_row_set_core("tracking", 9999.0),
            "clamped to the row max"
        );
        assert_eq!(layer(&app)?.tracking, Some(5000.0));
        assert!(!app.image_text_row_set_core("unknown", 1.0));

        assert!(app.image_text_row_set_core("text-size", 64.0));
        assert_eq!(layer(&app)?.font_size, Some(64.0));
        assert!(app.image_text_row_set_core("indent-left", 24.0));
        assert!(app.image_text_row_set_core("space-after", 12.0));
        let paragraph = paragraph_of(&layer(&app)?);
        assert_eq!((paragraph.indent_left, paragraph.space_after), (24.0, 12.0));
        Ok(())
    }

    #[test]
    fn case_align_and_toggles_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        assert!(app.image_text_option_core("text-case", "small-caps"));
        assert_eq!(layer(&app)?.text_case, Some(CanvasPsTextCase::SmallCaps));
        assert!(app.image_text_option_core("text-case", "bogus"));
        assert_eq!(layer(&app)?.text_case, Some(CanvasPsTextCase::None));

        assert!(app.image_text_option_core("align", "center"));
        assert_eq!(paragraph_of(&layer(&app)?).align, CanvasPsTextAlign::Center);
        assert!(app.image_text_row_check_core("hyphenate", true));
        assert!(paragraph_of(&layer(&app)?).hyphenate);
        assert!(app.image_text_row_check_core("hyphenate", false));
        assert!(!paragraph_of(&layer(&app)?).hyphenate);

        assert!(app.image_text_row_check_core("faux-bold", true));
        assert!(app.image_text_row_check_core("faux-italic", true));
        assert!(app.image_text_row_check_core("underline", true));
        assert!(app.image_text_row_check_core("strikethrough", true));
        let styled = layer(&app)?;
        assert_eq!(styled.faux_bold, Some(true));
        assert_eq!(styled.faux_italic, Some(true));
        assert_eq!(styled.underline, Some(true));
        assert_eq!(styled.strikethrough, Some(true));
        assert!(app.image_text_row_check_core("underline", false));
        assert_eq!(layer(&app)?.underline, Some(false));
        Ok(())
    }

    #[test]
    fn text_path_and_warp_arm_and_clear() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        assert!(app.image_text_option_core("text-path", "p-1"));
        assert_eq!(layer(&app)?.text_path_id.as_deref(), Some("p-1"));
        assert!(app.image_text_option_core("text-path", ""));
        assert_eq!(layer(&app)?.text_path_id, None);

        assert!(app.image_text_option_core("warp", "wave"));
        let warp = layer(&app)?.text_warp.clone().ok_or("warp")?;
        assert_eq!(warp.style, CanvasPsTextWarpStyle::Wave);
        assert!((warp.bend - 0.0).abs() < 1e-9, "the default bend is zero");

        assert!(app.image_text_warp_core("warp-bend", 60.0));
        assert!(app.image_text_warp_core("warp-horizontal", 12.0));
        assert!(app.image_text_warp_core("warp-vertical", -8.0));
        let warp = layer(&app)?.text_warp.clone().ok_or("warp")?;
        assert_eq!(
            (warp.bend, warp.horizontal, warp.vertical),
            (60.0, 12.0, -8.0)
        );
        assert_eq!(
            warp.style,
            CanvasPsTextWarpStyle::Wave,
            "sliders keep the style"
        );

        assert!(app.image_text_option_core("warp", "squeeze"));
        assert_eq!(
            layer(&app)?.text_warp.clone().ok_or("warp")?.style,
            CanvasPsTextWarpStyle::Squeeze
        );
        assert!(app.image_text_option_core("warp", ""));
        assert!(layer(&app)?.text_warp.clone().is_none());
        assert!(!app.image_text_option_core("warp", "bogus"));

        // Moving a warp slider with no style armed arms the Arc default.
        assert!(app.image_text_warp_core("warp-bend", 30.0));
        let warp = layer(&app)?.text_warp.clone().ok_or("warp")?;
        assert_eq!(warp.style, CanvasPsTextWarpStyle::Arc);
        assert!((warp.bend - 30.0).abs() < 1e-9);
        assert!(!app.image_text_warp_core("warp-nope", 1.0));
        Ok(())
    }

    #[test]
    fn id_tables_round_trip_and_covers_every_enum() {
        assert_eq!(TEXT_CASE_LABELS.len(), 4);
        assert_eq!(TEXT_ALIGN_LABELS.len(), 4);
        assert_eq!(TEXT_WARP_LABELS.len(), 10);
        for (label, case) in TEXT_CASE_LABELS {
            assert_eq!(labelled(&TEXT_CASE_LABELS, label), Some(case));
        }
        for (label, align) in TEXT_ALIGN_LABELS {
            assert_eq!(labelled(&TEXT_ALIGN_LABELS, label), Some(align));
        }
        for (label, style) in TEXT_WARP_LABELS {
            assert_eq!(labelled(&TEXT_WARP_LABELS, label), Some(style));
        }
        assert_eq!(
            labelled(&TEXT_WARP_LABELS, "arc-lower"),
            Some(CanvasPsTextWarpStyle::ArcLower),
            "kebab ids resolve too"
        );
        assert_eq!(labelled(&TEXT_WARP_LABELS, "nope"), None);
        assert_eq!(warp_choice(0), None);
        assert_eq!(warp_choice(5), Some(CanvasPsTextWarpStyle::Wave));
        assert_eq!(path_choice(&[], 3), None);
    }

    #[test]
    fn warp_and_path_choice_rows_pick_by_index() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        let path = open_canvas_ps::create_path(
            "p-1",
            "Line",
            vec![
                open_canvas_ps::path_anchor(0.0, 0.0),
                open_canvas_ps::path_anchor(40.0, 0.0),
            ],
            false,
        );
        app.image.project.board_paths = Some(vec![path]);

        assert!(app.image_text_row_set_core("text-path", 1.0));
        assert_eq!(layer(&app)?.text_path_id.as_deref(), Some("p-1"));
        assert!(app.image_text_row_set_core("text-path", 0.0));
        assert_eq!(layer(&app)?.text_path_id, None);

        assert!(app.image_text_row_set_core("warp", 5.0));
        assert_eq!(
            layer(&app)?.text_warp.clone().ok_or("warp")?.style,
            CanvasPsTextWarpStyle::Wave,
            "index 5 is the fifth style"
        );
        assert!(app.image_text_row_set_core("warp", 0.0));
        assert!(layer(&app)?.text_warp.clone().is_none());

        let rows = text_panel_rows(
            layer(&app).ok().as_ref(),
            &app.image.project.board_paths.clone().unwrap_or_default(),
            crate::image::TextPanelTab::Character,
        );
        let warp_row = rows
            .iter()
            .find(|row| row.key == "warp")
            .ok_or("warp row")?;
        assert_eq!(warp_row.kind, "choice");
        assert_eq!(warp_row.text, "None");
        assert!((warp_row.max - 10.0).abs() < 1e-9);
        let path_row = rows
            .iter()
            .find(|row| row.key == "text-path")
            .ok_or("path row")?;
        assert_eq!(path_row.text, "None");
        assert!(
            (path_row.max - 1.0).abs() < 1e-9,
            "one board path is selectable"
        );
        Ok(())
    }

    #[test]
    fn non_text_layers_ignore_the_panel_commands() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        app.image_layer_action_core("add-pixel", "");
        assert!(matches!(
            app.image
                .project
                .board_layers
                .last()
                .map(|layer| layer.kind),
            Some(open_canvas_ps::CanvasPsLayerKind::Pixel)
        ));
        assert!(!app.image_text_row_set_core("tracking", 10.0));
        assert!(!app.image_text_option_core("text-case", "upper"));
        assert!(!app.image_text_row_check_core("underline", true));
        assert!(text_panel_rows(
            app.image.project.board_layers.last(),
            &[],
            crate::image::TextPanelTab::Character,
        )
        .is_empty());
        Ok(())
    }

    /// An app whose active layer is a shape (`createPsShapeLayer`).
    fn app_with_shape() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let store = crate::store::Store::at(dir.path().to_path_buf());
        let mut app = App::new(store);
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 64.0;
        project.height = 64.0;
        let mut gen = IdGen::new("s");
        let shape = open_canvas_ps::create_ps_shape_layer(
            &mut gen,
            open_canvas_ps::CanvasPsShapeKind::Rectangle,
            open_canvas_ps::Bounds::new(8.0, 8.0, 40.0, 40.0),
            "Shape",
            "#ff0000",
            "",
            0.0,
        );
        project.board_layers = vec![shape];
        app.enter_image(
            project,
            HashMap::new(),
            crate::image::assets::ImageAssets::default(),
        );
        Ok((app, dir))
    }

    fn shape(app: &App) -> Result<CanvasPsLayer, Box<dyn std::error::Error>> {
        app.image
            .project
            .board_layers
            .first()
            .cloned()
            .ok_or_else(|| "shape".into())
    }

    #[test]
    fn shape_properties_edit_and_reach_the_renderer() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_shape()?;
        let rows = shape_panel_rows(&shape(&app)?);
        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "shape-kind",
                "shape-fill",
                "shape-stroke",
                "shape-stroke-width",
                "shape-radius",
                "shape-sides"
            ]
        );
        assert_eq!(rows.first().map(|row| row.kind), Some("select"));
        assert_eq!(rows.first().map(|row| row.text.as_str()), Some("Rectangle"));

        // Type: the label maps back to the enum.
        assert!(app.image_text_option_core("shape-kind", "Ellipse"));
        assert_eq!(
            shape(&app)?.shape,
            Some(open_canvas_ps::CanvasPsShapeKind::Ellipse)
        );
        assert!(app.image_text_option_core("shape-kind", "Rounded"));
        assert_eq!(
            shape(&app)?.shape,
            Some(open_canvas_ps::CanvasPsShapeKind::RoundedRectangle)
        );
        assert!(!app.image_text_option_core("shape-kind", "blob"));

        // Fill / stroke colours.
        assert!(app.image_text_row_text_core("shape-fill", "#00ff00"));
        assert_eq!(shape(&app)?.shape_fill.as_deref(), Some("#00ff00"));
        assert!(app.image_text_row_text_core("shape-stroke", "#0000ff"));
        assert_eq!(shape(&app)?.shape_stroke.as_deref(), Some("#0000ff"));
        assert!(!app.image_text_row_text_core("shape-fill", "junk"));

        // Stroke width / radius / sides.
        assert!(app.image_text_row_set_core("shape-stroke-width", 8.0));
        assert!(app.image_text_row_set_core("shape-radius", 12.0));
        assert!(app.image_text_row_set_core("shape-sides", 5.0));
        let edited = shape(&app)?;
        assert_eq!(edited.shape_stroke_width, Some(8.0));
        assert_eq!(edited.shape_radius, Some(12.0));
        assert_eq!(edited.shape_sides, Some(5.0));

        // The edits reach the compositor: a wider stroke changes the pixels.
        assert!(app.image_text_row_set_core("shape-stroke-width", 0.0));
        let thin = composite(&app);
        assert!(app.image_text_row_set_core("shape-stroke-width", 8.0));
        assert_ne!(composite(&app), thin, "the stroke width changes pixels");

        // A non-shape layer rejects the shape commands.
        app.image_layer_action_core("add-pixel", "");
        assert!(!app.image_text_row_set_core("shape-radius", 4.0));
        assert!(!app.image_text_row_text_core("shape-fill", "#ffffff"));
        assert!(!app.image_text_option_core("shape-kind", "Ellipse"));
        Ok(())
    }

    #[test]
    fn the_text_panel_tab_switches_the_row_list() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        let rows = |app: &App| {
            text_panel_rows(
                app.image.project.board_layers.first(),
                &[],
                app.image.text_tab,
            )
        };
        let character = rows(&app);
        assert_eq!(
            character.first().map(|row| row.key.as_str()),
            Some("text-tab"),
            "the tab row leads the list"
        );
        assert_eq!(
            character.first().map(|row| row.text.as_str()),
            Some("Character")
        );
        assert!(character.iter().any(|row| row.key == "tracking"));
        assert!(!character.iter().any(|row| row.key == "align"));
        // On-path + warp stay on both tabs (the web shows them below the tabs).
        assert!(character.iter().any(|row| row.key == "warp"));
        assert!(character.iter().any(|row| row.key == "text-path"));

        assert!(app.image_text_option_core("text-tab", "paragraph"));
        assert_eq!(app.image.text_tab, crate::image::TextPanelTab::Paragraph);
        let paragraph = rows(&app);
        assert_eq!(
            paragraph.first().map(|row| row.text.as_str()),
            Some("Paragraph")
        );
        assert!(paragraph.iter().any(|row| row.key == "align"));
        assert!(paragraph.iter().any(|row| row.key == "hyphenate"));
        assert!(!paragraph.iter().any(|row| row.key == "tracking"));

        // The tab row's chip click routes through the same option callback.
        assert!(app.image_text_option_core("text-tab", "Character"));
        assert_eq!(app.image.text_tab, crate::image::TextPanelTab::Character);
        assert!(!app.image_text_option_core("text-tab", "nope"));
        Ok(())
    }

    #[test]
    fn the_layer_row_dispatcher_picks_by_kind() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_shape()?;
        let rows = layer_panel_rows(
            app.image.project.board_layers.first(),
            &[],
            crate::image::TextPanelTab::Character,
        );
        assert!(rows.iter().any(|row| row.key == "shape-kind"));

        app.image_layer_action_core("add-text", "");
        let rows = layer_panel_rows(
            app.image.project.board_layers.last(),
            &[],
            crate::image::TextPanelTab::Character,
        );
        assert!(rows.iter().any(|row| row.key == "text-tab"));

        app.image_layer_action_core("add-pixel", "");
        assert!(layer_panel_rows(
            app.image.project.board_layers.last(),
            &[],
            crate::image::TextPanelTab::Character,
        )
        .is_empty());
        Ok(())
    }

    #[test]
    fn a_warped_text_layer_still_renders_pixels() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = app_with_text()?;
        let plain = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("plain")?
            .data()
            .to_vec();
        assert!(app.image_text_option_core("warp", "arc"));
        assert!(app.image_text_warp_core("warp-bend", 90.0));
        let warped = crate::image::render::board_surface(&app.image.project, &app.image.bitmaps)
            .ok_or("warped")?
            .data()
            .to_vec();
        assert_ne!(plain, warped, "the warp mesh reshapes the rendered text");
        Ok(())
    }
}
