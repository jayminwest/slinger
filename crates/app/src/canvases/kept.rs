//! The pen's strokes not yet written, kept on disk (slinger-e76db0): drawn
//! while the daemon is down, a stroke survives the app quitting and is sent
//! after the next start, ahead of anything drawn then. The file follows the
//! ink each time the strokes waiting change; erases, moves and the other
//! changes are not kept.

use super::Canvases;
use crate::backend::{Problem, Unsent};
use crate::scene::ink::Change;

/// The file, which strokes it holds (by the ink's keys), and why it could
/// not be read or written, if it could not.
pub(super) struct Kept {
    file: Unsent,
    saved: Vec<u64>,
    problem: Option<Problem>,
}

impl Kept {
    pub(super) fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }
}

impl Canvases {
    /// Keeps the strokes not yet written in `file` from now on, and first
    /// queues the strokes it kept, to be sent before any drawn from now on.
    pub fn keeping(mut self, file: Unsent) -> Self {
        let (strokes, problem) = match file.load() {
            Ok(strokes) => (strokes, None),
            Err(problem) => (Vec::new(), Some(problem)),
        };
        let saved = strokes.into_iter().map(|s| self.ink.push_keyed(Change::Add(s))).collect();
        self.kept = Some(Kept { file, saved, problem });
        self
    }

    /// Writes the strokes not yet written to the file, if they changed.
    pub(super) fn keep(&mut self) {
        let Some(kept) = &mut self.kept else { return };
        let unsent: Vec<_> = self.ink.unsent().collect();
        if unsent.iter().map(|(key, _)| *key).eq(kept.saved.iter().copied()) {
            return;
        }
        let saved = kept.file.save(unsent.iter().map(|(_, stroke)| *stroke));
        kept.problem = saved.err();
        kept.saved = unsent.into_iter().map(|(key, _)| key).collect();
    }
}

#[cfg(test)]
mod tests;
