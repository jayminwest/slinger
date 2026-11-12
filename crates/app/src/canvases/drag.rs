//! The pen moving and lifting on the canvas panes. A stroke, an erase, a
//! loop, a box and an arrow stay on the pane the pen went down in, read
//! through its view wherever the pen goes. A drag of the selection goes by
//! the canvas pane under the pen, read through that pane's own view (its
//! pan and zoom), at every pen event: so the items stay under the pen in
//! whichever pane it is over, and land there ([`crate::scene::select`]'s
//! `carry`).
//!
//! Both panes on one page show the same items, so a selection's move shows
//! live in both, and so do its box, handles and count: the selection is of
//! the page's items, not of a pane, and delete, escape and sling act on it
//! from any pane that shows its page.

use super::{Canvases, canvas_of, find, pane_shown, to_canvas, wanted};
use crate::layout::{PaneId, Point, Rect};
use crate::mirror::Mirror;
use crate::panes::Panes;
use crate::scene::canvas::ItemId;
use crate::scene::ink::Pen;
use crate::scene::select::Landing;
use crate::scene::tools::Tool;

impl Canvases {
    /// The pen moved while down, over `under`, the canvas pane of `panes`
    /// there and its frame, if any: the stroke or the gesture under it
    /// goes on. Returns false when none is under it.
    pub fn pen_drag(&mut self, panes: &Panes, pen: Pen, under: Option<(PaneId, Rect)>) -> bool {
        let Some((pane, frame)) = self.pen else { return false };
        let at = to_canvas(self.view(pane).viewport, frame, pen.at);
        self.unshown = Some(self.unshown.map_or(pen.when, |t| t.min(pen.when)));
        if self.ink.drag(at) || self.shapes.drag(at) {
            return true;
        }
        let landing = self.landing(panes, under, pen.at);
        let landing = landing.as_ref().map(|(canvas, page, at)| Landing { canvas, page, at: *at });
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        self.select.hover(at, landing, &items)
    }

    /// The pen lifted over `under`, the canvas pane of `panes` there and its
    /// frame, if any: the stroke, box, arrow, erase or move is finished and
    /// goes to the daemon in its turn, or the loop selects. A drag of the
    /// selection lands as the module says: one that lifts outside every
    /// canvas pane moves nothing. A double tap on a text with the select
    /// tool or the lasso edits it once the pen lifts where it went down.
    /// Returns false when nothing was under the pen.
    pub fn pen_up(&mut self, panes: &Panes, pen: Pen, under: Option<(PaneId, Rect)>) -> bool {
        let Some((pane, frame)) = self.pen.take() else { return false };
        let at = to_canvas(self.view(pane).viewport, frame, pen.at);
        let landing = self.landing(panes, under, pen.at);
        let ended = self.ink.up(Some(at)) || {
            let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
            self.shapes.up(Some(at), &items, &mut self.ink) || {
                let landing =
                    landing.as_ref().map(|(canvas, page, at)| Landing { canvas, page, at: *at });
                let source = canvas_of(panes, pane)
                    .and_then(|c| find(&self.open, c, wanted(&self.views, pane, c)));
                let source = source.and_then(|o| o.mirror.as_ref());
                let carry = |ids: &[ItemId], by| match (source, landing) {
                    (Some(mirror), Some(to)) => mirror.carried(ids, to.page, by),
                    _ => Vec::new(),
                };
                self.select.lift(at, landing, &items, &mut self.ink, carry)
            }
        };
        let edits = self.shapes.tapped(at);
        self.unshown = None;
        self.send();
        ended || edits
    }

    /// Whether a press of `pen` with `tool` over `under`, the canvas pane
    /// of `panes` there and its frame, grabs the selection: the lasso or
    /// the select tool inside the selection's box. Such a press drags the
    /// selection even within reach of a divider.
    pub fn grabs(
        &self,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        pen: Pen,
        tool: Tool,
    ) -> bool {
        let Some((pane, frame)) = under.filter(|_| !pen.touch) else { return false };
        let moves = matches!(tool, Tool::Lasso | Tool::Select);
        let Some(canvas) = canvas_of(panes, pane).filter(|_| moves) else { return false };
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(Mirror::page) else {
            return false;
        };
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v.viewport);
        let viewport = view.unwrap_or_else(crate::layout::Viewport::start);
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        let at = to_canvas(viewport, frame, pen.at);
        self.select.grabs((canvas, shown), at, viewport.zoom(), &items)
    }

    /// The canvas, the page and the canvas point under `at`, a point in the
    /// window, in `under`, the canvas pane of `panes` there, by its own
    /// view; `None` outside every canvas pane or before its page arrives.
    fn landing(
        &mut self,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        at: Point,
    ) -> Option<(String, String, Point)> {
        let (to, frame) = under?;
        let canvas = canvas_of(panes, to)?;
        let open = find(&self.open, canvas, wanted(&self.views, to, canvas))?;
        let shown = open.mirror.as_ref()?.page().to_string();
        let canvas = canvas.to_string();
        Some((canvas, shown, to_canvas(self.view(to).viewport, frame, at)))
    }
}

#[cfg(test)]
mod tests;
