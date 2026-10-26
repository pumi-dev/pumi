pub mod graphics;
pub mod grid;
pub mod images;
pub mod keys;
pub mod local;
pub mod screens;
pub mod scroll;
pub mod selection;
pub mod session;
pub mod vt;

pub use grid::{CursorShape, Grid};
pub use session::{Size, TerminalSession};

/// The cursor a program gets until it asks for another (Settings' Cursor style and Blink), which
/// every terminal's emulator follows: Ghostty's `cursor-style` and `cursor-style-blink`.
pub mod cursor_defaults {
    use super::CursorShape;
    use std::sync::atomic::{AtomicU32, Ordering};
    /// Bits 0–7 the shape, 8–9 the blink (0 the emulator's own, 1 off, 2 on), 10 up a generation.
    static PACKED: AtomicU32 = AtomicU32::new(0);
    fn shape_code(shape: CursorShape) -> u32 {
        match shape {
            CursorShape::Block => 0,
            CursorShape::Bar => 1,
            CursorShape::Underline => 2,
            CursorShape::Hollow => 3,
        }
    }
    /// Sets them; terminals pick them up as they next draw.
    pub fn set(shape: CursorShape, blink: Option<bool>) {
        let blink = match blink {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        let now = PACKED.load(Ordering::Acquire);
        let wanted = shape_code(shape) | (blink << 8);
        if now & 0x3ff != wanted {
            PACKED.store((((now >> 10) + 1) << 10) | wanted, Ordering::Release);
        }
    }
    /// The generation, the shape and the blink.
    pub fn get() -> (u32, CursorShape, Option<bool>) {
        let packed = PACKED.load(Ordering::Acquire);
        let shape = match packed & 0xff {
            1 => CursorShape::Bar,
            2 => CursorShape::Underline,
            3 => CursorShape::Hollow,
            _ => CursorShape::Block,
        };
        let blink = match (packed >> 8) & 0b11 {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        };
        (packed >> 10, shape, blink)
    }
}
