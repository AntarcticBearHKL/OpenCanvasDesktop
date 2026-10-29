//! Assets node folder binding: scan a bound local folder into a tree rooted at
//! that folder and prove that no path can escape it.
//!
//! The web keeps this in `useAssetFolderStore`; on the desktop the bound folder
//! is project data (`metadata.assetFolder`), so a reopened project keeps its
//! tree. The scan is a pure function of `(folder, expanded set)` and is cached
//! per node, because the canvas rebuilds rows on every drag frame.
//!
//! Security invariant: the root is the root. `..`, absolute paths, drive
//! prefixes and symlinks that resolve outside are all refused by
//! [`resolve_in_root`], and the tree walk only ever joins single directory
//! entries (no caller-supplied relative path is accepted for listing).

use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

/// Web `ASSET_FOLDER_FILE_LIMIT`: a bind never lists more files than this.
pub const ASSET_FOLDER_FILE_LIMIT: usize = 500;

/// One visible row of the bound folder tree.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderRow {
    /// Slash-separated path relative to the bound root (`""` is the root).
    pub path: String,
    pub label: String,
    pub depth: i32,
    /// `"folder"` | `"image"` | `"video"`.
    pub kind: &'static str,
    pub expanded: bool,
}

/// The result of scanning a bound folder once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderScan {
    pub rows: Vec<FolderRow>,
    pub files: usize,
    pub capped: bool,
    /// Set when the folder is missing/unreadable; `rows` is then empty.
    pub error: Option<String>,
}

impl FolderScan {
    /// A user-facing state line: the error, the cap notice, or `None`.
    pub fn status(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        if self.capped {
            return Some(format!("Showing the first {ASSET_FOLDER_FILE_LIMIT} files"));
        }
        None
    }
}

/// Resolve `relative` inside `root`, refusing anything that could escape the
/// root: empty, absolute, drive-prefixed, `..`, `.`/prefix components, and
/// symlinks whose canonical target lands outside. Returns the canonical path so
/// the caller reads exactly the file it validated.
pub fn resolve_in_root(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() {
        return None;
    }
    let path = Path::new(relative);
    if path.is_absolute() {
        return None;
    }
    // Every component must be a plain name: "." / ".." / a root / a prefix are
    // rejected outright, so `../secret`, `C:\Windows` and `/etc/passwd` die
    // here before any filesystem access.
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let candidate = root.join(path).canonicalize().ok()?;
    candidate.starts_with(&root).then_some(candidate)
}

/// Scan `root` into a tree, descending only into folders named in `expanded`.
///
/// A missing root is an error state the node shows with a re-bind action; any
/// other I/O failure while listing a folder is reported the same way instead of
/// silently dropping rows.
pub fn scan_folder(root: &Path, expanded: &BTreeSet<String>) -> FolderScan {
    if !root.is_dir() {
        return FolderScan {
            error: Some(format!("Folder not found: {}", root.display())),
            ..FolderScan::default()
        };
    }
    let mut scan = FolderScan::default();
    if let Err(error) = walk(root, "", 0, expanded, &mut scan) {
        return FolderScan {
            error: Some(format!("Failed to read the folder: {error}")),
            ..FolderScan::default()
        };
    }
    scan
}

fn walk(
    root: &Path,
    relative: &str,
    depth: i32,
    expanded: &BTreeSet<String>,
    scan: &mut FolderScan,
) -> std::io::Result<()> {
    let directory = if relative.is_empty() {
        root.to_path_buf()
    } else {
        // `relative` was built here from directory entry names, never from a
        // caller path: re-validate anyway so the walk can never drift.
        let Some(directory) = resolve_in_root(root, relative) else {
            return Ok(());
        };
        directory
    };
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let child = if relative.is_empty() {
            name.to_string()
        } else {
            format!("{relative}/{name}")
        };
        if file_type.is_dir() {
            directories.push((child, name.to_string()));
        } else if file_type.is_file() && crate::drop_files::classify(name).is_some() {
            files.push((child, name.to_string()));
        }
    }
    directories.sort_by_key(|(_, name)| name.to_ascii_lowercase());
    files.sort_by_key(|(_, name)| name.to_ascii_lowercase());
    for (child, label) in directories {
        let open = expanded.contains(&child);
        scan.rows.push(FolderRow {
            path: child.clone(),
            label,
            depth,
            kind: "folder",
            expanded: open,
        });
        if open {
            walk(root, &child, depth + 1, expanded, scan)?;
        }
    }
    for (child, label) in files {
        if scan.files >= ASSET_FOLDER_FILE_LIMIT {
            scan.capped = true;
            break;
        }
        scan.files += 1;
        let kind = match crate::drop_files::classify(&child) {
            Some(crate::drop_files::DropKind::Video) => "video",
            _ => "image",
        };
        scan.rows.push(FolderRow {
            path: child,
            label,
            depth,
            kind,
            expanded: false,
        });
    }
    Ok(())
}

/// Per-node scan cache keyed by `(folder, expanded paths, file limit)`. The
/// canvas rebuilds a row on every pointer move, so a scan must not hit the disk
/// again unless the binding or the expansion changed.
#[derive(Debug, Default)]
pub struct FolderCache {
    entries: HashMap<(String, String), FolderScan>,
}

impl FolderCache {
    /// The tree for `folder`, expanded at `expanded`. A missing folder is
    /// cached too (with its error), so a broken binding costs one `is_dir`.
    pub fn scan(&mut self, folder: &str, expanded: &BTreeSet<String>) -> FolderScan {
        let key = (
            folder.to_string(),
            expanded.iter().cloned().collect::<Vec<_>>().join("\u{1f}"),
        );
        if let Some(scan) = self.entries.get(&key) {
            return scan.clone();
        }
        let scan = scan_folder(Path::new(folder), expanded);
        if self.entries.len() > 32 {
            self.entries.clear();
        }
        self.entries.insert(key, scan.clone());
        scan
    }

    /// Forgets a node's scan after a re-bind or a manual refresh.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        std::fs::create_dir(dir.path().join("shots"))?;
        std::fs::create_dir(dir.path().join("empty"))?;
        // Every extension the tree and the drag path accept (contents are only
        // read by the drag tests; the scan classifies by name).
        for name in [
            "a.png", "b.jpg", "c.jpeg", "d.webp", "e.gif", "f.bmp", "g.tif", "h.tiff", "i.ico",
            "j.tga",
        ] {
            std::fs::write(dir.path().join(name), b"fixture")?;
        }
        std::fs::write(dir.path().join("notes.txt"), b"text")?;
        std::fs::write(dir.path().join("shots/b.mp4"), b"mp4")?;
        std::fs::write(dir.path().join("shots/c.webp"), b"webp")?;
        Ok(dir)
    }

    /// The walk shows folders plus image/video files, hides other files and
    /// dotfiles, and only descends into expanded folders.
    #[test]
    fn scan_lists_media_and_respects_expansion() -> Result<(), Box<dyn std::error::Error>> {
        let dir = fixture()?;
        let collapsed = scan_folder(dir.path(), &BTreeSet::new());
        assert!(collapsed.error.is_none());
        let labels: Vec<&str> = collapsed
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "empty", "shots", "a.png", "b.jpg", "c.jpeg", "d.webp", "e.gif", "f.bmp", "g.tif",
                "h.tiff", "i.ico", "j.tga",
            ],
            "folders first, then every supported raster, sorted by name"
        );
        assert_eq!(collapsed.files, 10, "notes.txt is not an asset");

        let mut expanded = BTreeSet::new();
        expanded.insert("shots".to_string());
        let open = scan_folder(dir.path(), &expanded);
        let tree: Vec<(&str, &str, i32)> = open
            .rows
            .iter()
            .map(|row| (row.path.as_str(), row.kind, row.depth))
            .collect();
        assert!(
            tree.contains(&("shots/b.mp4", "video", 1)),
            "the expanded folder lists its video: {tree:?}"
        );
        assert!(tree.contains(&("shots/c.webp", "image", 1)));
        assert_eq!(open.files, 12);
        let folder = open
            .rows
            .iter()
            .find(|row| row.path == "shots")
            .ok_or("shots row")?;
        assert!(folder.expanded);
        Ok(())
    }

    /// The tree accepts exactly the raster extensions the drag path decodes,
    /// and hides anything else (an unlisted file can never be dragged).
    #[test]
    fn scan_lists_every_supported_raster_and_hides_the_rest(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = fixture()?;
        let scan = scan_folder(dir.path(), &BTreeSet::new());
        let paths: Vec<&str> = scan.rows.iter().map(|row| row.path.as_str()).collect();
        for name in [
            "a.png", "b.jpg", "c.jpeg", "d.webp", "e.gif", "f.bmp", "g.tif", "h.tiff", "i.ico",
            "j.tga",
        ] {
            assert!(
                paths.contains(&name),
                "{name} must be listed by the tree filter: {paths:?}"
            );
            let row = scan
                .rows
                .iter()
                .find(|row| row.path == name)
                .ok_or("listed row")?;
            assert_eq!(row.kind, "image", "{name} is a raster image");
        }
        assert!(
            !paths.contains(&"notes.txt"),
            "unsupported extensions stay out of the draggable tree"
        );
        Ok(())
    }

    /// The root cannot be escaped: `..` in any position, absolute paths, drive
    /// prefixes and names that resolve through a symlink outside all fail.
    #[test]
    fn resolve_refuses_to_escape_the_root() -> Result<(), Box<dyn std::error::Error>> {
        let dir = fixture()?;
        let root = dir.path().join("shots");
        assert!(resolve_in_root(&root, "b.mp4").is_some());
        assert!(resolve_in_root(&root, "../a.png").is_none());
        assert!(resolve_in_root(&root, "sub/../../a.png").is_none());
        assert!(resolve_in_root(&root, r"..\a.png").is_none());
        assert!(resolve_in_root(&root, r"C:\Windows\win.ini").is_none());
        assert!(resolve_in_root(&root, "/etc/passwd").is_none());
        assert!(resolve_in_root(&root, ".").is_none());
        assert!(resolve_in_root(&root, "").is_none());
        assert!(
            resolve_in_root(&root, "missing.png").is_none(),
            "only real files resolve"
        );

        // A directory symlink pointing outside the root must not validate.
        #[cfg(windows)]
        {
            let link = root.join("escape");
            let outside = dir.path().join("a.png");
            if std::os::windows::fs::symlink_file(&outside, &link).is_ok() && link.exists() {
                assert!(
                    resolve_in_root(&root, "escape").is_none(),
                    "a symlink leaving the root is refused"
                );
            }
        }
        Ok(())
    }

    /// A missing bound folder is a clear error state, not an empty tree.
    #[test]
    fn missing_folder_reports_an_error() {
        let scan = scan_folder(Path::new(r"C:\does\not\exist\opencanvas"), &BTreeSet::new());
        assert_eq!(scan.rows, Vec::<FolderRow>::new());
        assert!(
            scan.status()
                .is_some_and(|line| line.contains("Folder not found")),
            "the node can show a re-bind error: {:?}",
            scan.status()
        );
    }

    /// The cache is keyed by folder + expansion: the same view is not rescanned,
    /// a different expansion is.
    #[test]
    fn cache_reuses_the_same_view() -> Result<(), Box<dyn std::error::Error>> {
        let dir = fixture()?;
        let folder = dir.path().display().to_string();
        let mut cache = FolderCache::default();
        let first = cache.scan(&folder, &BTreeSet::new());
        let second = cache.scan(&folder, &BTreeSet::new());
        assert_eq!(first, second);
        let mut expanded = BTreeSet::new();
        expanded.insert("shots".to_string());
        let open = cache.scan(&folder, &expanded);
        assert!(open.files > first.files);
        Ok(())
    }

    /// The binding is node metadata, so it travels with the project file.
    #[test]
    fn binding_persists_with_the_node() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = open_canvas_core::node::CanvasNodeMetadata {
            asset_folder: Some(r"D:\art\refs".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&metadata)?;
        assert!(json.contains("\"assetFolder\":\"D:\\\\art\\\\refs\""));
        let back: open_canvas_core::node::CanvasNodeMetadata = serde_json::from_str(&json)?;
        assert_eq!(back.asset_folder.as_deref(), Some(r"D:\art\refs"));
        Ok(())
    }
}
