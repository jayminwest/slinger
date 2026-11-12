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
//! escape or the `show-keys` key do.

use crate::commands::{Chord, Command, Group, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Size};
use crate::scene::Label;
use crate::scene::palette::Taken;
use crate::scene::style;

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

/// How wide a chip for `text` is: room for its glyphs and its padding.
pub fn chip_width(text: &str) -> f64 {
    f64::from(u32::try_from(text.chars().count()).unwrap_or(u32::MAX)) * 9.0 + 14.0
}

/// A chip's height.
pub const CHIP_HIGH: f64 = 20.0;

/// How much of the window the dim hides.
const OPACITY: f64 = 0.75;
/// The least space between the panel and the window's edge.
const MARGIN: f64 = 16.0;
/// The panel's widest.
const MAX_WIDTH: f64 = 840.0;
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
const COLUMNS: u32 = 3;

const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const UP: Chord = Chord::new(0, '\u{f700}');
const DOWN: Chord = Chord::new(0, '\u{f701}');

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

/// How tall a group is: its heading and a row per command.
fn tall(group: Group) -> f64 {
    HEADING + commands(group).map(|_| ROW).sum::<f64>()
}

/// The tallest column's height: its groups, a gap between two.
fn content() -> f64 {
    let column = |n| groups(n).map(|g| tall(g) + GROUP_GAP).sum::<f64>() - GROUP_GAP;
    (0..COLUMNS).map(column).fold(0.0, f64::max)
}

/// Where the overlay's parts sit in a window, scrolled.
struct Place {
    panel: Rect,
    /// The ✕ that closes it, at the header's right.
    close: Rect,
    /// Where the rows show; a row not wholly in it is not shown.
    body: Rect,
    /// How far the rows are scrolled, and the most they can be.
    scroll: f64,
    most: f64,
    /// Each group's heading and each command's row, where they are now.
    headings: Vec<(Group, Rect)>,
    rows: Vec<(Command, Rect)>,
}

/// The panel in a window of `size`, centred, as tall as its rows or the
/// window allows, with its rows scrolled by `scroll` (kept in range).
fn place(size: Size, scroll: f64) -> Place {
    let width = (size.width - 2.0 * MARGIN).clamp(0.0, MAX_WIDTH);
    let (pad, gap) = if width < 600.0 { (NARROW_PAD, NARROW_GAP) } else { (PAD, GAP) };
    let room = (size.height - 2.0 * MARGIN).max(0.0);
    let height = (2.0 * PAD_Y + HEADER + UNDER_HEADER + content()).min(room);
    let (x, y) = ((size.width - width) / 2.0, (size.height - height) / 2.0);
    let panel = Rect { x, y, width, height };
    let close = Rect { x: x + width - pad / 2.0 - ROW, y: y + PAD_Y, width: ROW, height: HEADER };
    let top = y + PAD_Y + HEADER + UNDER_HEADER;
    let body_high = (y + height - PAD_Y - top).max(0.0);
    let body = Rect { x: x + pad, y: top, width: (width - 2.0 * pad).max(0.0), height: body_high };
    let most = (content() - body.height).max(0.0);
    let scroll = scroll.clamp(0.0, most);
    let columns = f64::from(COLUMNS);
    let column = ((body.width - gap * (columns - 1.0)) / columns).max(0.0);
    let (mut headings, mut rows) = (Vec::new(), Vec::new());
    for n in 0..COLUMNS {
        let x = body.x + f64::from(n) * (column + gap);
        let mut y = body.y - scroll;
        for group in groups(n) {
            headings.push((group, Rect { x, y, width: column, height: HEADING }));
            y += HEADING;
            for command in commands(group) {
                rows.push((command, Rect { x, y, width: column, height: ROW }));
                y += ROW;
            }
            y += GROUP_GAP;
        }
    }
    Place { panel, close, body, scroll, most, headings, rows }
}

impl Place {
    /// Whether `frame` is wholly in the body: it shows.
    fn shows(&self, frame: Rect) -> bool {
        let end = self.body.y + self.body.height + 0.5;
        frame.y >= self.body.y - 0.5 && frame.y + frame.height <= end
    }

    /// The row, shown, whose target holds `at`: the row, reaching past
    /// its text on either side.
    fn row_at(&self, at: Point) -> Option<Command> {
        let target =
            |frame: Rect| Rect { x: frame.x - REACH, width: frame.width + 2.0 * REACH, ..frame };
        let mut shown = self.rows.iter().filter(|(_, frame)| self.shows(*frame));
        shown.find(|(_, frame)| target(*frame).contains(at)).map(|&(command, _)| command)
    }
}

/// A press on the panel, until the pen lifts: where it went down, where it
/// is now, and whether it has moved far enough to scroll.
#[derive(Debug, Clone, Copy)]
struct Held {
    from: Point,
    last: Point,
    dragged: bool,
}

/// The key overlay: whether it is open, how far its rows are scrolled,
/// where the pen is over it, and the press on it, if any.
#[derive(Debug, Default)]
pub struct KeyOverlay {
    open: bool,
    scroll: f64,
    pointer: Option<Point>,
    held: Option<Held>,
}

impl KeyOverlay {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The `show-keys` command: opens the overlay at its top, or closes it.
    pub fn toggle(&mut self) {
        *self = KeyOverlay { open: !self.open, ..KeyOverlay::default() };
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

    /// A press at `at` in a window of `size` while open: one outside the
    /// panel or on its ✕ closes it; one inside holds until the pen lifts
    /// ([`KeyOverlay::lift`]). Closed, [`Taken::No`].
    pub fn press(&mut self, at: Point, size: Size) -> Taken {
        if !self.open {
            return Taken::No;
        }
        let place = place(size, self.scroll);
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
                self.scroll = place(size, self.scroll + held.last.y - at.y).scroll;
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
        let place = place(size, self.scroll);
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
        let place = place(size, self.scroll);
        let before = self.pointer.and_then(|p| place.row_at(p));
        self.pointer = Some(at);
        before != place.row_at(at)
    }

    /// A scroll by `dy` points (down the page is negative) while open: the
    /// rows follow it. True while open.
    pub fn scroll(&mut self, dy: f64, size: Size) -> bool {
        if self.open {
            self.scroll = place(size, self.scroll - dy).scroll;
        }
        self.open
    }

    /// The open overlay in a window of `size`, in `theme`'s colours, with
    /// the keys of `keys`; none when it is closed.
    pub fn shown(&self, keys: &Keymap, theme: &Theme, size: Size) -> Option<Overlay> {
        self.open.then(|| self.overlay(keys, theme, size))
    }

    /// The panel, its title, how to close it and its ✕; the row under the
    /// pen lit (more while pressed); each shown group's heading and each
    /// shown row with the command's title and its key's chip; and, when the
    /// rows do not all fit, where the shown ones are among them.
    fn overlay(&self, keys: &Keymap, theme: &Theme, size: Size) -> Overlay {
        let place = place(size, self.scroll);
        let panel = Line {
            edge: Some(style::edge(theme)),
            ..Line::fill(style::panel(theme), place.panel, style::PANEL_RADIUS)
        };
        let mut lines = vec![panel];
        let (x, y) = (place.body.x, place.panel.y + PAD_Y);
        let title = Rect { x, y, width: place.body.width / 2.0, height: HEADER };
        lines.push(Line {
            bold: true,
            ..Line::text("Keys".into(), theme.text, title, 16.0, Align::Left)
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
        for &(group, frame) in &place.headings {
            if place.shows(frame) {
                let heading = group.title().to_uppercase();
                lines.push(Line::text(heading, theme.muted, frame, 11.0, Align::Left));
            }
            let rows = place.rows.iter().filter(|(c, f)| c.group() == group && place.shows(*f));
            for &(command, frame) in rows {
                if let Some((_, fill)) = lit.filter(|(c, _)| *c == command) {
                    let reach =
                        Rect { x: frame.x - REACH, width: frame.width + 2.0 * REACH, ..frame };
                    lines.push(Line::fill(fill, reach, style::ROW_RADIUS));
                }
                lines.extend(row(command, keys, theme, frame));
            }
        }
        if place.most > 0.0 {
            let body = place.body;
            let high = body.height * body.height / (body.height + place.most);
            let y = body.y + (body.height - high) * place.scroll / place.most;
            let x = place.panel.x + place.panel.width - (body.x - place.panel.x) / 2.0 - BAR / 2.0;
            let bar = Rect { x, y, width: BAR, height: high };
            lines.push(Line::fill(style::chip(theme), bar, BAR / 2.0));
        }
        Overlay { frame: size.rect(), dim: theme.background, opacity: OPACITY, lines }
    }
}

/// A command's row in `frame`: its title, then its key's chip at the
/// right, or "no key".
fn row(command: Command, keys: &Keymap, theme: &Theme, frame: Rect) -> Vec<Line> {
    let key = keys.chord(command).map(Chord::glyphs);
    let wide = key.as_deref().map_or(chip_width(NO_KEY), chip_width).min(frame.width / 2.0);
    let name = Rect { width: (frame.width - wide - 8.0).max(0.0), ..frame };
    let mut lines = vec![Line::text(command.title().into(), theme.text, name, 13.0, Align::Left)];
    let y = frame.y + (frame.height - CHIP_HIGH) / 2.0;
    let chip = Rect { x: frame.x + frame.width - wide, y, width: wide, height: CHIP_HIGH };
    lines.push(match key {
        Some(glyphs) => Line::chip(glyphs, theme, style::chip(theme), chip),
        None => Line::text(NO_KEY.into(), theme.muted, chip, 12.0, Align::Right),
    });
    lines
}

#[cfg(test)]
mod tests;
