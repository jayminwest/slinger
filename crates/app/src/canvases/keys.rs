//! What the keys do on the canvas panes: typed and pasted text go to a
//! page command's question or the text being typed, and delete and escape
//! act on the selection from any pane that shows its page.

use super::{Canvases, canvas_of, find, wanted};
use crate::layout::PaneId;
use crate::mirror::Mirror;
use crate::panes::Panes;
use crate::scene::select::Selection;

impl Canvases {
    /// Whether a text is being typed, or a page command asks: the keys
    /// that type go to it.
    pub fn editing(&self) -> bool {
        self.shapes.editing() || self.asks.asking()
    }

    /// Typed `text` answers a page command's question, or goes into the
    /// text being typed; escape ends it, and the text goes to the daemon.
    /// Returns false when neither is open.
    pub fn typed(&mut self, text: &str) -> bool {
        // The sling's open entry has the keys: its text is not the canvas's.
        if self.typing() {
            return false;
        }
        if self.asks.asking() {
            if let Some((canvas, op)) = self.asks.typed(text) {
                self.change_pages(canvas, op);
            }
            return true;
        }
        let used = self.shapes.typed(text, &mut self.ink);
        if used {
            self.send();
        }
        used
    }

    /// Pasted `text` goes where typed text goes, as text, not keys. False
    /// when nothing takes it (the sling's entry pastes for itself).
    pub fn pasted(&mut self, text: &str) -> bool {
        !self.typing() && (self.asks.pasted(text) || self.shapes.pasted(text))
    }

    /// The selection, if any: one page's items, by the server's ids.
    pub fn selection(&self) -> Option<&Selection> {
        self.select.selection()
    }

    /// Whether `pane` of `panes` shows the selection's page.
    pub(super) fn shows_selection(&self, panes: &Panes, pane: PaneId) -> bool {
        let Some(s) = self.selection() else { return false };
        let asked = wanted(&self.views, pane, &s.canvas);
        let open = find(&self.open, &s.canvas, asked).and_then(|o| o.mirror.as_ref());
        let showing = asked.or(open.map(Mirror::page));
        canvas_of(panes, pane) == Some(&s.canvas) && showing == Some(&s.page)
    }

    /// Removes the selection, in one step, when `pane` of `panes` shows its
    /// page. Returns false when it does not, or there is none.
    pub fn delete_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.select.delete(&mut self.ink) && {
            self.send();
            true
        }
    }

    /// Clears the selection when `pane` of `panes` shows its page.
    /// Returns false when it does not, or there is none.
    pub fn clear_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.select.clear()
    }
}
