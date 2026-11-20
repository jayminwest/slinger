//! Folding boxes on the canvas panes (docs/design/fold.md): `fold` folds
//! each selected box to its miniature, or unfolds a folded one, each as
//! one undo step through the ink, in turn with the strokes. An unfold
//! whose old room is taken writes nothing and shows the box as a **peek**
//! instead: its contents at full size in its old room, over the page, in
//! the pane the command ran in, until escape or a press outside it (which
//! does nothing else). A press inside does nothing: a peek writes nothing.
//!
//! The pen's way is the **fold mark**: the top right corner of a box,
//! turned down. Every box shows it, open or folded (slinger-125f94): a fold is as
//! near the pen as an unfold. A press on it with any tool but the eraser
//! folds or unfolds that box, and does nothing else; the selection's
//! corner handle, right at the corner, still takes a press there.
//!
//! What a pane draws for its folded boxes is [`drawn`]: each one's
//! miniature, every box's fold mark, and the peek over everything.

use std::borrow::Cow;

use super::Canvases;
use crate::commands::Chord;
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect};
use crate::mirror::{LINE_WIDTH, Mirror};
use crate::scene::canvas::{Fill, Item, ItemId, Shape};
use crate::scene::ink::Change;

/// The draw ids of fold marks: this plus the box's id. Above every item's
/// id, below the ink's own ([`crate::scene::ink::PENDING`]).
const MARK: u64 = 1 << 60;
/// The draw id of a peek's paper.
const PEEK: ItemId = ItemId(MARK - 1);
/// How long a fold mark's sides are, at most, in canvas units.
const MARK_SIZE: f64 = 12.0;
/// How near a box's corner, in screen points, a press is the selection's
/// handle's and not the mark's: half a handle.
const CORNER: f64 = 4.5;

impl Canvases {
    /// Folds each box of the selection on `page` of `canvas`, or unfolds it
    /// when folded, each one undo step; an unfold whose room is taken peeks
    /// in `pane`. False when the selection holds no box shown there.
    pub(super) fn fold_selection(&mut self, pane: PaneId, here: (&str, &str)) -> bool {
        let Some(s) = self.select.selection().filter(|s| (&*s.canvas, &*s.page) == here) else {
            return false;
        };
        let Some(mirror) = self.follow.mirror(here.0, here.1) else { return false };
        let boxes: Vec<(ItemId, bool)> =
            s.ids.iter().filter_map(|&id| Some((id, mirror.box_folded(id)?))).collect();
        self.fold_boxes(pane, here, &boxes)
    }

    /// Folds each box of `boxes` on `page` of `canvas`, or unfolds it when
    /// it says so, each one undo step. False for none.
    pub(super) fn fold_boxes(
        &mut self,
        pane: PaneId,
        here: (&str, &str),
        boxes: &[(ItemId, bool)],
    ) -> bool {
        for &(id, open) in boxes {
            let (canvas, on) = (here.0.to_string(), here.1.to_string());
            let fold = Change::Fold { canvas: canvas.clone(), page: on.clone(), id, open };
            let key = self.ink.push_keyed(fold);
            if open {
                self.follow.unfolding(key, pane, (canvas, on), id);
            }
        }
        !boxes.is_empty() && {
            self.send();
            true
        }
    }

    /// The box whose fold mark a press at `at` on `page` of `canvas`, at
    /// `zoom`, is on, of `items` (the page as shown), and whether it is
    /// folded: the press folds or unfolds it ([`Canvases::fold_boxes`]).
    /// `None`, and the press goes on, when it is on none.
    pub(super) fn mark_under(
        &self,
        here: (&str, &str),
        (at, zoom): (Point, f64),
        items: &[Cow<'_, Item>],
    ) -> Option<(ItemId, bool)> {
        let mirror = self.follow.mirror(here.0, here.1)?;
        let on = |i: &&Cow<'_, Item>| match i.shape {
            Shape::Box { frame, .. } => {
                mirror.box_folded(i.id).is_some() && on_mark(frame, at, zoom)
            }
            _ => false,
        };
        let id = items.iter().rev().find(on)?.id;
        Some((id, mirror.box_folded(id)?))
    }

    /// Escape closes the peek. False when there is none, or `chord` is
    /// not escape.
    pub fn peek_key(&mut self, chord: Chord) -> bool {
        self.follow.peek_key(chord)
    }
}

/// Whether `at` is on the fold mark of a box in `frame`, at `zoom`: in
/// its turned-down corner, but not at the corner itself.
fn on_mark(frame: Rect, at: Point, zoom: f64) -> bool {
    let d = side(frame);
    let (right, top) = (frame.x + frame.width, frame.y);
    let corner = (at.x - right).hypot(at.y - top) <= CORNER / zoom;
    !corner && contains(Rect { x: right - d, y: top, width: d, height: d }, at)
}

/// How long the sides of a box's fold mark are.
fn side(frame: Rect) -> f64 {
    MARK_SIZE.min(frame.width / 4.0).min(frame.height / 4.0)
}

/// Whether `r` holds `p`, edges included.
pub(super) fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.y >= r.y && p.x <= r.x + r.width && p.y <= r.y + r.height
}

/// `items`, a pane's page as shown from `mirror`, with each folded box's
/// miniature after it, the fold mark of each of its boxes, open or folded,
/// and the box `peek` names, if any, over everything at full size in its
/// old room on `theme`'s paper. A box shown away from where the server has
/// it (dragged, resized) draws its miniature where it shows.
pub(super) fn drawn<'a>(
    items: Vec<Cow<'a, Item>>,
    mirror: &'a Mirror,
    peek: Option<ItemId>,
    theme: &Theme,
) -> Vec<Cow<'a, Item>> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let (id, colour) = (item.id, item.colour);
        let Shape::Box { frame, .. } = item.shape else {
            out.push(item);
            continue;
        };
        let Some(mini) = mirror.miniature(id) else {
            out.push(item);
            if mirror.box_folded(id) == Some(false) {
                out.push(Cow::Owned(Item { id: ItemId(MARK + id.0), colour, shape: mark(frame) }));
            }
            continue;
        };
        let moved = mirror.item(id).is_none_or(|stored| stored.shape != item.shape);
        out.push(item);
        if peek != Some(id) {
            if moved {
                out.extend(mirror.inside(id, frame).into_iter().map(Cow::Owned));
            } else {
                out.extend(mini.iter().map(Cow::Borrowed));
            }
        }
        out.push(Cow::Owned(Item { id: ItemId(MARK + id.0), colour, shape: mark(frame) }));
    }
    let room = peek.and_then(|id| Some((id, mirror.room(id)?)));
    if let Some((id, room)) = room {
        let colour = mirror.item(id).map_or(theme.text, |i| i.colour);
        let fill = Some(Fill { colour: theme.background, alpha: 1.0 });
        let paper = Shape::Panel { frame: room, fill, width: LINE_WIDTH, dashed: false };
        out.push(Cow::Owned(Item { id: PEEK, colour, shape: paper }));
        out.extend(mirror.inside(id, room).into_iter().map(Cow::Owned));
    }
    out
}

/// A box's fold mark: its top right corner turned down, as a line.
fn mark(frame: Rect) -> Shape {
    let d = side(frame);
    let (right, top) = (frame.x + frame.width, frame.y);
    let points = vec![
        Point { x: right - d, y: top },
        Point { x: right - d, y: top + d },
        Point { x: right, y: top + d },
    ];
    Shape::Stroke { points, width: LINE_WIDTH / 2.0 }
}

#[cfg(test)]
mod tests;
