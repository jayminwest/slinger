//! Where a new text starts (slinger-b16804): at a tap of the text tool, or
//! from the keyboard (`new-text`) at the pointer, or chained a line under
//! the text just ended, so notes follow one another without a tap.

use super::{Shapes, snap};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::editor::{Editing, Editor};
use crate::scene::glyphs;
use crate::scene::ink::Ink;

impl Shapes {
    /// Starts a new, empty text on `page` of `canvas` in `colour`, at the
    /// size set ([`Shapes::size`]), with the middle of its first line's
    /// left end at `at`, where a tap puts it. A text being typed ends
    /// first, to `ink`.
    pub fn start(&mut self, (canvas, page): (&str, &str), at: Point, colour: Rgb, ink: &mut Ink) {
        self.end(ink);
        let size = self.size();
        let half = glyphs::rows_frame(at, "", size, glyphs::UNWRAPPED).height / 2.0;
        self.editing = Some(Editing {
            canvas: canvas.to_string(),
            page: page.to_string(),
            at: snap(Point { x: at.x, y: at.y - half }),
            editor: Editor::default(),
            colour,
            size,
            column: glyphs::UNWRAPPED,
            item: None,
        });
    }

    /// Where the next text chains on, as [`Shapes::start`] takes it, with
    /// its canvas and page: its left end at the left of the text being
    /// typed, which ends now (to `ink`), or else of the last text ended,
    /// half a line under it. `None` when no text has ended yet.
    pub fn chained(&mut self, ink: &mut Ink) -> Option<(String, String, Point)> {
        self.end(ink);
        let (canvas, page, frame) = self.ended.clone()?;
        let left = Point { x: frame.x, y: frame.y + frame.height };
        let line = glyphs::rows_frame(left, "", self.size(), glyphs::UNWRAPPED).height;
        Some((canvas, page, Point { x: left.x, y: left.y + line }))
    }

    /// The frame of the text being typed, if any, in canvas units.
    pub fn typing_frame(&self) -> Option<Rect> {
        self.editing.as_ref().map(Editing::frame)
    }
}

#[cfg(test)]
mod tests;
