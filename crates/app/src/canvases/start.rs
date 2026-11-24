//! A text started from the keyboard (`new-text`, slinger-b16804): where it
//! starts is [`super::editing`]'s (`start`); this keeps it from an entry
//! or a page command's question that has the keys.

use super::Canvases;
use crate::commands::Keymap;
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;

impl Canvases {
    /// Only `SLINGER_NVIM` turns Nvim on in these canvases, not the config:
    /// a driven window's, which types the same on any machine.
    #[must_use]
    pub fn driven(mut self) -> Self {
        self.editing.driven();
        self
    }

    /// Follows the config's `[editor] nvim`: which Nvim edits a text.
    pub fn follow_editor(&mut self, nvim: &crate::config::Nvim) {
        self.editing.follow_editor(nvim);
    }

    /// Follows `keys`: the hint under a typed text names `new-text`'s key.
    pub fn follow_keys(&mut self, keys: &Keymap) {
        self.editing.follow_keys(keys);
    }

    /// Starts a new text in `colour` on the page `pane` of `panes` (laid at
    /// `frame`) shows, or chained under the text being typed on that text's
    /// page; `pointer` is where the pointer is in the window, if known. The
    /// new text is brought into view when the pane shows its page. False,
    /// and nothing starts, while an entry or a page command's question has
    /// the keys, or the pane shows no page.
    pub fn new_text(
        &mut self,
        panes: &Panes,
        at: (PaneId, Rect),
        pointer: Option<Point>,
        colour: Rgb,
    ) -> bool {
        if self.typing() || self.asks.asking() {
            return false;
        }
        self.editing.new_text(&mut self.follow, panes, at, (pointer, colour))
    }
}
