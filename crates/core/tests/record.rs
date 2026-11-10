//! The record's formats, through the public interface only.

use serde_json::{Value, json};
use slinger_core::record::{
    AgentBlocked, AgentDone, BlobId, Body, Event, FileTouched, KINDS, PaneMoved, ReplyLanded,
    SlingSent, Tags, VERSION, decode, encode, to_value,
};

fn tags() -> Tags {
    Tags {
        machine: "my-laptop".into(),
        workspace: Some("notes".into()),
        agent: Some("scout".into()),
        canvas: Some("c1".into()),
    }
}

fn every_kind() -> Vec<Body> {
    vec![
        Body::SlingSent(SlingSent {
            prompt: "what is this?".into(),
            packet: BlobId::of(b"packet"),
        }),
        Body::ReplyLanded(ReplyLanded { sling: 3, items: vec!["i1".into(), "i2".into()] }),
        Body::FileTouched(FileTouched {
            path: "/src/main.rs".into(),
            hash: BlobId::of(b"fn main() {}"),
        }),
        Body::PaneMoved(PaneMoved {
            pane: "p1".into(),
            x: 0.0,
            y: 12.5,
            width: 640.0,
            height: 480.0,
        }),
        Body::AgentBlocked(AgentBlocked { reason: "which file?".into() }),
        Body::AgentDone(AgentDone { summary: Some("done".into()) }),
        Body::AgentDone(AgentDone { summary: None }),
    ]
}

#[test]
fn every_kind_round_trips() {
    for (n, body) in every_kind().into_iter().enumerate() {
        let mut event = Event::new(1_762_790_400_000, "slingerd", tags(), body);
        event.number = Some(n as u64 + 1);
        let line = encode(&event);
        assert!(!line.contains('\n'));
        assert_eq!(decode(&line).unwrap(), event, "{line}");
    }
}

#[test]
fn a_new_event_has_no_number_and_the_current_version() {
    let event =
        Event::new(5, "cli", Tags::on("my-laptop"), Body::AgentDone(AgentDone { summary: None }));
    assert_eq!(event.number, None);
    assert_eq!(event.version, VERSION);
    let doc: Value = serde_json::from_str(&encode(&event)).unwrap();
    assert_eq!(
        doc,
        json!({
            "time": 5, "kind": "agent_done", "version": 1, "source": "cli",
            "tags": {"machine": "my-laptop"}, "payload": {}
        })
    );
    assert_eq!(decode(&encode(&event)).unwrap(), event);
}

#[test]
fn file_touched_holds_a_hash_not_contents() {
    let body = Body::FileTouched(FileTouched { path: "a.txt".into(), hash: BlobId::of(b"secret") });
    let line = encode(&Event::new(1, "agent", Tags::on("m"), body));
    assert!(!line.contains("secret"));
    assert!(line.contains(&BlobId::of(b"secret").to_string()));
}

#[test]
fn a_stored_event_of_an_unknown_kind_is_kept_whole() {
    let stored = include_str!("fixtures/event-from-a-newer-writer.json").trim();
    let event = decode(stored).unwrap();
    assert_eq!(event.number, Some(7));
    assert_eq!(event.version, 3);
    assert_eq!(event.tags.workspace.as_deref(), Some("notes"));
    assert_eq!(event.body.kind(), "canvas_shared");
    let Body::Other { payload, .. } = &event.body else {
        panic!("expected Other, got {:?}", event.body)
    };
    assert_eq!(payload, &json!({"canvas": "c1", "with": ["ada"]}));
    let again: Value = serde_json::from_str(&encode(&event)).unwrap();
    assert_eq!(again, serde_json::from_str::<Value>(stored).unwrap());
}

#[test]
fn a_known_kind_with_extra_fields_keeps_them() {
    let stored = include_str!("fixtures/known-kind-with-extra-field.json").trim();
    let event = decode(stored).unwrap();
    assert_eq!(event.body, Body::AgentDone(AgentDone { summary: Some("wrote the plan".into()) }));
    let again: Value = serde_json::from_str(&encode(&event)).unwrap();
    assert_eq!(again, serde_json::from_str::<Value>(stored).unwrap());
}

#[test]
fn decode_rejects_what_is_not_an_event() {
    for bad in [
        "not json",
        "[]",
        r#"{"time":1,"kind":"agent_done","version":1,"source":"x","payload":{}}"#,
        r#"{"time":1,"kind":"agent_done","version":1,"source":"x","tags":{},"payload":{}}"#,
        r#"{"time":1,"kind":"sling_sent","version":1,"source":"x","tags":{"machine":"m"},"payload":{"prompt":"p"}}"#,
        r#"{"time":1,"kind":"file_touched","version":1,"source":"x","tags":{"machine":"m"},"payload":{"path":"p","hash":"md5:00"}}"#,
    ] {
        let err = decode(bad).unwrap_err();
        assert_eq!(err.code(), "invalid", "{bad}");
        assert!(err.hint().is_some());
    }
}

#[test]
fn a_writer_gets_a_typed_body_for_every_kind() {
    for body in every_kind() {
        let doc = to_value(&Event::new(1, "cli", Tags::on("m"), body.clone()));
        assert_eq!(Body::new(body.kind(), doc["payload"].clone()).unwrap(), body);
        assert!(KINDS.contains(&body.kind()));
    }
}

#[test]
fn to_value_is_what_encode_writes() {
    let event = decode(include_str!("fixtures/known-kind-with-extra-field.json").trim()).unwrap();
    assert_eq!(to_value(&event).to_string(), encode(&event));
}

#[test]
fn a_writer_cannot_use_an_unknown_kind_or_a_payload_that_does_not_fit() {
    for (kind, payload) in [
        ("canvas_shared", json!({})),
        ("", json!({})),
        ("agent_blocked", json!({})),
        ("reply_landed", json!({"sling": "three", "items": []})),
        ("file_touched", json!({"path": "a", "hash": "md5:00"})),
        ("agent_done", json!("done")),
    ] {
        let err = Body::new(kind, payload).unwrap_err();
        assert_eq!(err.code(), "invalid", "{kind}");
        assert!(err.hint().is_some());
    }
    let err = Body::new("nope", json!({})).unwrap_err();
    assert!(err.hint().unwrap().contains("agent_done"));
}
