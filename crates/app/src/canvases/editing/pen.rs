//! The pen on the canvas panes, from the press its tool takes to the lift.
//! A stroke, an erase, a loop, a box and an arrow stay on the pane the pen
//! went down in, read through its view wherever the pen goes. A drag of
//! the selection goes by the canvas pane under the pen, read through that
//! pane's own view (its pan and zoom), at every pen event: so the items
//! stay under the pen in whichever pane it is over, and land there
//! ([`crate::scene::select`]'s `carry`).

use std::borrow::Cow;

use super::Editing;
use crate::canvases::follow::Follow;
use crate::canvases::{canvas_of, step, to_canvas};
use crate::layout::{PaneId, Point, Rect};
use crate::mirror::Mirror;
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId};
use crate::scene::cursor::Cursor;
use crate::scene::ink::{Ink, Pen};
use crate::scene::select::{Landing, Select};
use crate::scene::shapes::Press;
use crate::scene::tools::Tool;

impl Editing {
    /// The pen went down with `tool` and `colour` at `press` on `here`,
    /// the page `pane` of `panes` (laid at `frame`) shows, once nothing
    /// else took the press: the text, box, arrow and crop tools start, the
    /// pen a stroke, the select tool a pick, and the eraser and the lasso
    /// an erase, a loop or a drag of the selection. What it adds until it
    /// lifts joins the folded box the pane is inside, if any. False, and
    /// nothing starts, when the tool takes no press there.
    pub(in crate::canvases) fn press(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        here: (&str, &str),
        press: Press,
        (tool, colour): (Tool, crate::config::Rgb),
    ) -> bool {
        let Press { at, when, zoom, shift } = press;
        follow.pen_in(pane, here, self.ink.mark());
        let items = follow.pane_shown(&self.ink, panes, pane);
        if self.shapes.down((tool, colour), here, press, &items, &mut self.ink) {
            drop(items);
            follow.typed_in(step::Typed::In(pane), &self.shapes, &mut self.ink);
            self.send(follow);
        } else if self.crop.down(tool, here, press, &items) {
            // Its box shows over the page; the lift makes the snippet.
        } else if tool == Tool::Pen {
            self.ink.down(here.0, here.1, colour, at);
        } else if tool == Tool::Select {
            self.select.pick(here, at, zoom, shift, &items);
        } else if !self.select.down(tool, here, at, zoom, &items) {
            return false;
        }
        self.pen = Some((pane, frame));
        self.unshown = Some(when);
        true
    }

    /// The pen moved while down, over `under`, the canvas pane of `panes`
    /// there and its frame, if any: the stroke or the gesture under it
    /// goes on. Returns false when none is under it.
    pub(in crate::canvases) fn drag(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        pen: Pen,
        under: Option<(PaneId, Rect)>,
    ) -> bool {
        let Some((pane, frame)) = self.pen else { return false };
        let at = to_canvas(follow.view(pane).viewport, frame, pen.at);
        self.unshown = Some(self.unshown.map_or(pen.when, |t| t.min(pen.when)));
        if self.ink.drag(at) || self.shapes.drag(at) || self.crop.drag(at) {
            return true;
        }
        let landing = landing(follow, panes, under, pen.at);
        let landing = landing.as_ref().map(|(canvas, page, at)| Landing { canvas, page, at: *at });
        let items = follow.pane_shown(&self.ink, panes, pane);
        self.select.keep_ratio(pen.shift);
        self.select.hover(at, landing, &items)
    }

    /// The pen lifted over `under`, the canvas pane of `panes` there and its
    /// frame, if any: the stroke, box, arrow, erase or move is finished and
    /// goes to the daemon in its turn, the loop selects, or the crop makes
    /// its snippet. A drag of the selection lands as the module says: one
    /// that lifts outside every canvas pane moves nothing. A double tap on
    /// a text with the select tool or the lasso edits it once the pen
    /// lifts where it went down. Returns false when nothing was under the
    /// pen.
    pub(in crate::canvases) fn lift(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        pen: Pen,
        under: Option<(PaneId, Rect)>,
    ) -> bool {
        let Some((pane, frame)) = self.pen.take() else { return false };
        let at = to_canvas(follow.view(pane).viewport, frame, pen.at);
        let landing = landing(follow, panes, under, pen.at);
        self.select.keep_ratio(pen.shift);
        let mark = self.ink.mark();
        let ended = self.ink.up(Some(at)) || {
            let items = follow.pane_shown(&self.ink, panes, pane);
            self.shapes.up(Some(at), &items, &mut self.ink) || self.crop.up(Some(at), &items) || {
                let landing =
                    landing.as_ref().map(|(canvas, page, at)| Landing { canvas, page, at: *at });
                let source = canvas_of(panes, pane).and_then(|c| follow.shown(c, pane));
                let source = source.and_then(|o| Some((o.canvas.as_str(), o.mirror.as_ref()?)));
                let carry = |ink: &Ink, ids: &[ItemId], by| match (source, landing) {
                    (Some((canvas, mirror)), Some(to)) => {
                        mirror.carried((canvas, ink), ids, to.page, by)
                    }
                    _ => Vec::new(),
                };
                self.select.lift(at, landing, &items, &mut self.ink, carry)
            }
        };
        let edits = self.shapes.tapped(at);
        follow.pen_joined(&mut self.ink);
        if let (Some((to, _)), Some((canvas, page, _))) = (under, &landing) {
            follow.added_in((mark, &mut self.ink), to, (canvas, page));
        }
        self.unshown = None;
        self.send(follow);
        ended || edits
    }

    /// The pointer, up, moved to `at` with `tool` over `under`, the canvas
    /// pane of `panes` there and its frame, if any: on a handle of the
    /// selection with the lasso or the select tool, that pane's pointer is
    /// the handle's resize cursor ([`Editing::handle_cursor`]) until the
    /// pointer next moves, so also while the pen drags the handle; on the
    /// right edge of the text being typed, with any tool, it is the left
    /// and right resize cursor. Returns whether that changed.
    pub(in crate::canvases) fn hover_handle(
        &mut self,
        follow: &Follow,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        (at, tool): (Point, Tool),
    ) -> bool {
        let handle = self.at_selection(follow, panes, under, (at, tool), Select::handle_cursor);
        let handle = handle.flatten().or_else(|| {
            let edge = self.at_pane(follow, panes, under, at, |e, here, at, zoom, _| {
                e.shapes.on_edge(here, at, zoom)
            });
            edge.unwrap_or(false).then_some(Cursor::ResizeLeftRight)
        });
        let handle = handle.zip(under).map(|(cursor, (pane, _))| (pane, cursor));
        std::mem::replace(&mut self.handle, handle) != handle
    }

    /// The pointer over `pane` while it is on a handle of the selection
    /// ([`Editing::hover_handle`]); `None` anywhere else.
    pub(in crate::canvases) fn handle_cursor(&self, pane: PaneId) -> Option<Cursor> {
        self.handle.filter(|(on, _)| *on == pane).map(|(_, cursor)| cursor)
    }

    /// What `ask` says of the selection at `at`, a point in the window, with
    /// `tool` over `under`, the canvas pane of `panes` there and its frame:
    /// it is handed the select, the canvas and page the pane shows, the
    /// canvas point, the zoom and the items the pane shows. `None` off every
    /// canvas pane, before its page arrives, or with a tool other than the
    /// lasso and the select tool.
    pub(in crate::canvases) fn at_selection<R>(
        &self,
        follow: &Follow,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        (at, tool): (Point, Tool),
        ask: impl FnOnce(&Select, (&str, &str), Point, f64, &[Cow<'_, Item>]) -> R,
    ) -> Option<R> {
        let moves = matches!(tool, Tool::Lasso | Tool::Select);
        self.at_pane(follow, panes, under.filter(|_| moves), at, |e, here, at, zoom, items| {
            ask(&e.select, here, at, zoom, items)
        })
    }

    /// What `ask` says at `at`, a point in the window, over `under`, the
    /// canvas pane of `panes` there and its frame: it is handed this, the
    /// canvas and page the pane shows in `follow`, the canvas point, the
    /// zoom and the items the pane shows. `None` off every canvas pane or
    /// before its page arrives.
    pub(in crate::canvases) fn at_pane<R>(
        &self,
        follow: &Follow,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        at: Point,
        ask: impl FnOnce(&Self, (&str, &str), Point, f64, &[Cow<'_, Item>]) -> R,
    ) -> Option<R> {
        let (pane, frame) = under?;
        let canvas = canvas_of(panes, pane)?;
        let open = follow.shown(canvas, pane);
        let shown = open.and_then(|o| o.mirror.as_ref()).map(Mirror::page)?;
        let view = follow.view_of(pane).map(|v| v.viewport);
        let viewport = view.unwrap_or_else(crate::layout::Viewport::start);
        let items = follow.pane_shown(&self.ink, panes, pane);
        let at = to_canvas(viewport, frame, at);
        Some(ask(self, (canvas, shown), at, viewport.zoom(), &items))
    }
}

/// The canvas, the page and the canvas point under `at`, a point in the
/// window, in `under`, the canvas pane of `panes` there, by its own view
/// in `follow`; `None` outside every canvas pane or before its page arrives.
fn landing(
    follow: &mut Follow,
    panes: &Panes,
    under: Option<(PaneId, Rect)>,
    at: Point,
) -> Option<(String, String, Point)> {
    let (to, frame) = under?;
    let canvas = canvas_of(panes, to)?;
    let open = follow.shown(canvas, to)?;
    let shown = open.mirror.as_ref()?.page().to_string();
    let canvas = canvas.to_string();
    Some((canvas, shown, to_canvas(follow.view(to).viewport, frame, at)))
}
