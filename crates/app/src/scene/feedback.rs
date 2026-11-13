//! Leaving feedback from the window, as Flick does: the `leave-feedback`
//! command opens a one-line entry in the focused pane, "Feedback" above it
//! and its keys beneath. Return saves the note, with what the window shows
//! ([`Context`]), and escape cancels; an empty note saves nothing. The
//! pen has buttons beside the box for both, and to dictate the note
//! ([`crate::scene::sling::pen`]). A saved
//! note closes the entry and flashes "Feedback saved"; a failure opens it
//! again with its text, and the problem under it. Under the open entry go
//! the most recent open notes ([`Recent`]), as Flick lists them beside its
//! own, so the user sees what is already said: five, or as many more as
//! the pane has room for. A "✓" at each one's end resolves it
//! ([`Feedback::ticks`]); a failure shows under the entry as a failed save
//! does. Listing all ([`Feedback::list_all`]: `show-all-feedback`) lists
//! the resolved notes too, under "All feedback": each says how it was
//! resolved, and has no "✓".
//!
//! Pure: `canvases` hands in the pane, the context, the text as it is typed,
//! the keys, the daemon's answer and the recent notes, and reads back the
//! entry and its lines.
//! The entry looks like the sling's ([`Entry::framed`]); the platform shows
//! whichever is open. It stands out from the canvas on a card in the
//! panel's colour, with its label, its keys and its lines
//! ([`Feedback::card`]).

use crate::backend::Problem;
use crate::commands::Chord;
use crate::config::Rgb;
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::scene::sling::pen::beside;
use crate::scene::sling::{Entry, HEIGHT, Key, SEND, SMALL, line, under};
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, style};

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
    /// Its `ts`, which names it: what resolving it sends.
    pub ts: String,
    pub text: String,
    /// When it was saved, as shown: `11-11 14:02`.
    pub when: String,
    /// How it was resolved, as shown (`resolved 11-12 09:30: slinger-1234`);
    /// `None` while it is open.
    pub resolved: Option<String>,
}

/// How many recent notes the entry lists at least, and at most: a tall
/// pane lists more than [`RECENT`], up to [`LISTED`].
pub const RECENT: usize = 5;
pub const LISTED: usize = 20;
/// The lines kept free under the recent notes, for what dictation says.
const SPARE: f64 = 2.0;
/// The space the card keeps around the entry and its lines.
pub const CARD: f64 = 8.0;

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
    /// Whether the recent notes are all of them, resolved ones too, or
    /// the open ones.
    all: bool,
    /// The bottom of the pane the entry opened in.
    bottom: f64,
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
        self.bottom = within.y + within.height;
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
    /// lists the first [`LISTED`], as many as fit. Returns whether anything
    /// shown changed.
    pub fn listed(&mut self, mut recent: Vec<Recent>) -> bool {
        recent.truncate(LISTED);
        let changed = recent != self.recent && self.is_typing();
        self.recent = recent;
        changed
    }

    /// Lists all the recent notes, resolved ones too, or the open ones,
    /// from now on. A change drops the notes listed, until the next list.
    pub fn list_all(&mut self, all: bool) {
        if all != self.all {
            (self.all, self.recent) = (all, Vec::new());
        }
    }

    /// Whether the entry lists all the recent notes, resolved ones too.
    pub fn lists_all(&self) -> bool {
        self.all
    }

    /// The user resolved the recent note listed `index`th: its `ts`, to
    /// send. It leaves the list now, and any problem shown goes. `None`
    /// when the entry is closed or lists no such note.
    pub fn resolve(&mut self, index: usize) -> Option<String> {
        if index >= self.shown() {
            return None;
        }
        if let Some((_, _, Stage::Typing(problem))) = &mut self.open {
            *problem = None;
        }
        Some(self.recent.remove(index).ts)
    }

    /// Resolving a note failed for `problem`: it shows under the open
    /// entry, as a failed save's does. Returns whether anything shown
    /// changed.
    pub fn unresolved(&mut self, problem: Problem) -> bool {
        let Some((_, _, Stage::Typing(shown))) = &mut self.open else { return false };
        *shown = Some(problem);
        true
    }

    /// The "✓" at the end of each open note listed under the open entry,
    /// in `theme`'s colours: their labels and their targets, which resolve
    /// it ([`Tap::Resolve`]). A resolved note has none.
    pub fn ticks(&self, theme: &Theme) -> (Vec<Label>, Vec<Target>) {
        let rows = self.labels(theme);
        let (mut labels, mut targets) = (Vec::new(), Vec::new());
        for (index, row) in rows[rows.len() - self.shown()..].iter().enumerate() {
            if self.recent[index].resolved.is_some() {
                continue;
            }
            let frame = Rect { x: row.frame.x + row.frame.width, width: style::TAP, ..row.frame };
            let fill = Some(style::panel(theme));
            labels.push(Label { text: "✓".into(), colour: theme.text, fill, frame });
            targets.push((frame, Tap::Resolve(index)));
        }
        (labels, targets)
    }

    /// How many recent notes the open entry lists: [`RECENT`], or as many
    /// more as fit above the pane's bottom, with [`SPARE`] lines to spare.
    fn shown(&self) -> usize {
        let Some((_, frame, Stage::Typing(problem))) = &self.open else { return 0 };
        let above = problem.as_ref().map_or(0.0, |p| if p.hint.is_some() { 2.0 } else { 1.0 });
        let theme = Theme::default();
        let first = line(*frame, above + 2.0, String::new(), theme.muted, &theme).frame;
        #[expect(clippy::cast_possible_truncation, reason = "a count of lines, kept to 0 and up")]
        #[expect(clippy::cast_sign_loss, reason = "kept to 0 and up")]
        let room = ((self.bottom - first.y) / first.height - SPARE).max(0.0) as usize;
        let fit = room.clamp(RECENT, LISTED);
        self.recent.len().min(fit)
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
        let fill = Some(style::panel(theme));
        (entry.title.label.fill, entry.keys.label.fill) = (fill, fill);
        Some(entry)
    }

    /// The card under the open entry, in `theme`'s panel colour: from its
    /// label above the box to its last line, [`CARD`] around them.
    pub fn card(&self, theme: &Theme) -> Option<Label> {
        let Some((_, frame, Stage::Typing(_))) = &self.open else { return None };
        let last = self.labels(theme).last().map(|l| l.frame.y + l.frame.height);
        let (top, bottom) = (frame.y - SMALL - CARD, last.unwrap_or(frame.y + HEIGHT + SMALL));
        let card = Rect {
            x: frame.x - CARD,
            y: top,
            width: frame.width + 2.0 * CARD,
            height: bottom + CARD - top,
        };
        let fill = style::panel(theme);
        Some(Label { text: String::new(), colour: fill, fill: Some(fill), frame: card })
    }

    /// `lines` (text and colour) under the open entry, after its own (a
    /// problem, the recent notes): what dictation says.
    pub fn under(&self, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<Label> {
        let Some((_, frame, Stage::Typing(_))) = &self.open else { return Vec::new() };
        on_card(under(*frame, self.labels(theme).len(), theme, lines), theme)
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
                let saving = line(*frame, 0.0, "saving feedback…".into(), theme.muted, theme);
                return on_card(vec![saving], theme);
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
            below(if self.all { "All feedback" } else { "Recent feedback" }.into(), theme.muted);
        }
        for recent in &self.recent[..self.shown()] {
            n += 1.0;
            let mut text = format!("{} · {}", recent.when, recent.text);
            if let Some(resolved) = &recent.resolved {
                text = format!("{text} · {resolved}");
            }
            let mut row = line(*frame, n, text, theme.muted, theme);
            // It makes room for its "✓" at the end; a resolved note's
            // rows stay as wide, so the notes stay in one column.
            row.frame.width = (row.frame.width - style::TAP).max(0.0);
            lines.push(row);
        }
        on_card(lines, theme)
    }
}

/// `lines` filled in `theme`'s panel colour, as the card they stand on.
fn on_card(mut lines: Vec<Label>, theme: &Theme) -> Vec<Label> {
    let fill = Some(style::panel(theme));
    for line in &mut lines {
        line.fill = fill;
    }
    lines
}

#[cfg(test)]
mod tests;
