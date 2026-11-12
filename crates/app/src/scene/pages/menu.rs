//! The page menu: a small panel a tap on a footer's page name opens just
//! above it, so the pen picks a page with no keyboard. One row per page of
//! the pane's canvas (the shown one marked ✓), then "New page", "Rename
//! page…", "Place send button" (a "send to agent" button in the middle of
//! the pane) and "Remove page", which run those commands on the pane. Pure:
//! `main` opens it with the pane's pages, hands in the pen, and draws the
//! [`Overlay`] it gives back (no dim: the window stays as it is).
//!
//! While open it takes every press, as the key overlay does: a row runs
//! when the pen lifts on the row it went down on (the row under the pen
//! lights, more when pressed); a press outside the panel, or escape,
//! closes it. Rows are [`style::TAP`] tall. When the pages do not all fit
//! above the footer, it opens on as many as do, the shown page kept in
//! view, and a scroll or a drag of the pen on the panel (past a small slop,
//! so it taps nothing) turns them a row at a time; the commands stay put.
//! A ▲ at the end of the top page's row, or a ▼ at the bottom one's, says
//! more pages are above or below.
//!
//! The same panel lists an agent's profiles to sling under, over the
//! footer's agent ([`PageMenu::open_profiles`]): one row each, the one its
//! process runs under marked, and no commands; a row slings under it.

use crate::commands::{Chord, Command};
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::scene::overlay::{Align, Line, Overlay};
use crate::scene::palette::Taken;
use crate::scene::style;
use crate::scene::taps::{Tap, Took};

/// The panel's width, the space inside its edge, and its gap above the
/// page name and from the window's edge.
const WIDTH: f64 = 220.0;
const PAD: f64 = 6.0;
const GAP: f64 = 4.0;
const EDGE: f64 = 8.0;
/// A row: tall enough for the pen; the space for the ✓ before a name.
const ROW: f64 = style::TAP;
const MARK: f64 = 22.0;
/// How far the pen moves before a press scrolls instead of tapping.
const SLOP: f64 = 6.0;
/// The space the hairline between the pages and the commands takes.
const RULE: f64 = 9.0;
const ESCAPE: Chord = Chord::new(0, '\u{1b}');
/// The commands under the pages, each as its row says it.
const COMMANDS: [(Command, &str); 4] = [
    (Command::NewPage, "New page"),
    (Command::RenamePage, "Rename page…"),
    (Command::PlaceButton, "Place send button"),
    (Command::RemovePage, "Remove page"),
];

/// What the menu lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// The pane's pages, then the page commands.
    Pages,
    /// The pane's agent's profiles.
    Profiles,
}

impl List {
    /// The commands under the rows.
    fn commands(self) -> &'static [(Command, &'static str)] {
        match self {
            List::Pages => &COMMANDS,
            List::Profiles => &[],
        }
    }

    /// What a tap on the row with index `i` does.
    fn tap(self, i: usize) -> Tap {
        match self {
            List::Pages => Tap::Page(i),
            List::Profiles => Tap::Profile(i),
        }
    }
}

/// The open menu: the pane it is for, what it lists, the name it opened
/// over, the rows' names and which one the pane shows (or runs under);
/// once scrolled, the first row shown, and the part of a row scrolled past
/// it.
#[derive(Debug, Clone, PartialEq)]
struct Open {
    pane: PaneId,
    list: List,
    anchor: Rect,
    names: Vec<String>,
    shown: Option<usize>,
    first: Option<usize>,
    spill: f64,
}

/// A press on the panel, until the pen lifts: the row it went down on,
/// where it went down and last was, and whether it moved far enough to
/// scroll.
#[derive(Debug, Clone, Copy)]
struct Held {
    tap: Option<Tap>,
    from: Point,
    last: Point,
    dragged: bool,
}

/// The page menu: open or not, where the pen is over it, and the press on it.
#[derive(Debug, Default)]
pub struct PageMenu {
    open: Option<Open>,
    pointer: Option<Point>,
    held: Option<Held>,
}

/// Where the panel and its rows are: each row's frame and what it runs;
/// the first page shown and the most it can be.
struct Place {
    panel: Rect,
    rows: Vec<(Rect, Tap)>,
    rule: Option<Rect>,
    first: usize,
    most: usize,
}

impl PageMenu {
    /// Opens the menu for `pane` above `anchor` (its footer's page name),
    /// listing `names` with the one at `shown` marked.
    pub fn open(&mut self, pane: PaneId, anchor: Rect, (names, shown): (Vec<String>, usize)) {
        self.open_list(List::Pages, pane, anchor, (names, Some(shown)));
    }

    /// Opens the menu for `pane` above `anchor` (its footer's agent),
    /// listing the profiles `names` with the one at `shown` (the one the
    /// agent runs under, if it runs) marked.
    pub fn open_profiles(
        &mut self,
        pane: PaneId,
        anchor: Rect,
        rows: (Vec<String>, Option<usize>),
    ) {
        self.open_list(List::Profiles, pane, anchor, rows);
    }

    fn open_list(
        &mut self,
        list: List,
        pane: PaneId,
        anchor: Rect,
        rows: (Vec<String>, Option<usize>),
    ) {
        let (names, shown) = rows;
        let open = Open { pane, list, anchor, names, shown, first: None, spill: 0.0 };
        *self = PageMenu { open: Some(open), ..PageMenu::default() };
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The pane the open menu is for, and what it lists.
    pub fn pane(&self) -> Option<(PaneId, List)> {
        self.open.as_ref().map(|o| (o.pane, o.list))
    }

    /// The rows it lists now, and the marked one (`None`: the pane shows no
    /// page or has no profiles now, and the menu closes).
    pub fn refresh(&mut self, rows: Option<(Vec<String>, Option<usize>)>) {
        match (rows, &mut self.open) {
            (Some((names, shown)), Some(open)) => (open.names, open.shown) = (names, shown),
            _ => *self = PageMenu::default(),
        }
    }

    /// Escape closes it; every other key goes on. False when closed.
    pub fn key(&mut self, chord: Chord) -> bool {
        let closes = self.is_open() && chord == ESCAPE;
        if closes {
            *self = PageMenu::default();
        }
        closes
    }

    /// A command chosen from the menu bar while open: the menu closes, and
    /// the command runs. Closed, [`Took::No`].
    pub fn chosen(&mut self, command: Command) -> Took {
        if !self.is_open() {
            return Took::No;
        }
        *self = PageMenu::default();
        Taken::Run(command).into()
    }

    /// A press at `at` in a window of `size`: outside the panel it closes
    /// the menu, inside it holds the row under it until the lift. Used up
    /// while open; [`Took::No`] when closed.
    pub fn press(&mut self, at: Point, size: Size) -> Took {
        let Some(place) = self.place(size) else { return Took::No };
        if place.panel.contains(at) {
            let held = Held { tap: row_at(&place, at), from: at, last: at, dragged: false };
            (self.held, self.pointer) = (Some(held), Some(at));
        } else {
            *self = PageMenu::default();
        }
        Took::Yes
    }

    /// The pen moved to `at`, down (`drag`) or not: past a small slop, a
    /// press scrolls the pages with it instead of tapping. True when that
    /// changed what lights; a drag is always the menu's while it is open.
    pub fn pointer(&mut self, at: Point, size: Size, drag: bool) -> bool {
        let Some(place) = self.place(size) else { return false };
        let before = self.pointer.and_then(|p| row_at(&place, p));
        self.pointer = Some(at);
        if let Some(mut held) = self.held.filter(|_| drag) {
            held.dragged |= (at.x - held.from.x).hypot(at.y - held.from.y) > SLOP;
            let by = held.last.y - at.y;
            held.last = at;
            self.held = Some(held);
            if held.dragged {
                self.turn(by, size);
            }
        }
        drag || before != row_at(&place, at)
    }

    /// A scroll by `dy` points (down the page is negative) while open: the
    /// pages follow it. True while open: the menu takes it.
    pub fn scroll(&mut self, dy: f64, size: Size) -> bool {
        self.turn(-dy, size);
        self.is_open()
    }

    /// Turns the pages `by` points (positive shows later pages), a row at a
    /// time; a part of a row waits for the rest. At either end they stay.
    fn turn(&mut self, by: f64, size: Size) {
        let Some(place) = self.place(size) else { return };
        let Some(open) = &mut self.open else { return };
        let (mut first, mut moved) = (place.first, open.spill + by);
        while moved >= ROW && first < place.most {
            (first, moved) = (first + 1, moved - ROW);
        }
        while moved <= -ROW && first > 0 {
            (first, moved) = (first - 1, moved + ROW);
        }
        let end = (first == place.most && moved > 0.0) || (first == 0 && moved < 0.0);
        (open.first, open.spill) = (Some(first), if end { 0.0 } else { moved });
    }

    /// The pen lifted at `at`: on the row it went down on, the menu closes
    /// to run it ([`Took::Tap`]); anywhere else it is used up. Closed,
    /// [`Took::No`].
    pub fn lift(&mut self, at: Point, size: Size) -> Took {
        let Some(place) = self.place(size) else { return Took::No };
        match self.held.take() {
            Some(Held { tap: Some(tap), dragged: false, .. })
                if row_at(&place, at) == Some(tap) =>
            {
                let frame = place.rows.iter().find(|(_, t)| *t == tap).map_or(place.panel, |r| r.0);
                *self = PageMenu::default();
                Took::Tap((frame, tap))
            }
            _ => Took::Yes,
        }
    }

    /// The open menu in a window of `size`, in `theme`'s colours: the panel,
    /// the row under the pen lit (more while pressed), each page's row with
    /// the shown one marked and bold (and a sign where more pages are
    /// above or below), a hairline, and the commands' rows.
    pub fn shown(&self, theme: &Theme, size: Size) -> Option<Overlay> {
        let (open, place) = (self.open.as_ref()?, self.place(size)?);
        let panel = Line {
            edge: Some(style::edge(theme)),
            ..Line::fill(style::panel(theme), place.panel, style::PANEL_RADIUS)
        };
        let mut lines = vec![panel];
        let under = self.pointer.and_then(|p| row_at(&place, p));
        let lit = match self.held {
            Some(Held { tap: Some(held), dragged: false, .. }) => {
                (under == Some(held)).then(|| (held, style::selected(theme)))
            }
            Some(_) => None,
            None => under.map(|tap| (tap, style::hover(theme))),
        };
        for &(frame, tap) in &place.rows {
            if let Some((_, fill)) = lit.filter(|(t, _)| *t == tap) {
                lines.push(Line::fill(fill, frame, style::ROW_RADIUS));
            }
            let text = Rect { x: frame.x + MARK, width: frame.width - MARK, ..frame };
            match tap {
                Tap::Page(i) | Tap::Profile(i) => {
                    let (name, here) = (open.names[i].clone(), Some(i) == open.shown);
                    if here {
                        let mark = Rect { width: MARK, ..frame };
                        lines.push(Line::text("✓".into(), theme.text, mark, 13.0, Align::Centre));
                    }
                    let more = more(&place, i);
                    let text = if more.is_empty() {
                        text
                    } else {
                        let arrow = Rect { x: frame.x + frame.width - MARK, width: MARK, ..frame };
                        lines.push(Line::text(more.into(), theme.muted, arrow, 9.0, Align::Centre));
                        Rect { width: text.width - MARK, ..text }
                    };
                    let line = Line::text(name, theme.text, text, 13.0, Align::Left);
                    lines.push(Line { bold: here, ..line });
                }
                Tap::Run(command) => {
                    let word = COMMANDS.iter().find(|(c, _)| *c == command).map_or("", |c| c.1);
                    lines.push(Line::text(word.into(), theme.text, text, 13.0, Align::Left));
                }
                Tap::Pages
                | Tap::Profiles
                | Tap::Workspace(_)
                | Tap::Slung
                | Tap::Send
                | Tap::Cancel => {}
            }
        }
        lines.extend(place.rule.map(|rule| Line::fill(style::edge(theme), rule, 0.0)));
        Some(Overlay { frame: size.rect(), dim: theme.background, opacity: 0.0, lines })
    }

    /// Where the open menu's panel and rows are in a window of `size`:
    /// above its page name, centred on it and kept inside the window; as
    /// many pages as fit, from where it was scrolled to, else from one that
    /// keeps the shown page in view.
    fn place(&self, size: Size) -> Option<Place> {
        let open = self.open.as_ref()?;
        let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
        let commands = open.list.commands();
        let ruled = if commands.is_empty() { 0.0 } else { RULE };
        let fixed = 2.0 * PAD + ruled + ROW * count(commands.len());
        let room = open.anchor.y - GAP - EDGE - fixed;
        let fit = (0..open.names.len()).take_while(|&n| ROW * count(n + 1) <= room).count();
        let most = open.names.len() - fit;
        let centred = open.shown.unwrap_or(0).saturating_sub(fit.saturating_sub(1) / 2);
        let first = open.first.unwrap_or(centred).min(most);
        let height = fixed + ROW * count(fit);
        let centre = open.anchor.x + open.anchor.width / 2.0;
        let x = (centre - WIDTH / 2.0).min(size.width - EDGE - WIDTH).max(EDGE);
        let y = (open.anchor.y - GAP - height).max(EDGE);
        let panel = Rect { x, y, width: WIDTH, height };
        let row = |n: usize, y: f64| Rect {
            x: x + PAD,
            y: y + ROW * count(n),
            width: WIDTH - 2.0 * PAD,
            height: ROW,
        };
        let mut rows: Vec<(Rect, Tap)> =
            (0..fit).map(|n| (row(n, y + PAD), open.list.tap(first + n))).collect();
        let below = y + PAD + ROW * count(fit);
        let rule = (fit > 0 && !commands.is_empty()).then_some(Rect {
            x: x + PAD,
            y: below + RULE / 2.0,
            width: WIDTH - 2.0 * PAD,
            height: style::HAIRLINE,
        });
        let after = below + ruled;
        rows.extend(commands.iter().enumerate().map(|(n, &(c, _))| (row(n, after), Tap::Run(c))));
        Some(Place { panel, rows, rule, first, most })
    }
}

/// The sign at the right end of page `i`'s row, when the pages scroll: ▲
/// on the first shown when there are pages above it, ▼ on the last shown
/// when there are pages below it; else nothing.
fn more(place: &Place, i: usize) -> &'static str {
    let listed = |tap: &Tap| matches!(tap, Tap::Page(_) | Tap::Profile(_));
    let shown = place.rows.iter().filter(|(_, tap)| listed(tap)).count();
    let up = i == place.first && place.first > 0;
    let down = i + 1 == place.first + shown && place.first < place.most;
    match (up, down) {
        (true, true) => "▲▼",
        (true, false) => "▲",
        (false, true) => "▼",
        (false, false) => "",
    }
}

/// What the row under `at` runs, if any.
fn row_at(place: &Place, at: Point) -> Option<Tap> {
    place.rows.iter().find(|(frame, _)| frame.contains(at)).map(|&(_, tap)| tap)
}

#[cfg(test)]
mod tests;
