//! The pen moving and lifting on the canvas panes, and the pointer over
//! them: what the gesture does is [`super::editing`]'s (`pen`), on the
//! pages [`super::follow`] shows.
//!
//! Both panes on one page show the same items, so a selection's move shows
//! live in both, and so do its box, handles and count: the selection is of
//! the page's items, not of a pane, and delete, escape and sling act on it
//! from any pane that shows its page.

use super::Canvases;
use crate::commands::context::Subject;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;
use crate::scene::cursor::Cursor;
use crate::scene::ink::Pen;
use crate::scene::select::Select;
use crate::scene::tools::Tool;

impl Canvases {
    /// The pen moved while down, over `under`, the canvas pane of `panes`
    /// there and its frame, if any: the stroke or the gesture under it
    /// goes on. Returns false when none is under it.
    pub fn pen_drag(&mut self, panes: &Panes, pen: Pen, under: Option<(PaneId, Rect)>) -> bool {
        self.editing.drag(&mut self.follow, panes, pen, under)
    }

    /// The pen lifted over `under`, the canvas pane of `panes` there and its
    /// frame, if any: the stroke, box, arrow, erase or move is finished and
    /// goes to the daemon in its turn, the loop selects, or the crop makes
    /// its snippet ([`Canvases::cropped`]). A drag of the selection that
    /// lifts outside every canvas pane moves nothing. Returns false when
    /// nothing was under the pen.
    pub fn pen_up(&mut self, panes: &Panes, pen: Pen, under: Option<(PaneId, Rect)>) -> bool {
        self.editing.lift(&mut self.follow, panes, pen, under)
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
        let under = under.filter(|_| !pen.touch);
        let grabs =
            self.editing.at_selection(&self.follow, panes, under, (pen.at, tool), Select::grabs);
        grabs.unwrap_or(false)
    }

    /// The pointer, up, moved to `at` with `tool` over `under`, the canvas
    /// pane of `panes` there and its frame, if any: on a handle of the
    /// selection, or the right edge of the text being typed, that pane's
    /// pointer is a resize cursor ([`Canvases::handle_cursor`]). Returns
    /// whether that changed.
    pub fn hover_handle(
        &mut self,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        at: Point,
        tool: Tool,
    ) -> bool {
        self.editing.hover_handle(&self.follow, panes, under, (at, tool))
    }

    /// The pointer over `pane` while it is on a handle of the selection
    /// ([`Canvases::hover_handle`]); `None` anywhere else.
    pub fn handle_cursor(&self, pane: PaneId) -> Option<Cursor> {
        self.editing.handle_cursor(pane)
    }

    /// Whether `at`, a point in the window over `under`, the canvas pane
    /// of `panes` there and its frame, is on a live item whose page shows
    /// and takes presses, and not on the selection a press there would
    /// grab: a right press there is the page's.
    pub fn page_under(&self, panes: &Panes, under: Option<(PaneId, Rect)>, at: Point) -> bool {
        let follow = &self.follow;
        self.editing
            .at_pane(follow, panes, under, at, |e, here, at, zoom, items| {
                follow.live_under(items, at, zoom) && !e.grabs(here, at, zoom, items)
            })
            .unwrap_or(false)
    }

    /// What a context click at `at`, a point in the window over `under`
    /// (the canvas pane of `panes` there and its frame), is on, by
    /// Finder's rule, the selection made accordingly
    /// ([`super::editing`]'s `context`). `None` off every canvas pane, or
    /// before its page arrives.
    pub fn context_at(
        &mut self,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        at: Point,
    ) -> Option<Subject> {
        self.editing.context_at(&mut self.follow, panes, under, at)
    }
}

#[cfg(test)]
mod tests;
