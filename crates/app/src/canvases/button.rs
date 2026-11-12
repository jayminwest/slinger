//! The "send to agent" button on the canvas panes (SPEC section 8). What it
//! shows is `scene::button`'s; this module ties it to the sling and the
//! agents' statuses. Runs on the main thread and never waits.
//!
//! A tap on a button slings what is new on its page after the last sling
//! (R1) with `[sling] prompt`, through [`Canvases::sling_now`]: the same
//! `sling_send` as the `sling` command, with nothing selected. While the
//! sling's entry is open, a tap sends it, as return does; the feedback
//! entry or a page's new name closes, as on any press. The `sling`
//! command with nothing selected opens the entry beside the focused page's
//! button, so a short prompt can be typed beside it first. The
//! `place-button` command puts one in the middle of the focused pane, in
//! the ink's queue like any item the pen makes.

use super::{Canvases, canvas_of, find, pane_shown, to_canvas, wanted};
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect, Viewport};
use crate::panes::Panes;
use crate::scene::ink::Change;
use crate::scene::select::Selection;

pub(super) use crate::scene::button::{shown, under};

impl Canvases {
    /// A tap on the button at `button` (canvas points) on the page `here`,
    /// seen through `viewport` in a pane laid at `frame`: sends the open
    /// entry, or slings what is new with no prompt. False while a sling is
    /// on its way.
    pub(super) fn tap(
        &mut self,
        (canvas, page): (&str, &str),
        button: Rect,
        viewport: Viewport,
        frame: Rect,
    ) -> bool {
        if self.sling.is_typing() {
            self.sling.send();
            self.entry_closed();
            self.poll_sling();
            return true;
        }
        // The feedback entry or a page's new name closes, as on any other press.
        self.close_entries();
        let selection = Selection { canvas: canvas.into(), page: page.into(), ids: Vec::new() };
        let around = in_window(viewport, frame, button);
        self.sling_now(&selection, "", around, frame)
    }

    /// The first button on the page the focused pane of `panes` laid in
    /// `area` shows, as a sling of what is new on it: its box and the
    /// pane's frame, in the window.
    pub(super) fn button_in(&self, panes: &Panes, area: Rect) -> Option<(Selection, Rect, Rect)> {
        let pane = panes.focus();
        let frame = panes.place(area).into_iter().find(|(id, ..)| *id == pane)?.2;
        let frame = crate::scene::status::content(frame);
        let canvas = canvas_of(panes, pane)?;
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas))?;
        let shown = open.mirror.as_ref()?.page().to_string();
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        let button = crate::scene::button::first(&items)?;
        let view = self.views.iter().find(|(p, _)| *p == pane);
        let viewport = view.map_or_else(Viewport::start, |(_, v)| v.viewport);
        let selection = Selection { canvas: canvas.into(), page: shown, ids: Vec::new() };
        Some((selection, in_window(viewport, frame, button), frame))
    }

    /// The `place-button` command: a new button in the middle of `pane` of
    /// `panes`, laid at `frame`, on the page it shows. Nothing while that
    /// page has not arrived.
    pub(super) fn place_button(&mut self, panes: &Panes, pane: PaneId, frame: Rect) {
        let Some((canvas, _, shown)) = self.pages_of(panes, pane) else { return };
        let middle = Point { x: frame.x + frame.width / 2.0, y: frame.y + frame.height / 2.0 };
        let centre = to_canvas(self.view(pane).viewport, frame, middle);
        let shape = crate::scene::button::placed(centre);
        let colour = Rgb { r: 0, g: 0, b: 0 };
        self.ink.push(Change::Put { canvas, page: shown, colour, shape });
        self.send();
    }
}

/// `r`, canvas points seen through `viewport` in a pane laid at `frame`,
/// in the window.
fn in_window(viewport: Viewport, frame: Rect, r: Rect) -> Rect {
    let on = viewport.rect_to_screen(r);
    Rect { x: on.x + frame.x, y: on.y + frame.y, ..on }
}

#[cfg(test)]
mod tests;
