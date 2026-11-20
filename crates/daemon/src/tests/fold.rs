//! `canvas_fold` and `canvas_unfold` through the real socket: a box folds
//! to its miniature and back, each one undo step; an unfold into a taken
//! room is `conflict` and writes nothing; an agent folds only its own box.

use serde_json::{Value, json};
use slinger_control::{CanvasFold, CanvasOps, CanvasStep, Client, FoldAsk};

use super::Daemon;

fn add(client: &Client, item: &Value) {
    let ops = json!({ "op": "add", "item": item });
    client
        .canvas_apply(CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: None })
        .unwrap();
}

fn boxed(x: i64, y: i64, w: i64, h: i64) -> Value {
    json!({ "page": "p1", "kind": "box", "x": x, "y": y, "w": w, "h": h })
}

/// A canvas `main` with the user's box `i1` and a small box `i2` inside it.
fn drawn(client: &Client) {
    client.canvas_create("main").unwrap();
    add(client, &boxed(0, 0, 10_000, 5000));
    add(client, &boxed(1000, 1000, 500, 500));
}

fn ask(by: &str, item: &str) -> CanvasFold {
    CanvasFold { canvas: "main".into(), item: item.into(), by: by.into(), once: None }
}

fn next(client: &Client) -> u64 {
    client.canvas_show("main", None).unwrap().next
}

#[test]
fn a_box_folds_to_its_miniature_and_unfolds_to_its_room() {
    let daemon = Daemon::start();
    let client = daemon.client();
    drawn(&client);
    let folded = client.canvas_fold(&FoldAsk::CanvasFold(ask("user", "b1"))).unwrap();
    assert_eq!((folded.item.as_str(), folded.page.as_str(), &folded.seq), ("i1", "p1", &vec![3]));
    assert_eq!((folded.x, folded.y), (0, 0));
    assert!(folded.w < 10_000 && folded.h < 5000, "{folded:?}");
    let shown = client.canvas_show("main", None).unwrap();
    assert_eq!(shown.items[0]["fold"]["items"], json!(["i2"]), "{:?}", shown.items[0]);
    // A box already folded writes nothing.
    let again = client.canvas_fold(&FoldAsk::CanvasFold(ask("user", "i1"))).unwrap();
    assert_eq!(again.seq, Vec::<u64>::new());
    let opened = client.canvas_fold_step(&FoldAsk::CanvasUnfold(ask("user", "i1"))).unwrap();
    assert_eq!((opened.seq, opened.made), (vec![4], vec![]));
    let shown = client.canvas_show("main", None).unwrap();
    assert_eq!((&shown.items[0]["w"], shown.items[0].get("fold")), (&json!(10_000), None));
    // Each is one step: undo folds it again.
    let step = CanvasStep { canvas: "main".into(), by: "user".into() };
    assert_eq!(client.canvas_undo(step).unwrap().seq.len(), 1);
    assert!(client.canvas_show("main", None).unwrap().items[0]["fold"].is_object());
    daemon.stop();
}

#[test]
fn an_unfold_into_a_taken_room_is_a_conflict_and_writes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    drawn(&client);
    client.canvas_fold(&FoldAsk::CanvasFold(ask("user", "i1"))).unwrap();
    add(&client, &boxed(8000, 3000, 500, 500));
    let before = next(&client);
    let err = client.canvas_fold(&FoldAsk::CanvasUnfold(ask("user", "i1"))).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("i3"), "{err:?}");
    assert!(err.hint().is_some());
    assert_eq!(next(&client), before);
    daemon.stop();
}

#[test]
fn an_agent_folds_only_its_own_box_and_a_bad_item_is_refused() {
    let daemon = Daemon::start();
    let client = daemon.client();
    drawn(&client);
    let err = client.canvas_fold(&FoldAsk::CanvasFold(ask("agent:scout", "i1"))).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert_eq!(
        client.canvas_fold(&FoldAsk::CanvasFold(ask("user", "i9"))).unwrap_err().code(),
        "not_found"
    );
    let other = CanvasFold { canvas: "nope".into(), ..ask("user", "i1") };
    assert_eq!(client.canvas_fold(&FoldAsk::CanvasFold(other)).unwrap_err().code(), "not_found");
    assert_eq!(
        client.canvas_fold(&FoldAsk::CanvasFold(ask("bob", "i1"))).unwrap_err().code(),
        "invalid"
    );
    daemon.stop();
}
