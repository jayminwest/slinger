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

use std::collections::HashMap;

use super::{Action, Applied, Author, Canvas, Op, Time};
use crate::Result;

/// The most steps each author can undo. The oldest step goes first.
const DEPTH: usize = 1000;

/// The undo and redo steps of every author on one canvas. It lives beside
/// the canvas in memory; it is not in the canvas file.
#[derive(Debug, Clone, Default)]
pub struct History {
    authors: HashMap<Author, Stacks>,
}

#[derive(Debug, Clone, Default)]
struct Stacks {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

/// The actions that take a step back, in the order to apply them.
type Step = Vec<Action>;

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

    /// Applies `actions` in order as one undo step of `by`, and clears what
    /// `by` could redo. Either every action applies or none does.
    ///
    /// # Errors
    ///
    /// The first error of [`Canvas::apply`]; the canvas is then as it was.
    pub fn apply(
        &mut self,
        canvas: &mut Canvas,
        by: &Author,
        at: Time,
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
            push(&mut stacks.undo, back);
            stacks.redo.clear();
        }
        Ok(applied)
    }

    /// Takes back the last step of `by` that still applies, and returns the
    /// operations that did it (none when there was nothing to undo).
    pub fn undo(&mut self, canvas: &mut Canvas, by: &Author, at: Time) -> Vec<Op> {
        self.walk(canvas, by, at, Way::Undo)
    }

    /// Puts back the last step `by` undid, and returns the operations that
    /// did it (none when there was nothing to redo).
    pub fn redo(&mut self, canvas: &mut Canvas, by: &Author, at: Time) -> Vec<Op> {
        self.walk(canvas, by, at, Way::Redo)
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

    fn walk(&mut self, canvas: &mut Canvas, by: &Author, at: Time, way: Way) -> Vec<Op> {
        let Some(stacks) = self.authors.get_mut(by) else { return Vec::new() };
        let (from, to) = match way {
            Way::Undo => (&mut stacks.undo, &mut stacks.redo),
            Way::Redo => (&mut stacks.redo, &mut stacks.undo),
        };
        while let Some(step) = from.pop() {
            let mut ops = Vec::new();
            let mut back = Vec::new();
            for action in step {
                let op = Op { by: by.clone(), at, action };
                if let Ok((_, Some(inverse))) = apply_one(canvas, op.clone()) {
                    ops.push(op);
                    back.push(inverse);
                }
            }
            if !back.is_empty() {
                back.reverse();
                push(to, back);
                return ops;
            }
        }
        Vec::new()
    }
}

fn push(stack: &mut Vec<Step>, step: Step) {
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
