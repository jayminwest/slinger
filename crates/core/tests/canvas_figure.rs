//! Figures and parts (docs/design/figures.md, "Figure and part" and
//! "Lock"): `locked`, `fig` and `part` through a request, the file, `show`,
//! a `set` and undo; what a reader from before them keeps; the figures a
//! canvas lists; and a figure address wherever an item id is taken.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Edit, Log, Put};
use slinger_core::canvas::{Address, Author, ItemId, Member, PageId, Rect, Time};

const AT: Time = Time(1_762_790_400_000);
const P1: PageId = PageId::new(1);

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

fn claude() -> Author {
    Author::Agent("claude".into())
}

/// A log and the bytes it wrote, header first.
struct Book {
    log: Log,
    bytes: String,
}

impl Book {
    fn new() -> Self {
        let (log, bytes) = Log::create("figures").unwrap();
        Self { log, bytes }
    }

    fn apply_as(&mut self, by: &Author, ops: &Value) -> Vec<String> {
        let w = self.log.apply(by, AT, ops).unwrap();
        self.bytes.push_str(&w.bytes);
        w.made
    }

    fn member(&self, item: &str) -> Member {
        self.log.canvas().item(id(item)).unwrap().member().clone()
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

/// A worksheet by claude: a title text, a question box and its two-item
/// answer area (a dashed box and a line), locked; and the user's free box.
fn worksheet(book: &mut Book) -> Vec<String> {
    let mut made = book.apply_as(
        &claude(),
        &json!([
            add(json!({"kind": "text", "x": 0, "y": 0, "text": "Growth",
                "locked": true, "fig": "worksheet", "part": "title"})),
            add(json!({"kind": "box", "x": 0, "y": 1000, "w": 2000, "h": 500,
                "locked": true, "fig": "worksheet", "part": "q1"})),
            add(json!({"kind": "box", "x": 0, "y": 2000, "w": 2000, "h": 1000, "dash": "dashed",
                "locked": true, "fig": "worksheet", "part": "1a-answer"})),
            add(json!({"kind": "arrow", "pts": [0, 3200, 2000, 0], "heads": "none",
                "locked": true, "fig": "worksheet", "part": "1a-answer"})),
            add(json!({"kind": "box", "x": 3000, "y": 0, "w": 400, "h": 400,
                "fig": "worksheet"})),
        ]),
    );
    made.extend(book.apply_as(
        &Author::User,
        &add(json!({"kind": "box", "x": 9000, "y": 0, "w": 100, "h": 100})),
    ));
    made
}

#[test]
fn a_figure_is_kept_in_the_file_shown_and_read_back() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    let named = |part: &str| Member {
        locked: true,
        fig: Some("worksheet".into()),
        part: Some(part.into()),
    };
    assert_eq!(book.member(&made[0]), named("title"));
    assert_eq!(book.member(&made[2]), named("1a-answer"));
    let free_part = Member { fig: Some("worksheet".into()), ..Member::default() };
    assert_eq!(book.member(&made[4]), free_part);
    assert_eq!(book.member(&made[5]), Member::default());

    // Written after the style keys, in the order locked, fig, part.
    let line = r#""dash":"dashed","locked":true,"fig":"worksheet","part":"1a-answer"}"#;
    assert!(book.bytes.contains(line), "{}", book.bytes);

    let shown = book.log.show(None).unwrap().items;
    assert_eq!(shown[0]["fig"], "worksheet");
    assert_eq!(shown[0]["part"], "title");
    assert_eq!(shown[0]["locked"], true);
    assert!(shown[5].get("fig").is_none());

    let reopened = book.reopened();
    for item in &made {
        assert_eq!(reopened.canvas().item(id(item)).unwrap().member(), &book.member(item));
    }
}

#[test]
fn an_item_in_no_figure_writes_no_figure_key() {
    let mut book = Book::new();
    book.apply_as(
        &Author::User,
        &json!([
            add(json!({"kind": "box", "x": 0, "y": 0, "w": 100, "h": 100})),
            add(json!({"kind": "box", "x": 0, "y": 0, "w": 100, "h": 100,
                "locked": false, "fig": null, "part": null})),
        ]),
    );
    for key in ["\"locked\"", "\"fig\"", "\"part\""] {
        assert!(!book.bytes.contains(key), "{key} in {}", book.bytes);
    }
}

#[test]
fn a_request_with_a_bad_figure_key_is_invalid_with_a_hint() {
    let item = |extra: Value| {
        let mut item = json!({"kind": "box", "x": 0, "y": 0, "w": 10, "h": 10});
        item.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        item
    };
    let bad = [
        (json!({"locked": "yes"}), "locked"),
        (json!({"fig": 3}), "fig"),
        (json!({"fig": "Work Sheet"}), "figure name"),
        (json!({"fig": "b4"}), "item id"),
        (json!({"fig": "-x"}), "figure name"),
        (json!({"fig": "a".repeat(41)}), "figure name"),
        (json!({"fig": "w", "part": "a/b"}), "part name"),
        (json!({"part": "q1"}), "fig"),
    ];
    let mut book = Book::new();
    for (extra, hint_has) in bad {
        let err = book.log.apply(&Author::User, AT, &add(item(extra.clone()))).unwrap_err();
        assert_eq!(err.code(), "invalid", "{extra}");
        let hint = err.hint().unwrap_or_default().to_owned();
        assert!(hint.contains(hint_has), "{extra}: {hint}");
    }
    assert_eq!(book.log.canvas().items(P1).count(), 0, "nothing was added");
}

#[test]
fn a_file_with_figure_keys_it_cannot_read_keeps_the_item_free() {
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
            "locked": "yes", "fig": "Not A Name", "part": "q1"}),
        )
        + &line(
            2,
            json!({"kind": "box", "color": "#000000", "x": 0, "y": 0, "w": 10, "h": 10,
            "locked": 1, "fig": 7, "part": 2}),
        )
        + &line(
            3,
            json!({"kind": "box", "color": "#000000", "x": 0, "y": 0, "w": 10, "h": 10,
            "part": "q1"}),
        );
    let (log, _) = Log::open(bytes.as_bytes()).unwrap();
    assert_eq!(log.canvas().items(P1).count(), 3, "every item is there");
    for item in log.canvas().items(P1) {
        assert_eq!(item.member(), &Member::default(), "{:?}", item.id());
    }
    assert_eq!(log.canvas().figures(), []);
}

#[test]
fn lock_and_unlock_is_a_set_by_the_maker_or_the_user_and_undo_puts_it_back() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    let unlock = json!({"op": "set", "id": made[1], "fields": {"locked": null}});
    let w = book.log.apply(&claude(), AT, &unlock).unwrap();
    book.bytes.push_str(&w.bytes);
    let keys = [r#""locked":null"#, r#""fig":"worksheet""#, r#""part":"q1""#];
    assert!(keys.iter().all(|k| w.bytes.contains(k)), "a set writes every figure key: {}", w.bytes);
    let free = Member { locked: false, ..book.member(&made[0]) };
    assert_eq!(book.member(&made[1]), Member { part: Some("q1".into()), ..free.clone() });

    let undone = book.log.undo(&claude(), AT);
    book.bytes.push_str(&undone.bytes);
    assert!(book.member(&made[1]).locked, "undo locks it again");
    assert!(book.reopened().canvas().item(id(&made[1])).unwrap().member().locked);

    let lock = json!({"op": "set", "id": made[4], "fields": {"locked": true}});
    book.apply_as(&Author::User, &lock);
    assert!(book.member(&made[4]).locked, "the user locks an agent's item");
    assert_eq!(book.member(&made[4]).fig.as_deref(), Some("worksheet"), "the figure stays");

    // Locked is for the user's hands only: the author rule is unchanged.
    let shape = json!({"op": "set", "id": made[1], "fields": {"w": 2400}});
    book.apply_as(&claude(), &shape);
    assert_eq!(book.log.canvas().item(id(&made[1])).unwrap().bounds().w, 240.0);
    assert!(book.member(&made[1]).locked, "a set that names no figure key keeps it");
    let other = Author::Agent("other".into());
    let err = book.log.apply(&other, AT, &unlock).unwrap_err();
    assert_eq!(err.code(), "invalid", "another agent may not unlock claude's item");

    let bad = json!({"op": "set", "id": made[5], "fields": {"part": "q1"}});
    let err = book.log.apply(&Author::User, AT, &bad).unwrap_err();
    assert_eq!(err.code(), "invalid", "a part needs a figure");
}

#[test]
fn an_undo_of_a_plain_edit_writes_no_figure_key_and_an_erase_keeps_it() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    book.apply_as(&claude(), &json!({"op": "move", "ids": [made[1]], "dx": 10, "dy": 0}));
    let undone = book.log.undo(&claude(), AT);
    book.bytes.push_str(&undone.bytes);
    assert!(!undone.bytes.contains("\"fig\""), "{}", undone.bytes);

    let before = book.member(&made[2]);
    let removed = book.log.remove(&Author::User, AT, &made[2]).unwrap();
    book.bytes.push_str(&removed.bytes);
    let restored = book.log.undo(&Author::User, AT);
    book.bytes.push_str(&restored.bytes);
    assert!(restored.bytes.contains(r#""part":"1a-answer""#), "{}", restored.bytes);
    assert_eq!(book.reopened().canvas().item(id(&made[2])).unwrap().member(), &before);
}

/// Every figure key of a file taken out: what a reader from before figures
/// reads, since it ignores fields it does not know.
fn without_figures(bytes: &str) -> String {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for key in ["locked", "fig", "part"] {
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

/// docs/design/figures.md, "What an old reader does with new fields": a
/// file with figures, locked, unlocked, moved and undone, read as a build
/// from before them reads it, keeps every item at the same place, free.
#[test]
fn a_reader_from_before_figures_loses_no_item() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    book.apply_as(&claude(), &json!({"op": "set", "id": made[0], "fields": {"locked": null}}));
    book.apply_as(&claude(), &json!({"op": "move", "ids": [made[2]], "dx": 500, "dy": 500}));
    let undone = book.log.undo(&claude(), AT);
    book.bytes.push_str(&undone.bytes);
    let full = book.reopened();
    let (old, _) = Log::open(without_figures(&book.bytes).as_bytes()).unwrap();
    let places = |log: &Log| -> Vec<(ItemId, Rect)> {
        log.canvas().items(P1).map(|i| (i.id(), i.bounds())).collect()
    };
    assert_eq!(places(&old), places(&full));
    assert_eq!(places(&old).len(), 6);
    assert!(old.canvas().items(P1).all(|i| i.member() == &Member::default()));
}

#[test]
fn a_canvas_lists_its_figures_with_their_parts_in_reading_order() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    book.apply_as(
        &Author::User,
        &add(json!({"kind": "box", "x": 0, "y": 5000, "w": 100, "h": 100, "fig": "notes"})),
    );
    let canvas = book.log.canvas();
    let figures = canvas.figures();
    let names: Vec<&str> = figures.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["worksheet", "notes"]);
    let sheet = &figures[0];
    assert_eq!(sheet.maker, claude());
    assert_eq!(sheet.page, P1);
    assert!(sheet.locked);
    assert_eq!(sheet.frame, Rect::new(0.0, 0.0, 340.0, 320.0));
    let parts: Vec<&str> = sheet.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(parts, ["title", "q1", "1a-answer"]);
    assert_eq!(sheet.parts[2].frame, Rect::new(0.0, 200.0, 200.0, 120.0));
    assert_eq!(sheet.parts[2].items, [id(&made[2]), id(&made[3])]);
    assert_eq!(sheet.items.len(), 5);
    assert!(!figures[1].locked);
    assert_eq!(canvas.figure("worksheet").as_ref(), Some(sheet));
    assert_eq!(canvas.figure("nothing"), None);
}

#[test]
fn an_address_names_a_figure_or_one_part_of_it() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    let canvas = book.log.canvas();
    let address = Address::parse("worksheet/1a-answer").unwrap();
    assert_eq!(address.to_string(), "worksheet/1a-answer");
    assert_eq!(canvas.part(&address), [id(&made[2]), id(&made[3])]);
    assert_eq!(canvas.part(&Address::parse("worksheet").unwrap()).len(), 5);
    assert_eq!(canvas.part(&Address::parse("worksheet/none").unwrap()), []);
    assert_eq!(Address::parse("b4").unwrap_err().code(), "invalid");
}

#[test]
fn put_beside_and_crop_take_a_figure_address() {
    let mut book = Book::new();
    worksheet(&mut book);
    // put --beside a part goes beside the part's joint frame.
    let put = book.log.put(&claude(), AT, "worksheet/1a-answer", &Put::Text("ok".into())).unwrap();
    assert!(put.frame[0] >= 2000, "right of the answer area: {:?}", put.frame);
    assert!((2000..3200).contains(&put.frame[1]), "{:?}", put.frame);
    // crop --item a figure or a part draws all of it.
    assert_eq!(book.log.crop("worksheet", "w").unwrap().len(), 1);
    assert_eq!(book.log.crop("worksheet/q1", "q").unwrap().len(), 1);
}

#[test]
fn an_edit_of_a_part_of_several_items_moves_them_as_one() {
    let mut book = Book::new();
    let made = worksheet(&mut book);
    let to = Edit { to: Some([500, 2500]), ..Edit::default() };
    let placed = book.log.edit(&claude(), AT, "worksheet/1a-answer", &to).unwrap();
    assert_eq!(placed.frame[..2], [500, 2500]);
    let at = |item: &str| book.log.canvas().item(id(item)).unwrap().bounds();
    assert_eq!((at(&made[2]).x, at(&made[2]).y), (50.0, 250.0));
    assert_eq!((at(&made[3]).x, at(&made[3]).y), (50.0, 370.0));
    let words = Edit { text: Some("new".into()), ..Edit::default() };
    let err = book.log.edit(&claude(), AT, "worksheet/1a-answer", &words).unwrap_err();
    assert_eq!(err.code(), "invalid");
    // A part of one item edits as its id does.
    let words = Edit { text: Some("Compound growth".into()), ..Edit::default() };
    book.log.edit(&claude(), AT, "worksheet/title", &words).unwrap();
}

#[test]
fn an_address_that_names_nothing_or_is_not_one_fails_with_a_hint() {
    let mut book = Book::new();
    worksheet(&mut book);
    let err = book.log.crop("nothing", "n").unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().unwrap_or_default().contains("canvas show"));
    assert_eq!(book.log.crop("worksheet/none", "n").unwrap_err().code(), "not_found");
    let err = book.log.crop("Not a name", "n").unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().unwrap_or_default().contains("worksheet/1a-answer"));
}

#[test]
fn remove_takes_a_whole_figure_as_one_undo_step() {
    let mut book = Book::new();
    worksheet(&mut book);
    let other = Author::Agent("other".into());
    assert_eq!(book.log.remove(&other, AT, "worksheet").unwrap_err().code(), "invalid");
    book.log.remove(&claude(), AT, "worksheet").unwrap();
    assert_eq!(book.log.canvas().figure("worksheet"), None);
    assert_eq!(book.log.canvas().items(P1).count(), 1, "the user's box stays");
    book.log.undo(&claude(), AT);
    assert_eq!(book.log.canvas().figure("worksheet").unwrap().items.len(), 5);
    assert_eq!(book.log.remove(&claude(), AT, "nothing").unwrap().bytes, "");
}

#[test]
fn show_lists_the_figures_of_each_page_so_a_new_name_can_avoid_every_one() {
    let mut book = Book::new();
    worksheet(&mut book);
    book.apply_as(&Author::User, &json!({"op": "page.add", "name": "two"}));
    book.apply_as(&Author::User, &json!({"op": "page.add", "name": "three"}));
    book.apply_as(
        &claude(),
        &json!({"op": "add", "item": {"page": "p2", "kind": "box", "x": 0, "y": 0, "w": 10,
                "h": 10, "fig": "plot"}}),
    );
    let shown = book.log.show(Some("p3")).unwrap();
    let figures: Vec<Option<&Value>> = shown.pages.iter().map(|p| p.get("figures")).collect();
    assert_eq!(figures, [Some(&json!(["worksheet"])), Some(&json!(["plot"])), None]);
}
