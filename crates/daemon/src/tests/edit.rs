//! `canvas_edit` and `canvas_remove` through the real socket: an agent
//! changes and removes what it made, and nothing else.

use serde_json::json;
use slinger_control::{CanvasEdit, CanvasOps, CanvasPut, Client, Put};

use super::Daemon;

/// A canvas `idea` with the user's ink `i1` and scout's text `i2` beside it.
fn idea(client: &Client) {
    client.canvas_create("idea").unwrap();
    let ink = json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 10, "pts": [0, 0, 100, 100] } });
    client.canvas_apply(CanvasOps { canvas: "idea".into(), by: "user".into(), ops: ink }).unwrap();
    client.canvas_put("idea", "scout", "i1", Put::Text("looks right".into())).unwrap();
}

fn edit(agent: &str, item: &str) -> CanvasEdit {
    CanvasEdit {
        canvas: "idea".into(),
        agent: agent.into(),
        item: item.into(),
        ..Default::default()
    }
}

#[test]
fn an_agent_edits_its_own_text_its_size_and_its_place() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    let text = CanvasEdit { text: Some("looks wrong".into()), ..edit("scout", "i2") };
    let edited = client.canvas_edit(text).unwrap();
    assert_eq!((edited.item.as_str(), edited.page.as_str(), edited.seq), ("i2", "p1", vec![3]));
    let moved = CanvasEdit { to: Some([500, 600]), size: Some([3000, 400]), ..edit("scout", "t2") };
    let edited = client.canvas_edit(moved.clone()).unwrap();
    assert_eq!([edited.x, edited.y, edited.w, edited.h], [500, 600, 3000, 400]);
    assert_eq!(edited.seq, vec![4]);
    // The same edit again changes nothing and writes nothing.
    assert_eq!(client.canvas_edit(moved).unwrap().seq, Vec::<u64>::new());
    let shown = client.canvas_show("idea", None).unwrap();
    let item = &shown.items[1];
    assert_eq!(
        (&item["text"], &item["x"], &item["by"]),
        (&json!("looks wrong"), &json!(500), &json!("agent:scout"))
    );
    assert_eq!(shown.next, 5);
    daemon.stop();
}

#[test]
fn a_text_size_in_tenths_puts_and_edits_and_one_out_of_range_is_invalid() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    let put = CanvasPut {
        canvas: "idea".into(),
        agent: "scout".into(),
        beside: "i1".into(),
        what: Put::Text("hi".into()),
        color: None,
        at: None,
        text_size: Some(480),
    };
    let placed = client.canvas_put_as(put.clone()).unwrap();
    assert_eq!([placed.w, placed.h], [2 * 480, 480], "the frame fits the words at the size");
    let small = CanvasEdit { text_size: Some(120), ..edit("scout", &placed.item) };
    let edited = client.canvas_edit(small).unwrap();
    assert_eq!([edited.w, edited.h], [2 * 120, 120]);
    let shown = client.canvas_show("idea", None).unwrap();
    assert_eq!(shown.items[2]["size"], json!(120));
    let next = shown.next;
    let err = client.canvas_put_as(CanvasPut { text_size: Some(4001), ..put }).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().is_some_and(|h| h.contains("40 to 4000")), "{err:?}");
    let err = client.canvas_edit(CanvasEdit { text_size: Some(39), ..edit("scout", "i2") });
    assert_eq!(err.unwrap_err().code(), "invalid");
    assert_eq!(client.canvas_show("idea", None).unwrap().next, next, "nothing written");
    daemon.stop();
}

#[test]
fn an_agent_may_not_edit_the_users_ink_or_another_agents_item() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    for (agent, item) in [("scout", "i1"), ("scout", "c1"), ("ada", "i2")] {
        let ask = CanvasEdit { to: Some([0, 0]), ..edit(agent, item) };
        let err = client.canvas_edit(ask).unwrap_err();
        assert_eq!(err.code(), "invalid", "{agent} {item}");
        assert!(err.hint().is_some_and(|h| h.contains("only items it made")), "{err:?}");
        let err = client.canvas_remove("idea", agent, item).unwrap_err();
        assert_eq!(err.code(), "invalid", "{agent} {item}");
    }
    // Text on a stroke is refused as not its own before it is refused as not text.
    let err = client.canvas_edit(CanvasEdit { text: Some("x".into()), ..edit("scout", "i1") });
    assert!(err.unwrap_err().hint().is_some_and(|h| h.contains("only items it made")));
    let code = |ask: CanvasEdit| client.canvas_edit(ask).unwrap_err().code().to_string();
    assert_eq!(code(edit("scout", "i9")), "not_found");
    assert_eq!(code(edit("scout", "zz")), "invalid");
    assert_eq!(code(edit(" ", "i2")), "invalid");
    assert_eq!(code(CanvasEdit { text: Some(" ".into()), ..edit("scout", "i2") }), "invalid");
    assert_eq!(code(CanvasEdit { size: Some([-1, 5]), ..edit("scout", "i2") }), "invalid");
    assert_eq!(client.canvas_show("idea", None).unwrap().next, 3);
    daemon.stop();
}

#[test]
fn removing_an_item_twice_succeeds_twice() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    assert_eq!(client.canvas_remove("idea", "scout", "i2").unwrap().seq, vec![3]);
    assert_eq!(client.canvas_remove("idea", "scout", "i2").unwrap().seq, Vec::<u64>::new());
    let shown = client.canvas_show("idea", None).unwrap();
    assert_eq!((shown.items.len(), shown.next), (1, 4));
    assert_eq!(client.canvas_remove("nope", "scout", "i2").unwrap_err().code(), "not_found");
    daemon.stop();
}
