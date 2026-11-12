//! The pen's strokes not yet written, kept on disk (slinger-e76db0): drawn
//! while the daemon is down, a stroke survives the app quitting and is sent
//! after the next start, ahead of anything drawn then, under the key it was
//! applied once under, so one that was on its way is not written twice. The
//! file follows the ink each time the strokes waiting change; erases, moves
//! and the other changes are not kept.

use super::Canvases;
use crate::backend::{KeptStroke, Problem, Unsent};
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
    /// The file is this app's alone while it runs ([`Unsent::claim`]).
    pub fn keeping(mut self, mut file: Unsent) -> Self {
        let (strokes, problem) = match file.claim() {
            Ok(strokes) => (strokes, None),
            Err(problem) => (Vec::new(), Some(problem)),
        };
        let resume = |k: KeptStroke| self.ink.resume(Change::Add(k.stroke), k.once);
        let saved = strokes.into_iter().map(resume).collect();
        self.kept = Some(Kept { file, saved, problem });
        self
    }

    /// Writes the strokes not yet written to the file, if they changed.
    pub(super) fn keep(&mut self) {
        let Some(kept) = &mut self.kept else { return };
        if self.ink.unsent().map(|(key, _)| key).eq(kept.saved.iter().copied()) {
            return;
        }
        let ink = &self.ink;
        let strokes: Vec<KeptStroke> = ink
            .unsent()
            .map(|(key, stroke)| KeptStroke { stroke: stroke.clone(), once: Some(ink.once(key)) })
            .collect();
        kept.problem = kept.file.save(&strokes).err();
        kept.saved = ink.unsent().map(|(key, _)| key).collect();
    }
}

#[cfg(test)]
mod tests;
