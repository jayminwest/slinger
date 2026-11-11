//! The sling from the window (SPEC section 1, section 6, D18): select,
//! prompt, send. Pure: `main` hands in the selection and where it is, the
//! prompt's text as the user types it, the keys and the daemon's answer,
//! and reads back the one-line entry and the labels to draw.
//!
//! The `sling` command opens a one-line entry under the selection's box
//! (above it when there is no room below), inside the focused pane. The
//! entry takes every key but two: return sends, escape cancels. An empty
//! prompt sends too: the selection is the question.
//!
//! A sling goes once the ink of its canvas is in the daemon: every stroke,
//! erase and move made before it is applied first, so the packet holds what
//! the user saw. Ink drawn after the selection was made is never in it (only
//! the server's items can be selected), but it goes in the packet's new ink
//! like any other. While it waits and while it goes, a muted line stands
//! where the entry was. The answer arrives on the canvas through the change
//! feed and the agent's status in the sidebar, so a sling that went simply
//! closes. A sling that failed opens the entry again with its prompt, and
//! the error's message and hint under it: return tries again, escape gives up.
//!
//! [`Sling::send_now`] is the one entry point for slings that need no
//! typing (a prompt from dictation, a button on a live item).

use crate::backend::Problem;
use crate::commands::Chord;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::Label;
use crate::scene::canvas::ItemId;
use crate::scene::select::Selection;

/// The entry's size, in points.
const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 24.0;
/// The space between the selection's box and the entry, and the closest
/// the entry comes to an edge of its pane.
const GAP: f64 = 8.0;
/// The height of a line under the entry.
const LINE: f64 = 22.0;

/// What a sling sends: a page's selected items, by the server's ids, and
/// the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub canvas: String,
    pub page: String,
    pub ids: Vec<ItemId>,
    pub prompt: String,
}

/// The one-line entry the window shows: where, and the text it starts with.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub frame: Rect,
    pub text: String,
    /// Shown while the text is empty.
    pub placeholder: String,
}

/// What a key does while the entry is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Send,
    Cancel,
    /// The entry's own: it types, deletes or moves the caret.
    Type,
}

/// Where an open sling is.
#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// The entry is open, with why the last try failed, if it did.
    Typing(Option<Problem>),
    /// Sent by the user; waits for its canvas's ink to reach the daemon.
    Waiting,
    /// With the daemon.
    Sending,
}

/// The sling being made, if any.
#[derive(Debug, Default)]
pub struct Sling {
    open: Option<(Ask, Rect, Stage)>,
}

impl Sling {
    /// Opens the entry for `selection`, whose box is `around` in the
    /// window, inside `within` (its pane). A sling already open starts
    /// again on the new selection, with its text; one on its way is left
    /// alone (false).
    pub fn open(&mut self, selection: &Selection, around: Rect, within: Rect) -> bool {
        if matches!(self.open, Some((_, _, Stage::Waiting | Stage::Sending))) {
            return false;
        }
        let prompt = self.open.take().map(|(ask, ..)| ask.prompt).unwrap_or_default();
        let ask = ask(selection, prompt);
        self.open = Some((ask, place(around, within), Stage::Typing(None)));
        true
    }

    /// Sends `selection` with `prompt` at once, with no entry; its line,
    /// and a failure's entry, stand under `around` inside `within`. False,
    /// and nothing goes, while another sling is on its way.
    pub fn send_now(
        &mut self,
        selection: &Selection,
        prompt: &str,
        around: Rect,
        within: Rect,
    ) -> bool {
        if matches!(self.open, Some((_, _, Stage::Waiting | Stage::Sending))) {
            return false;
        }
        let ask = ask(selection, prompt.to_string());
        self.open = Some((ask, place(around, within), Stage::Waiting));
        true
    }

    /// Whether the entry is open.
    pub fn is_typing(&self) -> bool {
        matches!(self.open, Some((_, _, Stage::Typing(_))))
    }

    /// The entry's text is now `text`. Ignored when the entry is closed.
    pub fn typed(&mut self, text: &str) {
        if let Some((ask, _, Stage::Typing(_))) = &mut self.open {
            ask.prompt = text.to_string();
        }
    }

    /// What `chord` does while the entry is open; `None` when it is closed,
    /// so the key goes to the keymap.
    pub fn key(&self, chord: Chord) -> Option<Key> {
        if !self.is_typing() {
            return None;
        }
        Some(if chord == Chord::new(0, '\r') {
            Key::Send
        } else if chord == Chord::new(0, '\u{1b}') {
            Key::Cancel
        } else {
            Key::Type
        })
    }

    /// The user sent the entry's prompt: it waits for its canvas's ink.
    /// False when the entry is closed.
    pub fn send(&mut self) -> bool {
        let Some((_, _, stage @ Stage::Typing(_))) = &mut self.open else { return false };
        *stage = Stage::Waiting;
        true
    }

    /// Closes the entry. False when it is closed.
    pub fn cancel(&mut self) -> bool {
        let typing = self.is_typing();
        if typing {
            self.open = None;
        }
        typing
    }

    /// The waiting sling, now going, once `settled` says its canvas's ink
    /// is in the daemon.
    pub fn ready(&mut self, settled: impl Fn(&str) -> bool) -> Option<Ask> {
        let Some((ask, _, stage @ Stage::Waiting)) = &mut self.open else { return None };
        if !settled(&ask.canvas) {
            return None;
        }
        *stage = Stage::Sending;
        Some(ask.clone())
    }

    /// The daemon's answer to the sling going: it went, and closes, or it
    /// failed, and the entry opens again with its prompt and the problem.
    /// Returns whether anything shown changed.
    pub fn answered(&mut self, answer: Result<(), Problem>) -> bool {
        let Some((_, _, stage @ Stage::Sending)) = &mut self.open else { return false };
        match answer {
            Ok(()) => self.open = None,
            Err(problem) => *stage = Stage::Typing(Some(problem)),
        }
        true
    }

    /// The entry, while it is open.
    pub fn entry(&self) -> Option<Entry> {
        let Some((ask, frame, Stage::Typing(_))) = &self.open else { return None };
        let placeholder = format!("sling {} to the agent: what to ask (or nothing)", count(ask));
        Some(Entry { frame: *frame, text: ask.prompt.clone(), placeholder })
    }

    /// Whether `at`, a point in the window, is on the open entry: a press
    /// there is the entry's, not the canvas's.
    pub fn covers(&self, at: Point) -> bool {
        self.entry().is_some_and(|e| e.frame.contains(at))
    }

    /// The lines drawn for the sling: where the entry was while it goes,
    /// and under the open entry why the last try failed.
    pub fn labels(&self, theme: &Theme) -> Vec<Label> {
        let Some((ask, frame, stage)) = &self.open else { return Vec::new() };
        let line = |n: f64, text: String, colour| line(*frame, n, text, colour, theme);
        match stage {
            Stage::Typing(None) => Vec::new(),
            Stage::Typing(Some(problem)) => {
                let mut lines = vec![line(1.0, format!("✗ {}", problem.message), theme.error)];
                lines.extend(problem.hint.clone().map(|hint| line(2.0, hint, theme.muted)));
                lines
            }
            Stage::Waiting | Stage::Sending => {
                let text = match ask.prompt.trim() {
                    "" => format!("slinging {}…", count(ask)),
                    prompt => format!("slinging {}: {prompt}", count(ask)),
                };
                vec![line(0.0, text, theme.muted)]
            }
        }
    }

    /// `lines` (text and colour) under the open entry, after its own: what
    /// dictation says.
    pub fn under(&self, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<Label> {
        let Some((_, frame, Stage::Typing(problem))) = &self.open else { return Vec::new() };
        let first = problem.as_ref().map_or(1, |p| 2 + u32::from(p.hint.is_some()));
        let placed = lines.into_iter().zip(first..);
        placed.map(|((text, colour), n)| line(*frame, f64::from(n), text, colour, theme)).collect()
    }
}

/// Line `n` (0 is the entry's own) of the lines under the entry at `frame`.
fn line(frame: Rect, n: f64, text: String, colour: Rgb, theme: &Theme) -> Label {
    let frame = Rect { y: frame.y + n * LINE + (HEIGHT - LINE), height: LINE, ..frame };
    Label { text, colour, fill: Some(theme.background), frame }
}

/// What a sling of `selection` with `prompt` sends.
fn ask(selection: &Selection, prompt: String) -> Ask {
    let Selection { canvas, page, ids } = selection.clone();
    Ask { canvas, page, ids, prompt }
}

/// "1 item", "3 items".
fn count(ask: &Ask) -> String {
    match ask.ids.len() {
        1 => "1 item".to_string(),
        n => format!("{n} items"),
    }
}

/// The entry's frame for a selection whose box is `around`, in `within`:
/// under the box, left-aligned with it; above it when there is no room
/// below; moved inside `within` when it would cross an edge.
fn place(around: Rect, within: Rect) -> Rect {
    let below = around.y + around.height + GAP;
    let y = if below + HEIGHT + GAP <= within.y + within.height {
        below
    } else {
        (around.y - GAP - HEIGHT).max(within.y + GAP)
    };
    let x = around.x.min(within.x + within.width - WIDTH - GAP).max(within.x + GAP);
    Rect { x, y, width: WIDTH, height: HEIGHT }
}

#[cfg(test)]
mod tests;
