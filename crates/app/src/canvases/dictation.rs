//! Speaking the sling's prompt (SPEC D2): the `dictate` command, what
//! dictation shows under the entry ([`crate::scene::dictation`]), and the
//! thread that asks Flick ([`Flick`]). Flick types what it heard into the
//! focused field, the entry, as if the user typed it, so the text reaches
//! the sling the usual way ([`Canvases::sling_typed`]). Runs on the main
//! thread and never waits.

use std::path::PathBuf;

use super::Canvases;
use crate::backend::Flick;
use crate::commands::Command;
use crate::config::Theme;
use crate::layout::Rect;
use crate::panes::Panes;
use crate::scene::Label;
use crate::scene::dictation::Dictation;

/// The dictation into the entry, and the thread that asks Flick for it.
#[derive(Debug)]
pub(super) struct Voice {
    dictation: Dictation,
    flick: Flick,
}

impl Voice {
    /// Asks the Flick listening on `socket`, once the user dictates.
    pub(super) fn new(socket: PathBuf) -> Self {
        Self { dictation: Dictation::default(), flick: Flick::start(socket) }
    }
}

impl Canvases {
    /// The `sling` and `dictate` commands on the focused pane of `panes`
    /// laid in `area`. `sling` opens the entry at the selection. `dictate`
    /// opens it too when it is closed, then starts Flick's dictation into
    /// it, or stops the one going, after which Flick types what it heard
    /// into the entry. False when the entry is closed and cannot open (no
    /// selection there, or a sling on its way).
    pub fn sling_command(&mut self, command: Command, panes: &Panes, area: Rect) -> bool {
        if command != Command::Dictate {
            return self.open_sling(panes, area);
        }
        if !self.typing() && !self.open_sling(panes, area) {
            return false;
        }
        if let Some(verb) = self.voice.dictation.toggle() {
            self.voice.flick.send(verb);
        }
        true
    }

    /// The entry closed: a dictation still going is cancelled.
    pub(super) fn entry_closed(&mut self) {
        if let Some(verb) = self.voice.dictation.closed() {
            self.voice.flick.send(verb);
        }
    }

    /// Takes Flick's answers. Returns whether anything shown changed.
    pub(super) fn poll_dictation(&mut self) -> bool {
        let mut changed = false;
        for (verb, answer) in self.voice.flick.answers() {
            changed |= self.voice.dictation.answered(verb, answer);
        }
        changed
    }

    /// What dictation says under the open entry, in `theme`'s colours.
    pub(super) fn dictation_labels(&self, theme: &Theme) -> Vec<Label> {
        self.sling.under(theme, self.voice.dictation.lines(theme))
    }

    /// Asks the Flick on `socket` from now on: a test's stand-in.
    #[cfg(test)]
    pub(super) fn listen_to(&mut self, socket: PathBuf) {
        self.voice = Voice::new(socket);
    }
}
