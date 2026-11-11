use serde_json::json;

use super::*;

fn shown(items: Vec<Value>) -> CanvasPage {
    CanvasPage {
        canvas: "main".into(),
        next: 5,
        page: "p1".into(),
        pages: vec![json!({"id": "p1", "name": "one"}), json!({"id": "p2", "name": "two"})],
        items,
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
    let frame = Rect { x: 0.0, y: 0.0, width: 200.0, height: 30.0 };
    assert_eq!(items[2].shape, Shape::Text { frame, text: "plan".into(), size: TEXT_SIZE });
    assert_eq!(items[2].colour, Rgb { r: 0, g: 0, b: 255 });
    let arrow = Shape::Arrow { from: pt(0.0, 0.0), to: pt(10.0, 10.0), width: LINE_WIDTH };
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
    let expected = Rect {
        x: 5.0,
        y: 6.0,
        width: 4.0 * TEXT_SIZE * CHAR_WIDTH,
        height: 2.0 * TEXT_SIZE * LINE_HEIGHT,
    };
    assert_eq!(*frame, expected);
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
fn pages_are_followed_and_the_next_one_wraps_around() {
    let mut m = Mirror::new(&shown(vec![]));
    assert_eq!(m.next_page(), "p2");
    m.apply(&feed(6, vec![json!({"op": "page.add", "page": "p3", "name": "three"})]));
    m.apply(&feed(7, vec![json!({"op": "page.remove", "page": "p2"})]));
    assert_eq!(m.next_page(), "p3");
    let restore = json!({"op": "page.restore", "page": "p2", "name": "two", "index": 1,
                         "items": [stroke("i4", "p2")]});
    assert_eq!(m.apply(&feed(8, vec![restore])), Applied::Same);
    assert_eq!(m.next_page(), "p2");
    m.apply(&feed(
        10,
        vec![
            json!({"op": "page.remove", "page": "p2"}),
            json!({"op": "page.remove", "page": "p3"}),
        ],
    ));
    assert_eq!(m.next_page(), "p1");
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
