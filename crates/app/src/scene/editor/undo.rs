//! Undo and redo of the edits of the canvas text being typed (slinger-304496),
//! as a Mac text field groups them: a run of typed characters is one step,
//! a run of deletes back one, a run of forward deletes one; a paste, a cut,
//! a word or row delete, ctrl+K, ctrl+T and ctrl+O are a step each; moving
//! the caret or the selection ends a run. Undo puts back the text with its
//! caret and selection as they were before the step; redo goes forward
//! again until the next edit. A new size (the size keys) and a new column
//! (one drag of the frame's edge) are a step each too (slinger-0b424a). The
//! history lives with the text: once it ends, undo is the canvas's.

use super::{Editing, Editor};
use crate::scene::glyphs;

/// How a text is set: its size and the column it wraps in, which the
/// [`Editing`] holds and a step can take back.
pub type Look = (f64, f64);

/// An edit that a next one of its kind can join, as a field joins them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Run {
    /// A character, or a line, typed.
    Typing,
    /// A character deleted back.
    Deleting,
    /// A character deleted forward.
    Forward,
}

/// The text with its caret and selection anchor, before or after a step,
/// and its look when the step changed that.
#[derive(Debug, Clone, Default, PartialEq)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: Option<usize>,
    look: Option<Look>,
}

/// The steps to undo and to redo, newest last, and the run the last edit
/// was part of, with where it left the caret.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    run: Option<(Run, usize)>,
}

impl Editor {
    /// Undoes the last step (with `redo`, redoes the last undone one), a
    /// step of the text's look putting `look` back too. False, and nothing
    /// changes, when there is none.
    pub fn undo(&mut self, redo: bool, look: &mut Look) -> bool {
        let now = self.snapshot();
        let History { undo, redo: undone, .. } = &mut self.history;
        let (from, to) = if redo { (undone, undo) } else { (undo, undone) };
        let Some(back) = from.pop() else { return false };
        to.push(Snapshot { look: back.look.map(|_| *look), ..now });
        if let Some(back) = back.look {
            *look = back;
        }
        self.history.run = None;
        self.text = back.text;
        self.caret = back.caret;
        self.anchor = back.anchor;
        self.goal = None;
        true
    }

    /// Called before an edit that is part of `run`, if any: unless it
    /// joins the last edit's run (same kind, the caret where that left it,
    /// nothing selected), the text as it is becomes a step to undo. Any
    /// edit drops what was undone.
    pub(super) fn remember(&mut self, run: Option<Run>) {
        let selected = self.selection().is_some();
        let joins = run.is_some_and(|r| self.history.run == Some((r, self.caret))) && !selected;
        if !joins {
            let snapshot = self.snapshot();
            self.history.undo.push(snapshot);
        }
        self.history.redo.clear();
    }

    /// Called when the caret or the selection moves: the run ends.
    pub(super) fn moved(&mut self) {
        self.history.run = None;
    }

    /// Called after an edit that is part of `run`, if any: the next edit
    /// of that kind at the caret joins it.
    pub(super) fn ran(&mut self, run: Option<Run>) {
        self.history.run = run.map(|r| (r, self.caret));
    }

    /// Called after the text's look changed from `before`: that is a step.
    fn restyled(&mut self, before: Look) {
        let snapshot = Snapshot { look: Some(before), ..self.snapshot() };
        self.history.undo.push(snapshot);
        self.history.redo.clear();
        self.history.run = None;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { text: self.text.clone(), caret: self.caret, anchor: self.anchor, look: None }
    }
}

impl Editing {
    /// Undoes its last step, its look's or its editor's (with `redo`,
    /// redoes one). False when there is none.
    pub fn undo(&mut self, redo: bool) -> bool {
        let mut look = (self.size, self.column);
        let done = self.editor.undo(redo, &mut look);
        (self.size, self.column) = look;
        done
    }

    /// It takes `size`, a step of its own: a wrapped text keeps its
    /// column's cells ([`glyphs::resized`]), so its rows stay the same.
    pub fn resize(&mut self, size: f64) {
        let before = (self.size, self.column);
        self.column = glyphs::resized(self.column, self.size, size);
        self.size = size;
        if (self.size, self.column) != before {
            self.editor.restyled(before);
        }
    }

    /// A drag of its frame's edge, that began with its column at `column`,
    /// ended: a new column is a step of its own.
    pub fn widened(&mut self, column: f64) {
        if self.column.total_cmp(&column).is_ne() {
            self.editor.restyled((self.size, column));
        }
    }
}
