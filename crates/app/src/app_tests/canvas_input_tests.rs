//! Canvas mouse mapping and the fading zoom read-out.
//!
//! Mapping contract: left-drag on the empty background moves the window (driven
//! from the `.slint` side; a left click without movement clears the selection),
//! middle-drag and right-drag pan the viewport, shift+left-drag keeps the
//! marquee, and Ctrl+right starts the wire-cut stroke. A left press on a node
//! never reaches this handler: the card owns it for select/drag/resize/connect.

use super::*;
use crate::coords::ScreenPos;
use crate::doc::{Doc, Drag};
use crate::input::{Button, PointerEvent, PointerKind};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type WiredApp = (tempfile::TempDir, AppWindow, Rc<RefCell<App>>);

fn wired() -> Result<WiredApp, Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.set_view("editor".into());
    Ok((dir, ui, app))
}

fn event(kind: PointerKind, x: f64, y: f64, button: Button, shift: bool) -> PointerEvent {
    PointerEvent {
        kind,
        pos: ScreenPos::new(x, y),
        button,
        shift,
        ctrl: false,
    }
}

fn ctrl_event(kind: PointerKind, x: f64, y: f64, button: Button) -> PointerEvent {
    PointerEvent {
        kind,
        pos: ScreenPos::new(x, y),
        button,
        shift: false,
        ctrl: true,
    }
}

#[test]
fn left_drag_on_the_background_moves_the_window_not_the_viewport() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.doc = Doc::demo();
        let before = app.doc.viewport();
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Left, false),
        );
        assert!(
            app.doc.drag().is_none(),
            "a left press on the background starts no canvas gesture (the window move lives in the .slint layer)"
        );
        assert!(!app.panning, "a left press does not show the pan cursor");
        assert!(app.canvas_click_pending, "the press is a pending click");
        assert!(
            ui.get_canvas_window_drag(),
            "the .slint layer is armed to move the window"
        );
        #[cfg(windows)]
        assert!(
            app.window_drag.is_some(),
            "the left press records a manual window-drag anchor"
        );

        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 540.0, 260.0, Button::Left, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 540.0, 260.0, Button::Left, false),
        );
        assert_eq!(
            app.doc.viewport(),
            before,
            "a left drag never pans the canvas"
        );
        assert!(!app.canvas_click_pending, "the release consumed the click");
        assert!(
            !ui.get_canvas_window_drag(),
            "the release disarms the window move"
        );
        #[cfg(windows)]
        assert!(
            app.window_drag.is_none(),
            "the release drops the window-drag anchor"
        );
    });
    Ok(())
}

#[test]
fn middle_drag_pans_the_viewport() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let before = app.borrow().doc.viewport();
    with_app(&app, |app| {
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Middle, false),
        );
        assert!(
            matches!(app.doc.drag(), Some(Drag::Pan { .. })),
            "a middle press starts a pan"
        );
        assert!(
            !ui.get_canvas_window_drag(),
            "the middle button never moves the window"
        );
        assert!(ui.get_panning(), "the canvas shows the grabbing cursor");
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 540.0, 260.0, Button::Middle, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 540.0, 260.0, Button::Middle, false),
        );
        assert!(!ui.get_panning(), "the grabbing cursor is cleared");
    });
    let after = app.borrow().doc.viewport();
    assert_eq!(after.x, before.x - 60.0, "the viewport follows the pan");
    assert_eq!(after.y, before.y - 40.0);
    assert!(app.borrow().doc.drag().is_none(), "the pan ended");
    Ok(())
}

#[test]
fn middle_drag_over_a_node_pans_the_viewport() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| app.doc = Doc::demo());
    let (before, node_id) = {
        let app = app.borrow();
        let node_id = app
            .doc
            .nodes()
            .first()
            .map(|node| node.id.clone())
            .ok_or("the demo has a node")?;
        (app.doc.viewport(), node_id)
    };
    with_app(&app, |app| {
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Down, 600.0, 300.0, Button::Middle, false),
        );
        assert!(
            matches!(app.doc.drag(), Some(Drag::Pan { .. })),
            "middle-drag over a node starts a canvas pan"
        );
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Move, 540.0, 260.0, Button::Middle, false),
        );
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Up, 540.0, 260.0, Button::Middle, false),
        );
    });
    let after = app.borrow().doc.viewport();
    assert_eq!(after.x, before.x - 60.0);
    assert_eq!(after.y, before.y - 40.0);
    assert!(app.borrow().doc.drag().is_none());
    Ok(())
}

#[test]
fn middle_drag_over_a_node_follows_the_pointer_before_release() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| app.doc = Doc::demo());
    let (before, node_id) = {
        let app = app.borrow();
        let node_id = app
            .doc
            .nodes()
            .first()
            .map(|node| node.id.clone())
            .ok_or("the demo has a node")?;
        (app.doc.viewport(), node_id)
    };
    with_app(&app, |app| {
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Down, 600.0, 300.0, Button::Middle, false),
        );
        // Slint reports the move with `other` while the middle button is held.
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Move, 540.0, 260.0, Button::Other, false),
        );
        app.view_tick(&ui);
        let moving = app.doc.viewport();
        assert_eq!(moving.x, before.x - 60.0, "the pan follows before release");
        assert_eq!(moving.y, before.y - 40.0);
        app.node_pointer(
            &ui,
            &node_id,
            &event(PointerKind::Up, 540.0, 260.0, Button::Other, false),
        );
    });
    assert!(app.borrow().doc.drag().is_none());
    Ok(())
}

#[test]
fn right_drag_pans_the_viewport() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let before = app.borrow().doc.viewport();
    with_app(&app, |app| {
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Right, false),
        );
        assert!(
            matches!(app.doc.drag(), Some(Drag::Pan { .. })),
            "a plain right press pans"
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 520.0, 240.0, Button::Right, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 520.0, 240.0, Button::Right, false),
        );
        assert!(app.doc.drag().is_none(), "the pan ended");
    });
    let after = app.borrow().doc.viewport();
    assert_eq!(after.x, before.x - 80.0);
    assert_eq!(after.y, before.y - 60.0);
    Ok(())
}

#[test]
fn right_click_toggles_the_side_panel() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.side_open = false;
        ui.set_side_panel_open(false);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Right, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 600.0, 300.0, Button::Right, false),
        );
        assert!(app.side_open, "an unmoved right-click opens the panel");
        assert!(ui.get_side_panel_open(), "the Slint flag follows");
        assert!(app.doc.drag().is_none(), "the right pan ended");

        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Right, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 600.0, 300.0, Button::Right, false),
        );
        assert!(!app.side_open, "a second right-click closes the panel");
        assert!(
            !ui.get_side_panel_open(),
            "the Slint flag follows the close"
        );
    });
    Ok(())
}

#[test]
fn right_hold_fills_the_ring_and_returns_home() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.view = crate::app::View::Editor;
        app.side_open = true;
        ui.set_side_panel_open(true);
        app.clock_override = Some(1_000);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Right, false),
        );
        assert!(
            app.home_hold.is_some(),
            "the still right press arms the hold"
        );
        assert!(
            !ui.get_home_hold_visible(),
            "a quick click does not flash the home ring"
        );

        app.clock_override = Some(1_100);
        app.home_hold_tick(&ui);
        assert!(
            ui.get_home_hold_visible(),
            "the ring appears after the delay"
        );
        assert!(
            !ui.get_home_hold_arc().is_empty(),
            "the visible ring already has a progress arc"
        );

        app.clock_override = Some(1_450);
        app.home_hold_tick(&ui);
        assert!(
            (ui.get_home_hold_progress() - 0.5).abs() < 0.01,
            "half the hold paints half the ring"
        );
        assert!(
            ui.get_home_hold_arc().contains("A 9 9"),
            "the progress arc is a real SVG arc"
        );

        app.clock_override = Some(1_900);
        app.home_hold_tick(&ui);
        assert!(app.home_hold.is_none(), "the completed hold disarms");
        assert!(!ui.get_home_hold_visible(), "the ring disappears");
        assert_eq!(app.view, crate::app::View::Library, "the app is home");
        assert_eq!(ui.get_view(), "library", "the Slint view follows");

        // The release after the navigation gesture must not toggle the rail.
        let side_before = app.side_open;
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 600.0, 300.0, Button::Right, false),
        );
        assert_eq!(app.side_open, side_before, "the release is consumed");
    });
    Ok(())
}

#[test]
fn right_drag_cancels_the_hold_and_does_not_toggle() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.side_open = false;
        ui.set_side_panel_open(false);
        app.clock_override = Some(1_000);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Right, false),
        );
        assert!(app.home_hold.is_some());
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 620.0, 320.0, Button::Right, false),
        );
        assert!(app.home_hold.is_none(), "a drag cancels the hold");
        assert!(!ui.get_home_hold_visible(), "the ring is hidden");
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 620.0, 320.0, Button::Right, false),
        );
        assert!(!app.side_open, "a right-drag is a pan, not a click");
        assert!(!ui.get_side_panel_open());
    });
    Ok(())
}

#[test]
fn home_hold_arc_is_a_clockwise_progress_arc() {
    assert_eq!(crate::app::home_hold_arc(0.0), "");
    let quarter = crate::app::home_hold_arc(0.25);
    assert!(quarter.contains("A 9 9 0 0 1"), "{quarter}");
    let three_quarters = crate::app::home_hold_arc(0.75);
    assert!(three_quarters.contains("A 9 9 0 1 1"), "{three_quarters}");
    let full = crate::app::home_hold_arc(1.0);
    assert!(full.contains("11.99 3"), "{full}");
}

#[test]
fn the_hold_ring_is_wired_into_the_canvas_overlay() {
    let src = include_str!("../../ui/app.slint").replace("\r\n", "\n");
    for needle in [
        "home-hold-visible",
        "home-hold-progress",
        "commands: root.home-hold-arc;",
        "commands: Icons.home;",
    ] {
        assert!(src.contains(needle), "the canvas overlay wires {needle}");
    }
}

#[test]
fn ctrl_right_starts_the_wire_cut_instead_of_a_pan() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.canvas_pointer(
            &ui,
            &ctrl_event(PointerKind::Down, 600.0, 300.0, Button::Right),
        );
        assert!(
            matches!(app.doc.drag(), Some(Drag::Cut { .. })),
            "Ctrl+right is still the wire-cut stroke"
        );
        assert!(!app.panning, "the cut gesture does not pan");
    });
    Ok(())
}

#[test]
fn a_left_click_without_movement_clears_the_selection() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.doc = Doc::demo();
        app.doc.select_node("prompt", false);
        app.sync_selection(&ui);
        assert!(!app.doc.selection().is_empty());

        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Left, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 600.0, 300.0, Button::Left, false),
        );
        assert!(
            app.doc.selection().is_empty(),
            "a click on the empty canvas deselects"
        );
    });
    Ok(())
}

#[test]
fn a_moved_window_drag_keeps_the_selection() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.doc = Doc::demo();
        app.doc.select_node("prompt", false);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Left, false),
        );
        // The `.slint` layer samples the native cursor on the 4 ms timer and
        // records that the window moved before this release.
        app.window_drag_moved = true;
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 540.0, 260.0, Button::Left, false),
        );
        assert!(
            !app.doc.selection().is_empty(),
            "moving the window is not a deselection"
        );
    });
    Ok(())
}

#[test]
fn a_moved_pan_keeps_the_selection() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.doc = Doc::demo();
        app.doc.select_node("prompt", false);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 600.0, 300.0, Button::Middle, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 520.0, 240.0, Button::Middle, false),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 520.0, 240.0, Button::Middle, false),
        );
        assert!(
            !app.doc.selection().is_empty(),
            "panning is not a deselection"
        );
    });
    Ok(())
}

#[test]
fn shift_left_drag_keeps_the_marquee() -> TestResult {
    let (_dir, ui, app) = wired()?;
    with_app(&app, |app| {
        app.doc = Doc::demo();
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, 60.0, 60.0, Button::Left, true),
        );
        assert!(
            matches!(app.doc.drag(), Some(Drag::Marquee { additive: true, .. })),
            "shift keeps the marquee reachable"
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Move, 900.0, 700.0, Button::Left, true),
        );
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, 900.0, 700.0, Button::Left, true),
        );
        assert!(
            !app.doc.selection().is_empty(),
            "the marquee selected the nodes it covered"
        );
    });
    Ok(())
}

#[test]
fn the_zoom_read_out_fades_in_on_zoom_and_hides_on_its_deadline() -> TestResult {
    let (_dir, ui, app) = wired()?;
    assert!(!ui.get_zoom_badge_visible(), "the pill starts hidden");

    with_app(&app, |app| {
        app.canvas_scroll(&ui, ScreenPos::new(500.0, 300.0), 100.0);
        // The wheel event only records the delta; the frame tick applies the
        // 1.5x product wheel step in one exponential zoom.
        app.view_tick(&ui);
        for _ in 0..100 {
            app.view_tick(&ui);
        }
    });
    assert!(ui.get_zoom_badge_visible(), "a zoom change raises the pill");
    assert!(
        app.borrow().zoom_badge_due.is_some(),
        "the hide deadline is armed"
    );
    assert_eq!(
        ui.get_zoom_label(),
        "115%",
        "the pill shows the 1.5x wheel zoom"
    );

    // A second zoom change pushes the deadline back and keeps it visible.
    with_app(&app, |app| {
        app.canvas_scroll(&ui, ScreenPos::new(500.0, 300.0), 100.0);
        app.view_tick(&ui);
        app.zoom_badge_due = Some(u64::MAX);
        assert!(!app.tick_zoom_badge(&ui), "the deadline has not passed");
    });
    assert!(ui.get_zoom_badge_visible(), "the pill stays up");

    // Once the deadline passes the tick hides it exactly once.
    with_app(&app, |app| {
        app.zoom_badge_due = Some(0);
        assert!(app.tick_zoom_badge(&ui), "the deadline fires");
        assert!(!app.tick_zoom_badge(&ui), "an idle tick does nothing");
    });
    assert!(!ui.get_zoom_badge_visible(), "the pill is hidden again");
    Ok(())
}
