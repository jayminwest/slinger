//! Leaving feedback from the window, as Flick does: the `leave-feedback`
//! command opens a one-line entry in the focused pane, "Feedback" above it
//! and its keys beneath. Return saves the note, with what the window shows
//! ([`Context`]), and escape cancels; an empty note saves nothing. A saved
//! note closes the entry and flashes "Feedback saved"; a failure opens it
//! again with its text, and the problem under it. Under the open entry go
//! the most recent open notes ([`Recent`]), as Flick lists them beside its
//! own, so the user sees what is already said.
//!
//! Pure: `canvases` hands in the pane, the context, the text as it is typed,
//! the keys, the daemon's answer and the recent notes, and reads back the
//! entry and its lines.
//! The entry looks like the sling's ([`Entry::framed`]); the platform shows
//! whichever is open.

use crate::backend::Problem;
use crate::commands::Chord;
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::scene::Label;
use crate::scene::sling::{Entry, Key, SEND, centred, line};

/// What the window showed when the note was left. The daemon leaves out
/// what is absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub workspace: Option<String>,
    pub canvas: Option<String>,
    /// The page's name.
    pub page: Option<String>,
    /// The pen's tool: `pen`, `lasso`.
    pub tool: Option<String>,
    /// How many items were selected; none is 0.
    pub selected: usize,
}

/// A note to save: its text and its context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub context: Context,
}

/// A note saved before, as listed under the entry: its text and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub text: String,
    /// When it was saved, as shown: `11-11 14:02`.
    pub when: String,
}

/// How many recent notes the entry lists.
pub const RECENT: usize = 5;

/// Where an open note is.
#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// The entry is open, with why the last try failed, if it did.
    Typing(Option<Problem>),
    /// With the daemon.
    Saving,
}

/// The note being left, if any, whether one was saved since the window
/// last flashed it, and the recent notes listed under the entry.
#[derive(Debug, Default)]
pub struct Feedback {
    open: Option<(Note, Rect, Stage)>,
    saved: bool,
    recent: Vec<Recent>,
}

impl Feedback {
    /// Opens the entry in `within` (the focused pane), for a note with
    /// `context`. An entry already open stays, with its text, and takes the
    /// new context; one saving is left alone (false).
    pub fn open(&mut self, within: Rect, context: Context) -> bool {
        if matches!(self.open, Some((_, _, Stage::Saving))) {
            return false;
        }
        let text = self.open.take().map(|(note, ..)| note.text).unwrap_or_default();
        self.open = Some((Note { text, context }, centred(within), Stage::Typing(None)));
        true
    }

    /// Whether the entry is open.
    pub fn is_typing(&self) -> bool {
        matches!(self.open, Some((_, _, Stage::Typing(_))))
    }

    /// The entry's text is now `text`. Ignored when the entry is closed.
    pub fn typed(&mut self, text: &str) {
        if let Some((note, _, Stage::Typing(_))) = &mut self.open {
            note.text = text.to_string();
        }
    }

    /// What `chord` does while the entry is open; `None` when it is closed.
    pub fn key(&self, chord: Chord) -> Option<Key> {
        self.is_typing().then(|| Key::of(chord))
    }

    /// The user saved the entry: its note, to send, unless it is empty,
    /// which closes the entry and saves nothing. `None` when it is closed.
    pub fn send(&mut self) -> Option<Note> {
        let Some((note, _, stage @ Stage::Typing(_))) = &mut self.open else { return None };
        if note.text.trim().is_empty() {
            self.open = None;
            return None;
        }
        *stage = Stage::Saving;
        Some(note.clone())
    }

    /// Closes the entry. False when it is closed.
    pub fn cancel(&mut self) -> bool {
        let typing = self.is_typing();
        if typing {
            self.open = None;
        }
        typing
    }

    /// The daemon's answer to the note: saved, and the entry closes, or
    /// failed, and it opens again with its text and the problem. Returns
    /// whether anything shown changed.
    pub fn answered(&mut self, answer: Result<(), Problem>) -> bool {
        let Some((_, _, stage @ Stage::Saving)) = &mut self.open else { return false };
        match answer {
            Ok(()) => (self.open, self.saved) = (None, true),
            Err(problem) => *stage = Stage::Typing(Some(problem)),
        }
        true
    }

    /// The recent open notes are now `recent`, newest first; the entry
    /// lists the first [`RECENT`]. Returns whether anything shown changed.
    pub fn listed(&mut self, mut recent: Vec<Recent>) -> bool {
        recent.truncate(RECENT);
        let changed = recent != self.recent && self.is_typing();
        self.recent = recent;
        changed
    }

    /// What to flash once a note is saved, once: "Feedback saved".
    pub fn flash(&mut self) -> Option<String> {
        std::mem::take(&mut self.saved).then(|| "Feedback saved".to_string())
    }

    /// The entry, while it is open, in `theme`'s colours.
    pub fn entry(&self, theme: &Theme) -> Option<Entry> {
        let Some((note, frame, Stage::Typing(_))) = &self.open else { return None };
        let keys = format!("{} saves · esc cancels", SEND.glyphs());
        let mut entry = Entry::framed(*frame, theme, "Feedback".into(), keys);
        entry.text.clone_from(&note.text);
        entry.placeholder = "what worked, what got in the way".into();
        Some(entry)
    }

    /// Whether `at`, a point in the window, is on the open entry's box.
    pub fn covers(&self, at: Point) -> bool {
        matches!(&self.open, Some((_, frame, Stage::Typing(_))) if frame.contains(at))
    }

    /// The lines drawn for the note: where the entry was while it saves,
    /// and under the open entry why the last try failed, then the recent
    /// notes, newest first.
    pub fn labels(&self, theme: &Theme) -> Vec<Label> {
        let Some((_, frame, stage)) = &self.open else { return Vec::new() };
        let problem = match stage {
            Stage::Typing(problem) => problem,
            Stage::Saving => {
                return vec![line(*frame, 0.0, "saving feedback…".into(), theme.muted, theme)];
            }
        };
        let (mut lines, mut n) = (Vec::new(), 0.0);
        let mut below = |text: String, colour| {
            n += 1.0;
            lines.push(line(*frame, n, text, colour, theme));
        };
        if let Some(problem) = problem {
            below(format!("✗ {}", problem.message), theme.error);
            if let Some(hint) = &problem.hint {
                below(hint.clone(), theme.muted);
            }
        }
        if !self.recent.is_empty() {
            below("Recent feedback".into(), theme.muted);
        }
        for recent in &self.recent {
            below(format!("{} · {}", recent.when, recent.text), theme.muted);
        }
        lines
    }
}

#[cfg(test)]
mod tests;
