//! PS document model — a 1:1 serde port of the Image Studio types in
//! `web/src/types/canvas.ts` (`CanvasPs*`) and
//! `web/src/stores/use-image-store.ts` (`ImageProject`). Field names are
//! camelCase and every optional field is omitted when `None`, matching the
//! JSON the web app writes, so a document round-trips losslessly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `Position` (canvas.ts:1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// `CanvasPsParamValue` (canvas.ts:84): number | string | number[] | string[].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CanvasPsParamValue {
    Number(f64),
    Text(String),
    Numbers(Vec<f64>),
    Texts(Vec<String>),
}

/// `Record<string, CanvasPsParamValue>` — an adjustment/style parameter bag.
pub type PsParams = BTreeMap<String, CanvasPsParamValue>;

/// `CanvasPsLayerKind` (canvas.ts:75).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsLayerKind {
    Image,
    Text,
    Group,
    Pixel,
    Shape,
    Adjustment,
}

/// `CanvasPsShapeKind` (canvas.ts:77).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsShapeKind {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Polygon,
    Line,
}

/// `CanvasPsAdjustmentType` (canvas.ts:80): the 15 adjustment layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsAdjustmentType {
    BrightnessContrast,
    Levels,
    Curves,
    Exposure,
    Vibrance,
    HueSaturation,
    ColorBalance,
    BlackWhite,
    PhotoFilter,
    ChannelMixer,
    GradientMap,
    Invert,
    Posterize,
    Threshold,
    SelectiveColor,
}

/// `CanvasPsLayerStyleType` (canvas.ts:82): the 10 layer styles (fx).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsLayerStyleType {
    Stroke,
    DropShadow,
    InnerShadow,
    OuterGlow,
    InnerGlow,
    Bevel,
    Satin,
    ColorOverlay,
    GradientOverlay,
    PatternOverlay,
}

/// `CanvasPsTextCase` (canvas.ts:125).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsTextCase {
    None,
    Upper,
    Lower,
    SmallCaps,
}

/// `CanvasPsTextAlign` (canvas.ts:127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasPsTextAlign {
    Left,
    Center,
    Right,
    Justify,
}

/// `CanvasPsTextWarpStyle` (canvas.ts:129): the 10 warp text presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasPsTextWarpStyle {
    Arc,
    ArcLower,
    ArcUpper,
    Flag,
    Wave,
    Fish,
    Rise,
    Bulge,
    Shell,
    Squeeze,
}

/// `CanvasPsTransform` (canvas.ts:120): normalized quad plus an optional 4x4
/// row-major warp mesh that takes precedence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsTransform {
    pub quad: Vec<Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warp: Option<Vec<Position>>,
}

/// `CanvasPsTextWarp` (canvas.ts:132).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsTextWarp {
    pub style: CanvasPsTextWarpStyle,
    pub bend: f64,
    pub horizontal: f64,
    pub vertical: f64,
}

/// `CanvasPsTextParagraph` (canvas.ts:134).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsTextParagraph {
    pub align: CanvasPsTextAlign,
    pub indent_left: f64,
    pub indent_right: f64,
    pub indent_first: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub hyphenate: bool,
}

/// `CanvasPsPathAnchor` (canvas.ts:145): handles are offsets from the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsPathAnchor {
    pub x: f64,
    pub y: f64,
    pub handle_in: Position,
    pub handle_out: Position,
}

/// `CanvasPsPath` (canvas.ts:148).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsPath {
    pub id: String,
    pub name: String,
    pub anchors: Vec<CanvasPsPathAnchor>,
    pub closed: bool,
    pub visible: bool,
}

/// `CanvasPsAlphaChannel` (canvas.ts:151).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsAlphaChannel {
    pub id: String,
    pub name: String,
    pub storage_key: String,
}

/// `CanvasPsLayerStyle` (canvas.ts:154).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsLayerStyle {
    pub id: String,
    #[serde(rename = "type")]
    pub style_type: CanvasPsLayerStyleType,
    pub enabled: bool,
    #[serde(default)]
    pub params: PsParams,
}

/// `CanvasPsLayer` (canvas.ts:162): the complete 38-field image layer entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPsLayer {
    pub id: String,
    pub name: String,
    pub kind: CanvasPsLayerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_storage_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<CanvasPsAdjustmentType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment_params: Option<PsParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<Vec<CanvasPsLayerStyle>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<CanvasPsShapeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_sides: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_stroke: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_stroke_width: Option<f64>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub blend_mode: String,
    pub hidden: bool,
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leading: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kerning: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_shift: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scale_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scale_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faux_bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faux_italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_case: Option<CanvasPsTextCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<CanvasPsTextParagraph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_path_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_warp: Option<CanvasPsTextWarp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<CanvasPsTransform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<String>>,
}

/// `boardChannelVisibility` (use-image-store.ts:23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelVisibility {
    pub r: bool,
    pub g: bool,
    pub b: bool,
}

/// `boardResolution` (use-image-store.ts:17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BoardResolution {
    #[serde(rename = "1k")]
    K1,
    #[default]
    #[serde(rename = "2k")]
    K2,
    #[serde(rename = "4k")]
    K4,
}

/// `ImageProject` (use-image-store.ts:8): the persisted Image Studio document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageProject {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub group_id: String,
    pub width: f64,
    pub height: f64,
    pub board_ratio: String,
    pub board_resolution: BoardResolution,
    pub board_background: String,
    pub board_background_opacity: f64,
    pub board_layers: Vec<CanvasPsLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_paths: Option<Vec<CanvasPsPath>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_alpha_channels: Option<Vec<CanvasPsAlphaChannel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_channel_visibility: Option<ChannelVisibility>,
}

/// `SmartCanvasBoard` (smart-canvas.ts:20): the composer's view of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartCanvasBoard {
    pub id: String,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub board_ratio: String,
    pub board_resolution: BoardResolution,
    pub board_background: String,
    pub board_background_opacity: f64,
    pub board_layers: Vec<CanvasPsLayer>,
    pub board_paths: Vec<CanvasPsPath>,
    pub board_alpha_channels: Vec<CanvasPsAlphaChannel>,
    pub board_channel_visibility: Option<ChannelVisibility>,
}

impl From<&ImageProject> for SmartCanvasBoard {
    fn from(project: &ImageProject) -> Self {
        Self {
            id: project.id.clone(),
            title: project.title.clone(),
            width: project.width,
            height: project.height,
            board_ratio: project.board_ratio.clone(),
            board_resolution: project.board_resolution,
            board_background: project.board_background.clone(),
            board_background_opacity: project.board_background_opacity,
            board_layers: project.board_layers.clone(),
            board_paths: project.board_paths.clone().unwrap_or_default(),
            board_alpha_channels: project.board_alpha_channels.clone().unwrap_or_default(),
            board_channel_visibility: project.board_channel_visibility,
        }
    }
}

/// `ImageGroup` (use-image-store.ts:26).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGroup {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// `ImagePreset` (use-image-store.ts:32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreset {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub ratio: String,
}

/// `IMAGE_PRESETS` (use-image-store.ts:40).
pub fn image_presets() -> Vec<ImagePreset> {
    let preset = |id: &str, name: &str, width: f64, height: f64| ImagePreset {
        id: id.to_string(),
        name: name.to_string(),
        width,
        height,
        ratio: id.to_string(),
    };
    vec![
        preset("16:9", "16:9 Widescreen (1920 × 1080)", 1920.0, 1080.0),
        preset("1:1", "1:1 Square (1024 × 1024)", 1024.0, 1024.0),
        preset("9:16", "9:16 Portrait (1080 × 1920)", 1080.0, 1920.0),
        preset("4:3", "4:3 Standard (1600 × 1200)", 1600.0, 1200.0),
        preset("3:4", "3:4 Portrait (1200 × 1600)", 1200.0, 1600.0),
        preset("21:9", "21:9 Ultra-Wide (2560 × 1080)", 2560.0, 1080.0),
    ]
}

/// Deterministic id source for the mutating ops (the web uses `nanoid`).
#[derive(Debug, Clone)]
pub struct IdGen {
    prefix: String,
    counter: u64,
}

impl IdGen {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            counter: 0,
        }
    }

    pub fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("{}-{}", self.prefix, self.counter)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn sample_layer() -> CanvasPsLayer {
        CanvasPsLayer {
            id: "l1".into(),
            name: "Base".into(),
            kind: CanvasPsLayerKind::Pixel,
            content: None,
            storage_key: Some("image:abc".into()),
            mask_storage_key: None,
            adjustment: None,
            adjustment_params: None,
            styles: None,
            text: None,
            shape: None,
            shape_radius: None,
            shape_sides: None,
            shape_fill: None,
            shape_stroke: None,
            shape_stroke_width: None,
            x: 0.0,
            y: 0.0,
            width: 640.0,
            height: 360.0,
            rotation: 0.0,
            opacity: 1.0,
            blend_mode: "normal".into(),
            hidden: false,
            locked: false,
            font_size: None,
            color: None,
            font_family: None,
            tracking: None,
            leading: None,
            kerning: None,
            baseline_shift: None,
            text_scale_x: None,
            text_scale_y: None,
            faux_bold: None,
            faux_italic: None,
            underline: None,
            strikethrough: None,
            text_case: None,
            paragraph: None,
            text_path_id: None,
            text_warp: None,
            transform: None,
            children: None,
        }
    }

    fn sample_project() -> ImageProject {
        let mut params = PsParams::new();
        params.insert("brightness".into(), CanvasPsParamValue::Number(12.0));
        let layer = CanvasPsLayer {
            styles: Some(vec![CanvasPsLayerStyle {
                id: "s1".into(),
                style_type: CanvasPsLayerStyleType::Stroke,
                enabled: true,
                params,
            }]),
            ..sample_layer()
        };
        ImageProject {
            id: "p1".into(),
            title: "Board".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            group_id: "g1".into(),
            width: 640.0,
            height: 360.0,
            board_ratio: "16:9".into(),
            board_resolution: BoardResolution::K2,
            board_background: "transparent".into(),
            board_background_opacity: 1.0,
            board_layers: vec![layer],
            board_paths: Some(vec![CanvasPsPath {
                id: "path1".into(),
                name: "P".into(),
                anchors: vec![CanvasPsPathAnchor {
                    x: 1.0,
                    y: 2.0,
                    handle_in: Position { x: 0.0, y: 0.0 },
                    handle_out: Position { x: 3.0, y: 4.0 },
                }],
                closed: false,
                visible: true,
            }]),
            board_alpha_channels: Some(vec![CanvasPsAlphaChannel {
                id: "a1".into(),
                name: "Alpha".into(),
                storage_key: "image:mask".into(),
            }]),
            board_channel_visibility: Some(ChannelVisibility {
                r: true,
                g: false,
                b: true,
            }),
        }
    }

    #[test]
    fn project_round_trips_losslessly() {
        let project = sample_project();
        let json = serde_json::to_string(&project).expect("serialize");
        let back: ImageProject = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(project, back);
    }

    #[test]
    fn camel_case_and_omitted_optionals_match_the_web_json() {
        let layer = sample_layer();
        let value: serde_json::Value = serde_json::to_value(&layer).expect("value");
        let object = value.as_object().expect("object");
        assert!(object.contains_key("storageKey"));
        assert!(object.contains_key("blendMode"));
        assert!(!object.contains_key("maskStorageKey"));
        assert!(!object.contains_key("adjustment"));
        assert!(object.get("kind") == Some(&serde_json::Value::String("pixel".into())));
    }

    #[test]
    fn param_value_is_untagged() {
        let number: CanvasPsParamValue = serde_json::from_str("12").expect("number");
        assert_eq!(number, CanvasPsParamValue::Number(12.0));
        let numbers: CanvasPsParamValue = serde_json::from_str("[0,0,255,255]").expect("numbers");
        assert_eq!(
            numbers,
            CanvasPsParamValue::Numbers(vec![0.0, 0.0, 255.0, 255.0])
        );
        let texts: CanvasPsParamValue = serde_json::from_str("[\"#000000@0\"]").expect("texts");
        assert_eq!(texts, CanvasPsParamValue::Texts(vec!["#000000@0".into()]));
    }

    #[test]
    fn board_view_drops_library_metadata() {
        let project = sample_project();
        let board = SmartCanvasBoard::from(&project);
        assert_eq!(board.id, "p1");
        assert_eq!(board.board_layers.len(), 1);
        assert_eq!(board.board_paths.len(), 1);
        assert_eq!(board.board_alpha_channels.len(), 1);
        assert!(board
            .board_channel_visibility
            .is_some_and(|visibility| !visibility.g));
    }
}
