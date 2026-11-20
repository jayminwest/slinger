//! Each canvas pane's view: where it looks, the page it shows and what it
//! last drew, kept by the pane's id ([`PaneId`], the server's) and per
//! workspace. A view is for one canvas: a pane that shows another starts
//! again. The views of a workspace not shown are parked, and come back as
//! they were left when it is shown again; a pane that closed meanwhile
//! loses its view then.

use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use slinger_control::PaneView;

use super::step::Trail;
use crate::layout::{PaneId, Point, Rect, Size, Viewport};
use crate::scene::canvas;

/// One canvas pane: the canvas it is for, where it looks, the page it
/// shows (by canvas and page id; the first page without one), and what it
/// last drew.
pub(super) struct View {
    /// `None` until it learns it.
    canvas: Option<String>,
    pub(super) viewport: Viewport,
    pub(super) page: Option<(String, String)>,
    pub(super) last: Option<Rc<canvas::Scene>>,
    /// In reader mode, the column fitted and the screen it was fitted to.
    pub(super) reader: Option<(Rect, Size)>,
    /// The boxes it is inside ([`super::step`]).
    pub(super) trail: Trail,
    /// Whether it has a place: false for a new view until its page first
    /// comes ([`View::place`]) or the user moves it. A view kept elsewhere
    /// that comes back is placed.
    pub(super) placed: bool,
}

impl Default for View {
    /// A new pane's view: at the start, on the first page, nothing drawn,
    /// not placed.
    fn default() -> Self {
        let (trail, viewport) = (Trail::default(), Viewport::start());
        let (page, last, reader) = (None, None, None);
        Self { canvas: None, viewport, page, last, reader, trail, placed: false }
    }
}

impl View {
    /// Makes this the view of `canvas`: one of another canvas starts again,
    /// so nothing of that one shows here.
    pub(super) fn on(&mut self, canvas: &str) {
        if self.canvas.as_deref().is_some_and(|c| c != canvas) {
            *self = Self::default();
        }
        if self.canvas.is_none() {
            self.canvas = Some(canvas.to_string());
        }
    }

    /// Places a view that has no place yet, once, with its page come, where
    /// it opens on `content` (what the page shows, or the room of the box
    /// it is inside) on a screen of `size` ([`Viewport::opening`]). A view
    /// in reader mode keeps its column's place.
    pub(super) fn place(&mut self, content: Option<Rect>, size: Size) {
        if !std::mem::replace(&mut self.placed, true) && self.reader.is_none() {
            self.viewport = Viewport::opening(content, size);
        }
    }

    /// Makes this the view of `canvas` ([`View::on`]) and, when it has no
    /// place yet, takes `kept`, where the daemon kept the pane looking, if
    /// that is of `canvas`: its page, origin and zoom ([`Viewport::kept`]),
    /// placed, so the first-open fit does not run. A view with a place wins.
    pub(super) fn learn(&mut self, canvas: &str, kept: Option<&PaneView>) {
        self.on(canvas);
        let Some(kept) = kept.filter(|k| !self.placed && k.canvas == canvas) else { return };
        self.viewport = Viewport::kept(Point { x: kept.x, y: kept.y }, kept.zoom);
        self.page = kept.page.clone().map(|page| (canvas.to_string(), page));
        self.placed = true;
    }

    /// The view of a pane split off this one: the same canvas, page, place
    /// and boxes, placed, with nothing drawn yet (its frame differs).
    fn copied(&self) -> Self {
        let (canvas, page, trail) = (self.canvas.clone(), self.page.clone(), self.trail.clone());
        let (viewport, reader) = (self.viewport, self.reader);
        Self { canvas, viewport, page, last: None, reader, trail, placed: true }
    }

    /// The canvas it is the view of, once it knows.
    pub(super) fn canvas(&self) -> Option<&str> {
        self.canvas.as_deref()
    }

    /// Whether it may be the view of `canvas`: it is, or it has not learnt
    /// its canvas yet.
    fn for_canvas(&self, canvas: &str) -> bool {
        self.canvas.as_deref().is_none_or(|c| c == canvas)
    }
}

type Shown = Vec<(PaneId, View)>;

/// The views of the panes shown, by pane, for one workspace (`None`: the
/// window's own panes), and those of the other workspaces, parked.
#[derive(Default)]
pub(super) struct Views {
    workspace: Option<String>,
    shown: Shown,
    parked: Vec<(Option<String>, Shown)>,
}

impl Deref for Views {
    type Target = [(PaneId, View)];

    fn deref(&self) -> &Self::Target {
        &self.shown
    }
}

impl DerefMut for Views {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shown
    }
}

impl Views {
    /// Shows the views of `workspace`'s panes: those shown are parked under
    /// their workspace, and those parked under this one come back. Whether
    /// the workspace changed.
    pub(super) fn within(&mut self, workspace: Option<&str>) -> bool {
        if self.workspace.as_deref() == workspace {
            return false;
        }
        let back = self.parked.iter().position(|(w, _)| w.as_deref() == workspace);
        let back = back.map(|at| self.parked.swap_remove(at).1).unwrap_or_default();
        let workspace = workspace.map(str::to_string);
        let left = std::mem::replace(&mut self.workspace, workspace);
        let shown = std::mem::replace(&mut self.shown, back);
        self.parked.push((left, shown));
        true
    }

    /// Pane `pane`'s view, a new one if it has none.
    pub(super) fn get(&mut self, pane: PaneId) -> &mut View {
        let at = self.shown.iter().position(|(p, _)| *p == pane).unwrap_or_else(|| {
            self.shown.push((pane, View::default()));
            self.shown.len() - 1
        });
        &mut self.shown[at].1
    }

    /// Gives pane `to` a copy of pane `from`'s view ([`View::copied`]); one
    /// with no view yet keeps its own.
    pub(super) fn copy(&mut self, from: PaneId, to: PaneId) {
        let copied = self.shown.iter().find(|(p, _)| *p == from).map(|(_, v)| v.copied());
        if let Some(view) = copied.filter(|_| from != to) {
            *self.get(to) = view;
        }
    }

    /// Forgets the views of the panes not in `panes`.
    pub(super) fn keep(&mut self, panes: &[PaneId]) {
        self.shown.retain(|(id, _)| panes.contains(id));
    }

    /// Turns pane `pane` to page `to` of `canvas`, a page it asked for:
    /// shown or parked, wherever its view is for that canvas; a pane with
    /// no view yet takes it shown.
    pub(super) fn turn(&mut self, pane: PaneId, canvas: String, to: String) {
        let unseen = !self.shown.iter().any(|(p, _)| *p == pane);
        let mine = |(p, v): &&mut (PaneId, View)| *p == pane && v.for_canvas(&canvas);
        let parked = self.parked.iter_mut().flat_map(|(_, shown)| shown.iter_mut());
        let found = self.shown.iter_mut().find(mine).or_else(|| parked.into_iter().find(mine));
        match found {
            Some((_, view)) => view.page = Some((canvas, to)),
            None if unseen => {
                self.get(pane).page = Some((canvas, to));
            }
            None => {}
        }
    }
}

impl super::Canvases {
    /// Pane `to`, split off pane `from`, shows what `from` shows, where it
    /// looks: the same page, pan, zoom and boxes. After that each moves on
    /// its own.
    pub fn split_view(&mut self, from: PaneId, to: PaneId) {
        self.views.copy(from, to);
    }
}

#[cfg(test)]
mod tests;
