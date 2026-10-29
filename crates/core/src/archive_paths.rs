//! Archive member path construction: sanitize project ids, stems, and pick the
//! extension the web exporter would, so paths stay stable and safe.

use std::collections::BTreeSet;

/// Sanitize a project id for use as one path component.
pub(crate) fn sanitize_project_id(value: &str) -> String {
    let cleaned = sanitize(value);
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned
    }
}

/// Build a unique, safe archive path for one asset.
///
/// Collisions (the same storage key used by two different payloads) get a
/// numeric suffix so every member name stays unique.
pub(crate) fn unique_path(
    project_id: &str,
    storage_key: &str,
    mime_type: &str,
    used: &BTreeSet<String>,
) -> String {
    let stem = sanitize_stem(storage_key);
    let extension = extension_for(mime_type, storage_key);
    let mut candidate = format!("projects/{project_id}/files/{stem}.{extension}");
    let mut suffix = 2u32;
    while used.contains(&candidate) {
        candidate = format!("projects/{project_id}/files/{stem}-{suffix}.{extension}");
        suffix = suffix.saturating_add(1);
    }
    candidate
}

fn sanitize_stem(value: &str) -> String {
    let cleaned = sanitize(value);
    if cleaned.is_empty() {
        "blob".to_string()
    } else {
        cleaned
    }
}

/// Replace path separators and Windows-forbidden characters; trim leading or
/// trailing dots so a value can never become `.`/`..` or a hidden name.
fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| match character {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other if other.is_control() => '_',
            other => other,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    trimmed.chars().take(80).collect()
}

/// The extension the web exporter maps a media type to (`fileExtension`).
fn extension_for(mime_type: &str, storage_key: &str) -> &'static str {
    if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("jpeg") {
        "jpg"
    } else if mime_type.contains("webp") {
        "webp"
    } else if mime_type.contains("gif") {
        "gif"
    } else if mime_type.contains("mp4") {
        "mp4"
    } else if mime_type.contains("webm") {
        "webm"
    } else if mime_type.contains("mpeg") || mime_type.contains("mp3") {
        "mp3"
    } else if mime_type.contains("midi") {
        "mid"
    } else if mime_type.contains("wav") {
        "wav"
    } else if mime_type.contains("ogg") {
        "ogg"
    } else if storage_key.starts_with("image:") {
        "png"
    } else {
        "bin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_and_falls_back_when_empty() {
        assert_eq!(sanitize_project_id(""), "project");
        assert_eq!(sanitize_project_id("p:1/../x"), "p_1_.._x");
        assert_eq!(sanitize_project_id(".."), "project");
        assert_eq!(sanitize_project_id("..a.."), "a");
    }

    #[test]
    fn unique_paths_avoid_collisions() {
        let mut used = BTreeSet::new();
        let first = unique_path("p1", "image:abc", "image/png", &used);
        used.insert(first.clone());
        let second = unique_path("p1", "image:abc", "image/png", &used);
        assert_ne!(first, second);
        assert!(first.ends_with("image_abc.png"));
        assert!(second.ends_with("image_abc-2.png"));
    }

    #[test]
    fn extensions_match_the_web_mapping() {
        assert_eq!(extension_for("image/jpeg", "image:a"), "jpg");
        assert_eq!(extension_for("video/webm", "media:a"), "webm");
        assert_eq!(extension_for("audio/mpeg", "media:a"), "mp3");
        assert_eq!(extension_for("application/octet-stream", "image:a"), "png");
        assert_eq!(extension_for("application/pdf", "media:a"), "bin");
    }
}
