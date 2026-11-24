//! The key overlay (`show-keys`, SPEC D21): a panel over the dimmed window
//! that lists every command by group, each with its live key. Pure: `main`
//! hands in the keys and the pen while it is open, runs the command it
//! gives back as it runs a key's, and reads back the [`Overlay`] to draw.
//!
//! The rows come from the registry ([`Command::ALL`], [`Command::group`],
//! [`Command::title`]) and the keys from the live [`Keymap`], so a new
//! command or a rebind shows with no change here. While open, the overlay
//! takes every key and every press. It works with the pen alone: each row
//! is a target at least [`style::TAP`] tall that runs its command when the
//! pen lifts on the row it went down on (the row under the pen lights up,
//! and more when pressed); a drag, a scroll or the arrow keys scroll rows
//! that do not fit; a press outside the panel or on its ✕ closes it, as
//! escape or the `show-keys` key do. After the groups come the fixed keys
//! ([`fixed`]): the caret keys of a typed text and the pans, rows with no
//! target.

use crate::commands::{Chord, Command, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Size};
use crate::scene::Label;
use crate::scene::palette::Taken;
use crate::scene::style;

pub mod fixed;
mod place;
use place::{NOTE, SPACE, Section, TITLE, place, wide};

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
/// against one side of its frame or centred in it, and centred top to
/// bottom. Its fill (the label's) has corners of `radius` points, and
/// `edge`, when set, is a hairline just inside its frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub label: Label,
    pub size: f64,
    pub bold: bool,
    pub align: Align,
    pub radius: f64,
    pub edge: Option<Rgb>,
}

/// Where a [`Line`]'s text sits in its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

impl Line {
    /// `text` in `colour`, `size` points, in `frame`, on no fill.
    pub fn text(text: String, colour: Rgb, frame: Rect, size: f64, align: Align) -> Line {
        let label = Label { text, colour, fill: None, frame };
        Line { label, size, bold: false, align, radius: 0.0, edge: None }
    }

    /// A box of `fill` over `frame`, its corners `radius` round.
    pub fn fill(fill: Rgb, frame: Rect, radius: f64) -> Line {
        let line = Line::text(String::new(), fill, frame, 13.0, Align::Left);
        Line { label: Label { fill: Some(fill), ..line.label }, radius, ..line }
    }

    /// A key chip: `text` in `theme`'s text colour on `fill`, centred in
    /// `frame`.
    pub fn chip(text: String, theme: &Theme, fill: Rgb, frame: Rect) -> Line {
        let line = Line::text(text, theme.text, frame, style::CHIP_TEXT, Align::Centre);
        Line { label: Label { fill: Some(fill), ..line.label }, radius: style::CHIP_RADIUS, ..line }
    }
}

/// How wide a chip for `text` is: room for its glyphs, as the system font
/// sets them, and its padding.
pub fn chip_width(text: &str) -> f64 {
    wide(text, style::CHIP_TEXT) + CHIP_PAD
}

/// A chip's padding past its label, across.
const CHIP_PAD: f64 = 8.0;

/// A chip's height.
pub const CHIP_HIGH: f64 = 20.0;

/// How much of the window the dim hides.
const OPACITY: f64 = 0.75;
/// The least space between the panel and the window's edge.
const MARGIN: f64 = 16.0;
/// The panel's width when its columns need no more (or the window's, less
/// its margin, when narrower).
const WIDTH: f64 = 840.0;
/// The space inside the panel's edge: across (less in a narrow panel), and
/// above and below.
const PAD: f64 = 28.0;
const NARROW_PAD: f64 = 16.0;
const PAD_Y: f64 = 20.0;
/// The title's line, and the space under it.
const HEADER: f64 = 28.0;
const UNDER_HEADER: f64 = 12.0;
/// A group's heading, and the space above each group after the first.
const HEADING: f64 = 22.0;
const GROUP_GAP: f64 = 12.0;
/// A row: tall enough for the pen.
const ROW: f64 = style::TAP;
/// The space between two columns (less in a narrow panel).
const GAP: f64 = 28.0;
const NARROW_GAP: f64 = 16.0;
/// How far a row's highlight reaches past its text on either side.
const REACH: f64 = 8.0;
/// How far the pen moves while down before it scrolls, not taps.
const SLOP: f64 = 6.0;
/// The scroll bar's width.
const BAR: f64 = 3.0;
/// What a row's chip says when the command has no key.
const NO_KEY: &str = "no key";
const CLOSE: &str = "✕";
/// The panel's title.
const KEYS: &str = "Keys";
/// The most columns, and the least width of one (each is as wide as its
/// widest row needs, [`place::widths`]).
const COLUMNS: u32 = 3;
const MIN_COLUMN: f64 = 220.0;

const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const UP: Chord = Chord::new(0, '\u{f700}');
const DOWN: Chord = Chord::new(0, '\u{f701}');

/// A press on the panel, until the pen lifts: where it went down, where it
/// is now, and whether it has moved far enough to scroll.
#[derive(Debug, Clone, Copy)]
struct Held {
    from: Point,
    last: Point,
    dragged: bool,
}

/// The key overlay: whether it is open, how far its rows are scrolled,
/// where the pen is over it, the press on it, if any, and how wide each
/// section's rows are with the keys it last showed.
#[derive(Debug, Default)]
pub struct KeyOverlay {
    open: bool,
    scroll: f64,
    pointer: Option<Point>,
    held: Option<Held>,
    widths: Vec<f64>,
    /// Whether Nvim edits the texts: a typed text's keys are its.
    nvim: bool,
}

impl KeyOverlay {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether Nvim edits the texts typed: the rows for a typed text name
    /// its keys.
    pub fn set_nvim(&mut self, on: bool) {
        self.nvim = on;
    }

    /// The `show-keys` command: opens the overlay at its top, or closes it.
    pub fn toggle(&mut self) {
        *self = KeyOverlay {
            open: !self.open,
            widths: std::mem::take(&mut self.widths),
            nvim: self.nvim,
            ..KeyOverlay::default()
        };
    }

    /// A key press while open: escape or the key `keys` gives `show-keys`
    /// closes it, up and down scroll a row; every key is used up (true).
    /// Closed, no key is its (false).
    pub fn key(&mut self, chord: Chord, keys: &Keymap) -> bool {
        if !self.open {
            return false;
        }
        if chord == ESCAPE || keys.command(chord) == Some(Command::ShowKeys) {
            self.toggle();
        } else if chord == UP || chord == DOWN {
            self.scroll += if chord == UP { -ROW } else { ROW };
        }
        true
    }

    /// A command chosen from the menu bar while open: the overlay closes,
    /// and the command runs, except `show-keys`, which only closed it.
    /// Closed, [`Taken::No`].
    pub fn chosen(&mut self, command: Command) -> Taken {
        if !self.open {
            return Taken::No;
        }
        self.toggle();
        if command == Command::ShowKeys { Taken::Yes } else { Taken::Run(command) }
    }

    /// A press at `at` in a window of `size` while open: one outside the
    /// panel or on its ✕ closes it; one inside holds until the pen lifts
    /// ([`KeyOverlay::lift`]). Closed, [`Taken::No`].
    pub fn press(&mut self, at: Point, size: Size) -> Taken {
        if !self.open {
            return Taken::No;
        }
        let place = place(size, self.scroll, &self.widths, self.nvim);
        if !place.panel.contains(at) || place.close.contains(at) {
            self.toggle();
        } else {
            (self.held, self.pointer) =
                (Some(Held { from: at, last: at, dragged: false }), Some(at));
        }
        Taken::Yes
    }

    /// The pen, down, moved to `at`: past a small slop the press scrolls
    /// the rows with it instead of tapping. True while open: the overlay
    /// takes it.
    pub fn drag(&mut self, at: Point, size: Size) -> bool {
        if let Some(held) = &mut self.held {
            let far = (at.x - held.from.x).hypot(at.y - held.from.y) > SLOP;
            held.dragged |= far;
            if held.dragged {
                self.scroll =
                    place(size, self.scroll + held.last.y - at.y, &self.widths, self.nvim).scroll;
            }
            held.last = at;
        }
        self.pointer = self.open.then_some(at);
        self.open
    }

    /// The pen lifted at `at`: a press that did not scroll, lifted on the
    /// row it went down on, closes the overlay to run that row's command,
    /// as its key would ([`Taken::Run`]); `show-keys`'s row just closes it.
    /// Closed, [`Taken::No`].
    pub fn lift(&mut self, at: Point, size: Size) -> Taken {
        if !self.open {
            return Taken::No;
        }
        let Some(held) = self.held.take() else { return Taken::Yes };
        let place = place(size, self.scroll, &self.widths, self.nvim);
        match place.row_at(held.from) {
            Some(command) if !held.dragged && place.row_at(at) == Some(command) => {
                self.toggle();
                if command == Command::ShowKeys { Taken::Yes } else { Taken::Run(command) }
            }
            _ => Taken::Yes,
        }
    }

    /// The pen or the pointer, up, moved to `at`: the row under it lights
    /// up. True when that changed what shows.
    pub fn hover(&mut self, at: Point, size: Size) -> bool {
        if !self.open {
            return false;
        }
        let place = place(size, self.scroll, &self.widths, self.nvim);
        let before = self.pointer.and_then(|p| place.row_at(p));
        self.pointer = Some(at);
        before != place.row_at(at)
    }

    /// A scroll by `dy` points (down the page is negative) while open: the
    /// rows follow it. True while open.
    pub fn scroll(&mut self, dy: f64, size: Size) -> bool {
        if self.open {
            self.scroll = place(size, self.scroll - dy, &self.widths, self.nvim).scroll;
        }
        self.open
    }

    /// The open overlay in a window of `size`, in `theme`'s colours, with
    /// the keys of `keys`; none when it is closed. Its columns are as wide
    /// as those keys need from now on, so a press finds the row it shows.
    pub fn shown(&mut self, keys: &Keymap, theme: &Theme, size: Size) -> Option<Overlay> {
        if self.open {
            self.widths = place::widths(keys, self.nvim);
        }
        self.open.then(|| self.overlay(keys, theme, size))
    }

    /// The panel, its title, how to close it and its ✕; the row under the
    /// pen lit (more while pressed); each shown group's heading and each
    /// shown row with the command's title and its key's chip; and, when the
    /// rows do not all fit, where the shown ones are among them.
    fn overlay(&self, keys: &Keymap, theme: &Theme, size: Size) -> Overlay {
        let place = place(size, self.scroll, &self.widths, self.nvim);
        let panel = Line {
            edge: Some(style::edge(theme)),
            ..Line::fill(style::panel(theme), place.panel, style::PANEL_RADIUS)
        };
        let mut lines = vec![panel];
        let (x, y) = (place.body.x, place.panel.y + PAD_Y);
        let title = Rect { x, y, width: wide(KEYS, 16.0) + SPACE, height: HEADER };
        lines.push(Line {
            bold: true,
            ..Line::text(KEYS.into(), theme.text, title, 16.0, Align::Left)
        });
        let mut note = String::from("from config.toml · esc");
        if let Some(chord) = keys.chord(Command::ShowKeys) {
            note.push_str(" or ");
            note.push_str(&chord.glyphs());
        }
        note.push_str(" closes");
        let right =
            Rect { x: x + title.width, width: place.close.x - x - title.width - 8.0, ..title };
        lines.push(Line::text(note, theme.muted, right, 12.0, Align::Right));
        lines.push(Line::text(CLOSE.into(), theme.muted, place.close, 15.0, Align::Centre));

        let pressed = self.held.filter(|h| !h.dragged).map(|h| (h.last, style::selected(theme)));
        let lit = pressed.or(self.pointer.map(|p| (p, style::hover(theme))));
        let lit = lit.and_then(|(at, fill)| Some((place.row_at(at)?, fill)));
        for &(section, frame) in &place.headings {
            if place.shows(frame) {
                let heading = section.title().to_uppercase();
                lines.push(Line::text(heading, theme.muted, frame, 11.0, Align::Left));
            }
            if let Section::Fixed(keys) = section {
                let rows = place.fixed.iter().filter(|(r, _)| keys.rows.contains(*r));
                for &(row, frame) in rows.filter(|(_, f)| place.shows(*f)) {
                    lines.extend(labels(row.title, Some(row.keys.shown()), theme, frame));
                }
                continue;
            }
            let rows = place
                .rows
                .iter()
                .filter(|(c, f)| Section::Commands(c.group()) == section && place.shows(*f));
            for &(command, frame) in rows {
                if let Some((_, fill)) = lit.filter(|(c, _)| *c == command) {
                    let reach =
                        Rect { x: frame.x - REACH, width: frame.width + 2.0 * REACH, ..frame };
                    lines.push(Line::fill(fill, reach, style::ROW_RADIUS));
                }
                lines.extend(row(command, keys, theme, frame));
            }
        }
        if let Some(bar) = place.bar() {
            lines.push(Line::fill(style::chip(theme), bar, BAR / 2.0));
        }
        Overlay { frame: size.rect(), dim: theme.background, opacity: OPACITY, lines }
    }
}

/// A command's row in `frame`: its title, then its key's chip at the
/// right, or "no key".
fn row(command: Command, keys: &Keymap, theme: &Theme, frame: Rect) -> Vec<Line> {
    labels(command.title(), keys.chord(command).map(Chord::glyphs), theme, frame)
}

/// A row in `frame`: `title`, then `key`'s chip at the right, or "no key",
/// as wide as it needs; the title has the rest.
fn labels(title: &str, key: Option<String>, theme: &Theme, frame: Rect) -> Vec<Line> {
    let wide = place::key_wide(key.as_deref()).min(frame.width);
    let name = Rect { width: (frame.width - wide - SPACE).max(0.0), ..frame };
    let mut lines = vec![Line::text(title.into(), theme.text, name, TITLE, Align::Left)];
    let y = frame.y + (frame.height - CHIP_HIGH) / 2.0;
    let chip = Rect { x: frame.x + frame.width - wide, y, width: wide, height: CHIP_HIGH };
    lines.push(match key {
        Some(glyphs) => Line::chip(glyphs, theme, style::chip(theme), chip),
        None => Line::text(NO_KEY.into(), theme.muted, chip, NOTE, Align::Right),
    });
    lines
}

#[cfg(test)]
mod tests;
