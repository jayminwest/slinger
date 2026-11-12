//! The pen's changes not yet written, kept on disk (slinger-e76db0,
//! slinger-e7fc87): made while the daemon is down, a stroke, an erase, a
//! move, a put, an edit or a page's change survives the app quitting and is
//! sent after the next start, in the order made, ahead of anything made
//! then, under the key it was applied once under, so one that was on its
//! way is not written twice. A kept change that names an item another kept
//! change adds still names it after the start ([`Ink::resume`]). The file
//! follows the ink each time the changes waiting change; an undo or a redo
//! is not kept ([`Ink::unsent`]). A pane waiting for a page kept to be
//! added turns to it once it is, after the start too (slinger-857848).
//!
//! [`Ink::resume`]: crate::scene::ink::Ink::resume
//! [`Ink::unsent`]: crate::scene::ink::Ink::unsent

use super::Canvases;
use crate::backend::{Problem, Unsent};
use crate::layout::PaneId;
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
        let ((changes, turning), problem) = match file.claim() {
            Ok(kept) => (kept, None),
            Err(problem) => ((Vec::new(), Vec::new()), Some(problem)),
        };
        let keys = self.ink.resume(changes);
        for (old, pane) in turning {
            let Some(&key) = keys.get(&old) else { continue };
            let Some((_, change)) = self.ink.unsent().find(|(k, _)| *k == key) else { continue };
            let canvas = change.canvas().to_string();
            self.turning.push((key, PaneId::numbered(pane), canvas));
        }
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
        let turning: Vec<_> =
            self.turning.iter().map(|(key, pane, _)| (*key, pane.number())).collect();
        kept.problem = kept.file.save(&self.ink.kept(), &turning).err();
        kept.saved = self.ink.unsent().map(|(key, change)| (key, change.clone())).collect();
    }
}

#[cfg(test)]
mod tests;
