//! The `pdf` item (docs/design/pdf.md): read, written and set.

use serde_json::json;

use super::super::*;
use crate::scene::ink::Change;

fn whole() -> Value {
    json!({"id": "i3", "kind": "pdf", "x": 0, "y": 0, "w": 16000, "h": 20706,
        "hash": "sha256:9f", "pdfpage": 1, "pagesize": [6120, 7920], "name": "paper.pdf"})
}

fn cropped() -> Value {
    json!({"id": "i9", "kind": "pdf", "x": 24000, "y": 3000, "w": 6000, "h": 2400,
        "hash": "sha256:9f", "pdfpage": 3, "pagesize": [6120, 7920], "crop": [720, 1800, 3000, 1200]})
}

#[test]
fn a_pdf_item_reads_in_units_and_page_points_and_writes_back_as_it_was() {
    let Some(Item { shape: Shape::Pdf { frame, hash, pdf_page, page_size, crop, name }, .. }) =
        item(&cropped())
    else {
        panic!()
    };
    assert_eq!(frame, Rect { x: 2400.0, y: 300.0, width: 600.0, height: 240.0 });
    assert_eq!((hash.as_str(), pdf_page, page_size), ("sha256:9f", 3, (612.0, 792.0)));
    assert_eq!(crop, Some(Rect { x: 72.0, y: 180.0, width: 300.0, height: 120.0 }));
    assert_eq!(name, None);
    for stored in [whole(), cropped()] {
        let written = object(&item(&stored).unwrap());
        assert_eq!(written, Some(stored), "no colour: a pdf item has none");
    }
}

#[test]
fn a_pdf_item_without_what_it_needs_is_not_drawn() {
    let without = |key: &str| {
        let mut o = whole();
        o.as_object_mut().unwrap().remove(key);
        o
    };
    let with = |key: &str, v: Value| {
        let mut o = whole();
        o[key] = v;
        o
    };
    for o in [
        without("hash"),
        without("pdfpage"),
        without("pagesize"),
        without("w"),
        with("pdfpage", json!(0)),
        with("pagesize", json!([6120, 0])),
        with("pagesize", json!([6120])),
        with("crop", json!([0, 0, 10])),
        with("crop", json!([0, 0, 10, 0])),
    ] {
        assert_eq!(item(&o), None, "{o}");
    }
    assert!(item(&with("crop", Value::Null)).is_some(), "a null crop is the whole page");
}

#[test]
fn a_resized_pdf_item_sets_every_field_and_clears_a_crop_it_has_not() {
    let mut resized = item(&whole()).unwrap();
    resized.shape = resized.shape.reframed(Rect { x: 10.0, y: 20.0, width: 800.0, height: 1035.3 });
    let change = Change::Reshape { canvas: "c".into(), page: "p1".into(), items: vec![resized] };
    let fields = json!({"x": 100, "y": 200, "w": 8000, "h": 10353, "hash": "sha256:9f",
        "pdfpage": 1, "pagesize": [6120, 7920], "name": "paper.pdf", "crop": null});
    assert_eq!(ops(&change), json!([{"op": "set", "id": "i3", "fields": fields}]));
}

#[test]
fn an_opened_page_is_added_without_a_colour() {
    let shape = item(&cropped()).unwrap().shape;
    let add = add_pdf("p1", &shape);
    let mut expected = cropped();
    let o = expected.as_object_mut().unwrap();
    o.remove("id");
    o.insert("page".into(), "p1".into());
    assert_eq!(add, json!({"op": "add", "item": expected}));
}
