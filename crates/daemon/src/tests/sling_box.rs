//! A sling of a box through the real socket (docs/design/fold.md, "Sling a
//! box"): its packet holds only what the box holds and names the box, and
//! the agent's answer with `in` lands in the user's folded box.

use std::time::Duration;

use serde_json::{Value, json};
use slinger_control::{CanvasFold, CanvasOps, CanvasPut, Client, FoldAsk, Put, SlingAsk};
use tempfile::TempDir;

use super::Daemon;
use super::session::open_with_agent;

fn add(client: &Client, item: &Value) {
    let ops = json!({ "op": "add", "item": item });
    let ops = CanvasOps { canvas: "notes".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(ops).unwrap();
}

fn stroke(x: i64, y: i64) -> Value {
    json!({ "page": "p1", "kind": "stroke", "width": 20, "pts": [x, y, 100, 50, 100, -50] })
}

#[test]
fn a_sling_of_a_folded_box_sends_only_its_contents_and_the_answer_lands_in_it() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    // The user's box i1 holds the stroke i2; the stroke i3 is outside it.
    add(&client, &json!({ "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 4000, "h": 2000 }));
    add(&client, &stroke(500, 500));
    add(&client, &stroke(9000, 500));
    let fold =
        CanvasFold { canvas: "notes".into(), item: "b1".into(), by: "user".into(), once: None };
    client.canvas_fold(&FoldAsk::CanvasFold(fold)).unwrap();

    // Nobody listens and no process runs: the sling waits, as text.
    client.session_stop("notes").unwrap();
    let ask = SlingAsk {
        canvas: "notes".into(),
        inside: Some("b1".into()),
        prompt: "what is this?".into(),
        ..SlingAsk::default()
    };
    client.sling_send(ask).unwrap();
    let taken = client.session_listen("notes", Duration::from_secs(30)).unwrap().sling.unwrap();
    let text = taken.text;
    assert!(text.starts_with("slinger-sling 1 canvas=notes agent=scout in=b1\n"), "{text}");
    assert!(text.lines().nth(1).unwrap().contains(" in=b1 "), "{text}");
    assert!(text.contains("c2 ink") && !text.contains("c3 ink"), "only the box's contents: {text}");
    assert!(text.contains("add `--in b1` to each `canvas put`"), "{text}");

    // The answer goes in the user's folded box, and answers the sling.
    let put = CanvasPut {
        canvas: "notes".into(),
        agent: "scout".into(),
        beside: "c2".into(),
        what: Put::Text("a tick".into()),
        color: None,
        at: None,
        text_size: None,
        inside: Some("b1".into()),
    };
    let placed = client.canvas_put_as(put.clone()).unwrap();
    assert!(placed.event.is_some(), "it answers the sling");
    let shown = client.canvas_show("notes", None).unwrap();
    assert_eq!(shown.items[0]["fold"]["items"], json!(["i2", placed.item]));
    // A box that is not there is refused, and nothing is put.
    let err = client.canvas_put_as(CanvasPut { inside: Some("b9".into()), ..put }).unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
    daemon.stop();
}

#[test]
fn the_backdrops_of_a_box_sling_are_only_under_the_ink_it_holds() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    // A PDF page i1 under the box i2, which holds the stroke i3; i4 is outside.
    let hash = format!("sha256:{}", "9f".repeat(32));
    add(
        &client,
        &json!({ "page": "p1", "kind": "pdf", "x": 0, "y": 0, "w": 16000, "h": 20706,
        "hash": hash, "pdfpage": 1, "pagesize": [6120, 7920], "name": "paper.pdf" }),
    );
    add(&client, &json!({ "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 4000, "h": 2000 }));
    add(&client, &stroke(500, 500));
    add(&client, &stroke(9000, 500));
    assert_eq!(client.sling_backdrops("notes", None, None).unwrap().len(), 2, "the page");
    let [area] = client.sling_backdrops("notes", None, Some("b2")).unwrap().try_into().unwrap();
    assert_eq!(area.item, "i1");
    assert!(area.area[0] < 4000, "the area under the stroke in the box: {:?}", area.area);
    let err = client.sling_backdrops("notes", None, Some("b9")).unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
    daemon.stop();
}
