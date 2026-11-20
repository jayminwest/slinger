//! The right and middle buttons in the window ([`pressed`]): which press
//! the app takes, which goes on to the view under it, and what a drag or
//! a lift becomes. The pan and the click are decided in
//! [`crate::scene::secondary`].

use super::pane_at;
use crate::Event;
use crate::canvases::Canvases;
use crate::layout::{Point, Rect};
use crate::panes::{self, Panes};
use crate::scene::secondary::{Button, Phase};
use crate::scene::status;
use crate::shared::Shared;

/// What a right or middle button's event comes to.
#[derive(Debug, PartialEq)]
pub enum Pressed {
    /// Route on as this event: a pan is the scroll it makes.
    On(Event),
    /// The app took it, and nothing changed.
    Took,
    /// It goes on to the view under it.
    Pass,
    /// A context click at this point in the window.
    Click(Point),
}

/// What the button event (`button`, `phase`, `at`) comes to, in the window
/// of `shared` with its panes laid in `area`. A press over an open entry's
/// field, an agent UI's page or a live item's page that takes presses goes
/// on to it (its own menu, else the system's); every other press is the
/// app's. A drag past the slop pans as an [`Event::Scroll`] at the press
/// point; a secondary lift inside it is a context click. A drag or a lift
/// of a press that went on goes on too.
pub fn pressed(
    (button, phase, at): (Button, Phase, Point),
    shared: &mut Shared,
    canvases: &Canvases,
    area: Rect,
) -> Pressed {
    let press = &mut shared.secondary;
    match phase {
        Phase::Down if goes_on(&shared.panes, canvases, area, at) => {
            press.drop_press();
            Pressed::Pass
        }
        Phase::Down => {
            press.down(button, at);
            Pressed::Took
        }
        Phase::Drag => match press.drag(at) {
            Some((at, dx, dy)) => Pressed::On(Event::Scroll { at, dx, dy }),
            None if press.held() => Pressed::Took,
            None => Pressed::Pass,
        },
        Phase::Up => {
            let held = press.held();
            match press.up(at) {
                Some(at) => Pressed::Click(at),
                None if held => Pressed::Took,
                None => Pressed::Pass,
            }
        }
    }
}

/// Whether a right or middle press at `at`, a point in the window, is the
/// view's under it: an open entry's field, an agent UI's page in a pane of
/// `panes` laid in `area` (not its footer), or a live item's page.
fn goes_on(panes: &Panes, canvases: &Canvases, area: Rect, at: Point) -> bool {
    let ui = |(_, pane, frame): &(_, &panes::Pane, Rect)| {
        pane.kind == panes::Kind::Ui && status::content(*frame).contains(at)
    };
    canvases.sling_covers(at)
        || panes.place(area).iter().any(ui)
        || canvases.page_under(panes, pane_at(panes, area, at), at)
}
