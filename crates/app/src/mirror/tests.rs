use std::borrow::Cow;

use serde_json::json;

use super::write::stroke_object;
use super::*;
use crate::scene::ink::Ink;
use crate::scene::ink::{Change, Stroke};
use crate::scene::select::Select;
use crate::scene::tools::Tool;

fn shown(items: Vec<Value>) -> CanvasPage {
    CanvasPage {
        canvas: "main".into(),
        next: 5,
        page: "p1".into(),
        pages: vec![json!({"id": "p1", "name": "one"}), json!({"id": "p2", "name": "two"})],
        items,
        path: None,
    }
}

fn stroke(id: &str, page: &str) -> Value {
    json!({"id": id, "page": page, "by": "user", "kind": "stroke", "color": "#ff0000",
           "width": 20, "pts": [100, 200, 10, -20, 5, 5]})
}

fn feed(next: u64, ops: Vec<Value>) -> CanvasFeed {
    CanvasFeed { next, ops }
}

fn ids(m: &Mirror) -> Vec<u64> {
    m.items().iter().map(|i| i.id.0).collect()
}

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn a_shown_page_reads_every_kind_in_units() {
    let m = Mirror::new(&shown(vec![
        stroke("i1", "p1"),
        json!({"id": "i2", "page": "p1", "kind": "box", "color": "#00ff00",
               "x": 10, "y": 20, "w": 300, "h": 400}),
        json!({"id": "i3", "page": "p1", "kind": "text", "color": "#0000ffcc",
               "x": 0, "y": 0, "w": 2000, "h": 300, "text": "plan"}),
        json!({"id": "i4", "page": "p1", "kind": "arrow", "color": "blue",
               "pts": [0, 0, 100, 0, 0, 100], "from": "i2"}),
    ]));
    let items = m.items();
    assert_eq!(
        items[0],
        Item {
            id: ItemId(1),
            colour: Rgb { r: 255, g: 0, b: 0 },
            shape: Shape::Stroke {
                points: vec![pt(10.0, 20.0), pt(11.0, 18.0), pt(11.5, 18.5)],
                width: 2.0
            },
        }
    );
    let frame = Rect { x: 1.0, y: 2.0, width: 30.0, height: 40.0 };
    assert_eq!(items[1].shape, Shape::Box { frame, width: LINE_WIDTH });
    assert_eq!(items[1].colour, Rgb { r: 0, g: 255, b: 0 });
    // Drawn and hit in its glyphs' frame, wrapped to its stored 200.
    let frame = glyphs::frame(pt(0.0, 0.0), "plan", TEXT_SIZE, 200.0);
    assert_eq!(items[2].shape, Shape::Text { frame, text: "plan".into(), size: TEXT_SIZE });
    assert_eq!(items[2].colour, Rgb { r: 0, g: 0, b: 255 });
    let arrow = Shape::Arrow {
        from: pt(0.0, 0.0),
        to: pt(10.0, 10.0),
        width: LINE_WIDTH,
        bound: [Some(ItemId(2)), None],
    };
    assert_eq!((&items[3].shape, items[3].colour), (&arrow, Rgb { r: 0, g: 0, b: 0 }));
}

#[test]
fn what_cannot_be_drawn_is_left_out() {
    let m = Mirror::new(&shown(vec![
        json!({"id": "i1", "page": "p1", "kind": "ellipse", "x": 0, "y": 0}),
        json!({"id": "i2", "page": "p1", "kind": "stroke", "width": 10, "pts": []}),
        json!({"id": "i3", "page": "p1", "kind": "stroke", "width": 10, "pts": [1, 2, 3]}),
        json!({"id": "i4", "page": "p1", "kind": "arrow", "pts": [1, 2]}),
        json!({"id": "x5", "page": "p1", "kind": "box", "x": 0, "y": 0}),
        json!({"id": "i6", "page": "p1", "kind": "box"}),
        json!("not an item"),
        stroke("i7", "p2"),
        stroke("i8", "p1"),
    ]));
    assert_eq!(ids(&m), [8]);
}

#[test]
fn text_too_narrow_to_wrap_gets_a_frame_its_size() {
    let m = Mirror::new(&shown(vec![json!({"id": "i1", "page": "p1", "kind": "text",
        "x": 50, "y": 60, "w": 80, "h": 99999, "text": "abcd\nab"})]));
    let Shape::Text { frame, .. } = &m.items()[0].shape else { panic!() };
    let expected = glyphs::frame(pt(5.0, 6.0), "abcd\nab", TEXT_SIZE, glyphs::UNWRAPPED);
    assert_eq!(*frame, expected);
}

#[test]
fn a_text_is_hit_where_its_glyphs_are_not_where_its_stored_frame_reaches() {
    // slinger-48e246: a text stored 24 a character (here 96 wide) is drawn
    // about half that. An eraser along an arrow at x 80, past the glyphs but
    // inside the stored width, takes the arrow and not the text.
    let m = Mirror::new(&shown(vec![
        json!({"id": "i1", "page": "p1", "kind": "text", "x": 0, "y": 0, "w": 960, "h": 312,
               "text": "note"}),
        json!({"id": "i2", "page": "p1", "kind": "arrow", "pts": [800, -400, 0, 1200]}),
    ]));
    let Shape::Text { frame, .. } = &m.items()[0].shape else { panic!() };
    assert!(frame.width < 60.0, "the glyphs' width, not the stored 96: {frame:?}");
    let items: Vec<Cow<'_, Item>> = m.items().iter().map(Cow::Borrowed).collect();
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.down(Tool::Eraser, ("main", "p1"), pt(80.0, -40.0), 1.0, &items));
    assert!(select.drag(pt(80.0, 80.0), &items));
    assert!(select.up(None, &items, &mut ink));
    let erased = ink.next().map(|(_, c)| c);
    assert!(matches!(erased, Some(Change::Remove { ids, .. }) if ids == [ItemId(2)]));
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.down(Tool::Eraser, ("main", "p1"), pt(20.0, 10.0), 1.0, &items));
    assert!(select.up(None, &items, &mut ink), "a dab on the glyphs");
    let erased = ink.next().map(|(_, c)| c);
    assert!(matches!(erased, Some(Change::Remove { ids, .. }) if ids == [ItemId(1)]));
}

#[test]
fn adds_and_restores_on_this_page_go_in_id_order() {
    let mut m = Mirror::new(&shown(vec![stroke("i2", "p1"), stroke("i9", "p1")]));
    let ops = vec![
        json!({"n": 5, "op": "add", "item": stroke("i10", "p1")}),
        json!({"n": 6, "op": "add", "item": stroke("i11", "p2")}),
        json!({"n": 7, "op": "restore", "item": stroke("i3", "p1")}),
    ];
    assert_eq!(m.apply(&feed(8, ops)), Applied::Changed);
    assert_eq!(ids(&m), [2, 3, 9, 10]);
}

#[test]
fn removes_moves_and_sets_change_what_is_drawn() {
    let boxed =
        json!({"id": "i3", "page": "p1", "kind": "box", "x": 10, "y": 10, "w": 10, "h": 10});
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1"), stroke("i2", "p1"), boxed]));
    let ops = vec![
        json!({"op": "remove", "ids": ["i2", "i99"]}),
        json!({"op": "move", "ids": ["i1", "i3"], "dx": 50, "dy": -100}),
        json!({"op": "set", "id": "i3", "fields": {"w": 100, "color": "#123456"}}),
    ];
    assert_eq!(m.apply(&feed(8, ops)), Applied::Changed);
    assert_eq!(ids(&m), [1, 3]);
    let Shape::Stroke { points, .. } = &m.items()[0].shape else { panic!() };
    assert_eq!(points[0], pt(15.0, 10.0));
    assert_eq!(points[1], pt(16.0, 8.0));
    let frame = Rect { x: 6.0, y: -9.0, width: 10.0, height: 1.0 };
    assert_eq!(m.items()[1].shape, Shape::Box { frame, width: LINE_WIDTH });
    assert_eq!(m.items()[1].colour, Rgb { r: 0x12, g: 0x34, b: 0x56 });
}

#[test]
fn a_set_that_clears_a_needed_field_drops_the_item() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1")]));
    let set = json!({"op": "set", "id": "i1", "fields": {"pts": null}});
    assert_eq!(m.apply(&feed(6, vec![set])), Applied::Changed);
    assert_eq!(ids(&m), Vec::<u64>::new());
    let bad = json!({"op": "set", "id": "i1"});
    assert_eq!(m.apply(&feed(7, vec![bad])), Applied::Same);
}

#[test]
fn moves_keep_fractions() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1")]));
    m.apply(&feed(6, vec![json!({"op": "move", "ids": ["i1"], "dx": 0.5})]));
    let Shape::Stroke { points, .. } = &m.items()[0].shape else { panic!() };
    assert_eq!(points[0], pt(100.5 / 10.0, 20.0));
}

#[test]
fn changes_elsewhere_change_nothing_drawn() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1")]));
    let ops = vec![
        json!({"op": "add", "item": stroke("i5", "p2")}),
        json!({"op": "remove", "ids": ["i5"]}),
        json!({"op": "page.rename", "page": "p2", "name": "x"}),
        json!({"op": "page.move", "page": "p2", "index": 0}),
        json!({"op": "something.new"}),
        json!({"no": "op"}),
    ];
    assert_eq!(m.apply(&feed(11, ops)), Applied::Same);
    assert_eq!(m.apply(&feed(11, vec![])), Applied::Same);
}

#[test]
fn pages_are_followed_with_their_names() {
    let mut m = Mirror::new(&shown(vec![]));
    let names =
        |m: &Mirror| m.pages().iter().map(|p| format!("{}={}", p.id, p.name)).collect::<Vec<_>>();
    assert_eq!(names(&m), ["p1=one", "p2=two"]);
    m.apply(&feed(6, vec![json!({"op": "page.add", "page": "p3", "name": "three"})]));
    m.apply(&feed(7, vec![json!({"op": "page.remove", "page": "p2"})]));
    assert_eq!(names(&m), ["p1=one", "p3=three"]);
    let restore = json!({"op": "page.restore", "page": "p2", "name": "two", "index": 1,
                         "items": [stroke("i4", "p2")]});
    assert_eq!(m.apply(&feed(8, vec![restore])), Applied::Same);
    let rename = json!({"op": "page.rename", "page": "p3", "name": "spec"});
    assert_eq!(m.apply(&feed(9, vec![rename])), Applied::Same, "nothing drawn changed");
    assert_eq!(names(&m), ["p1=one", "p2=two", "p3=spec"]);
    m.apply(&feed(
        10,
        vec![
            json!({"op": "page.remove", "page": "p2"}),
            json!({"op": "page.remove", "page": "p3"}),
            json!({"op": "page.rename", "page": "p9", "name": "lost"}),
            json!({"op": "page.add", "page": "p4"}),
        ],
    ));
    assert_eq!(names(&m), ["p1=one", "p4=p4"], "a page with no name is named by its id");
}

#[test]
fn page_changes_and_steps_are_written_as_their_operations() {
    use crate::scene::pages::PageOp;
    let page = |op| ops(&Change::Page { canvas: "main".into(), op });
    assert_eq!(
        page(PageOp::Add { name: "Page 2".into() }),
        json!({"op": "page.add", "name": "Page 2"})
    );
    assert_eq!(
        page(PageOp::Rename { page: "p2".into(), name: "spec".into() }),
        json!({"op": "page.rename", "page": "p2", "name": "spec"})
    );
    assert_eq!(
        page(PageOp::Remove { page: "p2".into() }),
        json!({"op": "page.remove", "page": "p2"})
    );
    assert_eq!(ops(&Change::Step { canvas: "main".into(), redo: true }), json!([]));
}

#[test]
fn removing_the_page_shown_is_gone() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1")]));
    let ops =
        vec![json!({"op": "page.remove", "page": "p1"}), json!({"op": "page.add", "page": "p3"})];
    assert_eq!(m.apply(&feed(7, ops)), Applied::Gone);
}

#[test]
fn colours_read_as_rgb_or_not_at_all() {
    assert_eq!(colour("#0a0B0c"), Some(Rgb { r: 10, g: 11, b: 12 }));
    assert_eq!(colour("#0a0b0c80"), Some(Rgb { r: 10, g: 11, b: 12 }));
    for bad in ["0a0b0c", "#0a0b0", "#0a0b0g", "black", "#ééé"] {
        assert_eq!(colour(bad), None, "{bad}");
    }
}

#[test]
fn a_pen_stroke_is_written_as_it_reads_back() {
    let stroke = Stroke {
        canvas: "main".into(),
        page: "p1".into(),
        colour: Rgb { r: 255, g: 0, b: 0x7f },
        width: 2.0,
        points: vec![pt(10.04, 20.0), pt(11.06, 18.0), pt(11.5, 18.5)],
    };
    let object = stroke_object(&stroke);
    assert_eq!(
        object,
        json!({"page": "p1", "kind": "stroke", "color": "#ff007f", "width": 20,
               "pts": [100, 200, 11, -20, 4, 5]})
    );
    let mut added = object;
    added["id"] = json!("i1");
    let m = Mirror::new(&shown(vec![added]));
    let Shape::Stroke { points, width } = &m.items()[0].shape else { panic!() };
    assert_eq!(
        (points.as_slice(), *width),
        ([pt(10.0, 20.0), pt(11.1, 18.0), pt(11.5, 18.5)].as_slice(), 2.0)
    );
    let thin = Stroke { width: 0.01, points: vec![pt(-0.05, 0.0)], ..stroke };
    assert_eq!(stroke_object(&thin)["width"], 1);
    assert_eq!(stroke_object(&thin)["pts"], json!([-1, 0]));
}

#[test]
fn a_move_and_a_removal_are_written_as_the_feed_brings_them_back() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1"), stroke("i2", "p1")]));
    let here = || ("main".to_string(), "p1".to_string());
    let (canvas, page) = here();
    let by = pt(1.04, -0.5);
    let moved = ops(&Change::Move { canvas, page, ids: vec![ItemId(1)], by, follow: vec![] });
    assert_eq!(moved, json!({"op": "move", "ids": ["i1"], "dx": 10, "dy": -5}));
    let (canvas, page) = here();
    let removed = ops(&Change::Remove { canvas, page, ids: vec![ItemId(2), ItemId(9)] });
    assert_eq!(removed, json!({"op": "remove", "ids": ["i2", "i9"]}));
    assert_eq!(m.apply(&feed(7, vec![moved, removed])), Applied::Changed);
    assert_eq!(ids(&m), [1]);
    let Shape::Stroke { points, .. } = &m.items()[0].shape else { panic!() };
    assert_eq!(points[0], pt(11.0, 19.5));
}

#[test]
fn a_text_a_box_and_an_arrow_are_written_as_they_read_back() {
    let put = |shape: Shape| Change::Put {
        canvas: "main".into(),
        page: "p1".into(),
        colour: Rgb { r: 0, g: 0x80, b: 0xff },
        shape,
    };
    // A text shows in its glyphs' frame; the file keeps the core's cells.
    let glyphs = glyphs::frame(pt(1.0, -2.5), "hi", TEXT_SIZE, glyphs::UNWRAPPED);
    let text = Shape::Text { frame: glyphs, text: "hi".into(), size: TEXT_SIZE };
    let boxed = Shape::Box {
        frame: Rect { x: 0.0, y: 0.0, width: 30.0, height: 40.04 },
        width: LINE_WIDTH,
    };
    let bound = [Some(ItemId(7)), None];
    let arrow = Shape::Arrow { from: pt(1.0, 2.0), to: pt(-3.0, 4.5), width: LINE_WIDTH, bound };
    let written: Vec<Value> = [&text, &boxed, &arrow].map(|s| ops(&put(s.clone()))).into();
    assert_eq!(
        written,
        [
            json!({"op": "add", "item": {"page": "p1", "kind": "text", "color": "#0080ff",
                   "x": 10, "y": -25, "w": 480, "h": 312, "text": "hi"}}),
            json!({"op": "add", "item": {"page": "p1", "kind": "box", "color": "#0080ff",
                   "x": 0, "y": 0, "w": 300, "h": 400}}),
            json!({"op": "add", "item": {"page": "p1", "kind": "arrow", "color": "#0080ff",
                   "pts": [10, 20, -40, 25], "from": "i7"}}),
        ]
    );
    let read: Vec<Value> = written
        .into_iter()
        .zip(1..)
        .map(|(mut op, n)| {
            op["item"]["id"] = json!(format!("i{n}"));
            op["item"].take()
        })
        .collect();
    let m = Mirror::new(&shown(read));
    let boxed =
        Shape::Box { frame: Rect { x: 0.0, y: 0.0, width: 30.0, height: 40.0 }, width: LINE_WIDTH };
    let shapes: Vec<&Shape> = m.items().iter().map(|i| &i.shape).collect();
    assert_eq!(shapes, [&text, &boxed, &arrow]);
}

#[test]
fn an_edit_is_a_set_and_a_move_sets_each_arrow_that_follows() {
    let (canvas, page) = ("main".to_string(), "p1".to_string());
    let frame = Rect { x: 0.0, y: 0.0, width: 24.0, height: 31.2 };
    let shape = Shape::Text { frame, text: "a".into(), size: TEXT_SIZE };
    let edit = Change::Set { canvas: canvas.clone(), page: page.clone(), id: ItemId(3), shape };
    assert_eq!(
        ops(&edit),
        json!({"op": "set", "id": "i3",
               "fields": {"x": 0, "y": 0, "w": 240, "h": 312, "text": "a"}})
    );
    let shape = Shape::Arrow {
        from: pt(1.0, 1.0),
        to: pt(2.0, 2.0),
        width: 2.0,
        bound: [None, Some(ItemId(1))],
    };
    let follow = vec![Item { id: ItemId(4), colour: Rgb { r: 0, g: 0, b: 0 }, shape }];
    let by = pt(1.0, 0.0);
    let moved = Change::Move { canvas, page, ids: vec![ItemId(1)], by, follow };
    assert_eq!(
        ops(&moved),
        json!([
            {"op": "move", "ids": ["i1"], "dx": 10, "dy": 0},
            {"op": "set", "id": "i4", "fields": {"pts": [10, 10, 10, 10], "from": null, "to": "i1"}},
        ])
    );

    // The feed brings them back: the arrow keeps its binding, and the end moves.
    let mut m = Mirror::new(&shown(vec![
        json!({"id": "i1", "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10}),
        json!({"id": "i4", "page": "p1", "kind": "arrow", "pts": [10, 10, 0, 0], "to": "i1"}),
    ]));
    let Value::Array(both) = ops(&moved) else { panic!() };
    assert_eq!(m.apply(&feed(9, both)), Applied::Changed);
    let Shape::Arrow { from, to, bound, .. } = &m.items()[1].shape else { panic!() };
    assert_eq!((*from, *to, *bound), (pt(1.0, 1.0), pt(2.0, 2.0), [None, Some(ItemId(1))]));
}

#[test]
fn carried_items_are_add_objects_on_the_other_page_moved_and_without_this_canvas_s_names() {
    let m = Mirror::new(&shown(vec![
        json!({"id": "i2", "page": "p1", "by": "agent:scout", "t": "2026-11-10T00:00:00Z",
               "kind": "arrow", "color": "#0000ff", "pts": [0, 0, 100, 0], "from": "i1", "to": "i9"}),
        stroke("i1", "p1"),
        json!({"id": "i3", "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10}),
        json!({"id": "i4", "page": "p1", "kind": "arrow", "pts": [0, 0, 5, 5], "to": "i3"}),
    ]));
    let carried = m.carried(&[ItemId(2), ItemId(1), ItemId(4), ItemId(7)], "p4", pt(1.0, -0.5));
    assert_eq!(carried.len(), 3, "i7 is not here");
    let order: Vec<_> = carried.iter().map(|c| c.item.id).collect();
    assert_eq!(order, [ItemId(1), ItemId(4), ItemId(2)], "an arrow after the items it points at");
    assert_eq!(
        carried[0].object,
        json!({"page": "p4", "kind": "stroke", "color": "#ff0000", "width": 20,
               "pts": [110, 195, 10, -20, 5, 5]})
    );
    assert_eq!(
        carried[2].object,
        json!({"page": "p4", "kind": "arrow", "color": "#0000ff", "pts": [10, -5, 100, 0],
               "from": "$0"}),
        "bound to the stroke's add; its end on an item not carried let go"
    );
    assert_eq!(carried[1].object.get("to"), None, "i3 stays here");
    let Shape::Arrow { from, to, bound, .. } = carried[2].item.shape else { panic!() };
    assert_eq!((from, to), (pt(1.0, -0.5), pt(11.0, -0.5)), "drawn where it will land");
    assert_eq!(bound, [None, None], "drawn bound to nothing until the server says");

    let change =
        Change::Carry { canvas: "board".into(), page: "p4".into(), items: carried, gone: vec![] };
    let written = ops(&change);
    assert_eq!(written.as_array().map(Vec::len), Some(3), "one canvas_apply, one add each");
    assert_eq!(written[2]["op"], "add");
    assert_eq!(written[2]["item"]["from"], "$0");
}

#[test]
fn a_resize_sets_each_item_a_stroke_by_its_points_and_an_agent_ui_by_its_place() {
    let mut m = Mirror::new(&shown(vec![stroke("i1", "p1")]));
    let red = Rgb { r: 255, g: 0, b: 0 };
    let scaled = Item {
        id: ItemId(1),
        colour: red,
        shape: Shape::Stroke { points: vec![pt(10.0, 20.0), pt(12.0, 16.0)], width: 2.0 },
    };
    let ui = Item {
        id: ItemId(2),
        colour: red,
        shape: Shape::Live {
            frame: Rect { x: 1.5, y: 2.0, width: 30.0, height: 20.0 },
            name: "pick".into(),
            open: "pick-slot#1".into(),
        },
    };
    let boxed = Item {
        id: ItemId(3),
        colour: red,
        shape: Shape::Box { frame: Rect { x: 0.0, y: 0.0, width: 4.0, height: 2.0 }, width: 2.0 },
    };
    let (canvas, page) = ("main".to_string(), "p1".to_string());
    let written = ops(&Change::Reshape { canvas, page, items: vec![scaled.clone(), ui, boxed] });
    assert_eq!(
        written,
        json!([
            {"op": "set", "id": "i1", "fields": {"pts": [100, 200, 20, -40]}},
            {"op": "set", "id": "i2", "fields": {"x": 15, "y": 20}},
            {"op": "set", "id": "i3", "fields": {"x": 0, "y": 0, "w": 40, "h": 20}},
        ]),
        "an agent UI keeps its size: only an agent sizes it"
    );
    let ops = written.as_array().unwrap()[..1].to_vec();
    assert_eq!(m.apply(&feed(7, ops)), Applied::Changed);
    assert_eq!(m.items()[0].shape, scaled.shape, "the feed brings the stroke back as shown");
}
