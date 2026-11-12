//! The tldraw importer, through its public interface: bytes in, canvas out.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::{Author, Canvas, Colour, End, Item, Point, Rect, Shape, Time};
use slinger_core::tldraw::{Import, import};

const AT: Time = Time(1_700_000_000_000);

/// The owner's real board (SPEC B8), committed whole with the owner's approval.
const BOARD: &[u8] = include_bytes!("fixtures/idea.tldraw");

fn items_of(c: &Canvas) -> Vec<&Item> {
    c.pages().flat_map(|p| c.items(p.id())).collect()
}

#[test]
fn the_real_board_imports_with_every_page_and_stroke() {
    let Import { canvas, report } = import(BOARD, AT).unwrap();

    // SPEC's Spike B result says 7562 strokes; this file holds 7563 draw
    // records (count them: `SELECT count(*) FROM documents WHERE
    // json_extract(state, '$.type') = 'draw'` on its db.sqlite), and every one
    // decodes, so the importer makes 7563. The board was still being saved on
    // 2026-11-10 after the spike ran (its document clock reached 68954), so the
    // spike most likely read a copy one stroke older. The committed fixture is
    // the reference from here on.
    assert_eq!(report.pages, 5);
    assert_eq!(report.strokes, 7563);
    let names: Vec<_> = canvas.pages().map(|p| p.name().to_string()).collect();
    assert_eq!(names, ["inital-planning", "claude", "slinger-diagrams", "fable-5.2", "spec"]);

    // 52 text shapes plus 73 rectangle labels; 80 rectangles; 57 arrows.
    assert_eq!((report.texts, report.boxes, report.arrows), (125, 80, 57));
    let skipped: Vec<_> = report.skipped.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(skipped, [("geo:diamond", 1), ("geo:ellipse", 2), ("group", 8), ("line", 4)]);

    let items = items_of(&canvas);
    assert_eq!(items.len(), 7563 + 125 + 80 + 57);
    assert!(items.iter().all(|i| i.author() == &Author::User && i.time() == AT));
    // Of 114 arrow bindings, 91 point at rectangles; the rest point at
    // diamonds, ellipses and lines, which have no item.
    let bound: usize = items
        .iter()
        .map(|i| match i.shape() {
            Shape::Arrow { from, to, .. } => {
                usize::from(from.item.is_some()) + usize::from(to.item.is_some())
            }
            _ => 0,
        })
        .sum();
    assert_eq!(bound, 91);
}

#[test]
fn bytes_that_are_not_a_tldraw_file_are_a_typed_invalid_error() {
    for bytes in [&b"not a board"[..], b"{}", &BOARD[..4096]] {
        let err = import(bytes, AT).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().is_some());
    }
}

/// A `.tldr` file (tldraw's older JSON format) holding these records.
fn tldr(records: &[Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({"tldrawFileFormatVersion": 1, "records": records})).unwrap()
}

fn page(id: &str, name: &str, index: &str) -> Value {
    json!({"typeName": "page", "id": id, "name": name, "index": index})
}

fn shape(id: &str, parent: &str, index: &str, kind: &str, x: f64, y: f64, props: &Value) -> Value {
    json!({"typeName": "shape", "id": id, "parentId": parent, "index": index, "type": kind,
           "x": x, "y": y, "rotation": 0, "props": props})
}

fn doc(lines: &[&str]) -> Value {
    let paragraphs: Vec<_> = lines
        .iter()
        .map(|l| json!({"type": "paragraph", "content": [{"type": "text", "text": l}]}))
        .collect();
    json!({"type": "doc", "content": paragraphs})
}

#[test]
fn pages_keep_their_order_and_names_unique() {
    let file = tldr(&[
        page("page:b", "Second", "a2"),
        page("page:a", "Ideas", "a1"),
        page("page:c", " ", "a3"),
        page("page:d", "Ideas", "a4"),
    ]);
    let Import { canvas, report } = import(&file, AT).unwrap();
    let names: Vec<_> = canvas.pages().map(|p| p.name().to_string()).collect();
    assert_eq!(names, ["Ideas", "Second", "Page 3", "Ideas 2"]);
    assert_eq!(report.pages, 4);
}

#[test]
fn shapes_map_to_items_in_drawing_order_with_colour() {
    let pen = json!({"segments": [{"type": "free", "points": [{"x": 0, "y": 0, "z": 0.5}, {"x": 10, "y": 5}]}],
                     "color": "red", "size": "xl", "scale": 2, "scaleX": 2, "scaleY": 1});
    let file = tldr(&[
        page("page:p", "P", "a1"),
        // Listed out of order: `index` decides the drawing order.
        shape(
            "shape:t",
            "page:p",
            "a3",
            "text",
            0.0,
            0.0,
            &json!({"richText": doc(&["one", "two"]), "w": 200, "size": "m", "color": "blue"}),
        ),
        shape("shape:d", "page:p", "a1", "draw", 100.0, 50.0, &pen),
        shape(
            "shape:r",
            "page:p",
            "a2",
            "geo",
            5.0,
            6.0,
            &json!({"geo": "rectangle", "w": 40, "h": 20, "growY": 4, "color": "green", "labelColor": "black", "richText": doc(&["label"])}),
        ),
        shape(
            "shape:o",
            "page:p",
            "a4",
            "text",
            0.0,
            0.0,
            &json!({"text": "old style", "w": 0, "color": "nope"}),
        ),
    ]);
    let Import { canvas, report } = import(&file, AT).unwrap();
    assert_eq!((report.strokes, report.boxes, report.texts), (1, 1, 3));
    let shapes: Vec<_> = items_of(&canvas).into_iter().map(|i| i.shape().clone()).collect();
    let frame = Rect::new(5.0, 6.0, 40.0, 24.0);
    let black = Colour::rgb(30, 30, 30);
    let stroke = Shape::Stroke {
        points: vec![Point::new(100.0, 50.0), Point::new(120.0, 55.0)],
        colour: Colour::rgb(215, 40, 50),
        width: 20.0,
    };
    let label = Shape::Text { text: "label".into(), frame, colour: black, size: 24.0 };
    assert_eq!(
        shapes[..3],
        [stroke, Shape::Box { frame, colour: Colour::rgb(40, 140, 90) }, label]
    );
    let texts: Vec<_> = shapes[3..]
        .iter()
        .filter_map(|s| match s {
            Shape::Text { text, frame, colour, .. } => Some((text.as_str(), frame.w, *colour)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [("one\ntwo", 200.0, Colour::rgb(60, 100, 220)), ("old style", 9.0 * 24.0 * 0.55, black)]
    );
}

#[test]
fn arrows_point_at_the_items_they_are_bound_to() {
    let arrow = json!({"start": {"x": 0, "y": 0}, "end": {"x": 50, "y": 0}, "color": "black"});
    let binding = |id: &str, to: &str, terminal: &str| {
        json!({"typeName": "binding", "type": "arrow", "id": id, "fromId": "shape:a", "toId": to,
               "props": {"terminal": terminal, "normalizedAnchor": {"x": 0.5, "y": 0.5}}})
    };
    let file = tldr(&[
        page("page:p", "P", "a1"),
        // The arrow draws before the box it points at.
        shape("shape:a", "page:p", "a1", "arrow", 10.0, 10.0, &arrow),
        shape(
            "shape:r",
            "page:p",
            "a2",
            "geo",
            100.0,
            0.0,
            &json!({"geo": "rectangle", "w": 20, "h": 40}),
        ),
        shape("shape:l", "page:p", "a3", "line", 0.0, 0.0, &json!({})),
        binding("binding:1", "shape:r", "end"),
        binding("binding:2", "shape:l", "start"),
    ]);
    let Import { canvas, report } = import(&file, AT).unwrap();
    let items = items_of(&canvas);
    assert_eq!(report.arrows, 1);
    assert_eq!(
        items[0].shape(),
        &Shape::Arrow {
            from: End::free(Point::new(10.0, 10.0)),
            to: End::on(Point::new(110.0, 20.0), items[1].id()),
            colour: Colour::rgb(30, 30, 30),
        }
    );
}

#[test]
fn group_children_land_in_page_space_and_unmapped_shapes_are_counted() {
    let quarter = std::f64::consts::FRAC_PI_2;
    let mut group = shape("shape:g", "page:p", "a1", "group", 100.0, 100.0, &json!({}));
    group["rotation"] = json!(quarter);
    let pen = json!({"segments": [{"type": "free", "points": [{"x": 10, "y": 0}]}]});
    let file = tldr(&[
        page("page:p", "P", "a1"),
        group,
        shape("shape:d", "shape:g", "a1", "draw", 10.0, 0.0, &pen),
        shape(
            "shape:e",
            "page:p",
            "a2",
            "geo",
            0.0,
            0.0,
            &json!({"geo": "ellipse", "w": 1, "h": 1}),
        ),
        shape(
            "shape:bad",
            "page:p",
            "a3",
            "draw",
            0.0,
            0.0,
            &json!({"segments": [{"path": "!!"}]}),
        ),
        shape("shape:lost", "page:gone", "a1", "draw", 0.0, 0.0, &pen),
        shape("shape:n", "page:p", "a4", "note", 0.0, 0.0, &json!({})),
    ]);
    let Import { canvas, report } = import(&file, AT).unwrap();
    let items = items_of(&canvas);
    let Shape::Stroke { points, .. } = items[0].shape() else { panic!("expected a stroke") };
    // (10, 0) then (10, 0) in the child: 20 along the group's x, which points down the page.
    assert!((points[0].x - 100.0).abs() < 1e-9 && (points[0].y - 120.0).abs() < 1e-9);
    assert_eq!(items.len(), 1);
    let skipped: Vec<_> = report.skipped.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(
        skipped,
        [("draw (damaged)", 1), ("geo:ellipse", 1), ("group", 1), ("note", 1), ("orphan", 1)]
    );
}
