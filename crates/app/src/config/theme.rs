//! The one theme (SPEC P6): the window's colours, each a `[theme]` key.

use super::Rgb;

/// The one theme (SPEC P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub error: Rgb,
    /// The accent: the focus border (only with two or more panes), the caret,
    /// the selection, the sling field's edge and a working agent's mark.
    pub focus: Rgb,
    /// A done agent's mark: not to be read as working or blocked.
    pub done: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: rgb(0x1e, 0x1e, 0x1e),
            text: rgb(0xe6, 0xe6, 0xe6),
            muted: rgb(0x8c, 0x8c, 0x8c),
            error: rgb(0xff, 0x6b, 0x6b),
            focus: rgb(0x4c, 0x9a, 0xff),
            done: rgb(0x4c, 0xc3, 0x6b),
        }
    }
}

pub(super) const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}
