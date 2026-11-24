//! Figures in the packet (docs/design/figures.md, "In the packet"). A
//! figure is one `figure` line, its id the figure's name: its items make no
//! lines of their own, and an arrow end bound to one names its address. The
//! line goes where the figure's lowest item would: a figure whose lowest
//! item a folded box hides is one of the lines that box's `folded=` counts.
//! Ink over a part says so in `over`. A packet read `--in` a figure or a
//! part opens that figure: its items there make a line each, with `part=`.

use std::collections::{BTreeSet, HashSet};

use super::fold::first_line;
use super::line::{Line, reading_order};
use super::{Page, area};
use crate::canvas::{Address, Author, Canvas, Figure, Item, ItemId, PageId, Rect, Shape};
use crate::{Error, Result};

/// The part every figure may name for its title: its text quotes the line.
const TITLE: &str = "title";

/// Splits `items`, what a packet reads, into the items that make lines of
/// their own and the figures that make one line each: those whose lowest
/// item is among `items`. The items of `open` make lines of their own.
pub(super) fn split<'a>(
    canvas: &Canvas,
    items: Vec<&'a Item>,
    open: Option<&str>,
) -> (Vec<&'a Item>, Vec<Figure>) {
    let here: HashSet<ItemId> = items.iter().map(|i| i.id()).collect();
    let (shut, own): (Vec<&Item>, Vec<&Item>) = items.into_iter().partition(|i| shut(i, open));
    let names: BTreeSet<&str> = shut.iter().filter_map(|i| i.member().fig.as_deref()).collect();
    let figures = names.into_iter().filter_map(|n| canvas.figure(n));
    (own, figures.filter(|f| f.items.first().is_some_and(|i| here.contains(i))).collect())
}

/// Whether `item` is in a figure that makes one line: any but `open`.
pub(super) fn shut(item: &Item, open: Option<&str>) -> bool {
    item.member().fig.as_deref().is_some_and(|f| Some(f) != open)
}

/// The address `item` goes by in a packet: `figure/part`, or `figure`
/// for an item of no part. `None` for an item in no figure.
pub(super) fn address(item: &Item) -> Option<String> {
    let member = item.member();
    let fig = member.fig.clone()?;
    Some(member.part.as_ref().map_or_else(|| fig.clone(), |p| format!("{fig}/{p}")))
}

/// The parts of the figures on `page` that the page shows an item of, as
/// `(figure/part, frame)`, the smallest first.
pub(super) fn parts(canvas: &Canvas, page: PageId) -> Vec<(String, Rect)> {
    let mut out = Vec::new();
    for figure in canvas.figures().into_iter().filter(|f| f.page == page) {
        for part in figure.parts {
            if part.items.iter().any(|i| !canvas.hidden(*i)) {
                out.push((format!("{}/{}", figure.name, part.name), part.frame));
            }
        }
    }
    out.sort_by(|a, b| area(a.1).total_cmp(&area(b.1)).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Whether the centre of `inner` lies in `outer`, edges included.
pub(super) fn centred(outer: Rect, inner: Rect) -> bool {
    let (x, y) = (inner.x + inner.w / 2.0, inner.y + inner.h / 2.0);
    outer.x <= x && x <= outer.x + outer.w && outer.y <= y && y <= outer.y + outer.h
}

/// What a packet read `--in` `address` holds: the items it names, and
/// every other item the page shows, not of that figure, whose box's centre
/// lies in their frame.
///
/// # Errors
///
/// `not_found` when `address` names nothing on `page`.
pub(super) fn scope<'c>(
    canvas: &'c Canvas,
    page: PageId,
    address: &Address,
) -> Result<Vec<&'c Item>> {
    let named: Vec<&Item> =
        canvas.part(address).into_iter().filter_map(|i| canvas.item(i)).collect();
    if named.first().is_none_or(|i| i.page() != page) {
        let what = if address.part.is_some() { "part" } else { "figure" };
        return Err(Error::NotFound {
            what: format!("{what} {address} on page p{}", page.get()),
            hint: Some("list the figures and their parts with 'slinger canvas show'".into()),
        });
    }
    let shown = named.iter().filter(|i| !canvas.hidden(i.id())).map(|i| i.id());
    let frame = canvas
        .bounds_of(shown)
        .or_else(|| canvas.bounds_of(named.iter().map(|i| i.id())))
        .unwrap_or_default();
    let fig = Some(address.figure.as_str());
    let over = canvas
        .visible(page)
        .filter(|i| i.member().fig.as_deref() != fig && centred(frame, i.bounds()));
    let mut items: Vec<&Item> = named.into_iter().chain(over).collect();
    items.sort_by_key(|i| i.id());
    Ok(items)
}

impl Page<'_> {
    /// The line of `figure`: its frame, its maker when an agent, its parts
    /// in reading order, `locked=yes` when any item is, the box it lies in,
    /// and the first line of its title (part `title`) or of its first text.
    pub(super) fn figure_line(&self, figure: &Figure) -> Line {
        let n = figure.items.first().map_or(0, |i| i.get());
        let mut fields = Vec::new();
        if let Author::Agent(name) = &figure.maker {
            fields.push(("by", name.clone()));
        }
        let parts: Vec<&str> = figure.parts.iter().map(|p| p.name.as_str()).collect();
        if !parts.is_empty() {
            fields.push(("parts", parts.join(",")));
        }
        if figure.locked {
            fields.push(("locked", "yes".to_owned()));
        }
        fields.extend(self.container(n, figure.frame).map(|b| ("in", b)));
        let text = quote(self.canvas, figure);
        Line { n, id: figure.name.clone(), kind: "figure", frame: figure.frame, fields, text }
    }
}

/// The first line of the text of part `title` of `figure`, or else of its
/// first text in reading order, cut as in `page`.
fn quote(canvas: &Canvas, figure: &Figure) -> Option<String> {
    let items = figure.items.iter().filter_map(|i| canvas.item(*i));
    let texts: Vec<(bool, Line)> = items
        .filter_map(|i| match i.shape() {
            Shape::Text { text, .. } => Some((
                i.member().part.as_deref() == Some(TITLE),
                Line {
                    n: i.id().get(),
                    id: String::new(),
                    kind: "text",
                    frame: i.bounds(),
                    fields: Vec::new(),
                    text: Some(text.clone()),
                },
            )),
            _ => None,
        })
        .collect();
    let titled = texts.iter().any(|(title, _)| *title);
    let lines = texts.into_iter().filter(|(title, _)| *title || !titled).map(|(_, l)| l);
    let first = reading_order(lines.collect()).into_iter().next()?.text?;
    Some(first_line(&first))
}
