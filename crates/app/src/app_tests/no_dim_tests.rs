//! Selection must not fade the rest of the canvas: selecting one node repaints
//! that node's own outline, and nothing else changes. Before this fix the
//! `relatedHighlight` dim multiplied every unrelated card's opacity by 0.62,
//! which altered the whole card's pixels - a measured, pixel-level guard.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use open_canvas_core::node::{CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position};

use crate::app::with_app;
use crate::store::Store;
use crate::test_support::init_backend_allow_existing;
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn text_node(id: &str, body: &str, x: f64, y: f64) -> CanvasNodeData {
    CanvasNodeData {
        id: id.into(),
        node_type: CanvasNodeType::Text,
        title: format!("{id} title"),
        position: Position::new(x, y),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            content: Some(body.to_string()),
            font_size: Some(14.0),
            ..CanvasNodeMetadata::default()
        }),
    }
}

/// Slice-based variant: unlike `test_support::differing_pixels` this also
/// compares alpha, because the dim assertion must catch a faded node.
fn differing_rgba(left: &[slint::Rgba8Pixel], right: &[slint::Rgba8Pixel]) -> usize {
    left.iter()
        .zip(right)
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b || a.a != b.a)
        .count()
}

/// Selecting node A changes A's chrome only. The unrelated node B (340x240 =
/// 81 600 pixels) keeps every pixel: no dim, no fade.
#[test]
fn selecting_a_node_never_dims_an_unrelated_node() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(crate::app::App::new(Store::at(
        dir.path().to_path_buf(),
    ))));
    crate::events::wire(&ui, &app);
    with_app(&app, |app| {
        app.view = crate::app::View::Editor;
        app.doc = crate::doc::Doc::assemble(
            vec![
                text_node(
                    "a",
                    "Alpha node body. The quick brown fox jumps over the lazy dog.",
                    40.0,
                    40.0,
                ),
                text_node(
                    "b",
                    "Beta node body. Pack my box with five dozen liquor jugs.",
                    520.0,
                    40.0,
                ),
            ],
            Vec::new(),
        );
        app.sync_all(&ui);
    });
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    let unselected = ui.window().take_snapshot()?;

    with_app(&app, |app| {
        app.doc.select_node("a", false);
        app.sync_selection(&ui);
        app.sync_flags(&ui);
    });
    ui.window().request_redraw();
    let selected = ui.window().take_snapshot()?;

    let changed = differing_rgba(unselected.as_slice(), selected.as_slice());
    eprintln!("selection repainted {changed} pixels");
    // A's own outline is a couple of thousand pixels; B's card alone would be
    // 81 600 pixels if it were faded, so the bound is unambiguous.
    assert!(
        changed > 0,
        "the selection paints the selected node's chrome"
    );
    assert!(
        changed < 15_000,
        "selection must not repaint the unrelated node ({changed} pixels changed)"
    );
    Ok(())
}
