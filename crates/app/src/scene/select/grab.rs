//! The unlock (docs/design/figures.md, "The unlock"): `unlock-figure`
//! grabs the figure under the pointer, the topmost locked item whose box
//! holds it ([`figure_at`]) and every item of its figure, as the
//! selection. A figure is held as one unit: a drag inside its box moves
//! all of it, the nudge keys move it, delete, duplicate, copy and cut take
//! all of it, and it shows no resize handles (a script decides sizes).
//!
//! Nothing is written. The grab is the selection it made: anything that
//! changes or ends that selection (escape, a press outside the figure,
//! select all, another pick) ends the grab, and the figure is locked again.
//! Every hit test still passes over locked items ([`free`]), so the figure
//! moves by the selection's own drag. A selection that is not a grab never
//! keeps a locked item ([`Select::keep`]).

use std::borrow::Cow;

use super::{Select, Selection};
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId, free, in_figure};
use crate::scene::ink::on_server;

impl Select {
    /// Grabs the figure under `at` on `page` of `canvas`, `reach` canvas
    /// units around it, over `items` (the page as shown): it is the
    /// selection, held as one. False, and nothing changes, with no locked
    /// item there or the pen down.
    pub fn unlock(
        &mut self,
        (canvas, page): (&str, &str),
        at: Point,
        reach: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let Some(hit) = figure_at(items, at, reach).filter(|_| self.gesture.is_none()) else {
            return false;
        };
        let ids: Vec<ItemId> =
            items.iter().filter(|i| on_server(i.id) && in_figure(i, hit)).map(|i| i.id).collect();
        let selection = Selection { canvas: canvas.into(), page: page.into(), ids };
        self.arriving = None;
        self.selection = Some(selection.clone());
        self.grab = Some(selection);
        true
    }

    /// Whether the selection is a grabbed figure: the one `unlock` made,
    /// unchanged.
    pub fn grabbed(&self) -> bool {
        self.grab.is_some() && self.grab == self.selection
    }
}

#[cfg(test)]
mod tests;

/// The topmost locked item of `items` whose box, `reach` bigger, holds
/// `at`: the figure `unlock-figure` grabs there.
pub fn figure_at<'b>(items: &'b [Cow<'_, Item>], at: Point, reach: f64) -> Option<&'b Item> {
    let locked = items.iter().rev().filter(|i| on_server(i.id) && !free(i));
    let mut under =
        locked.filter(|i| i.shape.bounds().is_some_and(|b| b.outset(reach).contains(at)));
    under.next().map(AsRef::as_ref)
}
