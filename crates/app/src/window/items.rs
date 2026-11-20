//! Who holds the keyboard ([`keyboard`]), and so whether the item commands
//! (select all, next and previous item, nudge, duplicate: [`Keyed`]) act
//! on the focused pane's items ([`run`]). The one place that decides it:
//! a caret inside a canvas text (slinger-ea308d) takes its keys here.

use super::focus;
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::layout::Rect;
use crate::scene::paste::Keyboard;
use crate::scene::select::Keyed;
use crate::shared::Shared;

/// Which view has the keyboard ([`Keyboard`]): an open entry's field; the
/// window while one of the app's own texts takes typing; else the view
/// the last press of `shared` left it with.
pub fn keyboard(shared: &Shared, canvases: &Canvases) -> Keyboard {
    match (canvases.typing(), shared.palette.is_open() || canvases.editing()) {
        (true, _) => Keyboard::Entry,
        (false, true) => Keyboard::Window,
        (false, false) => shared.keyboard,
    }
}

/// Runs an item command on the focused pane of `shared` laid in `area`
/// while the items hold the keyboard: the window has it and no text of
/// the app takes typing. False when it did nothing, so the key goes on (a
/// select all to an entry's field or a page; an arrow to a page). Any
/// other command goes on (true).
pub fn run(command: Command, shared: &Shared, canvases: &mut Canvases, area: Rect) -> bool {
    let Some(keyed) = Keyed::of(command) else { return true };
    let items = keyboard(shared, canvases) == Keyboard::Window && !canvases.editing();
    items && canvases.keyed(&shared.panes, focus(&shared.panes, area), keyed)
}
