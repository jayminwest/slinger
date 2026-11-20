//! A workspace's panes (SPEC D12, section 7): what each pane shows and how
//! the panes tile the window, as the server keeps them in the workspace's
//! `[layout]` (docs/workspace.md). The pane verbs change them here, so the
//! server, the app and agents share one arrangement.
//!
//! This module owns the layout's text (`text.rs`): `version = `[`VERSION`],
//! `panes = "<text>"`, the tree in prefix order, and `next = <id>`. A pane
//! is `canvas:<id>@<pane id>` or `ui:<name>@<pane id>`; a split is `row`
//! (side by side) or `column` (stacked), the first side's share, its first
//! side and its second. A pane is named by its number: 1 for the first in
//! order (depth first, the first side of a split before the second), which
//! is left to right and top to bottom. Its number moves as panes open and
//! close; its id does not: an id is given once in a layout and never again.

use crate::record::PaneMoved;
use crate::{Error, Result};

/// The version of the layout text this module reads and writes.
pub const VERSION: u32 = 2;

/// The most panes a layout holds. It also bounds how deep a split nests.
pub const MAX_PANES: usize = 64;

/// The least share of a split each side keeps; a share past the limits is
/// held in them.
pub const MIN_RATIO: f64 = 0.1;

/// What a pane shows (D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A canvas, by its id.
    Canvas,
    /// An agent's UI, by its name.
    Ui,
}

impl Kind {
    /// The kind's name in the layout text and on the wire.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::Ui => "ui",
        }
    }
}

/// One pane: its id, a kind and its target, a canvas id or a UI name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    /// The pane's id in its layout: 1 or more, unique in the layout and
    /// kept while the pane is open. 0 for a pane no layout holds yet: the
    /// layout gives it one.
    pub id: u32,
    pub kind: Kind,
    pub target: String,
}

impl Pane {
    /// A pane that shows `target`, with no id yet.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for an empty target or one with white space in it:
    /// no canvas id or UI name has any.
    pub fn new(kind: Kind, target: &str) -> Result<Self> {
        if target.is_empty() || target.contains(char::is_whitespace) {
            return Err(Error::Invalid {
                message: format!("{target:?} is not a {} to show in a pane", kind.name()),
                hint: Some("name a canvas id or a UI name, with no spaces".into()),
            });
        }
        Ok(Self { id: 0, kind, target: target.to_string() })
    }

    /// What the pane shows, as the record names it: `canvas:main`. The
    /// layout text adds the id (`canvas:main@3`).
    #[must_use]
    pub fn word(&self) -> String {
        format!("{}:{}", self.kind.name(), self.target)
    }
}

/// How a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side: the first side on the left.
    Row,
    /// Stacked: the first side on top.
    Column,
}

/// One node of the tree: a pane, or a split of its space in two.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Pane(Pane),
    /// `ratio` is the first side's share, in `MIN_RATIO..=1 - MIN_RATIO`.
    Split {
        axis: Axis,
        ratio: f64,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// A pane's place in the window as a share of it: 0 to 1 on each axis,
/// origin at the top left. The server has no window, so it has no points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The panes of a workspace: 1 to [`MAX_PANES`] of them. Closing the last
/// one, or one that is not there, changes nothing. Each pane has its own
/// id; `next` is the id the next pane opened gets.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    root: Node,
    next: u32,
}

impl Layout {
    /// One pane, showing `pane`, with id 1.
    #[must_use]
    pub fn new(pane: Pane) -> Self {
        Self { root: Node::Pane(Pane { id: 1, ..pane }), next: 2 }
    }

    /// The id the next pane opened gets: more than every id the layout has
    /// given.
    #[must_use]
    pub fn next(&self) -> u32 {
        self.next
    }

    /// The tree, to walk.
    #[must_use]
    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Every pane, in order: pane `n` is at `n - 1`.
    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        let mut out = Vec::new();
        self.root.leaves(&mut out);
        out
    }

    /// Opens `pane` beside pane number `beside`: that pane keeps the first
    /// half of its space and the new pane takes the second, to its right
    /// for [`Axis::Row`] or below it for [`Axis::Column`]. With no `beside`,
    /// the new pane takes that half of the whole window. The new pane gets
    /// the next id, whatever id `pane` had. Returns the new pane's number
    /// and the `pane_moved` the change records.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when there is no pane `beside`;
    /// [`Error::Invalid`] when the layout already holds [`MAX_PANES`].
    pub fn open(
        &mut self,
        pane: Pane,
        beside: Option<usize>,
        axis: Axis,
    ) -> Result<(usize, PaneMoved)> {
        let count = self.panes().len();
        if count >= MAX_PANES {
            return Err(Error::Invalid {
                message: format!("the layout holds {MAX_PANES} panes, the most it can"),
                hint: Some("close a pane first".into()),
            });
        }
        let word = pane.word();
        let (id, next) = self.fresh();
        let pane = Pane { id, ..pane };
        let (at, number) = match beside {
            None => (&mut self.root, count + 1),
            Some(n) => match self.root.leaf_mut(n) {
                Some(leaf) => (leaf, n + 1),
                None => {
                    return Err(Error::NotFound {
                        what: format!("pane {n}"),
                        hint: Some(format!("the panes are 1 to {count}; list them first")),
                    });
                }
            },
        };
        let old = std::mem::replace(at, Node::Pane(pane.clone()));
        let (first, second) = (Box::new(old), Box::new(Node::Pane(pane)));
        *at = Node::Split { axis, ratio: 0.5, first, second };
        self.next = next;
        Ok((number, self.moved(number, word)))
    }

    /// Closes pane number `number`; its sibling takes the space the two
    /// shared. Returns the `pane_moved` the change records: the closed
    /// pane, with no size. `None` when nothing changed: there is no such
    /// pane, or it is the last one, which stays.
    pub fn close(&mut self, number: usize) -> Option<PaneMoved> {
        let word = self.panes().get(number.checked_sub(1)?)?.word();
        let gone = PaneMoved { pane: word, x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
        self.root.remove(number).then_some(gone)
    }

    /// Moves the border after pane number `number` (on its right, or below
    /// it) so the split it divides gives its first side `ratio`, held in
    /// the ratio's limits. That split is the one whose first side ends with
    /// the pane, so every border is after exactly one pane. Returns the
    /// `pane_moved` the change records: the pane, with its new frame.
    /// `None` when nothing changed: there is no such pane, it is the last
    /// one (no border comes after it), the split already has that share, or
    /// `ratio` is not a finite number.
    pub fn resize(&mut self, number: usize, ratio: f64) -> Option<PaneMoved> {
        if !ratio.is_finite() {
            return None;
        }
        let word = self.panes().get(number.checked_sub(1)?)?.word();
        let ratio = ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        self.root.resize(number, ratio)?.then(|| self.moved(number, word))
    }

    /// Each pane's frame, in order.
    #[must_use]
    pub fn frames(&self) -> Vec<Frame> {
        let mut out = Vec::new();
        let window = Frame { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
        self.root.place(window, &mut out);
        out
    }

    /// An id no pane has, and the next id after it is given. Past the last
    /// u32, which no real layout reaches, the least id no pane has now.
    fn fresh(&self) -> (u32, u32) {
        if let Some(after) = self.next.checked_add(1) {
            return (self.next, after);
        }
        let taken: Vec<u32> = self.panes().iter().map(|p| p.id).collect();
        // At most MAX_PANES ids are taken, so one of the first past that is free.
        let free = (1..).take(MAX_PANES + 1).find(|id| !taken.contains(id));
        (free.unwrap_or(1), self.next)
    }

    fn moved(&self, number: usize, pane: String) -> PaneMoved {
        let frame = self.frames()[number - 1];
        PaneMoved { pane, x: frame.x, y: frame.y, width: frame.width, height: frame.height }
    }
}

impl Node {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a Pane>) {
        match self {
            Self::Pane(pane) => out.push(pane),
            Self::Split { first, second, .. } => {
                first.leaves(out);
                second.leaves(out);
            }
        }
    }

    fn count(&self) -> usize {
        match self {
            Self::Pane(_) => 1,
            Self::Split { first, second, .. } => first.count() + second.count(),
        }
    }

    /// The node that is pane number `n` under this node, counting from 1.
    fn leaf_mut(&mut self, n: usize) -> Option<&mut Self> {
        match self {
            Self::Pane(_) => (n == 1).then_some(self),
            Self::Split { first, second, .. } => {
                let before = first.count();
                if n <= before { first.leaf_mut(n) } else { second.leaf_mut(n - before) }
            }
        }
    }

    /// Removes pane number `n` from under this node: the split that holds
    /// it becomes its sibling.
    fn remove(&mut self, n: usize) -> bool {
        let Self::Split { first, second, .. } = self else { return false };
        let before = first.count();
        let keep = if n == 1 && matches!(**first, Self::Pane(_)) {
            second
        } else if n == before + 1 && matches!(**second, Self::Pane(_)) {
            first
        } else if n <= before {
            return first.remove(n);
        } else {
            return second.remove(n - before);
        };
        let gone = Self::Pane(Pane { id: 0, kind: Kind::Canvas, target: String::new() });
        *self = std::mem::replace(&mut **keep, gone);
        true
    }

    /// Sets to `ratio` the share of the split under this node whose first
    /// side ends with pane number `n`. `None` when there is none;
    /// `Some(changed)` once found.
    fn resize(&mut self, n: usize, ratio: f64) -> Option<bool> {
        let Self::Split { ratio: share, first, second, .. } = self else { return None };
        let before = first.count();
        match n.cmp(&before) {
            std::cmp::Ordering::Equal => {
                let changed = (*share - ratio).abs() > f64::EPSILON;
                *share = ratio;
                Some(changed)
            }
            std::cmp::Ordering::Less => first.resize(n, ratio),
            std::cmp::Ordering::Greater => second.resize(n - before, ratio),
        }
    }

    fn place(&self, area: Frame, out: &mut Vec<Frame>) {
        match self {
            Self::Pane(_) => out.push(area),
            Self::Split { axis: Axis::Row, ratio, first, second } => {
                let a = area.width * ratio;
                first.place(Frame { width: a, ..area }, out);
                second.place(Frame { x: area.x + a, width: area.width - a, ..area }, out);
            }
            Self::Split { axis: Axis::Column, ratio, first, second } => {
                let a = area.height * ratio;
                first.place(Frame { height: a, ..area }, out);
                second.place(Frame { y: area.y + a, height: area.height - a, ..area }, out);
            }
        }
    }
}

mod text;

#[cfg(test)]
mod tests;
