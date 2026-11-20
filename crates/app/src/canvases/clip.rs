//! Cut, copy and paste on the canvas panes, when no text field takes them
//! ([`crate::scene::paste`]):
//!
//! - Copy puts the characters selected in the text being typed on the
//!   clipboard as plain text; cut then takes them out of it. With none
//!   selected, a text being typed copies nothing: Edit > Copy and Cut grey
//!   out. With no text being typed, copy puts the selection's items on the
//!   clipboard, as their file's objects and the plain text of its texts
//!   ([`crate::mirror::clip`]), when the focused pane shows its page; cut
//!   then removes them, in one step.
//! - Paste puts the clipboard's text where typed text goes (a text being
//!   typed, a page command's question); else its items on the focused
//!   pane's page, centred in the pane, in one step, and they are the
//!   selection once there. Plain text alone (from another app) goes there
//!   as one new text item of the pen's colour. With no page, Edit > Paste
//!   greys out.
//! - The crop tool's lift puts its snippet of a PDF there
//!   ([`Canvases::cropped`]); the canvas does not change.

use super::{Canvases, canvas_of};
use crate::config::Rgb;
use crate::layout::{Point, Rect, Viewport};
use crate::mirror::{Mirror, clip};
use crate::panes::Panes;
use crate::scene::paste::Clip;

impl Canvases {
    /// Whether copy and cut have something to take: characters selected
    /// in the text being typed; with none being typed, a selection the
    /// focused pane of `panes` shows.
    pub fn can_copy(&self, panes: &Panes) -> bool {
        let shown = || {
            self.shows_selection(panes, panes.focus()) && self.editing.holds_selection(&self.follow)
        };
        !self.typing() && if self.editing() { self.editing.text_selects() } else { shown() }
    }

    /// The selection as the clipboard takes it ([`Canvases::can_copy`]);
    /// with `cut`, it is then removed in one step. `None` when there is
    /// nothing to take.
    pub fn copy(&mut self, panes: &Panes, cut: bool) -> Option<Clip> {
        if !self.can_copy(panes) {
            return None;
        }
        if self.editing() {
            let text = self.editing.copy_text(cut);
            if cut {
                self.send();
            }
            return Some(Clip::new(text, None));
        }
        let clip = self.editing.copied(&self.follow)?;
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
        let viewport = self.follow.view_of(focus).map_or_else(Viewport::start, |v| v.viewport);
        let centre = viewport.to_canvas(Point { x: frame.width / 2.0, y: frame.height / 2.0 });
        let page = mirror.page().to_string();
        let here = (canvas.to_string(), page);
        self.editing.place(&mut self.follow, focus, here, (&items, centre))
    }

    /// The snippet the crop tool's last lift made, as the clipboard takes
    /// it, once (docs/design/pdf.md).
    pub fn cropped(&mut self) -> Option<Clip> {
        self.editing.cropped()
    }

    /// The canvas the focused pane of `panes` shows, and its page, once it
    /// has arrived.
    fn focused_page(&self, panes: &Panes) -> Option<(&str, &Mirror)> {
        let pane = panes.focus();
        let canvas = canvas_of(panes, pane)?;
        let open = self.follow.shown(canvas, pane)?;
        Some((open.canvas.as_str(), open.mirror.as_ref()?))
    }
}
