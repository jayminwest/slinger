//! A workspace's panes (SPEC D12, section 7): what each pane shows and how
//! the panes tile the window, as the server keeps them in the workspace's
//! `[layout]` (docs/workspace.md). The pane verbs change them here, so the
//! server, the app and agents share one arrangement.
//!
//! This module owns the layout's text: `version = `[`VERSION`] and
//! `panes = "<text>"`, the tree in prefix order. A pane is `canvas:<id>` or
//! `ui:<name>`; a split is `row` (side by side) or `column` (stacked), the
//! first side's share, its first side and its second. A pane is named by its
//! number: 1 for the first in order (depth first, the first side of a split
//! before the second), which is left to right and top to bottom.

use crate::record::PaneMoved;
use crate::{Error, Result};

/// The version of the layout text this module reads and writes.
pub const VERSION: u32 = 1;

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

/// One pane's content: a kind and its target, a canvas id or a UI name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub kind: Kind,
    pub target: String,
}

impl Pane {
    /// A pane that shows `target`.
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
        Ok(Self { kind, target: target.to_string() })
    }

    /// The pane's word in the layout text: `canvas:main`.
    #[must_use]
    pub fn word(&self) -> String {
        format!("{}:{}", self.kind.name(), self.target)
    }

    fn from_word(word: &str) -> Option<Self> {
        let (kind, target) = word.split_once(':')?;
        let kind = match kind {
            "canvas" => Kind::Canvas,
            "ui" => Kind::Ui,
            _ => return None,
        };
        Self::new(kind, target).ok()
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
/// one, or one that is not there, changes nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    root: Node,
}

impl Layout {
    /// One pane, showing `pane`.
    #[must_use]
    pub fn new(pane: Pane) -> Self {
        Self { root: Node::Pane(pane) }
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
    /// the new pane takes that half of the whole window. Returns the new
    /// pane's number and the `pane_moved` the change records.
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

    /// Each pane's frame, in order.
    #[must_use]
    pub fn frames(&self) -> Vec<Frame> {
        let mut out = Vec::new();
        let window = Frame { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
        self.root.place(window, &mut out);
        out
    }

    /// The layout text (see the module's comment).
    #[must_use]
    pub fn encode(&self) -> String {
        let mut words = Vec::new();
        self.root.write(&mut words);
        words.join(" ")
    }

    /// The layout [`encode`](Self::encode) wrote. `None` for text that is
    /// not exactly one tree, a share that is not a finite number, a pane that
    /// is not `canvas:<id>` or `ui:<name>`, or more than [`MAX_PANES`] panes.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        let mut words = text.split_ascii_whitespace();
        let mut count = 0;
        let root = Node::read(&mut words, 0, &mut count)?;
        words.next().is_none().then_some(Self { root })
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
        let gone = Self::Pane(Pane { kind: Kind::Canvas, target: String::new() });
        *self = std::mem::replace(&mut **keep, gone);
        true
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

    fn write(&self, words: &mut Vec<String>) {
        match self {
            Self::Pane(pane) => words.push(pane.word()),
            Self::Split { axis, ratio, first, second } => {
                let name = match axis {
                    Axis::Row => "row",
                    Axis::Column => "column",
                };
                words.push(name.to_string());
                words.push(ratio.to_string());
                first.write(words);
                second.write(words);
            }
        }
    }

    /// One node from `words`, `depth` splits down. `count` counts the panes
    /// read so far. A tree of at most `MAX_PANES` panes nests fewer splits
    /// than that, so deeper text is refused before it can use up the stack.
    fn read<'a>(
        words: &mut impl Iterator<Item = &'a str>,
        depth: usize,
        count: &mut usize,
    ) -> Option<Self> {
        let word = words.next()?;
        let axis = match word {
            "row" => Axis::Row,
            "column" => Axis::Column,
            _ => {
                *count += 1;
                if *count > MAX_PANES {
                    return None;
                }
                return Pane::from_word(word).map(Self::Pane);
            }
        };
        if depth + 1 >= MAX_PANES {
            return None;
        }
        let ratio: f64 = words.next()?.parse().ok()?;
        if !ratio.is_finite() {
            return None;
        }
        let first = Box::new(Self::read(words, depth + 1, count)?);
        let second = Box::new(Self::read(words, depth + 1, count)?);
        Some(Self::Split { axis, ratio: ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO), first, second })
    }
}

#[cfg(test)]
mod tests;
