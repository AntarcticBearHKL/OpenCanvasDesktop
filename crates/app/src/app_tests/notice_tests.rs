//! The toast stack: push, cap, auto-dismiss on the deadline, dismiss by id and
//! the render assertion that a stack of two paints a second row.
use super::*;

#[test]
fn toasts_stack_cap_and_auto_dismiss() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    with_app(&app, |app| {
        app.notices.clear();
        for index in 0..(NOTICE_STACK_MAX + 2) {
            app.set_notice(&ui, format!("notice {index}"), false);
        }
    });

    assert_eq!(
        app.borrow().notices.len(),
        NOTICE_STACK_MAX,
        "the stack is capped"
    );
    assert_eq!(
        app.borrow().notices.first().map(|n| n.text.as_str()),
        Some("notice 2"),
        "the oldest leaves first"
    );
    assert_eq!(
        app.borrow().notices.last().map(|n| n.text.as_str()),
        Some("notice 5"),
        "the newest is last"
    );
    assert!(app.borrow().notice.is_some(), "the newest mirror is set");
    assert_eq!(
        ui.get_notices().row_count(),
        NOTICE_STACK_MAX,
        "the window model mirrors the stack"
    );

    // Nothing is due one tick before the earliest deadline...
    let earliest = app.borrow().notices.first().map_or(0, |n| n.expires_ms);
    assert!(
        !app.borrow_mut().tick_notices_at(earliest - 1),
        "not due yet"
    );
    assert_eq!(app.borrow().notices.len(), NOTICE_STACK_MAX);

    // ...everything is due at the last deadline.
    let latest = app.borrow().notices.last().map_or(0, |n| n.expires_ms);
    assert!(
        app.borrow_mut().tick_notices_at(latest),
        "due at the deadline"
    );
    assert_eq!(app.borrow().notices.len(), 0, "every toast expired");
    assert!(
        app.borrow().notice.is_none(),
        "the mirror follows the stack"
    );
    with_app(&app, |app| app.sync_notice(&ui));
    assert_eq!(
        ui.get_notices().row_count(),
        0,
        "the emptied stack repaints"
    );
    Ok(())
}

#[test]
fn a_failure_lives_longer_and_dismiss_removes_one_row() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    with_app(&app, |app| {
        app.notices.clear();
        app.set_notice(&ui, "saved".to_string(), false);
        app.set_notice(&ui, "failed".to_string(), true);
    });
    // The two deadlines differ by the extra error grace. The two pushes may
    // land either side of a millisecond boundary on a loaded machine, so the
    // difference is compared with a one-millisecond tolerance.
    let first = app.borrow().notices[0].expires_ms;
    let second = app.borrow().notices[1].expires_ms;
    let grace = second.saturating_sub(first);
    let expected = NOTICE_TTL_ERROR_MS - NOTICE_TTL_MS;
    assert!(
        grace.abs_diff(expected) <= 1,
        "a failure lives longer than a status line ({grace} vs {expected} ms)"
    );

    let id = i32::try_from(app.borrow().notices[0].id).unwrap_or(0);
    with_app(&app, |app| app.dismiss_notice(&ui, id));
    assert_eq!(app.borrow().notices.len(), 1, "only the named row leaves");
    assert_eq!(app.borrow().notices[0].text, "failed");
    assert_eq!(ui.get_notices().row_count(), 1, "the model follows");

    with_app(&app, |app| {
        app.notices.clear();
        app.notice = None;
        app.sync_notice(&ui);
    });
    assert_eq!(app.borrow().notices.len(), 0);
    assert_eq!(ui.get_notices().row_count(), 0);
    Ok(())
}

/// The stack paints: two toasts cover more of the shell than one.
#[test]
fn a_second_toast_paints_a_second_row() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    let render = |count: usize| -> Result<
        slint::SharedPixelBuffer<slint::Rgba8Pixel>,
        Box<dyn std::error::Error>,
    > {
        with_app(&app, |app| {
            app.notices.clear();
            app.notice = None;
            for index in 0..count {
                app.set_notice(&ui, format!("toast {index}"), false);
            }
            app.sync_all(&ui);
        });
        ui.set_view("editor".into());
        ui.show()?;
        ui.window().request_redraw();
        Ok(ui.window().take_snapshot()?)
    };

    let one = render(1)?;
    let two = render(2)?;
    let diff = one
        .as_slice()
        .iter()
        .zip(two.as_slice())
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b)
        .count();
    assert!(diff > 500, "the second toast paints its own row ({diff})");
    Ok(())
}
