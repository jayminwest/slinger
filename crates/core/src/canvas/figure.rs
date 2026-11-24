//! Figures and parts (docs/design/figures.md, "Figure and part"): items
//! that share a figure name, some of them a part name, and whether the
//! user's hands pass over them (`locked`). Membership is three fields on
//! each item ([`Member`]), never a list kept elsewhere, so it cannot go
//! stale and a reader from before figures keeps every item.
//!
//! The core never reads `locked`: the author rule is unchanged, an agent
//! edits its own locked items, and the user's verbs act on them as on any.
//! It is the app's to skip. A figure exists while an item has its name.

pub(super) mod matching;

use std::collections::BTreeMap;
use std::fmt;

use super::find::by_place;
use super::item::invalid;
use super::{Author, Canvas, Item, ItemId, PageId, Rect};
use crate::Result;

/// The longest figure or part name, in characters.
pub const NAME_MAX: usize = 40;

/// Which figure an item is in, which part of it, and whether it is locked.
/// The default is a free item in no figure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Member {
    /// The user's hands pass over it in the app (a pen draws over it as on
    /// empty paper). Agents and the user's verbs are not stopped by it.
    pub locked: bool,
    /// The figure's name ([`figure_name`]).
    pub fig: Option<String>,
    /// The part's name ([`part_name`]); only with `fig`.
    pub part: Option<String>,
}

impl Member {
    /// Rejects a name that breaks the rules, or a part with no figure.
    pub(super) fn check(&self) -> Result<()> {
        if let Some(fig) = &self.fig
            && !figure_name(fig)
        {
            return Err(invalid(format!("`fig` {fig:?} is not a figure name"), FIGURE_RULE));
        }
        match &self.part {
            Some(part) if self.fig.is_none() => Err(invalid(
                format!("`part` {part:?} has no `fig`"),
                "a part is a part of a figure: give `fig` too, or leave `part` out",
            )),
            Some(part) if !part_name(part) => {
                Err(invalid(format!("`part` {part:?} is not a part name"), PART_RULE))
            }
            _ => Ok(()),
        }
    }
}

/// The rule for a figure name, as a hint.
pub(crate) const FIGURE_RULE: &str = "a figure name is 1 to 40 of a-z, 0-9 and -, starts with \
    a letter, and is not shaped like an item id (a letter then only digits, as b4)";

/// The rule for a part name, as a hint.
pub(crate) const PART_RULE: &str = "a part name is 1 to 40 of a-z, 0-9 and -";

/// Whether `s` is a figure name: 1 to [`NAME_MAX`] of `a-z`, `0-9` and
/// `-`, a letter first, and not a letter then only digits (`b4`), so a
/// name and an item id never read alike.
#[must_use]
pub fn figure_name(s: &str) -> bool {
    let id_shaped = || s.len() > 1 && s[1..].bytes().all(|b| b.is_ascii_digit());
    part_name(s) && s.starts_with(|c: char| c.is_ascii_lowercase()) && !id_shaped()
}

/// Whether `s` is a part name: 1 to [`NAME_MAX`] of `a-z`, `0-9` and `-`.
#[must_use]
pub fn part_name(s: &str) -> bool {
    (1..=NAME_MAX).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// What a figure address names: `figure`, or `figure/part`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    pub figure: String,
    pub part: Option<String>,
}

impl Address {
    /// Reads `figure` or `figure/part`.
    ///
    /// # Errors
    ///
    /// `invalid`, with the name rules, for anything else.
    pub fn parse(s: &str) -> Result<Self> {
        let (figure, part) = s.split_once('/').map_or((s, None), |(f, p)| (f, Some(p)));
        if !figure_name(figure) {
            return Err(invalid(format!("{s:?} names no figure"), FIGURE_RULE));
        }
        if part.is_some_and(|p| !part_name(p)) {
            return Err(invalid(format!("{s:?} names no part"), PART_RULE));
        }
        Ok(Self { figure: figure.to_owned(), part: part.map(str::to_owned) })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.part {
            Some(part) => write!(f, "{}/{part}", self.figure),
            None => f.write_str(&self.figure),
        }
    }
}

/// A figure on a canvas, as [`Canvas::figures`] lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    pub name: String,
    /// The maker of its first item.
    pub maker: Author,
    /// The page of its first item: a figure is drawn on one page.
    pub page: PageId,
    /// The joint frame of its items the page shows (all of them, when a
    /// folded box hides every one).
    pub frame: Rect,
    /// Its parts, in reading order of their frames.
    pub parts: Vec<Part>,
    /// Whether any of its items is locked.
    pub locked: bool,
    /// Its items, in id order.
    pub items: Vec<ItemId>,
}

/// A named part of a figure.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    /// The joint frame of its items, as a figure's.
    pub frame: Rect,
    /// Its items, in id order.
    pub items: Vec<ItemId>,
}

impl Canvas {
    /// The figures, page by page in page order, on a page in reading order
    /// of their frames (top to bottom, then left to right, then by name).
    #[must_use]
    pub fn figures(&self) -> Vec<Figure> {
        let mut by_name: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
        for item in self.items.values() {
            if let Some(fig) = &item.member.fig {
                by_name.entry(fig).or_default().push(item);
            }
        }
        let mut out: Vec<Figure> =
            by_name.into_iter().map(|(n, items)| self.built(n, &items)).collect();
        let order: Vec<PageId> = self.pages().map(super::Page::id).collect();
        let at = |p: PageId| order.iter().position(|o| *o == p);
        out.sort_by(|a, b| {
            at(a.page).cmp(&at(b.page)).then(by_place(a.frame, b.frame)).then(a.name.cmp(&b.name))
        });
        out
    }

    /// The figure named `name`, if an item has that name.
    #[must_use]
    pub fn figure(&self, name: &str) -> Option<Figure> {
        let items: Vec<&Item> =
            self.items.values().filter(|i| i.member.fig.as_deref() == Some(name)).collect();
        (!items.is_empty()).then(|| self.built(name, &items))
    }

    /// The items `address` names, in id order: the whole figure, or the
    /// items of one part. None when it names nothing.
    #[must_use]
    pub fn part(&self, address: &Address) -> Vec<ItemId> {
        let fig = Some(address.figure.as_str());
        let named = |i: &&Item| {
            i.member.fig.as_deref() == fig
                && (address.part.is_none() || i.member.part == address.part)
        };
        self.items.values().filter(named).map(Item::id).collect()
    }

    /// The figure `name` of `items`, which are in id order and not empty.
    fn built(&self, name: &str, items: &[&Item]) -> Figure {
        let mut parts: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
        for item in items {
            if let Some(part) = &item.member.part {
                parts.entry(part).or_default().push(item);
            }
        }
        let mut parts: Vec<Part> = parts
            .into_iter()
            .map(|(name, items)| Part {
                name: name.to_owned(),
                frame: self.shown_frame(&items),
                items: items.iter().map(|i| i.id).collect(),
            })
            .collect();
        parts.sort_by(|a, b| by_place(a.frame, b.frame).then(a.name.cmp(&b.name)));
        Figure {
            name: name.to_owned(),
            maker: items[0].author.clone(),
            page: items[0].page,
            frame: self.shown_frame(items),
            parts,
            locked: items.iter().any(|i| i.member.locked),
            items: items.iter().map(|i| i.id).collect(),
        }
    }

    /// The joint frame of the `items` the page shows, or of all of them
    /// when it shows none.
    fn shown_frame(&self, items: &[&Item]) -> Rect {
        let shown: Vec<ItemId> = items.iter().map(|i| i.id).filter(|i| !self.hidden(*i)).collect();
        let ids = if shown.is_empty() { items.iter().map(|i| i.id).collect() } else { shown };
        self.bounds_of(ids).unwrap_or_default()
    }
}

/// The part names of `items`, in reading order of each part's joint frame
/// (then by name), as [`Figure::parts`] lists a figure's on the canvas.
pub(crate) fn parts_in_order(items: &[Item]) -> Vec<String> {
    let mut frames: BTreeMap<&str, Rect> = BTreeMap::new();
    for item in items {
        if let Some(part) = &item.member.part {
            let bounds = item.bounds();
            frames
                .entry(part)
                .and_modify(|f| *f = crate::render::union(*f, bounds))
                .or_insert(bounds);
        }
    }
    let mut parts: Vec<(&str, Rect)> = frames.into_iter().collect();
    parts.sort_by(|a, b| by_place(a.1, b.1).then(a.0.cmp(b.0)));
    parts.into_iter().map(|(name, _)| name.to_owned()).collect()
}

impl Item {
    /// Which figure and part the item is in, and whether it is locked.
    #[must_use]
    pub fn member(&self) -> &Member {
        &self.member
    }
}
