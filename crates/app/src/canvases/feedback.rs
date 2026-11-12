//! Leaving feedback from the window: the `leave-feedback` command opens the
//! entry ([`crate::scene::feedback`]) in the focused pane, with what the
//! pane shows, and the thread that saves the note ([`Noter`]) sends it.
//! Each open asks it for the recent notes to list under the entry.
//! The entry takes the keys as the sling's does, and only one of the two
//! is open. Runs on the main thread and never waits.

use slinger_control::Client;

use super::{Canvases, canvas_of};
use crate::backend::Noter;
use crate::commands::Chord;
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;
use crate::scene::Label;
use crate::scene::feedback::{Context, Feedback};
use crate::scene::sling::{Entry, Key};
use crate::scene::tools::Tool;

/// The note being left, and the thread that saves it.
#[derive(Debug)]
pub(super) struct Notes {
    entry: Feedback,
    noter: Noter,
}

impl Notes {
    /// Saves through `daemon`.
    pub(super) fn new(daemon: Client) -> Self {
        Self { entry: Feedback::default(), noter: Noter::new(daemon) }
    }

    /// Saves through `daemon` from the next note on.
    pub(super) fn reconnect(&mut self, daemon: Client) {
        self.noter.reconnect(daemon);
    }

    /// Whether the entry is open: it takes the keys.
    pub(super) fn is_typing(&self) -> bool {
        self.entry.is_typing()
    }

    /// Closes the entry. False when it is closed.
    pub(super) fn cancel(&mut self) -> bool {
        self.entry.cancel()
    }

    /// Whether `at`, a point in the window, is on the open entry.
    pub(super) fn covers(&self, at: Point) -> bool {
        self.entry.covers(at)
    }

    /// The entry, while it is open, and its lines, in `theme`'s colours.
    pub(super) fn shown(&self, theme: &Theme) -> (Option<Entry>, Vec<Label>) {
        (self.entry.entry(theme), self.entry.labels(theme))
    }
}

impl Canvases {
    /// The `leave-feedback` command: opens the entry in `pane` of `panes`,
    /// whose frame is `frame`, for a note on what it shows: `workspace`,
    /// its canvas and page, `tool` and the items selected there. Another
    /// open entry closes. False while a note saves.
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
        if !self.notes.entry.open(frame, context) {
            return false;
        }
        self.notes.noter.list();
        if self.sling.cancel() | self.asks.cancel_rename() {
            self.entry_closed();
        }
        true
    }

    /// What to flash once a note is saved, once.
    pub fn take_flash(&mut self) -> Option<String> {
        self.notes.entry.flash()
    }

    /// A key press while the feedback entry is open, as
    /// [`Canvases::sling_key`]: `None` while it is closed.
    pub(super) fn feedback_key(&mut self, chord: Chord) -> Option<bool> {
        let notes = &mut self.notes;
        let closed = match notes.entry.key(chord)? {
            Key::Send => {
                if let Some(note) = notes.entry.send() {
                    notes.noter.send(note);
                }
                true
            }
            Key::Cancel => notes.entry.cancel(),
            Key::Type => return Some(false),
        };
        self.entry_closed();
        Some(closed)
    }

    /// The feedback entry's text is now `text`. False while it is closed.
    pub(super) fn feedback_typed(&mut self, text: &str) -> bool {
        let typing = self.notes.is_typing();
        self.notes.entry.typed(text);
        typing
    }

    /// Takes the daemon's answers to the notes and its list of the recent
    /// ones. Returns whether anything shown changed.
    pub(super) fn poll_feedback(&mut self) -> bool {
        let notes = &mut self.notes;
        let mut changed = notes.noter.listed().is_some_and(|recent| notes.entry.listed(recent));
        for answer in self.notes.noter.answers() {
            changed |= self.notes.entry.answered(answer);
        }
        changed
    }
}

#[cfg(test)]
mod tests;
