//! What the keys do on the canvas panes: typed and pasted text go to a
//! page command's question or the text being typed, the caret keys to the
//! text being typed, and delete and escape
//! act on the selection from any pane that shows its page; a colour picked
//! recolours it. Select all, next and previous item, nudge, duplicate and
//! fold act on the focused pane's page ([`Keyed`]).

use super::{Canvases, canvas_of, find, pane_shown, wanted};
use crate::commands::{CMD, Chord};
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::mirror::Mirror;
use crate::panes::Panes;
use crate::scene::select::{Keyed, Selection, into_view};

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
            self.seek_sought();
            return true;
        }
        let used = self.shapes.typed(text, &mut self.ink);
        if used {
            self.send();
        }
        used
    }

    /// A key of the text being typed: its caret, selection and delete keys
    /// ([`crate::scene::editor`]), which come before the keymap. False when
    /// no canvas text is being typed (or the sling's entry, or a page
    /// command's question, has the keys), or the key is not one of them.
    pub fn edit_key(&mut self, chord: Chord) -> bool {
        let used = !self.typing() && !self.asks.asking() && self.shapes.key(chord);
        if used {
            self.send();
        }
        used
    }

    /// Selects every character of the text being typed. False when none
    /// is ([`Canvases::edit_key`]).
    pub fn select_text(&mut self) -> bool {
        self.edit_key(Chord::new(CMD, 'a'))
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

    /// Makes the selection's strokes, texts, boxes and arrows `colour`, in
    /// one step ([`crate::scene::select::Select::recolour`]). Returns false
    /// when there is no selection, or none of it changes.
    pub fn recolour(&mut self, colour: Rgb) -> bool {
        let Some(s) = self.select.selection() else { return false };
        let open = find(&self.open, &s.canvas, Some(&s.page)).and_then(|o| o.mirror.as_ref());
        let Some(mirror) = open else { return false };
        let items = self.ink.shown(&s.canvas, &s.page, mirror.items());
        self.select.recolour(colour, &items, &mut self.ink) && {
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

    /// Does `keyed` on the page `pane` of `panes` (laid at `frame`) shows:
    /// select all; step to the next or previous item, panning it into
    /// view; nudge or duplicate the selection when the pane shows it.
    /// False, and the key goes on, when there is nothing to act on.
    pub fn keyed(&mut self, panes: &Panes, (pane, frame): (PaneId, Rect), keyed: Keyed) -> bool {
        let Some(canvas) = canvas_of(panes, pane) else { return false };
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let Some(on_page) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string())
        else {
            return false;
        };
        let viewport = self.view(pane).viewport;
        let shown = self.shows_selection(panes, pane);
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        let here = (canvas, on_page.as_str());
        let zoom = viewport.zoom();
        match keyed {
            Keyed::All => self.select.all(here, &items),
            Keyed::Step(forward) => {
                let Some(item) = self.select.step(here, &items, forward) else { return false };
                drop(items);
                let size = Size { width: frame.width, height: frame.height };
                if let Some((dx, dy)) = into_view(viewport, size, item) {
                    self.pan(pane, dx, dy);
                }
                true
            }
            Keyed::Nudge(dx, dy) => {
                let by = Point { x: dx / zoom, y: dy / zoom };
                let moved = shown && self.select.nudge(by, &items, &mut self.ink);
                drop(items);
                moved && {
                    self.send();
                    true
                }
            }
            Keyed::Duplicate => {
                let Some(at) = self.select.duplicate_at(&items, zoom).filter(|_| shown) else {
                    return false;
                };
                drop(items);
                let ink = (canvas, &self.ink);
                let ids = self.selection().map(|s| s.ids.clone()).unwrap_or_default();
                let open = find(&self.open, canvas, Some(&on_page)).and_then(|o| o.mirror.as_ref());
                let Some(items) = open.and_then(|m| m.copied(ink, &ids)?.items) else {
                    return false;
                };
                self.place((canvas.to_string(), on_page), &items, at)
            }
            Keyed::Fold => {
                drop(items);
                shown && self.fold_selection(pane, (canvas, &on_page))
            }
        }
    }
}
