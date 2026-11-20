//! Where the shown workspace's panes look, handed to the daemon to keep
//! ([`crate::backend::Saver`]) each time a draw changes it, and at quit.
//! What comes back from the daemon is [`super::View::restore`]'s.

use std::time::Duration;

use slinger_control::{PaneView, PaneViewed};

use super::find::{find, wanted};
use super::view::View;
use super::{Canvases, Open};
use crate::backend::Looks;
use crate::layout::PaneId;
use crate::mirror::Mirror;
use crate::panes::Panes;

/// The longest quit waits for the views to be sent: the client's own
/// timeout is far longer.
const AT_QUIT: Duration = Duration::from_millis(500);

/// The looks of `panes`, whose canvas panes' views are `views`: each view
/// that has a place, by its pane's id, with its canvas, the page it shows
/// (that page's id once it has come), its origin and zoom; and the focused
/// pane's id. `None` for the window's own panes, which the daemon does not
/// keep.
pub(super) fn looks(panes: &Panes, views: &[(PaneId, View)], open: &[Open]) -> Option<Looks> {
    let workspace = panes.workspace()?.to_string();
    let viewed = |(id, view): &(PaneId, View)| {
        let canvas = view.canvas().filter(|_| view.placed)?;
        let asked = wanted(views, *id, canvas);
        let came = find(open, canvas, asked).and_then(|o| o.mirror.as_ref()).map(Mirror::page);
        let page = came.or(asked).map(str::to_string);
        let (origin, zoom) = (view.viewport.origin(), view.viewport.zoom());
        let view = PaneView { canvas: canvas.to_string(), page, x: origin.x, y: origin.y, zoom };
        Some(PaneViewed { id: id.number(), view })
    };
    let views = views.iter().filter_map(viewed).collect();
    Some(Looks { workspace, views, focus: Some(panes.focus().number()) })
}

impl Canvases {
    /// Hands the looks of `panes` over to be kept, if they changed.
    pub(super) fn save(&mut self, panes: &Panes) {
        if let Some(looks) = looks(panes, &self.views, &self.open) {
            self.saver.hand(looks);
        }
    }

    /// The app quits: the looks of `panes` go to the daemon at once, and
    /// this waits for that at most [`AT_QUIT`].
    pub fn quitting(&mut self, panes: &Panes) {
        self.save(panes);
        self.saver.flush(AT_QUIT);
    }
}
