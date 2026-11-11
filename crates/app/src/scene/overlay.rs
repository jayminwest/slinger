//! The key overlay (`show-keys`, SPEC D21): a panel over the dimmed window
//! that lists every command by group, each with its live key. Pure: `main`
//! hands in the keys and presses while it is open, and reads back the
//! [`Overlay`] to draw.
//!
//! The rows come from the registry ([`Command::ALL`], [`Command::group`],
//! [`Command::title`]) and the keys from the live [`Keymap`], so a new
//! command or a rebind shows with no change here. While open, the overlay
//! takes every key and every press: escape or the `show-keys` key closes
//! it, and so does a press outside the panel.

use crate::commands::{Chord, Command, Group, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Size};
use crate::scene::Label;

/// A panel over the dimmed window, drawn over everything else.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    /// The window, which `dim` covers at `opacity`.
    pub frame: Rect,
    pub dim: Rgb,
    pub opacity: f64,
    /// In order, each over the ones before it.
    pub lines: Vec<Line>,
}

/// One line of an [`Overlay`]: a label in a font of `size` points, set
/// against one side of its frame or centred in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub label: Label,
    pub size: f64,
    pub bold: bool,
    pub align: Align,
}

/// Where a [`Line`]'s text sits in its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// How much of the window shows through the dim.
const OPACITY: f64 = 0.7;
/// The least space between the panel and the window's edge.
const MARGIN: f64 = 16.0;
/// The panel's widest.
const MAX_WIDTH: f64 = 760.0;
/// The space inside the panel's edge.
const PAD: f64 = 12.0;
/// The title's line.
const HEADER: f64 = 28.0;
/// The space between two columns.
const GAP: f64 = 20.0;
/// A row's tallest; a small window makes the rows shorter.
const ROW: f64 = 22.0;
/// A key chip's width.
const CHIP: f64 = 64.0;
/// What a row's chip says when the command has no key.
const NO_KEY: &str = "no key";
const COLUMNS: u32 = 3;

const ESCAPE: Chord = Chord::new(0, '\u{1b}');

/// The column a group shows in: tools, pop-ups and sling, then the canvas
/// and its pages, then the panes and the window.
const fn column(group: Group) -> u32 {
    match group {
        Group::Tools | Group::PopUps | Group::Sling => 0,
        Group::Canvas | Group::Pages => 1,
        Group::Panes | Group::Window => 2,
    }
}

/// The groups of column `n`, in the registry's order.
fn groups(n: u32) -> impl Iterator<Item = Group> {
    Group::ALL.into_iter().filter(move |&g| column(g) == n)
}

/// The commands of `group`, in the registry's order.
fn commands(group: Group) -> impl Iterator<Item = Command> {
    Command::ALL.into_iter().map(|(c, _)| c).filter(move |c| c.group() == group)
}

/// The tallest column, in rows: a heading and a row per command for each
/// group, and half a row between two groups.
fn rows() -> f64 {
    let group = |g| 1.0 + commands(g).map(|_| 1.0).sum::<f64>() + 0.5;
    (0..COLUMNS).map(|n| groups(n).map(group).sum::<f64>() - 0.5).fold(0.0, f64::max)
}

/// The panel in a window of `size`, centred, and its row's height.
fn place(size: Size) -> (Rect, f64) {
    let width = (size.width - 2.0 * MARGIN).min(MAX_WIDTH);
    let room = size.height - 2.0 * (MARGIN + PAD) - HEADER;
    let row = (room / rows()).min(ROW);
    let height = 2.0 * PAD + HEADER + rows() * row;
    let (x, y) = ((size.width - width) / 2.0, (size.height - height) / 2.0);
    (Rect { x, y, width, height }, row)
}

/// Whether the key overlay is open.
#[derive(Debug, Default)]
pub struct KeyOverlay {
    open: bool,
}

impl KeyOverlay {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The `show-keys` command: opens the overlay, or closes it.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// A key press while open: escape or the key `keys` gives `show-keys`
    /// closes it; every key is used up (true). Closed, no key is its (false).
    pub fn key(&mut self, chord: Chord, keys: &Keymap) -> bool {
        if !self.open {
            return false;
        }
        if chord == ESCAPE || keys.command(chord) == Some(Command::ShowKeys) {
            self.open = false;
        }
        true
    }

    /// A press at `at` in a window of `size` while open: one outside the
    /// panel closes it; every press is used up (true). Closed, false.
    pub fn press(&mut self, at: Point, size: Size) -> bool {
        if !self.open {
            return false;
        }
        if !place(size).0.contains(at) {
            self.open = false;
        }
        true
    }

    /// The open overlay in a window of `size`, in `theme`'s colours, with
    /// the keys of `keys`; none when it is closed.
    pub fn shown(&self, keys: &Keymap, theme: &Theme, size: Size) -> Option<Overlay> {
        self.open.then(|| overlay(keys, theme, size))
    }
}

/// The overlay's lines: the panel and its edge, the title and how to close
/// it, then each column's groups, each a heading over a row per command
/// with the command's title and its key's chip.
fn overlay(keys: &Keymap, theme: &Theme, size: Size) -> Overlay {
    let (panel, row) = place(size);
    let font = (row * 0.6).min(13.0);
    let line = |text: String, colour, fill, frame, size, align| Line {
        label: Label { text, colour, fill, frame },
        size,
        bold: false,
        align,
    };
    let filled =
        |colour, frame| line(String::new(), colour, Some(colour), frame, font, Align::Left);
    let mut lines = vec![filled(theme.muted, panel), filled(theme.background, panel.outset(-1.0))];

    let (x, y) = (panel.x + PAD, panel.y + PAD);
    let half = (panel.width - 2.0 * PAD) / 2.0;
    let title = Rect { x, y, width: half, height: HEADER };
    lines.push(Line {
        bold: true,
        ..line("Keys".into(), theme.text, None, title, 15.0, Align::Left)
    });
    let mut note = String::from("from config.toml · esc");
    if let Some(chord) = keys.chord(Command::ShowKeys) {
        note.push_str(" or ");
        note.push_str(&chord.glyphs());
    }
    note.push_str(" closes");
    let right = Rect { x: x + half, ..title };
    lines.push(line(note, theme.muted, None, right, 11.0, Align::Right));

    let columns = f64::from(COLUMNS);
    let width = (panel.width - 2.0 * PAD - GAP * (columns - 1.0)) / columns;
    for n in 0..COLUMNS {
        let x = x + f64::from(n) * (width + GAP);
        let mut y = y + HEADER;
        for group in groups(n) {
            let frame = Rect { x, y, width, height: row };
            let heading =
                line(group.title().into(), theme.muted, None, frame, font * 0.85, Align::Left);
            lines.push(Line { bold: true, ..heading });
            y += row;
            for command in commands(group) {
                let name = Rect { x, y, width: width - CHIP - 4.0, height: row };
                lines.push(line(command.title().into(), theme.text, None, name, font, Align::Left));
                let chip = Rect { x: x + width - CHIP, y: y + 1.0, width: CHIP, height: row - 2.0 };
                lines.push(match keys.chord(command) {
                    Some(chord) => {
                        let fill = Some(theme.muted);
                        line(chord.glyphs(), theme.background, fill, chip, font, Align::Centre)
                    }
                    None => line(NO_KEY.into(), theme.muted, None, chip, font, Align::Centre),
                });
                y += row;
            }
            y += row / 2.0;
        }
    }
    Overlay { frame: size.rect(), dim: theme.background, opacity: OPACITY, lines }
}

#[cfg(test)]
mod tests;
