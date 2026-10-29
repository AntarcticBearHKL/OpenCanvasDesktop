//! Guard against the retired Pixel and Texture studios creeping back in.
//!
//! The two workspaces were removed entirely (UI, Rust modules, crates, project
//! kinds and storage). This test pins that: no studio symbol may reappear in
//! `crates/app`, the workspace must not list the deleted crates, and the Slint
//! tree must not carry their files, imports or bindings.

use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Source-level markers that only the retired studios used.
const FORBIDDEN_SYMBOLS: [&str; 16] = [
    "open_canvas_pixel",
    "open_canvas_texture",
    "crate::pixel",
    "crate::texture",
    "PixelStudio",
    "TextureStudio",
    "View::Pixel",
    "View::Texture",
    "sync_pixel",
    "sync_texture",
    "PROJECT_KIND_PIXEL",
    "PROJECT_KIND_TEXTURE",
    "pixel_save_now",
    "texture_save_now",
    "library_create_pixel_project",
    "library_create_texture_project",
];

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_dir() -> PathBuf {
    app_dir().join("../..")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn slint_sources(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            slint_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "slint") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn no_pixel_or_texture_studio_symbol_survives_in_the_app() -> TestResult {
    let mut sources = Vec::new();
    rust_sources(&app_dir().join("src"), &mut sources)?;
    let this_file = PathBuf::from(file!());
    for path in &sources {
        if path.file_name() == this_file.file_name() {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        for needle in FORBIDDEN_SYMBOLS {
            assert!(
                !text.contains(needle),
                "{} still references `{needle}`",
                path.display()
            );
        }
    }
    Ok(())
}

#[test]
fn the_studio_modules_and_slint_files_are_gone() -> TestResult {
    for name in [
        "pixel.rs",
        "pixel_actions.rs",
        "pixel_export.rs",
        "pixel_ops.rs",
        "pixel_render.rs",
        "pixel_wire.rs",
        "texture.rs",
        "texture_export.rs",
        "texture_render.rs",
        "texture_wire.rs",
    ] {
        assert!(
            !app_dir().join("src").join(name).exists(),
            "src/{name} still exists"
        );
    }
    let mut files = Vec::new();
    slint_sources(&app_dir().join("ui"), &mut files)?;
    for path in &files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert!(
            !(name.starts_with("pixel") || name.starts_with("texture")),
            "{} is a retired studio file",
            path.display()
        );
        let text = std::fs::read_to_string(path)?;
        assert!(
            !text.contains("PixelStudio") && !text.contains("TextureStudio"),
            "{} still mounts a retired studio",
            path.display()
        );
    }
    Ok(())
}

#[test]
fn the_workspace_manifest_no_longer_lists_the_deleted_crates() -> TestResult {
    let root = workspace_dir();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))?;
    for member in ["crates/pixel", "crates/texture"] {
        assert!(
            !manifest.contains(member),
            "the workspace still lists {member}"
        );
    }
    let app_manifest = std::fs::read_to_string(app_dir().join("Cargo.toml"))?;
    for dependency in ["open-canvas-pixel", "open-canvas-texture"] {
        assert!(
            !app_manifest.contains(dependency),
            "the app still depends on {dependency}"
        );
    }
    assert!(
        !root.join("crates/pixel").exists(),
        "crates/pixel still exists"
    );
    assert!(
        !root.join("crates/texture").exists(),
        "crates/texture still exists"
    );
    Ok(())
}

#[test]
fn the_home_screen_has_no_workspace_switcher_and_no_image_rows() {
    // The Image workspace page and its library filter are gone: there is no
    // `ProjectKind`/`kind_filter`, no workspace tab strip and no kind badge.
    let library = include_str!("library.rs");
    for needle in [
        "ProjectKind",
        "kind_filter",
        "library_create_image_project",
        "library_title(",
    ] {
        assert!(
            !library.contains(needle),
            "the home library still carries the image workspace filter ({needle})"
        );
    }
    let app = include_str!("../ui/app.slint");
    for needle in [
        "TopBar",
        "workspace-tab",
        "select-workspace",
        "library-create-image-project",
    ] {
        assert!(
            !app.contains(needle),
            "app.slint still carries the removed chrome ({needle})"
        );
    }
    let screen = include_str!("../ui/components/library.slint");
    for needle in [
        "create-image-project",
        "kind:",
        "stats",
        "CANVAS",
        "group-count",
    ] {
        assert!(
            !screen.contains(needle),
            "the home screen still carries the removed image/card chrome ({needle})"
        );
    }
    // Image storage and the project factory stay: Smart Canvas depends on them.
    let ops = include_str!("library_ops.rs");
    assert!(ops.contains("create_image_project_file"));
}
