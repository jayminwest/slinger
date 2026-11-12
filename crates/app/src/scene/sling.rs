//! The sling from the window (SPEC section 1, section 6, D18): select,
//! prompt, send. Pure: `main` hands in the selection and where it is, the
//! prompt's text as the user types it, the keys and the daemon's answer,
//! and reads back the one-line entry and the labels to draw.
//!
//! The `sling` command opens a one-line entry under the selection's box
//! (above it when there is no room below), inside the focused pane: a
//! labelled field, "Sling to <agent>" above it ("Sling" when the workspace
//! has no agent), its keys beneath, from the live keymap. The entry takes
//! every key a text field uses ([`edits`]) but two: return sends, escape
//! cancels; a chord the keymap binds otherwise runs its command (cmd+Q
//! quits). An empty prompt sends too: the selection is the question.
//!
//! A sling goes once the ink of its canvas is in the daemon: every stroke,
//! erase and move made before it is applied first, so the packet holds what
//! the user saw. Ink drawn after the selection was made is never in it (only
//! the server's items can be selected), but it goes in the packet's new ink
//! like any other. While it waits and while it goes, a muted line stands
//! where the entry was. What the agent does next is [`crate::scene::slung`]'s
//! to show, so a sling that went simply closes. A typed sling that failed
//! opens the entry again with its prompt, and the error's message and hint
//! under it: return tries again, escape gives up.
//!
//! [`Sling::send_now`] is the one entry point for slings that need no
//! typing (a tap on a "Sling" button). A sling with nothing selected sends
//! what is new on its page. One that fails closes, and says why where the
//! pen tapped ([`crate::scene::slung`]): the pen cannot press escape.

use crate::backend::Problem;
use crate::commands::{CMD, CTRL, Chord};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::canvas::ItemId;
use crate::scene::select::Selection;
use crate::scene::sidebar::{Font, Row};
use crate::scene::{Border, Label, style};

pub mod buttons;
pub mod pen;

/// The entry's box, in points.
pub const WIDTH: f64 = 360.0;
pub const HEIGHT: f64 = 44.0;
/// The space between the selection's box and the field, and the closest
/// the field comes to an edge of its pane.
pub const GAP: f64 = 8.0;
/// The height of a line under the entry.
const LINE: f64 = 22.0;
/// The height of the label above the box and of the keys beneath it.
pub const SMALL: f64 = 16.0;
/// The label's and the keys' text size, in points.
const SMALL_TEXT: Font = Font { size: Some(11.0), bold: false, spacing: 0.0 };
/// The text inside the box: its height, its size, and its space from the
/// box's side.
const TEXT: f64 = 20.0;
const TEXT_SIZE: f64 = 15.0;
const PAD: f64 = 12.0;
/// The box's border width.
const EDGE: f64 = 1.0;
/// The room a pane keeps at its top for the flash, over a centred entry.
const FLASH: f64 = 40.0;
/// The key that sends, and the one that cancels.
pub const SEND: Chord = Chord::new(0, '\r');
pub const CANCEL: Chord = Chord::new(0, '\u{1b}');

/// What a sling sends: a page's selected items, by the server's ids, and
/// the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub canvas: String,
    pub page: String,
    pub ids: Vec<ItemId>,
    pub prompt: String,
}

/// The one-line entry the window shows: the box and the text it starts
/// with, the label above it and the keys beneath. Every entry (the sling's
/// prompt, a note of feedback, a page's new name) looks the same
/// ([`Entry::framed`]), and the platform gives each the keyboard and its
/// caret the same way.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The box: filled with `fill`, inside `border`, its corners `radius`
    /// round.
    pub frame: Rect,
    pub fill: Rgb,
    pub border: Border,
    pub radius: f64,
    /// Where the text is typed, inside the box, its colour (the caret's
    /// too) and its size in points.
    pub field: Rect,
    pub colour: Rgb,
    pub size: f64,
    pub text: String,
    /// Shown while the text is empty.
    pub placeholder: String,
    /// "Sling to scout", above the box.
    pub title: Row,
    /// Beneath the box: how to send and cancel, from its left edge; how to
    /// dictate, from its right edge, when a key does.
    pub keys: Row,
    pub dictate: Option<Row>,
    /// The word on the pen's "✓" button beside the box ([`pen`]): "Sling",
    /// "Save" or "Rename"; none for an entry with no buttons.
    pub done: Option<&'static str>,
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
    /// `typed`: it came from the entry, which a failure opens again.
    Waiting { typed: bool },
    /// With the daemon.
    Sending { typed: bool },
}

/// What became of the sling the daemon answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    /// No sling was on its way.
    Not,
    /// It went, or it failed with no entry to open: it closed.
    Closed,
    /// A typed sling failed: its entry is open again, with why.
    Reopened,
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
        if matches!(self.open, Some((_, _, Stage::Waiting { .. } | Stage::Sending { .. }))) {
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
        if matches!(self.open, Some((_, _, Stage::Waiting { .. } | Stage::Sending { .. }))) {
            return false;
        }
        let ask = ask(selection, prompt.to_string());
        self.open = Some((ask, place(around, within), Stage::Waiting { typed: false }));
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
        self.is_typing().then(|| Key::of(chord))
    }

    /// The user sent the entry's prompt: it waits for its canvas's ink.
    /// False when the entry is closed.
    pub fn send(&mut self) -> bool {
        let Some((_, _, stage @ Stage::Typing(_))) = &mut self.open else { return false };
        *stage = Stage::Waiting { typed: true };
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
        let Some((ask, _, stage)) = &mut self.open else { return None };
        let Stage::Waiting { typed } = *stage else { return None };
        if !settled(&ask.canvas) {
            return None;
        }
        *stage = Stage::Sending { typed };
        Some(ask.clone())
    }

    /// The daemon's answer to the sling going: it went, and closes, or it
    /// failed: a typed sling's entry opens again with its prompt and the
    /// problem, and any other closes.
    pub fn answered(&mut self, answer: Result<(), Problem>) -> Answered {
        let Some((_, _, stage)) = &mut self.open else { return Answered::Not };
        let Stage::Sending { typed } = *stage else { return Answered::Not };
        match answer {
            Err(problem) if typed => {
                *stage = Stage::Typing(Some(problem));
                Answered::Reopened
            }
            _ => {
                self.open = None;
                Answered::Closed
            }
        }
    }

    /// The canvas the sling is from, while one is open or on its way.
    pub fn canvas(&self) -> Option<&str> {
        self.open.as_ref().map(|(ask, ..)| ask.canvas.as_str())
    }

    /// The entry, while it is open, in `theme`'s colours: to `agent`, the
    /// agent of its canvas's workspace, if it has one; `dictate` is the
    /// key that dictates, if any.
    pub fn entry(
        &self,
        theme: &Theme,
        agent: Option<&str>,
        dictate: Option<Chord>,
    ) -> Option<Entry> {
        let Some((ask, frame, Stage::Typing(_))) = &self.open else { return None };
        let title = match agent {
            Some(agent) => format!("Sling to {agent}"),
            None => "Sling".to_string(),
        };
        let keys = format!("{} sends · esc cancels", SEND.glyphs());
        let mut entry = Entry::framed(*frame, theme, title, keys);
        entry.text.clone_from(&ask.prompt);
        entry.placeholder = format!("what to ask about {} (or nothing)", count(ask));
        entry.done = Some("Sling");
        Some(entry.dictated(theme, dictate))
    }

    /// Whether `at`, a point in the window, is on the open entry's box: a
    /// press there is the entry's, not the canvas's.
    pub fn covers(&self, at: Point) -> bool {
        matches!(&self.open, Some((_, frame, Stage::Typing(_))) if frame.contains(at))
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
            Stage::Waiting { .. } | Stage::Sending { .. } => {
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
        let after = problem.as_ref().map_or(0, |p| 1 + usize::from(p.hint.is_some()));
        under(*frame, after, theme, lines)
    }
}

impl Entry {
    /// An empty entry whose box is `frame`, in `theme`'s colours, with
    /// `title` above it and `keys` beneath, no placeholder and no dictate
    /// key: the look every one-line entry shares.
    pub fn framed(frame: Rect, theme: &Theme, title: String, keys: String) -> Entry {
        let field = Rect {
            x: frame.x + PAD,
            y: frame.y + (HEIGHT - TEXT) / 2.0,
            width: frame.width - 2.0 * PAD,
            height: TEXT,
        };
        let fill = Some(theme.background);
        Entry {
            frame,
            fill: style::panel(theme),
            border: Border { frame, width: EDGE, colour: theme.focus },
            radius: style::FIELD_RADIUS,
            field,
            colour: theme.text,
            size: TEXT_SIZE,
            text: String::new(),
            placeholder: String::new(),
            title: small(theme, frame, title, frame.y - SMALL, fill),
            keys: small(theme, frame, keys, frame.y + HEIGHT, fill),
            dictate: None,
            done: None,
        }
    }

    /// This entry with `chord`, the key that dictates, if any, beneath its
    /// box from the right edge, in `theme`'s colours.
    #[must_use]
    pub fn dictated(mut self, theme: &Theme, chord: Option<Chord>) -> Entry {
        let text = chord.map(|chord| format!("{} dictate", chord.glyphs()));
        let under = self.frame.y + HEIGHT;
        self.dictate = text.map(|text| small(theme, self.frame, text, under, None));
        self
    }
}

/// An entry's box centred across `within` (a pane), a third of the way
/// down, and never over the flash, which says what the entry did.
pub fn centred(within: Rect) -> Rect {
    let width = WIDTH.min(within.width - 2.0 * GAP).max(0.0);
    let x = within.x + (within.width - width) / 2.0;
    let top = within.y + FLASH + GAP + SMALL;
    let y = (within.y + (within.height - HEIGHT) / 3.0).max(top);
    Rect { x, y, width, height: HEIGHT }
}

impl Key {
    /// What `chord` does in an open entry: return sends, escape cancels,
    /// and any other key is the entry's own.
    pub fn of(chord: Chord) -> Key {
        if chord == SEND {
            Key::Send
        } else if chord == CANCEL {
            Key::Cancel
        } else {
            Key::Type
        }
    }
}

/// Whether `chord`, held with command or control, is a text field's own
/// while the entry is open: cut, copy, paste, select all, undo and redo
/// (command with x, c, v, a or z, shift or not), the caret's moves and
/// deletes to a line's or a word's end (an arrow or a delete with command),
/// and the Emacs keys (control with a letter). Any other chord with command
/// or control is the keymap's: cmd+Q quits with the entry open.
pub fn edits(chord: Chord) -> bool {
    let key = chord.key().to_ascii_lowercase();
    let caret = matches!(key, '\u{7f}' | '\u{8}' | '\u{f700}'..='\u{f703}' | '\u{f728}');
    if chord.mods() & CMD != 0 {
        return matches!(key, 'x' | 'c' | 'v' | 'a' | 'z') || caret;
    }
    chord.mods() & CTRL == 0 || key.is_ascii_lowercase() || caret
}

/// A muted row in small type at `y`, as wide as the entry's box at `frame`.
fn small(theme: &Theme, frame: Rect, text: String, y: f64, fill: Option<Rgb>) -> Row {
    let frame = Rect { y, height: SMALL, ..frame };
    Row { label: Label { text, colour: theme.muted, fill, frame }, font: SMALL_TEXT }
}

/// Line `n` of the lines under the entry's box at `frame`, after its keys;
/// line 0 stands in the box's place.
pub fn line(frame: Rect, n: f64, text: String, colour: Rgb, theme: &Theme) -> Label {
    let y = if n == 0.0 {
        frame.y + (HEIGHT - LINE) / 2.0
    } else {
        frame.y + HEIGHT + SMALL + (n - 1.0) * LINE
    };
    let frame = Rect { y, height: LINE, ..frame };
    Label { text, colour, fill: Some(theme.background), frame }
}

/// `lines` (text and colour) under the entry's box at `frame`, after
/// `after` lines of its own (a problem and its hint).
pub fn under(frame: Rect, after: usize, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<Label> {
    let first = u32::try_from(after).unwrap_or(u32::MAX).saturating_add(1);
    let placed = lines.into_iter().zip(first..);
    placed.map(|((text, colour), n)| line(frame, f64::from(n), text, colour, theme)).collect()
}

/// What a sling of `selection` with `prompt` sends.
fn ask(selection: &Selection, prompt: String) -> Ask {
    let Selection { canvas, page, ids } = selection.clone();
    Ask { canvas, page, ids, prompt }
}

/// "1 item", "3 items"; "what is new" for none (a button's sling).
fn count(ask: &Ask) -> String {
    match ask.ids.len() {
        0 => "what is new".to_string(),
        1 => "1 item".to_string(),
        n => format!("{n} items"),
    }
}

/// The entry's box for a selection whose box is `around`, in `within`:
/// the field (the label, the box, the keys) under the selection,
/// left-aligned with it; above it when there is no room below; moved inside
/// `within` when it or the pen's buttons beside it ([`pen`]) would cross an
/// edge, and narrower in a pane too narrow for both.
fn place(around: Rect, within: Rect) -> Rect {
    let below = around.y + around.height + GAP + SMALL;
    let y = if below + HEIGHT + SMALL + GAP <= within.y + within.height {
        below
    } else {
        (around.y - GAP - SMALL - HEIGHT).max(within.y + GAP + SMALL)
    };
    let width = WIDTH.min(within.width - 2.0 * GAP - pen::ROOM).max(0.0);
    let x = around.x.min(within.x + within.width - width - pen::ROOM - GAP).max(within.x + GAP);
    Rect { x, y, width, height: HEIGHT }
}

#[cfg(test)]
mod tests;
