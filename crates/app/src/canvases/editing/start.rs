//! A text started from the keyboard (`new-text`, slinger-b16804): while a
//! text is being typed, it ends and the next starts half a line under it,
//! so notes chain without a tap; else the new text starts at the pointer
//! when it is over the pane (or a text there opens to be edited, as a tap
//! opens it), under the last text ended on the pane's page, or at the
//! pane's centre.

use super::Editing;
use crate::canvases::follow::Follow;
use crate::canvases::{canvas_of, step, to_canvas};
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::panes::Panes;
use crate::scene::select::into_view;
use crate::scene::shapes::REACH;

impl Editing {
    /// Starts a new text in `colour` on the page `pane` of `panes` (laid at
    /// `frame`) shows, or chained under the text being typed on that text's
    /// page; `pointer` is where the pointer is in the window, if known. The
    /// new text is brought into view when the pane shows its page. False,
    /// and nothing starts, when the pane shows no page.
    pub(in crate::canvases) fn new_text(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        (pointer, colour): (Option<Point>, Rgb),
    ) -> bool {
        let Some(canvas) = canvas_of(panes, pane).map(str::to_string) else { return false };
        let open = follow.shown(&canvas, pane);
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string()) else {
            return false;
        };
        let viewport = follow.view(pane).viewport;
        let typed = self.shapes.editing();
        let joined = follow.typing_in();
        let aimed = pointer.filter(|p| !typed && frame.contains(*p));
        let chained = if aimed.is_none() { self.shapes.chained(&mut self.ink) } else { None };
        let centre = Point { x: frame.x + frame.width / 2.0, y: frame.y + frame.height / 2.0 };
        let here = |c: &str, p: &str| c == canvas && p == shown;
        let (on, at) = match (aimed, chained) {
            (Some(p), _) => (None, to_canvas(viewport, frame, p)),
            (None, Some((c, p, at))) if typed || here(&c, &p) => (Some((c, p)), at),
            (None, _) => (None, to_canvas(viewport, frame, centre)),
        };
        let seen = on.as_ref().is_none_or(|(c, p)| here(c, p));
        let typed = if on.is_some() { step::Typed::Under(joined) } else { step::Typed::In(pane) };
        let (canvas, shown) = on.unwrap_or((canvas, shown));
        if aimed.is_some() {
            let items = follow.pane_shown(&self.ink, panes, pane);
            let reach = (at, REACH / viewport.zoom());
            self.shapes.start_at((&canvas, &shown), reach, &items, colour, &mut self.ink);
        } else {
            self.shapes.start((&canvas, &shown), at, colour, &mut self.ink);
        }
        follow.typed_in(typed, &self.shapes, &mut self.ink);
        let size = Size { width: frame.width, height: frame.height };
        let into = self.shapes.typing_frame().and_then(|f| into_view(viewport, size, f));
        if let Some((dx, dy)) = into.filter(|_| seen) {
            follow.view(pane).pan(dx, dy);
        }
        self.send(follow);
        true
    }
}
