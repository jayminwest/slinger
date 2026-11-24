//! `unlock-figure` on a pane (docs/design/figures.md, "The unlock"): the
//! figure under the pointer is grabbed as the selection
//! ([`Select::unlock`](crate::scene::select::Select::unlock)); a tool
//! change to the pen, the eraser or the text tool lets it go.

use super::Editing;
use crate::canvases::follow::Follow;
use crate::canvases::{canvas_of, to_canvas};
use crate::layout::{PaneId, Point, Rect, Viewport};
use crate::panes::Panes;
use crate::scene::select::{REACH, figure_at};

/// What the flash says when no figure is under the pointer.
const NO_FIGURE: &str = "No figure under the pointer";

impl Editing {
    /// Grabs the figure under `at`, a point in the window over `pane` of
    /// `panes` laid at `frame`, as the selection. False when no locked
    /// item is there.
    pub(in crate::canvases) fn unlock(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        at: Point,
    ) -> bool {
        let Some(canvas) = canvas_of(panes, pane) else { return false };
        let open = follow.shown(canvas, pane).and_then(|o| o.mirror.as_ref());
        let Some(shown) = open.map(|m| m.page().to_string()) else { return false };
        let viewport = follow.view(pane).viewport;
        let items = follow.pane_shown(&self.ink, panes, pane);
        let (at, reach) = (to_canvas(viewport, frame, at), REACH / viewport.zoom());
        self.select.unlock((canvas, &shown), at, reach, &items)
    }

    /// The flash says no figure is under the pointer.
    pub(in crate::canvases) fn no_figure(&mut self) {
        self.said = Some(NO_FIGURE);
    }

    /// Whether a locked item is under `at`, a point in the window over
    /// `pane` of `panes` laid at `frame`: `unlock-figure` has one to grab.
    pub(in crate::canvases) fn figure_under(
        &self,
        follow: &Follow,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        at: Point,
    ) -> bool {
        let viewport = follow.view_of(pane).map_or_else(Viewport::start, |v| v.viewport);
        let items = follow.pane_shown(&self.ink, panes, pane);
        let (at, reach) = (to_canvas(viewport, frame, at), REACH / viewport.zoom());
        figure_at(&items, at, reach).is_some()
    }

    /// A tool change to one that writes lets go of a grabbed figure: it is
    /// locked again. False when none was grabbed.
    pub(in crate::canvases) fn relock(&mut self) -> bool {
        self.select.grabbed() && self.select.clear()
    }
}
