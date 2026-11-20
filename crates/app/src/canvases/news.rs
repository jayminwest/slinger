//! What the canvases take in on a tick: what the followers heard and what
//! the daemon said to the ink; and, for the switch's trace, when each
//! canvas pane's page came.

use super::Canvases;
use crate::layout::Rect;
use crate::panes::Panes;
use crate::scene::ink::Answer;
use crate::trace::Paged;

impl Canvases {
    /// Takes in what the followers heard and what the daemon said to the
    /// ink, and sends the next stroke. Returns whether anything a pane
    /// shows changed.
    pub fn poll(&mut self) -> bool {
        let changed = self.follow.hear();
        let (sent, dictated) = (self.send(), self.flow.poll_dictation());
        let came = self.follow.poll_lives();
        self.poll_sling() || sent || changed || dictated || came | self.follow.poll_pdfs()
    }

    /// Takes the daemon's answers to the ink (a pane that asked for a new
    /// page turns to it), lets go of the changes every page followed of
    /// their canvas now holds, and hands the next change to the outbox.
    /// Returns whether there were answers.
    pub(super) fn send(&mut self) -> bool {
        if !self.shapes.editing() {
            self.follow.text_joined(&mut self.ink);
        }
        let answers = self.outbox.answers();
        let answered = !answers.is_empty();
        for (key, answer) in answers {
            self.follow.turned(key, &answer);
            if let Answer::Made { ids, .. } = &answer {
                self.select.made(key, ids);
            }
            self.follow.fold_answered(key, &answer);
            let renamed = self.ink.answered(key, answer);
            self.select.renamed(&renamed);
        }
        // Every page of a canvas hears the same numbers: the slowest decides.
        for (canvas, upto) in self.follow.heard() {
            self.ink.heard(canvas, upto);
        }
        if let Some((key, change)) = self.ink.next() {
            self.outbox.send(key, self.ink.once(key), change, self.ink.joins(key));
        }
        self.keep();
        answered
    }

    /// When the page of each canvas pane of `panes` laid in `area` came, or
    /// `None` while one has none yet ([`crate::trace`]).
    pub fn paged(&self, panes: &Panes, area: Rect) -> Option<Vec<Paged>> {
        self.follow.paged(panes, area)
    }
}
