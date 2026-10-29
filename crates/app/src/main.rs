//! `OpenCanvas` desktop app (P2: interactive canvas).
//!
//! State lives in Rust: `doc` owns the `open-canvas-core` document, `app` holds
//! the `App` state, `sync` pushes the Slint view model (one module per concern),
//! `events` handles pointers/keys and `vm` builds the layer path commands. The
//! Image Studio is the `image/` module tree (`image::mod` owns the studio
//! state). Fonts: Baloo 2 is embedded and registered by the `.slint` font
//! import in `ui/app.slint` (Slint 1.18 removed
//! `slint::register_font_from_path`; the markup import is the documented
//! registration API). The CJK tail of the web font stack is completed at
//! startup below.

// Release builds are GUI-only: without this, launching the exe from Explorer or
// the desktop shortcut opens a console window alongside the app. Debug builds
// keep the console so `println!` and backtraces stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

mod agent_bridge;
mod ai_service;
mod app;
mod appearance;
mod attempt_ui;
mod bar_events;
mod bars;
mod canvas_content;
mod config_tabs;
mod config_ui;
mod content_events;
mod edit_events;
mod events;
mod file_drop;
mod glass_capture;
mod glass_runtime;
mod grid_tile;
mod handlers;
mod i18n;
mod image;
mod library;
mod library_export;
mod library_ops;
mod library_wire;
mod media;
mod mention;
mod node_list;
mod node_ui;
mod plugin_actions;
mod plugin_content;
mod plugin_host;
mod plugin_manager;
mod plugin_manager_model;
mod plugin_views;
mod plugin_wire;
mod ruler_text;
mod save;
mod shortcuts;
mod store;
mod sync;
mod vm;

// UI-independent logic lives in `open-canvas-app-core`, a Slint-free crate that
// compiles in parallel with this codegen unit and is not rebuilt when only the
// `.slint` UI changes. Re-exporting its modules here keeps every existing
// `crate::doc::…` / `crate::coords::…` path in this crate unchanged.
pub use open_canvas_app_core::{
    asset_folder, clipboard, connect, coords, cost, doc, doc_edit, doc_project, drop_bindings,
    drop_files, gesture, glass, inline_edit, input, local_models, log_view, nav, overlay, panels,
    paths, storage_scan, system_clipboard, wires,
};

#[cfg(test)]
#[path = "config_test_support.rs"]
mod config_test_support;

// This test module stayed here: it asserts on the Slint row types that
// `config_tabs::cost_rows` produces, so it needs the UI crate.
#[cfg(test)]
#[path = "cost_source_tests.rs"]
mod cost_source_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod ui_shell_tests;

#[cfg(test)]
mod widget_tests;

#[cfg(test)]
mod ui_consistency_tests;

#[cfg(test)]
mod node_card_tests;

#[cfg(test)]
mod home_bar_tests;

#[cfg(test)]
mod studio_removal_tests;

#[cfg(test)]
#[path = "plugin_tests_support.rs"]
mod plugin_tests_support;

/// Timing guards for the canvas/studio hot paths (see the module header).
#[cfg(test)]
#[path = "perf_tests.rs"]
mod perf_tests;

#[expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::todo,
    clippy::let_underscore_must_use,
    reason = "Slint's generated module writes unwrap/panic on internally-proven invariants"
)]
mod generated {
    slint::include_modules!();
}
pub use generated::*;

fn main() -> Result<(), slint::PlatformError> {
    // Slint's lazy redraw path asks winit for the monitor refresh rate. On
    // Windows that query can return None (60 Hz fallback) or a stale rate while
    // the panel runs at 120 Hz, capping interactive canvas frames at ~40 fps.
    // Full-speed mode is Slint's own animation-driven path and sustains the
    // panel rate; the fast zoom preview keeps the per-frame work small. Set
    // OC_LAZY_RENDER=1 to keep the old power-saving lazy path.
    if std::env::var_os("OC_LAZY_RENDER").is_none() {
        std::env::set_var("SLINT_DEBUG_PERFORMANCE", "refresh_full_speed,console");
    }
    install_crash_log();
    // Windows clamps timers to 15.6 ms unless the process asks for a finer
    // resolution; without this the 8 ms interactive tick (and the window-drag
    // cursor sampling on it) really ran at ~64 Hz.
    glass::begin_high_resolution_timer();
    register_cjk_fallback();
    log_frame_budget();
    // Strip WS_CAPTION in the CBT creation hook, before winit ever calls
    // CreateWindowExW. The style guard and the non-client subclass are the
    // post-creation guards; this one prevents the first native title bar from
    // existing at all.
    let creation_hook = glass::install_creation_hook();
    let ui = AppWindow::new()?;
    // The .slint window no longer pins width/height (a constant binding froze
    // the surface size and broke OS resize); push the initial size here, before
    // the first show(). Logical units so a HiDPI display still gets a
    // 1280x800-DIP window.
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    let store = store::Store::resolve();
    let app = Rc::new(RefCell::new(app::App::new(store)));
    // Expose the shared app to worker-thread callbacks (a background decode
    // cannot move the `Rc`, which is not `Send`).
    APP.with(|cell| {
        *cell.borrow_mut() = Some(app.clone());
    });
    events::wire(&ui, &app);
    agent_bridge::start(&ui, &app);
    app::with_app(&app, |app| app.sync_all(&ui));
    start_frame_loop(&ui, &app);
    // The winit backend creates the window hidden before `show()`. If the HWND
    // is already available, strip the caption and disable DWM non-client
    // rendering while it is still hidden: the first visible frame then cannot
    // contain the native title bar, even for one frame.
    #[cfg(windows)]
    if let Some(hwnd) = hwnd_of(ui.window()) {
        glass::strip_native_frame(hwnd);
        glass::disable_nc_rendering(hwnd);
    }
    // Show once explicitly so the native HWND exists, then dress the window
    // (rounded corners + transparent-window fallback) before the first visible frame. The tick
    // re-applies only when the HWND, the maximise state or the activation state
    // changes.
    ui.show()?;
    apply_window_chrome(&ui);
    glass::remove_creation_hook(creation_hook);
    ui.run()
}

thread_local! {
    /// The shared app, reachable from an event-loop callback that a background
    /// worker posts back to the UI thread.
    static APP: RefCell<Option<Rc<RefCell<app::App>>>> = const { RefCell::new(None) };
}

/// Prints the requested tick rate and the display's own ceiling once at startup
/// when `OC_FRAME_TRACE` is set: the tick is what the app can produce, the panel
/// is what the user can see, and the report must not conflate them.
fn log_frame_budget() {
    if std::env::var_os("OC_FRAME_TRACE").is_some() {
        eprintln!("{}", glass::frame_budget_report(FRAME_TICK_MS));
    }
}

/// Appends a panic report to `%TEMP%\open-canvas-crash.log`.
///
/// A release build has no console, so an unexpected exit would otherwise leave
/// nothing behind. The hook keeps the default behaviour (the process still
/// aborts) and only adds the message, location and backtrace.
fn install_crash_log() {
    let path = std::env::temp_dir().join("open-canvas-crash.log");
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let backtrace = std::backtrace::Backtrace::force_capture();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            // A panic handler cannot do anything useful with a write error.
            writeln!(file, "--- panic at {stamp} ---\n{info}\n{backtrace}\n").ok();
        }
    }));
}

/// Runs `f` with the shared app on the UI thread (from an event-loop callback).
pub(crate) fn with_shared_app(f: impl FnOnce(&mut app::App)) {
    APP.with(|cell| {
        if let Some(rc) = cell.borrow().as_ref() {
            app::with_app(rc, f);
        }
    });
}

/// How often the interactive frame loop runs: one tick every 8 ms, i.e.
/// **125 Hz**. The tick mirrors the window's maximise state and pumps native
/// file drops; the window is opaque, so a resize costs no desktop work at all.
/// The display remains the ceiling - a 60 Hz panel can only present 60 fps no
/// matter how often the tick runs (see [`glass::display_refresh_hz`]).
pub const FRAME_TICK_MS: u64 = 8;

/// Set once the native window has been dressed: the HWND it was dressed for, the
/// maximise state and the activation state at that moment. A new HWND (recreated
/// window), a new maximise state or a flip between active and inactive
/// re-applies; every other tick is a handful of relaxed loads.
#[cfg(windows)]
static NATIVE_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static NATIVE_MAXIMIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether the window owned the foreground when it was last dressed. Windows
/// disables the acrylic material for inactive windows, so a flip re-applies the
/// backdrop (which repairs the accent path too).
#[cfg(windows)]
static NATIVE_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The blur level the backdrop was last dressed for (0–100). A change re-applies
/// the material, so the Appearance tab's Blur slider takes effect on the frame
/// tick after the setting changes, and 0 disables the backdrop outright.
#[cfg(windows)]
static NATIVE_BLUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The theme the accent tint was last dressed for. A light/dark switch changes
/// the acrylic gradient (white vs dark), so it is part of the re-dress key.
#[cfg(windows)]
static NATIVE_DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The physical window size (`width << 32 | height`) last dressed. A DPI or
/// size change can make Windows rewrite `GWL_STYLE`, so it is part of the
/// re-dress key and forces a traced style checkpoint.
#[cfg(windows)]
static NATIVE_SIZE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The scale factor (`f32::to_bits`) last dressed, so a monitor move that only
/// changes DPI also re-checks the style.
#[cfg(windows)]
static NATIVE_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Consecutive refused material requests for the current key. A refusal during
/// the very first frames (before DWM is ready for the window) gets a few more
/// tries instead of leaving an opaque window until the next activation flip.
#[cfg(windows)]
static NATIVE_RETRIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// How many extra times a refused backdrop is retried on the key it was
/// refused for. Three ticks at 125 Hz is ~24 ms — enough for the compositor,
/// cheap enough not to spin on a machine that genuinely cannot frost.
#[cfg(windows)]
const BACKDROP_RETRIES: u32 = 3;

/// Dresses the native window: the truly-frameless style, DWM rounding and the
/// transparent-window accent fallback. The visible frost is the captured,
/// blurred desktop painted by `glass_runtime`; the accent only keeps the first
/// frames and the area outside the capture see-through.
///
/// **The style guard runs every tick**, before the memoised key is checked: one
/// `GetWindowLongPtrW` read and a compare, with `SetWindowLongPtrW` +
/// `SWP_FRAMECHANGED` only when the caption bits drifted back in (a DPI change,
/// a size change or a restore from minimise can do that). The rest of the
/// native work is keyed on `(HWND, maximised, active, blur, size, scale)`, so
/// the rounding and accent calls run once at startup, once if the window is
/// recreated, once after each maximise/restore, once per activation flip, once
/// per blur-setting change and once per DPI/size change — never otherwise.
/// Neither call reads a screen pixel: the frost comes from the worker-thread
/// capture, while the accent is only the transparent-window fallback. The
/// activation poll is one `GetForegroundWindow` call per tick, so the OS
/// disabling acrylic for an inactive window is repaired as soon as the window
/// becomes active again.
///
/// winit keeps `WS_CAPTION` in the style even for `no-frame: true` (it hides
/// the non-client area in `WM_NCCALCSIZE` instead), so the checkpoint and the
/// per-tick guard keep rewriting `GWL_STYLE` to a borderless popup. Nothing
/// extends the DWM frame, so there is no caption for DWM to paint.
#[cfg(windows)]
fn apply_window_chrome(ui: &AppWindow) {
    use std::sync::atomic::Ordering;

    let Some(hwnd) = hwnd_of(ui.window()) else {
        // No native handle yet (or a non-Win32 backend): the plate stays
        // opaque and the next tick retries. Worth a line when tracing, because
        // "the frost never appears" often means this branch kept running.
        if std::env::var_os("OC_BACKDROP_TRACE").is_some() {
            eprintln!("backdrop: no native window handle yet");
        }
        return;
    };
    // Per-tick drift repair. This must not sit behind the memoised key: a
    // style that drifted on its own would otherwise stay decorated until some
    // unrelated key changed. On the steady path it is one read + compare and
    // writes nothing.
    glass::guard_native_frame(hwnd);
    let maximized = ui.get_window_maximized();
    let active = glass::window_is_active(hwnd);
    let blur = backdrop_blur(ui);
    let dark = ui.global::<crate::Theme>().get_dark();
    let size = ui.window().size();
    let scale = ui.window().scale_factor().to_bits();
    let size_key = (u64::from(size.width) << 32) | u64::from(size.height);
    let geometry_changed = NATIVE_SIZE.load(Ordering::Relaxed) != size_key
        || NATIVE_SCALE.load(Ordering::Relaxed) != scale;
    let key_matches = NATIVE_HWND.load(Ordering::Relaxed) == hwnd
        && NATIVE_MAXIMIZED.load(Ordering::Relaxed) == maximized
        && NATIVE_ACTIVE.load(Ordering::Relaxed) == active
        && NATIVE_BLUR.load(Ordering::Relaxed) == blur
        && NATIVE_DARK.load(Ordering::Relaxed) == dark;
    // A refused request (blur on, no accepted material) is retried a few times
    // on the same key before the opaque fallback is treated as final.
    let retrying =
        blur > 0 && !ui.get_backdrop() && NATIVE_RETRIES.load(Ordering::Relaxed) < BACKDROP_RETRIES;
    if !geometry_changed && key_matches && !retrying {
        return;
    }
    if geometry_changed && key_matches && !retrying {
        // A DPI/size change only needs the style checkpoint: the material and
        // the rounding do not depend on the window geometry, and a resize drag
        // must not re-issue the DWM calls on every tick.
        NATIVE_SIZE.store(size_key, Ordering::Relaxed);
        NATIVE_SCALE.store(scale, Ordering::Relaxed);
        glass::strip_native_frame(hwnd);
        glass::install_chrome_subclass(hwnd);
        glass::disable_nc_rendering(hwnd);
        return;
    }
    NATIVE_HWND.store(hwnd, Ordering::Relaxed);
    NATIVE_MAXIMIZED.store(maximized, Ordering::Relaxed);
    NATIVE_ACTIVE.store(active, Ordering::Relaxed);
    NATIVE_BLUR.store(blur, Ordering::Relaxed);
    NATIVE_DARK.store(dark, Ordering::Relaxed);
    NATIVE_SIZE.store(size_key, Ordering::Relaxed);
    NATIVE_SCALE.store(scale, Ordering::Relaxed);
    // The checkpoint re-dress: after the first show, every maximise/restore and
    // every DPI/size change. It prints the before/after bits under
    // `OC_WINDOW_TRACE`, so the style can be verified without a native caption
    // ever being visible.
    glass::strip_native_frame(hwnd);
    glass::install_chrome_subclass(hwnd);
    glass::disable_nc_rendering(hwnd);
    glass::round_corners(hwnd);
    // The Slint plate stays opaque unless the OS accepted a backdrop, so a
    // refused request degrades to the old window instead of a black surface.
    // Blur 0 asks for no material and returns `false`: the plate is opaque.
    let backdrop = glass::enable_backdrop(hwnd, blur, dark);
    if backdrop || blur == 0 {
        NATIVE_RETRIES.store(0, Ordering::Relaxed);
    } else {
        NATIVE_RETRIES.fetch_add(1, Ordering::Relaxed);
    }
    if ui.get_backdrop() != backdrop {
        ui.set_backdrop(backdrop);
    }
}

/// The Appearance tab's blur level as the 0–100 integer the backdrop helper
/// takes (clamped, so a bad value can never pick a material outside the bands).
#[cfg(windows)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0-100 (finite) first, so the rounded u32 is exact"
)]
fn backdrop_blur(ui: &AppWindow) -> u32 {
    let blur = ui.global::<crate::Theme>().get_blur();
    if blur.is_finite() {
        blur.clamp(0.0, 100.0).round() as u32
    } else {
        0
    }
}

/// Re-dresses the window immediately (used by the Appearance tab when the
/// setting changes, so the frost updates on the click instead of the next
/// tick). Clears the memoised key first; the call is idempotent.
#[cfg(windows)]
pub(crate) fn refresh_window_chrome(ui: &AppWindow) {
    NATIVE_HWND.store(0, std::sync::atomic::Ordering::Relaxed);
    NATIVE_RETRIES.store(0, std::sync::atomic::Ordering::Relaxed);
    apply_window_chrome(ui);
}

#[cfg(not(windows))]
pub(crate) fn refresh_window_chrome(_ui: &AppWindow) {}

#[cfg(not(windows))]
fn apply_window_chrome(_ui: &AppWindow) {}

/// Starts the 125 Hz interactive frame loop and keeps the window's maximise
/// state in step with the OS. The timer is owned by `main` for the lifetime of
/// `ui.run()`.
fn start_frame_loop(ui: &AppWindow, app: &Rc<RefCell<app::App>>) {
    let weak_ui = ui.as_weak();
    let weak_app = Rc::downgrade(app);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(FRAME_TICK_MS),
        move || {
            let (Some(ui), Some(app)) = (weak_ui.upgrade(), weak_app.upgrade()) else {
                return;
            };
            frame_tick(&ui, &app);
        },
    );
    // Leak the timer into the event loop: it must outlive this function, and
    // dropping it would stop the loop.
    std::mem::forget(timer);
}

/// One iteration of the interactive frame loop.
///
/// Five cheap effects, and nothing else: drain queued OS file drops, mirror the
/// window's maximise state (the title-bar drag is sampled here), apply the
/// coalesced node gesture, apply the coalesced viewport input, and dress the
/// native window when its handle, its maximise state or its activation state
/// changed. The frame tick itself performs no screen access; `glass_runtime`
/// only reads the window origin and schedules the worker capture, so an OS
/// resize costs the same as an idle tick.
fn frame_tick(ui: &AppWindow, app: &Rc<RefCell<app::App>>) {
    // OS file drops: install the handler on the current HWND (idempotent, and
    // re-installs after the window is recreated) and drain any queued paths.
    // With an empty queue this is one map lookup plus one check.
    crate::file_drop::pump(ui, app);
    app::with_app(app, |app| {
        app.sync_window_state(ui);
        // A title-bar drag is sampled here, not from Slint's pointer events:
        // once the window follows the cursor, the cursor stops moving inside
        // the window's coordinate space, so no further `move` events arrive and
        // an event-driven drag simply stops following. This is a no-op unless a
        // drag is in flight.
        app.drag_tick(ui);
        // Coalesced node gestures: a node drag/resize/connect pointer move
        // only recorded its latest position, so the tick applies it once and
        // updates just the dragged rows, the wires touching them and the drag
        // overlays (one paint per frame).
        app.node_drag_tick(ui);
        // Canvas input coalescing: pointer moves and wheel events only recorded
        // their latest state, so this is where a pan/zoom becomes a frame — one
        // viewport application and one paint per tick, whatever the event
        // rate (a 1000 Hz mouse now paints at most 125 frames/s). A no-op when
        // there is no pending input.
        app.view_tick(ui);
        // Still right-hold: paint the Zelda-stamina ring and return home when
        // it fills. The ring itself is a tiny SVG arc, not a screen capture.
        app.home_hold_tick(ui);
    });
    // Rounding + backdrop, keyed on the native handle, the maximise state and
    // the activation state: a plain tick pays three atomic loads, one
    // `GetForegroundWindow` call and returns.
    apply_window_chrome(ui);
    // Keep the captured desktop pinned to the window and decide whether the
    // frost needs a re-blur or a fresh capture. Cheap on the common path (one
    // `GetWindowRect`, one placement compare); capture + Gaussian run on a
    // worker.
    glass_runtime::step(ui, app);
}

/// The window's native handle (Windows only; the rounded-corner preference
/// needs it).
#[cfg(windows)]
pub(crate) fn hwnd_of(window: &slint::Window) -> Option<isize> {
    use slint::winit_030::{winit, WinitWindowAccessor};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window
        .with_winit_window(|w| {
            let handle = HasWindowHandle::window_handle(w).ok()?;
            match handle.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                _ => None,
            }
        })
        .flatten()
}

/// Appends the `Microsoft YaHei` face to the Han/Hira/Kana fallback chain.
///
/// Slint built-in fallback leaves the per-glyph choice to fontique system
/// fallback, which on an en-US process locale can select a Japanese face lacking
/// simplified-Chinese-only glyphs (e.g. render as tofu). Making `YaHei` the first
/// Han fallback pins the CJK behavior to the stack the web spec names.
fn register_cjk_fallback() {
    use slint::fontique_011::fontique;

    for candidate in [r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\msyh.ttf"] {
        let Ok(bytes) = std::fs::read(candidate) else {
            continue;
        };
        let blob = fontique::Blob::new(Arc::new(bytes));
        let mut collection = slint::fontique_011::shared_collection();
        let fonts = collection.register_fonts(blob, None);
        if fonts.is_empty() {
            continue;
        }
        let families: Vec<fontique::FamilyId> = fonts.iter().map(|(id, _)| *id).collect();
        for script in [*b"Hani", *b"Hira", *b"Kana"] {
            collection.append_fallbacks(
                fontique::FallbackKey::new(fontique::Script::from_bytes(script), None),
                families.iter().copied(),
            );
        }
        return;
    }
}
