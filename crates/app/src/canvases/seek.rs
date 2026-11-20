//! `find` and `find-next` on a canvas pane: the entry that asks for the
//! words, the daemon's `canvas_find` over every page of the pane's canvas,
//! and the jump: the pane turns to the text's page and centres it, the
//! text is the selection (so the item keys act from it), and the flash
//! says which of how many it is. A text a folded box hides is not on the
//! page the pane draws: the jump goes to that box (the outermost, as the
//! hit's `folded` names it) and selects it instead. Which text is next is
//! [`crate::scene::pages::find`]'s; the search is the daemon's.

use serde_json::Value;

use super::{Canvases, canvas_of, find, wanted};
use crate::layout::{PaneId, Rect};
use crate::mirror;
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId};
use crate::scene::pages::find::{self as finding, Hit, Sought};

/// What `find` flashes on a pane that shows no canvas.
const NO_CANVAS: &str = "No canvas in this pane to find on";

/// The last find: the canvas, the words and the text it went to, if any.
pub(super) struct Last {
    canvas: String,
    text: String,
    item: Option<ItemId>,
}

impl Canvases {
    /// The `find` command on `pane` of `panes`, laid at `frame`: an entry
    /// for the words opens there, holding the last find's on the same canvas.
    pub(super) fn ask_find(&mut self, panes: &Panes, (pane, frame): (PaneId, Rect)) {
        let Some(canvas) = canvas_of(panes, pane).map(str::to_string) else {
            self.zoomed = Some(NO_CANVAS.to_string());
            return;
        };
        self.shapes.end(&mut self.ink);
        self.close_entries();
        let last = self.found.as_ref().filter(|l| l.canvas == canvas).map(|l| l.text.as_str());
        self.asks.find((pane, frame), &canvas, last.unwrap_or_default());
        self.send();
    }

    /// The `find-next` command: the text after the last find's, with the
    /// same words on the same canvas; with none to go on from, as `find`.
    pub(super) fn find_next(&mut self, panes: &Panes, (pane, frame): (PaneId, Rect)) {
        let canvas = canvas_of(panes, pane);
        match self.found.as_ref().filter(|l| Some(l.canvas.as_str()) == canvas) {
            Some(last) => {
                let (canvas, text) = (last.canvas.clone(), last.text.clone());
                self.seek(Sought { pane: (pane, frame), canvas, text });
            }
            None => self.ask_find(panes, (pane, frame)),
        }
    }

    /// Finds the words a find's entry was answered with, if it was.
    pub(super) fn seek_sought(&mut self) {
        if let Some(sought) = self.asks.sought() {
            self.seek(sought);
        }
    }

    /// Asks the daemon for the texts that hold `sought`'s words, goes to
    /// the next ([`finding::next`]) and flashes where it is, or why not.
    fn seek(&mut self, Sought { pane: (pane, frame), canvas, text }: Sought) {
        let found = match self.daemon.canvas_find(&canvas, &text) {
            Ok(found) => found,
            Err(e) => {
                self.zoomed = Some(e.to_string());
                return;
            }
        };
        let hits: Vec<Hit> = found.iter().filter_map(hit).collect();
        let open = find(&self.open, &canvas, wanted(&self.views, pane, &canvas));
        let mirror = open.and_then(|o| o.mirror.as_ref());
        let pages = mirror.map(|m| m.pages().to_vec()).unwrap_or_default();
        let shown = mirror.map(|m| m.page().to_string()).unwrap_or_default();
        let same = self.found.as_ref().filter(|l| l.canvas == canvas && l.text == text);
        let at = finding::next(&hits, &pages, &shown, same.and_then(|l| l.item));
        self.asks.found(finding::said(&text, &hits, at, &pages));
        let hit = at.and_then(|i| hits.get(i));
        if let Some(hit) = hit {
            let to = self.shown_as(mirror.filter(|m| m.page() == hit.page), &canvas, hit);
            self.jump(pane, frame, (&canvas, &hit.page), to);
        }
        self.found = Some(Last { canvas, text, item: hit.map(|h| h.item) });
    }

    /// What a find goes to for `hit` on `canvas`: the text and its frame,
    /// or the folded box that hides it and the box's frame (its miniature),
    /// from `shown`, the mirror of the hit's page when the pane shows it,
    /// else from the daemon's page. A box not found keeps the text's frame.
    fn shown_as(&self, shown: Option<&mirror::Mirror>, canvas: &str, hit: &Hit) -> (ItemId, Rect) {
        let Some(folded) = hit.folded else { return (hit.item, hit.frame) };
        let frame = |item: &Item| (item.id == folded).then(|| item.shape.bounds()).flatten();
        let frame = match shown {
            Some(mirror) => mirror.items().iter().find_map(frame),
            None => self.daemon.canvas_show(canvas, Some(&hit.page)).ok().and_then(|page| {
                page.items.iter().filter_map(mirror::item).find_map(|i| frame(&i))
            }),
        };
        (folded, frame.unwrap_or(hit.frame))
    }

    /// Turns `pane`, laid at `frame`, to `place` (a canvas and its page),
    /// centres the item `id` at `at` in what its footer leaves of it and
    /// selects it alone.
    fn jump(&mut self, pane: PaneId, frame: Rect, place: (&str, &str), (id, at): (ItemId, Rect)) {
        self.select.only(place, id);
        let frame = crate::scene::status::content(frame);
        let view = self.view(pane);
        view.page = Some((place.0.to_string(), place.1.to_string()));
        let on = view.viewport.rect_to_screen(at);
        let (x, y) = (on.x + on.width / 2.0, on.y + on.height / 2.0);
        view.pan(frame.width / 2.0 - x, frame.height / 2.0 - y);
    }
}

/// A found item as a [`Hit`]: its page, its frame and the folded box
/// that hides it, if one does.
fn hit(found: &Value) -> Option<Hit> {
    let item = mirror::item(found)?;
    let page = found.get("page")?.as_str()?.to_string();
    let folded = found.get("folded").and_then(Value::as_str).and_then(mirror::item_id);
    Some(Hit { page, item: item.id, frame: item.shape.bounds()?, folded })
}

#[cfg(test)]
mod tests;
