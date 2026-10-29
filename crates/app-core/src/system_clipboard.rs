//! System-clipboard bridge for node copy/paste.
//!
//! The web falls back to the real clipboard when the internal register is empty
//! (project.tsx:1229-1232); the desktop mirrors that with the `arboard` crate
//! that Slint's winit backend already pulls in. Every call is best-effort:
//! clipboard access can fail (locked clipboard, no window station) and must
//! never surface as a panic or a blocked UI.

/// Writes plain text to the system clipboard; `false` when unavailable.
pub fn write_text(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .is_ok()
}

/// Reads plain text from the system clipboard; `None` when unavailable or the
/// clipboard is not text.
pub fn read_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// A bitmap read from the system clipboard, as straight RGBA8.
#[derive(Debug, Clone)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Reads an image from the system clipboard; `None` when unavailable or the
/// clipboard does not contain a bitmap.
pub fn read_image() -> Option<ClipboardImage> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;
    Some(ClipboardImage {
        width: image.width,
        height: image.height,
        rgba: image.bytes.into_owned(),
    })
}
