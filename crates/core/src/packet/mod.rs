//! The packet: what an agent reads from a canvas page instead of a
//! screenshot. The format is `docs/design/packet.md`.
//!
//! [`Index`] is the text part: one line per item, in three sections. `new`
//! holds the user's ink clusters with a stroke that changed since the last
//! sling, `made` the items an agent made or changed since then, and `page`
//! everything else on the page, as an outline. Items an agent made are always
//! text lines, never images (SPEC R2): [`Index::render`] lists only clusters of
//! the user's ink.
//!
//! [`cluster`] groups strokes into the clusters the index takes. What counts
//! as changed is decided elsewhere and passed in; the index decides the ids,
//! the lines and their order.
//!
//! [`build`] puts it all together: a canvas's log, a page, a marker and a
//! selection in, a [`Packet`] out, with its header, version and images.
//! [`Packet::text`] writes the format and [`Packet::read`] reads it back.

mod build;
mod cluster;
mod format;
mod line;

use std::collections::BTreeMap;

use crate::canvas::{Author, Canvas, Item, ItemId, PageId, Rect, Shape};
use line::{Line, colour_name, reading_order};

pub use build::{Ask, build};
pub use cluster::cluster;
pub use format::{Header, Packet, VERSION};
pub use line::Entry;

/// A new cluster of the user's ink that needs an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Render {
    /// The cluster's id in the packet, for example `c42`. Image names start with it.
    pub id: String,
    /// The strokes to draw, in drawing order.
    pub strokes: Vec<ItemId>,
}

/// The text sections of one page's packet: `new`, `made` and `page`.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    new: Vec<Line>,
    made: Vec<Line>,
    page: Vec<Line>,
    render: Vec<Render>,
}

impl Index {
    /// Builds the index of `page`.
    ///
    /// `clusters` groups the user's strokes. A cluster's id is `c` plus the
    /// number of its earliest stroke. Ids in a cluster that are not the user's
    /// strokes on this page are skipped, a stroke in two clusters stays in the
    /// first, and a user stroke in no cluster is a cluster of its own. An
    /// agent's stroke is never clustered: it is a one-stroke `ink` line with
    /// `by`, and no image.
    ///
    /// `changed` says whether an item was made or changed in the packet's op
    /// range. A cluster with a changed stroke goes in `new`; an agent's
    /// changed item goes in `made`; everything else goes in `page`.
    #[must_use]
    pub fn new(
        canvas: &Canvas,
        page: PageId,
        clusters: &[Vec<ItemId>],
        changed: impl Fn(ItemId) -> bool,
    ) -> Self {
        let page = Page::new(canvas, page, clusters);
        let (mut new, mut made, mut rest) = (Vec::new(), Vec::new(), Vec::new());
        for (&n, strokes) in &page.clusters {
            let line = page.ink_line(n, strokes);
            let fresh = strokes.iter().any(|s| changed(s.id()));
            match (strokes[0].author(), fresh) {
                (Author::User, true) => new.push(line),
                (Author::Agent(_), true) => made.push(line),
                _ => rest.push(line),
            }
        }
        for item in page.items.iter().filter(|i| !matches!(i.shape(), Shape::Stroke { .. })) {
            let line = page.item_line(item);
            match item.author() {
                Author::Agent(_) if changed(item.id()) => made.push(line),
                _ => rest.push(line),
            }
        }
        let new = reading_order(new);
        let render = new
            .iter()
            .map(|l| Render {
                id: l.id.clone(),
                strokes: page.clusters[&l.n].iter().map(|s| s.id()).collect(),
            })
            .collect();
        Self { new, made: reading_order(made), page: reading_order(rest), render }
    }

    /// The clusters that need images, in the order their lines are read. Only
    /// the user's ink is ever here.
    #[must_use]
    pub fn render(&self) -> &[Render] {
        &self.render
    }

    /// The three sections as text, each with its `# name` heading, even when
    /// empty. `images` gives the image names of a cluster in `new` by its id;
    /// an empty list writes no `img` field.
    #[must_use]
    pub fn text(&self, images: impl Fn(&str) -> Vec<String>) -> String {
        let mut out = String::new();
        let sections = [("new", &self.new), ("made", &self.made), ("page", &self.page)];
        for (name, lines) in sections {
            out.push_str("# ");
            out.push_str(name);
            out.push('\n');
            for l in lines {
                let images = if name == "new" { images(&l.id) } else { Vec::new() };
                l.write(&mut out, &images, name == "page");
            }
        }
        out
    }

    /// Keeps in the `page` section only the lines whose item number (a
    /// cluster's is its earliest stroke's) passes `keep`.
    fn narrow(&mut self, keep: impl Fn(u64) -> bool) {
        self.page.retain(|l| keep(l.n));
    }
}

/// One page's items, with strokes grouped into clusters.
struct Page<'a> {
    items: Vec<&'a Item>,
    /// By cluster number (its earliest stroke's), the strokes in drawing order.
    clusters: BTreeMap<u64, Vec<&'a Item>>,
    /// Each stroke's cluster number.
    cluster_of: BTreeMap<ItemId, u64>,
    boxes: Vec<&'a Item>,
}

impl<'a> Page<'a> {
    fn new(canvas: &'a Canvas, page: PageId, given: &[Vec<ItemId>]) -> Self {
        let items: Vec<&Item> = canvas.items(page).collect();
        let is_stroke = |i: &Item| matches!(i.shape(), Shape::Stroke { .. });
        let mut cluster_of = BTreeMap::new();
        let mut clusters: BTreeMap<u64, Vec<&Item>> = BTreeMap::new();
        let user = |i: &&Item| *i.author() == Author::User;
        let given = given.iter().map(|group| {
            let found = group.iter().filter_map(|id| canvas.item(*id));
            found.filter(|i| i.page() == page && is_stroke(i)).filter(user).collect::<Vec<_>>()
        });
        // Every stroke left over, the agents' included, is a cluster of its own.
        let lone = items.iter().copied().filter(|i| is_stroke(i)).map(|i| vec![i]);
        for group in given.chain(lone) {
            let mut strokes: Vec<&Item> =
                group.into_iter().filter(|s| !cluster_of.contains_key(&s.id())).collect();
            strokes.sort_by_key(|s| s.id());
            strokes.dedup_by_key(|s| s.id());
            let Some(first) = strokes.first() else { continue };
            let n = first.id().get();
            for s in &strokes {
                cluster_of.insert(s.id(), n);
            }
            clusters.insert(n, strokes);
        }
        let boxes =
            items.iter().copied().filter(|i| matches!(i.shape(), Shape::Box { .. })).collect();
        Self { items, clusters, cluster_of, boxes }
    }

    /// The packet id of an item on this page, if it is still there.
    fn id_of(&self, item: ItemId) -> Option<String> {
        if let Some(n) = self.cluster_of.get(&item) {
            return Some(format!("c{n}"));
        }
        let found = self.items.iter().find(|i| i.id() == item)?;
        let prefix = match found.shape() {
            Shape::Stroke { .. } => "c",
            Shape::Text { .. } => "t",
            Shape::Box { .. } => "b",
            Shape::Arrow { .. } => "a",
            Shape::Live { .. } => "l",
        };
        Some(format!("{prefix}{}", item.get()))
    }

    /// The id of the smallest box, other than `n` itself, that holds `frame`.
    fn container(&self, n: u64, frame: Rect) -> Option<String> {
        self.boxes
            .iter()
            .filter(|b| b.id().get() != n && holds(b.bounds(), frame))
            .min_by(|a, b| area(a.bounds()).total_cmp(&area(b.bounds())).then(a.id().cmp(&b.id())))
            .map(|b| format!("b{}", b.id().get()))
    }

    fn ink_line(&self, n: u64, strokes: &[&Item]) -> Line {
        let first = strokes[0];
        let frame = strokes.iter().map(|s| s.bounds()).reduce(union).unwrap_or_default();
        let colour = match first.shape() {
            Shape::Stroke { colour, .. } => colour_name(*colour),
            _ => String::new(),
        };
        let mut fields = vec![("colour", colour), ("strokes", strokes.len().to_string())];
        if let Author::Agent(name) = first.author() {
            fields.push(("by", name.clone()));
        }
        // An arrow that starts on a stroke of this cluster points the cluster at its target.
        let to = self.items.iter().find_map(|a| match a.shape() {
            Shape::Arrow { from, to, .. }
                if from.item.is_some_and(|f| self.cluster_of.get(&f) == Some(&n)) =>
            {
                to.item.and_then(|t| self.id_of(t))
            }
            _ => None,
        });
        fields.extend(to.map(|t| ("to", t)));
        fields.extend(self.container(n, frame).map(|b| ("in", b)));
        Line { n, id: format!("c{n}"), kind: "ink", frame, fields, text: None }
    }

    fn item_line(&self, item: &Item) -> Line {
        let n = item.id().get();
        let mut fields = Vec::new();
        if let Author::Agent(name) = item.author() {
            fields.push(("by", name.clone()));
        }
        let (kind, text) = match item.shape() {
            Shape::Text { text, .. } => ("text", Some(text.clone())),
            Shape::Box { .. } => ("box", None),
            Shape::Arrow { from, to, .. } => {
                fields.extend(from.item.and_then(|f| self.id_of(f)).map(|f| ("from", f)));
                fields.extend(to.item.and_then(|t| self.id_of(t)).map(|t| ("to", t)));
                ("arrow", None)
            }
            Shape::Stroke { .. } => ("ink", None),
            // An agent UI goes as text (R2): its name and its showing.
            Shape::Live { ui, open, .. } => {
                fields.extend([("ui", ui.clone()), ("open", open.clone())]);
                ("live", None)
            }
        };
        let frame = item.bounds();
        fields.extend(self.container(n, frame).map(|b| ("in", b)));
        let id = self.id_of(item.id()).unwrap_or_default();
        Line { n, id, kind, frame, fields, text }
    }
}

fn holds(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && inner.x + inner.w <= outer.x + outer.w
        && inner.y + inner.h <= outer.y + outer.h
}

fn area(r: Rect) -> f64 {
    r.w * r.h
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect::new(x, y, (a.x + a.w).max(b.x + b.w) - x, (a.y + a.h).max(b.y + b.h) - y)
}
