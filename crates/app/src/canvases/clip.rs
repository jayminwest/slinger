//! Cut, copy and paste on the canvas panes, when no text field takes them
//! ([`crate::scene::paste`]):
//!
//! - Copy puts the selection's items on the clipboard, as their file's
//!   objects and the plain text of its texts ([`crate::mirror::clip`]), when
//!   the focused pane shows its page; cut then removes them, in one step. A
//!   text being typed has its caret at its end and no selection inside it,
//!   so it copies nothing: Edit > Copy and Cut grey out while one is.
//! - Paste puts the clipboard's text where typed text goes (a text being
//!   typed, a page command's question); else its items on the focused
//!   pane's page, centred in the pane, in one step, and they are the
//!   selection once there. Plain text alone (from another app) goes there
//!   as one new text item of the pen's colour. With no page, Edit > Paste
//!   greys out.

use super::{Canvases, canvas_of, find, wanted};
use crate::config::Rgb;
use crate::layout::{Point, Rect, Viewport};
use crate::mirror::{Mirror, clip};
use crate::panes::Panes;
use crate::scene::ink::Change;
use crate::scene::paste::Clip;

impl Canvases {
    /// Whether copy and cut have something to take: a selection the
    /// focused pane of `panes` shows, with no text being typed.
    pub fn can_copy(&self, panes: &Panes) -> bool {
        !self.typing() && !self.editing() && self.shows_selection(panes, panes.focus())
    }

    /// The selection as the clipboard takes it ([`Canvases::can_copy`]);
    /// with `cut`, it is then removed in one step. `None` when there is
    /// nothing to take.
    pub fn copy(&mut self, panes: &Panes, cut: bool) -> Option<Clip> {
        if !self.can_copy(panes) {
            return None;
        }
        let selection = self.selection()?;
        let open = find(&self.open, &selection.canvas, Some(&selection.page));
        let clip = open.and_then(|o| o.mirror.as_ref())?.copied(&selection.ids)?;
        if cut {
            self.delete_selection(panes, panes.focus());
        }
        Some(clip)
    }

    /// Whether a paste of `clip` has somewhere to go on the focused pane of
    /// `panes`: its text where typed text goes, or its items (or its text,
    /// as a new text item) on the page.
    pub fn can_paste(&self, clip: &Clip, panes: &Panes) -> bool {
        let page = || self.focused_page(panes).is_some();
        !self.typing() && if self.editing() { clip.text.is_some() } else { page() }
    }

    /// Pastes `clip` as the module says, on the focused pane of `panes`
    /// laid in `area`, a new text in `colour`. False when nothing took it.
    pub fn paste(&mut self, clip: &Clip, panes: &Panes, area: Rect, colour: Rgb) -> bool {
        if self.typing() || self.editing() {
            return clip.text.as_deref().is_some_and(|text| self.pasted(text));
        }
        let text = || clip.text.as_deref().and_then(|text| clip::text_items(text, colour));
        let Some(items) = clip.items.clone().or_else(text) else { return false };
        let Some((canvas, mirror)) = self.focused_page(panes) else { return false };
        let focus = panes.focus();
        let placed = panes.place(area).into_iter().find(|(id, _, _)| *id == focus);
        let Some(frame) = placed.map(|(_, _, frame)| crate::scene::status::content(frame)) else {
            return false;
        };
        let view = self.views.iter().find(|(p, _)| *p == focus);
        let viewport = view.map_or_else(Viewport::start, |(_, v)| v.viewport);
        let centre = viewport.to_canvas(Point { x: frame.width / 2.0, y: frame.height / 2.0 });
        let page = mirror.page().to_string();
        let items = clip::pasted(&items, &page, centre);
        if items.is_empty() {
            return false;
        }
        let canvas = canvas.to_string();
        let change =
            Change::Carry { canvas: canvas.clone(), page: page.clone(), items, gone: vec![] };
        let key = self.ink.push_keyed(change);
        self.select.pasted(key, &canvas, &page);
        self.send();
        true
    }

    /// The canvas the focused pane of `panes` shows, and its page, once it
    /// has arrived.
    fn focused_page(&self, panes: &Panes) -> Option<(&str, &Mirror)> {
        let pane = panes.focus();
        let canvas = canvas_of(panes, pane)?;
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas))?;
        Some((open.canvas.as_str(), open.mirror.as_ref()?))
    }
}
