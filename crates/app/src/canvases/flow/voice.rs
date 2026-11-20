//! Speaking the sling's prompt (SPEC D2), or into any open entry: what
//! dictation shows under the entry ([`crate::scene::dictation`]) and the
//! thread that asks Flick ([`Flick`]). Flick types what it heard into the
//! focused field, the entry, as if the user typed it, so the text reaches
//! it the usual way ([`super::SlingFlow::typed`]). Runs on the main thread
//! and never waits.

use std::path::PathBuf;

use crate::backend::{Flick, Wake};
use crate::config::{Rgb, Theme};
use crate::scene::dictation::Dictation;

/// The dictation into the entry, and the thread that asks Flick for it.
#[derive(Debug)]
pub(super) struct Voice {
    dictation: Dictation,
    flick: Flick,
}

impl Voice {
    /// Asks the Flick listening on `socket`, once the user dictates.
    pub(super) fn new(socket: PathBuf, wake: &Wake) -> Self {
        Self { dictation: Dictation::default(), flick: Flick::start(socket, wake) }
    }

    /// Starts Flick's dictation into the open entry, or stops the one going.
    pub(super) fn toggle(&mut self) {
        if let Some(verb) = self.dictation.toggle() {
            self.flick.send(verb);
        }
    }

    /// The entry closed: a dictation still going is cancelled.
    pub(super) fn closed(&mut self) {
        if let Some(verb) = self.dictation.closed() {
            self.flick.send(verb);
        }
    }

    /// Takes Flick's answers. Returns whether anything shown changed.
    pub(super) fn poll(&mut self) -> bool {
        let mut changed = false;
        for (verb, answer) in self.flick.answers() {
            changed |= self.dictation.answered(verb, answer);
        }
        changed
    }

    /// What dictation says, in `theme`'s colours.
    pub(super) fn lines(&self, theme: &Theme) -> Vec<(String, Rgb)> {
        self.dictation.lines(theme)
    }

    /// Whether Flick listens now.
    pub(super) fn listening(&self) -> bool {
        self.dictation.listening()
    }
}
