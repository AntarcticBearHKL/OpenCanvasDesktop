//! Headless tests for the custom window chrome, the frosted window plate and
//! the canvas grid inks.
//! Project rule: code-only verification — no GUI, no screenshots; the render
//! assertions read the real pixel buffer.
use super::*;

use crate::test_support::differing_pixels;

/// Pixels the shell painted at all (any alpha), for the "still paints"
/// assertion.
fn painted(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> usize {
    buffer.as_slice().iter().filter(|p| p.a > 0).count()
}

/// The title-bar maximise toggle flips the tracked state and mirrors it to
/// the window property the chrome reads for its restore glyph.
#[test]
fn chrome_maximise_toggle_is_reflected() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    assert!(!ui.get_window_maximized(), "windows start restored");
    with_app(&app, |app| app.toggle_maximize(&ui));
    assert!(ui.get_window_maximized(), "the first toggle maximises");
    assert!(app.borrow().window_maximized);
    with_app(&app, |app| app.toggle_maximize(&ui));
    assert!(!ui.get_window_maximized(), "the second toggle restores");
    assert!(!app.borrow().window_maximized);
    Ok(())
}

// -----------------------------------------------------------------------
// Canvas chrome: no bar, one floating pill, a drag strip
// -----------------------------------------------------------------------

/// No view has an app title bar: the canvas is edge-to-edge (moved by a
/// left-drag), the home screen owns the window controls in its header and the
/// studio draws its own menu row. The old `TopBar`/`CanvasChrome` are gone.
#[test]
fn the_canvas_view_has_no_title_bar_and_no_chrome() {
    let app = include_str!("../../ui/app.slint");
    assert!(!app.contains("TopBar"), "no view mounts the old title bar");
    assert!(
        !app.contains("CanvasChrome"),
        "the canvas mounts no floating window chrome"
    );
    assert!(
        !app.contains("height: Theme.titlebar-h"),
        "no layout row reserves a title-bar strip"
    );
    assert!(
        app.contains("canvas-area.window-dragging = true;")
            && app.contains("root.drag-begin();")
            && app.contains("root.drag-tick();")
            && app.contains("root.drag-end();"),
        "a left-drag on the canvas background moves the window"
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("ui/components/top-bar.slint")
            .exists(),
        "the removed title bar leaves no source file behind"
    );
    let home = include_str!("../../ui/components/library.slint");
    assert!(
        home.contains("WindowButtons {") && home.contains("callback minimize();"),
        "the home header carries the window controls"
    );
    assert!(
        !home.contains("CanvasChrome") && !home.contains("pill-inset"),
        "the removed canvas chrome leaves no symbol behind"
    );
}

// -----------------------------------------------------------------------
// Frosted window plate (the OS backdrop, never a capture)
// -----------------------------------------------------------------------

/// The `(r, g, b)` of one pixel, or `None` when out of bounds.
fn pixel_at(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    x: u32,
    y: u32,
) -> Option<(u8, u8, u8)> {
    rgba_at(buffer, x, y).map(|(r, g, b, _)| (r, g, b))
}

/// The `(r, g, b, a)` of one pixel, or `None` when out of bounds.
fn rgba_at(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    x: u32,
    y: u32,
) -> Option<(u8, u8, u8, u8)> {
    let width = usize::try_from(buffer.width()).ok()?;
    let index = usize::try_from(y).ok()? * width + usize::try_from(x).ok()?;
    buffer.as_slice().get(index).map(|p| (p.r, p.g, p.b, p.a))
}

/// A probe clearly inside the bare canvas: right of the 280px side panel and
/// the ruler, left of the right dock, below the ruler and above the bottom
/// dock. The canvas starts at the window's top edge now, so this only has to
/// clear the floating chrome (which now reaches 62px: ruler 20 + gap 6 + pill
/// 36) and the ruler.
fn canvas_probe(width: u32, height: u32) -> (u32, u32) {
    (width / 2, height / 3)
}

/// The canvas field region: everything right of the side panel + ruler and
/// clear of the top ruler, the floating chrome and the dock.
fn canvas_region(width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x0 = 300u32.min(width.saturating_sub(20));
    let y0 = 72u32.min(height / 4);
    (x0, y0, width.saturating_sub(20), height.saturating_sub(120))
}

/// WCAG relative luminance of an sRGB triple.
fn luminance(rgb: (u8, u8, u8)) -> f64 {
    let channel = |value: u8| {
        let s = f64::from(value) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.0) + 0.7152 * channel(rgb.1) + 0.0722 * channel(rgb.2)
}

/// WCAG contrast ratio between two sRGB triples.
fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Renders the editor (the canvas always paints the dot grid now).
fn render_canvas(
    ui: &AppWindow,
    app: &Rc<RefCell<App>>,
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    with_app(app, |app| {
        app.sync_all(ui);
    });
    ui.set_view("editor".into());
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// With no backdrop the root plate is the opaque `Theme.background` and the
/// translucent content tier above it (the same family of base tones) composites
/// back to within a couple of points of that colour: the fallback is the old
/// window, never a black or washed surface.
#[test]
fn the_window_falls_back_to_an_opaque_plate() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    assert!(
        !ui.get_backdrop(),
        "the backdrop is off until Rust enables it"
    );
    for dark in [false, true] {
        theme.set_dark(dark);
        let surface = render_canvas(&ui, &app)?;
        assert!(
            painted(&surface) > 100_000,
            "the fallback shell still paints ({} pixels)",
            painted(&surface)
        );
        let (probe_x, probe_y) = canvas_probe(surface.width(), surface.height());
        let (red, green, blue, alpha) =
            rgba_at(&surface, probe_x, probe_y).ok_or("probe inside the surface")?;
        let background = theme.get_background();
        let want = (background.red(), background.green(), background.blue());
        eprintln!(
            "opaque fallback (dark={dark}): probe {:?}, Theme.background {want:?}",
            (red, green, blue, alpha)
        );
        assert_eq!(
            alpha, 255,
            "the fallback plate is fully opaque (dark={dark})"
        );
        // The canvas field is the translucent `glass-content` tier (0.30) over
        // the opaque fallback plate; both share the palette's base tone family,
        // so the composite lands within 3 points of `Theme.background`.
        for (got, want) in [(red, want.0), (green, want.1), (blue, want.2)] {
            assert!(
                got.abs_diff(want) <= 5,
                "the canvas composites back to Theme.background (dark={dark}): {:?} vs {want:?}",
                (red, green, blue)
            );
        }
        assert_eq!(
            background.alpha(),
            255,
            "Theme.background stays the opaque base tone"
        );
    }
    Ok(())
}

/// When the OS accepted the backdrop the root plate becomes the translucent
/// tint and the canvas tier stays genuinely see-through over it: the desktop
/// (composited by DWM behind the window) shows through every surface.
#[test]
fn the_window_plate_is_translucent_with_a_backdrop() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    for dark in [false, true] {
        theme.set_dark(dark);
        ui.set_backdrop(true);
        let surface = render_canvas(&ui, &app)?;
        let (x, y) = canvas_probe(surface.width(), surface.height());
        let (_, _, _, alpha) = rgba_at(&surface, x, y).ok_or("probe inside the surface")?;
        eprintln!("frosted plate (dark={dark}): canvas alpha {alpha}");
        assert!(
            alpha < 255,
            "the canvas field lets the blurred desktop through (dark={dark}, alpha {alpha})"
        );
        // The shipped 80% white frost + the 0.30 content tier composites to
        // ~0.866 in both themes: a bright frosted field, never an opaque one.
        let (low, high) = (210, 230);
        assert!(
            (low..=high).contains(&alpha),
            "the field keeps its frosted weight (dark={dark}, alpha {alpha}, want {low}..={high})"
        );
    }
    ui.set_backdrop(false);
    theme.set_dark(false);
    Ok(())
}

/// The source invariant: the root is transparent for the first frames, and the
/// captured, Gaussian-blurred desktop is the layer under the tint.
#[test]
fn the_root_draws_the_captured_blurred_desktop() {
    let src = include_str!("../../ui/app.slint");
    assert!(
        src.contains("background: transparent;"),
        "the root window is a transparent surface"
    );
    for needle in [
        "if root.glass-backdrop-valid : Image",
        "source: root.glass-backdrop;",
        "glass-x",
        "glass-y",
        "glass-w",
        "glass-h",
        "root.glass-backdrop-valid || root.backdrop",
    ] {
        assert!(
            src.contains(needle),
            "app.slint wires the captured backdrop ({needle})"
        );
    }

    let capture = include_str!("../glass_capture.rs").replace("\r\n", "\n");
    for needle in [
        "BitBlt",
        "CreateDIBSection",
        "SharedPixelBuffer",
        "WDA_EXCLUDEFROMCAPTURE",
        "pub fn gaussian",
    ] {
        assert!(
            capture.contains(needle),
            "glass_capture.rs implements the asset-library pipeline ({needle})"
        );
    }

    let runtime = include_str!("../glass_runtime.rs").replace("\r\n", "\n");
    assert!(
        runtime.contains("std::thread::spawn") && runtime.contains("fn glass_work"),
        "the capture + blur run off the UI thread under one work policy"
    );
}

/// The capture loop never photographs a moving window, and it is driven from
/// the frame tick rather than a UI-thread `BitBlt`.
#[test]
fn the_capture_loop_is_off_thread_and_never_photographs_a_moving_window() {
    let main = include_str!("../main.rs").replace("\r\n", "\n");
    let tick = main
        .split("fn frame_tick(")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .expect("frame_tick exists");
    assert!(
        tick.contains("apply_window_chrome(ui)") && tick.contains("glass_runtime::step(ui, app)"),
        "the tick dresses the native window and advances the capture loop"
    );
    for forbidden in ["BitBlt", "StretchBlt", "Thread::spawn", "take_snapshot"] {
        assert!(
            !tick.contains(forbidden),
            "the frame tick performs no screen work ({forbidden})"
        );
    }

    let runtime = include_str!("../glass_runtime.rs").replace("\r\n", "\n");
    assert!(
        runtime.contains("if moving {") && runtime.contains("return GlassWork::None;"),
        "a moving window only re-registers the existing image"
    );
    assert!(
        runtime.contains("std::thread::spawn"),
        "capture + Gaussian run on a worker thread"
    );
    assert!(
        runtime.contains("GLASS_IDLE_REFRESH"),
        "the loop waits for the window to settle before capturing"
    );

    let capture = include_str!("../glass_capture.rs").replace("\r\n", "\n");
    assert!(
        capture.contains("SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)"),
        "the app window is excluded from its own screenshot"
    );
    assert!(
        capture.contains("fn BitBlt") && capture.contains("pub fn capture(hwnd: isize)"),
        "the whole-desktop blit is the capture primitive"
    );

    let glass = include_str!("../../../app-core/src/glass.rs").replace("\r\n", "\n");
    assert!(
        glass.contains("ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;")
            && glass.contains("ACCENT_FLAG_DRAW_ALL_BORDERS"),
        "the transparent-window accent remains the first-frame fallback"
    );
    assert!(
        !glass.contains("fn DwmExtendFrameIntoClientArea")
            && !glass.contains("win::extend_frame_into_client_area"),
        "the default path never extends the DWM frame (it paints the native caption)"
    );
}

/// The style guard removes `WS_CAPTION`, but Windows can still paint the
/// caption for one frame on creation or a transition. `DWMNCRP_DISABLED` is the
/// compositor-level guard: DWM must never render the non-client area, even if
/// the style drifts back. Both the initial dress and the pre-show pass must
/// call it.
#[test]
fn the_default_path_disables_dwm_non_client_rendering() {
    let glass = include_str!("../../../app-core/src/glass.rs").replace("\r\n", "\n");
    assert!(
        glass.contains("DWMWA_NCRENDERING_POLICY: u32 = 2;"),
        "the non-client rendering policy attribute is wired"
    );
    assert!(
        glass.contains("DWMNCRP_DISABLED: i32 = 1;"),
        "the policy value is DWMNCRP_DISABLED"
    );
    assert!(
        glass.contains("pub fn disable_nc_rendering(hwnd: isize)"),
        "the helper exists"
    );
    let main = include_str!("../main.rs").replace("\r\n", "\n");
    assert!(
        main.contains("glass::disable_nc_rendering(hwnd);"),
        "the window dress applies the policy"
    );
    assert!(
        main.matches("glass::disable_nc_rendering(hwnd);").count() >= 2,
        "the pre-show pass and the post-show dress both apply it"
    );
}

/// The frosted plate is only visible if nothing opaque sits on top of it: the
/// home and studio roots add no plate of their own, and the canvas field is the
/// translucent content tier. (The plate itself is `Theme.window-tint`, whose
/// alpha is the Appearance tab's opacity.)
#[test]
fn the_window_roots_do_not_paint_over_the_tint() {
    let app = include_str!("../../ui/app.slint");
    assert!(
        app.contains("root.glass-backdrop-valid || root.backdrop")
            && app.contains("Theme.window-tint")
            && app.contains("Theme.background"),
        "the only window plate is the tint/opaque pair"
    );
    assert!(
        app.contains("background: Theme.glass-content;"),
        "the canvas field is the translucent content tier"
    );
    for (name, src) in [
        ("home", include_str!("../../ui/components/library.slint")),
        ("studio", include_str!("../../ui/components/image.slint")),
    ] {
        assert!(
            src.contains("background: transparent;"),
            "the {name} root adds no plate of its own"
        );
        assert!(
            !src.contains("background: Theme.background;"),
            "the {name} root never paints an opaque Theme.background over the tint"
        );
    }
}

/// The strip this whole file guards against came from DWM having an extended
/// frame to paint: `DwmExtendFrameIntoClientArea` made DWM draw the native
/// caption band (the window title + system buttons) above the app content even
/// when `GWL_STYLE` had no caption bits. The default path must not reference
/// the API at all, and the acrylic accent covers the client area by itself.
/// The DWM system material (mica) is gone too, because it only paints in that
/// frame.
#[test]
fn the_default_path_never_extends_the_dwm_frame() {
    let glass = include_str!("../../../app-core/src/glass.rs").replace("\r\n", "\n");
    for forbidden in [
        "fn DwmExtendFrameIntoClientArea",
        "extend_frame_into_client_area",
        "reset_frame",
        "DWMWA_SYSTEMBACKDROP_TYPE",
        "system_backdrop_allowed",
        "mica_opt_in",
    ] {
        assert!(
            !glass.contains(forbidden),
            "the default path never extends the frame or asks for a system material ({forbidden})"
        );
    }
    let block = glass
        .split("pub fn enable_backdrop")
        .nth(1)
        .expect("the enable path exists");
    let accent = block
        .find("win::accent_backdrop(hwnd, dark)")
        .expect("the acrylic accent is the backdrop");
    assert!(
        !block[..accent].contains("Dwm"),
        "nothing DWM-related runs before the accent: no frame, no system material"
    );
    assert!(
        block.contains("win::accent_disabled(hwnd)"),
        "blur 0 disables the accent explicitly (opaque fallback)"
    );

    // The diagnostic: one env-gated line per attempt, and the blur level is part
    // of the native pass's key so a slider change re-applies immediately.
    let main = include_str!("../main.rs").replace("\r\n", "\n");
    assert!(
        main.contains("OC_BACKDROP_TRACE"),
        "the backdrop trace is env-gated"
    );
    assert!(
        glass.contains("OC_BACKDROP_TRACE") && glass.contains("log_backdrop_trace"),
        "enable_backdrop prints the material/result line"
    );
    assert!(
        main.contains("NATIVE_BLUR.load") && main.contains("NATIVE_BLUR.store"),
        "a blur change is part of the re-dress key"
    );
    let line = crate::glass::backdrop_trace(crate::glass::BackdropMaterial::Acrylic, true);
    assert!(line.contains("frame-extended no"), "{line}");
    assert!(line.contains("strategy accent"), "{line}");
}

/// The regression this guard exists for: winit keeps `WS_CAPTION` in the style
/// of a `no-frame` window, and the OS can rewrite the style on a DPI, size or
/// maximise transition. The style must be rewritten to a borderless popup
/// before the backdrop is touched, again on every checkpoint, and once per
/// frame tick when the bits drifted. `OC_WINDOW_TRACE` prints the before/after
/// bits for every re-apply.
#[test]
fn the_native_caption_is_stripped_before_the_backdrop_and_guarded_every_tick() {
    let glass = include_str!("../../../app-core/src/glass.rs").replace("\r\n", "\n");
    for needle in [
        "pub const WS_CAPTION: u32 = 0x00C0_0000;",
        "pub const WS_SYSMENU: u32 = 0x0008_0000;",
        "pub const WS_THICKFRAME: u32 = 0x0004_0000;",
        "pub const WS_BORDER: u32 = 0x0080_0000;",
        "pub const WS_DLGFRAME: u32 = 0x0040_0000;",
        "pub const WS_POPUP: u32 = 0x8000_0000;",
        "pub const NATIVE_CHROME_BITS: u32 = WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME;",
        "pub const fn frameless_window_style(style: u32) -> u32",
        "(style & !NATIVE_CHROME_BITS) | WS_POPUP",
        "pub const fn has_native_chrome(style: u32) -> bool",
        "pub const fn needs_frameless_repair(style: u32) -> bool",
        "pub fn strip_native_frame(hwnd: isize) -> bool",
        "pub fn guard_native_frame(hwnd: isize) -> bool",
        "fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;",
        "fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;",
        "SWP_FRAMECHANGED",
        "OC_WINDOW_TRACE",
    ] {
        assert!(
            glass.contains(needle),
            "glass.rs rewrites GWL_STYLE to a frameless popup ({needle})"
        );
    }

    // The per-tick guard is a read + compare on the steady path; the write and
    // the frame recalculation only run inside the drift branch.
    assert!(
        glass.contains("if !needs_frameless_repair(before) {\n            return true;"),
        "the tick returns after the compare when the style already matches"
    );
    assert!(
        glass.contains("let before = window_style(hwnd);\n        let after = frameless_window_style(before);\n        let applied = after != before;\n        if applied {\n            set_window_style(hwnd, after);\n        }"),
        "the checkpoint only writes SetWindowLongPtr when the bits changed"
    );
    assert!(
        glass.contains("SetWindowLongPtrW(hwnd, GWL_STYLE, value);")
            && glass.contains(
                "SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE"
            ),
        "a repair writes the style and forces one frame recalculation"
    );

    let main = include_str!("../main.rs").replace("\r\n", "\n");
    let guard_call = main
        .find("glass::guard_native_frame(hwnd);")
        .expect("the chrome pass guards the style every tick");
    let key_check = main
        .find("if !geometry_changed && key_matches && !retrying {")
        .expect("the memoised key short-circuit");
    assert!(
        guard_call < key_check,
        "the guard runs before the key short-circuit, so drift is repaired on any tick"
    );
    let style = main
        .find("glass::strip_native_frame(hwnd);")
        .expect("the chrome pass checkpoints the style");
    let backdrop = main
        .find("glass::enable_backdrop(hwnd, blur, dark);")
        .expect("the chrome pass enables the backdrop");
    assert!(
        style < backdrop,
        "the caption is stripped before the backdrop is requested"
    );
    assert!(
        main.contains("ui.show()?;\n    apply_window_chrome(&ui);"),
        "the style is applied after the HWND exists (the first show)"
    );
    assert!(
        main.contains("&& NATIVE_MAXIMIZED.load(Ordering::Relaxed) == maximized"),
        "a maximise/restore re-runs the style checkpoint"
    );
    assert!(
        main.contains("NATIVE_SIZE.load(Ordering::Relaxed) != size_key")
            && main.contains("NATIVE_SCALE.load(Ordering::Relaxed) != scale"),
        "a DPI/size change re-runs the style checkpoint"
    );
    assert!(
        main.contains("NATIVE_SIZE.store(size_key, Ordering::Relaxed);")
            && main.contains("NATIVE_SCALE.store(scale, Ordering::Relaxed);"),
        "the geometry is recorded so the next steady tick is cheap"
    );

    // No other native frame is expected: the window is `no-frame` and the home
    // header draws its own window buttons.
    let app = include_str!("../../ui/app.slint");
    assert!(
        app.contains("no-frame: true;"),
        "the Slint window is frameless"
    );
    let home = include_str!("../../ui/components/library.slint");
    assert!(
        home.contains("WindowButtons {") && !home.contains("native-title-bar"),
        "the app draws the window controls; there is no native frame to expect"
    );
    assert!(
        home.find("icon: Icons.home;")
            .zip(home.find("tip: I18n.library-new-project;"))
            .zip(home.find("WindowButtons {"))
            .is_some_and(|((home, plus), controls)| home < plus && plus < controls),
        "the home screen draws Home, New Project and its own window controls"
    );

    // The trace names the bits before and after the re-apply.
    let line = crate::glass::window_style_trace(0x06CF_0000, 0x8603_0000, true);
    assert!(
        line.contains("0x06CF0000") && line.contains("0x86030000"),
        "{line}"
    );
    assert!(line.contains("native chrome no"), "{line}");
    assert!(line.contains("popup yes"), "{line}");
}

/// Windows disables the acrylic material for inactive windows, so the native
/// pass polls the foreground window once per tick and re-applies the backdrop
/// when the activation state flips. The re-apply repairs the accent fallback
/// path too, and the plate still falls back to opaque when nothing is accepted.
#[test]
fn the_backdrop_is_reapplied_when_the_window_activation_flips() {
    let main = include_str!("../main.rs").replace("\r\n", "\n");
    assert!(
        main.contains("let active = glass::window_is_active(hwnd);"),
        "the chrome pass reads the window's activation state"
    );
    assert!(
        main.contains("&& NATIVE_ACTIVE.load(Ordering::Relaxed) == active"),
        "the activation state is part of the re-dress key"
    );
    assert!(
        main.contains("NATIVE_ACTIVE.store(active, Ordering::Relaxed);"),
        "the flip is recorded so the next tick is cheap again"
    );
    // The re-apply is the same enable_backdrop call the startup pass uses, and
    // the plate stays opaque when it returns false.
    assert!(
        main.contains("let backdrop = glass::enable_backdrop(hwnd, blur, dark);")
            && main.contains("ui.set_backdrop(backdrop);"),
        "the activation flip re-applies the backdrop and keeps the opaque fallback"
    );

    let glass = include_str!("../../../app-core/src/glass.rs").replace("\r\n", "\n");
    assert!(
        glass.contains("fn GetForegroundWindow() -> isize;"),
        "the poll is one GetForegroundWindow call"
    );
    assert!(
        glass.contains("pub fn window_is_active(hwnd: isize) -> bool"),
        "the activation read is exposed as a helper"
    );
    assert!(
        glass.contains("win::foreground_hwnd() == Some(hwnd)"),
        "the helper compares the foreground window with the app's HWND"
    );
}

/// The grid must read as a quiet gray texture: visible when you look for it,
/// never a polka-dot pattern. Measured dots must stay in a 1.35–1.85:1 band
/// against the field in both themes (invisible below, loud above).
#[test]
fn canvas_grid_reads_against_the_opaque_field() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    for dark in [false, true] {
        theme.set_dark(dark);
        let dots = render_canvas(&ui, &app)?;

        let (x0, y0, x1, y1) = canvas_region(dots.width(), dots.height());
        let mut counts: std::collections::HashMap<(u8, u8, u8), usize> =
            std::collections::HashMap::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(rgb) = pixel_at(&dots, x, y) {
                    *counts.entry(rgb).or_insert(0) += 1;
                }
            }
        }
        // Dots are sparse 2.5px marks on a 48px lattice, so the modal colour of
        // the canvas region is the field the dots are drawn on.
        let field = counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(rgb, _)| *rgb)
            .ok_or("the canvas region is not empty")?;

        let mut best = field;
        let mut best_delta = 0i32;
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(rgb) = pixel_at(&dots, x, y) {
                    let delta = i32::from(rgb.0.abs_diff(field.0))
                        + i32::from(rgb.1.abs_diff(field.1))
                        + i32::from(rgb.2.abs_diff(field.2));
                    if delta > best_delta {
                        best_delta = delta;
                        best = rgb;
                    }
                }
            }
        }

        eprintln!(
            "dark={dark} field {field:?}; dot ink {best:?} delta {best_delta} contrast {:.3}",
            contrast_ratio(field, best),
        );
        assert!(
            best_delta >= 20,
            "the dots are distinguishable from the field, dark={dark} ({best_delta})"
        );
        let dot_contrast = contrast_ratio(field, best);
        assert!(
            (1.35..=1.85).contains(&dot_contrast),
            "the dot contrast stays in the quiet band 1.35..=1.85:1, dark={dark} ({dot_contrast:.3})"
        );
    }
    theme.set_dark(false);
    Ok(())
}

/// The grid layer's `opacity` in `app.slint`: the dot/line ink alpha is scaled
/// by this before it reaches the field.
const GRID_LAYER_OPACITY: f64 = 0.65;

/// The same band measured straight from the tokens that paint the dots: the dot
/// ink composited over `Theme.canvas-background` at the grid layer's opacity in
/// both themes. This is the design-token side of the quiet-gray contract.
#[test]
fn the_dot_token_clears_the_contrast_floor() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    // The ink alpha alone is not what reaches the screen: the grid layer draws
    // the path at 0.65 opacity (`app.slint`).
    let app = include_str!("../../ui/app.slint");
    assert!(
        app.contains("opacity: 0.65;"),
        "the grid layer opacity stays 0.65"
    );

    let over = |ink: slint::Color, field: slint::Color| {
        let a = f64::from(ink.alpha()) / 255.0 * GRID_LAYER_OPACITY;
        let mix = |src: u8, dst: u8| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a convex mix of two u8 channels stays in 0..=255"
            )]
            let mixed = (f64::from(src).mul_add(a, f64::from(dst) * (1.0 - a))).round() as u8;
            mixed
        };
        (
            mix(ink.red(), field.red()),
            mix(ink.green(), field.green()),
            mix(ink.blue(), field.blue()),
        )
    };

    for dark in [false, true] {
        theme.set_dark(dark);
        let field = theme.get_canvas_background();
        let rgb = (field.red(), field.green(), field.blue());
        let dot = over(theme.get_canvas_dot(), field);
        let ratio = contrast_ratio(rgb, dot);
        eprintln!("dark={dark} dot token {dot:?} on {rgb:?} = {ratio:.3}:1");
        assert!(
            (1.35..=1.85).contains(&ratio),
            "the dot token stays in the quiet band 1.35..=1.85:1 in both themes, dark={dark} ({ratio:.3})"
        );
    }
    theme.set_dark(false);
    Ok(())
}

/// The shared token values the design language is built on: 6/8px chrome
/// radii, the three see-through surface tiers (content 0.30 / panel 0.55 /
/// strong 0.80 light, 0.82 dark), the quiet gray grid inks, and the sibling's
/// 90/110/150/180ms motion steps.
#[expect(
    clippy::too_many_lines,
    reason = "one test pins the whole shared token contract in one place"
)]
#[test]
fn design_tokens_are_the_shared_values() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    assert!(
        (theme.get_radius_sm() - 6.0).abs() < f32::EPSILON,
        "controls are 6px"
    );
    assert!(
        (theme.get_radius_md() - 8.0).abs() < f32::EPSILON,
        "cards and panels are 8px"
    );

    let background = theme.get_background();
    assert_eq!(
        background.alpha(),
        255,
        "Theme.background is the opaque base tone (the fallback plate)"
    );
    let alpha = |color: slint::Color| f64::from(color.alpha()) / 255.0;
    let was_dark = theme.get_dark();
    for dark in [false, true] {
        theme.set_dark(dark);
        // The canvas field shares the window plate's paper tone; the chrome
        // tiers keep the opaque base tone, so the chrome still composites to
        // the fallback colour exactly when no frost shows.
        let plate = theme.get_window_tint();
        let chrome_base = theme.get_background();
        for (name, tier, want, base) in [
            ("content", theme.get_glass_content(), 0.30, plate),
            ("panel", theme.get_glass_panel(), 0.55, chrome_base),
            (
                "strong",
                theme.get_glass_panel_strong(),
                if dark { 0.82 } else { 0.80 },
                chrome_base,
            ),
        ] {
            assert!(
                (alpha(tier) - want).abs() < 0.015,
                "the {name} tier keeps its frost alpha {want} (dark={dark}, {})",
                alpha(tier)
            );
            assert_eq!(
                (tier.red(), tier.green(), tier.blue()),
                (base.red(), base.green(), base.blue()),
                "the {name} tier keeps its base tone (dark={dark})"
            );
            assert!(alpha(tier) < 1.0, "no surface tier is opaque (dark={dark})");
        }
        // The canvas field is the plate's exact tone at 30%.
        let content = theme.get_glass_content();
        assert_eq!(
            (content.red(), content.green(), content.blue()),
            (plate.red(), plate.green(), plate.blue()),
            "the canvas field is the window plate's paper tone (dark={dark})"
        );
    }
    theme.set_dark(was_dark);
    assert_eq!(
        theme.get_canvas_background(),
        background,
        "the canvas base stays the opaque reference tone"
    );
    // The dot ink is quiet neutral gray: 0.52 light / 0.30 dark, drawn at 0.65
    // layer opacity, and the dot diameter is the 2.5px stroke width in
    // `app.slint`.
    theme.set_dark(false);
    let light_dot = theme.get_canvas_dot();
    theme.set_dark(true);
    let dark_dot = theme.get_canvas_dot();
    theme.set_dark(was_dark);
    assert!(
        (alpha(light_dot) - 0.52).abs() < 0.02,
        "the light dot ink is 0.52 ({})",
        alpha(light_dot)
    );
    assert!(
        (alpha(dark_dot) - 0.30).abs() < 0.02,
        "the dark dot ink is 0.30 ({})",
        alpha(dark_dot)
    );
    // The old inks leaned warm (r > b by 18 and 7 points); the neutral gray
    // keeps the channels within a few points of each other in both themes.
    for (name, ink) in [("light", light_dot), ("dark", dark_dot)] {
        assert!(
            ink.red().abs_diff(ink.green()) <= 6 && ink.green().abs_diff(ink.blue()) <= 6,
            "the {name} dot ink is neutral gray, not warm ({:?})",
            (ink.red(), ink.green(), ink.blue())
        );
    }
    assert_ne!(light_dot, dark_dot, "light and dark dots stay distinct");
    let app = include_str!("../../ui/app.slint");
    assert!(
        app.contains("stroke-width: 2.5px;"),
        "the dot diameter is the 2.5px stroke width"
    );
    assert!(
        !app.contains("grid-mode"),
        "the canvas has no background mode any more"
    );
    assert!(
        app.contains("opacity: 0.65;"),
        "the grid layer draws the quiet inks at 0.65 opacity"
    );

    // The panel border/hairlines are the only translucent chrome lines; the
    // modal scrim and other overlays keep their alpha.
    assert!(
        (alpha(theme.get_glass_panel_border()) - 0.12).abs() < 0.01,
        "the panel border is foreground at 12%"
    );

    // The hover step of a control border exists and is distinct from the
    // resting border.
    assert_ne!(theme.get_border(), theme.get_border_strong());
    assert!(
        theme.get_overlay().alpha() < 255,
        "the modal scrim stays translucent"
    );

    assert_eq!(theme.get_motion_press(), 90, "press feedback is 90ms");
    assert_eq!(theme.get_motion_hover(), 110, "hover is 110ms");
    assert_eq!(theme.get_motion_select(), 150, "selection is 150ms");
    assert_eq!(theme.get_motion_lift(), 180, "lift is 180ms");
    Ok(())
}

/// The window plate is the asset-library 80% white frost in light and the same
/// strength dark wash. The plate is the only wash; the surface tiers above it
/// carry the rest of the frost weight.
#[test]
fn the_window_tint_is_near_white_in_light() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();
    let alpha = |color: slint::Color| f64::from(color.alpha()) / 255.0;

    let was_dark = theme.get_dark();
    theme.set_dark(false);
    let light_tint = theme.get_window_tint();
    theme.set_dark(true);
    let dark_tint = theme.get_window_tint();
    theme.set_dark(was_dark);

    assert!(
        (alpha(light_tint) - 0.808).abs() < 0.02,
        "the light plate is the 80% white frost ({})",
        alpha(light_tint)
    );
    assert!(
        (alpha(dark_tint) - 0.808).abs() < 0.02,
        "the dark plate uses the same 80% tint strength ({})",
        alpha(dark_tint)
    );
    // "White with only a hint of frost": the light plate is bright and its
    // channels sit within a few points of each other (no grey mud).
    assert!(
        light_tint.red() >= 240 && light_tint.green() >= 240 && light_tint.blue() >= 240,
        "the light plate is near-white ({:?})",
        (light_tint.red(), light_tint.green(), light_tint.blue())
    );
    assert!(
        light_tint.red().abs_diff(light_tint.blue()) <= 8,
        "the light plate keeps the warm neutral hue, not a grey cast ({:?})",
        (light_tint.red(), light_tint.green(), light_tint.blue())
    );
    assert_ne!(light_tint, dark_tint, "the two plates stay distinct");
    Ok(())
}

/// Two themes must still paint differently (the plate is not a constant
/// white): a light and a dark render differ across the whole window.
#[test]
fn the_theme_still_repaints_the_window() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    theme.set_dark(false);
    let light = render_canvas(&ui, &app)?;
    theme.set_dark(true);
    let dark = render_canvas(&ui, &app)?;
    let whole = differing_pixels(&light, &dark).map_err(ToString::to_string)?;
    eprintln!("theme repaint: {whole} pixels differ");
    assert!(
        whole > 100_000,
        "the dark plate repaints the window ({whole})"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// The one-tint / one-frost stack: composite + contrast measurements
// ---------------------------------------------------------------------------

/// White and black desktops behind the OS blur: the two extremes every frost
/// surface has to stay legible over.
const WHITE_DESKTOP: (f64, f64, f64) = (255.0, 255.0, 255.0);
const BLACK_DESKTOP: (f64, f64, f64) = (0.0, 0.0, 0.0);

/// Source-over of a translucent surface onto an opaque sRGB base, in f64.
fn over_frost(top: slint::Color, base: (f64, f64, f64)) -> (f64, f64, f64) {
    let a = f64::from(top.alpha()) / 255.0;
    let blend = |src: u8, dst: f64| f64::from(src).mul_add(a, dst * (1.0 - a));
    (
        blend(top.red(), base.0),
        blend(top.green(), base.1),
        blend(top.blue(), base.2),
    )
}

/// The frost over a desktop: the window tint first (the only wash), then the
/// surface tier.
fn frost(theme: &crate::Theme, tier: slint::Color, desktop: (f64, f64, f64)) -> (f64, f64, f64) {
    over_frost(tier, over_frost(theme.get_window_tint(), desktop))
}

/// An f64 sRGB triple rounded to the byte triple `contrast_ratio` reads.
fn rgb8(rgb: (f64, f64, f64)) -> (u8, u8, u8) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a value clamped to 0..=255 is a valid u8"
    )]
    fn one(value: f64) -> u8 {
        value.round().clamp(0.0, 255.0) as u8
    }
    (one(rgb.0), one(rgb.1), one(rgb.2))
}

/// The alpha a stack of translucent layers leaves for the desktop:
/// `1 - product(1 - alpha)` — source-over as the renderer composites it.
fn stack_alpha(layers: &[slint::Color]) -> f64 {
    1.0 - layers.iter().fold(1.0, |left, layer| {
        left * (1.0 - f64::from(layer.alpha()) / 255.0)
    })
}

/// The one-tint / one-frost stack composites into the bright asset-library
/// band: the canvas field (content) lands at ~0.87, the chrome (panel) at
/// ~0.91, and the menus (strong) at ~0.96. The desktop is still visible through
/// the tint, but the light theme stays white rather than turning into dark
/// glass.
#[test]
fn the_frost_stack_composites_to_the_frosted_band() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    for dark in [false, true] {
        theme.set_dark(dark);
        let tint = theme.get_window_tint();
        let content = stack_alpha(&[tint, theme.get_glass_content()]);
        let panel = stack_alpha(&[tint, theme.get_glass_panel()]);
        let strong = stack_alpha(&[tint, theme.get_glass_panel_strong()]);
        eprintln!(
            "composited frost (dark={dark}): content {content:.3}, panel {panel:.3}, strong {strong:.3}"
        );

        assert!(
            (0.84..=0.89).contains(&content),
            "the canvas field is the bright frost layer (dark={dark}, {content:.3})"
        );
        assert!(
            (0.89..=0.94).contains(&panel),
            "the chrome lands in the bright frosted band (dark={dark}, {panel:.3})"
        );
        assert!(
            (0.94..=0.98).contains(&strong),
            "the menus stay near-opaque but never seal the frost (dark={dark}, {strong:.3})"
        );
        assert!(
            strong < 0.99 && panel < 0.95 && content < 0.90,
            "no tier reintroduces an opaque plate (dark={dark})"
        );
        // The tier's own alpha is never the whole story: each one is genuinely
        // see-through on its own (menus included).
        for (name, layer) in [
            ("content", theme.get_glass_content()),
            ("panel", theme.get_glass_panel()),
            ("strong", theme.get_glass_panel_strong()),
        ] {
            let alpha = f64::from(layer.alpha()) / 255.0;
            assert!(
                alpha <= 0.85,
                "the {name} tier keeps its desktop visible (dark={dark}, {alpha:.3})"
            );
        }
    }
    theme.set_dark(false);
    Ok(())
}

/// Body text keeps WCAG AA (>= 4.5:1) on the composited chrome over both a
/// white and a black desktop, in both themes. The panel tier is the surface
/// that carries the rows, toolbar labels and dock text; the strong tier carries
/// menus and dialogs. (The canvas field itself carries no body text — nodes and
/// wire labels are opaque cards — so its floor is the large-text 4.0, asserted
/// separately below.)
#[test]
fn body_text_clears_the_contrast_floor_over_the_frost() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    for dark in [false, true] {
        theme.set_dark(dark);
        let ink = theme.get_foreground();
        let text = (ink.red(), ink.green(), ink.blue());
        for (desktop_name, desktop) in [("white", WHITE_DESKTOP), ("black", BLACK_DESKTOP)] {
            for (tier_name, tier) in [
                ("panel", theme.get_glass_panel()),
                ("strong", theme.get_glass_panel_strong()),
            ] {
                let field = rgb8(frost(&theme, tier, desktop));
                let ratio = contrast_ratio(field, text);
                eprintln!(
                    "body text on {tier_name} over {desktop_name} (dark={dark}): {field:?} vs {text:?} = {ratio:.2}:1"
                );
                assert!(
                    ratio >= 4.5,
                    "body text clears AA on the {tier_name} tier over a {desktop_name} desktop \
                     (dark={dark}, {ratio:.2}:1)"
                );
            }

            // The clear canvas field: worst case is the dark theme's light ink
            // over a bright desktop, where the 0.30 field keeps ~4.1:1 — above
            // the 3:1 large-text floor and the actual body text sits on the
            // opaque node cards (node-fill), never straight on the field.
            let field = rgb8(frost(&theme, theme.get_glass_content(), desktop));
            let ratio = contrast_ratio(field, text);
            eprintln!(
                "body ink on the canvas field over {desktop_name} (dark={dark}): {field:?} = {ratio:.2}:1"
            );
            assert!(
                ratio >= 4.0,
                "the canvas field stays legible for overlays over a {desktop_name} desktop \
                 (dark={dark}, {ratio:.2}:1)"
            );
        }
    }
    theme.set_dark(false);
    Ok(())
}

/// The dot ink keeps the quiet 1.35–1.85:1 band against the frosted field over
/// the app's own base tone (the steady state), and stays visible-but-never-loud
/// over the two desktop extremes. Dot ink is baked into the grid tile at the
/// 0.65 grid-layer opacity, so the band is a property of the ink/field pair.
#[test]
fn the_canvas_dots_keep_the_quiet_band_on_the_frost() -> Result<(), Box<dyn std::error::Error>> {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, _app) = crate::config_test_support::new_wired_app(store)?;
    let theme = ui.global::<crate::Theme>();

    for dark in [false, true] {
        theme.set_dark(dark);
        let ink = theme.get_canvas_dot();
        let alpha = f64::from(ink.alpha()) / 255.0 * GRID_LAYER_OPACITY;
        let dot_over = |field: (f64, f64, f64)| {
            let one = |src: u8, dst: f64| f64::from(src).mul_add(alpha, dst * (1.0 - alpha));
            (
                one(ink.red(), field.0),
                one(ink.green(), field.1),
                one(ink.blue(), field.2),
            )
        };

        // Steady state: the desktop is the app's own base tone.
        let base = {
            let background = theme.get_background();
            (
                f64::from(background.red()),
                f64::from(background.green()),
                f64::from(background.blue()),
            )
        };
        let field = frost(&theme, theme.get_glass_content(), base);
        let ratio = contrast_ratio(rgb8(field), rgb8(dot_over(field)));
        eprintln!("dot band (dark={dark}): {ratio:.3}:1");
        assert!(
            (1.35..=1.85).contains(&ratio),
            "the dots stay in the quiet band over the frosted field (dark={dark}, {ratio:.3}:1)"
        );

        // Extremes: the field itself moves with the desktop, so the band is
        // checked as visibility (>= 1.15) and "never a polka dot" (<= 1.85).
        for (desktop_name, desktop) in [("white", WHITE_DESKTOP), ("black", BLACK_DESKTOP)] {
            let field = frost(&theme, theme.get_glass_content(), desktop);
            let dot = dot_over(field);
            let field8 = rgb8(field);
            let dot8 = rgb8(dot);
            let ratio = contrast_ratio(field8, dot8);
            let delta = i32::from(field8.0.abs_diff(dot8.0))
                + i32::from(field8.1.abs_diff(dot8.1))
                + i32::from(field8.2.abs_diff(dot8.2));
            eprintln!(
                "dots over {desktop_name} (dark={dark}): {field8:?} -> {dot8:?} {ratio:.3}:1"
            );
            assert!(
                (1.15..=1.85).contains(&ratio) && delta >= 20,
                "the dots stay visible but quiet over a {desktop_name} desktop \
                 (dark={dark}, {ratio:.3}:1, delta {delta})"
            );
        }
    }
    theme.set_dark(false);
    Ok(())
}

/// Source guard: every chrome surface draws on a glass tier instead of an
/// opaque literal. The canvas field, the dock bar, the side panel, the home
/// screen, the studio root and every studio band are checked by name; a flat
/// `Theme.background`/`Theme.card`/`Theme.popover`/`Theme.toolbar-surface` plate
/// or a hard-coded `#rrggbb` fill in chrome is a failure.
#[test]
fn no_chrome_surface_falls_back_to_an_opaque_fill() {
    let app = include_str!("../../ui/app.slint");
    let toolbar = include_str!("../../ui/components/toolbar.slint");
    let side_panel = include_str!("../../ui/components/side-panel.slint");
    let library = include_str!("../../ui/components/library.slint");
    let image = include_str!("../../ui/components/image.slint");
    let studio_theme = include_str!("../../ui/studio-theme.slint");

    // The one plate: translucent over the OS blur, opaque only as the fallback.
    assert!(
        app.contains("root.glass-backdrop-valid || root.backdrop")
            && app.contains("Theme.window-tint")
            && app.contains("Theme.background"),
        "the window plate is the tint, with the opaque fallback when no capture has landed"
    );
    // The canvas field / dock / side panel / studio root / home screen.
    assert!(
        app.contains("background: Theme.glass-content;"),
        "the canvas field paints the content tier"
    );
    assert!(
        toolbar.contains("background: Theme.glass-panel;"),
        "the dock bar paints the panel tier"
    );
    assert!(
        side_panel.contains("background: Theme.glass-panel;"),
        "the side panel paints the panel tier"
    );
    assert!(
        image.contains("background: transparent;")
            && !image.contains("background: StudioTheme.background;")
            && !image.contains("background: Theme.background;"),
        "the studio root draws on the window plate, not on an opaque studio fill"
    );
    assert!(
        library.contains("background: transparent;")
            && !library.contains("background: Theme.background;"),
        "the home screen draws on the window plate, not on an opaque surface"
    );
    // Every studio band is one of the shared glass tiers.
    for (surface, tier) in [
        ("menu-bar", "Theme.glass-panel-strong"),
        ("rail", "Theme.glass-panel-strong"),
        ("panel-body", "Theme.glass-panel-strong"),
        ("panel-tab", "Theme.glass-panel-strong"),
        ("status-bar", "Theme.glass-panel-strong"),
        ("menu-surface", "Theme.glass-panel-strong"),
        ("panel-header", "Theme.glass-panel"),
    ] {
        assert!(
            studio_theme.contains(&format!("{surface}: {tier};")),
            "the studio's {surface} maps to {tier}"
        );
    }

    // No flat opaque plate and no hard-coded fill anywhere in the chrome
    // sources. Conditional state washes (hover/active/accent) are fills over a
    // glass tier, so only whole-value opaque assignments are rejected.
    for (name, src) in [
        ("app.slint", app),
        ("components/toolbar.slint", toolbar),
        ("components/side-panel.slint", side_panel),
        ("components/library.slint", library),
        ("components/image.slint", image),
    ] {
        for banned in [
            "background: Theme.card;",
            "background: Theme.popover;",
            "background: Theme.toolbar-surface;",
            "background: Theme.node-fill;",
            "background: Theme.node-panel;",
            "background: StudioTheme.background;",
            "background: rgba(",
            "background: #",
        ] {
            assert!(
                !src.contains(banned),
                "{name} paints a chrome surface with an opaque fill: {banned}"
            );
        }
        if name != "app.slint" {
            assert!(
                !src.contains("background: Theme.background;"),
                "{name} paints a chrome surface with the opaque base tone"
            );
        }
    }

    // The home rows and the side-panel rows use translucent washes, not the
    // opaque accent-soft plates.
    for (name, src) in [
        ("components/library.slint", library),
        ("components/side-panel.slint", side_panel),
    ] {
        for opaque_wash in [
            "background: Theme.node-accent-soft;",
            "background: Theme.node-danger-soft;",
            "background: Theme.brand-soft;",
        ] {
            assert!(
                !src.contains(opaque_wash),
                "{name} keeps its row wash translucent: {opaque_wash}"
            );
        }
    }
}
