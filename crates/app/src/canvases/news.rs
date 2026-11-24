//! What the canvases take in on a tick: what the followers heard and what
//! the daemon said to the ink; and, for the switch's trace, when each
//! canvas pane's page came.

use super::Canvases;
use crate::layout::Rect;
use crate::panes::Panes;
use crate::trace::Paged;

impl Canvases {
    /// Takes in what the followers heard and what the daemon said to the
    /// ink, and sends the next stroke. Returns whether anything a pane
    /// shows changed.
    pub fn poll(&mut self) -> bool {
        let changed = self.follow.hear() | self.editing.poll_vim(&mut self.follow);
        let (sent, dictated) = (self.send(), self.flow.poll_dictation());
        let came = self.follow.poll_lives();
        self.poll_sling() || sent || changed || dictated || came | self.follow.poll_pdfs()
    }

    /// Takes the daemon's answers to the ink (a pane that asked for a new
    /// page turns to it), lets go of the changes every page followed of
    /// their canvas now holds, and hands the next change to the outbox.
    /// Returns whether there were answers.
    pub(super) fn send(&mut self) -> bool {
        self.editing.send(&mut self.follow)
    }

    /// When the page of each canvas pane of `panes` laid in `area` came, or
    /// `None` while one has none yet ([`crate::trace`]).
    pub fn paged(&self, panes: &Panes, area: Rect) -> Option<Vec<Paged>> {
        self.follow.paged(panes, area)
    }
}
