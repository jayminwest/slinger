//! Write focus (`write-focus`, docs/design/writing-column.md step 6): the
//! focused pane zooms and pans so the column of the text being typed, or
//! of the one text selected, fills its width ([`writing`]), and the view
//! it had comes back when a text ends or on the same command again. The
//! jump between nvim's grid and the drawn text then hides inside a view
//! change.

use super::Canvases;
use crate::layout::{MAX_ZOOM, MIN_ZOOM, PaneId, Point, Rect, Size, Viewport};
use crate::nvim::COLUMNS;
use crate::panes::Panes;
use crate::scene::canvas::Shape;
use crate::scene::glyphs::CELL;
use crate::scene::status;

/// How many cells of margin a focused column has on each side.
const MARGIN: f64 = 4.0;
/// What write focus flashes when the pane has no text to focus on.
const NO_TEXT: &str = "No text to focus on: type one, or select one";

/// A pane in write focus: the view it had, and how many texts had ended
/// then ([`super::editing`]): one more ending gives the view back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Focus {
    pane: PaneId,
    before: Viewport,
    ends: u64,
}

/// The view of a text whose top left is `at`, `size` high, on a screen of
/// `screen`: the column of [`COLUMNS`] cells and [`MARGIN`] cells each
/// side fills the screen's width (the zoom held in `MIN_ZOOM..=MAX_ZOOM`,
/// the column centred when held), and the text's top is a row (one size)
/// below the screen's top. Pure.
pub(super) fn writing(at: Point, size: f64, screen: Size) -> Viewport {
    let cell = size * CELL;
    #[expect(clippy::cast_precision_loss, reason = "80 cells")]
    let column = COLUMNS as f64 * cell;
    let zoom = (screen.width / (column + 2.0 * MARGIN * cell)).clamp(MIN_ZOOM, MAX_ZOOM);
    let x = at.x + column / 2.0 - screen.width / zoom / 2.0;
    Viewport::kept(Point { x, y: at.y - size }, zoom)
}

impl Canvases {
    /// Runs `write-focus` on `pane` of `panes`, laid at `frame`: a pane in
    /// write focus gets its view back; else it focuses on its text
    /// ([`Canvases::focus_target`]), or flashes that it has none. A view in
    /// reader mode keeps its place.
    pub(super) fn write_focus(&mut self, panes: &Panes, (pane, frame): (PaneId, Rect)) {
        if self.focus.is_some_and(|f| f.pane == pane) {
            self.unfocus();
            return;
        }
        let Some((at, size)) = self.focus_target(panes, pane) else {
            self.zoomed = Some(NO_TEXT.to_string());
            return;
        };
        self.unfocus();
        let screen = status::content(frame);
        let screen = Size { width: screen.width, height: screen.height };
        let view = self.view(pane);
        if view.reader.is_some() {
            return;
        }
        let before = view.viewport;
        view.placed = true;
        view.viewport = writing(at, size, screen);
        self.focus = Some(Focus { pane, before, ends: self.editing.ends() });
    }

    /// The top left and size of the text write focus would fit `pane` of
    /// `panes` to: the one being typed on the page it shows, else the
    /// selection there when it is one text.
    pub fn focus_target(&self, panes: &Panes, pane: PaneId) -> Option<(Point, f64)> {
        let (canvas, _, shown) = self.pages_of(panes, pane)?;
        if let Some((c, p, at, size)) = self.editing.typing_place() {
            return (c == canvas && p == shown).then_some((at, size));
        }
        let s = self.selection().filter(|_| self.shows_selection(panes, pane))?;
        let [id] = s.ids.as_slice() else { return None };
        let items = self.follow.pane_shown(self.editing.ink(), panes, pane);
        match items.iter().find(|i| i.id == *id)?.shape {
            Shape::Text { frame, size, .. } => Some((Point { x: frame.x, y: frame.y }, size)),
            _ => None,
        }
    }

    /// Whether `pane` is in write focus.
    pub fn write_focused(&self, pane: PaneId) -> bool {
        self.focus.is_some_and(|f| f.pane == pane)
    }

    /// Gives the pane in write focus its view back once a text ended.
    pub(super) fn keep_focus(&mut self) {
        if self.focus.is_some_and(|f| f.ends != self.editing.ends()) {
            self.unfocus();
        }
    }

    /// Gives the pane in write focus, if any, the view it had, when it
    /// still has a view.
    fn unfocus(&mut self) {
        let Some(focus) = self.focus.take() else { return };
        if self.follow.view_of(focus.pane).is_some() {
            self.view(focus.pane).viewport = focus.before;
        }
    }
}

#[cfg(test)]
mod tests;
