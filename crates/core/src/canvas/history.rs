//! Undo and redo, per author.
//!
//! Every change goes through [`History::apply`] as one step: a single
//! operation, or a group such as a lasso move or one erase gesture. For each
//! step the history keeps its inverse. [`History::undo`] applies that inverse
//! as new operations and returns them, so a log of operations stays
//! append-only and replays to the same canvas, undo included.
//!
//! Each author has their own steps. The user's undo never takes back what an
//! agent did, and an agent's undo never takes back the user's work.
//!
//! An undo or redo with nothing to do succeeds and changes nothing. When
//! someone else has changed the canvas since (for example, the user removed
//! an item the agent then tries to un-move), the parts of a step that no
//! longer apply are skipped; a step with no part left is dropped and the one
//! before it is undone instead.
//!
//! Each step has a mark, a number its caller names it by (the canvas file
//! uses the number of the step's first line). The file keeps on each line
//! where it stands ([`Step`]), so [`Rebuild`] builds the same history again
//! from the lines, and a restart of the daemon keeps what can be undone.

use std::collections::HashMap;

use super::{Action, Applied, Author, Canvas, Op, Time};
use crate::Result;

/// The most steps each author can undo. The oldest step goes first.
const DEPTH: usize = 1000;

/// The undo and redo steps of every author on one canvas. It lives beside
/// the canvas in memory, and [`Rebuild`] makes it again from the lines.
#[derive(Debug, Clone, Default)]
pub struct History {
    authors: HashMap<Author, Stacks>,
}

/// Where an operation stands in the undo history. A canvas file keeps it on
/// each line (`step`, `undoes`, `redoes`), so the history can be built again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// Not part of an undo step: nothing takes it back. A new canvas's
    /// button, an import, and every line from before the file kept steps.
    #[default]
    None,
    /// Part of a new step, with the step's mark.
    New(u64),
    /// Part of the undo of the step with this mark.
    Undo(u64),
    /// Part of the redo of the step with this mark: the undo it takes back.
    Redo(u64),
}

/// What an undo or redo did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Walked {
    /// The operations that did it, in order: none when there was nothing to do.
    pub ops: Vec<Op>,
    /// [`Step::Undo`] or [`Step::Redo`] of the step taken back; [`Step::None`]
    /// when there are no operations.
    pub step: Step,
}

#[derive(Debug, Clone, Default)]
struct Stacks {
    undo: Vec<Back>,
    redo: Vec<Back>,
}

/// A step's mark, and the actions that take it back, in the order to apply them.
#[derive(Debug, Clone)]
struct Back {
    mark: u64,
    actions: Vec<Action>,
}

#[derive(Clone, Copy)]
enum Way {
    Undo,
    Redo,
}

impl History {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies `actions` in order as one undo step of `by`, marked `mark`,
    /// and clears what `by` could redo. Either every action applies or none
    /// does.
    ///
    /// # Errors
    ///
    /// The first error of [`Canvas::apply`]; the canvas is then as it was.
    pub fn apply(
        &mut self,
        canvas: &mut Canvas,
        by: &Author,
        at: Time,
        mark: u64,
        actions: Vec<Action>,
    ) -> Result<Vec<Applied>> {
        let before = (actions.len() > 1).then(|| canvas.clone());
        let mut applied = Vec::with_capacity(actions.len());
        let mut back = Vec::new();
        for action in actions {
            match apply_one(canvas, Op { by: by.clone(), at, action }) {
                Ok((done, inverse)) => {
                    applied.push(done);
                    back.extend(inverse);
                }
                Err(e) => {
                    if let Some(before) = before {
                        *canvas = before;
                    }
                    return Err(e);
                }
            }
        }
        if !back.is_empty() {
            back.reverse();
            let stacks = self.authors.entry(by.clone()).or_default();
            push(&mut stacks.undo, Back { mark, actions: back });
            stacks.redo.clear();
        }
        Ok(applied)
    }

    /// Takes back the last step of `by` that still applies, as a step
    /// `by` can redo, marked `mark`.
    pub fn undo(&mut self, canvas: &mut Canvas, by: &Author, at: Time, mark: u64) -> Walked {
        self.walk(canvas, by, at, mark, Way::Undo)
    }

    /// Puts back the last step `by` undid, as a step `by` can undo, marked
    /// `mark`.
    pub fn redo(&mut self, canvas: &mut Canvas, by: &Author, at: Time, mark: u64) -> Walked {
        self.walk(canvas, by, at, mark, Way::Redo)
    }

    /// Whether `by` has a step to undo.
    #[must_use]
    pub fn can_undo(&self, by: &Author) -> bool {
        self.authors.get(by).is_some_and(|s| !s.undo.is_empty())
    }

    /// Whether `by` has a step to redo.
    #[must_use]
    pub fn can_redo(&self, by: &Author) -> bool {
        self.authors.get(by).is_some_and(|s| !s.redo.is_empty())
    }

    fn walk(&mut self, canvas: &mut Canvas, by: &Author, at: Time, mark: u64, way: Way) -> Walked {
        let Some(stacks) = self.authors.get_mut(by) else { return Walked::default() };
        let (from, to) = match way {
            Way::Undo => (&mut stacks.undo, &mut stacks.redo),
            Way::Redo => (&mut stacks.redo, &mut stacks.undo),
        };
        while let Some(step) = from.pop() {
            let mut ops = Vec::new();
            let mut back = Vec::new();
            for action in step.actions {
                let op = Op { by: by.clone(), at, action };
                if let Ok((_, Some(inverse))) = apply_one(canvas, op.clone()) {
                    ops.push(op);
                    back.push(inverse);
                }
            }
            if !back.is_empty() {
                back.reverse();
                push(to, Back { mark, actions: back });
                let step = match way {
                    Way::Undo => Step::Undo(step.mark),
                    Way::Redo => Step::Redo(step.mark),
                };
                return Walked { ops, step };
            }
        }
        Walked::default()
    }
}

/// Builds a [`History`] again from the operations of a canvas file, each
/// with the [`Step`] its line states, as replay applies them in order. The
/// lines of one step are next to each other, by one author, with one
/// [`Step`]; a step's mark is its first line's number.
///
/// An undo that found nothing to take back wrote no line, so the steps it
/// dropped are back after a rebuild; an undo of them skips what no longer
/// applies, as it did.
#[derive(Debug, Default)]
pub(crate) struct Rebuild {
    history: History,
    /// The step the last lines were part of, until a line of another one.
    open: Option<(Author, Step, Back)>,
}

impl Rebuild {
    /// Applies `op`, from line `n`, which stands at `step`. Returns whether
    /// the canvas took it; one it refuses changes nothing (SPEC B4).
    pub(crate) fn apply(&mut self, canvas: &mut Canvas, n: u64, op: Op, step: Step) -> bool {
        let same = self.open.as_ref().is_some_and(|(by, s, _)| *by == op.by && *s == step);
        if !same {
            self.close();
            if step == Step::None {
                return canvas.apply(op).is_ok();
            }
            let stacks = self.history.authors.entry(op.by.clone()).or_default();
            let mark = match step {
                Step::New(mark) => {
                    stacks.redo.clear();
                    mark
                }
                Step::None => n,
                Step::Undo(undone) => drop_to(&mut stacks.undo, undone, n),
                Step::Redo(redone) => drop_to(&mut stacks.redo, redone, n),
            };
            self.open = Some((op.by.clone(), step, Back { mark, actions: Vec::new() }));
        }
        let Ok((_, inverse)) = apply_one(canvas, op) else { return false };
        if let Some((_, _, back)) = &mut self.open {
            back.actions.extend(inverse);
        }
        true
    }

    /// The history the lines built.
    pub(crate) fn finish(mut self) -> History {
        self.close();
        self.history
    }

    fn close(&mut self) {
        let Some((by, step, mut back)) = self.open.take() else { return };
        if back.actions.is_empty() {
            return;
        }
        back.actions.reverse();
        let stacks = self.history.authors.entry(by).or_default();
        let to = if matches!(step, Step::Undo(_)) { &mut stacks.redo } else { &mut stacks.undo };
        push(to, back);
    }
}

/// Pops `stack` down past the step marked `mark` (the steps above it were
/// dropped by the walk that took it), and returns `n`, the mark of the step
/// the walk made.
fn drop_to(stack: &mut Vec<Back>, mark: u64, n: u64) -> u64 {
    while stack.pop().is_some_and(|back| back.mark != mark) {}
    n
}

fn push(stack: &mut Vec<Back>, step: Back) {
    if stack.len() == DEPTH {
        stack.remove(0);
    }
    stack.push(step);
}

/// Applies one op, and returns what it did and the action that takes it back
/// (none when the canvas did not change).
fn apply_one(canvas: &mut Canvas, op: Op) -> Result<(Applied, Option<Action>)> {
    let undoes = inverse(canvas, &op.action);
    let applied = canvas.apply(op)?;
    let back = match applied {
        Applied::Added(item) => Some(Action::Remove { item }),
        Applied::PageAdded(page) => Some(Action::PageRemove { page }),
        Applied::Changed => undoes,
        Applied::Unchanged => None,
    };
    Ok((applied, back))
}

/// The action that would take `action` back, read from the canvas before it
/// applies. An add's inverse needs the new id, so it comes from [`Applied`].
fn inverse(canvas: &Canvas, action: &Action) -> Option<Action> {
    match action {
        Action::Add { .. } | Action::PageAdd { .. } => None,
        Action::Edit { item, .. } | Action::Move { item, .. } => {
            canvas.item(*item).map(|old| Action::Edit { item: *item, shape: old.shape().clone() })
        }
        Action::Remove { item } => canvas.item(*item).cloned().map(|item| Action::Restore { item }),
        Action::Restore { item } => Some(Action::Remove { item: item.id() }),
        Action::PageRename { page, .. } => canvas
            .page(*page)
            .map(|p| Action::PageRename { page: *page, name: p.name().to_string() }),
        Action::PageRemove { page } => {
            let index = canvas.pages().position(|p| p.id() == *page)?;
            let items = canvas.items(*page).cloned().collect();
            canvas.page(*page).cloned().map(|page| Action::PageRestore { page, index, items })
        }
        Action::PageRestore { page, .. } => Some(Action::PageRemove { page: page.id() }),
    }
}
