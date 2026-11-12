//! Leaving feedback from the window, as Flick does: the `leave-feedback`
//! command opens a one-line entry in the focused pane, "Feedback" above it
//! and its keys beneath. Return saves the note, with what the window shows
//! ([`Context`]), and escape cancels; an empty note saves nothing. The
//! pen has buttons beside the box for both, and to dictate the note
//! ([`crate::scene::sling::pen`]). A saved
//! note closes the entry and flashes "Feedback saved"; a failure opens it
//! again with its text, and the problem under it.
//!
//! Pure: `canvases` hands in the pane, the context, the text as it is typed,
//! the keys and the daemon's answer, and reads back the entry and its lines.
//! The entry looks like the sling's ([`Entry::framed`]); the platform shows
//! whichever is open.

use crate::backend::Problem;
use crate::commands::Chord;
use crate::config::Rgb;
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::scene::Label;
use crate::scene::sling::pen::beside;
use crate::scene::sling::{Entry, Key, SEND, line, under};

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

/// Where an open note is.
#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// The entry is open, with why the last try failed, if it did.
    Typing(Option<Problem>),
    /// With the daemon.
    Saving,
}

/// The note being left, if any, and whether one was saved since the
/// window last flashed it.
#[derive(Debug, Default)]
pub struct Feedback {
    open: Option<(Note, Rect, Stage)>,
    saved: bool,
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
        self.open = Some((Note { text, context }, beside(within), Stage::Typing(None)));
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
        entry.done = Some("Save");
        Some(entry)
    }

    /// `lines` (text and colour) under the open entry, after its own: what
    /// dictation says.
    pub fn under(&self, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<Label> {
        let Some((_, frame, Stage::Typing(problem))) = &self.open else { return Vec::new() };
        under(*frame, problem.as_ref(), theme, lines)
    }

    /// Whether `at`, a point in the window, is on the open entry's box.
    pub fn covers(&self, at: Point) -> bool {
        matches!(&self.open, Some((_, frame, Stage::Typing(_))) if frame.contains(at))
    }

    /// The lines drawn for the note: where the entry was while it saves,
    /// and under the open entry why the last try failed.
    pub fn labels(&self, theme: &Theme) -> Vec<Label> {
        let Some((_, frame, stage)) = &self.open else { return Vec::new() };
        match stage {
            Stage::Typing(None) => Vec::new(),
            Stage::Typing(Some(problem)) => {
                let text = format!("✗ {}", problem.message);
                let mut lines = vec![line(*frame, 1.0, text, theme.error, theme)];
                let hint = problem.hint.clone();
                lines.extend(hint.map(|hint| line(*frame, 2.0, hint, theme.muted, theme)));
                lines
            }
            Stage::Saving => {
                vec![line(*frame, 0.0, "saving feedback…".into(), theme.muted, theme)]
            }
        }
    }
}

#[cfg(test)]
mod tests;
