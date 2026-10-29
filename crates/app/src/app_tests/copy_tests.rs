//! Copy and focus invariants: the shipped UI carries no replacement character
//! (a mojibake decode failure) and no CJK copy (the product is en-US only), the
//! canvas has no "dim every other node" path, and the library header separator
//! is a clean middle dot.

use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn app_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, extension, out);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            out.push(path);
        }
    }
}

/// Every `.slint` / `.rs` file of the app, sorted for a stable failure message.
fn app_files(extension: &str) -> Vec<PathBuf> {
    let root = app_root();
    let mut files = Vec::new();
    collect(&root.join("ui"), extension, &mut files);
    collect(&root.join("src"), extension, &mut files);
    files.sort();
    files
}

/// Test-only sources: they may carry non-English fixtures on purpose (unicode
/// round-trip tests, rendering probes) and are not shipped copy.
fn is_test_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.ends_with("_tests.rs") || name == "render_tests.rs" {
        return true;
    }
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("app_tests" | "plugin_tests")
        )
    })
}

/// Item 6: no U+FFFD anywhere under `ui/` or `src/`. A replacement character
/// means a byte sequence was decoded with the wrong encoding at some point.
#[test]
fn no_replacement_character_in_ui_or_src() -> TestResult {
    let mut offenders = Vec::new();
    for file in app_files("slint").into_iter().chain(app_files("rs")) {
        let text = std::fs::read_to_string(&file)?;
        if text.contains('\u{FFFD}') {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "files with U+FFFD (mojibake) found: {offenders:?}"
    );
    Ok(())
}

/// Item 7: the shipped copy is English only. Every `.slint` file and every
/// non-test Rust source must be free of CJK code points (which also catches the
/// `路`/`鈥` style mojibake, because those decode to CJK ideographs).
#[test]
fn shipped_copy_is_english_only() -> TestResult {
    let mut offenders = Vec::new();
    for file in app_files("slint").into_iter().chain(
        app_files("rs")
            .into_iter()
            .filter(|path| !is_test_source(path)),
    ) {
        let text = std::fs::read_to_string(&file)?;
        for (line, content) in text.lines().enumerate() {
            if content.chars().any(crate::i18n::is_cjk_or_mojibake) {
                offenders.push(format!("{}:{}", file.display(), line + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "CJK/mojibake copy in shipped sources: {offenders:?}"
    );
    Ok(())
}

/// Item 7 covers the official plugin copy too: the plugin manager and the node
/// cards render the definitions' titles/descriptions straight into the UI.
#[test]
fn official_plugin_copy_is_english_only() -> TestResult {
    let root = app_root().join("../plugins/src/official");
    let mut files = Vec::new();
    collect(&root, "rs", &mut files);
    assert!(!files.is_empty(), "the official plugin sources are found");
    let mut offenders = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        for (line, content) in text.lines().enumerate() {
            if content.chars().any(crate::i18n::is_cjk_or_mojibake) {
                offenders.push(format!("{}:{}", file.display(), line + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "CJK/mojibake copy in the official plugins: {offenders:?}"
    );
    Ok(())
}

/// Item 4: selecting a node must never fade the other nodes. The dim path is
/// gone from both the Rust rows and the Slint card, so the source cannot
/// reintroduce it without this test failing.
#[test]
fn the_canvas_has_no_focus_dim_path() -> TestResult {
    for (file, needles) in [
        ("src/vm.rs", vec!["dimmed", "focus-fade"]),
        ("src/sync/rows.rs", vec!["dimmed", "focus-fade"]),
        (
            "ui/components/node-card.slint",
            vec!["dimmed", "focus-fade"],
        ),
        ("ui/app.slint", vec!["dimmed", "focus-fade"]),
    ] {
        let text = std::fs::read_to_string(app_root().join(file))?;
        for needle in needles {
            assert!(
                !text.contains(needle),
                "{file} still carries the focus-dim path ({needle})"
            );
        }
    }
    // Selection and the related outline remain.
    let card = std::fs::read_to_string(app_root().join("ui/components/node-card.slint"))?;
    assert!(card.contains("in property <bool> related: false;"));
    assert!(card.contains("in property <bool> selected: false;"));
    Ok(())
}
