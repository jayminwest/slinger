//! The emacs keys a Mac text field takes with ctrl (the system's standard
//! key bindings), for the canvas text being typed:
//!
//! | Keys | Does |
//! |---|---|
//! | ctrl+B / ctrl+F | back / forward a character |
//! | ctrl+P / ctrl+N | the row above / below |
//! | ctrl+A / ctrl+E | the line's start / end (a wrapped line's rows are one line) |
//! | ctrl+H / ctrl+D | delete the character before / after |
//! | ctrl+K | cuts to the line's end (at its end, the line end), and keeps it for ctrl+Y |
//! | ctrl+Y | puts back what ctrl+K last cut in this text |
//! | ctrl+T | swaps the characters either side of the caret (at a line's end, the two before it) |
//! | ctrl+O | starts a line after the caret |
//!
//! Shift with a key that moves extends the selection, as with the arrows.
//! What ctrl+K cuts stays in the text, not on the clipboard, as on a Mac.

use super::{Editor, Unit, after, before, line_end};
use crate::commands::{DELETE, DOWN, FORWARD_DELETE, LEFT, RIGHT, UP};

impl Editor {
    /// Ctrl and `key`, for the text laid `size` high in `column`: false when
    /// it is not one of the keys above.
    pub(super) fn emacs(&mut self, key: char, extend: bool, size: f64, column: f64) -> bool {
        let (key, unit) = match key {
            'b' => (LEFT, Unit::Char),
            'f' => (RIGHT, Unit::Char),
            'p' => (UP, Unit::Char),
            'n' => (DOWN, Unit::Char),
            'a' => (LEFT, Unit::Line),
            'e' => (RIGHT, Unit::Line),
            'h' => (DELETE, Unit::Char),
            'd' => (FORWARD_DELETE, Unit::Char),
            'k' => return self.kill(),
            'y' => {
                let killed = self.killed.clone();
                self.insert(&killed);
                return true;
            }
            't' => return self.transpose(),
            'o' => {
                self.insert("\n");
                self.caret -= 1;
                return true;
            }
            _ => return false,
        };
        self.apply(key, unit, extend, size, column)
    }

    /// Cuts the selection, else to the line's end, else the line end, and
    /// keeps it for ctrl+Y.
    fn kill(&mut self) -> bool {
        let end = match line_end(&self.text, self.caret) {
            end if end == self.caret => after(&self.text, end),
            end => end,
        };
        let (a, b) = self.selection().unwrap_or((self.caret, end));
        if a < b {
            self.killed = self.text[a..b].to_string();
            self.replace(a, b, "");
        }
        true
    }

    /// Swaps the characters either side of the caret, or at a line's end
    /// the two before it; the caret goes after them. Nothing with a
    /// selection, or with no two characters of one line there.
    fn transpose(&mut self) -> bool {
        if self.selection().is_some() {
            return true;
        }
        let mut mid = self.caret;
        if line_end(&self.text, mid) == mid {
            mid = before(&self.text, mid);
        }
        let (a, b) = (before(&self.text, mid), after(&self.text, mid));
        if a < mid && mid < b && !self.text[a..b].contains('\n') {
            let swapped = format!("{}{}", &self.text[mid..b], &self.text[a..mid]);
            self.replace(a, b, &swapped);
        }
        true
    }
}
