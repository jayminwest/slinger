//! An item's style (docs/design/figures.md, "The model"): `fill`, `dash`,
//! `heads` and `closed` through a request, the file, `show`, a `set` and
//! undo, and what a reader from before styles keeps of a file with them.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::{Author, Colour, Dash, Heads, ItemId, PageId, Rect, Style, Time};

const AT: Time = Time(1_762_790_400_000);
const P1: PageId = PageId::new(1);

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

/// A log and the bytes it wrote, header first.
struct Book {
    log: Log,
    bytes: String,
}

impl Book {
    fn new() -> Self {
        let (log, bytes) = Log::create("styles").unwrap();
        Self { log, bytes }
    }

    fn apply(&mut self, ops: &Value) -> Vec<String> {
        let w = self.log.apply(&Author::User, AT, ops).unwrap();
        self.bytes.push_str(&w.bytes);
        w.made
    }

    fn style(&self, item: &str) -> Style {
        *self.log.canvas().item(id(item)).unwrap().style()
    }

    fn reopened(&self) -> Log {
        Log::open(self.bytes.as_bytes()).unwrap().0
    }
}

fn add(item: Value) -> Value {
    let mut item = item;
    item["page"] = "p1".into();
    json!({"op": "add", "item": item})
}

/// A filled dashed box, a dotted closed filled stroke, and a headless and a
/// two-headed arrow.
fn styled(book: &mut Book) -> Vec<String> {
    book.apply(&json!([
        add(json!({"kind": "box", "x": 0, "y": 0, "w": 2000, "h": 1000,
            "fill": "#ffcc0033", "dash": "dashed"})),
        add(json!({"kind": "stroke", "width": 20, "pts": [3000, 0, 500, 0, 0, 500],
            "closed": true, "fill": "#2f80ed", "dash": "dotted"})),
        add(json!({"kind": "arrow", "pts": [0, 2000, 1000, 0], "heads": "none"})),
        add(json!({"kind": "arrow", "pts": [0, 3000, 1000, 0], "heads": "both", "dash": "dashed"})),
    ]))
}

#[test]
fn a_style_is_kept_in_the_file_shown_and_read_back() {
    let mut book = Book::new();
    let made = styled(&mut book);
    let yellow = Colour { r: 0xff, g: 0xcc, b: 0, a: 0x33 };
    assert_eq!(
        book.style(&made[0]),
        Style { fill: Some(yellow), dash: Dash::Dashed, ..Style::default() }
    );
    let blue = Some(Colour::rgb(0x2f, 0x80, 0xed));
    let loop_ = Style { fill: blue, dash: Dash::Dotted, closed: true, ..Style::default() };
    assert_eq!(book.style(&made[1]), loop_);
    assert_eq!(book.style(&made[2]).heads, Heads::None);
    assert_eq!(
        book.style(&made[3]),
        Style { dash: Dash::Dashed, heads: Heads::Both, ..Style::default() }
    );

    // Written after the kind's own fields, in the order fill, dash, heads, closed.
    assert!(
        book.bytes.contains(r##""h":1000,"fill":"#ffcc0033","dash":"dashed"}"##),
        "{}",
        book.bytes
    );
    assert!(book.bytes.contains(r##""fill":"#2f80ed","dash":"dotted","closed":true}"##));
    assert!(book.bytes.contains(r#""heads":"none"}"#));

    let shown = book.log.show(None).unwrap().items;
    assert_eq!(shown[0]["fill"], "#ffcc0033");
    assert_eq!(shown[1]["closed"], true);
    assert_eq!(shown[3]["heads"], "both");

    let reopened = book.reopened();
    for item in &made {
        assert_eq!(reopened.canvas().item(id(item)).unwrap().style(), &book.style(item));
    }
}

#[test]
fn an_item_without_a_style_writes_no_style_key() {
    let mut book = Book::new();
    book.apply(&json!([
        add(json!({"kind": "box", "x": 0, "y": 0, "w": 100, "h": 100})),
        add(json!({"kind": "arrow", "pts": [0, 0, 10, 10], "heads": "end", "fill": null})),
        add(json!({"kind": "stroke", "width": 20, "pts": [0, 0, 10, 10], "closed": false})),
    ]));
    for key in ["fill", "dash", "heads", "closed"] {
        assert!(!book.bytes.contains(key), "{key} in {}", book.bytes);
    }
}

#[test]
fn a_request_with_a_style_the_item_cannot_take_is_invalid_with_a_hint() {
    let bad = [
        json!({"kind": "text", "x": 0, "y": 0, "text": "hi", "dash": "dashed"}),
        json!({"kind": "box", "x": 0, "y": 0, "w": 10, "h": 10, "heads": "both"}),
        json!({"kind": "stroke", "width": 20, "pts": [0, 0, 10, 10], "fill": "#ff0000"}),
        json!({"kind": "arrow", "pts": [0, 0, 10, 10], "closed": true}),
        json!({"kind": "box", "x": 0, "y": 0, "w": 10, "h": 10, "fill": "yellow"}),
        json!({"kind": "box", "x": 0, "y": 0, "w": 10, "h": 10, "dash": "wavy"}),
        json!({"kind": "arrow", "pts": [0, 0, 10, 10], "heads": 2}),
        json!({"kind": "stroke", "width": 20, "pts": [0, 0, 10, 10], "closed": "yes"}),
    ];
    let mut book = Book::new();
    for item in bad {
        let err = book.log.apply(&Author::User, AT, &add(item.clone())).unwrap_err();
        assert_eq!(err.code(), "invalid", "{item}");
        let key = ["fill", "dash", "heads", "closed"].into_iter().find(|k| item.get(k).is_some());
        let hint = err.hint().unwrap_or_default().to_owned();
        assert!(hint.contains(key.unwrap()), "{item}: {hint}");
    }
    assert_eq!(book.log.canvas().items(P1).count(), 0, "nothing was added");
}

#[test]
fn a_file_with_a_style_it_cannot_read_draws_the_item_plain() {
    let (_, header) = Log::create("c").unwrap();
    let line = |n: u64, item: Value| {
        let mut item = item;
        item["id"] = format!("i{n}").into();
        item["page"] = "p1".into();
        format!(
            "{}\n",
            json!({"n": n, "t": "2025-11-10T16:00:00.000Z", "by": "user", "op": "add", "item": item})
        )
    };
    let bytes = header
        + &line(
            1,
            json!({"kind": "box", "color": "#000000", "x": 0, "y": 0, "w": 10, "h": 10,
            "fill": "yellow", "dash": "wavy", "heads": "both", "closed": true}),
        )
        + &line(
            2,
            json!({"kind": "text", "color": "#000000", "x": 0, "y": 0, "text": "a",
            "fill": "#ff0000", "dash": "dashed"}),
        )
        + &line(
            3,
            json!({"kind": "stroke", "color": "#000000", "width": 10, "pts": [0, 0, 5, 5],
            "fill": "#ff0000", "closed": 1}),
        );
    let (log, _) = Log::open(bytes.as_bytes()).unwrap();
    assert_eq!(log.canvas().items(P1).count(), 3, "every item is there");
    for item in log.canvas().items(P1) {
        assert_eq!(item.style(), &Style::default(), "{:?}", item.id());
    }
}

#[test]
fn a_set_changes_the_style_only_when_it_names_it_and_undo_puts_it_back() {
    let mut book = Book::new();
    let made = styled(&mut book);
    let before = book.style(&made[0]);
    book.apply(&json!({"op": "set", "id": made[0], "fields": {"w": 3000}}));
    assert_eq!(book.style(&made[0]), before, "a set of the shape keeps the style");
    book.apply(&json!({"op": "move", "ids": [made[0]], "dx": 10, "dy": 0}));
    assert_eq!(book.style(&made[0]), before, "a move keeps it");

    let w = book
        .log
        .apply(&Author::User, AT, &json!({"op": "set", "id": made[0], "fields": {"fill": null}}))
        .unwrap();
    book.bytes.push_str(&w.bytes);
    assert!(w.bytes.contains(r#""fill":null"#), "a set writes the whole style: {}", w.bytes);
    assert_eq!(book.style(&made[0]), Style { dash: Dash::Dashed, ..Style::default() });

    let undone = book.log.undo(&Author::User, AT);
    book.bytes.push_str(&undone.bytes);
    assert_eq!(book.style(&made[0]), before);
    assert_eq!(book.reopened().canvas().item(id(&made[0])).unwrap().style(), &before);

    let err = book.log.apply(
        &Author::User,
        AT,
        &json!({"op": "set", "id": made[0], "fields": {"heads": "both"}}),
    );
    assert_eq!(err.unwrap_err().code(), "invalid");
}

#[test]
fn an_undo_of_a_plain_edit_writes_no_style() {
    let mut book = Book::new();
    let made = styled(&mut book);
    book.apply(&json!({"op": "move", "ids": [made[0]], "dx": 10, "dy": 0}));
    let undone = book.log.undo(&Author::User, AT);
    assert!(!undone.bytes.contains("fill"), "{}", undone.bytes);
}

#[test]
fn an_erase_and_undo_keeps_the_style() {
    let mut book = Book::new();
    let made = styled(&mut book);
    let before = book.style(&made[1]);
    let removed = book.log.remove(&Author::User, AT, &made[1]).unwrap();
    book.bytes.push_str(&removed.bytes);
    let restored = book.log.undo(&Author::User, AT);
    book.bytes.push_str(&restored.bytes);
    assert!(restored.bytes.contains(r#""closed":true"#));
    assert_eq!(book.reopened().canvas().item(id(&made[1])).unwrap().style(), &before);
}

/// Every style key of a file taken out: what a reader from before styles
/// reads, since it ignores fields it does not know.
fn without_styles(bytes: &str) -> String {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for key in ["fill", "dash", "heads", "closed"] {
                    m.remove(key);
                }
                m.values_mut().for_each(strip);
            }
            Value::Array(list) => list.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut out = String::new();
    for line in bytes.lines() {
        let mut v: Value = serde_json::from_str(line).unwrap();
        strip(&mut v);
        out.push_str(&v.to_string());
        out.push('\n');
    }
    out
}

/// docs/design/figures.md, "What an old reader does with new fields": a file
/// with styles, set and undone, read as a build from before them reads it,
/// keeps every item at the same place, drawn plain.
#[test]
fn a_reader_from_before_styles_loses_no_item() {
    let mut book = Book::new();
    let made = styled(&mut book);
    book.apply(&json!({"op": "set", "id": made[0], "fields": {"dash": "dotted"}}));
    book.apply(&json!({"op": "move", "ids": [made[1]], "dx": 500, "dy": 500}));
    let full = book.reopened();
    let (old, _) = Log::open(without_styles(&book.bytes).as_bytes()).unwrap();
    let places = |log: &Log| -> Vec<(ItemId, Rect)> {
        log.canvas().items(P1).map(|i| (i.id(), i.bounds())).collect()
    };
    assert_eq!(places(&old), places(&full));
    assert_eq!(places(&old).len(), 4);
    assert!(old.canvas().items(P1).all(|i| i.style() == &Style::default()));
}
