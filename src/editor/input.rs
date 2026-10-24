//! Platform-agnostic input events, so the editor can be driven by winit or by tests.

use crate::geom::V2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd on macOS, the Windows/Super key elsewhere.
    pub logo: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        logo: false,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
        logo: false,
    };
    /// The platform command modifier: Cmd on macOS, Ctrl elsewhere.
    pub fn command(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.logo
        } else {
            self.ctrl
        }
    }
    pub fn cmd() -> Modifiers {
        if cfg!(target_os = "macos") {
            Modifiers {
                logo: true,
                ..Modifiers::NONE
            }
        } else {
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            }
        }
    }
    pub fn cmd_shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Modifiers::cmd()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Primary,
    Middle,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Delete,
    Tab,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// Which mouse cursor to show while over the canvas, derived from the hovered
/// resize handle (see [`super::Editor::hover_cursor`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    /// Diagonal ⤢: a corner handle that resizes along the nw–se axis.
    NwSe,
    /// Diagonal ⤡: a corner handle that resizes along the ne–sw axis.
    NeSw,
    /// ⟷: a top/bottom side handle that resizes the width.
    Ew,
    /// ↕: a left/right side handle that resizes the height.
    Ns,
}

impl CursorShape {
    /// The shape for a resize handle, from its position as a fraction of the
    /// selection bounds (one of [`super::hit::HANDLES`]).
    pub fn resize((hx, hy): (f64, f64)) -> CursorShape {
        let mid = |v: f64| v == 0.5;
        match (mid(hx), mid(hy)) {
            // A mid handle on the top/bottom edges changes the height (↕).
            (true, _) => CursorShape::Ns,
            // A mid handle on the left/right edges changes the width (⟷).
            (_, true) => CursorShape::Ew,
            // True corners: nw-se handles have hx == hy, ne-sw the opposite.
            _ if hx == hy => CursorShape::NwSe,
            _ => CursorShape::NeSw,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// Screen position in logical pixels relative to the canvas viewport.
    PointerDown {
        pos: V2,
        button: Button,
        pressure: Option<f64>,
        time_ms: f64,
    },
    PointerMove {
        pos: V2,
        pressure: Option<f64>,
        time_ms: f64,
    },
    PointerUp {
        pos: V2,
        button: Button,
        time_ms: f64,
    },
    /// Scroll in screen pixels (positive y = content moves up).
    Scroll {
        delta: V2,
        pos: V2,
    },
    /// Multiplicative zoom about a point (trackpad pinch).
    Zoom {
        factor: f64,
        pos: V2,
    },
    KeyDown {
        key: Key,
        repeat: bool,
    },
    KeyUp {
        key: Key,
    },
    /// Committed text input (IME/char), separate from key presses.
    Text(String),
    Modifiers(Modifiers),
}
