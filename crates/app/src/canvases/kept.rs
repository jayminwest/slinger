//! The pen's changes not yet written, kept on disk (slinger-e76db0,
//! slinger-e7fc87): made while the daemon is down, a stroke, an erase, a
//! move, a put, an edit or a page's change survives the app quitting and is
//! sent after the next start, in the order made, ahead of anything made
//! then, under the key it was applied once under, so one that was on its
//! way is not written twice. A kept change that names an item another kept
//! change adds still names it after the start ([`Ink::resume`]). The file
//! follows the ink each time the changes waiting change; an undo or a redo
//! is not kept ([`Ink::unsent`]).
//!
//! [`Ink::resume`]: crate::scene::ink::Ink::resume
//! [`Ink::unsent`]: crate::scene::ink::Ink::unsent

use super::Canvases;
use crate::backend::{Problem, Unsent};
use crate::scene::ink::Change;

/// The file, which changes it holds (by the ink's keys, as they were
/// saved), and why it could not be read or written, if it could not.
pub(super) struct Kept {
    file: Unsent,
    saved: Vec<(u64, Change)>,
    problem: Option<Problem>,
}

impl Kept {
    pub(super) fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }
}

impl Canvases {
    /// Keeps the changes not yet written in `file` from now on, and first
    /// queues the changes it kept, to be sent before any made from now on.
    /// The file is this app's alone while it runs ([`Unsent::claim`]).
    pub fn keeping(mut self, mut file: Unsent) -> Self {
        let (changes, problem) = match file.claim() {
            Ok(changes) => (changes, None),
            Err(problem) => (Vec::new(), Some(problem)),
        };
        self.ink.resume(changes);
        // Saved again under this run's keys at the first poll.
        self.kept = Some(Kept { file, saved: Vec::new(), problem });
        self
    }

    /// Writes the changes not yet written to the file, if they changed: a
    /// change made, written or dropped, or one renamed to the ids the
    /// daemon made.
    pub(super) fn keep(&mut self) {
        let Some(kept) = &mut self.kept else { return };
        if self.ink.unsent().eq(kept.saved.iter().map(|(key, change)| (*key, change))) {
            return;
        }
        kept.problem = kept.file.save(&self.ink.kept()).err();
        kept.saved = self.ink.unsent().map(|(key, change)| (key, change.clone())).collect();
    }
}

#[cfg(test)]
mod tests;
