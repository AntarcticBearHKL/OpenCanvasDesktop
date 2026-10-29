//! Storage scan tests: per-kind aggregation over a real temp directory, the
//! leftover bucket, human formatting and the exports clear action.

use std::error::Error;
use std::fs;

use super::*;

fn write(path: &Path, bytes: usize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, vec![0_u8; bytes])?;
    Ok(())
}

fn kind<'a>(usage: &'a StorageUsage, id: &str) -> Option<&'a StorageKind> {
    usage.kinds.iter().find(|kind| kind.id == id)
}

#[test]
fn scan_aggregates_each_kind_and_the_total() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    write(&root.join("library.json"), 100)?;
    write(&root.join("config.json"), 50)?;
    write(&root.join("ui.json"), 25)?;
    write(&root.join("costs.json"), 75)?;
    write(&root.join("projects/a.json"), 200)?;
    write(&root.join("projects/nested/b.json"), 300)?;
    write(&root.join("images/i.json"), 41)?;
    write(&root.join("exports/one.json"), 10)?;
    write(&root.join("exports/two.json"), 20)?;
    write(&root.join("plugins/x.js"), 7)?;
    write(&root.join("models/model.onnx"), 900)?;
    write(&root.join("stray.txt"), 3)?;

    let usage = scan(root);
    let library = kind(&usage, "library").ok_or("library bucket")?;
    assert_eq!((library.bytes, library.files), (100, 1));
    let config = kind(&usage, "config").ok_or("config bucket")?;
    assert_eq!((config.bytes, config.files), (75, 2));
    let costs = kind(&usage, "costs").ok_or("costs bucket")?;
    assert_eq!((costs.bytes, costs.files), (75, 1));
    let projects = kind(&usage, "projects").ok_or("projects bucket")?;
    assert_eq!((projects.bytes, projects.files), (500, 2));
    let images = kind(&usage, "images").ok_or("images bucket")?;
    assert_eq!((images.bytes, images.files), (41, 1));
    let exports = kind(&usage, "exports").ok_or("exports bucket")?;
    assert_eq!((exports.bytes, exports.files), (30, 2));
    let models = kind(&usage, "models").ok_or("models bucket")?;
    assert_eq!((models.bytes, models.files), (900, 1));
    let other = kind(&usage, "other").ok_or("other bucket")?;
    assert_eq!((other.bytes, other.files), (3, 1));
    let expected: u64 = 100 + 75 + 75 + 500 + 41 + 30 + 7 + 900 + 3;
    assert_eq!(usage.total_bytes, expected);
    assert_eq!(usage.total_files, 12);
    Ok(())
}

#[test]
fn scan_of_a_missing_root_is_empty() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let missing = dir.path().join("does-not-exist");
    let usage = scan(&missing);
    assert_eq!(usage, StorageUsage::default());
    assert_eq!(usage.kinds, Vec::new());
    assert_eq!(usage.total_bytes, 0);
    Ok(())
}

#[test]
fn scan_hides_empty_kinds_and_keeps_display_order() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    write(&root.join("library.json"), 5)?;
    write(&root.join("exports/e.json"), 1)?;
    let usage = scan(root);
    let ids: Vec<&str> = usage.kinds.iter().map(|kind| kind.id).collect();
    assert_eq!(ids, vec!["library", "exports"]);
    Ok(())
}

#[test]
fn human_bytes_uses_readable_buckets() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(1023), "1023 B");
    assert_eq!(human_bytes(1024), "1.0 KB");
    assert_eq!(human_bytes(1536), "1.5 KB");
    assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(human_bytes(1024 * 1024 * 3 / 2), "1.5 MB");
    assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GB");
}

#[test]
fn clear_exports_removes_only_the_export_files() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    write(&root.join("exports/one.json"), 10)?;
    write(&root.join("exports/two.json"), 10)?;
    write(&root.join("projects/keep.json"), 10)?;
    let removed = clear_exports(root)?;
    assert_eq!(removed, 2);
    assert!(!root.join("exports/one.json").exists());
    assert!(root.join("projects/keep.json").exists());
    assert_eq!(clear_exports(root)?, 0, "a second clear is a no-op");
    Ok(())
}

#[test]
fn clear_exports_without_a_directory_is_a_no_op() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    assert_eq!(clear_exports(dir.path())?, 0);
    Ok(())
}
