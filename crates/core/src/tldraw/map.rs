//! tldraw records to canvas operations. Every item goes through
//! [`Canvas::apply`], so an imported canvas keeps the same rules as any other.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{Import, Report, path};
use crate::canvas::{
    Action, Applied, Author, Canvas, Colour, End, ItemId, Op, PageId, Point, Rect, Shape, Time,
};

static NULL: Value = Value::Null;

/// One tldraw shape record.
struct Node<'a> {
    id: &'a str,
    parent: &'a str,
    index: &'a str,
    kind: &'a str,
    at: Point,
    rotation: f64,
    props: &'a Value,
}

impl<'a> Node<'a> {
    fn read(r: &'a Value) -> Option<Self> {
        Some(Self {
            id: r.get("id")?.as_str()?,
            parent: r.get("parentId")?.as_str()?,
            index: text(r, "index"),
            kind: r.get("type")?.as_str()?,
            at: Point::new(num(r, "x", 0.0), num(r, "y", 0.0)),
            rotation: num(r, "rotation", 0.0),
            props: r.get("props").unwrap_or(&NULL),
        })
    }

    fn num(&self, key: &str, default: f64) -> f64 {
        num(self.props, key, default)
    }

    fn colour(&self, key: &str) -> Colour {
        colour(text(self.props, key))
    }
}

/// Where a shape's own space sits on the page: its origin and its turn.
#[derive(Debug, Clone, Copy, Default)]
struct Place {
    origin: Point,
    angle: f64,
}

impl Place {
    fn to_page(self, p: Point) -> Point {
        let (sin, cos) = self.angle.sin_cos();
        Point::new(self.origin.x + p.x * cos - p.y * sin, self.origin.y + p.x * sin + p.y * cos)
    }

    fn child(self, n: &Node) -> Self {
        Self { origin: self.to_page(n.at), angle: self.angle + n.rotation }
    }

    /// The page-space box around a `w` by `h` rectangle in this space.
    fn rect(self, w: f64, h: f64) -> Rect {
        let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
        Rect::around(corners.map(|(x, y)| self.to_page(Point::new(x, y)))).unwrap_or_default()
    }
}

/// An arrow added with free ends, waiting for its bindings.
struct Arrow {
    item: ItemId,
    from: End,
    to: End,
    colour: Colour,
}

struct Builder {
    canvas: Canvas,
    report: Report,
    at: Time,
    /// tldraw shape id to its item and the item's bounds.
    items: HashMap<String, (ItemId, Rect)>,
    arrows: HashMap<String, Arrow>,
    seen: HashSet<String>,
}

pub(super) fn build(records: &[Value], at: Time) -> Import {
    let of = |name: &'static str| records.iter().filter(move |r| text(r, "typeName") == name);
    let mut pages: Vec<&Value> = of("page").collect();
    pages.sort_by_key(|p| (text(p, "index"), text(p, "id")));
    let nodes: Vec<Node> = of("shape").filter_map(Node::read).collect();
    let mut children: HashMap<&str, Vec<&Node>> = HashMap::new();
    for n in &nodes {
        children.entry(n.parent).or_default().push(n);
    }
    for list in children.values_mut() {
        list.sort_by_key(|n| (n.index, n.id));
    }

    let mut b = Builder {
        canvas: Canvas::new(),
        report: Report::default(),
        at,
        items: HashMap::new(),
        arrows: HashMap::new(),
        seen: HashSet::new(),
    };
    for (i, page) in pages.iter().enumerate() {
        let id = b.page(i, text(page, "name"));
        b.walk(&children, text(page, "id"), id, Place::default());
    }
    b.report.pages = b.canvas.pages().count();
    let orphans = nodes.iter().filter(|n| !b.seen.contains(n.id)).count();
    if orphans > 0 {
        b.report.skipped.insert("orphan".into(), orphans);
    }
    b.bind(of("binding"));
    Import { canvas: b.canvas, report: b.report }
}

impl Builder {
    fn apply(&mut self, action: Action) -> Option<Applied> {
        self.canvas.apply(Op { by: Author::User, at: self.at, action }).ok()
    }

    /// The canvas page for the `i`th tldraw page, named as near to `name` as
    /// the canvas allows (names are unique and not blank).
    fn page(&mut self, i: usize, name: &str) -> PageId {
        let first = self.canvas.pages().next().map(crate::canvas::Page::id);
        let base = if name.trim().is_empty() { format!("Page {}", i + 1) } else { name.into() };
        let mut name = base.clone();
        for n in 2.. {
            if i == 0 || self.canvas.page_named(&name).is_none() {
                break;
            }
            name = format!("{base} {n}");
        }
        match (i, first) {
            (0, Some(page)) => {
                self.apply(Action::PageRename { page, name });
                page
            }
            _ => match self.apply(Action::PageAdd { name }) {
                Some(Applied::PageAdded(page)) => page,
                _ => first.unwrap_or(PageId::new(1)),
            },
        }
    }

    /// Adds `parent`'s children, depth first in drawing order.
    fn walk(
        &mut self,
        children: &HashMap<&str, Vec<&Node>>,
        parent: &str,
        page: PageId,
        place: Place,
    ) {
        for &n in children.get(parent).map_or(&[][..], Vec::as_slice) {
            if !self.seen.insert(n.id.to_string()) {
                continue;
            }
            let own = place.child(n);
            self.shape(n, own, page);
            self.walk(children, n.id, page, own);
        }
    }

    fn shape(&mut self, n: &Node, own: Place, page: PageId) {
        let shapes = match (n.kind, text(n.props, "geo")) {
            ("draw", _) => draw(n, own).map(|s| vec![s]),
            ("text", _) => Some(vec![label(n, own, "color", n.num("w", 0.0), None)]),
            ("geo", "rectangle") => Some(rectangle(n, own)),
            ("arrow", _) => arrow(n, own).map(|s| vec![s]),
            ("geo", geo) => return self.skip(format!("geo:{geo}")),
            (kind, _) => return self.skip(kind.into()),
        };
        let Some(shapes) = shapes else {
            return self.skip(format!("{} (damaged)", n.kind));
        };
        for shape in shapes {
            let Some(Applied::Added(item)) = self.apply(Action::Add { page, shape: shape.clone() })
            else {
                return self.skip(format!("{} (damaged)", n.kind));
            };
            match shape {
                Shape::Stroke { .. } => self.report.strokes += 1,
                Shape::Text { .. } => self.report.texts += 1,
                Shape::Box { .. } => self.report.boxes += 1,
                Shape::Arrow { from, to, colour } => {
                    self.report.arrows += 1;
                    self.arrows.insert(n.id.into(), Arrow { item, from, to, colour });
                }
                // tldraw has no live items; the mapping never makes one.
                Shape::Live { .. } => {}
            }
            // A rectangle's label must not hide the box from arrows bound to it.
            self.items.entry(n.id.into()).or_insert((item, shape.bounds()));
        }
    }

    fn skip(&mut self, kind: String) {
        *self.report.skipped.entry(kind).or_default() += 1;
    }

    /// Points each bound arrow end at its item, at the binding's anchor.
    fn bind<'a>(&mut self, bindings: impl Iterator<Item = &'a Value>) {
        let mut bound = Vec::new();
        for b in bindings.filter(|b| text(b, "type") == "arrow") {
            let (Some(arrow), Some(&(item, r))) =
                (self.arrows.get_mut(text(b, "fromId")), self.items.get(text(b, "toId")))
            else {
                continue;
            };
            let anchor = b.get("props").and_then(|p| p.get("normalizedAnchor")).unwrap_or(&NULL);
            let at =
                Point::new(r.x + r.w * num(anchor, "x", 0.5), r.y + r.h * num(anchor, "y", 0.5));
            match text(b.get("props").unwrap_or(&NULL), "terminal") {
                "start" => arrow.from = End::on(at, item),
                "end" => arrow.to = End::on(at, item),
                _ => continue,
            }
            bound.push(arrow.item);
        }
        bound.sort_unstable();
        bound.dedup();
        let mut edits: Vec<_> = self
            .arrows
            .values()
            .filter(|a| bound.binary_search(&a.item).is_ok())
            .map(|a| (a.item, Shape::Arrow { from: a.from, to: a.to, colour: a.colour }))
            .collect();
        edits.sort_by_key(|(item, _)| *item);
        for (item, shape) in edits {
            self.apply(Action::Edit { item, shape });
        }
    }
}

/// tldraw's stroke widths for its four sizes, in canvas units.
fn width(size: &str) -> f64 {
    match size {
        "s" => 2.0,
        "l" => 5.0,
        "xl" => 10.0,
        _ => 3.5,
    }
}

/// Font sizes for the four sizes, in canvas units.
fn font(size: &str) -> f64 {
    match size {
        "s" => 18.0,
        "l" => 36.0,
        "xl" => 44.0,
        _ => 24.0,
    }
}

fn draw(n: &Node, own: Place) -> Option<Shape> {
    let (sx, sy) = (n.num("scaleX", 1.0), n.num("scaleY", 1.0));
    let mut points = Vec::new();
    for segment in n.props.get("segments")?.as_array()? {
        let local = path::points(segment)?;
        points.extend(local.into_iter().map(|p| own.to_page(Point::new(p.x * sx, p.y * sy))));
    }
    let width = width(text(n.props, "size")) * n.num("scale", 1.0);
    Some(Shape::Stroke { points, colour: n.colour("color"), width })
}

fn rectangle(n: &Node, own: Place) -> Vec<Shape> {
    let (w, h) = (n.num("w", 0.0), n.num("h", 0.0) + n.num("growY", 0.0));
    let frame = own.rect(w, h);
    let mut out = vec![Shape::Box { frame, colour: n.colour("color") }];
    if !plain(n.props).trim().is_empty() {
        out.push(label(n, own, "labelColor", w, Some(frame)));
    }
    out
}

/// A shape's text as a text item. Without a `frame`, the frame is `w` wide
/// (scaled) and as tall as the text is likely to be at its font size.
fn label(n: &Node, own: Place, colour: &str, w: f64, frame: Option<Rect>) -> Shape {
    let text = plain(n.props);
    let frame = frame.unwrap_or_else(|| {
        let scale = n.num("scale", 1.0);
        let size = font(self::text(n.props, "size")) * scale;
        let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
        let w = if w > 0.0 { w * scale } else { chars(longest) * size * 0.55 };
        own.rect(w, lines(&text, w, size) * size * 1.35)
    });
    Shape::Text { text, frame, colour: n.colour(colour) }
}

/// About how many lines `text` takes when wrapped to `w` at font size `size`.
fn lines(text: &str, w: f64, size: f64) -> f64 {
    let per_line = (w / (size * 0.55)).max(1.0);
    text.split('\n').map(|l| (chars(l.chars().count().max(1)) / per_line).ceil()).sum()
}

/// A character count as a number.
fn chars(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

fn arrow(n: &Node, own: Place) -> Option<Shape> {
    let end = |key: &str| {
        let p = n.props.get(key)?;
        Some(End::free(own.to_page(Point::new(p.get("x")?.as_f64()?, p.get("y")?.as_f64()?))))
    };
    Some(Shape::Arrow { from: end("start")?, to: end("end")?, colour: n.colour("color") })
}

/// The plain text of a shape: `richText` (a document tree) or the older `text`.
fn plain(props: &Value) -> String {
    fn walk(node: &Value, out: &mut String) {
        match text(node, "type") {
            "text" => out.push_str(text(node, "text")),
            "hardBreak" => out.push('\n'),
            kind => {
                for child in node.get("content").and_then(Value::as_array).into_iter().flatten() {
                    walk(child, out);
                }
                if matches!(kind, "paragraph" | "heading" | "codeBlock") {
                    out.push('\n');
                }
            }
        }
    }
    match props.get("richText") {
        Some(doc) => {
            let mut out = String::new();
            walk(doc, &mut out);
            out.trim_end_matches('\n').to_string()
        }
        None => text(props, "text").to_string(),
    }
}

/// Slinger's value for each of tldraw's colour names. Unknown names are black.
fn colour(name: &str) -> Colour {
    match name {
        "grey" => Colour::rgb(140, 145, 155),
        "light-violet" => Colour::rgb(215, 160, 240),
        "violet" => Colour::rgb(140, 70, 200),
        "blue" => Colour::rgb(60, 100, 220),
        "light-blue" => Colour::rgb(80, 170, 240),
        "yellow" => Colour::rgb(240, 180, 40),
        "orange" => Colour::rgb(235, 120, 40),
        "green" => Colour::rgb(40, 140, 90),
        "light-green" => Colour::rgb(80, 190, 110),
        "light-red" => Colour::rgb(245, 120, 130),
        "red" => Colour::rgb(215, 40, 50),
        "white" => Colour::rgb(255, 255, 255),
        _ => Colour::rgb(30, 30, 30),
    }
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(default)
}
