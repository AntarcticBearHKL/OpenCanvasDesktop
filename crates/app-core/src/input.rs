//! Event payloads forwarded from the Slint canvas layers.

use open_canvas_core::node::ConnectionHandleType;

use crate::coords::ScreenPos;

/// Pointer event kinds (mirrors `PointerEventKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

impl PointerKind {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "down" => Some(Self::Down),
            "move" => Some(Self::Move),
            "up" => Some(Self::Up),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// Pointer buttons we act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Other,
}

impl Button {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "left" => Some(Self::Left),
            "middle" => Some(Self::Middle),
            "right" => Some(Self::Right),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// A pointer event forwarded from a canvas layer.
#[derive(Debug, Clone, Copy)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub pos: ScreenPos,
    pub button: Button,
    pub shift: bool,
    pub ctrl: bool,
}

/// Parses the connection-handle side forwarded by the node handles layer.
pub fn handle_type(id: &str) -> Option<ConnectionHandleType> {
    match id {
        "source" => Some(ConnectionHandleType::Source),
        "target" => Some(ConnectionHandleType::Target),
        _ => None,
    }
}
