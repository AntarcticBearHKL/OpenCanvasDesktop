//! File-drop classification and drop-point mapping: the half of the OS drop
//! path that needs no window handle.
//!
//! Split out of `crates/app`'s `file_drop` (which keeps the Win32 drop target,
//! the queue and the `App` hook-up) so the pure rules -- which extensions are
//! accepted, how duplicates collapse, how a client point becomes a world point --
//! are testable without the GUI and usable from the asset-folder scan.
//! `crates/app::file_drop` re-exports these names, so `crate::file_drop::classify`
//! and friends keep working there.

use std::collections::HashSet;
use std::path::Path;

use crate::coords::{Viewport, WorldPos};

/// One file handed over by an OS drop, with the drop point in window-client
/// physical pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DroppedFile {
    pub path: String,
    pub client: (f64, f64),
}

/// What an accepted drop becomes on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropKind {
    Image,
    Video,
}

/// Stagger between multiple dropped files (web `STAGGER = 40`).
pub const DROP_STAGGER: f64 = 40.0;

const IMAGE_EXTENSIONS: [&str; 10] = [
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "ico", "tga",
];
const VIDEO_EXTENSIONS: [&str; 8] = ["mp4", "webm", "mov", "m4v", "avi", "mkv", "ogv", "wmv"];

/// The lower-case extension of a path (no dot, no directory), or `None`.
pub fn extension(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Classify a dropped path by extension; anything else is rejected (the drop
/// only ever creates image or video nodes).
pub fn classify(path: &str) -> Option<DropKind> {
    let extension = extension(path)?;
    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        Some(DropKind::Image)
    } else if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        Some(DropKind::Video)
    } else {
        None
    }
}

/// The `image/<ext>` or `video/<ext>` MIME type stored on the node.
pub fn mime_for(path: &str) -> &'static str {
    match extension(path).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("tif" | "tiff") => "image/tiff",
        Some("ico") => "image/x-icon",
        Some("tga") => "image/x-tga",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("m4v") => "video/x-m4v",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        Some("ogv") => "video/ogg",
        Some("wmv") => "video/x-ms-wmv",
        Some("mp4") => "video/mp4",
        _ => "image/png",
    }
}

/// The file name without its directory or extension, for the node title.
pub fn file_title(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Dropped file")
        .to_string()
}

/// The accepted part of a drop: image/video extensions only, duplicates
/// removed (case-insensitively, keeping the first occurrence), order preserved.
/// A path that appears twice in one drop must create one node, not two.
pub fn accepted_paths(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut accepted = Vec::new();
    for path in paths {
        if classify(&path).is_none() {
            continue;
        }
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        accepted.push(path);
    }
    accepted
}

/// Window-client physical pixels ??canvas world coordinates.
///
/// `scale` is the display scale factor (physical ??logical), `canvas_origin` is
/// the canvas area's logical position inside the window (side panel + title
/// bar), and `viewport` is the canvas pan/zoom. The result is the point the
/// node is created at.
pub fn world_at(
    client: (f64, f64),
    scale: f64,
    canvas_origin: (f64, f64),
    viewport: Viewport,
) -> WorldPos {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let k = if viewport.k.is_finite() && viewport.k > 0.0 {
        viewport.k
    } else {
        1.0
    };
    let local_x = client.0 / scale - canvas_origin.0;
    let local_y = client.1 / scale - canvas_origin.1;
    WorldPos::new((local_x - viewport.x) / k, (local_y - viewport.y) / k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Viewport;

    #[test]
    fn images_and_videos_are_accepted_and_others_rejected() {
        assert_eq!(classify(r"C:\art\hero.PNG"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.jpeg"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.webp"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.gif"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.bmp"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.tif"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.tiff"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.ico"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\art\hero.tga"), Some(DropKind::Image));
        assert_eq!(classify(r"C:\clips\loop.webm"), Some(DropKind::Video));
        assert_eq!(classify(r"C:\art\notes.txt"), None);
        assert_eq!(classify(r"C:\art\archive"), None, "no extension");
        assert_eq!(classify(r"C:\art\.png"), None, "a dotfile is not an image");
        assert_eq!(
            classify(r"C:\art\hero.avif"),
            None,
            "a format the media decoder does not link stays out"
        );
        assert_eq!(mime_for(r"C:\art\hero.JPG"), "image/jpeg");
        assert_eq!(mime_for(r"C:\art\hero.webp"), "image/webp");
        assert_eq!(mime_for(r"C:\art\hero.gif"), "image/gif");
        assert_eq!(mime_for(r"C:\art\hero.bmp"), "image/bmp");
        assert_eq!(mime_for(r"C:\art\hero.tiff"), "image/tiff");
        assert_eq!(mime_for(r"C:\art\hero.ico"), "image/x-icon");
        assert_eq!(mime_for(r"C:\art\hero.tga"), "image/x-tga");
        assert_eq!(mime_for(r"C:\clips\loop.mp4"), "video/mp4");
        assert_eq!(file_title(r"C:\art\two words.png"), "two words");
    }

    /// The drop-path pipeline: extension filter first, then case-insensitive
    /// duplicate removal (a file dropped twice is one node).
    #[test]
    fn drop_paths_are_filtered_then_deduped() {
        let accepted = accepted_paths([
            r"C:\art\hero.png".to_string(),
            r"C:\art\notes.txt".to_string(),
            r"C:\art\HERO.PNG".to_string(),
            r"C:\clips\loop.mp4".to_string(),
            r"C:\clips\loop.mp4".to_string(),
            r"C:\art\hero.png".to_string(),
        ]);
        assert_eq!(
            accepted,
            vec![
                r"C:\art\hero.png".to_string(),
                r"C:\clips\loop.mp4".to_string()
            ],
            "keep the first spelling of each accepted path"
        );
        assert_eq!(accepted_paths(Vec::new()), Vec::<String>::new());
        assert_eq!(
            accepted_paths([r"C:\a\b.txt".to_string()]),
            Vec::<String>::new()
        );
    }

    /// The drop point maps from window-client pixels through the display scale,
    /// the canvas origin and the viewport into world coordinates.
    #[test]
    fn drop_point_maps_to_world_coordinates() {
        let identity = Viewport::new();
        let world = world_at((400.0, 240.0), 1.0, (280.0, 40.0), identity);
        assert!((world.x - 120.0).abs() < 1e-6);
        assert!((world.y - 200.0).abs() < 1e-6);

        // At 2x the same physical point is half the logical distance.
        let hidpi = world_at((800.0, 480.0), 2.0, (280.0, 40.0), identity);
        assert!((hidpi.x - 120.0).abs() < 1e-6);
        assert!((hidpi.y - 200.0).abs() < 1e-6);

        // A panned and zoomed canvas: world = (local - viewport) / k.
        let view = Viewport {
            x: -100.0,
            y: 50.0,
            k: 2.0,
        };
        let zoomed = world_at((280.0 + 200.0, 40.0 + 100.0), 1.0, (280.0, 40.0), view);
        assert!((zoomed.x - 150.0).abs() < 1e-6);
        assert!((zoomed.y - 25.0).abs() < 1e-6);

        // Degenerate inputs never produce NaN/inf.
        let fallback = world_at(
            (0.0, 0.0),
            0.0,
            (0.0, 0.0),
            Viewport {
                x: 0.0,
                y: 0.0,
                k: 0.0,
            },
        );
        assert!(fallback.x.is_finite() && fallback.y.is_finite());
    }

    #[test]
    fn stagger_matches_the_web_rule() {
        assert!((DROP_STAGGER - 40.0).abs() < f64::EPSILON);
    }
}
