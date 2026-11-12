//! Clustering ink, through the public interface: strokes in, clusters out.
//! The real board is the main fixture; small drawn pages pin the rules the
//! board cannot show (it was imported with one time for every item).
#![cfg(test)]

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, Item, ItemId, Op, PageId, Point, Rect, Shape, Time,
};
use slinger_core::packet::cluster;
use slinger_core::tldraw::import;

const BLACK: Colour = Colour::BLACK;
const RED: Colour = Colour::rgb(255, 0, 0);
const MINUTE: u64 = 60_000;

/// The owner's real board (SPEC B8).
const BOARD: &[u8] = include_bytes!("fixtures/idea.tldraw");

fn board() -> Canvas {
    import(BOARD, Time(1_700_000_000_000)).unwrap().canvas
}

fn page_clusters(canvas: &Canvas, page: PageId) -> Vec<Vec<ItemId>> {
    cluster(canvas, canvas.items(page).map(Item::id))
}

fn colour_of(canvas: &Canvas, id: ItemId) -> Colour {
    match canvas.item(id).unwrap().shape() {
        Shape::Stroke { colour, .. } => *colour,
        _ => panic!("not a stroke"),
    }
}

fn gap(a: Rect, b: Rect) -> f64 {
    let dx = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w)).max(0.0);
    let dy = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h)).max(0.0);
    dx.hypot(dy)
}

#[test]
fn every_fixture_page_has_its_recorded_cluster_count() {
    let canvas = board();
    let counts: Vec<_> = canvas
        .pages()
        .map(|p| (p.name().to_string(), page_clusters(&canvas, p.id()).len()))
        .collect();
    let counts: Vec<_> = counts.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    // Spike B made 220 crops of the whole board with distance alone.
    assert_eq!(
        counts,
        [
            ("inital-planning", 67),
            ("claude", 4),
            ("slinger-diagrams", 12),
            ("fable-5.2", 27),
            ("spec", 13)
        ]
    );
}

#[test]
fn two_notes_of_different_colours_side_by_side_stay_two_clusters() {
    // On "slinger-diagrams" a yellow note ("Recognizer is a upon user request
    // kinda thing ...") sits 36 canvas units left of a purple one ("~/sync/
    // is just an example ..."), closer than the word gaps inside each note.
    let canvas = board();
    let page = canvas.page_named("slinger-diagrams").unwrap().id();
    let clusters = page_clusters(&canvas, page);
    let find = |first: u64| clusters.iter().find(|c| c[0] == ItemId::new(first)).unwrap();
    let (purple, yellow) = (find(3085), find(3191));
    assert_eq!(colour_of(&canvas, purple[0]), Colour::rgb(140, 70, 200));
    assert_eq!(colour_of(&canvas, yellow[0]), Colour::rgb(240, 180, 40));
    assert_eq!((purple.len(), yellow.len()), (106, 211));
    let apart = gap(
        canvas.bounds_of(purple.iter().copied()).unwrap(),
        canvas.bounds_of(yellow.iter().copied()).unwrap(),
    );
    assert!(apart < 40.0, "{apart}");
}

#[test]
fn a_note_with_wide_word_gaps_is_one_cluster_per_paragraph() {
    // On "spec", "I have an agent working on the / Scaffolding now. Lets do
    // the spikes while the other / agent works." has word gaps of 124 to 152
    // canvas units, about four size units (slinger-c94c3e). A blank line
    // splits it in two.
    let canvas = board();
    let page = canvas.page_named("spec").unwrap().id();
    let clusters = page_clusters(&canvas, page);
    let firsts: Vec<u64> = clusters.iter().map(|c| c[0].get()).collect();
    for id in [7729, 7773] {
        assert!(firsts.contains(&id), "{firsts:?}");
    }
    for id in [7744, 7793, 7798] {
        assert!(!firsts.contains(&id), "{firsts:?}");
        assert!(clusters.iter().any(|c| c.contains(&ItemId::new(id))));
    }
}

#[test]
fn clusters_never_mix_colours_and_cover_every_stroke_once() {
    let canvas = board();
    for page in canvas.pages() {
        let clusters = page_clusters(&canvas, page.id());
        let mut all: Vec<ItemId> = clusters.iter().flatten().copied().collect();
        let strokes = canvas.items(page.id()).filter(|i| matches!(i.shape(), Shape::Stroke { .. }));
        assert_eq!(all.len(), strokes.count());
        all.sort();
        all.dedup();
        assert_eq!(all.len(), clusters.iter().map(Vec::len).sum::<usize>());
        for c in &clusters {
            assert!(c.iter().all(|&s| colour_of(&canvas, s) == colour_of(&canvas, c[0])));
        }
    }
}

/// One page drawn stroke by stroke, a minute apart unless a test says so.
struct Page {
    canvas: Canvas,
    on: PageId,
    clock: u64,
}

impl Page {
    fn new() -> Self {
        let canvas = Canvas::new();
        let page = canvas.pages().next().unwrap().id();
        Self { canvas, on: page, clock: 1_700_000_000_000 }
    }

    fn wait(&mut self, ms: u64) {
        self.clock += ms;
    }

    fn draw(&mut self, by: &Author, colour: Colour, points: &[(f64, f64)]) -> ItemId {
        let points = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        let shape = Shape::Stroke { points, colour, width: 2.0 };
        let op = Op {
            by: by.clone(),
            at: Time(self.clock),
            action: Action::Add { page: self.on, shape },
        };
        let Applied::Added(id) = self.canvas.apply(op).unwrap() else { panic!("not added") };
        id
    }

    /// A letter: a 20 by 20 zigzag with its top left at `x`, `y`.
    fn letter(&mut self, colour: Colour, x: f64, y: f64) -> ItemId {
        self.draw(&Author::User, colour, &[(x, y), (x + 10.0, y + 20.0), (x + 20.0, y)])
    }

    /// A word of `n` letters 10 apart, starting at `x`, `y`.
    fn note(&mut self, colour: Colour, x: f64, y: f64, n: u32) -> Vec<ItemId> {
        (0..n).map(|k| self.letter(colour, x + 30.0 * f64::from(k), y)).collect()
    }

    fn clusters(&self) -> Vec<Vec<ItemId>> {
        page_clusters(&self.canvas, self.on)
    }
}

#[test]
fn a_frame_around_three_notes_is_its_own_cluster_and_joins_none() {
    let mut p = Page::new();
    let a = p.note(BLACK, 20.0, 20.0, 6);
    let b = p.note(BLACK, 400.0, 20.0, 6);
    let c = p.note(BLACK, 20.0, 200.0, 6);
    let frame = p.draw(
        &Author::User,
        BLACK,
        &[(0.0, 0.0), (800.0, 0.0), (800.0, 300.0), (0.0, 300.0), (0.0, 0.0)],
    );
    assert_eq!(p.clusters(), [a, b, c, vec![frame]]);
}

#[test]
fn a_long_stroke_beside_two_notes_does_not_join_them() {
    let mut p = Page::new();
    let a = p.note(BLACK, 0.0, 0.0, 4);
    // A bracket down the left of both notes, 9 units from each.
    let line = p.draw(&Author::User, BLACK, &[(-10.0, 0.0), (-10.0, 220.0)]);
    let b = p.note(BLACK, 0.0, 200.0, 4);
    assert_eq!(p.clusters(), [a, vec![line], b]);
}

#[test]
fn letters_close_together_join_and_a_far_note_does_not() {
    let mut p = Page::new();
    let mut a = p.note(BLACK, 0.0, 0.0, 3);
    // The next line, 20 below: the same note.
    a.extend(p.note(BLACK, 0.0, 40.0, 3));
    let far = p.note(BLACK, 0.0, 200.0, 3);
    assert_eq!(p.clusters(), [a, far]);
}

#[test]
fn words_on_one_line_join_further_apart_than_lines_do() {
    let mut p = Page::new();
    // Letters about 20 tall, so a size unit is about 20: words about 80
    // apart, near four units.
    let mut line = p.note(BLACK, 0.0, 0.0, 3);
    line.extend(p.note(BLACK, 160.0, 0.0, 3));
    // A gap a little wider, down the page, is a new note.
    let below = p.note(BLACK, 0.0, 110.0, 3);
    assert_eq!(p.clusters(), [line, below]);
}

#[test]
fn ink_drawn_long_after_its_neighbour_starts_a_new_cluster() {
    let mut p = Page::new();
    let mut a = p.note(BLACK, 0.0, 0.0, 3);
    p.wait(MINUTE);
    // A pause of a minute inside a note keeps it whole.
    a.extend(p.note(BLACK, 90.0, 0.0, 2));
    p.wait(30 * MINUTE);
    let later = p.note(BLACK, 0.0, 30.0, 3);
    assert_eq!(p.clusters(), [a, later]);
}

#[test]
fn colours_split_what_distance_would_join() {
    let mut p = Page::new();
    let black = p.note(BLACK, 0.0, 0.0, 3);
    let red = p.note(RED, 90.0, 0.0, 3);
    assert_eq!(p.clusters(), [black, red]);
}

#[test]
fn an_agents_stroke_is_never_clustered() {
    let mut p = Page::new();
    let mut a = p.note(BLACK, 0.0, 0.0, 2);
    let agent = Author::Agent("claude".into());
    let theirs = p.draw(&agent, BLACK, &[(60.0, 0.0), (70.0, 20.0), (80.0, 0.0)]);
    a.extend(p.note(BLACK, 90.0, 0.0, 1));
    assert_eq!(p.clusters(), [a, vec![theirs]]);
}

#[test]
fn the_same_strokes_in_any_order_give_the_same_clusters() {
    let canvas = board();
    let page = canvas.page_named("spec").unwrap().id();
    let ids: Vec<ItemId> = canvas.items(page).map(Item::id).collect();
    let forward = cluster(&canvas, ids.iter().copied());
    let mut doubled: Vec<ItemId> = ids.iter().rev().copied().collect();
    doubled.extend(ids.iter().step_by(3));
    assert_eq!(cluster(&canvas, doubled), forward);
    assert!(forward.windows(2).all(|w| w[0][0] < w[1][0]));
    assert!(forward.iter().all(|c| c.windows(2).all(|w| w[0] < w[1])));
}

#[test]
fn ids_that_are_not_strokes_are_skipped() {
    let mut p = Page::new();
    let a = p.note(BLACK, 0.0, 0.0, 2);
    let op = Op {
        by: Author::User,
        at: Time(p.clock),
        action: Action::Add {
            page: p.on,
            shape: Shape::Box { frame: Rect::new(0.0, 0.0, 50.0, 50.0), colour: BLACK },
        },
    };
    let Applied::Added(frame) = p.canvas.apply(op).unwrap() else { panic!("not added") };
    let ids = a.iter().copied().chain([frame, ItemId::new(999)]);
    assert_eq!(cluster(&p.canvas, ids), [a]);
    assert_eq!(cluster(&p.canvas, []), Vec::<Vec<ItemId>>::new());
}
