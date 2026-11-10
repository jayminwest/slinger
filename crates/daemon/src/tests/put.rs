//! `canvas_put` through the real socket: the item lands beside the one named,
//! as the agent's, and a put that answers a sling lands in the record.

use serde_json::{Value, json};
use slinger_control::{CanvasOps, Client, EventFilter, NewEvent, NewTags, Put};

use super::Daemon;

/// A canvas `idea` with one box of the user's, `i1`.
fn idea(client: &Client) {
    client.canvas_create("idea").unwrap();
    let bx = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    client.canvas_apply(CanvasOps { canvas: "idea".into(), by: "user".into(), ops: bx }).unwrap();
}

fn landed(client: &Client) -> Vec<Value> {
    let filter = EventFilter { kind: Some("reply_landed".into()), ..EventFilter::default() };
    client.record_list(filter).unwrap()
}

#[test]
fn a_put_text_shows_on_the_canvas_with_its_author_beside_the_item() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    let placed = client.canvas_put("idea", "scout", "i1", Put::Text("looks right".into())).unwrap();
    assert_eq!((placed.item.as_str(), placed.page.as_str()), ("i2", "p1"));
    assert_eq!([placed.x, placed.y, placed.w, placed.h], [1240, 0, 2640, 240]);
    assert_eq!((placed.seq, placed.event), (2, None));
    let shown = client.canvas_show("idea", None).unwrap();
    let item = &shown.items[1];
    assert_eq!(
        (&item["id"], &item["kind"], &item["by"]),
        (&json!("i2"), &json!("text"), &json!("agent:scout"))
    );
    assert_eq!((&item["text"], &item["x"]), (&json!("looks right"), &json!(1240)));
    assert!(item["t"].as_str().is_some_and(|t| !t.is_empty()), "{item}");
    // Nothing was slung to scout, so the put answers no sling.
    assert_eq!(landed(&client), Vec::<Value>::new());
    daemon.stop();
}

#[test]
fn a_put_after_a_sling_to_the_agent_lands_a_reply_in_the_record() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    let packet = client.canvas_read(slinger_control::PacketAsk {
        canvas: "idea".into(),
        page: None,
        after: None,
        select: Vec::new(),
        out: None,
    });
    let sling = |agent: &str| {
        let tags = NewTags {
            agent: Some(agent.into()),
            canvas: Some("idea".into()),
            ..NewTags::default()
        };
        let payload = json!({ "prompt": "check", "packet": packet.as_ref().unwrap().packet });
        let new = NewEvent { kind: "sling_sent".into(), source: "slinger".into(), tags, payload };
        client.record_append(new).unwrap()["number"].as_u64().unwrap()
    };
    let (to_scout, _) = (sling("scout"), sling("ada"));
    let placed = client.canvas_put("idea", "scout", "i1", Put::Box([300, 200])).unwrap();
    let events = landed(&client);
    assert_eq!(events.len(), 1);
    assert_eq!(placed.event, events[0]["number"].as_u64());
    assert_eq!(events[0]["payload"], json!({ "sling": to_scout, "items": ["i2"] }));
    assert_eq!(
        (&events[0]["source"], &events[0]["tags"]["agent"]),
        (&json!("scout"), &json!("scout"))
    );
    assert_eq!(events[0]["tags"]["canvas"], "idea");
    let arrow = Put::Arrow(["i1".into(), "i2".into()]);
    let placed = client.canvas_put("idea", "scout", "i1", arrow).unwrap();
    assert_eq!(placed.item, "i3");
    assert_eq!(landed(&client).len(), 2);
    daemon.stop();
}

#[test]
fn a_put_beside_an_unknown_item_or_by_no_agent_fails_and_changes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    idea(&client);
    let text = || Put::Text("hi".into());
    let code = |canvas: &str, agent: &str, beside: &str| {
        client.canvas_put(canvas, agent, beside, text()).unwrap_err().code().to_string()
    };
    assert_eq!(code("idea", "scout", "i9"), "not_found");
    assert_eq!(code("nope", "scout", "i1"), "not_found");
    assert_eq!(code("idea", "scout", "zz"), "invalid");
    assert_eq!(code("idea", " ", "i1"), "invalid");
    assert_eq!(client.canvas_show("idea", None).unwrap().next, 2);
    daemon.stop();
}
