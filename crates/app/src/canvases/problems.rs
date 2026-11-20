//! What is wrong, in two kinds (slinger-724433): problems from asking the
//! daemon, which a bundled app's launch line replaces while it starts
//! slingerd ([`crate::scene::daemon::said`]), and the app's own (the kept
//! ink's file, a PDF it could not read), which show whatever the daemon does.

use super::Canvases;
use crate::backend::Problem;

impl Canvases {
    /// What asking the daemon went wrong with: following a canvas, or
    /// sending the ink.
    #[cfg(test)]
    pub fn daemon_problems(&self) -> impl Iterator<Item = &Problem> {
        self.asking(None)
    }

    /// What asking the daemon went wrong with, given why the window's
    /// layout could not be had (`layout`): that, else what following the
    /// panes' canvases did; then sending the ink. A failed layout leaves
    /// the window the panes it had (the last workspace's, or the first
    /// launch's `main`), not those of the workspace asked for, so what
    /// following them says is not shown under its problem (slinger-77724d).
    pub fn asking<'a>(&'a self, layout: Option<&'a Problem>) -> impl Iterator<Item = &'a Problem> {
        let open = self.open.iter().filter_map(|o| o.problem.as_ref());
        let open = open.filter(move |_| layout.is_none());
        layout.into_iter().chain(open).chain(self.ink.problem())
    }

    /// What went wrong in the app itself: keeping the unsent ink in its
    /// file, or reading a PDF.
    pub fn own_problems(&self) -> impl Iterator<Item = &Problem> {
        let kept = self.kept.as_ref().and_then(super::kept::Kept::problem);
        kept.into_iter().chain(self.pdfs.problem())
    }
}
