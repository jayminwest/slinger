//! The dividers between panes (SPEC section 11: the pen resizes panes):
//! where the pen grabs one, a band [`REACH`] points either side of it, wide
//! enough for a pen tip, and the share a divider dragged to a point gives
//! its split. Pure, and every branch is unit-tested.
//!
//! A divider is named by the pane before it: the last pane of its split's
//! first side, so it is on that pane's right or below it. Every divider is
//! after exactly one pane, and the server names it the same way
//! (`pane_resize`, docs/service.md).

use super::{Axis, MIN_RATIO, Node, PaneId, Tree};
use crate::layout::{Point, Rect};

/// How far from a divider the pen still grabs it, in points, each side.
pub const REACH: f64 = 8.0;

/// The edge between the two sides of one split.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Divider {
    /// The pane before it: the last of the split's first side.
    pub before: PaneId,
    pub axis: Axis,
    /// The space the split divides.
    pub area: Rect,
    /// Where the edge is: its x for a row, its y for a column.
    pub at: f64,
}

impl Divider {
    /// The share of its split the first side gets with the divider dragged
    /// to `to`: in thousandths, held in the ratio's limits.
    pub fn ratio(&self, to: Point) -> f64 {
        let (offset, length) = match self.axis {
            Axis::Row => (to.x - self.area.x, self.area.width),
            Axis::Column => (to.y - self.area.y, self.area.height),
        };
        let share = if length > 0.0 { offset / length } else { 0.5 };
        ((share * 1000.0).round() / 1000.0).clamp(MIN_RATIO, 1.0 - MIN_RATIO)
    }

    /// How far `p` is from the edge, when it is beside the edge's span and
    /// no further than [`REACH`].
    fn reach(&self, p: Point) -> Option<f64> {
        let a = self.area;
        let (gap, along, start, length) = match self.axis {
            Axis::Row => ((p.x - self.at).abs(), p.y, a.y, a.height),
            Axis::Column => ((p.y - self.at).abs(), p.x, a.x, a.width),
        };
        (gap <= REACH && (start..=start + length).contains(&along)).then_some(gap)
    }
}

impl Tree {
    /// Every divider when the tree fills `area`, outer splits first.
    pub fn dividers(&self, area: Rect) -> Vec<Divider> {
        let area = Rect { width: area.width.max(0.0), height: area.height.max(0.0), ..area };
        let mut out = Vec::new();
        self.root.dividers(area, &mut out);
        out
    }

    /// The divider under `p` when the tree fills `area`: of those within
    /// [`REACH`] of it, the nearest, and the inner one where two meet.
    /// `None` away from every divider.
    pub fn divider_at(&self, area: Rect, p: Point) -> Option<Divider> {
        let mut best: Option<(f64, Divider)> = None;
        for divider in self.dividers(area) {
            if let Some(gap) = divider.reach(p)
                && best.is_none_or(|(g, _)| gap <= g)
            {
                best = Some((gap, divider));
            }
        }
        best.map(|(_, divider)| divider)
    }

    /// Gives the split of the divider after pane `before` the share
    /// `ratio`, held in the ratio's limits. Returns false, and changes
    /// nothing, when no divider is after it (the last pane, or one not in
    /// the tree) or `ratio` is not a number.
    pub fn set_ratio(&mut self, before: PaneId, ratio: f64) -> bool {
        ratio.is_finite() && self.root.set_ratio(before, ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO))
    }

    /// The divider [`Tree::grow`] would move for pane `at` and `by`, as the
    /// pane before it and its new share; `None` when the grow changes
    /// nothing. The tree itself does not change.
    pub fn grown(&self, at: PaneId, by: f64) -> Option<(PaneId, f64)> {
        let mut grown = self.clone();
        grown.grow(at, by);
        let (mut was, mut now) = (Vec::new(), Vec::new());
        self.root.shares(&mut was);
        grown.root.shares(&mut now);
        was.into_iter().zip(now).find(|(a, b)| a != b).map(|(_, b)| b)
    }
}

impl Node {
    /// The last pane under this node.
    fn last(&self) -> PaneId {
        match self {
            Self::Pane(id) => *id,
            Self::Split { second, .. } => second.last(),
        }
    }

    fn dividers(&self, area: Rect, out: &mut Vec<Divider>) {
        let Self::Split { axis, ratio, first, second } = self else { return };
        let (a, b) = area.divide(*axis, *ratio);
        let at = match axis {
            Axis::Row => b.x,
            Axis::Column => b.y,
        };
        out.push(Divider { before: first.last(), axis: *axis, area, at });
        first.dividers(a, out);
        second.dividers(b, out);
    }

    fn set_ratio(&mut self, before: PaneId, to: f64) -> bool {
        let Self::Split { ratio, first, second, .. } = self else { return false };
        if first.last() == before {
            *ratio = to;
            return true;
        }
        first.set_ratio(before, to) || second.set_ratio(before, to)
    }

    /// Each split's divider, as the pane before it and its share, in order.
    fn shares(&self, out: &mut Vec<(PaneId, f64)>) {
        let Self::Split { ratio, first, second, .. } = self else { return };
        out.push((first.last(), *ratio));
        first.shares(out);
        second.shares(out);
    }
}

#[cfg(test)]
mod tests;
