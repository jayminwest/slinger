//! What the keys do on the canvas panes: typed and pasted text go to a
//! page command's question or the text being typed, the caret keys to the
//! text being typed, and delete and escape
//! act on the selection from any pane that shows its page; a colour picked
//! recolours it, and a size command resizes the text being typed or the
//! selected texts ([`Canvases::resize`]). Select all, next and previous
//! item, nudge, duplicate and fold act on the focused pane's page ([`Keyed`]).

use super::Canvases;
use crate::commands::{CMD, Chord, Command, Keymap};
use crate::config::Rgb;
use crate::layout::{PaneId, Rect};
use crate::panes::Panes;
use crate::scene::select::{Keyed, Selection};
use crate::scene::sling::edits;
use crate::scene::tools::Resize;

impl Canvases {
    /// Whether a text is being typed, or a page command asks: the keys
    /// that type go to it.
    pub fn editing(&self) -> bool {
        self.editing.text_open() || self.asks.asking()
    }

    /// Whether a key no command took is a text's: while the entry is open,
    /// every key a text field uses ([`edits`]) and every key `keys` binds
    /// to nothing, but its `dictate` chord; and a key that types while a
    /// text is being typed, but `size-up`'s and `size-down`'s, which resize
    /// a canvas text being typed (alt+shift+] would type a ’). So cmd+Q
    /// still quits with the entry open.
    pub fn takes(&self, chord: Chord, keys: &Keymap) -> bool {
        let bound = keys.command(chord);
        let field = bound.is_none() || bound != Some(Command::Dictate) && edits(chord);
        let sizes = matches!(bound, Some(Command::SizeUp | Command::SizeDown));
        let types = chord.types() && !(sizes && self.editing.text_open());
        self.typing() && field || self.editing() && types
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
            self.seek_sought();
            return true;
        }
        self.editing.typed(&mut self.follow, text)
    }

    /// A key of the text being typed: its caret, selection and delete keys
    /// ([`crate::scene::editor`]), which come before the keymap. False when
    /// no canvas text is being typed (or the sling's entry, or a page
    /// command's question, has the keys), or the key is not one of them.
    pub fn edit_key(&mut self, chord: Chord) -> bool {
        !self.typing() && !self.asks.asking() && self.editing.key(&mut self.follow, chord)
    }

    /// Selects every character of the text being typed. False when none
    /// is ([`Canvases::edit_key`]).
    pub fn select_text(&mut self) -> bool {
        self.edit_key(Chord::new(CMD, 'a'))
    }

    /// Pasted `text` goes where typed text goes, as text, not keys. False
    /// when nothing takes it (the sling's entry pastes for itself).
    pub fn pasted(&mut self, text: &str) -> bool {
        !self.typing() && (self.asks.pasted(text) || self.editing.pasted(text))
    }

    /// The selection, if any: one page's items, by the server's ids.
    pub fn selection(&self) -> Option<&Selection> {
        self.editing.selection()
    }

    /// Whether `pane` of `panes` shows the selection's page.
    pub fn shows_selection(&self, panes: &Panes, pane: PaneId) -> bool {
        self.editing.shows_selection(&self.follow, panes, pane)
    }

    /// Removes the selection, in one step, when `pane` of `panes` shows its
    /// page. Returns false when it does not, or there is none.
    pub fn delete_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.editing.delete(&mut self.follow)
    }

    /// Makes the selection's strokes, texts, boxes and arrows `colour`, in
    /// one step ([`crate::scene::select::Select::recolour`]). Returns false
    /// when there is no selection, or none of it changes.
    pub fn recolour(&mut self, colour: Rgb) -> bool {
        self.editing.recolour(&mut self.follow, colour)
    }

    /// A size command (`size-up`, `size-down`, a size picked) resizes the
    /// text being typed, else the selected texts, in one step each
    /// ([`Resize`]). False when there is neither: it sets a new text's size.
    pub fn resize(&mut self, resize: Resize) -> bool {
        self.editing.resize(&mut self.follow, resize)
    }

    /// Clears the selection when `pane` of `panes` shows its page.
    /// Returns false when it does not, or there is none.
    pub fn clear_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.editing.deselect()
    }

    /// Does `keyed` on the page `pane` of `panes` (laid at `frame`) shows:
    /// select all; step to the next or previous item, panning it into
    /// view; nudge or duplicate the selection when the pane shows it.
    /// False, and the key goes on, when there is nothing to act on.
    pub fn keyed(&mut self, panes: &Panes, at: (PaneId, Rect), keyed: Keyed) -> bool {
        self.editing.keyed(&mut self.follow, panes, at, keyed)
    }
}
