use serde_json::json;
use slinger_control::CanvasPage;

use super::*;

fn mirror(items: Vec<Value>) -> Mirror {
    Mirror::new(&CanvasPage {
        canvas: "main".into(),
        next: 5,
        page: "p1".into(),
        pages: vec![json!({"id": "p1", "name": "one"})],
        items,
        path: None,
    })
}

fn page() -> Mirror {
    mirror(vec![
        json!({"id": "i1", "page": "p1", "by": "user", "kind": "box", "color": "#ff0000",
               "x": 0, "y": 0, "w": 100, "h": 40}),
        json!({"id": "i2", "page": "p1", "by": "user", "kind": "text", "color": "#000000",
               "x": 0, "y": 500, "text": "first"}),
        json!({"id": "i3", "page": "p1", "by": "agent:scout", "kind": "arrow", "color": "#0000ff",
               "pts": [0, 0, 100, 0], "to": "i1"}),
        json!({"id": "i4", "page": "p1", "by": "user", "kind": "text", "color": "#000000",
               "x": 0, "y": 900, "text": "second"}),
    ])
}

#[test]
fn a_copy_holds_the_items_objects_and_the_texts_one_a_line() {
    let clip = page()
        .copied(("main", &Ink::default()), &[ItemId(4), ItemId(1), ItemId(2), ItemId(9)])
        .unwrap();
    assert_eq!(clip.text.as_deref(), Some("first\nsecond"), "in drawing order");
    let items: Value = serde_json::from_str(clip.items.as_deref().unwrap()).unwrap();
    assert_eq!(
        items[0],
        json!({"kind": "box", "color": "#ff0000", "x": 0, "y": 0, "w": 100, "h": 40}),
        "no id, maker, time or page"
    );
    assert_eq!(items.as_array().map(Vec::len), Some(3));

    let arrow = page().copied(("main", &Ink::default()), &[ItemId(3)]).unwrap();
    assert_eq!(arrow.text, None, "no text: none of the items is a text");
    assert!(!arrow.items.unwrap().contains("\"to\""), "an arrow's ends name this canvas's items");
    assert_eq!(page().copied(("main", &Ink::default()), &[ItemId(9)]), None);
}

#[test]
fn a_paste_adds_the_items_to_the_page_centred_on_the_point() {
    let clip = page().copied(("main", &Ink::default()), &[ItemId(1), ItemId(3)]).unwrap();
    let pasted = pasted(clip.items.as_deref().unwrap(), "p2", Point { x: 500.0, y: 300.0 });
    assert_eq!(pasted.len(), 2);
    // The box is 10 by 4 and the arrow 10 long on its top edge: together
    // centred on (5, 2) before, (500, 300) after.
    let Some(around) = pasted.iter().filter_map(|c| c.item.shape.bounds()).reduce(Rect::union)
    else {
        panic!()
    };
    let centre = (around.x + around.width / 2.0, around.y + around.height / 2.0);
    assert!((centre.0 - 500.0).abs() < 0.1 && (centre.1 - 300.0).abs() < 0.1, "{centre:?}");
    assert_eq!(pasted[0].object["page"], "p2");
    assert!(pasted[0].object.get("id").is_none(), "the server gives the id");
    let Shape::Arrow { bound, .. } = pasted[1].item.shape else { panic!() };
    assert_eq!(bound, [None, None], "drawn bound to nothing until the server says");
    assert_eq!(pasted[1].object["to"], "$0", "bound to the box pasted with it");
}

#[test]
fn a_pasted_arrow_keeps_only_the_ends_on_items_pasted_before_it() {
    let arrow = |from: &str, to: &str| json!({"kind": "arrow", "color": "#0000ff", "pts": [0, 0, 100, 0], "from": from, "to": to});
    let clip = json!([
        {"kind": "unknown"},
        {"kind": "box", "x": 0, "y": 0, "w": 100, "h": 40},
        arrow("$1", "$0"),
        arrow("i1", "$3"),
        arrow("$2", "$x"),
    ]);
    let pasted = pasted(&clip.to_string(), "p1", Point { x: 0.0, y: 0.0 });
    assert_eq!(pasted.len(), 4, "the unknown kind is left out");
    let ends = |n: usize| (pasted[n].object.get("from"), pasted[n].object.get("to"));
    assert_eq!(ends(1), (Some(&json!("$0")), None), "renumbered; an end on what is left out goes");
    assert_eq!(ends(2), (None, None), "an id of the copied canvas, and an item after it, go");
    assert_eq!(ends(3), (Some(&json!("$1")), None), "an arrow may point at an arrow");
}

#[test]
fn a_paste_of_anything_else_adds_nothing() {
    let at = Point { x: 0.0, y: 0.0 };
    for text in ["hello", "{}", "[]", "[1, 2]", r#"[{"kind": "unknown"}]"#] {
        assert!(pasted(text, "p1", at).is_empty(), "{text}");
    }
}

#[test]
fn plain_text_pastes_as_one_text_item_of_the_pen_s_colour() {
    let colour = Rgb { r: 0x12, g: 0x34, b: 0x56 };
    let items = text_items("ideas\r\nfor\tmonday", colour).unwrap();
    let pasted = pasted(&items, "p1", Point { x: 200.0, y: 100.0 });
    assert_eq!(pasted.len(), 1);
    assert_eq!(pasted[0].object["text"], "ideas\nfor monday");
    assert_eq!(pasted[0].object["color"], "#123456");
    let Shape::Text { frame, .. } = &pasted[0].item.shape else { panic!() };
    let centre = (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0);
    assert!((centre.0 - 200.0).abs() < 1.0 && (centre.1 - 100.0).abs() < 1.0, "{centre:?}");
    assert_eq!(text_items(" \n\t", colour), None, "blank text adds nothing");
}

/// slinger-8e4112: what the ink shows is copied and carried as it shows,
/// ink not yet written too: a stroke, an arrow bound to it, a box moved.
#[test]
fn ink_not_yet_written_is_copied_and_carried_as_it_shows() {
    use crate::scene::ink::{Answer, Change, PENDING};
    let mut ink = Ink::default();
    let red = Rgb { r: 255, g: 0, b: 0 };
    ink.down("main", "p1", red, Point { x: 1.0, y: 2.0 });
    ink.drag(Point { x: 3.0, y: 2.0 });
    assert!(ink.up(None));
    let stroke = ItemId(PENDING);
    let (from, to) = (Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 });
    let bound = [Some(stroke), Some(ItemId(1))];
    let shape = Shape::Arrow { from, to, width: 2.0, bound };
    let (canvas, on) = (String::from("main"), String::from("p1"));
    ink.push(Change::Put { canvas: canvas.clone(), page: on.clone(), colour: red, shape });
    let arrow = ItemId(PENDING + 1);
    let by = Point { x: 10.0, y: 0.0 };
    ink.push(Change::Move { canvas, page: on, ids: vec![ItemId(1)], by, follow: vec![] });

    let ids = [arrow, ItemId(1), stroke];
    let clip = page().copied(("main", &ink), &ids).unwrap();
    let items: Value = serde_json::from_str(clip.items.as_deref().unwrap()).unwrap();
    assert_eq!(
        items,
        json!([
            {"kind": "box", "color": "#ff0000", "x": 100, "y": 0, "w": 100, "h": 40},
            {"kind": "stroke", "color": "#ff0000", "width": 20, "pts": [10, 20, 20, 0]},
            {"kind": "arrow", "color": "#ff0000", "pts": [0, 0, 100, 0], "from": "$1", "to": "$0"},
        ]),
        "the box as moved; the arrow after the items it is bound to"
    );

    // Made, the stroke shows under the server's id before the page holds it.
    let (key, _) = ink.next().unwrap();
    ink.answered(key, Answer::Made { seq: 9, ids: vec![ItemId(7)] });
    let carried = page().carried(("main", &ink), &[ItemId(7)], "p2", Point { x: 1.0, y: 0.0 });
    assert_eq!(
        carried.iter().map(|c| c.object.clone()).collect::<Vec<_>>(),
        [json!({"page": "p2", "kind": "stroke", "color": "#ff0000", "width": 20,
                "pts": [20, 20, 20, 0]})]
    );
    assert_eq!(page().carried(("other", &ink), &[ItemId(7)], "p2", by), []);
}

#[test]
fn a_snippet_goes_as_one_pdf_object_and_pastes_back_as_that_crop() {
    let crop = Rect { x: 72.0, y: 180.0, width: 300.0, height: 120.0 };
    let shape = Shape::Pdf {
        frame: Rect { x: 2400.0, y: 300.0, width: 600.0, height: 240.0 },
        hash: "sha256:9f86".into(),
        pdf_page: 3,
        page_size: (612.0, 792.0),
        crop: Some(crop),
        name: Some("paper.pdf".into()),
    };
    let clip = snippet(&shape).unwrap();
    assert_eq!(clip.text, None, "a snippet has no text");
    let items: Value = serde_json::from_str(clip.items.as_deref().unwrap()).unwrap();
    assert_eq!(
        items,
        json!([{"kind": "pdf", "x": 24000, "y": 3000, "w": 6000, "h": 2400, "hash": "sha256:9f86",
                "pdfpage": 3, "pagesize": [6120, 7920], "crop": [720, 1800, 3000, 1200],
                "name": "paper.pdf"}]),
        "as the design's example line, with no id and no colour"
    );
    let pasted = pasted(clip.items.as_deref().unwrap(), "p1", Point { x: 0.0, y: 0.0 });
    let [one] = &pasted[..] else { panic!("{pasted:?}") };
    let frame = Rect { x: -300.0, y: -120.0, width: 600.0, height: 240.0 };
    assert_eq!(one.item.shape, shape.reframed(frame), "centred on the point, the crop kept");
    assert_eq!(one.object["page"], "p1");
    assert_eq!(snippet(&Shape::Panel { frame, fill: None, width: 1.0, dashed: false }), None);
}
