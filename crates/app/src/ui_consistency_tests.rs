//! Source-level UI consistency gates: dead Slint state, shared motion tokens,
//! animated hover/focus feedback, one-layer Escape behaviour and a render smoke
//! test per key surface.
//!
//! These are plain text scans of the shipped `.slint` tree (no GUI, no
//! screenshots) plus the real window for the render smoke test. They are cheap
//! and they fail when a new property is declared but never wired, when an
//! animation invents a duration/easing, or when a hover state paints without a
//! transition.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slint::ComponentHandle;

/// The inert constants the dead-declaration scan still reports after the
/// interactive-state sweep: unused design-system palette tokens and unused
/// icon path constants. None of them is UI state (nothing sets or reads them),
/// and none is an `app.slint` / component property that drives a control.
/// Token declarations that intentionally stay unreferenced inside the UI: they
/// are part of the design contract (values asserted by the render tests) even
/// where no component currently consumes them.
const INERT_DEAD: &[&str] = &[
    "motion-lift",
    "motion-ants",
    // The Image Studio's canvas surround is the studio palette's own
    // `canvas-surround` now (the studio never reads `Theme` for a colour), so
    // the shared plate wash has no consumer left in the UI tree.
    "image-board-plate",
    // The 34px tool-options band under the menu bar is gone (the options ride in
    // a floating card), so the band height token has no consumer left. It stays
    // in the palette until the shared theme file is next touched.
    "options-bar-h",
    // The app title bar is gone (the home header carries the window controls),
    // so its 40px height token has no consumer. It stays in the palette until
    // the shared theme file is next touched.
    "titlebar-h",
    // The chrome's theme toggle is gone (theme lives in Settings ▸ Appearance),
    // so the moon glyph has no consumer; it stays with the icon set.
    "moon",
    // The PROJECTS header's create action is a plain `+` now, so the folder-plus
    // glyph has no consumer; it stays with the icon set.
    "folder-plus",
    // The home rows and the side-panel rows now wash with the accent at 12/13%
    // (`Theme.node-accent.with-alpha`), so the opaque `brand-soft` plate has no
    // consumer left. It stays in the palette for content surfaces.
    "brand-soft",
    // The drag ghost chip was removed: a dragged node (and a file dragged from
    // an assets card, which becomes a node at the press threshold) is its own
    // preview. The entry fade token stays in the shared theme.
    "motion-ghost",
];

/// Properties whose animation is intentionally structural (hit-testing or a
/// layout swap) rather than a painted transition.
const NON_PAINTED_HOVER_PROPERTIES: &[&str] = &["enabled", "visible", "show-dots"];

fn ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn slint_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_slint(&ui_dir(), &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_slint(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_slint(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "slint") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), text.replace("\r\n", "\n")));
            }
        }
    }
}

fn rust_sources() -> String {
    let mut out = String::new();
    collect_rust(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

fn collect_rust(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let is_test_source =
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.ends_with("_tests.rs") || name == "config_test_support.rs"
                    });
            if is_test_source {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text.replace("\r\n", "\n"));
                out.push('\n');
            }
        }
    }
}

/// `(line number, identifier)` for every `[in|out|in-out|private] property`
/// declaration in a Slint source.
fn declared_properties(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("in-out property <")
            .or_else(|| trimmed.strip_prefix("in property <"))
            .or_else(|| trimmed.strip_prefix("out property <"))
            .or_else(|| trimmed.strip_prefix("private property <"))
            .or_else(|| trimmed.strip_prefix("public property <"))
        else {
            continue;
        };
        let Some((_, after)) = rest.split_once('>') else {
            continue;
        };
        let name: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push((index + 1, name));
        }
    }
    out
}

fn occurrences(haystack: &str, needle: &str, extra_identifier: &[char]) -> usize {
    let mut count = 0;
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let at = start + offset;
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        let boundary = |c: Option<char>| {
            c.is_none_or(|c| {
                !(c.is_ascii_alphanumeric()
                    || c == '_' && extra_identifier.contains(&'_')
                    || c == '-' && extra_identifier.contains(&'-'))
            })
        };
        if boundary(before) && boundary(after) {
            count += 1;
        }
        start = at + needle.len();
    }
    count
}

/// The dead-state sweep: every declared property must be referenced somewhere
/// in Slint or (in `snake_case`) from Rust. Returns `(file, line, name)` for
/// the declarations that are not.
fn dead_declarations() -> Vec<(String, usize, String)> {
    let files = slint_files();
    let slint_text = files
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let rust_text = rust_sources();
    let mut dead = Vec::new();
    for (path, text) in &files {
        for (line, name) in declared_properties(text) {
            let snake = name.replace('-', "_");
            let slint_refs = occurrences(&slint_text, &name, &['-']);
            let rust_refs = occurrences(&rust_text, &snake, &[]);
            if slint_refs + rust_refs <= 1 {
                dead.push((path.clone(), line, name));
            }
        }
    }
    dead
}

#[test]
fn dead_state_count_after_the_sweep() {
    let files = slint_files();
    assert!(
        files.len() >= 30,
        "the scan must see the whole UI tree ({} files)",
        files.len()
    );
    let dead = dead_declarations();
    let names: BTreeSet<&str> = dead.iter().map(|(_, _, name)| name.as_str()).collect();
    let inert: BTreeSet<&str> = INERT_DEAD.iter().copied().collect();
    assert_eq!(
        names, inert,
        "only the known inert palette/icon constants may be unreferenced; found {dead:?}"
    );
    assert_eq!(dead.len(), INERT_DEAD.len(), "dead declarations: {dead:?}");
}

#[test]
fn swept_ui_state_has_no_declaration_left() {
    let app = std::fs::read_to_string(ui_dir().join("app.slint")).unwrap_or_default();
    let image =
        std::fs::read_to_string(ui_dir().join("components/image.slint")).unwrap_or_default();
    for needle in [
        "property <bool> connecting",
        "property <bool> panel-open",
        "property <bool> image-show-grid",
        "property <bool> image-show-guides",
        "property <string> image-text-size",
        "property <string> image-text-color",
    ] {
        assert!(!app.contains(needle), "app.slint still declares `{needle}`");
    }
    assert!(
        !image.contains("property <string> text-size"),
        "image.slint still declares a dead text-size"
    );
    assert!(
        app.contains("panel-open: root.side-panel-open;"),
        "the side panel's panel-open reads the one side-panel-open flag"
    );
    // The brush opacity slider is fed by the one Rust-driven value.
    assert!(
        image.contains("brush-opacity: root.brush-opacity;"),
        "the tool options read the studio's brush-opacity"
    );
    assert!(
        app.contains("brush-opacity: root.image-opacity;"),
        "the studio's brush-opacity comes from Rust's image-opacity"
    );
}

/// Extracts every `animate … { … }` block from one source.
fn animate_blocks(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("animate ") {
            continue;
        }
        let mut depth = 0_i32;
        let mut chunk = Vec::new();
        for (offset, inner) in lines.iter().enumerate().skip(index) {
            chunk.push(*inner);
            depth += i32::try_from(inner.matches('{').count()).unwrap_or(i32::MAX);
            depth -= i32::try_from(inner.matches('}').count()).unwrap_or(i32::MAX);
            if depth <= 0 && offset > index {
                break;
            }
        }
        blocks.push((index + 1, chunk.join("\n")));
    }
    blocks
}

#[test]
fn every_animation_uses_a_shared_motion_token() {
    for (path, text) in slint_files() {
        for (line, block) in animate_blocks(&text) {
            let durations: Vec<&str> = block
                .lines()
                .filter_map(|l| l.trim().strip_prefix("duration:"))
                .collect();
            assert_eq!(
                durations.len(),
                1,
                "{path}:{line}: one duration per animate block"
            );
            let duration = durations[0].trim().trim_end_matches(';').trim();
            assert!(
                duration.contains("Theme.") || duration == "0ms" || duration == "root.hover-motion",
                "{path}:{line}: duration `{duration}` is not a Theme token"
            );
            for literal in [
                "110ms", "150ms", "160ms", "220ms", "420ms", "500ms", "1000ms",
            ] {
                assert!(
                    !duration.contains(literal),
                    "{path}:{line}: literal duration `{duration}`"
                );
            }
            let easing = block
                .lines()
                .find_map(|l| l.trim().strip_prefix("easing:"))
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .trim();
            assert!(
                matches!(
                    easing,
                    "ease-out"
                        | "linear"
                        // The canvas node entrance and the side-panel slide are
                        // the two curves the motion catalog specifies verbatim.
                        | "cubic-bezier(0.2, 0.85, 0.18, 1)"
                        | "cubic-bezier(0.22, 1, 0.36, 1)"
                ),
                "{path}:{line}: easing `{easing}` is not a shared ease"
            );
        }
    }
}

#[test]
fn hover_and_focus_bindings_are_animated() {
    for (path, text) in slint_files() {
        let animated: Vec<String> = animate_blocks(&text)
            .into_iter()
            .flat_map(|(_, block)| {
                block
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .strip_prefix("animate ")
                    .map(|rest| {
                        rest.split(|c: char| c == '{' || c == ',' || c.is_whitespace())
                            .filter(|name| !name.is_empty())
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            let Some((property, _)) = trimmed.split_once(':') else {
                continue;
            };
            let property = property.trim();
            if property.is_empty()
                || !property
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                continue;
            }
            let driven_by_state = line.contains("has-hover")
                || line.contains("has-focus")
                || line.contains(".pressed");
            if !driven_by_state {
                continue;
            }
            if NON_PAINTED_HOVER_PROPERTIES.contains(&property) {
                continue;
            }
            assert!(
                animated.iter().any(|name| name == property),
                "{path}:{}: `{property}` changes on hover/focus without a transition",
                index + 1
            );
        }
    }
}

#[test]
fn escape_closes_one_overlay_layer_per_press() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.set_view("editor".into());
    crate::app::with_app(&app, |app| {
        app.view = crate::app::View::Editor;
        app.config_open = true;
        app.sync_all(&ui);
        // The layer popover needs a real node; set it after the sync (a sync
        // clears a popover whose node no longer exists).
        app.node_layer = Some("layer-target".into());
    });

    let press_escape = || {
        crate::app::with_app(&app, |app| {
            app.key_down(&ui, "\u{1b}", false, false);
        });
    };

    // Config modal is the topmost layer: only it closes.
    press_escape();
    crate::app::with_app(&app, |app| {
        assert!(!app.config_open, "Esc closes the config modal");
        assert!(app.node_layer.is_some(), "the node popover stays");
    });

    // Then the node-level layer popover.
    press_escape();
    crate::app::with_app(&app, |app| {
        assert!(app.node_layer.is_none(), "Esc closes the node popover");
    });

    // Nothing is left above the canvas: one more Escape clears the selection
    // instead of closing a popover.
    press_escape();
    crate::app::with_app(&app, |app| {
        assert!(app.doc.selection().is_empty(), "the selection clears");
        assert!(!app.shortcuts_open);
    });
    Ok(())
}

fn surface_pixels(view: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        app.view = crate::app::View::Editor;
        app.sync_all(&ui);
    });
    ui.set_view(view.into());
    ui.show()?;
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;
    let pixels: Vec<[u8; 4]> = buffer
        .as_slice()
        .iter()
        .map(|p| [p.r, p.g, p.b, p.a])
        .collect();
    let first = pixels.first().copied().unwrap_or_default();
    Ok(pixels.iter().filter(|pixel| **pixel != first).count())
}

#[test]
fn key_surfaces_still_paint() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    for view in ["editor", "library", "image"] {
        let painted = surface_pixels(view)?;
        assert!(
            painted > 10_000,
            "the `{view}` surface paints its chrome ({painted} pixels)"
        );
    }
    Ok(())
}
