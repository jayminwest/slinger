//! Where the key overlay's parts sit: how many columns fit, which sections
//! each holds and how wide each is. A column is never narrower than its
//! widest row as the system font sets it ([`widths`]), so no title and no
//! chip is cut short; a window too narrow for that many columns gets fewer,
//! whatever the count of commands or the keys bound to them.

use super::{
    BAR, COLUMNS, GAP, GROUP_GAP, HEADER, HEADING, MARGIN, MIN_COLUMN, NARROW_GAP, NARROW_PAD,
    NO_KEY, PAD, PAD_Y, REACH, ROW, UNDER_HEADER, WIDTH, chip_width, fixed,
};
use crate::commands::{Chord, Command, Group, Keymap};
use crate::layout::{Point, Rect, Size};
use crate::scene::glyphs;

/// A row title's font size, a key's ("no key") and the space between them.
pub(super) const TITLE: f64 = 13.0;
pub(super) const NOTE: f64 = 12.0;
pub(super) const SPACE: f64 = 8.0;
/// What a label takes past its glyphs: the text field's own inset.
const INSET: f64 = 6.0;

/// How wide a label of `text` in the system font at `size` points must be
/// to show it whole.
pub(super) fn wide(text: &str, size: f64) -> f64 {
    glyphs::frame(Point { x: 0.0, y: 0.0 }, text, size).width + INSET
}

/// How wide a row's key is: its chip, or the "no key" note.
pub(super) fn key_wide(key: Option<&str>) -> f64 {
    key.map_or_else(|| wide(NO_KEY, NOTE), chip_width)
}

/// For each section of [`Section::all`], in order, the narrowest column
/// that shows its every row whole with `keys`: the title, the space and
/// the key; never under [`MIN_COLUMN`].
pub(super) fn widths(keys: &Keymap) -> Vec<f64> {
    let row =
        |title: &str, key: Option<String>| wide(title, TITLE) + SPACE + key_wide(key.as_deref());
    let section = |section: Section| match section {
        Section::Commands(group) => {
            let keyed = |c: Command| row(c.title(), keys.chord(c).map(Chord::glyphs));
            commands(group).map(keyed).fold(MIN_COLUMN, f64::max)
        }
        Section::Fixed(fixed) => {
            let rows = fixed.rows.iter().map(|r| row(r.title, Some(r.keys.shown())));
            rows.fold(MIN_COLUMN, f64::max)
        }
    };
    Section::all().into_iter().map(section).collect()
}

/// The commands of `group`, in the registry's order.
pub(super) fn commands(group: Group) -> impl Iterator<Item = Command> {
    Command::ALL.into_iter().map(|(c, _)| c).filter(move |c| c.group() == group)
}

/// A heading and its rows: a group's commands, or fixed keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Commands(Group),
    Fixed(&'static fixed::Fixed),
}

impl Section {
    /// Every section, in order: the groups, then the fixed keys.
    pub(super) fn all() -> Vec<Section> {
        let groups = Group::ALL.into_iter().map(Section::Commands);
        groups.chain(fixed::ALL.into_iter().map(Section::Fixed)).collect()
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Section::Commands(group) => group.title(),
            Section::Fixed(fixed) => fixed.title,
        }
    }

    fn rows(self) -> usize {
        match self {
            Section::Commands(group) => commands(group).count(),
            Section::Fixed(fixed) => fixed.rows.len(),
        }
    }
}

/// How tall a section is: its heading and its rows.
fn tall(section: Section) -> f64 {
    HEADING + ROW * f64::from(u32::try_from(section.rows()).unwrap_or(u32::MAX))
}

/// The columns' sections for the most columns, up to [`COLUMNS`], that
/// fit across `width` with `gap` between two, each as wide as its widest
/// section in `widths` (one per section of [`Section::all`]; a section
/// with none is [`MIN_COLUMN`] wide); never fewer than one. The tallest
/// column's height, the columns and their widths.
pub(super) fn columns(width: f64, gap: f64, widths: &[f64]) -> (f64, Vec<Vec<Section>>, Vec<f64>) {
    let all = Section::all();
    let need = |i: usize| widths.get(i).copied().unwrap_or(MIN_COLUMN).max(MIN_COLUMN);
    let laid = |n: u32| {
        let (tallest, runs) = split(&all, n);
        let mut first = 0;
        let wide = runs.iter().map(|run| {
            first += run.len();
            (first - run.len()..first).map(need).fold(MIN_COLUMN, f64::max)
        });
        let wide: Vec<f64> = wide.collect();
        (tallest, runs, wide)
    };
    let across =
        |wide: &[f64]| wide.iter().sum::<f64>() + gap * count(wide.len().saturating_sub(1));
    let fits = (2..=COLUMNS).rev().map(laid).find(|(_, _, wide)| across(wide) <= width);
    fits.unwrap_or_else(|| laid(1))
}

/// `n` as a number of points' worth.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// `groups`, in order, cut into at most `n` runs, a column each, so the
/// tallest column is as short as it can be (the earliest cut on a tie):
/// the tallest's height, and the columns.
pub(super) fn split(groups: &[Section], n: u32) -> (f64, Vec<Vec<Section>>) {
    let high = |run: &[Section]| run.iter().map(|&g| tall(g) + GROUP_GAP).sum::<f64>() - GROUP_GAP;
    let whole = (high(groups), vec![groups.to_vec()]);
    if n <= 1 {
        return whole;
    }
    (1..groups.len()).fold(whole, |best, cut| {
        let (first, rest) = groups.split_at(cut);
        let (tallest, mut runs) = split(rest, n - 1);
        runs.insert(0, first.to_vec());
        let next = (high(first).max(tallest), runs);
        if next.0 < best.0 { next } else { best }
    })
}

/// Where the overlay's parts sit in a window, scrolled.
pub(super) struct Place {
    pub(super) panel: Rect,
    /// The ✕ that closes it, at the header's right.
    pub(super) close: Rect,
    /// Where the rows show; a row not wholly in it is not shown.
    pub(super) body: Rect,
    /// How far the rows are scrolled, and the most they can be.
    pub(super) scroll: f64,
    pub(super) most: f64,
    /// Each section's heading, each command's row and each fixed row,
    /// where they are now.
    pub(super) headings: Vec<(Section, Rect)>,
    pub(super) rows: Vec<(Command, Rect)>,
    pub(super) fixed: Vec<(&'static fixed::Row, Rect)>,
}

/// The panel in a window of `size`, centred, as tall as its rows or the
/// window allows, with its rows scrolled by `scroll` (kept in range). Its
/// groups fill as many columns as fit, left to right, each as wide as its
/// widest section in `widths` ([`columns`]); the panel is [`WIDTH`] wide,
/// or wider when its columns need it, and they share what is left over.
pub(super) fn place(size: Size, scroll: f64, widths: &[f64]) -> Place {
    let room = (size.width - 2.0 * MARGIN).max(0.0);
    let (pad, gap) = if room.min(WIDTH) < 600.0 { (NARROW_PAD, NARROW_GAP) } else { (PAD, GAP) };
    let (content, runs, wide) = columns(room - 2.0 * pad, gap, widths);
    let n = count(runs.len());
    let need = wide.iter().sum::<f64>() + gap * (n - 1.0);
    let width = (need + 2.0 * pad).max(WIDTH).min(room);
    let room_y = (size.height - 2.0 * MARGIN).max(0.0);
    let height = (2.0 * PAD_Y + HEADER + UNDER_HEADER + content).min(room_y);
    let (x, y) = ((size.width - width) / 2.0, (size.height - height) / 2.0);
    let panel = Rect { x, y, width, height };
    let close = Rect { x: x + width - pad / 2.0 - ROW, y: y + PAD_Y, width: ROW, height: HEADER };
    let top = y + PAD_Y + HEADER + UNDER_HEADER;
    let body_high = (y + height - PAD_Y - top).max(0.0);
    let body = Rect { x: x + pad, y: top, width: (width - 2.0 * pad).max(0.0), height: body_high };
    let most = (content - body.height).max(0.0);
    let scroll = scroll.clamp(0.0, most);
    let (spare, mut x) = ((body.width - need) / n, body.x);
    let (mut headings, mut rows, mut fixed) = (Vec::new(), Vec::new(), Vec::new());
    for (sections, wide) in runs.into_iter().zip(wide) {
        let column = if n > 1.0 { wide + spare } else { body.width };
        let mut y = body.y - scroll;
        for section in sections {
            headings.push((section, Rect { x, y, width: column, height: HEADING }));
            y += HEADING;
            let mut next = || {
                y += ROW;
                Rect { x, y: y - ROW, width: column, height: ROW }
            };
            match section {
                Section::Commands(group) => rows.extend(commands(group).map(|c| (c, next()))),
                Section::Fixed(keys) => fixed.extend(keys.rows.iter().map(|r| (r, next()))),
            }
            y += GROUP_GAP;
        }
        x += column + gap;
    }
    Place { panel, close, body, scroll, most, headings, rows, fixed }
}

impl Place {
    /// Whether `frame` is wholly in the body: it shows.
    pub(super) fn shows(&self, frame: Rect) -> bool {
        let end = self.body.y + self.body.height + 0.5;
        frame.y >= self.body.y - 0.5 && frame.y + frame.height <= end
    }

    /// The row, shown, whose target holds `at`: the row, reaching past
    /// its text on either side.
    pub(super) fn row_at(&self, at: Point) -> Option<Command> {
        let target =
            |frame: Rect| Rect { x: frame.x - REACH, width: frame.width + 2.0 * REACH, ..frame };
        let mut shown = self.rows.iter().filter(|(_, frame)| self.shows(*frame));
        shown.find(|(_, frame)| target(*frame).contains(at)).map(|&(command, _)| command)
    }

    /// The scroll bar's frame, when the rows do not all fit.
    pub(super) fn bar(&self) -> Option<Rect> {
        (self.most > 0.0).then(|| {
            let body = self.body;
            let high = body.height * body.height / (body.height + self.most);
            let y = body.y + (body.height - high) * self.scroll / self.most;
            let x = self.panel.x + self.panel.width - (body.x - self.panel.x) / 2.0 - BAR / 2.0;
            Rect { x, y, width: BAR, height: high }
        })
    }
}
