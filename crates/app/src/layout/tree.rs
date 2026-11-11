//! The pane tree: panes that tile a rectangle, split by ratios. Pure, and
//! every branch is unit-tested.

use super::Rect;

mod divider;

pub use divider::Divider;

/// How a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side: the first side on the left.
    Row,
    /// Stacked: the first side on top.
    Column,
}

/// A way to move on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// A pane in a [`Tree`]. A tree never gives one id to two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneId(u32);

/// The least share of a split each side keeps.
pub const MIN_RATIO: f64 = 0.1;
/// The most panes a tree holds. It also bounds how deep a split nests.
pub const MAX_PANES: usize = 64;

/// Panes that tile a rectangle. Each split divides its space between two
/// sides; its ratio is the first side's share, always in
/// `MIN_RATIO..=1 - MIN_RATIO`. A tree holds 1 to [`MAX_PANES`] panes:
/// closing the last one, or splitting past the most, changes nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    root: Node,
    /// The id the next new pane gets.
    next: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Pane(PaneId),
    Split { axis: Axis, ratio: f64, first: Box<Node>, second: Box<Node> },
}

impl Default for Tree {
    /// One pane.
    fn default() -> Self {
        Self { root: Node::Pane(PaneId(0)), next: 1 }
    }
}

impl Tree {
    /// Every pane, in order: depth first, the first side of a split before
    /// the second (left before right, top before bottom).
    pub fn panes(&self) -> Vec<PaneId> {
        self.rects(Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 })
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Each pane, in order, and its part of `area`. Edges fall on whole
    /// points when `area`'s do. A side too small for its share gets what is
    /// left, so no rectangle is ever wider or taller than `area`, and none is
    /// negative.
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let area = Rect { width: area.width.max(0.0), height: area.height.max(0.0), ..area };
        let mut out = Vec::new();
        self.root.place(area, &mut out);
        out
    }

    /// Splits pane `at` along `axis`, half and half: `at` keeps the first
    /// side and a new pane takes the second. The new pane, or `None` when
    /// `at` is not in the tree or the tree is full.
    pub fn split(&mut self, at: PaneId, axis: Axis) -> Option<PaneId> {
        self.split_by(at, axis, 0.5)
    }

    /// [`Tree::split`], with `ratio` of the space to `at`, held in the
    /// ratio's limits.
    pub fn split_by(&mut self, at: PaneId, axis: Axis, ratio: f64) -> Option<PaneId> {
        if self.panes().len() >= MAX_PANES {
            return None;
        }
        let new = PaneId(self.next);
        let leaf = self.root.leaf_mut(at)?;
        let (first, second) = (Box::new(Node::Pane(at)), Box::new(Node::Pane(new)));
        let ratio = if ratio.is_finite() { ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO) } else { 0.5 };
        *leaf = Node::Split { axis, ratio, first, second };
        self.next += 1;
        Some(new)
    }

    /// Removes pane `at`; its sibling takes the space the two shared.
    /// Returns whether it did: the last pane, or one not in the tree, stays.
    pub fn close(&mut self, at: PaneId) -> bool {
        self.root.remove(at)
    }

    /// Gives pane `at` `by` more of the nearest split it is in (less for a
    /// negative `by`), held in the ratio's limits. A lone pane, or one not in
    /// the tree, has no split to grow.
    pub fn grow(&mut self, at: PaneId, by: f64) {
        self.root.grow(at, by);
    }

    /// The pane after `at` in order, back to the first after the last. `at`
    /// itself when it is the only pane or not in the tree.
    pub fn next(&self, at: PaneId) -> PaneId {
        let panes = self.panes();
        panes.iter().position(|&p| p == at).map_or(at, |i| panes[(i + 1) % panes.len()])
    }

    /// The pane beside `at` in `direction` when the tree fills `area`: of the
    /// panes past that edge of `at` that share some of its span, the nearest,
    /// then the one that shares the most, then the first in order. `None` at
    /// the edge of `area`, or when `at` is not in the tree.
    pub fn toward(&self, at: PaneId, direction: Direction, area: Rect) -> Option<PaneId> {
        let rects = self.rects(area);
        let &(_, from) = rects.iter().find(|(id, _)| *id == at)?;
        let mut best: Option<(f64, f64, PaneId)> = None;
        for &(id, rect) in rects.iter().filter(|(id, _)| *id != at) {
            if let Some((gap, shared)) = from.beside(rect, direction)
                && best.is_none_or(|(g, s, _)| (gap, -shared) < (g, -s))
            {
                best = Some((gap, shared, id));
            }
        }
        best.map(|(_, _, id)| id)
    }
}

impl Node {
    fn place(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Self::Pane(id) => out.push((*id, area)),
            Self::Split { axis, ratio, first, second } => {
                let (a, b) = area.divide(*axis, *ratio);
                first.place(a, out);
                second.place(b, out);
            }
        }
    }

    /// The node that is pane `id`.
    fn leaf_mut(&mut self, id: PaneId) -> Option<&mut Self> {
        if *self == Self::Pane(id) {
            return Some(self);
        }
        match self {
            Self::Pane(_) => None,
            Self::Split { first, second, .. } => first.leaf_mut(id).or_else(|| second.leaf_mut(id)),
        }
    }

    /// Removes pane `id` from under this node: the split that holds it
    /// becomes its sibling.
    fn remove(&mut self, id: PaneId) -> bool {
        let Self::Split { first, second, .. } = self else { return false };
        let keep = if **first == Self::Pane(id) {
            second
        } else if **second == Self::Pane(id) {
            first
        } else {
            return first.remove(id) || second.remove(id);
        };
        *self = std::mem::replace(&mut **keep, Self::Pane(id));
        true
    }

    /// `None` when pane `id` is not under this node; `Some(false)` when this
    /// is the pane, so the split above it still has to grow; `Some(true)`
    /// once a split has.
    fn grow(&mut self, id: PaneId, by: f64) -> Option<bool> {
        match self {
            Self::Pane(p) => (*p == id).then_some(false),
            Self::Split { ratio, first, second, .. } => {
                let (done, sign) = match first.grow(id, by) {
                    Some(done) => (done, 1.0),
                    None => (second.grow(id, by)?, -1.0),
                };
                if !done {
                    // Thousandths, so steps add up to the share they name.
                    let wanted = ((*ratio + sign * by) * 1000.0).round() / 1000.0;
                    *ratio = wanted.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
                }
                Some(true)
            }
        }
    }
}

impl Rect {
    /// This rectangle, of no negative size, cut in two along `axis`: the
    /// first part `ratio` of it, rounded to a whole point.
    fn divide(self, axis: Axis, ratio: f64) -> (Self, Self) {
        let cut = |length: f64| {
            let first = (length * ratio).round().min(length);
            (first, length - first)
        };
        match axis {
            Axis::Row => {
                let (a, b) = cut(self.width);
                (Self { width: a, ..self }, Self { x: self.x + a, width: b, ..self })
            }
            Axis::Column => {
                let (a, b) = cut(self.height);
                (Self { height: a, ..self }, Self { y: self.y + a, height: b, ..self })
            }
        }
    }

    /// How far `other` is past this rectangle's edge in `direction`, and how
    /// much of this one's span across that direction the two share; `None`
    /// when it is not past the edge or they share none of the span.
    fn beside(self, other: Self, direction: Direction) -> Option<(f64, f64)> {
        let shared = |a: f64, a_len: f64, b: f64, b_len: f64| (a + a_len).min(b + b_len) - a.max(b);
        let across_x = shared(self.x, self.width, other.x, other.width);
        let across_y = shared(self.y, self.height, other.y, other.height);
        let (gap, span) = match direction {
            Direction::Left => (self.x - (other.x + other.width), across_y),
            Direction::Right => (other.x - (self.x + self.width), across_y),
            Direction::Up => (self.y - (other.y + other.height), across_x),
            Direction::Down => (other.y - (self.y + self.height), across_x),
        };
        (gap >= 0.0 && span > 0.0).then_some((gap, span))
    }
}

#[cfg(test)]
mod tests;
