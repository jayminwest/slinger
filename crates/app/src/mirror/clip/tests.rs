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
    let clip = page().copied(&[ItemId(4), ItemId(1), ItemId(2), ItemId(9)]).unwrap();
    assert_eq!(clip.text.as_deref(), Some("first\nsecond"), "in drawing order");
    let items: Value = serde_json::from_str(clip.items.as_deref().unwrap()).unwrap();
    assert_eq!(
        items[0],
        json!({"kind": "box", "color": "#ff0000", "x": 0, "y": 0, "w": 100, "h": 40}),
        "no id, maker, time or page"
    );
    assert_eq!(items.as_array().map(Vec::len), Some(3));

    let arrow = page().copied(&[ItemId(3)]).unwrap();
    assert_eq!(arrow.text, None, "no text: none of the items is a text");
    assert!(!arrow.items.unwrap().contains("\"to\""), "an arrow's ends name this canvas's items");
    assert_eq!(page().copied(&[ItemId(9)]), None);
}

#[test]
fn a_paste_adds_the_items_to_the_page_centred_on_the_point() {
    let clip = page().copied(&[ItemId(1), ItemId(3)]).unwrap();
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
    assert_eq!(bound, [None, None]);
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
