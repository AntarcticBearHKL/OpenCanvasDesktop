//! Input coalescing and the two-tier viewport sync.
//!
//! A desktop mouse reports up to 1000 moves per second, and a high-resolution
//! wheel can report far more. Applying the viewport (and syncing the whole
//! canvas: labels, rulers, dot-grid path, panel) per event is what made the
//! canvas feel laggy. The contract asserted here:
//!
//! * pointer moves and wheel events only *record* state; the 8 ms frame tick
//!   applies it once, so an input rate of any size produces at most one
//!   viewport application and **exactly one** redraw per frame;
//! * the release position is applied exactly, not the last coalesced move;
//! * a coalesced wheel burst is one cursor-anchored zoom, so the world point
//!   under the cursor never moves and zoom-in + zoom-out returns to the same
//!   viewport;
//! * the expensive tiers (labels, rulers, grid, panel) are deferred, so the
//!   per-frame viewport work stays in the hot tier.

use super::*;
use crate::coords::ScreenPos;
use crate::input::{Button, PointerEvent, PointerKind};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type WiredApp = (tempfile::TempDir, AppWindow, Rc<RefCell<App>>);

fn wired() -> TestResult<WiredApp> {
    crate::config_test_support::init_backend()?;
    let (dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.set_view("editor".into());
    Ok((dir, ui, app))
}

fn event(kind: PointerKind, x: f64, y: f64, button: Button) -> PointerEvent {
    PointerEvent {
        kind,
        pos: ScreenPos::new(x, y),
        button,
        shift: false,
        ctrl: false,
    }
}

/// One interactive frame of a pan: `moves` pointer moves and `wheels` wheel
/// events arrive inside the 8 ms frame, then the tick runs.
fn frame(
    app: &Rc<RefCell<App>>,
    ui: &AppWindow,
    clock: &mut u64,
    moves: usize,
    wheels: usize,
    pos: &mut (f64, f64),
) -> (u64, u64) {
    crate::app::with_app_r(app, |app| {
        app.view_stats.reset();
        app.clock_override = Some(*clock);
        for _ in 0..moves {
            pos.0 -= 1.0;
            pos.1 += 0.5;
            *clock += 1;
            app.clock_override = Some(*clock);
            app.canvas_pointer(ui, &event(PointerKind::Move, pos.0, pos.1, Button::Middle));
        }
        for _ in 0..wheels {
            *clock += 1;
            app.clock_override = Some(*clock);
            app.canvas_scroll(ui, ScreenPos::new(pos.0, pos.1), 12.0);
        }
        // The frame tick: one viewport application + one redraw, whatever the
        // input rate was.
        *clock += 1;
        app.clock_override = Some(*clock);
        app.view_tick(ui);
        (app.view_stats.applications, app.view_stats.redraws)
    })
    .unwrap_or((u64::MAX, u64::MAX))
}

/// The headline budget: 1000 Hz input against the 125 Hz tick. 30 frames carry
/// 240 pointer moves and 60 wheel events — the whole second of a fast pan.
#[test]
fn a_thousand_hertz_of_input_costs_one_application_per_frame() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let mut clock = 10_000u64;
    let mut pos = (900.0, 500.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock);
        app.canvas_pointer(&ui, &event(PointerKind::Down, pos.0, pos.1, Button::Middle));
    });

    let (mut applications, mut redraws) = (0u64, 0u64);
    for _ in 0..30 {
        let (applied, drew) = frame(&app, &ui, &mut clock, 8, 2, &mut pos);
        assert_eq!(applied, 1, "one viewport application per frame");
        assert_eq!(drew, 1, "exactly one redraw per frame");
        applications += applied;
        redraws += drew;
    }
    // 240 moves + 60 wheel events in, 30 frames out.
    assert_eq!(
        applications, 30,
        "<= 30 applications for 240 moves + 60 wheel events"
    );
    assert_eq!(redraws, 30, "<= 30 redraws for 240 moves + 60 wheel events");

    // The tick is a rate limit on the *sync*, not on the value: the final
    // position of the last tick is applied (exactness is asserted per se in
    // `the_release_position_is_applied_exactly`).
    let viewport = app.borrow().doc.viewport();
    assert!(
        viewport.x < -100.0,
        "the viewport followed the pan ({viewport:?})"
    );
    assert!(
        viewport.k > 1.0,
        "the wheel zoom was applied ({viewport:?})"
    );

    // Once the input stops, the deferred cold tier lands and the frame cost
    // returns to zero: no application, no redraw.
    crate::app::with_app(&app, |app| {
        for _ in 0..20 {
            clock += 8;
            app.clock_override = Some(clock);
            app.view_stats.reset();
            app.view_tick(&ui);
            assert_eq!(app.view_stats.applications, 0, "idle ticks apply nothing");
            assert_eq!(app.view_stats.redraws, 0, "idle ticks redraw nothing");
        }
        assert!(app.pending.cold_due.is_none(), "the cold tier ran once");
    });
    // The release lands the final position exactly (see the next test for the
    // dedicated assertion).
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock);
        app.canvas_pointer(&ui, &event(PointerKind::Up, pos.0, pos.1, Button::Middle));
    });
    Ok(())
}

/// A release that lands between two ticks applies the *release* position, not
/// the last coalesced move. This is the "no drift at the end of a pan" rule.
#[test]
fn the_release_position_is_applied_exactly() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let clock = 20_000u64;
    let start = (700.0, 400.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Down, start.0, start.1, Button::Middle),
        );
        // Moves arrive and are never ticked: the release alone must land the
        // exact end of the gesture.
        for step in 1..=40 {
            let x = start.0 - f64::from(step) * 2.5;
            let y = start.1 + f64::from(step) * 0.75;
            app.canvas_pointer(&ui, &event(PointerKind::Move, x, y, Button::Middle));
        }
    });
    let release = (start.0 - 40.0 * 2.5 - 0.25, start.1 + 40.0 * 0.75 + 0.125);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock + 5);
        app.canvas_pointer(
            &ui,
            &event(PointerKind::Up, release.0, release.1, Button::Middle),
        );
    });
    let viewport = app.borrow().doc.viewport();
    assert_eq!(
        viewport.x,
        release.0 - start.0,
        "the release x is applied exactly (no sub-pixel drift)"
    );
    assert_eq!(viewport.y, release.1 - start.1, "the release y is exact");
    assert!(
        (f64::from(ui.get_vp_x()) - viewport.x).abs() < 0.001,
        "the window paints the release position"
    );
    assert!(!app.borrow().panning, "the pan ended");
    Ok(())
}

/// A wheel burst in one frame is a single `zoom_at(latest_cursor, sum)`: the
/// world point under the cursor stays fixed, and zooming back out returns to
/// the exact same viewport.
#[test]
fn a_coalesced_wheel_burst_is_cursor_anchored_and_reversible() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let cursor = ScreenPos::new(640.0, 360.0);
    let before = app.borrow().doc.viewport();
    let world_before = before.screen_to_world(cursor);

    // Ten wheel-in events, no tick between them: one application.
    let (applied, redrawn) = crate::app::with_app_r(&app, |app| {
        app.clock_override = Some(1_000);
        app.view_stats.reset();
        for _ in 0..10 {
            app.canvas_scroll(&ui, cursor, 10.0);
        }
        app.clock_override = Some(1_008);
        app.view_tick(&ui);
        (app.view_stats.applications, app.view_stats.redraws)
    })
    .expect("app");
    assert_eq!(applied, 1, "the whole burst is one viewport application");
    assert_eq!(redrawn, 1, "the whole burst is one redraw");

    let zoomed = app.borrow().doc.viewport();
    let expected = 1.1_f64.powf(10.0 * 10.0 * crate::app::App::WHEEL_ZOOM_SPEED / 100.0);
    assert!(
        (zoomed.k - expected).abs() < 1e-9,
        "10 x 10 wheel units compose to the 1.5x-speed zoom ({})",
        zoomed.k
    );
    let world_after = zoomed.screen_to_world(cursor);
    assert!(
        (world_after.x - world_before.x).abs() < 1e-9
            && (world_after.y - world_before.y).abs() < 1e-9,
        "the point under the cursor is fixed: {world_before:?} -> {world_after:?}"
    );

    // The same total delta back out returns to the original viewport.
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(1_016);
        for _ in 0..10 {
            app.canvas_scroll(&ui, cursor, -10.0);
        }
        app.clock_override = Some(1_024);
        app.view_tick(&ui);
    });
    let back = app.borrow().doc.viewport();
    assert!(
        (back.k - before.k).abs() < 1e-9
            && (back.x - before.x).abs() < 1e-9
            && (back.y - before.y).abs() < 1e-9,
        "zoom in + out round-trips: {before:?} -> {back:?}"
    );
    assert!(
        app.borrow().pending.wheel.is_none(),
        "the burst was consumed"
    );
    Ok(())
}

/// A wheel burst uses the hot tier exclusively until the cold settle pass: ten
/// events per 8 ms frame produce one application and one redraw per frame, and
/// the expensive screen-space layers (rulers, panel) run exactly
/// once after the gesture. A trackpad-style dense burst (40 events per frame)
/// is coalesced the same way and never starves the tick.
#[test]
fn a_wheel_burst_runs_only_the_hot_tier_until_the_cold_pass() -> TestResult {
    let (_dir, ui, app) = wired()?;
    crate::app::with_app(&app, |app| app.sync_view(&ui));
    let mut clock = 50_000u64;
    let mut cursor = (500.0, 300.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock);
        app.view_stats.reset();
        for frame in 0..60u64 {
            let events = if frame % 2 == 0 { 10 } else { 40 };
            let k_before = app.doc.viewport().k;
            for _ in 0..events {
                cursor.0 += 0.25;
                cursor.1 += 0.125;
                app.clock_override = Some(clock);
                app.canvas_scroll(&ui, ScreenPos::new(cursor.0, cursor.1), 0.25);
            }
            clock += crate::FRAME_TICK_MS;
            app.clock_override = Some(clock);
            app.view_tick(&ui);
            let expected_k = (k_before
                * 1.1_f64
                    .powf(f64::from(events) * 0.25 * crate::app::App::WHEEL_ZOOM_SPEED / 100.0))
            .min(5.0);
            assert!(
                (app.doc.viewport().k - expected_k).abs() < 1e-12,
                "frame {frame}: the whole batch is one zoom step ({k_before} -> {expected_k}, got {})",
                app.doc.viewport().k
            );
            assert_eq!(
                app.view_stats.applications,
                frame + 1,
                "ten (or forty) wheel events are one application in frame {frame}"
            );
            assert_eq!(
                app.view_stats.redraws,
                frame + 1,
                "one redraw in frame {frame}"
            );
            assert_eq!(app.view_stats.ruler_passes, 0, "no ruler pass mid-burst");
            assert_eq!(
                app.view_stats.follow_passes, 0,
                "the follow tier is skipped for a wheel gesture"
            );
        }
        // The one cold pass, 100 ms after the last wheel event.
        clock += crate::sync::view::VIEW_COLD_SETTLE_MS;
        app.clock_override = Some(clock);
        app.view_tick(&ui);
        assert_eq!(
            app.view_stats.applications, 60,
            "the cold tick applies no viewport"
        );
        assert_eq!(
            app.view_stats.redraws, 60,
            "the cold tick requests no hot redraw"
        );
        assert_eq!(app.view_stats.follow_passes, 0);
        assert_eq!(app.view_stats.cold_passes, 1, "one cold settle pass");
        assert_eq!(app.view_stats.ruler_passes, 1, "rulers sync exactly once");
        assert!(app.pending.wheel.is_none(), "the burst was consumed");
        assert!(app.pending.cold_due.is_none(), "the cold tier ran");
    });
    Ok(())
}

/// The cursor position of the *latest* wheel event is the anchor, so a burst
/// that moves the pointer between events still keeps the last point fixed.
#[test]
fn the_latest_cursor_anchors_the_coalesced_zoom() -> TestResult {
    let (_dir, ui, app) = wired()?;
    // The last wheel event of the burst (step 7 of 0..8).
    let latest = ScreenPos::new(200.0 + 7.0 * 12.5, 650.0 + 7.0 * 7.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(3_000);
        for step in 0..8 {
            let x = 200.0 + f64::from(step) * 12.5;
            app.canvas_scroll(&ui, ScreenPos::new(x, 650.0 + f64::from(step) * 7.0), 9.0);
        }
    });
    let world_before = app.borrow().doc.viewport().screen_to_world(latest);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(3_008);
        app.view_tick(&ui);
    });
    let world_after = app.borrow().doc.viewport().screen_to_world(latest);
    assert!(
        (world_after.x - world_before.x).abs() < 1e-9
            && (world_after.y - world_before.y).abs() < 1e-9,
        "the last cursor position is the anchor: {world_before:?} -> {world_after:?}"
    );
    Ok(())
}

/// A pan frame neither re-rasterises the dot-grid tile nor rebuilds the ruler
/// models: both live in the cold tier, so panning is pure `.slint` bindings.
#[test]
fn a_pan_frame_does_not_regenerate_the_grid_tile() -> TestResult {
    let (_dir, ui, app) = wired()?;
    let mut clock = 40_000u64;
    let mut pos = (800.0, 400.0);
    crate::app::with_app(&app, |app| {
        app.clock_override = Some(clock);
        app.canvas_pointer(&ui, &event(PointerKind::Down, pos.0, pos.1, Button::Middle));
        // Warm the cold tier so the tile exists.
        app.sync_view(&ui);
    });
    let key_before = app.borrow().grid_tile.as_ref().map(|tile| tile.key);
    assert!(key_before.is_some(), "the cold sync built the tile");
    for _ in 0..4 {
        let (applied, redrawn) = frame(&app, &ui, &mut clock, 8, 0, &mut pos);
        assert_eq!((applied, redrawn), (1, 1));
    }
    let key_after = app.borrow().grid_tile.as_ref().map(|tile| tile.key);
    assert_eq!(
        key_after, key_before,
        "panning never re-rasterises the dot tile"
    );
    Ok(())
}

/// A zoom sweep never rebuilds the dot-grid tile: the grid is screen-anchored,
/// so the spacing (and therefore the bitmap) is the same at every zoom level.
#[test]
fn a_zoom_sweep_does_not_regenerate_the_grid_tile() -> TestResult {
    let (_dir, ui, app) = wired()?;
    crate::app::with_app(&app, |app| app.sync_view(&ui));
    let key_before = app.borrow().grid_tile.as_ref().map(|tile| tile.key);
    assert!(key_before.is_some(), "the cold sync built the tile");
    for step in 0..=10 {
        let k = 5.0_f64.powf(f64::from(step) / 10.0);
        crate::app::with_app(&app, |app| {
            app.doc.viewport = crate::coords::Viewport {
                x: -37.5,
                y: 12.0,
                k,
            };
            app.sync_view(&ui);
        });
        assert_eq!(
            app.borrow().grid_tile.as_ref().map(|tile| tile.key),
            key_before,
            "the tile is reused at {k:.2}x"
        );
        assert!(
            (f64::from(ui.get_grid_step()) - crate::paths::GRID_SCREEN_SPACING).abs()
                < f64::from(f32::EPSILON),
            "the lattice spacing stays {} px at {k:.2}x, got {}",
            crate::paths::GRID_SCREEN_SPACING,
            ui.get_grid_step()
        );
    }
    Ok(())
}

/// There is exactly one raw `request_redraw` site, and only the interactive
/// frame paths use it: the viewport hot tier and the coalesced node-gesture
/// tick each paint the frame they just built. A viewport change is one frame,
/// not one repaint per event and per tier; the follow/cold tiers never request
/// their own paint.
#[test]
fn only_the_hot_tier_requests_a_redraw() {
    for (name, source) in [
        ("sync/view.rs", include_str!("../sync/view.rs")),
        ("handlers.rs", include_str!("../handlers.rs")),
        ("main.rs", include_str!("../main.rs")),
    ] {
        let calls = source.matches(".request_redraw()").count();
        let expected = usize::from(name == "sync/view.rs");
        assert_eq!(
            calls, expected,
            "{name} must make {expected} raw redraw request(s) (the shared site), found {calls}"
        );
    }
    // The raw call lives in the shared helper; the hot tier consumes it.
    let view = include_str!("../sync/view.rs");
    let helper = view
        .split("pub(crate) fn request_frame_redraw(")
        .nth(1)
        .and_then(|rest| rest.split("\n    }").next())
        .expect("request_frame_redraw exists");
    assert!(
        helper.contains(".request_redraw()"),
        "the helper wraps the single raw redraw call"
    );
    let hot = view
        .split("pub fn sync_view_hot(")
        .nth(1)
        .and_then(|rest| rest.split("\n    }").next())
        .expect("sync_view_hot exists");
    assert!(
        hot.contains("Self::request_frame_redraw(ui)"),
        "the hot tier goes through the shared redraw site"
    );
    // And the drag frame (handlers.rs) uses the same helper rather than a
    // second raw call.
    let handlers = include_str!("../handlers.rs");
    assert!(
        handlers.contains("Self::request_frame_redraw(ui)"),
        "the node-gesture frame goes through the shared redraw site"
    );
    for tier in ["pub fn sync_view_follow(", "pub fn sync_view_cold("] {
        let body = view
            .split(tier)
            .nth(1)
            .and_then(|rest| rest.split("\n    }").next())
            .unwrap_or_default();
        assert!(
            !body.contains("request_frame_redraw"),
            "{tier} must not request its own redraw"
        );
    }
}
