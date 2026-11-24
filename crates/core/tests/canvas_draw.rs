//! A figure placed whole and drawn again by match (docs/design/figures.md,
//! "The verbs" and "Replace by match"): where each placement puts it, that
//! a rerun writes nothing, that a redraw keeps ids and the user's ink, who
//! may draw over a name, and what a bad shape says. A redraw finds the
//! figure where the user moved it.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::Error;
use slinger_core::canvas::file::{Again, Drawn, Log, Placement, Script, Source};
use slinger_core::canvas::{Author, GAP, Item, ItemId, Shape, Time};
use slinger_core::render;

const AT: Time = Time(1_762_790_400_000);

fn claude() -> Author {
    Author::Agent("claude".into())
}

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

fn beside(id: &str) -> Placement {
    Placement::Beside(id.into())
}

/// A log and the bytes it wrote, header first.
struct Book {
    log: Log,
    bytes: String,
}

impl Book {
    /// A canvas with the user's box i1 at 0,0, 100 by 100 units.
    fn new() -> Self {
        let (log, bytes) = Log::create("draw").unwrap();
        let mut book = Self { log, bytes };
        book.apply(
            &Author::User,
            &add(json!({"kind": "box", "x": 0, "y": 0, "w": 1000, "h": 1000})),
        );
        book
    }

    fn apply(&mut self, by: &Author, ops: &Value) -> Vec<String> {
        let w = self.log.apply(by, AT, ops).unwrap();
        self.bytes.push_str(&w.bytes);
        w.made
    }

    fn draw(&mut self, by: &Author, name: &str, shapes: &Value, at: &Placement) -> Drawn {
        let shapes = shapes.as_array().unwrap();
        let drawn = self.log.place_figure(by, AT, (name, shapes), at, None, None).unwrap();
        self.bytes.push_str(&drawn.written.bytes);
        drawn
    }

    /// As [`Book::draw`], from a script whose text is `text`, on `data`.
    fn script(
        &mut self,
        name: &str,
        shapes: &Value,
        at: &Placement,
        (text, data): (&str, Value),
    ) -> Drawn {
        let shapes = shapes.as_array().unwrap();
        let script = Script { text: text.into(), data, target: None };
        let drawn = self.log.place_figure(&claude(), AT, (name, shapes), at, None, Some(script));
        let drawn = drawn.unwrap();
        self.bytes.push_str(&drawn.written.bytes);
        drawn
    }

    fn fails(&self, by: &Author, name: &str, shapes: &Value, at: &Placement) -> Error {
        let shapes = shapes.as_array().unwrap();
        self.log.clone().place_figure(by, AT, (name, shapes), at, None, None).unwrap_err()
    }

    fn item(&self, item: ItemId) -> &Item {
        self.log.canvas().item(item).unwrap()
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

/// A small worksheet: a title, an answer area (a dashed box with the
/// agent's answer in it, and a rule under it) and a free box.
fn sheet(answer: &str) -> Value {
    json!([
        {"kind": "text", "x": 0, "y": 0, "text": "Growth", "part": "title", "locked": true},
        {"kind": "box", "x": 0, "y": 600, "w": 2000, "h": 800, "dash": "dashed",
            "part": "1a-answer", "locked": true},
        {"kind": "text", "x": 100, "y": 700, "text": answer, "part": "1a-answer", "locked": true},
        {"kind": "arrow", "pts": [0, 1600, 2000, 0], "heads": "none", "part": "1a-answer"},
        {"kind": "box", "x": 2400, "y": 0, "w": 400, "h": 400},
    ])
}

fn ids(drawn: &Drawn) -> Vec<ItemId> {
    drawn.draft.items.iter().map(Item::id).collect()
}

/// Each item of the draft is on the canvas as drafted, the maker's, in
/// the figure.
fn landed_as_drafted(book: &Book, drawn: &Drawn) {
    for item in &drawn.draft.items {
        assert_eq!(book.item(item.id()), item, "the draft is what landed");
        assert_eq!(item.member().fig.as_deref(), Some("worksheet"));
        assert_eq!(*item.author(), claude());
    }
}

#[test]
fn a_draw_beside_an_item_places_the_figure_as_one_undo_step() {
    let mut book = Book::new();
    let drawn = book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    let draft = &drawn.draft;
    assert_eq!((draft.name.as_str(), draft.page.as_str()), ("worksheet", "p1"));
    // GAP right of i1, top-aligned: the shapes' frame starts at their origin.
    let x = draft.origin[0];
    assert_eq!(f64::from(i32::try_from(x).unwrap()) / 10.0, 100.0 + GAP);
    assert_eq!(draft.origin, [x, 0]);
    assert_eq!(draft.frame, [x, 0, 2800, 1600]);
    assert_eq!(draft.parts, ["title", "1a-answer"]);
    assert_eq!(drawn.written.made, ["i2", "i3", "i4", "i5", "i6"]);
    assert_eq!(drawn.written.seq.len(), 5);
    assert_eq!(ids(&drawn), ["i2", "i3", "i4", "i5", "i6"].map(id));
    landed_as_drafted(&book, &drawn);
    assert!(book.item(id("i2")).member().locked);
    assert!(!book.item(id("i6")).member().locked);
    let figure = book.log.canvas().figure("worksheet").unwrap();
    assert_eq!(figure.items, ["i2", "i3", "i4", "i5", "i6"].map(id));
    assert_eq!(book.reopened().canvas(), book.log.canvas());

    let undone = book.log.undo(&claude(), AT);
    assert_eq!(undone.seq.len(), 5);
    assert!(book.log.canvas().figure("worksheet").is_none());
}

#[test]
fn the_same_draw_again_writes_nothing() {
    let mut book = Book::new();
    let first = book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    let next = book.log.next();
    let again = book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    assert!(again.written.bytes.is_empty() && again.written.seq.is_empty());
    assert!(again.draft.unchanged());
    assert_eq!(book.log.next(), next);
    assert_eq!((again.draft.items, again.draft.frame), (first.draft.items, first.draft.frame));
    // An undo still takes back the draw, not an empty step.
    book.log.undo(&claude(), AT);
    assert!(book.log.canvas().figure("worksheet").is_none());
}

#[test]
fn a_redraw_sets_what_changed_in_place_and_leaves_the_users_ink() {
    let mut book = Book::new();
    let first = book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    let ink = book.apply(
        &Author::User,
        &add(json!({"kind": "stroke", "x": 0, "y": 0,
        "pts": [3700, 900, 100, 50], "width": 20})),
    );
    let ink_before = book.item(id(&ink[0])).clone();
    let mut changed = sheet("about 147");
    changed.as_array_mut().unwrap().pop();
    changed.as_array_mut().unwrap().push(json!({"kind": "text", "x": 0, "y": 1800,
        "text": "Show your work", "part": "note"}));
    let [x, y] = first.draft.origin;
    let again = book.draw(&claude(), "worksheet", &changed, &Placement::At { page: None, x, y });
    // One set (the answer), one add (the note), one remove (the free box).
    let lines: Vec<Value> =
        again.written.bytes.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let ops: Vec<&str> = lines.iter().map(|l| l["op"].as_str().unwrap()).collect();
    assert_eq!(ops, ["set", "add", "remove"]);
    assert_eq!(lines[0]["id"], "i4");
    assert_eq!(lines[2]["ids"], json!(["i6"]));
    assert_eq!(ids(&again)[..4], ids(&first)[..4]);
    let Shape::Text { text, .. } = book.item(id("i4")).shape() else { panic!() };
    assert_eq!(text, "about 147");
    assert_eq!(book.item(id(&ink[0])), &ink_before);
    assert_eq!(book.reopened().canvas(), book.log.canvas());
    // The redraw is one step: an undo puts the first drawing back.
    book.log.undo(&claude(), AT);
    let figure = book.log.canvas().figure("worksheet").unwrap();
    assert_eq!(figure.items, ids(&first));
}

#[test]
fn a_reorder_within_a_part_and_kind_churns_contents_never_ids() {
    let mut book = Book::new();
    let two = |a: &str, b: &str| {
        json!([
            {"kind": "text", "x": 0, "y": 0, "text": a, "part": "rows"},
            {"kind": "box", "x": 0, "y": 500, "w": 100, "h": 100, "part": "other"},
            {"kind": "text", "x": 0, "y": 300, "text": b, "part": "rows"},
        ])
    };
    let at = Placement::At { page: None, x: 5000, y: 0 };
    let first = book.draw(&claude(), "rows", &two("one", "two"), &at);
    // Across parts and kinds a reorder is nothing at all.
    let mut moved = two("one", "two");
    moved.as_array_mut().unwrap().swap(0, 1);
    let same = book.draw(&claude(), "rows", &moved, &at);
    assert_eq!(same.written.bytes, "");
    assert_eq!(same.draft.items[0].id(), first.draft.items[1].id());
    // Within one, the first text stays the first id, with the other words.
    let swapped = book.draw(&claude(), "rows", &two("two", "one"), &at);
    assert_eq!(ids(&swapped), ids(&first));
    let lines: Vec<&str> = swapped.written.bytes.lines().collect();
    assert_eq!(lines.len(), 2, "two sets: {lines:?}");
    assert!(lines.iter().all(|l| l.contains(r#""op":"set""#)));
}

#[test]
fn over_puts_the_origin_on_the_items_top_left_and_moves_nothing() {
    let mut book = Book::new();
    let user_box = book.item(id("i1")).clone();
    let over = Placement::Over("i1".into());
    assert_eq!(book.log.target(&over).unwrap(), Some([0, 0, 1000, 1000]));
    let marks = json!([{"kind": "box", "x": 100, "y": 100, "w": 200, "h": 200}]);
    let drawn = book.draw(&claude(), "marks", &marks, &over);
    assert_eq!(drawn.draft.origin, [0, 0]);
    assert_eq!(drawn.draft.frame, [100, 100, 200, 200]);
    assert_eq!(book.item(id("i1")), &user_box);
}

#[test]
fn at_puts_the_origin_on_a_point_of_a_page() {
    let mut book = Book::new();
    book.apply(&Author::User, &json!({"op": "page.add", "name": "Two"}));
    let at = Placement::At { page: Some("Two".into()), x: -500, y: 250 };
    assert_eq!(book.log.target(&at).unwrap(), None);
    let drawn = book.draw(&claude(), "worksheet", &sheet("?"), &at);
    assert_eq!((drawn.draft.page.as_str(), drawn.draft.origin), ("p2", [-500, 250]));
    assert_eq!(drawn.draft.frame[..2], [-500, 250]);
    // Drawn on the first page now, it is drawn anew there: new ids.
    let first_page = Placement::At { page: None, x: 0, y: 3000 };
    let moved = book.draw(&claude(), "worksheet", &sheet("?"), &first_page);
    assert_eq!(moved.draft.page, "p1");
    assert!(ids(&moved).iter().all(|i| !ids(&drawn).contains(i)));
    assert_eq!(book.log.canvas().figure("worksheet").unwrap().items, ids(&moved));
}

#[test]
fn in_puts_the_figure_in_the_boxs_room() {
    let mut book = Book::new();
    let room = add(json!({"kind": "box", "x": 0, "y": 3000, "w": 6000, "h": 4000}));
    let made = book.apply(&Author::User, &room);
    let room = Placement::In(made[0].clone());
    assert_eq!(book.log.target(&room).unwrap(), Some([0, 3000, 6000, 4000]));
    let drawn = book.draw(&claude(), "worksheet", &sheet("?"), &room);
    let [x, y, w, h] = drawn.draft.frame;
    assert!(x >= 0 && y >= 3000 && x + w <= 6000 && y + h <= 7000, "{:?}", drawn.draft.frame);
    // No room: a conflict, and the draw writes nothing.
    let tiny = add(json!({"kind": "box", "x": 0, "y": 9000, "w": 100, "h": 100}));
    let tiny = book.apply(&Author::User, &tiny);
    let err = book.fails(&claude(), "other", &sheet("?"), &Placement::In(tiny[0].clone()));
    assert_eq!(err.code(), "conflict");
    let err = book.fails(&claude(), "other", &sheet("?"), &Placement::In("i3".into()));
    assert_eq!(err.code(), "invalid", "{err}");
}

#[test]
fn only_the_maker_or_the_user_may_draw_over_a_name() {
    let mut book = Book::new();
    book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    let other = Author::Agent("codex".into());
    let err = book.fails(&other, "worksheet", &sheet("!"), &beside("i1"));
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("worksheet"), "{err}");
    assert_eq!(err.hint(), Some("pick another --name"));
    let drawn = book.draw(&Author::User, "worksheet", &sheet("!"), &beside("i1"));
    assert_eq!(drawn.written.seq.len(), 1, "the user set the one text that changed");
    assert_eq!(*book.item(id("i4")).author(), Author::User, "the user's words are the user's");
}

#[test]
fn a_draw_never_leaves_one_name_on_two_pages() {
    let mut book = Book::new();
    book.apply(&Author::User, &json!({"op": "page.add", "name": "Two"}));
    book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    // The user's answer is the user's now: the agent may not move it.
    book.draw(&Author::User, "worksheet", &sheet("!"), &beside("i1"));
    let two = Placement::At { page: Some("Two".into()), x: 0, y: 0 };
    let err = book.fails(&claude(), "worksheet", &sheet("?"), &two);
    assert_eq!(err.code(), "conflict", "{err}");
    assert_eq!(err.hint(), Some("draw it on page Page 1, or pick another --name"), "{err}");
    let err =
        book.log.draft_figure(&claude(), AT, "worksheet", sheet("?").as_array().unwrap(), &two);
    assert_eq!(err.unwrap_err().code(), "conflict", "a dry run says the same");
    // On its own page the agent still draws, and the user moves it whole.
    book.draw(&claude(), "worksheet", &sheet("!"), &beside("i1"));
    let moved = book.draw(&Author::User, "worksheet", &sheet("!"), &two);
    let figure = book.log.canvas().figure("worksheet").unwrap();
    assert_eq!(figure.items, ids(&moved));
    assert_eq!(moved.draft.page, "p2");
    assert!(figure.items.iter().all(|i| book.item(*i).page() == book.item(figure.items[0]).page()));
}

#[test]
fn a_bad_shape_is_invalid_and_names_its_place() {
    let book = Book::new();
    let at = beside("i1");
    let boxed = |extra: Value| {
        let mut b = json!({"kind": "box", "x": 0, "y": 0, "w": 1, "h": 1});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        b
    };
    for (shapes, says) in [
        (json!([boxed(json!({})), {"kind": "blob"}]), "shape 1:"),
        (json!([boxed(json!({"page": "p1"}))]), "shape 0: it has `page`"),
        (json!([boxed(json!({"id": "i9"}))]), "it has `id`"),
        (json!([boxed(json!({"fig": "other"}))]), "is not \"worksheet\""),
        (json!([boxed(json!({"part": "Bad Part"}))]), "shape 0:"),
        (json!(["box"]), "it is not an object"),
        (json!([]), "0 shapes"),
    ] {
        let err = book.fails(&claude(), "worksheet", &shapes, &at);
        assert_eq!(err.code(), "invalid", "{shapes}");
        assert!(err.to_string().contains(says), "{err} has no {says:?}");
        assert!(err.hint().is_some());
    }
    let one = json!([boxed(json!({}))]);
    for name in ["b4", "Worksheet", "4th", ""] {
        assert_eq!(book.fails(&claude(), name, &one, &at).code(), "invalid", "{name}");
    }
    assert_eq!(book.fails(&claude(), "worksheet", &one, &beside("i99")).code(), "not_found");
}

#[test]
fn an_agents_figure_text_keeps_the_agents_length_rule() {
    let mut book = Book::new();
    let long = vec!["a line"; 13].join("\n");
    let shapes = json!([{"kind": "text", "x": 0, "y": 0, "text": long}]);
    let err = book.fails(&claude(), "notes", &shapes, &beside("i1"));
    assert_eq!(err.code(), "too_long");
    assert!(err.to_string().contains("figure notes"), "{err}");
    book.draw(&Author::User, "notes", &shapes, &beside("i1"));
}

#[test]
fn a_draft_writes_nothing_and_draws_alone() {
    let mut book = Book::new();
    let next = book.log.next();
    let shapes = sheet("?");
    let list = shapes.as_array().unwrap();
    let draft = book.log.draft_figure(&claude(), AT, "worksheet", list, &beside("i1")).unwrap();
    assert_eq!(book.log.next(), next);
    assert!(book.log.canvas().figure("worksheet").is_none());
    let images = render::shapes(&draft.items, "worksheet");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].name, "worksheet.png");
    let drawn = book.draw(&claude(), "worksheet", &shapes, &beside("i1"));
    assert_eq!(drawn.draft, draft);
}

#[test]
fn a_draw_under_a_key_it_was_written_under_is_not_written_twice() {
    let mut book = Book::new();
    let shapes = sheet("?");
    let (list, at) = (shapes.as_array().unwrap(), beside("i1"));
    let first =
        book.log.place_figure(&claude(), AT, ("worksheet", list), &at, Some("k1"), None).unwrap();
    let again =
        book.log.place_figure(&claude(), AT, ("worksheet", list), &at, Some("k1"), None).unwrap();
    assert_eq!(again.written.bytes, "");
    assert_eq!(again.written.seq, first.written.seq);
    assert_eq!(again.written.made, first.written.made);
}

#[test]
fn a_redraw_follows_the_figure_where_it_was_moved_and_keeps_the_author_rule() {
    let mut book = Book::new();
    let drawn = book.script("worksheet", &sheet("?"), &beside("i1"), ("sheet()", json!({})));
    let (origin, frame) = (drawn.draft.origin, drawn.draft.frame);
    let again = book.log.again(&claude(), "worksheet").unwrap();
    let at = Placement::At { page: Some("p1".into()), x: origin[0], y: origin[1] };
    let source = book.log.source("worksheet").unwrap().clone();
    let (page, parts) = ("p1".into(), vec![]);
    let want = Again { at, page, origin, frame, parts, items: 5, source };
    assert_eq!(Again { parts: vec![], ..again.clone() }, want, "nothing moved");
    assert_eq!(again.parts, ["title", "1a-answer"]);
    let unmoved = book.draw(&claude(), "worksheet", &sheet("?"), &again.at);
    assert!(unmoved.draft.unchanged(), "drawn again where it is, it writes nothing");

    let ids: Vec<String> = ids(&drawn).iter().map(|i| format!("i{}", i.get())).collect();
    book.apply(&Author::User, &json!({"op": "move", "ids": ids, "dx": 300, "dy": -200}));
    let moved = book.log.again(&claude(), "worksheet").unwrap();
    assert_eq!(moved.origin, [origin[0] + 300, origin[1] - 200]);
    assert_eq!(moved.frame, [frame[0] + 300, frame[1] - 200, frame[2], frame[3]]);
    let redrawn = book.draw(&claude(), "worksheet", &sheet("42"), &moved.at);
    assert_eq!(redrawn.written.seq.len(), 1, "only the answer's text is set, from no script");
    assert_eq!(redrawn.draft.frame[..2], moved.frame[..2], "drawn where it was moved to");

    let ada = Author::Agent("ada".into());
    let e = book.log.again(&ada, "worksheet").unwrap_err();
    assert_eq!(e.code(), "invalid", "the author rule: {e}");
    assert!(book.log.again(&Author::User, "worksheet").is_ok(), "the user may");
    let e = book.log.again(&claude(), "nothing").unwrap_err();
    assert_eq!(e.code(), "not_found");
}

#[test]
fn the_canvas_file_keeps_what_a_redraw_needs_and_a_reader_finds_it_there() {
    let mut book = Book::new();
    let data = json!({"answer": "?", "rate": 0.08});
    let drawn = book.script("worksheet", &sheet("?"), &beside("i1"), ("sheet(data)", data.clone()));
    let line = book.bytes.lines().last().unwrap();
    assert!(line.contains(r#""op":"figure","fig":"worksheet""#), "{line}");
    assert!(
        line.ends_with(r#""data":{"answer":"?","rate":0.08},"script":"sheet(data)"}"#),
        "{line}"
    );
    assert!(!line.contains("step"), "no undo step: {line}");
    assert_eq!(drawn.written.seq.len(), 6, "five adds and the figure line");
    let want = Source {
        script: Script { text: "sheet(data)".into(), data, target: None },
        origin: drawn.draft.origin,
        frame: drawn.draft.frame,
    };
    assert_eq!(book.log.source("worksheet"), Some(&want));

    // The file alone, opened anywhere, redraws: no record is needed.
    let copied = book.reopened();
    assert_eq!(copied.source("worksheet"), Some(&want));
    assert_eq!(copied.again(&claude(), "worksheet").unwrap().source, want);
    assert_eq!(copied.canvas(), book.log.canvas(), "the line changes no item");

    // The same draw again writes nothing, and no figure line either.
    let next = book.log.next();
    let same = book.script("worksheet", &sheet("?"), &beside("i1"), ("sheet(data)", json!({})));
    assert!(same.draft.unchanged() && same.written.bytes.is_empty());
    assert_eq!(
        (book.log.next(), &book.log.source("worksheet").unwrap().script.data),
        (next, &want.script.data)
    );

    // The last line wins; undo takes the items, and with them the redraw.
    let at = Placement::At { page: Some("p1".into()), x: want.origin[0], y: want.origin[1] };
    book.script("worksheet", &sheet("42"), &at, ("sheet(data)", json!({"answer": "42"})));
    let last = book.reopened().source("worksheet").unwrap().script.data.clone();
    assert_eq!(last, json!({"answer": "42"}));
    book.log.undo(&claude(), AT);
    book.log.undo(&claude(), AT);
    assert_eq!(book.log.again(&claude(), "worksheet").unwrap_err().code(), "not_found");
    let user = book.log.again(&claude(), "i1").unwrap_err();
    assert_eq!(user.code(), "not_found", "{user}");
}

#[test]
fn a_figure_no_script_drew_is_not_drawn_again() {
    let mut book = Book::new();
    book.draw(&claude(), "worksheet", &sheet("?"), &beside("i1"));
    let e = book.log.again(&claude(), "worksheet").unwrap_err();
    assert_eq!(e.code(), "not_found");
    assert!(e.to_string().contains("drawn by a script"), "{e}");
}
