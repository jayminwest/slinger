//! The pane tree: panes that tile a rectangle, split by ratios. Pure, and
//! every branch is unit-tested.

use std::str::SplitAsciiWhitespace;

use super::Rect;

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
        if self.panes().len() >= MAX_PANES {
            return None;
        }
        let new = PaneId(self.next);
        let leaf = self.root.leaf_mut(at)?;
        let (first, second) = (Box::new(Node::Pane(at)), Box::new(Node::Pane(new)));
        *leaf = Node::Split { axis, ratio: 0.5, first, second };
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

/// The text a workspace keeps.
#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
impl Tree {
    /// The tree as text: each pane is the word `leaf` writes for it (one
    /// word, with no space in it), and each split is `row` or `column`, its
    /// ratio, then its first side and its second. Words are joined by one
    /// space: `row 0.5 a column 0.25 b c`.
    pub fn encode(&self, leaf: impl Fn(PaneId) -> String) -> String {
        let mut words = Vec::new();
        self.root.write(&leaf, &mut words);
        words.join(" ")
    }

    /// The tree [`encode`](Self::encode) wrote. Each pane's word goes to
    /// `leaf` with the id the pane gets, in order; `leaf` returns false to
    /// refuse it. A ratio past its limits is held in them. `None` for text
    /// that is not exactly one tree, a ratio that is not a finite number,
    /// more than [`MAX_PANES`] panes, or a word `leaf` refuses.
    pub fn decode(text: &str, mut leaf: impl FnMut(PaneId, &str) -> bool) -> Option<Self> {
        let mut words = text.split_ascii_whitespace();
        let mut next = 0;
        let root = Node::read(&mut words, 0, &mut next, &mut leaf)?;
        words.next().is_none().then_some(Self { root, next })
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
                    // Thousandths, so the ratio reads back as written.
                    let wanted = ((*ratio + sign * by) * 1000.0).round() / 1000.0;
                    *ratio = wanted.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
                }
                Some(true)
            }
        }
    }
}

/// The text a workspace keeps.
#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
impl Node {
    fn write(&self, leaf: &dyn Fn(PaneId) -> String, words: &mut Vec<String>) {
        match self {
            Self::Pane(id) => words.push(leaf(*id)),
            Self::Split { axis, ratio, first, second } => {
                let name = match axis {
                    Axis::Row => "row",
                    Axis::Column => "column",
                };
                words.push(name.to_string());
                words.push(ratio.to_string());
                first.write(leaf, words);
                second.write(leaf, words);
            }
        }
    }

    /// One node from `words`, `depth` splits down. `next` counts the panes
    /// read so far. A tree of at most `MAX_PANES` panes nests fewer splits
    /// than that, so deeper text is refused before it can use up the stack.
    fn read(
        words: &mut SplitAsciiWhitespace<'_>,
        depth: usize,
        next: &mut u32,
        leaf: &mut dyn FnMut(PaneId, &str) -> bool,
    ) -> Option<Self> {
        let word = words.next()?;
        let axis = match word {
            "row" => Axis::Row,
            "column" => Axis::Column,
            _ => {
                let id = PaneId(*next);
                *next += 1;
                return (*next as usize <= MAX_PANES && leaf(id, word)).then_some(Self::Pane(id));
            }
        };
        if depth + 1 >= MAX_PANES {
            return None;
        }
        let ratio: f64 = words.next()?.parse().ok()?;
        if !ratio.is_finite() {
            return None;
        }
        let first = Box::new(Self::read(words, depth + 1, next, leaf)?);
        let second = Box::new(Self::read(words, depth + 1, next, leaf)?);
        Some(Self::Split { axis, ratio: ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO), first, second })
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
