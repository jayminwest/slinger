//! The page menu: a small panel a tap on a footer's page name opens just
//! above it, so the pen picks a page with no keyboard. One row per page of
//! the pane's canvas (the shown one marked ✓), then "New page", "Rename
//! page…" and "Remove page", which run those commands on the pane. Pure:
//! `main` opens it with the pane's pages, hands in the pen, and draws the
//! [`Overlay`] it gives back (no dim: the window stays as it is).
//!
//! While open it takes every press, as the key overlay does: a row runs
//! when the pen lifts on the row it went down on (the row under the pen
//! lights, more when pressed); a press outside the panel, or escape,
//! closes it. Rows are [`style::TAP`] tall. Pages that do not fit above the
//! footer are left out, the shown page kept in view.

use crate::commands::{Chord, Command};
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::scene::overlay::{Align, Line, Overlay};
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
/// The space the hairline between the pages and the commands takes.
const RULE: f64 = 9.0;
const ESCAPE: Chord = Chord::new(0, '\u{1b}');
/// The commands under the pages, each as its row says it.
const COMMANDS: [(Command, &str); 3] = [
    (Command::NewPage, "New page"),
    (Command::RenamePage, "Rename page…"),
    (Command::RemovePage, "Remove page"),
];

/// The open menu: the pane it is for, the page name it opened over, the
/// pages' names and which one the pane shows.
#[derive(Debug, Clone, PartialEq)]
struct Open {
    pane: PaneId,
    anchor: Rect,
    names: Vec<String>,
    shown: usize,
}

/// The page menu: open or not, where the pen is over it, and the row it
/// went down on.
#[derive(Debug, Default)]
pub struct PageMenu {
    open: Option<Open>,
    pointer: Option<Point>,
    held: Option<Tap>,
}

/// Where the panel and its rows are: each row's frame and what it runs.
struct Place {
    panel: Rect,
    rows: Vec<(Rect, Tap)>,
    rule: Option<Rect>,
}

impl PageMenu {
    /// Opens the menu for `pane` above `anchor` (its footer's page name),
    /// listing `names` with the one at `shown` marked.
    pub fn open(&mut self, pane: PaneId, anchor: Rect, (names, shown): (Vec<String>, usize)) {
        *self = PageMenu { open: Some(Open { pane, anchor, names, shown }), ..PageMenu::default() };
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The pane the open menu is for.
    pub fn pane(&self) -> Option<PaneId> {
        self.open.as_ref().map(|o| o.pane)
    }

    /// The pane's pages now (`None`: it shows none, and the menu closes).
    pub fn refresh(&mut self, pages: Option<(Vec<String>, usize)>) {
        match (pages, &mut self.open) {
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

    /// A press at `at` in a window of `size`: outside the panel it closes
    /// the menu, inside it holds the row under it until the lift. Used up
    /// while open; [`Took::No`] when closed.
    pub fn press(&mut self, at: Point, size: Size) -> Took {
        let Some(place) = self.place(size) else { return Took::No };
        if place.panel.contains(at) {
            (self.held, self.pointer) = (row_at(&place, at), Some(at));
        } else {
            *self = PageMenu::default();
        }
        Took::Yes
    }

    /// The pen moved to `at`, down (`drag`) or not. True when that changed
    /// what lights; a drag is always the menu's while it is open.
    pub fn pointer(&mut self, at: Point, size: Size, drag: bool) -> bool {
        let Some(place) = self.place(size) else { return false };
        let before = self.pointer.and_then(|p| row_at(&place, p));
        self.pointer = Some(at);
        drag || before != row_at(&place, at)
    }

    /// The pen lifted at `at`: on the row it went down on, the menu closes
    /// to run it ([`Took::Tap`]); anywhere else it is used up. Closed,
    /// [`Took::No`].
    pub fn lift(&mut self, at: Point, size: Size) -> Took {
        let Some(place) = self.place(size) else { return Took::No };
        match self.held.take() {
            Some(tap) if row_at(&place, at) == Some(tap) => {
                let frame = place.rows.iter().find(|(_, t)| *t == tap).map_or(place.panel, |r| r.0);
                *self = PageMenu::default();
                Took::Tap((frame, tap))
            }
            _ => Took::Yes,
        }
    }

    /// The open menu in a window of `size`, in `theme`'s colours: the panel,
    /// the row under the pen lit (more while pressed), each page's row with
    /// the shown one marked and bold, a hairline, and the commands' rows.
    pub fn shown(&self, theme: &Theme, size: Size) -> Option<Overlay> {
        let (open, place) = (self.open.as_ref()?, self.place(size)?);
        let panel = Line {
            edge: Some(style::edge(theme)),
            ..Line::fill(style::panel(theme), place.panel, style::PANEL_RADIUS)
        };
        let mut lines = vec![panel];
        let under = self.pointer.and_then(|p| row_at(&place, p));
        let lit = match self.held {
            Some(held) => (under == Some(held)).then(|| (held, style::selected(theme))),
            None => under.map(|tap| (tap, style::hover(theme))),
        };
        for &(frame, tap) in &place.rows {
            if let Some((_, fill)) = lit.filter(|(t, _)| *t == tap) {
                lines.push(Line::fill(fill, frame, style::ROW_RADIUS));
            }
            let text = Rect { x: frame.x + MARK, width: frame.width - MARK, ..frame };
            match tap {
                Tap::Page(i) => {
                    let (name, here) = (open.names[i].clone(), i == open.shown);
                    if here {
                        let mark = Rect { width: MARK, ..frame };
                        lines.push(Line::text("✓".into(), theme.text, mark, 13.0, Align::Centre));
                    }
                    let line = Line::text(name, theme.text, text, 13.0, Align::Left);
                    lines.push(Line { bold: here, ..line });
                }
                Tap::Run(command) => {
                    let word = COMMANDS.iter().find(|(c, _)| *c == command).map_or("", |c| c.1);
                    lines.push(Line::text(word.into(), theme.text, text, 13.0, Align::Left));
                }
                Tap::Pages | Tap::Workspace(_) | Tap::Slung => {}
            }
        }
        lines.extend(place.rule.map(|rule| Line::fill(style::edge(theme), rule, 0.0)));
        Some(Overlay { frame: size.rect(), dim: theme.background, opacity: 0.0, lines })
    }

    /// Where the open menu's panel and rows are in a window of `size`:
    /// above its page name, centred on it and kept inside the window; as
    /// many pages as fit, from one that keeps the shown page in view.
    fn place(&self, size: Size) -> Option<Place> {
        let open = self.open.as_ref()?;
        let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
        let fixed = 2.0 * PAD + RULE + ROW * count(COMMANDS.len());
        let room = open.anchor.y - GAP - EDGE - fixed;
        let fit = (0..open.names.len()).take_while(|&n| ROW * count(n + 1) <= room).count();
        let first =
            open.shown.saturating_sub(fit.saturating_sub(1) / 2).min(open.names.len() - fit);
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
            (0..fit).map(|n| (row(n, y + PAD), Tap::Page(first + n))).collect();
        let below = y + PAD + ROW * count(fit);
        let rule = (fit > 0).then_some(Rect {
            x: x + PAD,
            y: below + RULE / 2.0,
            width: WIDTH - 2.0 * PAD,
            height: style::HAIRLINE,
        });
        let after = below + RULE;
        rows.extend(COMMANDS.iter().enumerate().map(|(n, &(c, _))| (row(n, after), Tap::Run(c))));
        Some(Place { panel, rows, rule })
    }
}

/// What the row under `at` runs, if any.
fn row_at(place: &Place, at: Point) -> Option<Tap> {
    place.rows.iter().find(|(frame, _)| frame.contains(at)).map(|&(_, tap)| tap)
}

#[cfg(test)]
mod tests;
