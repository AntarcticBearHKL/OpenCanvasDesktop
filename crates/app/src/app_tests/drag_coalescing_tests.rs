//! Coalesced node gestures: drag, resize and connect.
//!
//! Node pointer moves used to apply the gesture per event: a 1000 Hz mouse
//! rebuilt the dragged rows, every wire path and every overlay a thousand times
//! per second. The contract asserted here:
//!
//! * a move only records the latest position; the document does not change and
//!   no model is written until the 8 ms frame tick;
//! * one tick applies the batch once, rewrites only the dragged rows in place
//!   and rebuilds only the wire paths touching them;
//! * the release position is applied exactly, even when the last move was never
//!   ticked, and the release frame does one full sync (the one row rebuild);
//! * resize and connect gestures keep the same end state.

use super::*;
use crate::coords::ScreenPos;
use crate::doc::Doc;
use crate::input::{Button, PointerEvent, PointerKind};
use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type WiredApp = (tempfile::TempDir, AppWindow, Rc<RefCell<App>>);

fn wired(doc: Doc) -> TestResult<WiredApp> {
    crate::config_test_support::init_backend()?;
    let (dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.set_view("editor".into());
    crate::app::with_app(&app, |app| {
        app.doc = doc;
        app.sync_all(&ui);
    });
    Ok((dir, ui, app))
}

fn node(id: &str, kind: CanvasNodeType, x: f64, y: f64) -> CanvasNodeData {
    CanvasNodeData {
        id: id.to_string(),
        node_type: kind,
        title: id.to_string(),
        position: Position::new(x, y),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata::default()),
    }
}

fn connection(id: &str, from: &str, to: &str) -> CanvasConnection {
    CanvasConnection {
        id: id.to_string(),
        from_node_id: from.to_string(),
        to_node_id: to.to_string(),
        relation: Some("reference".to_string()),
        kind: None,
        reason: None,
        changes: None,
        agent_run_id: None,
    }
}

/// Six text cards in a row plus one wire from the first to the last, enough to
/// select five and drag them away from the remaining card and wire.
fn drag_doc() -> Doc {
    let nodes = (0..6)
        .map(|index| {
            node(
                &format!("n{index}"),
                CanvasNodeType::Text,
                100.0 + f64::from(index) * 400.0,
                100.0,
            )
        })
        .collect();
    Doc::assemble(nodes, vec![connection("c0", "n0", "n5")])
}

fn pointer(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        kind,
        pos: ScreenPos::new(x, y),
        button: Button::Left,
        shift: false,
        ctrl: false,
    }
}

fn select_dragged(app: &mut App) {
    for index in 0..5 {
        app.doc.select_node(&format!("n{index}"), true);
    }
}

fn positions(doc: &Doc) -> Vec<(String, Position)> {
    doc.nodes()
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect()
}

/// The five cards the drag tests select (`n0`..`n4`); `n5` stays untouched.
fn is_dragged(id: &str) -> bool {
    id.len() == 2 && id.starts_with('n') && matches!(id.as_bytes()[1], b'0'..=b'4')
}

fn geometry_of(app: &App, only_untouched: bool) -> Vec<(String, f32, f32, f32, f32)> {
    (0..app.rows.row_count())
        .filter_map(|index| app.rows.row_data(index))
        .filter(|row| is_dragged(row.id.as_str()) != only_untouched)
        .map(|row| {
            (
                row.id.to_string(),
                row.world_x,
                row.world_y,
                row.width,
                row.height,
            )
        })
        .collect()
}

/// A move batch does not touch the document. The tick applies the latest
/// position once, and the release lands the exact release point even when it
/// was never ticked.
#[test]
fn a_move_waits_for_the_tick_and_the_release_is_exact() -> TestResult {
    let (_dir, ui, app) = wired(drag_doc())?;
    let start = ScreenPos::new(10_000.0, 10_000.0);
    let release = ScreenPos::new(10_240.25, 10_120.125);
    let before = app.borrow().doc.nodes().to_vec();
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(1_000);
        select_dragged(app);
        app.sync_rows_full(&ui);
        app.drag_stats.reset();
        app.node_pointer(&ui, "n0", &pointer(PointerKind::Down, start.x, start.y));
        // Forty moves, no tick: the document stays where it was.
        for step in 1..=40u32 {
            let at = ScreenPos::new(
                start.x + f64::from(step) * 6.0,
                start.y + f64::from(step) * 3.0,
            );
            app.clock_override = Some(1_000 + u64::from(step));
            app.node_pointer(&ui, "n0", &pointer(PointerKind::Move, at.x, at.y));
        }
        assert_eq!(
            positions(&app.doc),
            positions(&Doc::assemble(
                before.clone(),
                vec![connection("c0", "n0", "n5")]
            )),
            "pointer moves only record the pending position"
        );
        assert!(app.pending_drag.to.is_some(), "the latest move is pending");

        // The release arrives before the tick: it must apply the release point,
        // not the last recorded move.
        app.clock_override = Some(1_100);
        app.node_pointer(&ui, "n0", &pointer(PointerKind::Up, release.x, release.y));
    });

    // The reference drag applies the same release position directly. Snapping
    // and the multi-selection are part of it, so the two documents must agree
    // on every node, not just the dragged five.
    let mut expected = Doc::assemble(before, vec![connection("c0", "n0", "n5")]);
    for index in 0..5 {
        expected.select_node(&format!("n{index}"), true);
    }
    expected.begin_node_drag("n0", start);
    expected.node_drag_to(release);

    let app = app.borrow();
    assert_eq!(positions(&app.doc), positions(&expected));
    assert_eq!(app.doc.selection().len(), 5, "the selection stayed intact");
    assert!(
        app.pending_drag.to.is_none(),
        "the release consumed the input"
    );
    assert!(app.doc.drag().is_none(), "the drag ended");
    assert_eq!(
        app.drag_stats.row_rebuilds, 1,
        "the release is the one full row rebuild of the gesture"
    );
    Ok(())
}

/// One tick rewrites only the dragged rows and only the wire paths touching
/// them; the wire string stays byte-identical to a full rebuild.
#[test]
fn the_tick_updates_only_dragged_rows_and_dirty_wires() -> TestResult {
    let (_dir, ui, app) = wired(drag_doc())?;
    let start = ScreenPos::new(10_000.0, 10_000.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(2_000);
        select_dragged(app);
        app.sync_rows_full(&ui);
        // No content here depends on the selection except the rows; give the
        // rows their selection flags before the geometry baseline.
        app.sync_selection(&ui);
        app.sync_wires(&ui);
        app.drag_stats.reset();
        let untouched = geometry_of(app, true);
        assert_eq!(untouched.len(), 1, "the fixture has one non-dragged card");

        app.node_pointer(&ui, "n0", &pointer(PointerKind::Down, start.x, start.y));
        app.clock_override = Some(2_010);
        for step in 1..=8u32 {
            let at = ScreenPos::new(start.x + f64::from(step) * 8.0, start.y);
            app.node_pointer(&ui, "n0", &pointer(PointerKind::Move, at.x, at.y));
        }
        app.clock_override = Some(2_020);
        app.node_drag_tick(&ui);

        assert_eq!(app.drag_stats.frames, 1, "one tick applied the batch");
        assert_eq!(app.drag_stats.redraws, 1, "one redraw for the frame");
        assert_eq!(
            app.drag_stats.row_rebuilds, 0,
            "the live tick never rebuilds the row model"
        );
        assert!(
            app.drag_stats.row_updates <= 5,
            "only the five dragged rows were written ({} writes)",
            app.drag_stats.row_updates
        );
        // The one wire touches a dragged node, so exactly one path is rebuilt
        // and the whole string is pushed once.
        assert_eq!(app.drag_stats.wire_paths, 1, "only the dirty wire path");
        assert_eq!(app.drag_stats.wire_pushes, 1, "one wires string per frame");

        // The incremental cache is exact: the pushed string equals a full
        // rebuild.
        assert_eq!(
            ui.get_wires().as_str(),
            crate::paths::wires_commands(app.doc.nodes(), app.doc.connections()),
            "the incremental wire string is byte-identical"
        );

        assert_eq!(
            geometry_of(app, true),
            untouched,
            "the non-dragged row never changed"
        );

        // Release: exactly one full rebuild, and the rows now carry the final
        // positions of all five dragged nodes.
        app.clock_override = Some(2_030);
        app.node_pointer(
            &ui,
            "n0",
            &pointer(PointerKind::Up, start.x + 64.0, start.y),
        );
    });
    let app = app.borrow();
    assert_eq!(
        app.drag_stats.row_rebuilds, 1,
        "the release is the one row rebuild"
    );
    for index in 0..5 {
        let id = format!("n{index}");
        let position = app.doc.node(&id).map(|node| node.position).ok_or("node")?;
        let row = (0..app.rows.row_count())
            .filter_map(|index| app.rows.row_data(index))
            .find(|row| row.id.as_str() == id)
            .ok_or("row")?;
        assert_eq!(row.world_x, crate::vm::len(position.x), "{id} row x");
        assert_eq!(row.world_y, crate::vm::len(position.y), "{id} row y");
    }
    Ok(())
}

/// Pressing a node clears the wire selection; the drag frame must clear the
/// selected-wire overlay too, or a stale path would stay painted behind the
/// cards until release.
#[test]
fn a_node_drag_clears_the_selected_wire_overlay() -> TestResult {
    let (_dir, ui, app) = wired(drag_doc())?;
    let start = ScreenPos::new(10_000.0, 10_000.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(2_500);
        app.doc.select_connection(Some("c0"));
        app.sync_wires(&ui);
        assert!(
            !ui.get_selected_wire().is_empty(),
            "the wire is highlighted"
        );

        app.node_pointer(&ui, "n0", &pointer(PointerKind::Down, start.x, start.y));
        for step in 1..=8u32 {
            let at = ScreenPos::new(start.x + f64::from(step) * 8.0, start.y);
            app.clock_override = Some(2_500 + u64::from(step));
            app.node_pointer(&ui, "n0", &pointer(PointerKind::Move, at.x, at.y));
        }
        app.clock_override = Some(2_600);
        app.node_drag_tick(&ui);
        assert!(
            ui.get_selected_wire().is_empty(),
            "the stale wire overlay is cleared on the drag frame"
        );
        app.node_pointer(
            &ui,
            "n0",
            &pointer(PointerKind::Up, start.x + 64.0, start.y),
        );
    });
    Ok(())
}

/// A resize follows the same path: moves are coalesced, the tick updates the
/// row geometry in place, and the release lands the exact bounds.
#[test]
fn a_coalesced_resize_lands_the_release_bounds() -> TestResult {
    let (_dir, ui, app) = wired(drag_doc())?;
    let start = ScreenPos::new(500.0, 500.0);
    let release = ScreenPos::new(560.25, 540.125);
    let before = app
        .borrow()
        .doc
        .node("n1")
        .map(|node| (node.position.x, node.position.y, node.width, node.height));
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(3_000);
        app.drag_stats.reset();
        app.resize_pointer(
            &ui,
            "n1",
            "bottom-right",
            &pointer(PointerKind::Down, start.x, start.y),
        );
        for step in 1..=20u32 {
            let at = ScreenPos::new(
                start.x + f64::from(step) * 3.0,
                start.y + f64::from(step) * 2.0,
            );
            app.clock_override = Some(3_000 + u64::from(step));
            app.resize_pointer(
                &ui,
                "n1",
                "bottom-right",
                &pointer(PointerKind::Move, at.x, at.y),
            );
        }
        app.clock_override = Some(3_100);
        app.node_drag_tick(&ui);
        assert_eq!(app.drag_stats.frames, 1, "the resize is coalesced too");
        assert_eq!(
            app.drag_stats.row_rebuilds, 0,
            "the resize does not rebuild rows"
        );
        app.resize_pointer(
            &ui,
            "n1",
            "bottom-right",
            &pointer(PointerKind::Up, release.x, release.y),
        );
    });

    let mut expected = drag_doc();
    expected.begin_resize("n1", crate::gesture::Corner::BottomRight, start);
    expected.resize_to(release);
    let expected = expected
        .node("n1")
        .map(|node| (node.position.x, node.position.y, node.width, node.height));
    let app = app.borrow();
    let actual = app
        .doc
        .node("n1")
        .map(|node| (node.position.x, node.position.y, node.width, node.height));
    assert_ne!(actual, before, "the resize moved the card");
    assert_eq!(actual, expected, "the release bounds are exact");
    let id = "n1";
    let row = (0..app.rows.row_count())
        .filter_map(|index| app.rows.row_data(index))
        .find(|row| row.id.as_str() == id)
        .ok_or("row")?;
    let (x, y, width, height) = actual.ok_or("node")?;
    assert_eq!(row.world_x, crate::vm::len(x), "the row follows the resize");
    assert_eq!(row.world_y, crate::vm::len(y), "the row follows the resize");
    assert_eq!(
        row.width,
        crate::vm::len(width),
        "the row follows the resize"
    );
    assert_eq!(
        row.height,
        crate::vm::len(height),
        "the row follows the resize"
    );
    Ok(())
}

/// A connection drag is coalesced the same way: the tick tracks the target, and
/// the release creates the connection exactly where the pointer was let go.
#[test]
fn a_coalesced_connect_lands_on_the_release_target() -> TestResult {
    let nodes = vec![
        node("image", CanvasNodeType::Image, 100.0, 100.0),
        node("prompt", CanvasNodeType::Prompt, 900.0, 400.0),
    ];
    let (_dir, ui, app) = wired(Doc::assemble(nodes, Vec::new()))?;
    let target = ScreenPos::new(900.0 + 170.0, 400.0 + 120.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(4_000);
        app.connect_pointer(
            &ui,
            "image",
            "source",
            &pointer(PointerKind::Down, 200.0, 200.0),
        );
        for step in 1..=20u32 {
            let t = f64::from(step) / 20.0;
            let x = 200.0 + (target.x - 200.0) * t;
            let y = 200.0 + (target.y - 200.0) * t;
            app.clock_override = Some(4_000 + u64::from(step));
            app.connect_pointer(&ui, "image", "source", &pointer(PointerKind::Move, x, y));
        }
        app.clock_override = Some(4_100);
        app.node_drag_tick(&ui);
        assert_eq!(app.drag_stats.frames, 1, "the connect move is coalesced");
        assert_eq!(
            app.doc.connect_target(),
            Some("prompt"),
            "the tick hit-tests the target"
        );
        app.connect_pointer(
            &ui,
            "image",
            "source",
            &pointer(PointerKind::Up, target.x, target.y),
        );
    });
    let app = app.borrow();
    assert_eq!(app.doc.connections().len(), 1, "the release connected");
    let wire = &app.doc.connections()[0];
    assert_eq!(wire.from_node_id, "image");
    assert_eq!(wire.to_node_id, "prompt");
    assert!(
        app.pending_drag.to.is_none(),
        "the release consumed the input"
    );
    assert!(
        app.wire_cache.matches(app.doc.connections()),
        "the wire cache rebuilt"
    );
    assert_eq!(
        ui.get_wires().as_str(),
        crate::paths::wires_commands(app.doc.nodes(), app.doc.connections()),
        "the new wire is painted"
    );
    Ok(())
}
