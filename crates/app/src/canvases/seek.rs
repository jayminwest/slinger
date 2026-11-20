//! `find` and `find-next` on a canvas pane: the entry that asks for the
//! words, the daemon's `canvas_find` over every page of the pane's canvas,
//! and the jump: the pane turns to the text's page and centres it, the
//! text is the selection (so the item keys act from it), and the flash
//! says which of how many it is. Which text is next is
//! [`crate::scene::pages::find`]'s; the search is the daemon's.

use serde_json::Value;

use super::{Canvases, canvas_of, find, wanted};
use crate::layout::{PaneId, Rect};
use crate::mirror;
use crate::panes::Panes;
use crate::scene::canvas::ItemId;
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
            self.jump(pane, frame, &canvas, hit);
        }
        self.found = Some(Last { canvas, text, item: hit.map(|h| h.item) });
    }

    /// Turns `pane`, laid at `frame`, to `hit`'s page of `canvas`, centres
    /// the text in what its footer leaves of it and selects it alone.
    fn jump(&mut self, pane: PaneId, frame: Rect, canvas: &str, hit: &Hit) {
        self.select.only((canvas, &hit.page), hit.item);
        let frame = crate::scene::status::content(frame);
        let view = self.view(pane);
        view.page = Some((canvas.to_string(), hit.page.clone()));
        let on = view.viewport.rect_to_screen(hit.frame);
        let (x, y) = (on.x + on.width / 2.0, on.y + on.height / 2.0);
        view.pan(frame.width / 2.0 - x, frame.height / 2.0 - y);
    }
}

/// A found item as a [`Hit`]: its page and its frame.
fn hit(found: &Value) -> Option<Hit> {
    let item = mirror::item(found)?;
    let page = found.get("page")?.as_str()?.to_string();
    Some(Hit { page, item: item.id, frame: item.shape.bounds()? })
}

#[cfg(test)]
mod tests;
