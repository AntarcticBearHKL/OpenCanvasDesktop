//! OS file drop onto the canvas window.
//!
//! Slint's winit backend does not forward `WM_DROPFILES`, so the app installs
//! the classic Windows drop target itself:
//! `DragAcceptFiles(hwnd, TRUE)` plus a window-proc subclass that handles
//! `WM_DROPFILES` (`DragQueryFileW` ??paths, `DragQueryPoint` ??client point).
//! Dropped paths are queued on the UI thread and drained by
//! [`pump`] from the backdrop tick; the app then turns each one into a node at
//! the drop point.
//!
//! The install is idempotent and keyed by HWND, so a recreated window (DPI
//! change, restore from minimise on some drivers, a future multi-window shell)
//! gets a fresh subclass on the next tick instead of silently losing drops.
//!
//! Multiple files keep the web's rule (`use-canvas-insertion.ts` `handleDrop`):
//! **one node per file**, staggered 40px down-right from the drop point ??never
//! one merged stack.
//!
//! Everything here except the Win32 calls is pure and tested: extension
//! classification, duplicate/extension filtering and the client→world mapping.

use std::sync::Mutex;

use slint::ComponentHandle;

// The window-free half (extension classification, drop-path filtering, the
// client -> world mapping) is `open-canvas-app-core`'s `drop_files`: same names,
// re-exported so every existing `crate::file_drop::…` path still resolves.
pub use open_canvas_app_core::drop_files::{
    accepted_paths, classify, file_title, mime_for, world_at, DropKind, DroppedFile, DROP_STAGGER,
};

/// Installs the drop target on `hwnd` and drains the queue into the app.
///
/// Idempotent per HWND; called from the 8ms frame tick, which keeps running
/// for the whole life of the window. A failure to subclass is harmless: the
/// next tick retries.
#[cfg(windows)]
pub fn pump(ui: &crate::AppWindow, app: &std::rc::Rc<std::cell::RefCell<crate::app::App>>) {
    let Some(hwnd) = hwnd_of(ui.window()) else {
        return;
    };
    win::install(hwnd);
    let files = win::take_dropped();
    if files.is_empty() {
        return;
    }
    crate::app::with_app(app, |app| app.apply_file_drop(ui, &files));
}

#[cfg(not(windows))]
pub fn pump(_ui: &crate::AppWindow, _app: &std::rc::Rc<std::cell::RefCell<crate::app::App>>) {}

/// The window's native handle (Windows only).
#[cfg(windows)]
pub fn hwnd_of(window: &slint::Window) -> Option<isize> {
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

#[cfg(not(windows))]
pub fn hwnd_of(_window: &slint::Window) -> Option<isize> {
    None
}

/// Asks the OS for a folder (the assets node's "Bind folder"). `None` when the
/// user cancels or the dialog cannot be shown.
#[cfg(windows)]
pub fn pick_folder(owner: isize) -> Option<String> {
    win::pick_folder(owner)
}

#[cfg(not(windows))]
pub fn pick_folder(_owner: isize) -> Option<String> {
    None
}

/// The queue the window proc pushes into; drained on the UI thread. A poisoned
/// lock keeps the last known paths (a drop must never panic the app).
#[cfg(windows)]
static DROPPED: Mutex<Vec<DroppedFile>> = Mutex::new(Vec::new());

#[cfg(windows)]
fn push_dropped(files: Vec<DroppedFile>) {
    if let Ok(mut queue) = DROPPED.lock() {
        queue.extend(files);
    }
}

#[cfg(windows)]
mod win {
    use super::{push_dropped, DroppedFile};
    use std::sync::atomic::{AtomicIsize, Ordering};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct WinPoint {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct BrowseInfoW {
        owner: isize,
        root: *mut core::ffi::c_void,
        display_name: *mut u16,
        title: *const u16,
        flags: u32,
        callback: *mut core::ffi::c_void,
        param: isize,
        image: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn DragAcceptFiles(hwnd: isize, accept: i32);
        fn DragQueryFileW(hdrop: isize, index: u32, file: *mut u16, length: u32) -> u32;
        fn DragQueryPoint(hdrop: isize, point: *mut WinPoint) -> i32;
        fn DragFinish(hdrop: isize);
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn CallWindowProcW(
            previous: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
            hwnd: isize,
            message: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
        fn DefWindowProcW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHBrowseForFolderW(info: *const BrowseInfoW) -> *mut core::ffi::c_void;
        fn SHGetPathFromIDListW(list: *const core::ffi::c_void, path: *mut u16) -> i32;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(pointer: *mut core::ffi::c_void);
    }

    const WM_DROPFILES: u32 = 0x0233;
    const GWLP_WNDPROC: i32 = -4;
    const MAX_PATH_W: usize = 32_768;
    /// `BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE`.
    const BROWSE_FLAGS: u32 = 0x0000_0001 | 0x0000_0040;

    static LAST_HWND: AtomicIsize = AtomicIsize::new(0);
    static OLD_PROC: AtomicIsize = AtomicIsize::new(0);

    /// `WM_DROPFILES` handler. Reads every path, reads the drop point once, and
    /// finishes the drop; then forwards everything else to the previous proc.
    unsafe extern "system" fn drop_proc(
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if message == WM_DROPFILES {
            let hdrop = wparam.cast_signed();
            let count = unsafe { DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0) };
            let mut point = WinPoint::default();
            // A failed point read still delivers the files at (0, 0): the app
            // creates the nodes at the canvas origin rather than dropping them.
            unsafe {
                DragQueryPoint(hdrop, std::ptr::from_mut(&mut point));
            }
            let client = (f64::from(point.x), f64::from(point.y));
            let mut files = Vec::with_capacity(count as usize);
            for index in 0..count {
                let length = unsafe { DragQueryFileW(hdrop, index, std::ptr::null_mut(), 0) };
                if length == 0 {
                    continue;
                }
                let mut buffer = vec![0u16; length as usize + 1];
                let written = unsafe {
                    DragQueryFileW(
                        hdrop,
                        index,
                        buffer.as_mut_ptr(),
                        u32::try_from(buffer.len()).unwrap_or(0),
                    )
                };
                buffer.truncate(written as usize);
                if let Ok(path) = String::from_utf16(&buffer) {
                    files.push(DroppedFile { path, client });
                }
            }
            unsafe {
                DragFinish(hdrop);
            }
            push_dropped(files);
            return 0;
        }
        let previous = OLD_PROC.load(Ordering::Relaxed);
        if previous == 0 {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        let proc = unsafe {
            std::mem::transmute::<
                isize,
                Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
            >(previous)
        };
        unsafe { CallWindowProcW(proc, hwnd, message, wparam, lparam) }
    }

    /// Registers `hwnd` as a file drop target and subclasses its window proc.
    /// Returns whether the window is registered (a repeated call with the same
    /// HWND is a cheap no-op).
    pub fn install(hwnd: isize) -> bool {
        if LAST_HWND.load(Ordering::Relaxed) == hwnd {
            return true;
        }
        unsafe {
            DragAcceptFiles(hwnd, 1);
        }
        let previous =
            unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, drop_proc as *const () as isize) };
        if previous == 0 {
            return false;
        }
        OLD_PROC.store(previous, Ordering::Relaxed);
        LAST_HWND.store(hwnd, Ordering::Relaxed);
        true
    }

    pub fn take_dropped() -> Vec<DroppedFile> {
        match super::DROPPED.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    /// The classic shell folder picker. Blocking by design: the app's backdrop
    /// tick keeps running on the event loop, and the assets node only needs the
    /// path once.
    pub fn pick_folder(owner: isize) -> Option<String> {
        let mut display = vec![0u16; MAX_PATH_W].into_boxed_slice();
        let info = BrowseInfoW {
            owner,
            root: std::ptr::null_mut(),
            display_name: display.as_mut_ptr(),
            title: std::ptr::null(),
            flags: BROWSE_FLAGS,
            callback: std::ptr::null_mut(),
            param: 0,
            image: 0,
        };
        let list = unsafe { SHBrowseForFolderW(std::ptr::from_ref(&info)) };
        if list.is_null() {
            return None;
        }
        let mut path = vec![0u16; MAX_PATH_W].into_boxed_slice();
        let ok = unsafe { SHGetPathFromIDListW(list, path.as_mut_ptr()) } != 0;
        unsafe {
            CoTaskMemFree(list);
        }
        if !ok {
            return None;
        }
        let end = path.iter().position(|unit| *unit == 0).unwrap_or(0);
        String::from_utf16(&path[..end]).ok()
    }
}

/// Applying a drop: one image/video node per accepted file, at the drop point
/// (staggered 40px for a multi-file drop), with the media already stored.
impl crate::app::App {
    pub fn apply_file_drop(&mut self, ui: &crate::AppWindow, files: &[DroppedFile]) {
        if self.view != crate::app::View::Editor {
            return;
        }
        let scale = f64::from(ui.window().scale_factor());
        let canvas_origin = (
            if ui.get_side_panel_open() {
                f64::from(ui.get_side_panel_width())
            } else {
                0.0
            },
            // The Canvas view is edge-to-edge now: the canvas column starts at
            // the window's top edge (the floating chrome is an overlay).
            0.0,
        );
        let viewport = self.doc.viewport();
        let now = self.now_ms();
        // Extension filter + case-insensitive dedupe first (web's rule: one
        // node per distinct accepted file).
        let accepted: std::collections::HashSet<String> =
            accepted_paths(files.iter().map(|file| file.path.clone()))
                .into_iter()
                .collect();
        let mut created = 0usize;
        let mut rejected = 0usize;
        for (index, file) in files.iter().enumerate() {
            if classify(&file.path).is_none() {
                rejected += 1;
                continue;
            }
            if !accepted.contains(&file.path) {
                continue;
            }
            let drop = world_at(file.client, scale, canvas_origin, viewport);
            let stagger = DROP_STAGGER * f64::from(u32::try_from(index).unwrap_or(0));
            let at = crate::coords::WorldPos::new(drop.x + stagger, drop.y + stagger);
            if self.insert_media_node(&file.path, at, now).is_some() {
                created += 1;
            } else {
                rejected += 1;
            }
        }
        if created > 0 {
            self.note_doc_change();
            self.sync_all(ui);
        } else if rejected > 0 {
            self.set_notice(
                ui,
                "Drop an image or video file onto the canvas".to_string(),
                true,
            );
        }
    }

    /// Creates one image/video node for `path` at `at`, with the media stored
    /// (images inline as a data URL, videos as the `file:` reference the
    /// reference pipeline already reads). `None` for an unreadable/too-large
    /// file or a non-media extension; otherwise the new node id.
    pub(crate) fn insert_media_node(
        &mut self,
        path: &str,
        at: crate::coords::WorldPos,
        now: u64,
    ) -> Option<String> {
        let kind = classify(path)?;
        let content = match kind {
            DropKind::Image => image_data_url(path)?,
            DropKind::Video => format!("file:{path}"),
        };
        let dimensions = match kind {
            DropKind::Image => {
                crate::media::decode_pixels(&content).map(|pixels| (pixels.width, pixels.height))
            }
            DropKind::Video => None,
        };
        Some(self.create_media_node(kind, path, content, dimensions, at, now))
    }

    fn create_media_node(
        &mut self,
        kind: DropKind,
        path: &str,
        content: String,
        dimensions: Option<(u32, u32)>,
        at: crate::coords::WorldPos,
        now: u64,
    ) -> String {
        use open_canvas_core::constants::get_node_spec;
        use open_canvas_core::geometry::fit_node_size;
        use open_canvas_core::node::{CanvasNodeStatus, CanvasNodeType};

        let node_type = match kind {
            DropKind::Image => CanvasNodeType::Image,
            DropKind::Video => CanvasNodeType::Video,
        };
        let mut spec = get_node_spec(&node_type);
        spec.title = file_title(path);
        spec.metadata.content = Some(content);
        spec.metadata.mime_type = Some(mime_for(path).to_string());
        spec.metadata.status = Some(CanvasNodeStatus::Success);
        if let Some((width, height)) = dimensions {
            spec.metadata.natural_width = Some(f64::from(width));
            spec.metadata.natural_height = Some(f64::from(height));
            // A dropped image node starts at its bitmap ratio; later resizes
            // stay locked to it as well.
            let (node_width, node_height) =
                fit_node_size(f64::from(width), f64::from(height), spec.width, spec.height);
            spec.width = node_width;
            spec.height = node_height;
        }
        self.doc.create_node_with(node_type, at, spec, now)
    }
}

/// A dropped image's bytes as a `data:` URL. The MIME type is sniffed from the
/// bytes when the `image` crate recognizes the format (so a mislabeled `.jpg`
/// still carries the format it really is), and falls back to the extension
/// otherwise. GIF/TIFF stay in their own container: the renderer decodes the
/// first frame as a still image. `None` above [`MAX_DROPPED_IMAGE_BYTES`]: the
/// media text cap would reject it later anyway, so the drop reports it.
pub(crate) fn image_data_url(path: &str) -> Option<String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;

    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_DROPPED_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{};base64,{}",
        sniffed_image_mime(&bytes, path),
        BASE64.encode(bytes)
    ))
}

/// The MIME type of `bytes` as `image` recognizes them; the extension is only
/// used when the sniff fails (a corrupt file keeps the label it was dropped
/// with and renders the node's empty state instead of vanishing).
fn sniffed_image_mime(bytes: &[u8], path: &str) -> &'static str {
    use image::ImageFormat;
    match image::guess_format(bytes) {
        Ok(ImageFormat::Png) => "image/png",
        Ok(ImageFormat::Jpeg) => "image/jpeg",
        Ok(ImageFormat::Gif) => "image/gif",
        Ok(ImageFormat::WebP) => "image/webp",
        Ok(ImageFormat::Bmp) => "image/bmp",
        Ok(ImageFormat::Tiff) => "image/tiff",
        Ok(ImageFormat::Ico) => "image/x-icon",
        Ok(ImageFormat::Tga) => "image/x-tga",
        _ => mime_for(path),
    }
}

/// Raw bytes accepted for an embedded dropped image (base64 inflates by 4/3,
/// and the media cache's text cap is 12 MiB).
pub const MAX_DROPPED_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
