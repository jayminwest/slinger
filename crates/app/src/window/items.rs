//! Who holds the keyboard ([`keyboard`]), and so whether the item commands
//! (select all, next and previous item, nudge, duplicate: [`Keyed`]) act
//! on the focused pane's items ([`run`]), delete-selection deletes them
//! ([`delete`]) and clear-selection clears them ([`clear`]). The one place that decides it: a caret inside a canvas
//! text (slinger-ea308d) takes its keys here, and so does a page a press
//! went on to, a live item's on a canvas (slinger-479dea).

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
/// slinger-0981ad).
pub fn clear(shared: &Shared, canvases: &mut Canvases) -> bool {
    items(shared, canvases) && canvases.clear_selection(&shared.panes, shared.panes.focus())
}

/// Whether the items hold the keyboard: the window has it and no text of
/// the app takes typing.
fn items(shared: &Shared, canvases: &Canvases) -> bool {
    keyboard(shared, canvases) == Keyboard::Window && !canvases.editing()
}
