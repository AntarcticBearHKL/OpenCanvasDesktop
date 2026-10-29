//! Local segmentation plumbing — port of `web/src/lib/image/mobile-sam-math.ts`
//! and the pre/post-processing in `mobile-sam.ts`. The ONNX runtime stays in the
//! app: this module prepares the model tensors, post-processes the mask and
//! reports a typed [`PsError::SegmenterUnavailable`] when no runtime is supplied.

use serde::{Deserialize, Serialize};

use crate::error::PsError;
use crate::filters::gaussian_blur;
use crate::surface::Surface;

/// `MOBILE_SAM_INPUT_SIZE` (mobile-sam-math.ts:1).
pub const SEGMENT_INPUT_SIZE: usize = 1024;
/// `PAD_COLOR` (mobile-sam.ts:13): the letterbox fill.
pub const SEGMENT_PAD_COLOR: [u8; 3] = [124, 116, 104];
/// The decoder's low-res mask input canvas (mobile-sam.ts:165).
pub const SEGMENT_MASK_INPUT_SIZE: usize = 256;

/// `SamPoint` (mobile-sam-math.ts:3): one prompt point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SamPoint {
    pub x: f64,
    pub y: f64,
    pub positive: bool,
}

impl SamPoint {
    pub const fn new(x: f64, y: f64, positive: bool) -> Self {
        Self { x, y, positive }
    }
}

/// `samResizeScale` (mobile-sam-math.ts:5): scale to fit the longest edge.
pub fn sam_resize_scale(width: usize, height: usize, size: usize) -> f64 {
    let longest = width.max(height);
    if longest > 0 {
        size as f64 / longest as f64
    } else {
        1.0
    }
}

/// `samModelPoint` (mobile-sam-math.ts:10): map a source point into model space.
pub fn sam_model_point(x: f64, y: f64, scale: f64) -> (f64, f64) {
    (x * scale, y * scale)
}

/// The normalized `[size, size, 3]` image tensor (`preprocess`, mobile-sam.ts:99).
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentModelInput {
    pub source_width: usize,
    pub source_height: usize,
    pub size: usize,
    pub scale: f64,
    pub values: Vec<f32>,
}

impl SegmentModelInput {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Letterbox + normalise a surface into the model input (`preprocess`).
pub fn prepare_segment_input(surface: &Surface) -> SegmentModelInput {
    prepare_segment_input_with_size(surface, SEGMENT_INPUT_SIZE)
}

/// [`prepare_segment_input`] with an explicit input size (tests use small sizes).
pub fn prepare_segment_input_with_size(surface: &Surface, size: usize) -> SegmentModelInput {
    let size = size.max(1);
    let (width, height) = (surface.width(), surface.height());
    let scale = sam_resize_scale(width, height, size);
    let resized_width = (width as f64 * scale).round().max(1.0) as usize;
    let resized_height = (height as f64 * scale).round().max(1.0) as usize;
    let resized = surface.resample_bilinear(resized_width, resized_height);
    let mut values = vec![0.0_f32; size * size * 3];
    for pixel in values.as_chunks_mut::<3>().0 {
        *pixel = [
            f32::from(SEGMENT_PAD_COLOR[0]),
            f32::from(SEGMENT_PAD_COLOR[1]),
            f32::from(SEGMENT_PAD_COLOR[2]),
        ];
    }
    let copy_width = resized_width.min(size);
    let copy_height = resized_height.min(size);
    for y in 0..copy_height {
        for x in 0..copy_width {
            let [r, g, b, _] = resized.get(x as i64, y as i64);
            let index = (y * size + x) * 3;
            values[index] = f32::from(r);
            values[index + 1] = f32::from(g);
            values[index + 2] = f32::from(b);
        }
    }
    SegmentModelInput {
        source_width: width,
        source_height: height,
        size,
        scale,
        values,
    }
}

/// The decoder tensors built from prompt points (`segmentImageWithPoints`,
/// mobile-sam.ts:153). `mask_input` is the zeroed low-res prompt mask.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentDecoderInputs {
    pub point_count: usize,
    pub point_coords: Vec<f32>,
    pub point_labels: Vec<f32>,
    pub mask_input: Vec<f32>,
    pub has_mask_input: f32,
    pub orig_size: [f32; 2],
}

pub fn build_decoder_inputs(points: &[SamPoint], scale: f64, size: usize) -> SegmentDecoderInputs {
    let mut point_coords = Vec::with_capacity(points.len() * 2);
    let mut point_labels = Vec::with_capacity(points.len());
    for point in points {
        let (x, y) = sam_model_point(point.x, point.y, scale);
        point_coords.push(x as f32);
        point_coords.push(y as f32);
        point_labels.push(if point.positive { 1.0 } else { 0.0 });
    }
    SegmentDecoderInputs {
        point_count: points.len(),
        point_coords,
        point_labels,
        mask_input: vec![0.0; SEGMENT_MASK_INPUT_SIZE * SEGMENT_MASK_INPUT_SIZE],
        has_mask_input: 0.0,
        orig_size: [size as f32, size as f32],
    }
}

/// `pickBestMaskIndex` (mobile-sam-math.ts:14): the highest IoU prediction, `0`
/// when none are scored.
pub fn pick_best_mask_index(scores: &[f64]) -> usize {
    let mut best = 0;
    for index in 1..scores.len() {
        if scores[index] > scores[best] {
            best = index;
        }
    }
    best
}

/// `clampSamScore` (mobile-sam-math.ts:22): finite and within `0..=1`.
pub fn clamp_sam_score(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `binarizeMaskLogits` (mobile-sam-math.ts:27): `logit > threshold`.
pub fn binarize_mask_logits(logits: &[f32], threshold: f32) -> Vec<u8> {
    logits
        .iter()
        .map(|logit| u8::from(*logit > threshold))
        .collect()
}

/// Post-processing switches for a decoded mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentOptions {
    pub threshold: f32,
    pub feather: f64,
    pub keep_largest: bool,
}

impl Default for SegmentOptions {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            feather: 0.0,
            keep_largest: true,
        }
    }
}

/// What the host segmenter (`crates/ps` never loads a model) returns.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentModelOutput {
    pub mask_logits: Vec<f32>,
    pub mask_width: usize,
    pub mask_height: usize,
    pub scores: Vec<f64>,
    pub mask_index: Option<usize>,
}

/// The post-processed mask: a `Surface` whose alpha channel is the coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentationResult {
    pub mask: Surface,
    pub score: f64,
    pub width: usize,
    pub height: usize,
}

/// The host-provided model runtime (ONNX session). Implemented by the app.
pub trait Segmenter {
    fn segment(
        &mut self,
        input: &SegmentModelInput,
        points: &[SamPoint],
    ) -> Result<SegmentModelOutput, PsError>;
}

/// Threshold a decoded mask plane, keep the largest component, resize back to
/// the source rectangle, then optionally feather it.
pub fn postprocess_mask(
    output: &SegmentModelOutput,
    source_width: usize,
    source_height: usize,
    options: &SegmentOptions,
) -> Result<SegmentationResult, PsError> {
    let (mask_width, mask_height) = (output.mask_width, output.mask_height);
    let plane = mask_width
        .checked_mul(mask_height)
        .filter(|plane| *plane > 0)
        .ok_or_else(|| invalid_mask(format!("{mask_width}x{mask_height}")))?;
    let index = output
        .mask_index
        .unwrap_or_else(|| pick_best_mask_index(&output.scores));
    let start = index
        .checked_mul(plane)
        .ok_or_else(|| invalid_mask(format!("mask index {index}")))?;
    let end = start
        .checked_add(plane)
        .ok_or_else(|| invalid_mask(format!("mask index {index}")))?;
    let logits = output
        .mask_logits
        .get(start..end)
        .ok_or_else(|| invalid_mask(format!("mask index {index} of {}", output.scores.len())))?;
    let mut binary = binarize_mask_logits(logits, options.threshold);
    if options.keep_largest {
        keep_largest_component(&mut binary, mask_width, mask_height);
    }
    let mut small = Surface::new(mask_width, mask_height);
    for y in 0..mask_height {
        for x in 0..mask_width {
            if binary[y * mask_width + x] != 0 {
                small.set(x as i64, y as i64, [255, 255, 255, 255]);
            }
        }
    }
    let mut mask = if mask_width == source_width && mask_height == source_height {
        small
    } else {
        small.resample_bilinear(source_width.max(1), source_height.max(1))
    };
    if options.feather > 0.0 {
        gaussian_blur(&mut mask, options.feather);
    }
    Ok(SegmentationResult {
        mask,
        score: clamp_sam_score(output.scores.get(index).copied().unwrap_or(0.0)),
        width: source_width,
        height: source_height,
    })
}

/// The one honest entry point: prepare tensors, run the host segmenter, then
/// post-process. With no runtime it fails with [`PsError::SegmenterUnavailable`].
pub fn segment_image(
    segmenter: Option<&mut dyn Segmenter>,
    surface: &Surface,
    points: &[SamPoint],
    options: &SegmentOptions,
) -> Result<SegmentationResult, PsError> {
    let Some(segmenter) = segmenter else {
        return Err(PsError::SegmenterUnavailable(
            "no segmentation runtime is configured".to_string(),
        ));
    };
    if points.is_empty() {
        return Err(PsError::Invalid {
            context: "segmentation points".to_string(),
            value: "at least one point is required".to_string(),
        });
    }
    let input = prepare_segment_input(surface);
    let output = segmenter.segment(&input, points)?;
    postprocess_mask(&output, surface.width(), surface.height(), options)
}

fn invalid_mask(detail: String) -> PsError {
    PsError::Invalid {
        context: "segment mask".to_string(),
        value: detail,
    }
}

/// 4-connected flood fill: zero every foreground pixel outside the largest blob.
fn keep_largest_component(mask: &mut [u8], width: usize, height: usize) {
    if width == 0 || height == 0 || mask.len() < width * height {
        return;
    }
    let mut labels = vec![0_u32; width * height];
    let mut best_label = 0_u32;
    let mut best_size = 0_usize;
    let mut next_label = 0_u32;
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..labels.len() {
        if mask[start] == 0 || labels[start] != 0 {
            continue;
        }
        next_label += 1;
        let label = next_label;
        stack.push(start);
        let mut size = 0_usize;
        while let Some(index) = stack.pop() {
            if mask[index] == 0 || labels[index] != 0 {
                continue;
            }
            labels[index] = label;
            size += 1;
            let x = index % width;
            let y = index / width;
            if x > 0 {
                stack.push(index - 1);
            }
            if x + 1 < width {
                stack.push(index + 1);
            }
            if y > 0 {
                stack.push(index - width);
            }
            if y + 1 < height {
                stack.push(index + width);
            }
        }
        if size > best_size {
            best_size = size;
            best_label = label;
        }
    }
    if best_label == 0 {
        return;
    }
    for (index, label) in labels.iter().enumerate() {
        if *label != best_label {
            mask[index] = 0;
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn filled(width: usize, height: usize, rgba: [u8; 4]) -> Surface {
        let mut surface = Surface::new(width, height);
        surface.fill(rgba);
        surface
    }

    fn mask_output(width: usize, height: usize, on: &[usize]) -> SegmentModelOutput {
        let plane = width * height;
        let mut logits = vec![-1.0_f32; plane];
        for index in on {
            logits[*index] = 1.0;
        }
        SegmentModelOutput {
            mask_logits: logits,
            mask_width: width,
            mask_height: height,
            scores: vec![0.8],
            mask_index: None,
        }
    }

    struct DummySegmenter;

    impl Segmenter for DummySegmenter {
        fn segment(
            &mut self,
            _input: &SegmentModelInput,
            _points: &[SamPoint],
        ) -> Result<SegmentModelOutput, PsError> {
            Ok(mask_output(2, 2, &[0, 1, 2, 3]))
        }
    }

    #[test]
    fn resize_scale_and_model_point() {
        assert!((sam_resize_scale(2048, 1024, SEGMENT_INPUT_SIZE) - 0.5).abs() < 1e-9);
        assert_eq!(sam_resize_scale(0, 0, SEGMENT_INPUT_SIZE), 1.0);
        assert_eq!(sam_model_point(10.0, 20.0, 0.5), (5.0, 10.0));
    }

    #[test]
    fn tensor_letterboxes_and_normalises() {
        let surface = filled(4, 2, [200, 100, 50, 255]);
        let input = prepare_segment_input_with_size(&surface, 8);
        assert_eq!(input.size, 8);
        assert!((input.scale - 2.0).abs() < 1e-9);
        assert_eq!(input.len(), 8 * 8 * 3);
        assert_eq!(&input.values[0..3], &[200.0, 100.0, 50.0]);
        assert_eq!(&input.values[72..75], &[200.0, 100.0, 50.0]);
        let pad = (5 * 8) * 3;
        assert_eq!(
            &input.values[pad..pad + 3],
            &[
                f32::from(SEGMENT_PAD_COLOR[0]),
                f32::from(SEGMENT_PAD_COLOR[1]),
                f32::from(SEGMENT_PAD_COLOR[2])
            ]
        );
    }

    #[test]
    fn default_input_size_is_mobile_sam() {
        assert_eq!(SEGMENT_INPUT_SIZE, 1024);
        let input = prepare_segment_input(&filled(1, 1, [0, 0, 0, 255]));
        assert_eq!(input.len(), SEGMENT_INPUT_SIZE * SEGMENT_INPUT_SIZE * 3);
    }

    #[test]
    fn decoder_inputs_match_the_web_tensors() {
        let points = [
            SamPoint::new(10.0, 20.0, true),
            SamPoint::new(30.0, 40.0, false),
        ];
        let inputs = build_decoder_inputs(&points, 0.5, 8);
        assert_eq!(inputs.point_count, 2);
        assert_eq!(inputs.point_coords, vec![5.0, 10.0, 15.0, 20.0]);
        assert_eq!(inputs.point_labels, vec![1.0, 0.0]);
        assert_eq!(
            inputs.mask_input.len(),
            SEGMENT_MASK_INPUT_SIZE * SEGMENT_MASK_INPUT_SIZE
        );
        assert_eq!(inputs.has_mask_input, 0.0);
        assert_eq!(inputs.orig_size, [8.0, 8.0]);
    }

    #[test]
    fn score_helpers_match_the_web() {
        assert_eq!(pick_best_mask_index(&[0.1, 0.9, 0.4]), 1);
        assert_eq!(pick_best_mask_index(&[]), 0);
        assert_eq!(clamp_sam_score(f64::NAN), 0.0);
        assert_eq!(clamp_sam_score(2.0), 1.0);
        assert_eq!(clamp_sam_score(-1.0), 0.0);
        assert_eq!(binarize_mask_logits(&[-1.0, 0.0, 1.0], 0.0), vec![0, 0, 1]);
    }

    #[test]
    fn postprocess_keeps_largest_component() {
        let output = mask_output(4, 4, &[0, 1, 4, 5, 10]);
        let result = postprocess_mask(&output, 4, 4, &SegmentOptions::default()).expect("mask");
        assert_eq!(result.mask.get(0, 0)[3], 255);
        assert_eq!(result.mask.get(1, 1)[3], 255);
        assert_eq!(result.mask.get(2, 2)[3], 0);
        assert_eq!(result.score, 0.8);
    }

    #[test]
    fn postprocess_thresholds_logits() {
        let output = SegmentModelOutput {
            mask_logits: vec![-1.0, 1.0],
            mask_width: 2,
            mask_height: 1,
            scores: vec![0.3],
            mask_index: None,
        };
        let options = SegmentOptions {
            threshold: 0.0,
            feather: 0.0,
            keep_largest: false,
        };
        let result = postprocess_mask(&output, 2, 1, &options).expect("mask");
        assert_eq!(result.mask.get(0, 0)[3], 0);
        assert_eq!(result.mask.get(1, 0)[3], 255);
    }

    #[test]
    fn postprocess_resizes_back_to_source() {
        let output = mask_output(4, 4, &[5, 6, 9, 10]);
        let options = SegmentOptions {
            threshold: 0.0,
            feather: 0.0,
            keep_largest: false,
        };
        let result = postprocess_mask(&output, 8, 8, &options).expect("mask");
        assert_eq!((result.mask.width(), result.mask.height()), (8, 8));
        assert_eq!(result.mask.get(3, 3)[3], 255);
    }

    #[test]
    fn postprocess_feather_softens_edges() {
        let left_half: Vec<usize> = (0..8)
            .flat_map(|y| (0..4).map(move |x| y * 8 + x))
            .collect();
        let output = mask_output(8, 8, &left_half);
        let options = SegmentOptions {
            threshold: 0.0,
            feather: 1.5,
            keep_largest: true,
        };
        let result = postprocess_mask(&output, 8, 8, &options).expect("mask");
        let soft = (0..8)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .any(|(x, y)| {
                let alpha = result.mask.get(x, y)[3];
                alpha > 0 && alpha < 255
            });
        assert!(soft, "feather should leave intermediate alpha values");
    }

    #[test]
    fn postprocess_rejects_bad_shapes() {
        let empty = SegmentModelOutput {
            mask_logits: Vec::new(),
            mask_width: 0,
            mask_height: 0,
            scores: Vec::new(),
            mask_index: None,
        };
        assert!(matches!(
            postprocess_mask(&empty, 1, 1, &SegmentOptions::default()),
            Err(PsError::Invalid { .. })
        ));
        let short = mask_output(2, 2, &[0]);
        let out_of_range = SegmentModelOutput {
            mask_index: Some(5),
            ..short
        };
        assert!(matches!(
            postprocess_mask(&out_of_range, 2, 2, &SegmentOptions::default()),
            Err(PsError::Invalid { .. })
        ));
    }

    #[test]
    fn segment_without_a_runtime_is_unavailable() {
        let surface = filled(4, 4, [1, 2, 3, 255]);
        let points = [SamPoint::new(1.0, 1.0, true)];
        let error = segment_image(None, &surface, &points, &SegmentOptions::default())
            .expect_err("unavailable");
        assert!(matches!(error, PsError::SegmenterUnavailable(_)));
    }

    #[test]
    fn segment_with_a_runtime_and_no_points_is_invalid() {
        let surface = filled(4, 4, [1, 2, 3, 255]);
        let mut segmenter = DummySegmenter;
        let error = segment_image(
            Some(&mut segmenter),
            &surface,
            &[],
            &SegmentOptions::default(),
        )
        .expect_err("invalid");
        assert!(matches!(error, PsError::Invalid { .. }));
    }

    #[test]
    fn segment_round_trips_through_a_runtime() {
        let surface = filled(4, 4, [1, 2, 3, 255]);
        let mut segmenter = DummySegmenter;
        let result = segment_image(
            Some(&mut segmenter),
            &surface,
            &[SamPoint::new(2.0, 2.0, true)],
            &SegmentOptions::default(),
        )
        .expect("segmented");
        assert_eq!((result.mask.width(), result.mask.height()), (4, 4));
        assert_eq!(result.score, 0.8);
    }
}
