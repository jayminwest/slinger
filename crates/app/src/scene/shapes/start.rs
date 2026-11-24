//! Where a new text starts (slinger-b16804): at a tap of the text tool, or
//! from the keyboard (`new-text`) at the pointer, or chained a line under
//! the text just ended, so notes follow one another without a tap. The
//! hint under a typed text names that key.

use std::borrow::Cow;

use super::{HINT, Shapes, follow_frames, snap, text_at};
use crate::commands::Chord;
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, Shape};
use crate::scene::editor::{Editing, Editor, Opened};
use crate::scene::glyphs;
use crate::scene::ink::{Change, Ink};

impl Shapes {
    /// Starts a new, empty text on `page` of `canvas` in `colour`, at the
    /// size set ([`Shapes::size`]), with the middle of its first line's
    /// left end at `at`, where a tap puts it. A text being typed ends
    /// first, to `ink`.
    pub fn start(&mut self, (canvas, page): (&str, &str), at: Point, colour: Rgb, ink: &mut Ink) {
        self.end(ink);
        let size = self.size();
        let half = glyphs::rows_frame(at, "", size, glyphs::UNWRAPPED).height / 2.0;
        let at = snap(Point { x: at.x, y: at.y - half });
        self.editing = Some(Editing::new(canvas, page, at, colour, size));
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
        match text_at(items, at, reach).and_then(|item| edit((canvas, page), item, items)) {
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

    /// Whether `at`, a point on `page` of `canvas`, is inside the shown
    /// frame of the text being typed ([`Editing::bounds`]), where a press
    /// moves its caret.
    pub fn typing_at(&self, (canvas, page): (&str, &str), at: Point) -> bool {
        let here = |e: &&Editing| e.canvas == canvas && e.page == page;
        self.editing.as_ref().filter(here).is_some_and(|e| e.bounds().contains(at))
    }
}

/// `item`, a text of `items` on `page` of `canvas`, opened to be edited,
/// its caret at its end, with the arrows of `items` bound to it; `None`
/// when it is not a text.
pub(super) fn edit(
    (canvas, page): (&str, &str),
    item: &Item,
    items: &[Cow<'_, Item>],
) -> Option<Editing> {
    let Shape::Text { frame, text, size, column } = &item.shape else { return None };
    let column = column.unwrap_or(glyphs::UNWRAPPED);
    // Each bound arrow as it is: its item's frame, as yet, stays.
    let arrows = follow_frames(items, &[(item.id, *frame, *frame)]);
    let (id, text, size, frame) = (item.id, text.clone(), *size, *frame);
    Some(Editing {
        canvas: canvas.to_string(),
        page: page.to_string(),
        at: Point { x: frame.x, y: frame.y },
        editor: Editor::new(text.clone()),
        colour: item.colour,
        size,
        column,
        item: Some(Opened { id, text, size, column, frame, arrows }),
    })
}

/// The one change that ending `editing` makes: a new text's `add`, an
/// edited text's `set`, or its `remove` when emptied; none for blank new
/// text or an unchanged edit. An edit that moved the text's frame (a size
/// key, typing that grows it, a column drag) takes the arrows bound to it
/// along in the same change, so one undo step (slinger-6fe682).
pub(super) fn ended(editing: Editing) -> Option<Change> {
    let (changed, frame) = (editing.changed(), editing.frame());
    let blank = editing.editor.text().trim().is_empty();
    let (size, column) = (editing.size, Some(editing.column));
    let shape = Shape::Text { frame, text: editing.editor.text().to_string(), size, column };
    let (canvas, page, colour) = (editing.canvas, editing.page, editing.colour);
    match editing.item {
        None if blank => None,
        None => Some(Change::Put { canvas, page, colour, shape }),
        Some(was) if blank => Some(Change::Remove { canvas, page, ids: vec![was.id] }),
        Some(_) if !changed => None,
        Some(Opened { id, frame: before, arrows, .. }) => {
            let bound: Vec<Cow<'_, Item>> = arrows.iter().map(Cow::Borrowed).collect();
            let mut follow = follow_frames(&bound, &[(id, before, frame)]);
            follow.retain(|arrow| !arrows.contains(arrow));
            if follow.is_empty() {
                return Some(Change::Set { canvas, page, id, shape });
            }
            follow.insert(0, Item::plain(id, colour, shape));
            Some(Change::Reshape { canvas, page, items: follow })
        }
    }
}

#[cfg(test)]
mod tests;
