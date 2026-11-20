//! Where a new text starts (slinger-b16804): at a tap of the text tool, or
//! from the keyboard (`new-text`) at the pointer, or chained a line under
//! the text just ended, so notes follow one another without a tap. The
//! hint under a typed text names that key.

use std::borrow::Cow;

use super::{HINT, Shapes, snap, text_at};
use crate::commands::Chord;
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, Shape};
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

    /// As [`Shapes::start`], except that a text of `items` within `reach`
    /// of `at` opens to be edited, as a tap of the text tool on it does.
    pub fn start_at(
        &mut self,
        (canvas, page): (&str, &str),
        (at, reach): (Point, f64),
        items: &[Cow<'_, Item>],
        colour: Rgb,
        ink: &mut Ink,
    ) {
        match text_at(items, at, reach).and_then(|item| edit((canvas, page), item)) {
            Some(editing) => {
                self.end(ink);
                self.editing = Some(editing);
            }
            None => self.start((canvas, page), at, colour, ink),
        }
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

    /// The key `new-text` is on, if any: the hint under a typed text names it.
    pub fn set_next(&mut self, key: Option<Chord>) {
        self.next = key;
    }

    /// The hint under the text being typed: how it ends, and the key that
    /// starts the next, when there is one.
    pub(super) fn hint(&self) -> String {
        match self.next {
            Some(key) => format!("{HINT} · {} new text", key.glyphs()),
            None => HINT.to_string(),
        }
    }

    /// The frame of the text being typed, if any, in canvas units.
    pub fn typing_frame(&self) -> Option<Rect> {
        self.editing.as_ref().map(Editing::frame)
    }
}

/// `item`, a text on `page` of `canvas`, opened to be edited, its caret
/// at its end; `None` when it is not a text.
pub(super) fn edit((canvas, page): (&str, &str), item: &Item) -> Option<Editing> {
    let Shape::Text { frame, text, size, column } = &item.shape else { return None };
    Some(Editing {
        canvas: canvas.to_string(),
        page: page.to_string(),
        at: Point { x: frame.x, y: frame.y },
        editor: Editor::new(text.clone()),
        colour: item.colour,
        size: *size,
        column: column.unwrap_or(glyphs::UNWRAPPED),
        item: Some((item.id, text.clone())),
    })
}

#[cfg(test)]
mod tests;
