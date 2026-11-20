//! The selection from the keyboard (slinger-e96ea3): select all, step to
//! the next or previous item, nudge the selection, and where a duplicate
//! lands. Pure: `canvases` hands in the focused pane's page as shown and
//! its zoom, and asks [`Keyed::of`] which of these a command is.
//!
//! - Select all: every item the select tool may take ([`takes`]) on the
//!   page, in drawing order.
//! - Next and previous: the items in reading order (by the top of their
//!   box, then its left; drawing order between equals). From the last item
//!   selected, one along, after the last back to the first; with nothing
//!   selected on the page, the first (or, going back, the last). The item
//!   comes into view ([`into_view`]).
//! - Nudge: the selection moves [`NUDGE`] screen points at any zoom, as one
//!   `move`, with the arrows bound to it following, as a drag's lift does.
//! - Duplicate: a copy of the selection lands [`DUPLICATE`] screen points
//!   down and right of it, as a paste lands, and is the selection once there.
//! - Resize: a size command with texts selected (and none being typed)
//!   gives each its size ([`Resize`]), as one `set`, top left kept, its
//!   column about as wide, on half a cell of the new size.
//! - One item: a find's jump selects the text it went to alone
//!   ([`Select::only`]), as next-item does, so the keys above act from it.

use std::borrow::Cow;

use super::{Select, Selection, bounds, takes};
use crate::commands::Command;
use crate::layout::{Point, Rect, Size, Viewport};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::glyphs;
use crate::scene::ink::{Change, Ink, on_server};
use crate::scene::shapes;
use crate::scene::tools::Resize;

/// How far a nudge moves the selection, in screen points.
pub const NUDGE: f64 = 10.0;
/// How far down and right of the selection its duplicate lands, in screen points.
pub const DUPLICATE: f64 = 20.0;

/// What a command does to the items of the focused pane's page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keyed {
    All,
    /// To the next item (true) or the previous.
    Step(bool),
    /// By this many screen points across and down.
    Nudge(f64, f64),
    Duplicate,
}

impl Keyed {
    /// What `command` does to the items, if it is one of these.
    pub const fn of(command: Command) -> Option<Keyed> {
        Some(match command {
            Command::SelectAll => Keyed::All,
            Command::NextItem => Keyed::Step(true),
            Command::PreviousItem => Keyed::Step(false),
            Command::NudgeLeft => Keyed::Nudge(-NUDGE, 0.0),
            Command::NudgeRight => Keyed::Nudge(NUDGE, 0.0),
            Command::NudgeUp => Keyed::Nudge(0.0, -NUDGE),
            Command::NudgeDown => Keyed::Nudge(0.0, NUDGE),
            Command::Duplicate => Keyed::Duplicate,
            _ => return None,
        })
    }
}

impl Select {
    /// Selects every item of `items` (`page` of `canvas` as shown) the
    /// select tool may take. False, and nothing changes, when there is none
    /// or the pen is down.
    pub fn all(&mut self, (canvas, page): (&str, &str), items: &[Cow<'_, Item>]) -> bool {
        let ids: Vec<ItemId> = ours(canvas, items).map(|i| i.id).collect();
        let took = self.gesture.is_none() && !ids.is_empty();
        if took {
            self.arriving = None;
            self.selection = Some(Selection { canvas: canvas.into(), page: page.into(), ids });
        }
        took
    }

    /// Selects the item after the last selected in reading order, or
    /// before it (`forward` false), alone, as the module says: its box.
    /// `None`, and nothing changes, when the page has no item or the pen is down.
    pub fn step(
        &mut self,
        (canvas, page): (&str, &str),
        items: &[Cow<'_, Item>],
        forward: bool,
    ) -> Option<Rect> {
        if self.gesture.is_some() {
            return None;
        }
        let mut order: Vec<(Rect, ItemId)> =
            ours(canvas, items).filter_map(|i| Some((i.shape.bounds()?, i.id))).collect();
        order.sort_by(|(a, _), (b, _)| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let here = self.selection.as_ref().filter(|s| s.canvas == canvas && s.page == page);
        let last = here.and_then(|s| s.ids.last());
        let at = last.and_then(|id| order.iter().position(|(_, i)| i == id));
        let n = order.len();
        let next = match (at, forward) {
            (Some(at), true) => (at + 1) % n,
            (Some(at), false) => (at + n - 1) % n,
            (None, true) => 0,
            (None, false) => n.checked_sub(1)?,
        };
        let (frame, id) = *order.get(next)?;
        self.only((canvas, page), id).then_some(frame)
    }

    /// Selects `id` alone on `page` of `canvas`, as next-item does. False,
    /// and nothing changes, while the pen is down.
    pub fn only(&mut self, (canvas, page): (&str, &str), id: ItemId) -> bool {
        let took = self.gesture.is_none();
        if took {
            self.arriving = None;
            self.selection =
                Some(Selection { canvas: canvas.into(), page: page.into(), ids: vec![id] });
        }
        took
    }

    /// Moves the selection by `by` canvas units (to the tenth, as the file
    /// keeps lengths, and never less than one) over `items`, its page as
    /// shown, as one change to `ink`. False with no selection, or the pen down.
    pub fn nudge(&self, by: Point, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        let Some(Selection { canvas, page, ids }) = self.selection.as_ref() else { return false };
        if self.gesture.is_some() {
            return false;
        }
        let tenth = |v: f64| match (v * 10.0).round() / 10.0 {
            t if t == 0.0 && v != 0.0 => 0.1f64.copysign(v),
            t => t,
        };
        let by = Point { x: tenth(by.x), y: tenth(by.y) };
        let follow = shapes::follow(items, ids, by);
        let (canvas, page, ids) = (canvas.clone(), page.clone(), ids.clone());
        ink.push(Change::Move { canvas, page, ids, by, follow });
        true
    }

    /// Gives each selected text of `items` (its page as shown) the size
    /// `resize` makes of its own, laid out again from its top left in its
    /// column, as one change to `ink`. False when the selection holds no
    /// text, or the pen is down: the command sets a new text's size.
    pub fn resize(&self, resize: Resize, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        let Some(Selection { canvas, page, ids }) = self.selection.as_ref() else { return false };
        let texts = items.iter().filter(|i| ids.contains(&i.id) && self.gesture.is_none());
        let mut held = false;
        let mut changed = Vec::new();
        for item in texts {
            let Shape::Text { frame, text, size, column } = &item.shape else { continue };
            held = true;
            let to = resize.of(*size);
            if to.total_cmp(size).is_ne() {
                let at = Point { x: frame.x, y: frame.y };
                let column = column.map(|c| glyphs::rewrapped(c, to));
                let frame = glyphs::rows_frame(at, text, to, column.unwrap_or(glyphs::UNWRAPPED));
                let shape = Shape::Text { frame, text: text.clone(), size: to, column };
                changed.push(Item { shape, ..item.as_ref().clone() });
            }
        }
        if !changed.is_empty() {
            let (canvas, page) = (canvas.clone(), page.clone());
            ink.push(Change::Reshape { canvas, page, items: changed });
        }
        held
    }

    /// Where the duplicate of the selection over `items` (its page as
    /// shown) at `zoom` centres, in canvas units; `None` without one.
    pub fn duplicate_at(&self, items: &[Cow<'_, Item>], zoom: f64) -> Option<Point> {
        let frame = bounds(items, &self.selection.as_ref()?.ids)?;
        let off = DUPLICATE / zoom;
        Some(Point { x: frame.x + frame.width / 2.0 + off, y: frame.y + frame.height / 2.0 + off })
    }
}

/// The items of `items` on `canvas` the select tool may take, in drawing order.
fn ours<'a>(canvas: &'a str, items: &'a [Cow<'_, Item>]) -> impl Iterator<Item = &'a Item> {
    items.iter().map(AsRef::as_ref).filter(move |i| on_server(i.id) && takes(canvas, i))
}

/// The pan, in screen points, that brings `frame` (canvas units) into a
/// pane of `size` looking through `viewport`: none when it shows whole,
/// else the pan that centres it.
pub fn into_view(viewport: Viewport, size: Size, frame: Rect) -> Option<(f64, f64)> {
    let seen = viewport.visible(size);
    let whole = seen.contains(Point { x: frame.x, y: frame.y })
        && seen.contains(Point { x: frame.x + frame.width, y: frame.y + frame.height });
    if whole {
        return None;
    }
    let mid = viewport
        .to_screen(Point { x: frame.x + frame.width / 2.0, y: frame.y + frame.height / 2.0 });
    Some((size.width / 2.0 - mid.x, size.height / 2.0 - mid.y))
}

#[cfg(test)]
mod tests;
