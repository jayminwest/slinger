//! Groups strokes into clusters: the notes an agent sees one image each of
//! (SPEC R3). Strokes in, clusters out.
//!
//! Two of the user's strokes join when all three signals agree (Spike B
//! finding 2: distance alone joined notes of different colours):
//!
//! - **distance**: their boxes are at most [`GAP`] size units apart, or at
//!   most [`ROW`] when they share a line of writing (their boxes overlap top
//!   to bottom), because words on a line sit further apart than lines do;
//! - **colour**: exactly the same colour;
//! - **time**: made at most [`PAUSE`] apart.
//!
//! Joining is transitive, so a note is a chain of strokes that join. Sizes are
//! relative, because one board holds writing 30 and 300 canvas units tall: a
//! stroke's size unit is the median long edge of the given strokes drawn at
//! its width, which is about how big that pen's letters are.
//!
//! A long stroke (a frame, an arrow, a divider) never joins anything (Spike B
//! finding 1): it is a cluster of its own. A stroke is long when its long edge
//! is over [`LONG`] size units. An agent's stroke is never clustered either
//! (SPEC R2).

use std::collections::BTreeMap;

use crate::canvas::{Author, Canvas, Colour, Item, ItemId, Rect, Shape};

/// How far apart, in size units, two strokes of one note may be.
const GAP: f64 = 3.0;
/// How far apart, in size units, two strokes on one line of a note may be.
/// Word gaps on the fixture board reach about four (the `spec` page).
const ROW: f64 = 5.0;
/// Over how many size units a stroke's long edge makes it long.
const LONG: f64 = 6.0;
/// How long, in milliseconds, the pen may rest between two neighbouring
/// strokes of one note.
const PAUSE: u64 = 5 * 60 * 1000;

/// Groups `strokes` into clusters.
///
/// Each cluster lists its strokes in drawing order (by id), and the clusters
/// come in the order of their earliest stroke, so the same strokes always
/// give the same clusters in the same order, whatever order they are given
/// in. Ids that are not strokes on the canvas are skipped, and an id given
/// twice counts once. Strokes on different pages never join.
#[must_use]
pub fn cluster(canvas: &Canvas, strokes: impl IntoIterator<Item = ItemId>) -> Vec<Vec<ItemId>> {
    let mut found: Vec<&Item> = strokes
        .into_iter()
        .filter_map(|id| canvas.item(id))
        .filter(|i| matches!(i.shape(), Shape::Stroke { .. }))
        .collect();
    found.sort_by_key(|i| i.id());
    found.dedup_by_key(|i| i.id());
    let strokes = sized(&found);

    // Sweep left to right: a stroke can only reach one that starts before its
    // right edge plus the widest reach.
    let mut order: Vec<&Stroke<'_>> = strokes.iter().filter(|s| s.joins).collect();
    order.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
    let widest = strokes.iter().map(|s| s.size).fold(0.0, f64::max) * ROW;
    let mut sets = Sets::new(strokes.len());
    for (k, a) in order.iter().enumerate() {
        let reach = a.bounds.x + a.bounds.w + widest;
        for b in order[k + 1..].iter().take_while(|b| b.bounds.x <= reach) {
            if a.joins_with(b) {
                sets.join(a.index, b.index);
            }
        }
    }

    // Roots are smallest indexes, and indexes are in id order, so the
    // clusters come out ordered by their earliest stroke.
    let mut groups: BTreeMap<usize, Vec<ItemId>> = BTreeMap::new();
    for s in &strokes {
        groups.entry(sets.root(s.index)).or_default().push(s.item.id());
    }
    groups.into_values().collect()
}

/// One stroke as clustering sees it.
struct Stroke<'a> {
    index: usize,
    item: &'a Item,
    colour: Colour,
    bounds: Rect,
    /// The size unit, in canvas units, of the pen that drew this stroke.
    size: f64,
    /// False for a long stroke and an agent's: they stay alone.
    joins: bool,
}

impl Stroke<'_> {
    fn joins_with(&self, other: &Stroke<'_>) -> bool {
        self.item.page() == other.item.page()
            && self.colour == other.colour
            && self.item.time().0.abs_diff(other.item.time().0) <= PAUSE
            && self.near(other)
    }

    /// Whether the boxes are close enough: [`ROW`] size units apart when
    /// they share a line, [`GAP`] otherwise.
    fn near(&self, other: &Stroke<'_>) -> bool {
        let (a, b) = (self.bounds, other.bounds);
        let size = self.size.max(other.size);
        let row = a.y < b.y + b.h && b.y < a.y + a.h;
        let reach = if row { ROW } else { GAP };
        distance(a, b) <= reach * size
    }
}

/// The strokes, in id order, each sized against the user's strokes drawn at
/// its width.
fn sized<'a>(found: &[&'a Item]) -> Vec<Stroke<'a>> {
    let pen = |i: &Item| match i.shape() {
        Shape::Stroke { colour, width, .. } => (*colour, width.to_bits()),
        _ => (Colour::BLACK, 0),
    };
    let long_edge = |r: Rect| r.w.max(r.h);
    let mut edges: BTreeMap<u64, Vec<f64>> = BTreeMap::new();
    for i in found.iter().filter(|i| *i.author() == Author::User) {
        edges.entry(pen(i).1).or_default().push(long_edge(i.bounds()));
    }
    // The lower median, so one long stroke beside one letter is still long.
    let unit: BTreeMap<u64, f64> = edges
        .into_iter()
        .map(|(width, mut e)| {
            e.sort_by(f64::total_cmp);
            (width, e[(e.len() - 1) / 2])
        })
        .collect();
    found
        .iter()
        .enumerate()
        .map(|(index, &item)| {
            let (colour, width) = pen(item);
            let bounds = item.bounds();
            let size = unit.get(&width).copied().unwrap_or(0.0);
            let joins = *item.author() == Author::User && long_edge(bounds) <= LONG * size;
            Stroke { index, item, colour, bounds, size, joins }
        })
        .collect()
}

/// Disjoint sets over stroke indexes. A set's root is its smallest index, so
/// the result does not depend on the order joins happen in.
struct Sets {
    parent: Vec<usize>,
}

impl Sets {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }

    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.parent[a.max(b)] = a.min(b);
    }
}

/// The gap between two rectangles; zero when they touch or overlap.
fn distance(a: Rect, b: Rect) -> f64 {
    let dx = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w)).max(0.0);
    let dy = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h)).max(0.0);
    dx.hypot(dy)
}
