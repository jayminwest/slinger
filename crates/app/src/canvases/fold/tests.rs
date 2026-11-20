use serde_json::json;
use slinger_control::CanvasPage;

use super::*;

/// A page with a box opened at (100, 100), 400 by 200, folded to 100 by
/// 50, holding a stroke at (300, 200); and a stroke beside it.
fn page() -> Mirror {
    let items = vec![
        json!({"id": "i1", "page": "p1", "kind": "box", "color": "#0000ff",
            "x": 1000, "y": 1000, "w": 1000, "h": 500,
            "fold": {"w": 4000, "h": 2000, "items": ["i2"]}}),
        json!({"id": "i2", "page": "p1", "kind": "stroke", "color": "#000000", "width": 20,
            "pts": [3000, 2000, 100, 100]}),
        json!({"id": "i3", "page": "p1", "kind": "stroke", "color": "#000000", "width": 20,
            "pts": [9000, 9000, 100, 100]}),
    ];
    let pages = vec![json!({"id": "p1"})];
    let page = "p1".into();
    Mirror::new(&CanvasPage { canvas: "main".into(), next: 1, page, pages, items, path: None })
}

fn ids(items: &[Cow<'_, Item>]) -> Vec<u64> {
    items.iter().map(|i| i.id.0).collect()
}

fn first_point(item: &Item) -> Point {
    let Shape::Stroke { points, .. } = &item.shape else { panic!("{item:?}") };
    points[0]
}

#[test]
fn a_folded_box_draws_its_miniature_then_its_mark_right_after_it() {
    let (m, theme) = (page(), Theme::default());
    let items: Vec<Cow<'_, Item>> = m.items().iter().map(Cow::Borrowed).collect();
    let out = drawn(items, &m, (None, None), &theme);
    assert_eq!(ids(&out), [1, 2, MARK + 1, 3]);
    assert_eq!(first_point(&out[1]), Point { x: 150.0, y: 125.0 });
    let Shape::Stroke { points, .. } = &out[2].shape else { panic!() };
    assert_eq!(points[2], Point { x: 200.0, y: 112.0 }, "the top right corner turned down");
}

#[test]
fn a_box_dragged_draws_its_miniature_where_it_shows() {
    let (m, theme) = (page(), Theme::default());
    let mut items: Vec<Cow<'_, Item>> = m.items().iter().map(Cow::Borrowed).collect();
    let moved = items[0].shape.moved(Point { x: 50.0, y: 0.0 });
    items[0] = Cow::Owned(Item { shape: moved, ..(*items[0]).clone() });
    let out = drawn(items, &m, (None, None), &theme);
    assert_eq!(first_point(&out[1]), Point { x: 200.0, y: 125.0 });
}

#[test]
fn a_peek_draws_paper_over_the_page_and_what_the_box_holds_at_full_size() {
    let (m, theme) = (page(), Theme::default());
    let items: Vec<Cow<'_, Item>> = m.items().iter().map(Cow::Borrowed).collect();
    let out = drawn(items, &m, (None, Some(ItemId(1))), &theme);
    assert_eq!(ids(&out), [1, MARK + 1, 3, PEEK.0, 2], "no miniature under the peek");
    let room = Rect { x: 100.0, y: 100.0, width: 400.0, height: 200.0 };
    let Shape::Panel { frame, fill: Some(fill), .. } = out[3].shape else { panic!() };
    assert_eq!((frame, fill.colour, fill.alpha), (room, theme.background, 1.0));
    assert_eq!(first_point(&out[4]), Point { x: 300.0, y: 200.0 });
}

#[test]
fn an_open_box_selected_alone_shows_its_mark_and_a_press_on_it_is_the_mark_s_off_the_corner() {
    let open = json!({"id": "i4", "page": "p1", "kind": "box", "color": "#000000",
        "x": 0, "y": 0, "w": 1000, "h": 1000});
    let pages = vec![json!({"id": "p1"})];
    let shown = CanvasPage {
        canvas: "main".into(),
        next: 1,
        page: "p1".into(),
        pages,
        items: vec![open],
        path: None,
    };
    let (m, theme) = (Mirror::new(&shown), Theme::default());
    let items: Vec<Cow<'_, Item>> = m.items().iter().map(Cow::Borrowed).collect();
    assert_eq!(ids(&drawn(items.clone(), &m, (None, None), &theme)), [4]);
    assert_eq!(ids(&drawn(items, &m, (Some(ItemId(4)), None), &theme)), [4, MARK + 4]);
    // A box 100 square: its mark is the 12 units square at its top right.
    let frame = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
    let on = |x, y, zoom| on_mark(frame, Point { x, y }, zoom);
    assert!(on(94.0, 6.0, 1.0) && on(88.0, 12.0, 1.0));
    assert!(!on(99.0, 1.0, 1.0), "at the corner: the selection's handle");
    assert!(on(99.0, 1.0, 4.0), "zoomed in, the handle is smaller");
    assert!(!on(80.0, 6.0, 1.0) && !on(94.0, 20.0, 1.0));
}

#[test]
fn a_press_is_inside_a_room_up_to_its_edges() {
    let room = Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
    assert!(contains(room, Point { x: 10.0, y: 0.0 }));
    assert!(!contains(room, Point { x: 10.1, y: 5.0 }));
}
