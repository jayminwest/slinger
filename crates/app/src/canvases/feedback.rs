//! Leaving feedback from the window: the `leave-feedback` command opens the
//! entry ([`crate::scene::feedback`]) in the focused pane, with what the
//! pane shows; the entry and the thread that saves the note are
//! [`super::flow`]'s. Each open asks it for the recent notes to list under
//! the entry, and a tap on a note's "✓" there has it resolve the note.
//! `show-all-feedback` lists the resolved notes there too
//! ([`Canvases::list_all_feedback`]). `show-feedback-file`
//! asks the daemon where the notes go ([`Canvases::feedback_file`]).
//! The entry takes the keys as the sling's does, and only one of the two
//! is open. Runs on the main thread and never waits.

use super::{Canvases, canvas_of};
use crate::layout::{PaneId, Rect};
use crate::panes::Panes;
use crate::scene::feedback::Context;
use crate::scene::tools::Tool;

impl Canvases {
    /// The `leave-feedback` command: opens the entry in `pane` of `panes`,
    /// whose frame is `frame`, for a note on what it shows: `workspace`,
    /// its canvas and page, `tool` and the items selected there. Another
    /// open entry closes, and a text being typed ends, as a page's new
    /// name's entry ends it: the entry's caret is then the only one, and
    /// the text's frozen caret does not stay on the canvas (slinger-b298f6).
    /// False while a note saves.
    pub fn open_feedback(
        &mut self,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        workspace: Option<&str>,
        tool: Tool,
    ) -> bool {
        let named = self.pages_of(panes, pane).and_then(|(_, pages, at)| {
            pages.into_iter().find(|shown| shown.id == at).map(|shown| shown.name)
        });
        let selected = self.selection().filter(|_| self.shows_selection(panes, pane));
        let context = Context {
            workspace: workspace.map(str::to_string),
            canvas: canvas_of(panes, pane).map(str::to_string),
            page: named,
            tool: Some(tool.name().to_string()),
            selected: selected.map_or(0, |selection| selection.ids.len()),
        };
        if !self.flow.open_feedback(frame, context) {
            return false;
        }
        if self.shapes.end(&mut self.ink) {
            self.send();
        }
        if self.flow.cancel_sling() | self.asks.cancel_entry() {
            self.flow.entry_closed();
        }
        true
    }

    /// Whether the feedback entry lists the resolved notes too, from its
    /// next open on: `show-all-feedback`, or only the open ones:
    /// `leave-feedback`.
    pub fn list_all_feedback(&mut self, all: bool) {
        self.flow.list_all_feedback(all);
    }

    /// A tap on the "✓" of the recent note listed `index`th under the
    /// open feedback entry: it leaves the list and the daemon resolves it.
    /// False when there is no such note.
    pub fn resolve_feedback(&mut self, index: usize) -> bool {
        self.flow.resolve_feedback(index)
    }

    /// The `show-feedback-file` command: the daemon's feedback file, for
    /// the Finder to show; else what the flash says. Waits for the daemon.
    pub fn feedback_file(&self) -> Result<String, String> {
        crate::backend::feedback_file(&self.daemon)
    }

    /// What to flash once a note is saved, once.
    pub fn take_flash(&mut self) -> Option<String> {
        self.flow.take_flash()
    }
}

#[cfg(test)]
mod tests;
