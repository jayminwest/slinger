//! The feedback being left ([`crate::scene::feedback`]) and the thread that
//! saves the note ([`Noter`]). Each open asks it for the recent notes to
//! list under the entry, and a tap on a note's "✓" there has it resolve the
//! note. Runs on the main thread and never waits.

use slinger_control::Client;

use crate::backend::{Noter, Wake};
use crate::commands::Chord;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::Label;
use crate::scene::feedback::{Context, Feedback};
use crate::scene::sling::{Entry, Key};
use crate::scene::taps::Target;

/// The note being left, and the thread that saves it.
#[derive(Debug)]
pub(super) struct Notes {
    entry: Feedback,
    noter: Noter,
}

impl Notes {
    /// Saves through `daemon`, waking the main thread with each answer.
    pub(super) fn new(daemon: Client, wake: &Wake) -> Self {
        Self { entry: Feedback::default(), noter: Noter::new(daemon, wake) }
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

    /// `lines` under the open entry, after its own: what dictation says.
    pub(super) fn under(&self, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<Label> {
        self.entry.under(theme, lines)
    }

    /// The "✓" at the end of each recent note under the open entry, in
    /// `theme`'s colours: their labels and their targets.
    pub(super) fn ticks(&self, theme: &Theme) -> (Vec<Label>, Vec<Target>) {
        self.entry.ticks(theme)
    }

    /// The entry, while it is open, and its card and lines, in `theme`'s
    /// colours.
    pub(super) fn shown(&self, theme: &Theme) -> (Option<Entry>, Vec<Label>) {
        let card = self.entry.card(theme).into_iter();
        (self.entry.entry(theme), card.chain(self.entry.labels(theme)).collect())
    }

    /// Opens the entry at `frame` for a note on `context`, and asks for the
    /// recent notes to list under it. False while a note saves.
    pub(super) fn open(&mut self, frame: Rect, context: Context) -> bool {
        if !self.entry.open(frame, context) {
            return false;
        }
        self.noter.list(self.entry.lists_all());
        true
    }

    /// Whether the entry lists the resolved notes too, from its next open on.
    pub(super) fn list_all(&mut self, all: bool) {
        self.entry.list_all(all);
    }

    /// Resolves the recent note listed `index`th under the open entry.
    /// False when there is no such note.
    pub(super) fn resolve(&mut self, index: usize) -> bool {
        let Some(ts) = self.entry.resolve(index) else { return false };
        self.noter.resolve(ts, self.entry.lists_all());
        true
    }

    /// What to flash once a note is saved, once.
    pub(super) fn flash(&mut self) -> Option<String> {
        self.entry.flash()
    }

    /// What `chord` does while the entry is open; `None` while it is closed.
    pub(super) fn key(&self, chord: Chord) -> Option<Key> {
        self.entry.key(chord)
    }

    /// Sends the note typed, for the thread to save.
    pub(super) fn send(&mut self) {
        if let Some(note) = self.entry.send() {
            self.noter.send(note);
        }
    }

    /// The entry's text is now `text`. False while it is closed.
    pub(super) fn typed(&mut self, text: &str) -> bool {
        let typing = self.is_typing();
        self.entry.typed(text);
        typing
    }

    /// Takes the daemon's answers to the notes saved and resolved, and its
    /// list of the recent ones. Returns whether anything shown changed.
    pub(super) fn poll(&mut self) -> bool {
        let mut changed = self.noter.listed().is_some_and(|recent| self.entry.listed(recent));
        for answer in self.noter.answers() {
            changed |= self.entry.answered(answer);
        }
        for answer in self.noter.resolutions() {
            if let Err(problem) = answer {
                changed |= self.entry.unresolved(problem);
            }
        }
        changed
    }
}
