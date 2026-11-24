//! What changed between two scenes of a pane: the screen box to repaint.
//! Pure.

use std::collections::HashMap;

use super::{Draw, ItemId, Mark};
use crate::layout::Rect;

/// The box around every draw that is in one list and not, unchanged, in the
/// other. A pen line that only grew at its end (the stroke under the pen)
/// counts only its new end, so a long stroke does not repaint whole at
/// every pen event.
pub(super) fn changed_between(old: &[Draw], new: &[Draw]) -> Option<Rect> {
    let before: HashMap<ItemId, &Draw> = old.iter().map(|d| (d.id, d)).collect();
    let after: HashMap<ItemId, &Draw> = new.iter().map(|d| (d.id, d)).collect();
    let gone = old.iter().filter(|d| !after.contains_key(&d.id)).map(|d| d.bounds);
    let came = new.iter().filter_map(|d| match before.get(&d.id) {
        None => Some(d.bounds),
        Some(was) if *was == d => None,
        Some(was) => Some(grown(was, d).unwrap_or_else(|| was.bounds.union(d.bounds))),
    });
    gone.chain(came).reduce(Rect::union)
}

/// The screen box of what changed between `old` and `new`, when both are
/// the same pen line from the same start and only its end changed (the
/// stroke under the pen). Smoothing changes only a line's last few curves
/// when a point comes, so the box holds the curves of each after those they
/// share. A cubic stays inside its points' box.
#[expect(clippy::float_cmp, reason = "the same line's width is the same value, from one product")]
fn grown(old: &Draw, new: &Draw) -> Option<Rect> {
    let (
        Mark::Stroke { start: a, curves: was, width: w, .. },
        Mark::Stroke { start: b, curves: now, width, .. },
    ) = (&old.mark, &new.mark)
    else {
        return None;
    };
    let shared = was.iter().zip(now).take_while(|(x, y)| x == y).count();
    let same = a == b && w == width && old.colour == new.colour;
    if !same {
        return None;
    }
    let from = if shared == 0 { *a } else { was[shared - 1][2] };
    let points = was[shared..].iter().chain(&now[shared..]).flatten().copied();
    // One point more on each side for the edge's smoothing.
    Rect::around(points.chain([from])).map(|r| r.outset(width / 2.0 + 1.0))
}
