//! The packet's text sections, through the public interface: a canvas in,
//! lines out. Snapshots hold the line format of `docs/design/packet.md`.
#![cfg(test)]

use std::collections::BTreeSet;

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, Item, ItemId, Op, PageId, Point, Rect, Shape,
    Time,
};
use slinger_core::packet::Index;

const BLACK: Colour = Colour::BLACK;
const RED: Colour = Colour::rgb(255, 0, 0);
const BLUE: Colour = Colour::rgb(0, 0, 255);

/// Builds one page with the operations a test names, and remembers which
/// items were added after the last sling.
struct Board {
    canvas: Canvas,
    page: PageId,
    slung: bool,
    changed: BTreeSet<ItemId>,
}

impl Board {
    fn new() -> Self {
        let canvas = Canvas::new();
        let page = canvas.pages().next().unwrap().id();
        Self { canvas, page, slung: false, changed: BTreeSet::new() }
    }

    /// Everything added after this call counts as changed.
    fn sling(&mut self) {
        self.slung = true;
    }

    fn op(&mut self, by: &Author, action: Action) -> Applied {
        let op = Op { by: by.clone(), at: Time(1_700_000_000_000), action };
        self.canvas.apply(op).unwrap()
    }

    fn add(&mut self, by: &Author, shape: Shape) -> ItemId {
        let page = self.page;
        let Applied::Added(id) = self.op(by, Action::Add { page, shape }) else {
            panic!("not added")
        };
        if self.slung {
            self.changed.insert(id);
        }
        id
    }

    fn stroke(&mut self, colour: Colour, points: &[(f64, f64)]) -> ItemId {
        let points = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        self.add(&Author::User, Shape::Stroke { points, colour, width: 2.0 })
    }

    fn index(&self, clusters: &[Vec<ItemId>]) -> Index {
        Index::new(&self.canvas, self.page, clusters, |id| self.changed.contains(&id))
    }
}

fn claude() -> Author {
    Author::Agent("claude".into())
}

fn text(body: &str, at: [f64; 4]) -> Shape {
    Shape::Text {
        text: body.into(),
        frame: Rect::new(at[0], at[1], at[2], at[3]),
        colour: BLACK,
        size: 24.0,
    }
}

fn frame(x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape::Box { frame: Rect::new(x, y, w, h), colour: BLACK }
}

fn arrow(from: End, to: End) -> Shape {
    Shape::Arrow { from, to, colour: BLACK }
}

fn pt(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn png(id: &str) -> Vec<String> {
    vec![format!("{id}.png")]
}

/// The example page of the design document: old ink, a box and a text item,
/// then since the last sling a black note and a red note with an arrow to text
/// the agent wrote.
fn example() -> (Board, Vec<Vec<ItemId>>) {
    let mut b = Board::new();
    let old_a = b.stroke(BLACK, &[(41.0, 1201.0), (939.0, 1599.0)]);
    let old_b = b.stroke(BLACK, &[(100.0, 1300.0), (500.0, 1400.0)]);
    b.add(&claude(), frame(980.0, 1200.0, 620.0, 300.0));
    let blue = b.stroke(BLUE, &[(1001.0, 1241.0), (1559.0, 1459.0)]);
    b.add(&claude(), text("Spike B: 44% of the screenshot cost.", [40.0, 1700.0, 700.0, 60.0]));
    b.sling();
    let note = b.stroke(BLACK, &[(121.0, 81.0), (639.0, 219.0)]);
    let red = b.stroke(RED, &[(121.0, 261.0), (419.0, 349.0)]);
    let said = b.add(&claude(), text("Packet: images of new ink.", [480.0, 240.0, 360.0, 120.0]));
    b.add(&claude(), arrow(End::on(pt(420.0, 300.0), red), End::on(pt(480.0, 300.0), said)));
    (b, vec![vec![old_a, old_b], vec![blue], vec![note], vec![red]])
}

#[test]
fn the_example_page_reads_as_the_design_document_says() {
    let (b, clusters) = example();
    let index = b.index(&clusters);
    let want = "\
# new
c6 ink 120,80 520x140 colour=black strokes=1 img=c6.png
c7 ink 120,260 300x90 colour=red strokes=1 to=t8 img=c7.png
# made
a9 arrow 420,300 60x0 by=claude from=c7 to=t8
t8 text 480,240 360x120 by=claude \"Packet: images of new ink.\"
# page
c1 ink 40,1200 900x400 colour=black strokes=2
b3 box 980,1200 620x300 by=claude
c4 ink 1000,1240 560x220 colour=blue strokes=1 in=b3
t5 text 40,1700 700x60 by=claude \"Spike B: 44% of the screenshot cost.\"
";
    assert_eq!(index.text(png), want);
    let render: Vec<_> = index.render().iter().map(|r| (r.id.as_str(), r.strokes.len())).collect();
    assert_eq!(render, [("c6", 1), ("c7", 1)]);
}

#[test]
fn nothing_new_still_writes_every_section() {
    let (mut b, clusters) = example();
    b.changed.clear();
    let index = b.index(&clusters);
    assert_eq!(index.render(), []);
    let text = index.text(png);
    assert!(text.starts_with("# new\n# made\n# page\nc6 ink "), "{text}");
    assert!(!text.contains("img="));
}

#[test]
fn image_tiles_are_listed_in_order_and_only_in_new() {
    let (b, clusters) = example();
    let text = b.index(&clusters).text(|id| vec![format!("{id}-1.png"), format!("{id}-2.png")]);
    assert!(text.contains(" img=c6-1.png,c6-2.png\n"));
    assert_eq!(text.matches("img=").count(), 2);
}

/// A busier page: nested boxes, rows, long and odd text, an agent's own
/// strokes, an arrow to a removed item, and clusters with ids that do not
/// belong in them.
fn fixture_page() -> (Board, Vec<Vec<ItemId>>) {
    let mut b = Board::new();
    let outer = b.add(&Author::User, frame(0.0, 0.0, 1000.0, 600.0));
    let inner = b.add(&claude(), frame(50.0, 50.0, 400.0, 300.0));
    let in_inner = b.stroke(BLACK, &[(100.0, 100.0), (300.0, 160.0)]);
    let in_outer = b.stroke(BLACK, &[(600.0, 90.0), (800.0, 140.0)]);
    let long = "A note with \"quotes\", a back\\slash, a line\nbreak, and enough words to run past eighty characters — ünïcödé too.";
    b.add(&Author::User, text(long, [60.0, 400.0, 600.0, 80.0]));
    let gone = b.add(&Author::User, text("soon gone", [1200.0, 0.0, 100.0, 20.0]));
    b.add(&Author::User, arrow(End::on(pt(900.0, 500.0), gone), End::free(pt(1200.0, 10.0))));
    let other = b.op(&Author::User, Action::PageAdd { name: "elsewhere".into() });
    let Applied::PageAdded(elsewhere) = other else { panic!("no page") };
    let home = std::mem::replace(&mut b.page, elsewhere);
    let away = b.stroke(BLACK, &[(10.0, 10.0), (20.0, 20.0)]);
    b.page = home;
    b.op(&Author::User, Action::Remove { item: gone });
    b.sling();
    let wobble =
        b.stroke(Colour { r: 10, g: 20, b: 30, a: 200 }, &[(120.0, 120.4), (180.6, 140.0)]);
    let late = b.stroke(RED, &[(610.0, 700.0), (700.0, 760.0)]);
    let solo = b.stroke(RED, &[(20.0, 700.0), (90.0, 760.0)]);
    let drawn = b
        .add(&claude(), Shape::Stroke { points: vec![pt(300.0, 700.0)], colour: BLUE, width: 4.0 });
    b.add(&claude(), arrow(End::on(pt(300.0, 700.0), drawn), End::on(pt(600.0, 100.0), in_outer)));
    b.add(&Author::User, arrow(End::on(pt(610.0, 700.0), late), End::on(pt(450.0, 50.0), inner)));
    let clusters = vec![
        vec![in_inner, away, outer],
        vec![late, in_inner, drawn],
        vec![in_outer],
        vec![ItemId::new(999)],
        vec![wobble],
    ];
    assert_eq!(b.canvas.item(solo).map(Item::page), Some(b.page));
    (b, clusters)
}

#[test]
fn the_fixture_page_matches_its_snapshot() {
    let (b, clusters) = fixture_page();
    let got = b.index(&clusters).text(png);
    let want = include_str!("fixtures/packet/fixture-page.txt");
    assert_eq!(got, want, "if the change is meant, paste this into the snapshot:\n{got}");
}

#[test]
fn no_agent_item_is_ever_in_the_render_list() {
    let (mut b, clusters) = fixture_page();
    // Count every item as changed, and offer the agent's stroke in every cluster.
    b.changed = (1..=40).map(ItemId::new).collect();
    let drawn: Vec<ItemId> = (1..=40)
        .map(ItemId::new)
        .filter(|id| b.canvas.item(*id).is_some_and(|i| i.author() != &Author::User))
        .collect();
    let clusters: Vec<Vec<ItemId>> = clusters
        .into_iter()
        .map(|c| c.into_iter().chain(drawn.iter().copied()).collect())
        .collect();
    let index = b.index(&clusters);
    assert_ne!(index.render(), []);
    for r in index.render() {
        for s in &r.strokes {
            assert_eq!(b.canvas.item(*s).unwrap().author(), &Author::User, "{} in {}", s, r.id);
        }
    }
    let text = index.text(png);
    let made = text.split("# made\n").nth(1).unwrap().split("# page\n").next().unwrap();
    assert!(made.lines().all(|l| l.contains(" by=claude") && !l.contains("img=")), "{made}");
}
