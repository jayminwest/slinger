//! What the keys change: typed and pasted text and the caret keys go to
//! the text being typed; delete, a colour picked and a size command act
//! on the selection; select all, next and previous item, nudge, duplicate
//! and fold on a pane's page ([`Keyed`]); and a clip's items placed on a
//! page, or the selection copied from it.

use super::Editing;
use crate::canvases::follow::Follow;
use crate::canvases::{canvas_of, find, wanted};
use crate::commands::Chord;
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::mirror::{Mirror, clip};
use crate::panes::Panes;
use crate::scene::canvas::ItemId;
use crate::scene::ink::Change;
use crate::scene::paste::Clip;
use crate::scene::select::{Keyed, into_view};
use crate::scene::tools::Resize;

impl Editing {
    /// Typed `text` goes into the text being typed; escape ends it, and the
    /// text goes to the daemon. Returns false when none is open.
    pub(in crate::canvases) fn typed(&mut self, follow: &mut Follow, text: &str) -> bool {
        if self.vim.typed(text, &self.shapes) {
            return true;
        }
        let used = self.shapes.typed(text, &mut self.ink);
        if used {
            self.send(follow);
        }
        used
    }

    /// A caret, selection or delete key of the text being typed
    /// ([`crate::scene::editor`]). False when none is typed, or the key is
    /// not one of them.
    pub(in crate::canvases) fn key(&mut self, follow: &mut Follow, chord: Chord) -> bool {
        let vim = self.vim.key(chord, &mut self.shapes, &mut self.ink);
        let used = vim.unwrap_or_else(|| self.shapes.key(chord));
        if used {
            self.send(follow);
        }
        used
    }

    /// Pasted `text` goes into the text being typed. False when none is.
    pub(in crate::canvases) fn pasted(&mut self, text: &str) -> bool {
        self.vim.pasted(text, &self.shapes) || self.shapes.pasted(text)
    }

    /// Whether `pane` of `panes` shows the selection's page in `follow`.
    pub(in crate::canvases) fn shows_selection(
        &self,
        follow: &Follow,
        panes: &Panes,
        pane: PaneId,
    ) -> bool {
        let Some(s) = self.selection() else { return false };
        let asked = wanted(follow.views(), pane, &s.canvas);
        let open = find(follow.open(), &s.canvas, asked).and_then(|o| o.mirror.as_ref());
        let showing = asked.or(open.map(Mirror::page));
        canvas_of(panes, pane) == Some(&s.canvas) && showing == Some(&s.page)
    }

    /// Removes the selection, in one step. False when there is none.
    pub(in crate::canvases) fn delete(&mut self, follow: &mut Follow) -> bool {
        self.select.delete(&mut self.ink) && {
            self.send(follow);
            true
        }
    }

    /// Makes the selection's strokes, texts, boxes and arrows `colour`, in
    /// one step ([`crate::scene::select::Select::recolour`]). Returns false
    /// when there is no selection, or none of it changes.
    pub(in crate::canvases) fn recolour(&mut self, follow: &mut Follow, colour: Rgb) -> bool {
        let Some(s) = self.select.selection() else { return false };
        let open = follow.mirror(&s.canvas, &s.page);
        let Some(mirror) = open else { return false };
        let items = self.ink.shown(&s.canvas, &s.page, mirror.items());
        self.select.recolour(colour, &items, &mut self.ink) && {
            self.send(follow);
            true
        }
    }

    /// A size command resizes the text being typed, else the selected
    /// texts, in one step each ([`Resize`]). False when there is neither.
    pub(in crate::canvases) fn resize(&mut self, follow: &mut Follow, resize: Resize) -> bool {
        if self.shapes.resize(resize) {
            return true;
        }
        let Some(s) = self.select.selection() else { return false };
        let open = follow.mirror(&s.canvas, &s.page);
        let Some(mirror) = open else { return false };
        let items = self.ink.shown(&s.canvas, &s.page, mirror.items());
        let resized = self.select.resize(resize, &items, &mut self.ink);
        if resized {
            self.send(follow);
        }
        resized
    }

    /// Does `keyed` on the page `pane` of `panes` (laid at `frame`) shows:
    /// select all; step to the next or previous item, panning it into
    /// view; nudge, duplicate or fold the selection when the pane shows it.
    /// False, and the key goes on, when there is nothing to act on.
    pub(in crate::canvases) fn keyed(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        keyed: Keyed,
    ) -> bool {
        let Some(canvas) = canvas_of(panes, pane) else { return false };
        let open = follow.shown(canvas, pane);
        let Some(on_page) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string())
        else {
            return false;
        };
        let viewport = follow.view(pane).viewport;
        let shown = self.shows_selection(follow, panes, pane);
        let items = follow.pane_shown(&self.ink, panes, pane);
        let here = (canvas, on_page.as_str());
        let zoom = viewport.zoom();
        match keyed {
            Keyed::All => self.select.all(here, &items),
            Keyed::Step(forward) => {
                let Some(item) = self.select.step(here, &items, forward) else { return false };
                drop(items);
                let size = Size { width: frame.width, height: frame.height };
                if let Some((dx, dy)) = into_view(viewport, size, item) {
                    follow.view(pane).pan(dx, dy);
                }
                true
            }
            Keyed::Nudge(dx, dy) => {
                let by = Point { x: dx / zoom, y: dy / zoom };
                let moved = shown && self.select.nudge(by, &items, &mut self.ink);
                drop(items);
                moved && {
                    self.send(follow);
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
                let open = follow.mirror(canvas, &on_page);
                let Some(items) = open.and_then(|m| m.copied(ink, &ids)?.items) else {
                    return false;
                };
                self.place(follow, pane, (canvas.to_string(), on_page), (&items, at))
            }
            Keyed::Fold => {
                drop(items);
                shown && self.fold_selection(follow, pane, (canvas, &on_page))
            }
        }
    }

    /// Folds each box of the selection on `here`, a page of a canvas, or
    /// unfolds it when folded, each one undo step ([`Editing::fold`]).
    /// False when the selection holds no box shown there.
    pub(in crate::canvases) fn fold_selection(
        &mut self,
        follow: &mut Follow,
        pane: PaneId,
        here: (&str, &str),
    ) -> bool {
        let Some(s) = self.select.selection().filter(|s| (&*s.canvas, &*s.page) == here) else {
            return false;
        };
        let Some(mirror) = follow.mirror(here.0, here.1) else { return false };
        let boxes: Vec<(ItemId, bool)> =
            s.ids.iter().filter_map(|&id| Some((id, mirror.box_folded(id)?))).collect();
        self.fold(follow, pane, here, &boxes)
    }

    /// Adds a clip's `items` to `page` of `canvas`, as pane `to` shows it,
    /// centred on `centre`, in one step: they are the selection once there,
    /// and join the folded box the pane is inside, if any. False when there
    /// is nothing to add.
    pub(in crate::canvases) fn place(
        &mut self,
        follow: &mut Follow,
        to: PaneId,
        (canvas, page): (String, String),
        (items, centre): (&str, Point),
    ) -> bool {
        let items = clip::pasted(items, &page, centre);
        if items.is_empty() {
            return false;
        }
        let change =
            Change::Carry { canvas: canvas.clone(), page: page.clone(), items, gone: vec![] };
        let key = self.ink.push_keyed(change);
        follow.added_in((key, &mut self.ink), to, (&canvas, &page));
        self.select.pasted(key, &canvas, &page);
        self.send(follow);
        true
    }

    /// Whether the selection's page as shown holds every selected item,
    /// which copy takes them from: the server's copy with the ink laid
    /// over it, so ink not yet written is copied too (slinger-8e4112).
    pub(in crate::canvases) fn holds_selection(&self, follow: &Follow) -> bool {
        let Some(selection) = self.selection() else { return false };
        let open = find(follow.open(), &selection.canvas, Some(&selection.page));
        let Some(mirror) = open.and_then(|o| o.mirror.as_ref()) else { return false };
        let items = self.ink.shown(&selection.canvas, mirror.page(), mirror.items());
        selection.ids.iter().all(|id| items.iter().any(|i| i.id == *id))
    }

    /// The selection's items as the clipboard takes them, from its page
    /// in `follow` with the ink laid over it. `None` when there is none.
    pub(in crate::canvases) fn copied(&self, follow: &Follow) -> Option<Clip> {
        let selection = self.selection()?;
        let open = find(follow.open(), &selection.canvas, Some(&selection.page));
        let ink = (selection.canvas.as_str(), &self.ink);
        open.and_then(|o| o.mirror.as_ref())?.copied(ink, &selection.ids)
    }

    /// The snippet the crop tool's last lift made, as the clipboard takes
    /// it, once (docs/design/pdf.md).
    pub(in crate::canvases) fn cropped(&mut self) -> Option<Clip> {
        self.crop.take().as_ref().and_then(clip::snippet)
    }
}
