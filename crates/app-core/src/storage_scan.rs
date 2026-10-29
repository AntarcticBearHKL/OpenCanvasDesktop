//! Storage-usage inspector over the app data directory (`%APPDATA%\
//! open-canvas-desktop`, see `crates/app/src/store.rs`). The web's local-storage tab reads
//! `navigator.storage.estimate()` + `IndexedDB` store sizes
//! (`services/local-storage-usage.ts`); the desktop equivalent is a real
//! filesystem walk, grouped by kind, plus the reveal/clear actions the tab
//! exposes. Read-only except [`clear_exports`], which only removes export
//! envelopes (never a project or config file).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One labeled size bucket (a fixed file, a folder, or the leftover bucket).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageKind {
    pub id: &'static str,
    pub bytes: u64,
    pub files: u64,
}

/// The scanned usage: non-empty kinds in display order plus the totals.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageUsage {
    pub kinds: Vec<StorageKind>,
    pub total_bytes: u64,
    pub total_files: u64,
}

/// Root files grouped into their bucket (first match wins).
const ROOT_FILES: &[(&str, &str)] = &[
    ("library.json", "library"),
    ("config.json", "config"),
    ("ui.json", "config"),
    ("costs.json", "costs"),
];

/// Subdirectories of the data root, one bucket each.
const DIR_KINDS: &[(&str, &str)] = &[
    ("projects", "projects"),
    ("images", "images"),
    ("exports", "exports"),
    ("plugins", "plugins"),
    ("models", "models"),
];

/// The display order of the buckets (mirrors the web's store list order,
/// general first, leftovers last).
const KIND_ORDER: &[&str] = &[
    "library", "config", "costs", "projects", "images", "exports", "plugins", "models", "other",
];

/// Walks `root` and returns the per-kind sizes; a missing root is an empty
/// usage (the data dir only exists after the first write).
pub fn scan(root: &Path) -> StorageUsage {
    let mut totals: Vec<(&'static str, u64, u64)> =
        KIND_ORDER.iter().map(|id| (*id, 0_u64, 0_u64)).collect();
    let mut add = |id: &str, bytes: u64, files: u64| {
        if let Some(slot) = totals.iter_mut().find(|(name, _, _)| *name == id) {
            slot.1 += bytes;
            slot.2 += files;
        }
    };

    let Ok(entries) = fs::read_dir(root) else {
        return StorageUsage::default();
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_dir() {
            let Some((_, kind)) = DIR_KINDS.iter().find(|(dir, _)| *dir == name) else {
                continue;
            };
            let (bytes, files) = dir_size(&entry.path());
            add(kind, bytes, files);
        } else if file_type.is_file() {
            let kind = ROOT_FILES
                .iter()
                .find(|(file, _)| *file == name)
                .map_or("other", |(_, kind)| *kind);
            add(kind, entry.metadata().map_or(0, |meta| meta.len()), 1);
        }
    }

    let kinds: Vec<StorageKind> = KIND_ORDER
        .iter()
        .filter_map(|id| {
            let (_, bytes, files) = *totals.iter().find(|(name, _, _)| name == id)?;
            (files > 0).then_some(StorageKind { id, bytes, files })
        })
        .collect();
    let total_bytes = kinds.iter().map(|kind| kind.bytes).sum();
    let total_files = kinds.iter().map(|kind| kind.files).sum();
    StorageUsage {
        kinds,
        total_bytes,
        total_files,
    }
}

/// Recursive byte/file count of one directory (unreadable entries skipped).
fn dir_size(path: &Path) -> (u64, u64) {
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0);
    };
    let mut bytes = 0;
    let mut files = 0;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let (child_bytes, child_files) = dir_size(&entry.path());
            bytes += child_bytes;
            files += child_files;
        } else if file_type.is_file() {
            bytes += entry.metadata().map_or(0, |meta| meta.len());
            files += 1;
        }
    }
    (bytes, files)
}

/// `1.2 MB` / `512 KB` / `0 B` — one decimal above 1024 (the web shows byte
/// counts with the same coarse buckets through `formatBytes`).
#[expect(
    clippy::cast_precision_loss,
    reason = "display-only formatting of a byte count; precision loss above 2^53 bytes is irrelevant"
)]
pub fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    let value = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", value / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", value / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", value / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Deletes every file inside `<root>\exports` (export envelopes are copies;
/// projects and settings are never touched) and returns how many were removed.
pub fn clear_exports(root: &Path) -> io::Result<u64> {
    let exports = exports_dir(root);
    let entries = match fs::read_dir(&exports) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry?;
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn exports_dir(root: &Path) -> PathBuf {
    root.join("exports")
}

#[cfg(test)]
#[path = "storage_scan_tests.rs"]
mod tests;
