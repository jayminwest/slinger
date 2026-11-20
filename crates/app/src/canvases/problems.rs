//! What is wrong, in two kinds (slinger-724433): problems from asking the
//! daemon, which a bundled app's launch line replaces while it starts
//! slingerd ([`crate::scene::daemon::said`]), and the app's own (the kept
//! ink's file, a PDF it could not read), which show whatever the daemon does.

use super::Canvases;
use crate::backend::Problem;

impl Canvases {
    /// What asking the daemon went wrong with: following a canvas, or
    /// sending the ink.
    pub fn daemon_problems(&self) -> impl Iterator<Item = &Problem> {
        let open = self.open.iter().filter_map(|o| o.problem.as_ref());
        open.chain(self.ink.problem())
    }

    /// What went wrong in the app itself: keeping the unsent ink in its
    /// file, or reading a PDF.
    pub fn own_problems(&self) -> impl Iterator<Item = &Problem> {
        let kept = self.kept.as_ref().and_then(super::kept::Kept::problem);
        kept.into_iter().chain(self.pdfs.problem())
    }
}
