//! Native window helpers: the transparent-window fallback, rounded corners, the
//! cursor read for the manual title-bar drag, the display ceiling reported by
//! the frame-budget line, and the frameless-style guard.
//!
//! The visible frosted backdrop is the captured, Gaussian-blurred desktop
//! (`crates/app/src/glass_capture.rs`), painted under the theme plate. The
//! legacy `SetWindowCompositionAttribute` acrylic accent remains only as the
//! transparent-window fallback for the first frames and for areas outside the
//! capture. Its gradient is fully transparent, so it never tints the capture.
//! The accent covers the whole client area itself
//! (`ACCENT_FLAG_DRAW_ALL_BORDERS`), so the window never needs an extended DWM
//! frame. That is deliberate: `DwmExtendFrameIntoClientArea` is what makes DWM
//! paint the native caption strip (the window title over an accent band) above
//! the app content, and the app draws its own window chrome on every screen.
//! The default path therefore never extends the frame, and the DWM system
//! material (mica) is gone: it only paints in an extended frame, so keeping it
//! as an opt-in would have reintroduced the strip it was hiding.
//!
//! **A frameless winit window still carries the caption style.**
//! winit 0.30 implements `decorations(false)` by removing the non-client area
//! in `WM_NCCALCSIZE`; it deliberately keeps `WS_CAPTION | WS_SYSMENU |
//! WS_THICKFRAME` (plus `WS_BORDER`/`WS_DLGFRAME`, which `WS_CAPTION` contains)
//! in `GWL_STYLE` ("Required styles to properly support common window
//! functionality like aero snap"). Windows styles are also rewritten on some
//! transitions — creation, show, maximise/restore, DPI or size changes — so the
//! style is re-read and repaired:
//!
//! * [`strip_native_frame`] is the checkpoint call after `show()` and after
//!   every maximise/restore, DPI or size change. It always reads the bits,
//!   rewrites only when they drift, and writes an `OC_WINDOW_TRACE` line with
//!   the before/after style on every call.
//! * [`guard_native_frame`] is the per-frame-tick version: one
//!   `GetWindowLongPtrW` read and a compare; it only calls `SetWindowLongPtrW`
//!   with `SWP_FRAMECHANGED` when the style actually drifted, and traces that
//!   repair.
//!
//! Together they guarantee the window never keeps
//! `WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME`, so DWM
//! has no caption bits to paint a native title bar from. [`frameless_window_style`],
//! [`has_native_chrome`] and [`window_style_trace`] are pure so the rewrite and
//! the trace format are tested without a desktop.
//!
//! Windows disables the acrylic material for inactive windows, so the native
//! pass also reads the foreground window once per tick and re-applies the
//! backdrop when the window's activation state flips.
//!
//! The blur setting (0–100, persisted in `ui.json`) selects the material: 0
//! disables the accent entirely (the caller keeps the opaque plate), 1–100
//! applies the acrylic accent. `OC_BACKDROP_TRACE` prints one line per attempt
//! with the material, whether the frame was extended (always "no") and whether
//! the OS accepted it.
//!
//! Everything that touches the OS is Windows-only; [`frame_budget_report`],
//! [`BackdropMaterial::for_blur`], [`backdrop_trace`], [`frameless_window_style`],
//! [`has_native_chrome`], [`needs_frameless_repair`] and [`window_style_trace`]
//! are platform-independent so their rules read the same everywhere.

/// `WS_CAPTION` (`WS_BORDER | WS_DLGFRAME`): the native title bar and its 1px
/// frame.
pub const WS_CAPTION: u32 = 0x00C0_0000;
/// `WS_SYSMENU`: the system menu / system buttons in the caption.
pub const WS_SYSMENU: u32 = 0x0008_0000;
/// `WS_THICKFRAME`: the sizing frame around the window.
pub const WS_THICKFRAME: u32 = 0x0004_0000;
/// `WS_BORDER`: the thin native border (part of `WS_CAPTION`'s value).
pub const WS_BORDER: u32 = 0x0080_0000;
/// `WS_DLGFRAME`: the dialog frame (the other half of `WS_CAPTION`).
pub const WS_DLGFRAME: u32 = 0x0040_0000;
/// `WS_POPUP`: the borderless window family the app draws itself.
pub const WS_POPUP: u32 = 0x8000_0000;

/// Every style bit that lets Windows (or DWM) draw native window chrome. The
/// window must never keep any of these: the app paints its own title, its own
/// settings and its own minimise / maximise / close controls.
#[rustfmt::skip]
pub const NATIVE_CHROME_BITS: u32 = WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME;

/// The `GWL_STYLE` a truly frameless window needs: every native caption bit
/// (caption, system menu, sizing frame, border, dialog frame) is removed and
/// the window becomes a borderless popup.
///
/// This is a pure function on purpose: winit's `decorations(false)` keeps the
/// caption bits in the style (it hides the non-client area in `WM_NCCALCSIZE`
/// instead), and Windows may rewrite the style on a transition. Pinning the
/// rewrite here lets a headless test check the exact winit style without a
/// desktop.
pub const fn frameless_window_style(style: u32) -> u32 {
    (style & !NATIVE_CHROME_BITS) | WS_POPUP
}

/// Whether `style` still contains any native chrome bit.
pub const fn has_native_chrome(style: u32) -> bool {
    style & NATIVE_CHROME_BITS != 0
}

/// Whether a style read needs a repair: it still has native chrome, is not a
/// popup, or both. The pure half of the per-tick guard, so the "only write on
/// drift" rule is testable.
pub const fn needs_frameless_repair(style: u32) -> bool {
    frameless_window_style(style) != style
}

/// The low 32 bits of the pointer-width `GWL_STYLE` read.
///
/// `GetWindowLongPtrW` returns a `LONG_PTR`; the style itself is 32 bits, and a
/// style with `WS_POPUP` (the high bit set) can arrive sign-extended on a 64-bit
/// build. Masking keeps `WS_POPUP` from being misread as a negative value (and
/// therefore as a permanent drift). Pure, so the rule is tested headlessly.
pub fn window_style_bits(raw: isize) -> u32 {
    u32::try_from(raw.cast_unsigned() & 0xFFFF_FFFF).unwrap_or_default()
}

/// One `OC_WINDOW_TRACE` line: the bits read before the re-apply, the bits the
/// frameless rewrite wants, and whether the process wrote them. Pure, so the
/// trace format is tested headlessly.
pub fn window_style_trace(before: u32, after: u32, applied: bool) -> String {
    format!(
        "window: style 0x{before:08X} -> 0x{after:08X} (native chrome {}, popup {}, {})",
        if has_native_chrome(after) {
            "yes"
        } else {
            "no"
        },
        if after & WS_POPUP != 0 { "yes" } else { "no" },
        if applied { "applied" } else { "unchanged" }
    )
}

/// The frost strength the Appearance tab's **Blur** slider asks for, resolved
/// to the OS material that implements it.
///
/// The app has exactly two states now:
///
/// * `0` — no backdrop at all: the caller keeps the opaque plate.
/// * `1..=100` — the acrylic accent, the only frameless material (the DWM
///   system materials need an extended frame, which brings the native caption
///   strip back). Anything above 100 is clamped by [`enable_backdrop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackdropMaterial {
    /// Blur 0: no backdrop, the opaque window plate.
    None,
    /// The frameless acrylic accent.
    Acrylic,
}

impl BackdropMaterial {
    /// The material for a blur level (see the type's docs).
    pub const fn for_blur(blur: u32) -> Self {
        if blur == 0 {
            Self::None
        } else {
            Self::Acrylic
        }
    }

    /// The label the `OC_BACKDROP_TRACE` line prints.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Acrylic => "acrylic",
        }
    }
}

/// One `OC_BACKDROP_TRACE` line: the material the blur level chose, whether the
/// frame was extended (never on this path) and whether the OS accepted the
/// accent. Pure, so the trace format is tested headlessly.
pub fn backdrop_trace(material: BackdropMaterial, accepted: bool) -> String {
    format!(
        "backdrop: material {}, frame-extended no, strategy accent, accepted {}",
        material.as_str(),
        if accepted { "yes" } else { "no" }
    )
}

/// The primary display's refresh rate, or `None` when it cannot be read.
#[cfg(windows)]
pub fn display_refresh_hz() -> Option<u32> {
    win::display_refresh_hz()
}

/// Non-Windows: the display refresh rate is unknown here, so callers report
/// "unknown" rather than inventing a number.
#[cfg(not(windows))]
pub fn display_refresh_hz() -> Option<u32> {
    None
}

/// Raises the process timer resolution so the interactive tick is real.
///
/// Windows clamps every timer (including `slint::Timer`'s) to the system clock
/// tick, 15.6 ms by default — about 64 Hz. The app asks for an 8 ms tick and
/// samples the cursor for the manual window drag there, so the clamp silently
/// halved both rates and made a drag follow late. `timeBeginPeriod(1)` lifts the
/// clamp for the life of the process; the OS restores the default on exit, so
/// there is no matching `timeEndPeriod`.
#[cfg(windows)]
pub fn begin_high_resolution_timer() {
    win::time_begin_period(1);
}

/// Non-Windows: the event loop's timer resolution is not clamped the same way.
#[cfg(not(windows))]
pub fn begin_high_resolution_timer() {}

/// One line of evidence for the frame budget: the tick rate the app requests
/// and the refresh rate the panel actually presents. A 125 Hz tick on a 60 Hz
/// display is still shown at 60 fps, and the report must say so instead of
/// implying the higher number is visible.
pub fn frame_budget_report(tick_ms: u64) -> String {
    let tick_ms = tick_ms.max(1);
    let requested = 1_000 / tick_ms;
    format!(
        "frames: interactive tick {tick_ms} ms ({requested} Hz requested); display ceiling {}",
        display_refresh_hz().map_or_else(
            || "unknown (assume 60 Hz)".to_string(),
            |hz| format!("{hz} Hz")
        )
    )
}

/// Asks the OS to frost the desktop behind `hwnd`, at the strength `blur` asks
/// for (0 = off). Returns `true` when the acrylic accent was accepted, i.e.
/// when the window plate may be translucent.
///
/// **This path never extends the DWM frame.** The whole client area is covered
/// by the accent policy itself (`ACCENT_FLAG_DRAW_ALL_BORDERS`), so
/// `DwmExtendFrameIntoClientArea` is not called at all: it is the API that
/// makes DWM paint the native caption strip above the app, and no material is
/// worth that. The DWM system material (mica) also needs the extended frame, so
/// it is not offered.
///
/// If the accent is refused the caller keeps the opaque plate
/// (`backdrop = false`), so the window can never end up black. Blur 0 disables
/// the accent explicitly, which returns the window to the opaque plate.
///
/// With `OC_BACKDROP_TRACE` set, one [`backdrop_trace`] line is printed per
/// attempt (material, frame extension, result), so the user can confirm which
/// path actually ran.
#[cfg(windows)]
pub fn enable_backdrop(hwnd: isize, blur: u32, dark: bool) -> bool {
    let material = BackdropMaterial::for_blur(blur.clamp(0, 100));
    let accepted = match material {
        BackdropMaterial::None => {
            win::accent_disabled(hwnd);
            false
        }
        BackdropMaterial::Acrylic => win::accent_backdrop(hwnd, dark),
    };
    log_backdrop_trace(&backdrop_trace(material, accepted));
    accepted
}

/// Non-Windows: no backdrop API is wired here, so the caller keeps the opaque
/// plate.
#[cfg(not(windows))]
pub fn enable_backdrop(_hwnd: isize, _blur: u32, _dark: bool) -> bool {
    false
}

/// Prints a backdrop trace line when `OC_BACKDROP_TRACE` is set.
#[cfg(windows)]
fn log_backdrop_trace(line: &str) {
    if std::env::var_os("OC_BACKDROP_TRACE").is_some() {
        eprintln!("{line}");
    }
}

/// Rewrites the native window style to a borderless popup and forces a frame
/// recalculation. Returns `true` when no native chrome bit remains.
///
/// This is the checkpoint call: after the HWND exists (the first `show()`),
/// after every maximise/restore, and after a DPI or size change. The style read
/// always happens; `SetWindowLongPtrW` + `SWP_FRAMECHANGED` only run when the
/// bits actually need the rewrite. With `OC_WINDOW_TRACE` set, the before/after
/// bits are printed on every call, so a re-dress can be verified even when it
/// was a no-op.
#[cfg(windows)]
pub fn strip_native_frame(hwnd: isize) -> bool {
    win::strip_native_frame(hwnd)
}

/// Non-Windows: the platform window manager owns the decorations, so there is
/// no Win32 style to rewrite.
#[cfg(not(windows))]
pub fn strip_native_frame(_hwnd: isize) -> bool {
    true
}

/// Installs the CBT creation hook that strips the native caption before the
/// window is created. Returns the hook handle (0 on non-Windows / failure).
#[cfg(windows)]
pub fn install_creation_hook() -> isize {
    win::install_creation_hook()
}

#[cfg(not(windows))]
pub fn install_creation_hook() -> isize {
    0
}

/// Removes the creation hook after the main window exists.
#[cfg(windows)]
pub fn remove_creation_hook(hook: isize) {
    win::remove_creation_hook(hook);
}

#[cfg(not(windows))]
pub fn remove_creation_hook(_hook: isize) {}

/// Installs the non-client subclass that refuses to paint a native caption on
/// every transition, even if the style is rewritten behind the app's back.
#[cfg(windows)]
pub fn install_chrome_subclass(hwnd: isize) {
    win::install_chrome_subclass(hwnd);
}

#[cfg(not(windows))]
pub fn install_chrome_subclass(_hwnd: isize) {}

/// The per-frame-tick guard: one `GetWindowLongPtrW` read and a compare, and a
/// `SetWindowLongPtrW` + `SWP_FRAMECHANGED` only when the style drifted.
/// Returns `true` when the window is frameless afterwards.
///
/// Windows (and winit) can re-apply the decorated style on transitions the app
/// does not control — a DPI change, a size change, a restore from minimise — so
/// the tick re-checks instead of trusting the last checkpoint. A steady tick is
/// just the read + compare; nothing is written.
#[cfg(windows)]
pub fn guard_native_frame(hwnd: isize) -> bool {
    win::guard_native_frame(hwnd)
}

/// Non-Windows: nothing to guard.
#[cfg(not(windows))]
pub fn guard_native_frame(_hwnd: isize) -> bool {
    true
}

/// Whether `hwnd` currently owns the foreground, i.e. whether the window is the
/// active one.
///
/// Windows disables the acrylic material while a window is inactive, so the
/// frame tick polls this once and re-applies the backdrop when it flips. One
/// `GetForegroundWindow` call per tick, no memory touched.
#[cfg(windows)]
pub fn window_is_active(hwnd: isize) -> bool {
    win::foreground_hwnd() == Some(hwnd)
}

/// Non-Windows: the platform window manager owns activation, so the native
/// pass treats the window as active (it is a no-op there anyway).
#[cfg(not(windows))]
pub fn window_is_active(_hwnd: isize) -> bool {
    true
}

/// The primary display's refresh rate, rounded corners and the style guard are
/// the only DWM calls the app makes. No frame extension.
#[cfg(windows)]
mod win {
    use core::ffi::c_void;

    use super::{frameless_window_style, needs_frameless_repair, window_style_trace};

    /// `POINT` for `GetCursorPos` (physical screen pixels).
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct WinPoint {
        x: i32,
        y: i32,
    }

    /// `ACCENT_POLICY` (`user32.h`, undocumented): the acrylic accent.
    #[repr(C)]
    struct AccentPolicy {
        state: i32,
        flags: i32,
        gradient_color: u32,
        animation_id: i32,
    }

    /// `WINDOWCOMPOSITIONATTRIBDATA` (`user32.h`).
    #[repr(C)]
    struct WindowCompositionAttributeData {
        attribute: i32,
        data: *mut c_void,
        size: usize,
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn GetDeviceCaps(dc: isize, index: i32) -> i32;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
        fn GetCursorPos(point: *mut WinPoint) -> i32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn SetWindowPos(
            hwnd: isize,
            insert_after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn SetWindowsHookExW(
            id_hook: i32,
            callback: Option<HookProc>,
            module: isize,
            thread_id: u32,
        ) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
        fn RedrawWindow(
            hwnd: isize,
            update_rect: *const c_void,
            update_rgn: isize,
            flags: u32,
        ) -> i32;
    }

    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(
            hwnd: isize,
            callback: SubclassProc,
            id: usize,
            ref_data: usize,
        ) -> i32;
        fn DefSubclassProc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    // `SetWindowCompositionAttribute` is exported by name from user32 but is not
    // listed in `user32.lib`, so it is resolved at runtime below.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
        fn GetCurrentThreadId() -> u32;
    }

    /// `WH_CBT` / `HCBT_CREATEWND` (`winuser.h`): the creation hook that rewrites
    /// the style before Windows creates the window.
    const WH_CBT: i32 = 5;
    const HCBT_CREATEWND: i32 = 3;
    /// `WS_EX_WINDOWEDGE`: the raised native border around a window.
    const WS_EX_WINDOWEDGE: u32 = 0x0000_0100;
    /// Non-client messages the subclass neutralises.
    const WM_NCCALCSIZE: u32 = 0x0083;
    const WM_NCPAINT: u32 = 0x0085;
    const WM_NCACTIVATE: u32 = 0x0086;
    const RDW_INVALIDATE: u32 = 0x0001;
    const RDW_UPDATENOW: u32 = 0x0100;
    const RDW_FRAME: u32 = 0x0400;

    type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;
    type SubclassProc = unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize;

    /// `CREATESTRUCTW` (`winuser.h`): the creation parameters handed to
    /// `CreateWindowExW`; the hook edits `style` before the window exists.
    #[repr(C)]
    struct CreateStructW {
        lp_create_params: *mut c_void,
        h_instance: isize,
        h_menu: isize,
        hwnd_parent: isize,
        cy: i32,
        cx: i32,
        y: i32,
        x: i32,
        style: u32,
        lpsz_name: *const u16,
        lpsz_class: *const u16,
        dw_ex_style: u32,
    }

    /// `CBT_CREATEWND` (`winuser.h`).
    #[repr(C)]
    struct CbtCreateWnd {
        lpcs: *mut CreateStructW,
        hwnd_insert_after: isize,
    }

    thread_local! {
        /// The creation hook only rewrites the first top-level winit window; the
        /// event loop's hidden message window and later popups keep their style.
        static CREATION_TARGET_DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// The HWND the non-client subclass is installed on (0 until installed).
        static SUBCLASS_HWND: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    }

    /// Compares a NUL-terminated UTF-16 string without allocating.
    unsafe fn wide_eq(ptr: *const u16, expected: &str) -> bool {
        if ptr.is_null() {
            return false;
        }
        for (index, ch) in expected.encode_utf16().enumerate() {
            if unsafe { *ptr.add(index) } != ch {
                return false;
            }
        }
        (unsafe { *ptr.add(expected.len()) }) == 0
    }

    /// `HCBT_CREATEWND`: strip the caption bits before `CreateWindowExW` creates
    /// the window. This is the only point where the native title bar can be
    /// prevented from ever existing, rather than repaired after the first paint.
    unsafe extern "system" fn creation_hook(code: i32, wparam: usize, lparam: isize) -> isize {
        if code == HCBT_CREATEWND {
            let create = unsafe { &*(lparam as *const CbtCreateWnd) };
            if !create.lpcs.is_null() {
                let cs = unsafe { &mut *create.lpcs };
                let top_level = cs.hwnd_parent == 0;
                let winit_class = unsafe { wide_eq(cs.lpsz_class, "Window Class") };
                let already = CREATION_TARGET_DONE.with(std::cell::Cell::get);
                if top_level && winit_class && !already {
                    CREATION_TARGET_DONE.with(|done| done.set(true));
                    cs.style = super::frameless_window_style(cs.style);
                    cs.dw_ex_style &= !WS_EX_WINDOWEDGE;
                }
            }
        }
        unsafe { CallNextHookEx(0, code, wparam, lparam) }
    }

    /// Installs the CBT creation hook on the UI thread. Returns 0 when the OS
    /// refuses it (the style guard still repairs the window afterwards).
    pub fn install_creation_hook() -> isize {
        CREATION_TARGET_DONE.with(|done| done.set(false));
        unsafe { SetWindowsHookExW(WH_CBT, Some(creation_hook), 0, GetCurrentThreadId()) }
    }

    /// Removes the creation hook after the main window exists.
    pub fn remove_creation_hook(hook: isize) {
        if hook != 0 {
            unsafe {
                UnhookWindowsHookEx(hook);
            }
        }
    }

    /// `WM_NCCALCSIZE` / `WM_NCACTIVATE` / `WM_NCPAINT`: the last line of
    /// defence. Even if Windows or winit rewrites `GWL_STYLE` during a
    /// transition, this subclass makes the client cover the whole window and
    /// refuses to let `DefWindowProc` paint a non-client caption.
    unsafe extern "system" fn chrome_subclass(
        hwnd: isize,
        msg: u32,
        wparam: usize,
        lparam: isize,
        _id: usize,
        _ref_data: usize,
    ) -> isize {
        match msg {
            WM_NCCALCSIZE if wparam != 0 => {
                // Returning 0 makes the client area cover the full window.
                return 0;
            }
            WM_NCACTIVATE => {
                // Claim the activation repaint without drawing the caption.
                return 1;
            }
            WM_NCPAINT => {
                // No non-client pixels, ever.
                return 0;
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Installs the non-client subclass once per HWND and forces a frame
    /// recalculation so any caption painted before the subclass existed is
    /// erased immediately.
    pub fn install_chrome_subclass(hwnd: isize) {
        let installed = SUBCLASS_HWND.with(std::cell::Cell::get);
        if installed == hwnd {
            return;
        }
        let ok = unsafe { SetWindowSubclass(hwnd, chrome_subclass, 1, 0) } != 0;
        if !ok {
            return;
        }
        SUBCLASS_HWND.with(|slot| slot.set(hwnd));
        unsafe {
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            RedrawWindow(
                hwnd,
                std::ptr::null(),
                0,
                RDW_INVALIDATE | RDW_FRAME | RDW_UPDATENOW,
            );
        }
    }

    /// The accent entry point once resolved.
    type SetWindowCompositionAttribute =
        unsafe extern "system" fn(isize, *mut WindowCompositionAttributeData) -> i32;

    // `timeBeginPeriod` (`mmsystem.h`): 1 ms timer resolution for the process.
    #[link(name = "winmm")]
    extern "system" {
        fn timeBeginPeriod(period: u32) -> u32;
    }

    /// Asks the OS for `period` ms timer resolution. The returned `MMRESULT` is
    /// ignored: the only failure mode is "already at least this fine".
    pub fn time_begin_period(period: u32) {
        // SAFETY: `timeBeginPeriod` takes an integer and touches no memory.
        unsafe {
            timeBeginPeriod(period);
        }
    }

    /// `DWMWA_NCRENDERING_POLICY`: DWM's non-client rendering policy.
    const DWMWA_NCRENDERING_POLICY: u32 = 2;
    /// `DWMNCRP_DISABLED`: DWM must not paint the native caption/frame.
    const DWMNCRP_DISABLED: i32 = 1;
    /// `DWMWA_WINDOW_CORNER_PREFERENCE` (Windows 11).
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    /// `DWMWA_BORDER_COLOR` (Windows 11).
    const DWMWA_BORDER_COLOR: u32 = 34;
    /// `DWMWCP_ROUND`: the OS clips the frameless window to its rounded frame.
    const DWMWCP_ROUND: i32 = 2;
    /// `DWMWA_COLOR_NONE`: draw no border at all, so the rounded edge has no
    /// square 1px frame around it.
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

    /// `WCA_ACCENT_POLICY` of `SetWindowCompositionAttribute`.
    const WCA_ACCENT_POLICY: i32 = 19;
    /// `ACCENT_ENABLE_ACRYLICBLURBEHIND`: the Windows 10/11 acrylic material.
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
    /// `ACCENT_DISABLED`: turns the accent material back off (blur 0).
    const ACCENT_DISABLED: i32 = 0;
    /// `ACCENT_FLAG_DRAW_ALL_BORDERS`: the accent covers the whole client area.
    const ACCENT_FLAG_DRAW_ALL_BORDERS: i32 = 2;
    /// A transparent acrylic gradient. The visible frost is the captured,
    /// blurred desktop; the accent is only the transparent-window fallback. A
    /// non-zero gradient is painted opaque by the accent API, which was what
    /// hid the desktop; zero keeps the window transparent at every opacity.
    const ACCENT_GRADIENT_LIGHT: u32 = 0x00_00_00_00;
    const ACCENT_GRADIENT_DARK: u32 = 0x00_00_00_00;

    /// `GWL_STYLE` (`winuser.h`): the window style index.
    const GWL_STYLE: i32 = -16;
    /// `SWP_FRAMECHANGED`: recompute the non-client area after a style change.
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;

    /// Reads `GWL_STYLE` as the `u32` the pure style functions take, masking
    /// the pointer-width sign extension (see [`super::window_style_bits`]).
    fn window_style(hwnd: isize) -> u32 {
        super::window_style_bits(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) })
    }

    /// Writes `GWL_STYLE` and makes Windows drop the old non-client area before
    /// the next frame (`SWP_FRAMECHANGED`). The caller has already established
    /// that the style changed.
    fn set_window_style(hwnd: isize, style: u32) {
        let value = isize::try_from(style).unwrap_or_default();
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, value);
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Prints the before/after bits when `OC_WINDOW_TRACE` is set.
    fn trace(before: u32, after: u32, applied: bool) {
        if std::env::var_os("OC_WINDOW_TRACE").is_some() {
            eprintln!("{}", window_style_trace(before, after, applied));
        }
    }

    /// Checkpoint re-dress: reads the style, repairs it only when it drifted,
    /// and traces before/after on every call.
    pub fn strip_native_frame(hwnd: isize) -> bool {
        let before = window_style(hwnd);
        let after = frameless_window_style(before);
        let applied = after != before;
        if applied {
            set_window_style(hwnd, after);
        }
        let actual = window_style(hwnd);
        trace(before, actual, applied);
        !super::has_native_chrome(actual)
    }

    /// Per-tick drift repair: returns immediately on the common path (the style
    /// already equals the frameless rewrite) after one `GetWindowLongPtrW`.
    pub fn guard_native_frame(hwnd: isize) -> bool {
        let before = window_style(hwnd);
        if !needs_frameless_repair(before) {
            return true;
        }
        let after = frameless_window_style(before);
        set_window_style(hwnd, after);
        let actual = window_style(hwnd);
        trace(before, actual, true);
        !super::has_native_chrome(actual)
    }

    /// Turns the acrylic accent off (blur 0), leaving the caller's opaque plate.
    pub fn accent_disabled(hwnd: isize) {
        accent_with_state(hwnd, ACCENT_DISABLED, 0);
    }

    /// The legacy acrylic accent. This is the only backdrop material: it covers
    /// the client area without an extended DWM frame, so no native caption can
    /// appear above the app. The gradient is theme-aware so the light theme
    /// stays white instead of turning into dark glass at low opacity.
    pub fn accent_backdrop(hwnd: isize, dark: bool) -> bool {
        accent_with_state(
            hwnd,
            ACCENT_ENABLE_ACRYLICBLURBEHIND,
            if dark {
                ACCENT_GRADIENT_DARK
            } else {
                ACCENT_GRADIENT_LIGHT
            },
        )
    }

    /// Applies one accent state (`ACCENT_ENABLE_ACRYLICBLURBEHIND` or
    /// `ACCENT_DISABLED`). The symbol is resolved at runtime because it is
    /// exported by user32 but absent from `user32.lib`.
    fn accent_with_state(hwnd: isize, state: i32, gradient_color: u32) -> bool {
        let module_name: Vec<u16> = "user32.dll\0".encode_utf16().collect();
        let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
        if module == 0 {
            return false;
        }
        let proc = unsafe {
            GetProcAddress(
                module,
                c"SetWindowCompositionAttribute".as_ptr().cast::<u8>(),
            )
        };
        if proc.is_null() {
            return false;
        }
        // SAFETY: the symbol is `SetWindowCompositionAttribute` and the
        // signature is the documented one; the policy pointer outlives the call.
        let apply: SetWindowCompositionAttribute = unsafe { std::mem::transmute(proc) };
        let mut policy = AccentPolicy {
            state,
            flags: ACCENT_FLAG_DRAW_ALL_BORDERS,
            gradient_color,
            animation_id: 0,
        };
        let mut data = WindowCompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: std::ptr::from_mut(&mut policy).cast(),
            size: std::mem::size_of::<AccentPolicy>(),
        };
        let applied = unsafe { apply(hwnd, std::ptr::from_mut(&mut data)) };
        applied != 0
    }

    /// The cursor's physical screen position, or `None` when the read fails.
    /// The title-bar drag reads it once per pointer move and turns the delta
    /// into a `Window::set_position` call, keeping the event loop alive.
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut point = WinPoint::default();
        (unsafe { GetCursorPos(std::ptr::from_mut(&mut point)) } != 0).then_some((point.x, point.y))
    }

    /// The foreground window's handle, or `None` when Windows reports none.
    /// The native-window pass compares it with its own HWND once per tick.
    pub fn foreground_hwnd() -> Option<isize> {
        let hwnd = unsafe { GetForegroundWindow() };
        (hwnd != 0).then_some(hwnd)
    }

    /// The primary display's refresh rate in Hz (`GetDeviceCaps(VREFRESH)`).
    ///
    /// This is the honest ceiling on how fast anything this app draws can be
    /// **seen**: the interactive tick may run at 125 Hz, but a 60 Hz panel still
    /// presents 60 frames per second. `None` when the driver answers `1`
    /// ("hardware default") or the query fails, in which case callers should
    /// assume 60 Hz rather than claim a number.
    pub fn display_refresh_hz() -> Option<u32> {
        /// `VREFRESH` (`wingdi.h`).
        const VREFRESH: i32 = 116;
        let dc = unsafe { GetDC(0) };
        if dc == 0 {
            return None;
        }
        let hz = unsafe { GetDeviceCaps(dc, VREFRESH) };
        unsafe {
            ReleaseDC(0, dc);
        }
        u32::try_from(hz).ok().filter(|hz| *hz > 1)
    }

    /// Tells DWM to stop rendering the window's non-client area.
    ///
    /// The style guard removes `WS_CAPTION`, but Windows still paints the
    /// caption for one frame on creation and on some transitions. This DWM
    /// policy is the compositor-level guard: even if the style drifts back for a
    /// frame, DWM will not draw the native title bar over the app's own chrome.
    /// Best-effort on older systems; the style guard remains the primary fix.
    pub fn disable_nc_rendering(hwnd: isize) {
        let policy = DWMNCRP_DISABLED;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                std::ptr::from_ref(&policy).cast(),
                u32::try_from(std::mem::size_of_val(&policy)).unwrap_or_default(),
            );
        }
    }

    /// Asks DWM to round the frameless window and to drop its square border.
    ///
    /// Best-effort: Windows 10 and older reject both attributes, which is fine
    /// (the window simply keeps square corners).
    pub fn round_corners(hwnd: isize) {
        let preference = DWMWCP_ROUND;
        let rounded = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                std::ptr::from_ref(&preference).cast(),
                u32::try_from(std::mem::size_of_val(&preference)).unwrap_or_default(),
            )
        } == 0;
        if rounded {
            let border = DWMWA_COLOR_NONE;
            unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    std::ptr::from_ref(&border).cast(),
                    u32::try_from(std::mem::size_of_val(&border)).unwrap_or_default(),
                );
            }
        }
    }
}

#[cfg(windows)]
pub use win::{cursor_pos, disable_nc_rendering, round_corners};

#[cfg(not(windows))]
pub fn round_corners(_hwnd: isize) {}

#[cfg(not(windows))]
pub fn disable_nc_rendering(_hwnd: isize) {}

/// Non-Windows: there is no Win32 cursor read; the manual drag simply stays
/// put (the platform window manager owns the move there).
#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The startup line names both numbers: the tick the app requests and the
    /// display ceiling it is limited by ("unknown" is stated, never guessed).
    #[test]
    fn frame_budget_report_states_both_rates() {
        let report = frame_budget_report(8);
        assert!(
            report.contains("interactive tick 8 ms (125 Hz requested)"),
            "{report}"
        );
        assert!(report.contains("display ceiling"), "{report}");
    }

    /// A degenerate tick can never divide by zero.
    #[test]
    fn frame_budget_report_clamps_the_tick() {
        assert!(frame_budget_report(0).contains("interactive tick 1 ms"));
    }

    /// winit 0.30 implements `decorations(false)` by hiding the non-client area
    /// in `WM_NCCALCSIZE`, but it keeps `WS_CAPTION | WS_SYSMENU |
    /// WS_THICKFRAME` in `GWL_STYLE`, and `WS_CAPTION` itself contains
    /// `WS_BORDER | WS_DLGFRAME`. The rewrite must remove every one of those
    /// bits and make the window a borderless popup; it must be idempotent so a
    /// re-dress is a no-op.
    #[test]
    fn frameless_style_strips_every_native_chrome_bit() {
        assert_eq!(WS_CAPTION, 0x00C0_0000);
        assert_eq!(WS_SYSMENU, 0x0008_0000);
        assert_eq!(WS_THICKFRAME, 0x0004_0000);
        assert_eq!(WS_BORDER, 0x0080_0000);
        assert_eq!(WS_DLGFRAME, 0x0040_0000);
        assert_eq!(WS_POPUP, 0x8000_0000);
        assert_eq!(NATIVE_CHROME_BITS, 0x00CC_0000);

        // WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX |
        // WS_MAXIMIZEBOX | WS_CLIPSIBLINGS | WS_CLIPCHILDREN: what a decorated
        // winit window reports (and what a frameless one kept before the fix).
        let winit_caption = 0x06CF_0000;
        assert!(has_native_chrome(winit_caption));
        let stripped = frameless_window_style(winit_caption);
        assert_eq!(
            stripped & NATIVE_CHROME_BITS,
            0,
            "no native caption, system menu, sizing frame, border or dialog frame remains ({stripped:#010X})"
        );
        assert_eq!(
            stripped & WS_POPUP,
            WS_POPUP,
            "the window becomes a borderless popup ({stripped:#010X})"
        );
        assert_ne!(
            stripped & 0x0002_0000,
            0,
            "the min/max box bits are unrelated to the caption and stay"
        );
        assert_eq!(
            frameless_window_style(stripped),
            stripped,
            "the rewrite is idempotent, so a re-dress changes nothing"
        );
        assert_eq!(
            frameless_window_style(0),
            WS_POPUP,
            "a window with no style at all still becomes a frameless popup"
        );
        assert!(!has_native_chrome(stripped));
    }

    /// The per-tick guard's pure predicate: only a style that differs from the
    /// frameless rewrite is repaired, so the steady tick writes nothing.
    #[test]
    fn only_drifted_styles_need_a_repair() {
        assert!(needs_frameless_repair(0x06CF_0000));
        assert!(needs_frameless_repair(WS_CAPTION));
        assert!(needs_frameless_repair(WS_SYSMENU));
        assert!(needs_frameless_repair(WS_THICKFRAME));
        assert!(needs_frameless_repair(WS_BORDER));
        assert!(needs_frameless_repair(WS_DLGFRAME));
        assert!(
            !needs_frameless_repair(frameless_window_style(0x06CF_0000)),
            "a frame that is already frameless is not written again"
        );
        assert!(!needs_frameless_repair(WS_POPUP));
    }

    /// No combination of the five native-chrome bits survives the rewrite, and
    /// every result is a popup: the "no caption ever" guard at its strongest.
    #[test]
    fn no_combination_of_native_chrome_bits_survives() {
        let bits = [
            WS_CAPTION,
            WS_SYSMENU,
            WS_THICKFRAME,
            WS_BORDER,
            WS_DLGFRAME,
        ];
        for mask in 0u32..32 {
            let mut style = 0x0002_0000_u32;
            for (index, bit) in bits.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    style |= bit;
                }
            }
            assert!(
                has_native_chrome(style) == (mask != 0),
                "the test mask {mask:#07b} names real chrome bits"
            );
            let frameless = frameless_window_style(style);
            assert!(
                !has_native_chrome(frameless),
                "no native chrome survives mask {mask:#07b} ({frameless:#010X})"
            );
            assert_eq!(
                frameless & WS_POPUP,
                WS_POPUP,
                "the rewrite is always a borderless popup (mask {mask:#07b})"
            );
            assert!(
                !needs_frameless_repair(frameless),
                "a repaired style is never rewritten again (mask {mask:#07b})"
            );
        }
    }

    /// `GWL_STYLE` is 32 bits even though the read is `LONG_PTR`; a style with
    /// `WS_POPUP` can arrive sign-extended. The mask must recover exactly the
    /// low 32 bits, so the guard sees a stable style (not a negative value that
    /// compares unequal every tick).
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_style_read_masks_the_pointer_width_sign_extension() {
        let popup = 0x8603_0000_u32;
        let raw = i32::from_ne_bytes(popup.to_ne_bytes());
        assert_eq!(window_style_bits(isize::try_from(raw).unwrap_or(0)), popup);
        assert_eq!(window_style_bits(-1), u32::MAX);
        assert_eq!(window_style_bits(isize::MIN), 0);
        // The 64-bit sign-extended form of the popup style.
        let sign_extended = i64::from(i32::from_ne_bytes(popup.to_ne_bytes()));
        assert_eq!(
            window_style_bits(isize::try_from(sign_extended).unwrap_or(0)),
            popup
        );
    }

    /// The `OC_WINDOW_TRACE` line names the bits before and after the re-apply,
    /// so a user can tell a no-op re-dress from a repair at a glance.
    #[test]
    fn the_window_trace_prints_the_style_bits() {
        let changed = window_style_trace(0x06CF_0000, 0x8603_0000, true);
        assert!(changed.contains("0x06CF0000"), "{changed}");
        assert!(changed.contains("0x86030000"), "{changed}");
        assert!(changed.contains("native chrome no"), "{changed}");
        assert!(changed.contains("popup yes"), "{changed}");
        assert!(changed.contains("applied"), "{changed}");

        let unchanged = window_style_trace(0x8603_0000, 0x8603_0000, false);
        assert!(unchanged.contains("unchanged"), "{unchanged}");
    }

    /// Blur 0 is "no backdrop"; every level above it uses the acrylic accent,
    /// the only material that does not need an extended DWM frame.
    #[test]
    fn blur_selects_accent_or_none() {
        assert_eq!(BackdropMaterial::for_blur(0), BackdropMaterial::None);
        assert_eq!(BackdropMaterial::for_blur(1), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::for_blur(34), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::for_blur(69), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::for_blur(70), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::for_blur(100), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::for_blur(250), BackdropMaterial::Acrylic);
        assert_eq!(BackdropMaterial::None.as_str(), "none");
        assert_eq!(BackdropMaterial::Acrylic.as_str(), "acrylic");
    }

    /// The diagnostic line names the material and states that the frame was
    /// never extended: the default path cannot bring the native caption strip
    /// back.
    #[test]
    fn the_backdrop_trace_names_acrylic_and_no_extension() {
        let line = backdrop_trace(BackdropMaterial::Acrylic, true);
        assert!(line.contains("material acrylic"), "{line}");
        assert!(line.contains("frame-extended no"), "{line}");
        assert!(line.contains("strategy accent"), "{line}");
        assert!(line.contains("accepted yes"), "{line}");

        let refused = backdrop_trace(BackdropMaterial::Acrylic, false);
        assert!(refused.contains("accepted no"), "{refused}");
        let off = backdrop_trace(BackdropMaterial::None, false);
        assert!(off.contains("material none"), "{off}");
    }
}
