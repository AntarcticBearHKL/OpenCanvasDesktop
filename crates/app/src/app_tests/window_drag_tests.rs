use super::*;

/// The drag maths: the window follows the cursor 1:1, in physical pixels.
#[test]
fn a_cursor_delta_moves_the_window_by_the_same_delta() {
    assert_eq!(
        drag_target((100, 200), (500, 400), (532, 417)),
        (132, 217),
        "the window origin follows the cursor delta"
    );
    assert_eq!(
        drag_target((100, 200), (500, 400), (500, 400)),
        (100, 200),
        "no cursor movement moves nothing"
    );
    assert_eq!(
        drag_target((100, 200), (500, 400), (480, 388)),
        (80, 188),
        "a left/up drag is negative"
    );
}

/// A drag anchor exists only between begin and end; a tick without one is a
/// no-op, and end clears it so the next drag cannot jump.
#[test]
fn the_drag_anchor_lives_between_begin_and_end() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    with_app(&app, App::drag_end);
    assert!(app.borrow().window_drag.is_none(), "nothing to end");

    with_app(&app, |app| {
        app.window_drag = Some(WindowDrag {
            cursor: (10, 10),
            window: (40, 50),
        });
    });
    with_app(&app, App::drag_end);
    assert!(app.borrow().window_drag.is_none(), "end forgets the anchor");

    // A tick without an anchor must not panic or invent a drag.
    with_app(&app, |app| app.drag_tick(&ui));
    assert!(app.borrow().window_drag.is_none());
    Ok(())
}
