//! A canvas file held open (`canvas::file::Log`), through its public interface:
//! requests in, the bytes to append out, and the file they make read back.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Log, check_id, parse_author};
use slinger_core::canvas::{Author, Time};

const AT: Time = Time(1_762_790_400_000);

fn ada() -> Author {
    Author::Agent("ada".into())
}

fn bx(x: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": x, "y": 0, "w": 100, "h": 50 } })
}

/// A new canvas and its file so far.
fn new_canvas() -> (Log, String) {
    Log::create("main").unwrap()
}

/// Applies `ops` and appends what they wrote to `file`.
fn apply(log: &mut Log, file: &mut String, by: &Author, ops: &Value) -> Vec<String> {
    let written = log.apply(by, AT, ops).unwrap();
    file.push_str(&written.bytes);
    written.made
}

/// What a page holds, as `show` gives it.
fn items(log: &Log) -> Vec<Value> {
    log.show(None).unwrap().items
}

#[test]
fn a_change_is_numbered_lines_that_rebuild_the_canvas() {
    let (mut log, mut file) = new_canvas();
    assert_eq!(file, "{\"slinger\":\"canvas\",\"v\":1,\"id\":\"main\",\"first\":1}\n");
    let written = log.apply(&Author::User, AT, &json!([bx(0), bx(500)])).unwrap();
    assert_eq!((written.seq, written.made), (vec![1, 2], vec!["i1".into(), "i2".into()]));
    assert_eq!(written.bytes.lines().count(), 2);
    file.push_str(&written.bytes);
    let made =
        apply(&mut log, &mut file, &Author::User, &json!({ "op": "page.add", "name": "two" }));
    assert_eq!(made, ["p2"]);
    assert_eq!(log.next(), 4);

    let (reopened, torn) = Log::open(file.as_bytes()).unwrap();
    assert_eq!(torn, 0);
    assert_eq!(reopened.next(), 4);
    assert_eq!(reopened.canvas(), log.canvas());
    assert_eq!(items(&reopened)[1]["x"], 500);
}

#[test]
fn a_change_that_fails_writes_nothing_and_changes_nothing() {
    let (mut log, _) = new_canvas();
    for bad in [
        json!([bx(0), { "op": "fly" }]),
        json!([bx(0), { "op": "add", "item": { "page": "p9", "kind": "box", "x": 0, "y": 0, "w": 1, "h": 1 } }]),
        json!([bx(0), { "op": "add", "item": { "page": "p1", "kind": "cloud" } }]),
        json!("not an op"),
        json!({ "op": "move", "ids": ["i1"], "dx": 1.5, "dy": 0 }),
    ] {
        let err = log.apply(&Author::User, AT, &bad).unwrap_err();
        assert!(matches!(err.code(), "invalid" | "not_found"), "{bad}: {err}");
        assert!(err.hint().is_some(), "{bad}");
    }
    assert_eq!(items(&log), Vec::<Value>::new());
    assert_eq!(log.next(), 1);
}

#[test]
fn an_agent_changes_only_what_it_made() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &bx(0));
    apply(&mut log, &mut file, &ada(), &bx(500));
    let err = log.apply(&ada(), AT, &json!({ "op": "remove", "ids": ["i1"] })).unwrap_err();
    assert_eq!(err.code(), "invalid");
    let moved = log.apply(&ada(), AT, &json!({ "op": "move", "ids": ["i2"], "dx": 15, "dy": -5 }));
    file.push_str(&moved.unwrap().bytes);
    let shown = items(&Log::open(file.as_bytes()).unwrap().0);
    assert_eq!(
        (&shown[1]["x"], &shown[1]["y"], &shown[1]["by"]),
        (&json!(515), &json!(-5), &json!("agent:ada"))
    );
}

#[test]
fn removing_what_is_gone_succeeds_and_writes_nothing() {
    let (mut log, _) = new_canvas();
    let written = log.apply(&Author::User, AT, &json!({ "op": "remove", "ids": ["i7"] })).unwrap();
    assert!(written.bytes.is_empty() && written.seq.is_empty());
    assert_eq!(log.apply(&Author::User, AT, &json!([])).unwrap().seq, [0_u64; 0]);
}

#[test]
fn an_arrow_added_with_items_points_at_them_by_their_place_in_made() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &json!([bx(0), { "op": "remove", "ids": ["i1"] }]));
    let arrow = |from: &str, to: &str| {
        let item = json!({ "page": "p1", "kind": "arrow", "pts": [100, 25, 300, 0], "from": from, "to": to });
        json!({ "op": "add", "item": item })
    };
    let page = json!({ "op": "page.add", "name": "two" });
    let made = apply(
        &mut log,
        &mut file,
        &Author::User,
        &json!([bx(0), page, bx(400), arrow("$0", "$2")]),
    );
    assert_eq!(made, ["i2", "p2", "i3", "i4"]);
    let shown = items(&Log::open(file.as_bytes()).unwrap().0);
    assert_eq!((&shown[2]["from"], &shown[2]["to"]), (&json!("i2"), &json!("i3")));

    let before = log.next();
    let page = json!({ "op": "page.add", "name": "three" });
    for (from, to) in [("$1", "$0"), ("$0", "$5"), ("$x", "$0"), ("$0", "$2")] {
        // A page, nothing made, not a number, the arrow itself.
        let ops = json!([bx(0), page, arrow(from, to)]);
        let err = log.apply(&Author::User, AT, &ops).unwrap_err();
        assert_eq!(err.code(), "invalid", "{from} {to}: {err}");
    }
    assert_eq!(log.next(), before);
}

#[test]
fn set_lays_fields_over_the_item() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &bx(0));
    let set = json!({ "op": "set", "id": "i1", "fields": { "w": 300, "color": "#ff0000" } });
    apply(&mut log, &mut file, &Author::User, &set);
    let item = &items(&Log::open(file.as_bytes()).unwrap().0)[0];
    assert_eq!(
        (&item["w"], &item["h"], &item["color"]),
        (&json!(300), &json!(50), &json!("#ff0000"))
    );
    let gone = json!({ "op": "set", "id": "i9", "fields": {} });
    assert_eq!(log.apply(&Author::User, AT, &gone).unwrap_err().code(), "not_found");
}

#[test]
fn undo_and_redo_append_lines_and_are_per_author() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &bx(0));
    apply(&mut log, &mut file, &ada(), &bx(500));
    let undone = log.undo(&Author::User, AT);
    assert_eq!(undone.seq, [3]);
    file.push_str(&undone.bytes);
    assert_eq!(items(&log).len(), 1);
    assert_eq!(items(&log)[0]["by"], "agent:ada");
    let redone = log.redo(&Author::User, AT);
    assert_eq!(redone.seq, [4]);
    file.push_str(&redone.bytes);
    assert_eq!(log.redo(&Author::User, AT).bytes, "");
    assert_eq!(log.undo(&Author::Agent("bob".into()), AT).seq, [0_u64; 0]);

    let reopened = Log::open(file.as_bytes()).unwrap().0;
    assert_eq!(reopened.canvas(), log.canvas());
    assert_eq!(reopened.next(), 5);
}

#[test]
fn show_picks_a_page_by_id_or_name() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &json!({ "op": "page.add", "name": "notes" }));
    let add = json!({ "op": "add", "item": { "page": "p2", "kind": "text", "x": 0, "y": 0, "text": "hi" } });
    apply(&mut log, &mut file, &Author::User, &add);
    for page in ["p2", "notes"] {
        let shown = log.show(Some(page)).unwrap();
        assert_eq!((shown.page.as_str(), shown.items.len()), ("p2", 1), "{page}");
        assert_eq!(shown.items[0]["color"], "#000000");
    }
    let shown = log.show(None).unwrap();
    assert_eq!(shown.page, "p1");
    assert_eq!(
        shown.pages,
        [json!({ "id": "p1", "name": "Page 1" }), json!({ "id": "p2", "name": "notes" })]
    );
    assert_eq!(log.show(Some("p3")).unwrap_err().code(), "not_found");
}

#[test]
fn a_torn_last_line_is_counted_for_the_caller_to_cut() {
    let (mut log, mut file) = new_canvas();
    apply(&mut log, &mut file, &Author::User, &bx(0));
    file.push_str("{\"n\":2,\"t\":");
    let (reopened, torn) = Log::open(file.as_bytes()).unwrap();
    assert_eq!((torn, reopened.next()), (11, 2));
}

#[test]
fn canvas_ids_name_a_file_and_never_a_path() {
    for id in ["main", "a", "idea-2.v1_x", &"x".repeat(64)] {
        assert!(check_id(id).is_ok(), "{id}");
    }
    for id in ["", ".hidden", "../x", "a/b", "Main", "a b", "é", &"x".repeat(65)] {
        let err = check_id(id).unwrap_err();
        assert_eq!(err.code(), "invalid", "{id}");
        assert!(err.hint().is_some());
    }
    assert_eq!(Log::create("../x").unwrap_err().code(), "invalid");
}

#[test]
fn authors_are_the_user_or_a_named_agent() {
    assert_eq!(parse_author("user").unwrap(), Author::User);
    assert_eq!(parse_author("agent:ada").unwrap(), ada());
    for bad in ["", "agent:", "agent: ", "robot"] {
        assert_eq!(parse_author(bad).unwrap_err().code(), "invalid", "{bad}");
    }
}

#[test]
fn a_change_sent_again_under_its_key_is_written_once_and_says_the_same() {
    use slinger_core::canvas::file::ONCES;
    let (mut log, mut file) = new_canvas();
    let first = log.apply_once(&Author::User, AT, &bx(0), "app.1").unwrap();
    file.push_str(&first.bytes);
    assert_eq!(
        (first.seq.as_slice(), first.made.as_slice()),
        ([1].as_slice(), ["i1".into()].as_slice())
    );
    let again = log.apply_once(&Author::User, AT, &bx(0), "app.1").unwrap();
    assert_eq!((again.bytes.as_str(), &again.seq, &again.made), ("", &first.seq, &first.made));
    assert_eq!(items(&log).len(), 1, "not applied twice");
    assert!(log.apply_once(&Author::User, AT, &json!({ "op": "fly" }), "app.2").is_err());
    let second = log.apply_once(&Author::User, AT, &bx(500), "app.2").unwrap();
    assert_eq!(second.made, ["i2"], "a key whose change failed may go again");
    for n in 0..ONCES {
        log.apply_once(&Author::User, AT, &bx(0), &format!("more.{n}")).unwrap();
    }
    let forgotten = log.apply_once(&Author::User, AT, &bx(0), "app.1").unwrap();
    assert!(!forgotten.bytes.is_empty(), "only the last {ONCES} keys are kept");
}
