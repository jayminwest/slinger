//! `unlock-figure` (docs/design/figures.md, "The unlock"): where it is
//! done is [`super::editing`]'s (`figure`).

use super::Canvases;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;

impl Canvases {
    /// Grabs the figure under the pointer, a point in the window over a
    /// pane of `panes` and that pane's frame, as one selection. False, and
    /// the flash says why, when no locked item is under it.
    pub fn unlock_figure(&mut self, panes: &Panes, under: Option<((PaneId, Rect), Point)>) -> bool {
        let grabbed = under
            .is_some_and(|(at, pointer)| self.editing.unlock(&mut self.follow, panes, at, pointer));
        if !grabbed {
            self.editing.no_figure();
        }
        grabbed
    }

    /// Whether `unlock-figure` has a figure under `pointer`, over `pane`
    /// of `panes` laid at `frame`.
    pub fn figure_under(&self, panes: &Panes, at: (PaneId, Rect), pointer: Point) -> bool {
        self.editing.figure_under(&self.follow, panes, at, pointer)
    }

    /// A tool that writes was chosen: a grabbed figure is locked again.
    pub fn relock(&mut self) -> bool {
        self.editing.relock()
    }
}
