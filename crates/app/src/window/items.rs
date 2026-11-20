//! Who holds the keyboard ([`keyboard`]), and so whether the item commands
//! (select all, next and previous item, nudge, duplicate: [`Keyed`]) act
//! on the focused pane's items ([`run`]). The one place that decides it:
//! a caret inside a canvas text (slinger-ea308d) takes its keys here.
//! Also `step-in-beside` ([`beside`]): the selected box, in a new pane.

use std::path::Path;

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
/// other command goes on (true). Select all chosen from the menu while a
/// canvas text is typed selects its characters, as its key does.
pub fn run(command: Command, shared: &Shared, canvases: &mut Canvases, area: Rect) -> bool {
    if command == Command::SelectAll && canvases.select_text() {
        return true;
    }
    let Some(keyed) = Keyed::of(command) else { return true };
    let items = keyboard(shared, canvases) == Keyboard::Window && !canvases.editing();
    items && canvases.keyed(&shared.panes, focus(&shared.panes, area), keyed)
}

/// Splits the focused pane of `shared` laid in `area` (through the daemon
/// on `socket` for a workspace) and steps the new pane into the box
/// selected there (docs/design/fold.md). False, and nothing splits, when
/// the focused pane shows no selected box.
pub fn beside(canvases: &mut Canvases, shared: &mut Shared, area: Rect, socket: &Path) -> bool {
    let Some(target) = canvases.step_target(&shared.panes, shared.panes.focus()) else {
        return false;
    };
    shared.command(Command::SplitRight, area, socket);
    canvases.step_into(shared.panes.focus(), target);
    true
}
