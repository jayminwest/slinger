//! Showings of agent UIs (docs/design/live-items.md): `ui open` shows one
//! version of a UI, with the agent's data, and each reply from it goes back
//! to that agent's session.
//!
//! This module owns what a showing is: its id, `<name>#<number>`, where the
//! number is its `ui_opened` event's (no UI reference has a `#`, so the two
//! never mix); the limits on its data and its replies; and the line the
//! agent's session receives for each reply. No IO: the daemon hands it the
//! events.

use serde::Serialize;
use serde_json::Value;

use super::{Version, check_name};
use crate::record::{BlobId, Body, Event, UiOpened, UiReply};
use crate::{Error, Result};

/// The most bytes of data a UI is given, serialized.
pub const MAX_DATA: usize = 16384;

/// The most bytes of one reply, serialized.
pub const MAX_REPLY: usize = 16384;

/// The most replies one showing sends.
pub const MAX_REPLIES: usize = 100;

/// Where a UI shows. A canvas item comes with live items (slinger-125a).
const HOSTS: [&str; 1] = ["pane"];

/// One showing of a UI, as its `ui_opened` event records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Showing {
    /// Its id: `<name>#<number>`.
    pub open: String,
    pub name: String,
    pub version: u64,
    pub hash: BlobId,
    pub host: String,
    /// The agent's data, `null` for none.
    pub data: Value,
    /// The agent that opened it, which its replies go to.
    pub agent: String,
    /// The workspace tag it was opened in.
    pub workspace: Option<String>,
}

impl Showing {
    /// The showing a `ui_opened` event records, or `None` for any other
    /// event or one not yet numbered.
    #[must_use]
    pub fn of(event: &Event) -> Option<Self> {
        let Body::UiOpened(opened) = &event.body else { return None };
        Some(Self {
            open: format!("{}#{}", opened.name, event.number?),
            name: opened.name.clone(),
            version: opened.version,
            hash: opened.hash,
            host: opened.host.clone(),
            data: opened.data.clone(),
            agent: event.tags.agent.clone().unwrap_or_else(|| event.source.clone()),
            workspace: event.tags.workspace.clone(),
        })
    }

    /// How many replies of this showing `events` hold.
    #[must_use]
    pub fn sent(&self, events: &[Event]) -> usize {
        let mine = |e: &&Event| matches!(&e.body, Body::UiReply(r) if r.open == self.open);
        events.iter().filter(mine).count()
    }

    /// The `ui_reply` payload that records `value`, once this showing has
    /// sent `sent` replies.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a value over [`MAX_REPLY`] bytes serialized,
    /// or once the showing sent [`MAX_REPLIES`].
    pub fn reply(&self, sent: usize, value: Value) -> Result<UiReply> {
        let size = value.to_string().len();
        if size > MAX_REPLY {
            return Err(invalid(
                format!("the reply is {size} bytes, over {MAX_REPLY}"),
                "send less: an id or a summary, not the page",
            ));
        }
        if sent >= MAX_REPLIES {
            return Err(invalid(
                format!(
                    "{} already sent {MAX_REPLIES} replies, the most a showing sends",
                    self.open
                ),
                "open the UI again with 'slinger ui open' to send more",
            ));
        }
        Ok(UiReply {
            open: self.open.clone(),
            name: self.name.clone(),
            version: self.version,
            hash: self.hash,
            value,
        })
    }
}

/// The `ui_opened` payload that shows `version` in `host` with `data`.
///
/// # Errors
///
/// [`Error::Invalid`] for a host that is not `pane`, or data over
/// [`MAX_DATA`] bytes serialized.
pub fn open(version: &Version, host: &str, data: Value) -> Result<UiOpened> {
    if !HOSTS.contains(&host) {
        return Err(invalid(
            format!("a UI cannot show as {host:?}"),
            "open it as a pane: --as pane",
        ));
    }
    let size = data.to_string().len();
    if size > MAX_DATA {
        return Err(invalid(
            format!("the data is {size} bytes, over {MAX_DATA}"),
            "give the UI less: what it shows, not what it could look up",
        ));
    }
    Ok(UiOpened {
        name: version.name.clone(),
        version: version.version,
        hash: version.hash,
        host: host.to_string(),
        data,
    })
}

/// The number of the `ui_opened` event that the showing id `target` names,
/// or `None` when `target` is not a showing's id (a UI reference is not).
#[must_use]
pub fn number(target: &str) -> Option<u64> {
    let (name, number) = target.split_once('#')?;
    check_name(name).ok()?;
    if !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

/// The line the agent's session receives for `reply`: one JSON object,
/// `{"v":1,"kind":"ui_reply","ui","version","hash","open","value"}`.
///
/// # Panics
///
/// Never: the line holds only string map keys.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "the line holds only string map keys, so serializing cannot fail"
)]
pub fn line(reply: &UiReply) -> String {
    #[derive(Serialize)]
    struct Line<'a> {
        v: u32,
        kind: &'static str,
        ui: &'a str,
        version: u64,
        hash: BlobId,
        open: &'a str,
        value: &'a Value,
    }
    let line = Line {
        v: 1,
        kind: "ui_reply",
        ui: &reply.name,
        version: reply.version,
        hash: reply.hash,
        open: &reply.open,
        value: &reply.value,
    };
    serde_json::to_string(&line).expect("a line serializes")
}

fn invalid(message: String, hint: &str) -> Error {
    Error::Invalid { message, hint: Some(hint.into()) }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::record::Tags;

    fn version() -> Version {
        Version {
            name: "pick-slot".into(),
            version: 3,
            hash: BlobId::of(b"<p>"),
            bytes: 3,
            source: "scout".into(),
            time: 1,
            number: 4,
        }
    }

    fn opened(number: u64, data: Value) -> Event {
        let body = Body::UiOpened(open(&version(), "pane", data).unwrap());
        let tags =
            Tags { agent: Some("scout".into()), workspace: Some("notes".into()), ..Tags::on("m") };
        let mut event = Event::new(9, "scout", tags, body);
        event.number = Some(number);
        event
    }

    #[test]
    fn a_showing_is_its_event_named_by_name_and_number() {
        let showing = Showing::of(&opened(12, json!({"slots": ["Tue"]}))).unwrap();
        assert_eq!(showing.open, "pick-slot#12");
        assert_eq!((showing.version, showing.host.as_str()), (3, "pane"));
        assert_eq!(
            (showing.agent.as_str(), showing.workspace.as_deref()),
            ("scout", Some("notes"))
        );
        assert_eq!(showing.data, json!({"slots": ["Tue"]}));
        let mut unnumbered = opened(1, Value::Null);
        unnumbered.number = None;
        assert_eq!(Showing::of(&unnumbered), None);
        let other =
            Event::new(1, "x", Tags::on("m"), Body::Other { kind: "k".into(), payload: 0.into() });
        assert_eq!(Showing::of(&other), None);
    }

    #[test]
    fn an_id_names_a_number_and_a_reference_names_none() {
        assert_eq!(number("pick-slot#12"), Some(12));
        for not in [
            "pick-slot",
            "pick-slot@3",
            "pick-slot#",
            "pick-slot#1a",
            "Pick#1",
            "#1",
            "a#-1",
            "sha256:00",
        ] {
            assert_eq!(number(not), None, "{not}");
        }
    }

    #[test]
    fn data_and_hosts_are_bounded() {
        let big = json!("x".repeat(MAX_DATA));
        assert_eq!(open(&version(), "pane", big).unwrap_err().code(), "invalid");
        let fits = json!("x".repeat(MAX_DATA - 2));
        assert!(open(&version(), "pane", fits).is_ok());
        let err = open(&version(), "canvas", Value::Null).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().is_some());
    }

    #[test]
    fn replies_are_bounded_counted_per_showing_and_reach_the_agent_as_one_line() {
        let showing = Showing::of(&opened(12, Value::Null)).unwrap();
        let reply = showing.reply(0, json!({"slot": "Wed"})).unwrap();
        assert_eq!(
            line(&reply),
            format!(
                r#"{{"v":1,"kind":"ui_reply","ui":"pick-slot","version":3,"hash":"{}","open":"pick-slot#12","value":{{"slot":"Wed"}}}}"#,
                BlobId::of(b"<p>")
            )
        );
        assert!(showing.reply(MAX_REPLIES - 1, Value::Null).is_ok());
        assert_eq!(showing.reply(MAX_REPLIES, Value::Null).unwrap_err().code(), "invalid");
        let big = json!("x".repeat(MAX_REPLY));
        assert_eq!(showing.reply(0, big).unwrap_err().code(), "invalid");

        let tags = Tags::on("m");
        let mine = Event::new(1, "user", tags.clone(), Body::UiReply(reply.clone()));
        let other = UiReply { open: "pick-slot#13".into(), ..reply };
        let theirs = Event::new(1, "user", tags, Body::UiReply(other));
        assert_eq!(showing.sent(&[mine.clone(), theirs, mine]), 2);
    }
}
