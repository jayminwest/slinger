//! The command palette (`show-commands`, SPEC D21): a panel over the dimmed
//! window with a search line and a row per command that matches it. Pure:
//! `main` hands in the keys and presses while it is open, runs the command
//! it gives back as it runs a key's, and reads back the [`Overlay`] to draw.
//!
//! The rows come from the registry ([`Command::ALL`], [`Command::title`],
//! [`Command::group`]) and the chips from the live [`Keymap`]. Typing is
//! drawn by the scene, not an `AppKit` field: the palette reads each typed
//! character from its key's [`Chord`], which has no case, so the search is
//! lowercase and matches any case; a paste goes in lowercase too. While
//! open the palette takes every key and every press: escape or the
//! `show-commands` key closes it, return runs the selected row's command,
//! a press on a row holds it and the lift on that row runs it (as on the
//! key overlay and the page menu: a press that slides off runs nothing),
//! a press outside closes it. A command chosen from the menu
//! bar closes it too, and then runs (`show-commands` only closes it).
//!
//! Opened empty it lists the command it last ran first, then the commands
//! by [`Group::ALL`] (the tools first) with Quit last, so return on a
//! just-opened palette never quits. Typed, it ranks the matches: a title
//! that starts with the search, then one with a word that does, then one
//! that holds it in order, then one whose title and group hold it.

use crate::commands::{Chord, Command, Group, Keymap};
use crate::config::Theme;
use crate::layout::{Point, Rect, Size};
use crate::scene::overlay::{Align, CHIP_HIGH, Line, Overlay, chip_width};
use crate::scene::{paste, style};

/// How much of the window the dim hides.
const OPACITY: f64 = 0.55;
/// The panel's widest.
const WIDTH: f64 = 520.0;
/// The least space between the panel and the window's edge.
const MARGIN: f64 = 16.0;
/// The space between the panel's edge and its rows.
const PAD: f64 = 6.0;
/// The space between the panel's edge and the search's and footer's text,
/// and between a row's edge and its text.
const INSET: f64 = 16.0;
const TEXT_PAD: f64 = 10.0;
/// The search line's height.
const SEARCH: f64 = 48.0;
/// A row's height: tall enough for the pen to tap.
const ROW: f64 = 44.0;
/// The footer's height.
const FOOT: f64 = 30.0;
/// A key chip's widest.
const CHIP: f64 = 64.0;
/// The group's column.
const GROUP: f64 = 90.0;
/// The most rows the panel shows at once.
const MOST: u32 = 8;
const FOOTER: &str = "↑↓ move · return runs · esc closes";
const PEN: &str = "the pen can tap a row";
const PLACEHOLDER: &str = "Search commands";
const NONE: &str = "No command matches";
const NO_KEY: &str = "no key";

const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const RETURN: Chord = Chord::new(0, '\r');
const DELETE: Chord = Chord::new(0, '\u{7f}');
const UP: Chord = Chord::new(0, '\u{f700}');
const DOWN: Chord = Chord::new(0, '\u{f701}');

/// What the palette did with a key or a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// It is closed: the key or press goes on.
    No,
    /// It used it up.
    Yes,
    /// It closed to run this command, as its key would.
    Run(Command),
}

/// The palette: whether it is open, what is typed, the selected row, the
/// row the pen holds and the command it last ran.
#[derive(Debug, Default)]
pub struct Palette {
    open: bool,
    query: String,
    /// The selected row among the matches.
    at: usize,
    /// The match the pen went down on, until it lifts.
    held: Option<usize>,
    /// The command a row last ran, kept while the palette closes and opens.
    last: Option<Command>,
}

impl Palette {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The `show-commands` command: opens the palette empty, or closes it.
    pub fn toggle(&mut self) {
        *self = Palette { open: !self.open, last: self.last, ..Palette::default() };
    }

    /// The commands whose title and group hold what is typed in order (a
    /// subsequence, in any case), best first: by [`rank`], then the
    /// [`order`] they list in. Empty, the command last run comes first.
    pub fn matches(&self) -> Vec<Command> {
        let query = self.query.to_lowercase();
        let mut ranked: Vec<(u8, (usize, bool, usize), Command)> = Command::ALL
            .into_iter()
            .filter_map(|(c, _)| Some((rank(c, &query)?, order(c), c)))
            .collect();
        ranked.sort_unstable_by_key(|&(rank, order, _)| (rank, order));
        let mut matches: Vec<Command> = ranked.into_iter().map(|(_, _, c)| c).collect();
        if let Some(last) = self.last.filter(|_| query.is_empty())
            && let Some(at) = matches.iter().position(|&c| c == last)
        {
            matches[..=at].rotate_right(1);
        }
        matches
    }

    /// A key press while open: up and down move the selection (round from
    /// the last to the first), return runs the selected command, escape or
    /// the key `keys` gives `show-commands` closes it, delete takes back a
    /// character and a key that types adds its character. Every key is
    /// used up. Closed, [`Taken::No`].
    pub fn key(&mut self, chord: Chord, keys: &Keymap) -> Taken {
        if !self.open {
            return Taken::No;
        }
        // The rows may move under the pen: a held one no longer runs.
        self.held = None;
        let count = self.matches().len();
        if chord == ESCAPE || keys.command(chord) == Some(Command::ShowCommands) {
            self.toggle();
        } else if chord == RETURN {
            return self.run(self.at);
        } else if chord == UP && count > 0 {
            self.at = (self.at + count - 1) % count;
        } else if chord == DOWN && count > 0 {
            self.at = (self.at + 1) % count;
        } else if chord == DELETE {
            self.query.pop();
            self.at = 0;
        } else if let Some(c) = typed(chord) {
            self.query.push(c);
            self.at = 0;
        }
        Taken::Yes
    }

    /// Pasted `text` while open goes into the search, on one line and
    /// lowercase. False, and nothing changes, while closed.
    pub fn pasted(&mut self, text: &str) -> bool {
        if self.open {
            self.query.extend(paste::one_line(text).chars().flat_map(char::to_lowercase));
            self.at = 0;
            self.held = None;
        }
        self.open
    }

    /// A press at `at` in a window of `size` while open: one on a row holds
    /// it until the pen lifts ([`Palette::lift`]), one outside the panel
    /// closes it; every press is used. Closed, [`Taken::No`].
    pub fn press(&mut self, at: Point, size: Size) -> Taken {
        if !self.open {
            return Taken::No;
        }
        if place(size).panel.contains(at) {
            self.held = self.row_at(at, size);
        } else {
            self.toggle();
        }
        Taken::Yes
    }

    /// The pen lifted at `at`: on the row it went down on, the palette
    /// closes to run that row's command; anywhere else the lift is used up.
    /// Closed, [`Taken::No`].
    pub fn lift(&mut self, at: Point, size: Size) -> Taken {
        if !self.open {
            return Taken::No;
        }
        match self.held.take() {
            Some(held) if self.row_at(at, size) == Some(held) => self.run(held),
            _ => Taken::Yes,
        }
    }

    /// The match whose row is under `at` in a window of `size`, if any.
    fn row_at(&self, at: Point, size: Size) -> Option<usize> {
        let place = place(size);
        let n = (0..place.rows).find(|&n| place.row(n).contains(at))?;
        let shown = self.first(place.rows) + n as usize;
        (shown < self.matches().len()).then_some(shown)
    }

    /// A command chosen from the menu bar while open: the palette closes,
    /// and the command runs, except `show-commands`, which only closed it.
    /// It is not the palette's run, so it is not listed first next time.
    /// Closed, [`Taken::No`].
    pub fn chosen(&mut self, command: Command) -> Taken {
        if !self.open {
            return Taken::No;
        }
        self.toggle();
        if command == Command::ShowCommands { Taken::Yes } else { Taken::Run(command) }
    }

    /// Closes to run the `n`th match; with none there, stays open.
    fn run(&mut self, n: usize) -> Taken {
        match self.matches().get(n) {
            Some(&command) => {
                self.toggle();
                if command != Command::Quit {
                    self.last = Some(command);
                }
                Taken::Run(command)
            }
            None => Taken::Yes,
        }
    }

    /// The first match shown when `rows` fit: the selection stays in view.
    fn first(&self, rows: u32) -> usize {
        (self.at + 1).saturating_sub(rows as usize)
    }

    /// The open palette in a window of `size`, in `theme`'s colours, with
    /// the keys of `keys`; none when it is closed.
    pub fn shown(&self, keys: &Keymap, theme: &Theme, size: Size) -> Option<Overlay> {
        self.open.then(|| self.overlay(keys, theme, size))
    }

    /// The panel, the search line over a hairline with how many match, a
    /// row per shown match (the selected one filled) with its title, group
    /// and key chip, and the footer under a hairline.
    fn overlay(&self, keys: &Keymap, theme: &Theme, size: Size) -> Overlay {
        let place = place(size);
        let edge = style::edge(theme);
        let panel = Line::fill(style::panel(theme), place.panel, style::PANEL_RADIUS);
        let mut lines = vec![Line { edge: Some(edge), ..panel }];
        let inside = place.panel.outset(-INSET);
        let search = Rect { y: place.panel.y, height: SEARCH, ..inside };
        lines.push(if self.query.is_empty() {
            Line::text(PLACEHOLDER.into(), theme.muted, search, 16.0, Align::Left)
        } else {
            Line::text(self.query.clone() + "▏", theme.text, search, 16.0, Align::Left)
        });
        let matches = self.matches();
        let count = format!("{} of {}", matches.len(), Command::ALL.len());
        lines.push(Line::text(count, theme.muted, search, 12.0, Align::Right));
        let rule = |y| Line::fill(edge, Rect { y, height: style::HAIRLINE, ..place.panel }, 0.0);
        lines.push(rule(place.panel.y + SEARCH));
        if matches.is_empty() {
            let none = Rect { x: inside.x, width: inside.width, ..place.row(0) };
            lines.push(Line::text(NONE.into(), theme.muted, none, 13.0, Align::Left));
        }
        let first = self.first(place.rows);
        let shown = matches.iter().enumerate().skip(first).take(place.rows as usize);
        for (n, (at, &command)) in shown.enumerate() {
            let row = place.row(u32::try_from(n).unwrap_or(0));
            let selected = at == self.at;
            if selected {
                lines.push(Line::fill(style::selected(theme), row, style::ROW_RADIUS));
            } else if self.held == Some(at) {
                lines.push(Line::fill(style::hover(theme), row, style::ROW_RADIUS));
            }
            lines.extend(self::row(command, keys, theme, row, selected));
        }
        let foot_y = place.panel.y + place.panel.height - FOOT;
        lines.push(rule(foot_y));
        let foot = Rect { y: foot_y, height: FOOT, ..inside };
        lines.push(Line::text(FOOTER.into(), theme.muted, foot, 11.0, Align::Left));
        lines.push(Line::text(PEN.into(), theme.muted, foot, 11.0, Align::Right));
        Overlay { frame: size.rect(), dim: theme.background, opacity: OPACITY, lines }
    }
}

/// A command's row in `frame`, `selected` or not: its title, its group
/// and its key's chip (on the selected row the group a lighter blue and
/// the chip on the background, so both read on the fill).
fn row(command: Command, keys: &Keymap, theme: &Theme, frame: Rect, selected: bool) -> Vec<Line> {
    let inset = Rect { x: frame.x + TEXT_PAD, width: frame.width - 2.0 * TEXT_PAD, ..frame };
    let right = inset.x + inset.width;
    let title = Rect { width: inset.width - CHIP - GROUP - 2.0 * TEXT_PAD, ..inset };
    let group = Rect { x: right - CHIP - TEXT_PAD - GROUP, width: GROUP, ..inset };
    let (label, chip) = if selected {
        (style::selected_label(theme), theme.background)
    } else {
        (theme.muted, style::chip(theme))
    };
    let mut lines = vec![
        Line::text(command.title().into(), theme.text, title, 14.0, Align::Left),
        Line::text(command.group().title().into(), label, group, 12.0, Align::Right),
    ];
    if let Some(glyphs) = keys.chord(command).map(Chord::glyphs) {
        let wide = chip_width(&glyphs).min(CHIP);
        let y = frame.y + (frame.height - CHIP_HIGH) / 2.0;
        let at = Rect { x: right - wide, y, width: wide, height: CHIP_HIGH };
        lines.push(Line::chip(glyphs, theme, chip, at));
    } else {
        let at = Rect { x: right - CHIP, width: CHIP, ..inset };
        lines.push(Line::text(NO_KEY.into(), theme.muted, at, 12.0, Align::Right));
    }
    lines
}

/// The character a key types into the search, if it types one: no command
/// or control held, and not a key macOS gives a private code point (the
/// arrows, the function keys).
fn typed(chord: Chord) -> Option<char> {
    let c = chord.key();
    let private = ('\u{f700}'..='\u{f8ff}').contains(&c);
    (chord.types() && !c.is_control() && !private).then_some(c)
}

/// How well lowercase `query` matches `command`, best first: 0 when the
/// title starts with it, 1 when a word of the title does, 2 when the title
/// holds it in order, 3 when only the title and group do; none when they do
/// not hold it.
fn rank(command: Command, query: &str) -> Option<u8> {
    let title = command.title().to_lowercase();
    let starts = |at: usize| title[at..].starts_with(query);
    let mut words =
        title.char_indices().filter(|&(_, c)| c == ' ').map(|(at, c)| at + c.len_utf8());
    if starts(0) {
        Some(0)
    } else if words.any(starts) {
        Some(1)
    } else if holds(&title, query) {
        Some(2)
    } else {
        holds(&format!("{title} {}", command.group().title().to_lowercase()), query).then_some(3)
    }
}

/// Whether `query` is a subsequence of `hay`.
fn holds(hay: &str, query: &str) -> bool {
    let mut hay = hay.chars();
    query.chars().all(|q| hay.any(|h| h == q))
}

/// Where `command` lists among equals: by its group's place in
/// [`Group::ALL`] (the tools first), Quit after the rest of its group, then
/// by the registry.
fn order(command: Command) -> (usize, bool, usize) {
    let group = Group::ALL.iter().position(|&g| g == command.group()).unwrap_or(Group::ALL.len());
    (group, command == Command::Quit, command as usize)
}

/// Where the palette's parts sit in a window.
struct Place {
    panel: Rect,
    /// How many rows fit.
    rows: u32,
}

/// The panel in a window of `size`: centred across, a fifth of the way
/// down, as many rows as fit up to [`MOST`].
fn place(size: Size) -> Place {
    let width = (size.width - 2.0 * MARGIN).min(WIDTH);
    let room = size.height - 2.0 * (MARGIN + PAD) - SEARCH - FOOT;
    let mut rows = MOST;
    while rows > 1 && f64::from(rows) * ROW > room {
        rows -= 1;
    }
    let height = SEARCH + 2.0 * PAD + f64::from(rows) * ROW + FOOT;
    let y = ((size.height - height) / 5.0).max(MARGIN.min((size.height - height) / 2.0));
    Place { panel: Rect { x: (size.width - width) / 2.0, y, width, height }, rows }
}

impl Place {
    /// The `n`th shown row's frame: under the search, inside the padding.
    fn row(&self, n: u32) -> Rect {
        let inside = self.panel.outset(-PAD);
        let y = self.panel.y + SEARCH + PAD + f64::from(n) * ROW;
        Rect { y, height: ROW, ..inside }
    }
}

#[cfg(test)]
mod tests;
