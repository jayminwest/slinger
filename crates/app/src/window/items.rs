//! Who holds the keyboard ([`keyboard`]), and so whether the item commands
//! (select all, next and previous item, nudge, duplicate: [`Keyed`]) act
//! on the focused pane's items ([`run`]), delete-selection deletes them
//! ([`delete`]) and clear-selection clears them ([`clear`]), or, with
//! nothing selected, steps the pane out of the box it is in. The one place that decides it: a caret inside a canvas
//! text (slinger-ea308d) takes its keys here, and so does a page a press
//! went on to, a live item's on a canvas (slinger-479dea). Also
//! `step-in-beside` ([`beside`]): the selected box, in a new pane.

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
    items(shared, canvases) && canvases.keyed(&shared.panes, focus(&shared.panes, area), keyed)
}

/// Deletes the selection on the focused pane of `shared` while the items
/// hold the keyboard, as [`run`] says. False when it did nothing, so the
/// key goes on (a delete to a page's form, or to a live item's page).
pub fn delete(shared: &Shared, canvases: &mut Canvases) -> bool {
    items(shared, canvases) && canvases.delete_selection(&shared.panes, shared.panes.focus())
}

/// Clears the selection on the focused pane of `shared` while the items
/// hold the keyboard, as [`run`] says. False when it did nothing, so the
/// key goes on (an escape to a page's form, or to a live item's page:
/// slinger-0981ad). With nothing selected, steps the pane out of the box
/// it is in, one level (docs/design/fold.md).
pub fn clear(shared: &Shared, canvases: &mut Canvases) -> bool {
    let (panes, focus) = (&shared.panes, shared.panes.focus());
    items(shared, canvases)
        && (canvases.clear_selection(panes, focus) || canvases.step_out(panes, focus))
}

/// Whether the items hold the keyboard: the window has it and no text of
/// the app takes typing.
fn items(shared: &Shared, canvases: &Canvases) -> bool {
    keyboard(shared, canvases) == Keyboard::Window && !canvases.editing()
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
