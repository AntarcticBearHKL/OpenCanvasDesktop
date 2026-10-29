//! Multi-angle generation — port of the web's angle label/prompt strings
//! (`canvas-generation-helpers.ts:143`) and the `CanvasImageAngleParams`
//! defaults (`canvas-node-angle-dialog.tsx:8`), plus a deterministic variant plan.

use serde::{Deserialize, Serialize};

/// `CanvasImageAngleParams` (canvas-node-angle-dialog.tsx:8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AngleParams {
    pub horizontal_angle: f64,
    pub pitch_angle: f64,
    pub camera_distance: f64,
    pub wide_angle: bool,
}

impl AngleParams {
    pub const fn new(
        horizontal_angle: f64,
        pitch_angle: f64,
        camera_distance: f64,
        wide_angle: bool,
    ) -> Self {
        Self {
            horizontal_angle,
            pitch_angle,
            camera_distance,
            wide_angle,
        }
    }
}

impl Default for AngleParams {
    /// `defaultParams` (canvas-node-angle-dialog.tsx:15).
    fn default() -> Self {
        Self::new(0.0, 9.0, 4.8, false)
    }
}

/// `buildAngleLabel` (canvas-generation-helpers.ts:143).
pub fn build_angle_label(params: &AngleParams) -> String {
    let horizontal = if params.horizontal_angle == 0.0 {
        "front view".to_string()
    } else if params.horizontal_angle > 0.0 {
        format!("rotated {} degrees right", params.horizontal_angle)
    } else {
        format!("rotated {} degrees left", params.horizontal_angle.abs())
    };
    let pitch = if params.pitch_angle == 0.0 {
        "eye-level view".to_string()
    } else if params.pitch_angle > 0.0 {
        format!("{}-degree top-down view", params.pitch_angle)
    } else {
        format!("{}-degree low-angle view", params.pitch_angle.abs())
    };
    let lens = if params.wide_angle {
        "Wide"
    } else {
        "Standard"
    };
    format!(
        "AI multi-angle: {horizontal}, {pitch}, camera distance {:.1}, {lens} lens",
        params.camera_distance
    )
}

/// `buildAnglePrompt` (canvas-generation-helpers.ts:149).
pub fn build_angle_prompt(params: &AngleParams) -> String {
    format!(
        "Regenerate a new view of the same subject from the reference image. Preserve the subject, colors, materials, and visual style; do not merely apply perspective distortion. {}.",
        build_angle_label(params)
    )
}

/// Lower bound shared with `getGenerationCount` (canvas-generation-helpers.ts:57).
pub const ANGLE_VARIANT_MIN: usize = 1;
/// Upper bound shared with `getGenerationCount` (canvas-generation-helpers.ts:57).
pub const ANGLE_VARIANT_MAX: usize = 15;

/// Fixed camera angles cycled after the base variant; all inside the dialog's
/// slider ranges (horizontal `-60..=60`, pitch `-45..=45`).
const ANGLE_VARIANT_OFFSETS: [(f64, f64); 8] = [
    (-45.0, 0.0),
    (45.0, 0.0),
    (0.0, 30.0),
    (0.0, -30.0),
    (-30.0, 20.0),
    (30.0, 20.0),
    (-60.0, 30.0),
    (60.0, -20.0),
];

/// One planned angle: its parameters, display label and generation prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct AngleVariant {
    pub index: usize,
    pub params: AngleParams,
    pub label: String,
    pub prompt: String,
}

/// The N deterministic variants for one multi-angle run.
#[derive(Debug, Clone, PartialEq)]
pub struct AngleVariantPlan {
    pub count: usize,
    pub variants: Vec<AngleVariant>,
}

/// Build `count` variants: the first is the base params, the rest cycle a fixed
/// camera-angle catalogue so the same input always yields the same prompts.
pub fn angle_variant_plan(base: &AngleParams, count: usize) -> AngleVariantPlan {
    let count = count.clamp(ANGLE_VARIANT_MIN, ANGLE_VARIANT_MAX);
    let variants = (0..count)
        .map(|index| {
            let params = if index == 0 {
                *base
            } else {
                let (horizontal, pitch) =
                    ANGLE_VARIANT_OFFSETS[(index - 1) % ANGLE_VARIANT_OFFSETS.len()];
                AngleParams::new(horizontal, pitch, base.camera_distance, base.wide_angle)
            };
            AngleVariant {
                index,
                label: build_angle_label(&params),
                prompt: build_angle_prompt(&params),
                params,
            }
        })
        .collect();
    AngleVariantPlan { count, variants }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_match_the_dialog() {
        let params = AngleParams::default();
        assert_eq!(params.horizontal_angle, 0.0);
        assert_eq!(params.pitch_angle, 9.0);
        assert_eq!(params.camera_distance, 4.8);
        assert!(!params.wide_angle);
    }

    #[test]
    fn label_uses_direction_words() {
        assert_eq!(
            build_angle_label(&AngleParams::default()),
            "AI multi-angle: front view, 9-degree top-down view, camera distance 4.8, Standard lens"
        );
        let left = AngleParams::new(-30.0, -20.0, 5.0, true);
        assert_eq!(
            build_angle_label(&left),
            "AI multi-angle: rotated 30 degrees left, 20-degree low-angle view, camera distance 5.0, Wide lens"
        );
        assert_eq!(
            build_angle_label(&AngleParams::new(0.0, 0.0, 1.0, false)),
            "AI multi-angle: front view, eye-level view, camera distance 1.0, Standard lens"
        );
    }

    #[test]
    fn prompt_embeds_the_label() {
        let params = AngleParams::new(45.0, 0.0, 4.8, false);
        let prompt = build_angle_prompt(&params);
        let label = build_angle_label(&params);
        assert!(prompt.contains("do not merely apply perspective distortion."));
        assert!(prompt.contains(&label));
        assert!(prompt.ends_with(&format!("{label}.")));
    }

    #[test]
    fn plan_is_deterministic_and_bounded() {
        let base = AngleParams::default();
        assert_eq!(angle_variant_plan(&base, 3), angle_variant_plan(&base, 3));
        assert_eq!(angle_variant_plan(&base, 0).count, ANGLE_VARIANT_MIN);
        assert_eq!(angle_variant_plan(&base, 0).variants.len(), 1);
        assert_eq!(angle_variant_plan(&base, 99).count, ANGLE_VARIANT_MAX);
        assert_eq!(
            angle_variant_plan(&base, 99).variants.len(),
            ANGLE_VARIANT_MAX
        );
    }

    #[test]
    fn plan_first_variant_is_the_base_and_prompts_differ() {
        let base = AngleParams::new(10.0, 12.0, 6.0, true);
        let plan = angle_variant_plan(&base, 4);
        assert_eq!(plan.variants[0].params, base);
        assert_eq!(plan.variants[0].index, 0);
        for pair in plan.variants.windows(2) {
            assert_ne!(pair[0].label, pair[1].label);
            assert_ne!(pair[0].prompt, pair[1].prompt);
        }
        assert!(plan
            .variants
            .iter()
            .all(|variant| variant.prompt.contains(&variant.label)));
    }
}
