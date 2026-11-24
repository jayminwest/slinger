//! The packet: what an agent reads from a canvas page instead of a
//! screenshot. The format is `docs/design/packet.md`.
//!
//! [`Index`] is the text part: one line per item, in three sections. `new`
//! holds the user's ink clusters with a stroke that changed since the last
//! sling, `made` the items an agent made or changed since then, and `page`
//! everything else on the page, as an outline: a `pdf` item only when an ink
//! line lies over it (`over`) or it is selected, so a long PDF costs nothing
//! a sling does not touch. Items an agent made are always
//! text lines, never images (SPEC R2): [`Index::render`] lists only clusters of
//! the user's ink.
//!
//! Items hidden in a folded box make no line: a folded box is one `box`
//! line with `folded=<n>` and, when it holds new ink, `new=<strokes>` and a
//! place in `new` (docs/design/fold.md, [`fold`]). An end of an arrow bound
//! to a hidden item names the folded box that holds it.
//!
//! A figure (docs/design/figures.md) is one `figure` line, named by the
//! figure; its items make none, and ink over a part names it in `over`
//! ([`figure`]).
//!
//! [`cluster`] groups strokes into the clusters the index takes. What counts
//! as changed is decided elsewhere and passed in; the index decides the ids,
//! the lines and their order.
//!
//! [`build`] puts it all together: a canvas's log, a page, a marker and a
//! selection in, a [`Packet`] out, with its header, version and images.
//! [`Packet::text`] writes the format and [`Packet::read`] reads it back.

mod areas;
mod build;
mod cluster;
mod figure;
mod fold;
mod format;
mod line;
mod words;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::canvas::{Author, Canvas, Figure, Item, ItemId, PageId, Rect, Shape};
use line::{Line, colour_name, reading_order};

pub use areas::{Area, backdrops};
pub use build::{Ask, Inside, build};
pub use cluster::cluster;
pub use format::{Header, Packet, VERSION};
pub use line::Entry;
pub use words::MAX_WORDS_CHARS;

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
    /// The `pdf` lines left out of `page`: no ink line names them. A
    /// selection may still take them ([`Index::narrow`]).
    unnamed: Vec<Line>,
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
    /// range. A cluster with a changed stroke goes in `new`, and so does a
    /// folded box that holds a changed stroke of the user's; an agent's
    /// changed item goes in `made`; everything else goes in `page`, but for
    /// a `pdf` item that no ink line's `over` names. Items hidden in a
    /// folded box are left out.
    #[must_use]
    pub fn new(
        canvas: &Canvas,
        page: PageId,
        clusters: &[Vec<ItemId>],
        changed: impl Fn(ItemId) -> bool,
    ) -> Self {
        Self::of(canvas, page, canvas.visible(page).collect(), clusters, &changed, None)
    }

    /// The index of `items` of `page`, as [`Index::new`] builds a page's:
    /// the visible items, or what one box or part holds (`canvas read
    /// --in`). The items of figure `open` make a line each, with `part=`.
    pub(crate) fn of(
        canvas: &Canvas,
        page: PageId,
        items: Vec<&Item>,
        clusters: &[Vec<ItemId>],
        changed: &dyn Fn(ItemId) -> bool,
        open: Option<&str>,
    ) -> Self {
        let page = Page::new(canvas, page, items, clusters, open);
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
            let line = page.item_line(item, changed);
            match item.author() {
                _ if line.fields.iter().any(|(k, _)| *k == "new") => new.push(line),
                Author::Agent(_) if changed(item.id()) => made.push(line),
                _ => rest.push(line),
            }
        }
        for figure in &page.figures {
            let line = page.figure_line(figure);
            match figure.maker {
                Author::Agent(_) if figure.items.iter().any(|i| changed(*i)) => made.push(line),
                _ => rest.push(line),
            }
        }
        let new = reading_order(new);
        let render = new
            .iter()
            .filter(|l| l.kind == "ink")
            .map(|l| Render {
                id: l.id.clone(),
                strokes: page.clusters[&l.n].iter().map(|s| s.id()).collect(),
            })
            .collect();
        let named = over(new.iter().chain(&made).chain(&rest));
        let (rest, unnamed) =
            rest.into_iter().partition(|l| l.kind != "pdf" || named.contains(&l.id));
        Self { new, made: reading_order(made), page: reading_order(rest), unnamed, render }
    }

    /// The id of the line whose item number is `n` (a cluster's is its
    /// earliest stroke's, a figure's its lowest item's), if there is one.
    fn id(&self, n: u64) -> Option<&str> {
        let mut lines = self.new.iter().chain(&self.made).chain(&self.page);
        lines.find(|l| l.n == n).map(|l| l.id.as_str())
    }

    /// How many lines the three sections hold.
    pub(crate) fn len(&self) -> usize {
        self.new.len() + self.made.len() + self.page.len()
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

    /// Ends the `new` line of cluster `id` with `text`: the PDF's words
    /// under its ink. `page` lines never carry words.
    fn say(&mut self, id: &str, text: String) {
        if let Some(line) = self.new.iter_mut().find(|l| l.id == id) {
            line.text = Some(text);
        }
    }

    /// Keeps in the `page` section only the lines whose item number (a
    /// cluster's is its earliest stroke's) passes `keep`, and the `pdf`
    /// lines an ink line still in the packet names in `over`.
    fn narrow(&mut self, keep: impl Fn(u64) -> bool) {
        let (pdfs, mut page): (Vec<Line>, Vec<Line>) =
            std::mem::take(&mut self.page).into_iter().partition(|l| l.kind == "pdf");
        page.retain(|l| keep(l.n));
        let named = over(self.new.iter().chain(&self.made).chain(&page));
        page.extend(pdfs.into_iter().filter(|l| keep(l.n) || named.contains(&l.id)));
        page.extend(std::mem::take(&mut self.unnamed).into_iter().filter(|l| keep(l.n)));
        self.page = reading_order(page);
    }
}

/// One page's items, or one box's, with strokes grouped into clusters.
struct Page<'a> {
    canvas: &'a Canvas,
    /// The page the items are on.
    on: PageId,
    /// The items that make lines of their own: the items read, but for
    /// those of a figure that makes one line.
    items: Vec<&'a Item>,
    /// Every item read, for the ids arrow ends name.
    all: Vec<&'a Item>,
    /// The figures that make one line each.
    figures: Vec<Figure>,
    /// The figure whose items make a line each, read `--in` it.
    open: Option<&'a str>,
    /// The parts of the page's figures, smallest first ([`figure::parts`]).
    parts: Vec<(String, Rect)>,
    /// Each item a folded box on the page holds, with that box.
    holders: HashMap<ItemId, ItemId>,
    /// By cluster number (its earliest stroke's), the strokes in drawing order.
    clusters: BTreeMap<u64, Vec<&'a Item>>,
    /// Each stroke's cluster number.
    cluster_of: BTreeMap<ItemId, u64>,
    boxes: Vec<&'a Item>,
    /// The `pdf` items, in drawing order.
    pdfs: Vec<&'a Item>,
}

impl<'a> Page<'a> {
    fn new(
        canvas: &'a Canvas,
        page: PageId,
        all: Vec<&'a Item>,
        given: &[Vec<ItemId>],
        open: Option<&'a str>,
    ) -> Self {
        let (items, figures) = figure::split(canvas, all.clone(), open);
        let here: HashSet<ItemId> = items.iter().map(|i| i.id()).collect();
        let is_stroke = |i: &Item| matches!(i.shape(), Shape::Stroke { .. });
        let mut cluster_of = BTreeMap::new();
        let mut clusters: BTreeMap<u64, Vec<&Item>> = BTreeMap::new();
        let user = |i: &&Item| *i.author() == Author::User;
        let given = given.iter().map(|group| {
            let found = group.iter().filter_map(|id| canvas.item(*id));
            found
                .filter(|i| here.contains(&i.id()) && is_stroke(i))
                .filter(user)
                .collect::<Vec<_>>()
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
        let kind = |pdf: bool| {
            let pick = move |i: &&Item| match i.shape() {
                Shape::Pdf { .. } => pdf,
                Shape::Box { .. } => !pdf,
                _ => false,
            };
            items.iter().copied().filter(pick).collect()
        };
        let (boxes, pdfs) = (kind(false), kind(true));
        let holders = canvas.holders(page);
        let parts = figure::parts(canvas, page);
        Self {
            canvas,
            on: page,
            items,
            all,
            figures,
            open,
            parts,
            holders,
            clusters,
            cluster_of,
            boxes,
            pdfs,
        }
    }

    /// The packet id of an item here, if it is still there: for an item
    /// hidden in a folded box, the outermost folded box here that holds it;
    /// for an item of a figure that makes one line, its address.
    fn id_of(&self, item: ItemId) -> Option<String> {
        if let Some(n) = self.cluster_of.get(&item) {
            return Some(format!("c{n}"));
        }
        let mut item = item;
        let found = loop {
            match self.all.iter().find(|i| i.id() == item) {
                Some(found) => break found,
                None => item = *self.holders.get(&item)?,
            }
        };
        if figure::shut(found, self.open) {
            return figure::address(found);
        }
        let prefix = match found.shape() {
            Shape::Stroke { .. } => "c",
            Shape::Text { .. } => "t",
            Shape::Box { .. } => "b",
            Shape::Arrow { .. } => "a",
            Shape::Live { .. } => "l",
            Shape::Button { .. } => "u",
            Shape::Pdf { .. } => "d",
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
        // The pages of PDFs the ink lies on, so a reader knows what it marks.
        let under: Vec<String> = self
            .pdfs
            .iter()
            .filter(|p| overlaps(p.bounds(), frame))
            .map(|p| format!("d{}", p.id().get()))
            .collect();
        // And the parts of figures it lies in, by the centre of its box.
        let parts = self.parts.iter().filter(|(_, part)| figure::centred(*part, frame));
        let under: Vec<String> = under.into_iter().chain(parts.map(|(a, _)| a.clone())).collect();
        if !under.is_empty() {
            fields.push(("over", under.join(",")));
        }
        Line { n, id: format!("c{n}"), kind: "ink", frame, fields, text: None }
    }

    fn item_line(&self, item: &Item, changed: &dyn Fn(ItemId) -> bool) -> Line {
        let n = item.id().get();
        let mut fields = Vec::new();
        if let Author::Agent(name) = item.author() {
            fields.push(("by", name.clone()));
        }
        if self.open.is_some() {
            fields.extend(item.member().part.clone().map(|p| ("part", p)));
        }
        let (kind, text) = match item.shape() {
            Shape::Text { text, .. } => ("text", Some(text.clone())),
            Shape::Box { fold: Some(_), .. } => {
                let (folded, text) = fold::line(self.canvas, self.on, item, changed);
                fields.extend(folded);
                ("box", text)
            }
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
            Shape::Button { .. } => ("button", None),
            // A PDF's page goes as text, never an image (R8): which page,
            // which part of it, and the blob that holds the file.
            Shape::Pdf { hash, pdf_page, crop, name, .. } => {
                fields.push(("pdfpage", pdf_page.to_string()));
                if let Some(c) = crop {
                    let points = [c.x, c.y, c.w, c.h].map(f64::round).map(|v| v.to_string());
                    fields.push(("crop", points.join(",")));
                }
                fields.extend(name.clone().map(|n| ("name", n)));
                fields.push(("blob", hash.clone()));
                ("pdf", None)
            }
        };
        let frame = item.bounds();
        fields.extend(self.container(n, frame).map(|b| ("in", b)));
        let id = self.id_of(item.id()).unwrap_or_default();
        Line { n, id, kind, frame, fields, text }
    }
}

/// The ids the `over` fields of `lines` name.
fn over<'a>(lines: impl Iterator<Item = &'a Line>) -> Vec<String> {
    lines
        .flat_map(|l| l.fields.iter().filter(|(k, _)| *k == "over"))
        .flat_map(|(_, v)| v.split(',').map(str::to_owned))
        .collect()
}

/// Whether two rectangles share any point, edges included.
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
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
