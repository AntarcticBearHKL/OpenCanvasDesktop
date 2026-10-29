//! Media size/resolution helpers for the image/video settings popovers, ported
//! from `web/src/lib/media-size.ts` (scale presets, aspect ratios, pixel
//! computation) plus the label helpers the settings buttons display
//! (`imageQualityLabel`/`imageSizeLabel`/`videoResolutionLabel`/…).

/// Image resolution tiers (`mediaScaleOptions`).
pub const MEDIA_SCALE_OPTIONS: [&str; 4] = ["1k", "2k", "4k", "auto"];
/// Image aspect ratios (`mediaRatioOptions`), in pill order.
pub const MEDIA_RATIO_OPTIONS: [&str; 10] = [
    "1:1", "2:3", "3:2", "4:3", "3:4", "16:9", "9:16", "21:9", "9:21", "auto",
];
/// Video aspect ratios (`videoRatioOptions`).
pub const VIDEO_RATIO_OPTIONS: [&str; 7] = ["1:1", "3:4", "4:3", "16:9", "9:16", "21:9", "auto"];
/// Video resolution pills (`resolutionOptions`).
pub const VIDEO_RESOLUTION_OPTIONS: [&str; 3] = ["480", "720", "1080"];
/// Video duration bounds (`VIDEO_SECONDS_MIN`/`VIDEO_SECONDS_MAX`).
pub const VIDEO_SECONDS_MIN: i64 = 4;
pub const VIDEO_SECONDS_MAX: i64 = 30;
/// Default duration used when the stored value is unparsable.
const VIDEO_SECONDS_DEFAULT: i64 = 6;

/// `imageSizePresets` (media-size.ts:15-19): pixel sizes per scale and ratio.
const IMAGE_SIZE_PRESETS: [(&str, [(&str, &str); 9]); 3] = [
    (
        "1k",
        [
            ("1:1", "1024x1024"),
            ("2:3", "1024x1536"),
            ("3:2", "1536x1024"),
            ("4:3", "1024x768"),
            ("3:4", "768x1024"),
            ("16:9", "1536x864"),
            ("9:16", "864x1536"),
            ("21:9", "2016x864"),
            ("9:21", "864x2016"),
        ],
    ),
    (
        "2k",
        [
            ("1:1", "2048x2048"),
            ("2:3", "1360x2048"),
            ("3:2", "2048x1360"),
            ("4:3", "2048x1536"),
            ("3:4", "1536x2048"),
            ("16:9", "2048x1152"),
            ("9:16", "1152x2048"),
            ("21:9", "2688x1152"),
            ("9:21", "1152x2688"),
        ],
    ),
    (
        "4k",
        [
            ("1:1", "2880x2880"),
            ("2:3", "2336x3520"),
            ("3:2", "3520x2336"),
            ("4:3", "3312x2480"),
            ("3:4", "2480x3312"),
            ("16:9", "3840x2160"),
            ("9:16", "2160x3840"),
            ("21:9", "3840x1648"),
            ("9:21", "1648x3840"),
        ],
    ),
];

/// Parse a `"<width>x<height>"` pixel size.
pub fn parse_pixel_size(value: &str) -> Option<(i64, i64)> {
    let (width, height) = value.split_once(['x', 'X'])?;
    let width: i64 = width.parse().ok()?;
    let height: i64 = height.parse().ok()?;
    (width >= 0 && height >= 0).then_some((width, height))
}

/// Parse an `"<w>:<h>"` aspect ratio with positive terms.
pub fn parse_aspect_ratio(value: &str) -> Option<(f64, f64)> {
    let (width, height) = value.split_once(':')?;
    let width: f64 = width.parse().ok()?;
    let height: f64 = height.parse().ok()?;
    (width > 0.0 && height > 0.0).then_some((width, height))
}

fn normalize_media_scale(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "2k" | "2048" => "2k",
        "4k" | "3840" => "4k",
        "auto" => "auto",
        _ => "1k",
    }
}

/// `inferMediaScale` (media-size.ts:58-70): the scale tier of a size string.
pub fn infer_media_scale(size: &str) -> &'static str {
    if size.is_empty() || size == "auto" {
        return "auto";
    }
    if parse_aspect_ratio(size).is_some() {
        return "auto";
    }
    let Some((width, height)) = parse_pixel_size(size) else {
        return "auto";
    };
    let pixels = format!("{width}x{height}");
    for (scale, presets) in IMAGE_SIZE_PRESETS {
        if presets.iter().any(|(_, value)| *value == pixels) {
            return scale;
        }
    }
    let long_side = width.max(height);
    if long_side >= 3072 {
        "4k"
    } else if long_side >= 1536 {
        "2k"
    } else {
        "1k"
    }
}

fn closest_ratio<'a>(options: &[&'a str], ratio: f64, fallback: &'a str) -> &'a str {
    let mut best = fallback;
    let mut best_distance = f64::MAX;
    for candidate in options.iter().filter(|value| **value != "auto") {
        if let Some((width, height)) = parse_aspect_ratio(candidate) {
            let distance = (width / height - ratio).abs();
            if distance < best_distance {
                best_distance = distance;
                best = candidate;
            }
        }
    }
    best
}

/// `inferMediaRatio` (media-size.ts:72-86), license-free default `1:1`.
pub fn infer_media_ratio(size: &str) -> &'static str {
    if size.is_empty() || size == "auto" {
        return "auto";
    }
    if let Some(known) = MEDIA_RATIO_OPTIONS.into_iter().find(|value| *value == size) {
        return known;
    }
    let Some((width, height)) = parse_pixel_size(size)
        .map(|(w, h)| (w as f64, h as f64))
        .or_else(|| parse_aspect_ratio(size))
    else {
        return "1:1";
    };
    closest_ratio(&MEDIA_RATIO_OPTIONS, width / height, "1:1")
}

/// `computeMediaSize` (media-size.ts:88-93): a preset pixel size for a scale and
/// ratio, `"auto"` when either side is automatic, else the bare ratio.
pub fn compute_media_size(scale: &str, ratio: &str) -> String {
    if ratio == "auto" || ratio.is_empty() {
        return "auto".to_string();
    }
    let scale = normalize_media_scale(scale);
    if scale == "auto" {
        return ratio.to_string();
    }
    IMAGE_SIZE_PRESETS
        .iter()
        .find(|(name, _)| *name == scale)
        .and_then(|(_, presets)| presets.iter().find(|(name, _)| *name == ratio))
        .map_or_else(|| ratio.to_string(), |(_, value)| (*value).to_string())
}

/// `imageQualityLabel` (image-settings-panel.tsx:179-181).
pub fn image_quality_label(value: &str) -> String {
    match value {
        "auto" => "Auto".to_string(),
        "high" => "High".to_string(),
        "medium" => "Medium".to_string(),
        "low" => "Low".to_string(),
        other => other.to_string(),
    }
}

/// `imageSizeLabel` (image-settings-panel.tsx:183-189).
pub fn image_size_label(size: &str) -> String {
    let scale = infer_media_scale(size);
    let ratio = infer_media_ratio(size);
    if ratio == "auto" || size.is_empty() || size == "auto" {
        return "Auto".to_string();
    }
    if scale == "auto" {
        return ratio.to_string();
    }
    format!("{scale} · {ratio}")
}

/// `readMediaDimensions` (media-size.ts:95-100): the W/H a size string encodes.
pub fn read_media_dimensions(size: &str, scale: &str, ratio: &str) -> (i64, i64) {
    if let Some(pixels) = parse_pixel_size(size) {
        return pixels;
    }
    let computed = compute_media_size(
        if scale == "auto" { "1k" } else { scale },
        if ratio == "auto" { "1:1" } else { ratio },
    );
    parse_pixel_size(&computed).unwrap_or((0, 0))
}

/// `clampVideoSeconds` (media-size.ts:102-105): floor to a whole second inside
/// the 4..=30 window, defaulting to 6 for unparsable input.
pub fn clamp_video_seconds(value: &str) -> i64 {
    let seconds = value
        .trim()
        .parse::<f64>()
        .map(|n| n.floor() as i64)
        .unwrap_or(VIDEO_SECONDS_DEFAULT);
    seconds.clamp(VIDEO_SECONDS_MIN, VIDEO_SECONDS_MAX)
}

/// `parseVideoResolution` (media-size.ts:107-117): the pixel tier of a vquality
/// value, defaulting to `"720"`.
pub fn parse_video_resolution(value: &str) -> String {
    let raw = value.trim().to_ascii_lowercase();
    match raw.as_str() {
        "low" => "480".to_string(),
        "1k" => "1024".to_string(),
        "2k" => "2048".to_string(),
        "4k" => "3840".to_string(),
        "auto" | "high" | "medium" | "" => "720".to_string(),
        other => {
            let number = other.strip_suffix('p').unwrap_or(other);
            let valid = !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
                && number.parse::<u32>().is_ok_and(|n| n > 0);
            if valid {
                number.to_string()
            } else {
                "720".to_string()
            }
        }
    }
}

/// `videoResolutionLabel` (video-settings-panel.tsx:128-131).
pub fn video_resolution_label(value: &str) -> String {
    let resolution = parse_video_resolution(value);
    if resolution.chars().all(|c| c.is_ascii_digit()) {
        format!("{resolution}p")
    } else {
        resolution
    }
}

/// `inferVideoRatio` (media-size.ts:119-133), default `16:9`.
pub fn infer_video_ratio(size: &str) -> &'static str {
    if size.is_empty() || size == "auto" {
        return "auto";
    }
    if let Some(known) = VIDEO_RATIO_OPTIONS.into_iter().find(|value| *value == size) {
        return known;
    }
    let Some((width, height)) = parse_pixel_size(size)
        .map(|(w, h)| (w as f64, h as f64))
        .or_else(|| parse_aspect_ratio(size))
    else {
        return "16:9";
    };
    closest_ratio(&VIDEO_RATIO_OPTIONS, width / height, "16:9")
}

/// `computeVideoSize` (media-size.ts:135-144): even-rounded pixel size for a
/// resolution and ratio; `"auto"` when either side is automatic.
pub fn compute_video_size(resolution: &str, ratio: &str) -> String {
    if ratio == "auto" || ratio.is_empty() {
        return "auto".to_string();
    }
    let Some((ratio_w, ratio_h)) = parse_aspect_ratio(ratio) else {
        return "auto".to_string();
    };
    let p = parse_video_resolution(resolution)
        .parse::<f64>()
        .unwrap_or(720.0)
        .max(1.0)
        .floor();
    let landscape = ratio_w >= ratio_h;
    let width = if landscape { p * ratio_w / ratio_h } else { p };
    let height = if landscape { p } else { p * ratio_h / ratio_w };
    format!("{}x{}", even_round(width), even_round(height))
}

/// `videoSizeLabel` (video-settings-panel.tsx:133-136).
pub fn video_size_label(size: &str) -> String {
    let ratio = infer_video_ratio(size);
    if ratio == "auto" {
        "Adaptive".to_string()
    } else {
        ratio.to_string()
    }
}

/// `reasoningEffortLabel` (text-settings-panel.tsx:41-43).
pub fn reasoning_label(value: &str) -> String {
    match value {
        "auto" => "Auto".to_string(),
        "low" => "Low".to_string(),
        "medium" => "Medium".to_string(),
        "high" => "High".to_string(),
        "xhigh" => "XHigh".to_string(),
        other => other.to_string(),
    }
}

fn even_round(value: f64) -> i64 {
    let rounded = (value / 2.0).round() * 2.0;
    rounded.max(2.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_compute_pixel_sizes_for_scale_and_ratio() {
        assert_eq!(compute_media_size("1k", "1:1"), "1024x1024");
        assert_eq!(compute_media_size("2k", "16:9"), "2048x1152");
        assert_eq!(compute_media_size("4k", "9:21"), "1648x3840");
        // auto on either side degrades to the ratio / auto.
        assert_eq!(compute_media_size("1k", "auto"), "auto");
        assert_eq!(compute_media_size("auto", "3:2"), "3:2");
        assert_eq!(compute_media_size("", ""), "auto");
    }

    #[test]
    fn scale_and_ratio_infer_from_size_strings() {
        assert_eq!(infer_media_scale("1024x1536"), "1k");
        assert_eq!(infer_media_scale("2048x2048"), "2k");
        assert_eq!(infer_media_scale("3840x2160"), "4k");
        assert_eq!(infer_media_scale("1:1"), "auto");
        assert_eq!(infer_media_scale("auto"), "auto");
        assert_eq!(infer_media_ratio("1536x1024"), "3:2");
        assert_eq!(infer_media_ratio("1024x1536"), "2:3");
        assert_eq!(infer_media_ratio("16:9"), "16:9");
        assert_eq!(infer_media_ratio(""), "auto");
    }

    #[test]
    fn labels_match_the_web_exports() {
        assert_eq!(image_quality_label("high"), "High");
        assert_eq!(image_quality_label("weird"), "weird");
        assert_eq!(image_size_label("1024x1024"), "1k · 1:1");
        assert_eq!(image_size_label("16:9"), "16:9");
        assert_eq!(image_size_label("auto"), "Auto");
        assert_eq!(video_resolution_label("720"), "720p");
        assert_eq!(video_resolution_label("low"), "480p");
        assert_eq!(video_resolution_label(""), "720p");
        assert_eq!(video_size_label("1280x720"), "16:9");
        assert_eq!(video_size_label("auto"), "Adaptive");
        assert_eq!(reasoning_label("xhigh"), "XHigh");
    }

    #[test]
    fn seconds_clamp_to_the_web_window() {
        assert_eq!(clamp_video_seconds("6"), 6);
        assert_eq!(clamp_video_seconds("2"), 4);
        assert_eq!(clamp_video_seconds("45"), 30);
        assert_eq!(clamp_video_seconds("x"), 6);
        assert_eq!(clamp_video_seconds(""), 6);
    }

    #[test]
    fn video_size_is_even_rounded_per_orientation() {
        assert_eq!(compute_video_size("720", "16:9"), "1280x720");
        assert_eq!(compute_video_size("720", "9:16"), "720x1280");
        assert_eq!(compute_video_size("480", "1:1"), "480x480");
        assert_eq!(compute_video_size("720", "auto"), "auto");
    }

    #[test]
    fn read_dimensions_falls_back_to_the_preset_pixels() {
        assert_eq!(
            read_media_dimensions("1024x1024", "auto", "auto"),
            (1024, 1024)
        );
        assert_eq!(read_media_dimensions("16:9", "1k", "16:9"), (1536, 864));
        assert_eq!(read_media_dimensions("auto", "auto", "auto"), (1024, 1024));
        assert_eq!(parse_pixel_size("10x20"), Some((10, 20)));
        assert_eq!(parse_pixel_size("nope"), None);
        assert_eq!(parse_aspect_ratio("0:1"), None);
    }
}
