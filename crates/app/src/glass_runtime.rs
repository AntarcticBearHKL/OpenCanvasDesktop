//! Capture-based frosted backdrop loop.
//!
//! This is the asset-library implementation ported to `OpenCanvas`: the whole
//! virtual desktop is photographed once, downscaled, Gaussian-blurred and drawn
//! under the window's tint. A move never re-photographs the screen; it only
//! changes the image offset. A blur change re-blurs the stored shot, and a
//! display change asks for one fresh capture.
//!
//! The actual Win32/Gaussian code lives in [`crate::glass_capture`]; this module
//! owns the timing policy and the Slint image placement.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::fn_params_excessive_bools,
    reason = "the glass policy mirrors the asset-library boolean rule; coordinates are physical-to-logical conversions"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::app;
use crate::glass_capture;
use crate::AppWindow;

/// The window must have been still this long before the backdrop re-captures,
/// and it re-captures at most this often while it stays still. This is the
/// asset-library rule: dragging never touches the screen, so it cannot lag.
pub(crate) const GLASS_IDLE_REFRESH: Duration = Duration::from_millis(500);

/// What the frosted backdrop has to do this tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GlassWork {
    /// Nothing: the image is current.
    None,
    /// Photograph the desktop once more (`glass_capture::capture`).
    Capture,
    /// Re-run downscale + Gaussian + the image upload from the stored shot.
    /// No screen access at all.
    Reblur,
}

/// Backdrop policy, in one pure place so the tests can pin it down:
/// * never touch the screen while the window is moving;
/// * once it has been still for [`GLASS_IDLE_REFRESH`], capture again so the
///   frost follows what is behind the window;
/// * a blur change re-blurs the stored shot — no screen access;
/// * a display change or an explicit resample captures even when the window
///   has not been idle long enough yet.
pub(crate) fn glass_work(
    has_shot: bool,
    moving: bool,
    refresh_due: bool,
    blur_changed: bool,
    recapture: bool,
) -> GlassWork {
    if moving {
        return GlassWork::None;
    }
    if recapture || !has_shot {
        return GlassWork::Capture;
    }
    if blur_changed {
        return GlassWork::Reblur;
    }
    if refresh_due {
        return GlassWork::Capture;
    }
    GlassWork::None
}

/// One iteration of the capture loop (called from the 125 Hz frame tick).
///
/// The tick itself is cheap when nothing changed: it reads the window origin,
/// keeps the whole-desktop image registered with the desktop, and decides
/// whether a capture or a re-blur is due. Capture + Gaussian run on a worker
/// thread; a drag only changes the image offset.
#[cfg(windows)]
pub(crate) fn step(ui: &AppWindow, app: &Rc<RefCell<app::App>>) {
    let blur = ui.global::<crate::Theme>().get_blur();
    if blur <= 0.5 {
        // Blur off: there is no blurred layer. The tint alone sits over the
        // transparent window; the next non-zero value takes a fresh capture
        // rather than reusing a shot that may be minutes old.
        app::with_app(app, |a| {
            if ui.get_glass_backdrop_valid() {
                ui.set_glass_backdrop(slint::Image::default());
                ui.set_glass_backdrop_valid(false);
            }
            a.glass_shot = None;
            a.glass_blur_at_render = None;
            a.glass_placed = None;
            a.glass_last_origin = None;
        });
        return;
    }
    let Some(hwnd) = crate::hwnd_of(ui.window()) else {
        return;
    };
    glass_capture::install_display_watch(hwnd);
    if glass_capture::take_display_change() {
        app::with_app(app, |a| a.glass_recapture = true);
    }
    let Some(origin) = glass_capture::window_origin(hwnd) else {
        return;
    };
    let now = Instant::now();
    app::with_app(app, |a| {
        let moving = a.glass_last_origin != Some(origin);
        if moving {
            a.glass_still_since = now;
        }
        a.glass_last_origin = Some(origin);
        place(ui, a);
        // Never spend a desktop BitBlt + Gaussian (or a large image upload)
        // while the user is interacting with the canvas. The captured backdrop
        // is independent of the viewport, so a zoom/pan/node drag only needs
        // the existing image; the idle refresh resumes once the gesture
        // settles.
        let interacting = a.pending.has_input()
            || a.pending.cold_due.is_some()
            || a.doc.drag().is_some()
            || a.window_drag.is_some();
        if a.glass_pending || interacting {
            return;
        }
        let blur = blur.round().clamp(0.0, 100.0) as u8;
        let refresh_due = now.saturating_duration_since(a.glass_still_since) >= GLASS_IDLE_REFRESH
            && now.saturating_duration_since(a.glass_last_grab) >= GLASS_IDLE_REFRESH;
        let work = glass_work(
            a.glass_shot.is_some(),
            moving,
            refresh_due,
            a.glass_blur_at_render != Some(blur),
            a.glass_recapture,
        );
        if !matches!(work, GlassWork::Capture | GlassWork::Reblur) {
            return;
        }
        let capture = work == GlassWork::Capture;
        if capture {
            a.glass_recapture = false;
            a.glass_last_grab = now;
        }
        a.glass_pending = true;
        let previous = a.glass_shot.take();
        spawn_grab(ui.as_weak(), hwnd, blur, capture, previous);
    });
}

/// Capture (and/or blur) off the UI thread. A whole-desktop `BitBlt` plus the
/// Gaussian costs tens of milliseconds; doing that on the UI thread twice a
/// second would hitch, and doing it while the window moves is what made the
/// old backdrop lag. One job at a time (`App::glass_pending`).
#[cfg(windows)]
fn spawn_grab(
    ui: slint::Weak<AppWindow>,
    hwnd: isize,
    blur: u8,
    capture: bool,
    previous: Option<glass_capture::Shot>,
) {
    std::thread::spawn(move || {
        let shot = if capture {
            if let Some((shot, hide)) = glass_capture::capture(hwnd) {
                if std::env::var_os("OC_GLASS_TRACE").is_some() {
                    eprintln!("glass: capture ok (hide={})", hide.id());
                }
                Some(shot)
            } else {
                if std::env::var_os("OC_GLASS_TRACE").is_some() {
                    eprintln!("glass: capture failed, keeping the previous shot");
                }
                previous
            }
        } else {
            previous
        };
        let Some(shot) = shot else {
            let posted = slint::invoke_from_event_loop(move || {
                crate::with_shared_app(|a| a.glass_pending = false);
            });
            if let Err(err) = posted {
                eprintln!("glass: could not clear pending state: {err}");
            }
            return;
        };
        let pixels = glass_capture::render_pixels(&shot, blur);
        let posted = slint::invoke_from_event_loop(move || {
            let Some(ui) = ui.upgrade() else {
                return;
            };
            ui.set_glass_backdrop(slint::Image::from_rgba8(pixels));
            ui.set_glass_backdrop_valid(true);
            crate::with_shared_app(|a| {
                a.glass_pending = false;
                a.glass_shot = Some(shot);
                a.glass_blur_at_render = Some(blur);
                place(&ui, a);
            });
        });
        if let Err(err) = posted {
            eprintln!("glass: could not post the blurred backdrop: {err}");
        }
    });
}

/// Placement of the whole-desktop capture inside the window, in logical
/// pixels: the capture origin relative to the window origin, then the capture
/// size. Keeping the image at the capture origin minus the current window
/// origin is what pins it to the desktop; a move only changes this offset.
pub(crate) fn backdrop_placement(
    shot_origin: (i32, i32),
    window_origin: (i32, i32),
    shot: (usize, usize),
    scale: f32,
) -> (f32, f32, f32, f32) {
    let scale = if scale <= 0.0 { 1.0 } else { scale };
    (
        (shot_origin.0 - window_origin.0) as f32 / scale,
        (shot_origin.1 - window_origin.1) as f32 / scale,
        shot.0 as f32 / scale,
        shot.1 as f32 / scale,
    )
}

/// Keep the backdrop registered with the desktop: place the whole-desktop
/// image at the capture origin minus the current window origin. Does nothing
/// until a shot exists or when the window has not moved.
#[cfg(windows)]
fn place(ui: &AppWindow, a: &mut app::App) {
    let Some((shot_origin, sw, sh)) = a.glass_shot.as_ref().map(|s| (s.origin, s.width, s.height))
    else {
        return;
    };
    let Some(hwnd) = crate::hwnd_of(ui.window()) else {
        return;
    };
    let Some(window_origin) = glass_capture::window_origin(hwnd) else {
        return;
    };
    let placed = backdrop_placement(
        shot_origin,
        window_origin,
        (sw, sh),
        ui.window().scale_factor(),
    );
    if a.glass_placed == Some(placed) {
        return;
    }
    a.glass_placed = Some(placed);
    ui.set_glass_x(placed.0);
    ui.set_glass_y(placed.1);
    ui.set_glass_w(placed.2);
    ui.set_glass_h(placed.3);
}

#[cfg(not(windows))]
pub(crate) fn step(_ui: &AppWindow, _app: &Rc<RefCell<app::App>>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_work_policy_never_touches_the_screen_while_moving() {
        assert_eq!(
            glass_work(true, true, true, true, true),
            GlassWork::None,
            "a moving window only re-registers the existing image"
        );
    }

    #[test]
    fn the_work_policy_captures_before_blurring_and_respects_resample() {
        assert_eq!(
            glass_work(false, false, false, false, false),
            GlassWork::Capture
        );
        assert_eq!(
            glass_work(true, false, false, true, false),
            GlassWork::Reblur
        );
        assert_eq!(
            glass_work(true, false, true, false, false),
            GlassWork::Capture
        );
        assert_eq!(
            glass_work(true, false, false, false, true),
            GlassWork::Capture
        );
        assert_eq!(
            glass_work(true, false, false, false, false),
            GlassWork::None
        );
    }

    #[test]
    fn placement_pins_the_capture_to_the_desktop() {
        // A capture starting at a negative virtual-screen origin, shown in a
        // window at (200, 100), at 100% scale.
        assert_eq!(
            backdrop_placement((-1920, -71), (200, 100), (3840, 1151), 1.0),
            (-2120.0, -171.0, 3840.0, 1151.0)
        );
        // At 125% the physical coordinates become logical ones.
        let (x, y, w, h) = backdrop_placement((0, 0), (200, 100), (1920, 1080), 1.25);
        assert!((x + 160.0).abs() < f32::EPSILON);
        assert!((y + 80.0).abs() < f32::EPSILON);
        assert!((w - 1536.0).abs() < f32::EPSILON);
        assert!((h - 864.0).abs() < f32::EPSILON);
    }
}
