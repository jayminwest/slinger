//! The record's data formats: events and blob ids (SPEC section 9, D14).
//!
//! This module owns how an event is written as JSON and how a blob is named.
//! It does no IO: the daemon stores what [`encode`] writes and hands what it
//! reads to [`decode`].
//!
//! The formats are API (B5). A reader keeps an event of a kind it does not
//! know, and it keeps fields it does not know: [`encode`] writes them back as
//! they came, so a copy through an older reader loses nothing.

mod blob;
mod body;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use blob::BlobId;
pub use body::{
    AgentBlocked, AgentDone, Body, FileTouched, KINDS, PaneMoved, ReplyLanded, SlingSent, UiPut,
};

use crate::{Error, Result};

/// The payload version of every v1 kind. A payload changes only by adding
/// fields, and a change bumps the version of that kind.
pub const VERSION: u32 = 1;

/// One entry in the record's append-only log.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The event's place in its machine's log. `None` until the store
    /// appends it; the store assigns it.
    pub number: Option<u64>,
    /// When it happened, in milliseconds since the Unix epoch, UTC.
    pub time: u64,
    /// The version of the payload's format for this kind.
    pub version: u32,
    /// The program or agent that wrote the event.
    pub source: String,
    /// Where it happened.
    pub tags: Tags,
    /// The kind and its payload.
    pub body: Body,
    /// Fields this reader does not know, at any depth, kept for [`encode`].
    unknown: Map<String, Value>,
}

/// Where an event happened. Every event names its machine (D15), and the
/// rest only when they apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tags {
    pub machine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
}

impl Tags {
    /// Tags with only the machine set.
    #[must_use]
    pub fn on(machine: impl Into<String>) -> Self {
        Self { machine: machine.into(), workspace: None, agent: None, canvas: None }
    }
}

impl Event {
    /// A new event, not yet in the log, at the current payload [`VERSION`].
    #[must_use]
    pub fn new(time: u64, source: impl Into<String>, tags: Tags, body: Body) -> Self {
        Self {
            number: None,
            time,
            version: VERSION,
            source: source.into(),
            tags,
            body,
            unknown: Map::new(),
        }
    }
}

/// The fields of an event as written. Anything else lands in `unknown`.
#[derive(Serialize, Deserialize)]
struct Wire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    number: Option<u64>,
    time: u64,
    kind: String,
    version: u32,
    source: String,
    tags: Tags,
    #[serde(default)]
    payload: Value,
}

/// Writes `event` as one line of JSON, with no newline.
#[must_use]
pub fn encode(event: &Event) -> String {
    to_value(event).to_string()
}

/// The JSON document [`encode`] writes, as a value to embed in another document.
#[must_use]
pub fn to_value(event: &Event) -> Value {
    let mut doc = known(event);
    merge(&mut doc, &event.unknown);
    Value::Object(doc)
}

/// Reads an event that [`encode`] wrote, by this or any other version.
///
/// # Errors
///
/// [`Error::Invalid`] when `json` is not an event: it is not JSON, a required
/// field is missing or has the wrong type, or the payload of a known kind
/// does not fit that kind.
pub fn decode(json: &str) -> Result<Event> {
    let invalid = |e: serde_json::Error| Error::Invalid {
        message: format!("not a record event: {e}"),
        hint: Some(
            "an event is a JSON object with time, kind, version, source, tags and payload".into(),
        ),
    };
    let doc: Map<String, Value> = serde_json::from_str(json).map_err(invalid)?;
    let wire: Wire = serde_json::from_value(Value::Object(doc.clone())).map_err(invalid)?;
    let mut event = Event {
        number: wire.number,
        time: wire.time,
        version: wire.version,
        source: wire.source,
        tags: wire.tags,
        body: Body::from_parts(wire.kind, wire.payload).map_err(invalid)?,
        unknown: Map::new(),
    };
    event.unknown = unknown(&doc, &known(&event));
    Ok(event)
}

/// The event as JSON, without the fields this reader does not know.
#[expect(clippy::expect_used, reason = "a struct of plain fields always serializes to an object")]
fn known(event: &Event) -> Map<String, Value> {
    let wire = Wire {
        number: event.number,
        time: event.time,
        kind: event.body.kind().to_owned(),
        version: event.version,
        source: event.source.clone(),
        tags: event.tags.clone(),
        payload: event.body.payload(),
    };
    match serde_json::to_value(wire).expect("an event serializes") {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// What `original` has that `known` lacks, at any depth of nested objects.
fn unknown(original: &Map<String, Value>, known: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in original {
        match (known.get(key), value) {
            (None, _) => {
                out.insert(key.clone(), value.clone());
            }
            (Some(Value::Object(k)), Value::Object(o)) => {
                let inner = unknown(o, k);
                if !inner.is_empty() {
                    out.insert(key.clone(), Value::Object(inner));
                }
            }
            (Some(_), _) => {}
        }
    }
    out
}

/// Adds `extra` to `doc`. A key `doc` already has keeps its known value.
fn merge(doc: &mut Map<String, Value>, extra: &Map<String, Value>) {
    for (key, value) in extra {
        match (doc.get_mut(key), value) {
            (None, _) => {
                doc.insert(key.clone(), value.clone());
            }
            (Some(Value::Object(d)), Value::Object(e)) => merge(d, e),
            (Some(_), _) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_known_values() {
        let mut doc = serde_json::json!({"a": 1, "o": {"b": 2}});
        let extra = serde_json::json!({"a": 9, "o": {"b": 9, "c": 3}, "d": 4});
        let (Value::Object(d), Value::Object(e)) = (&mut doc, &extra) else { unreachable!() };
        merge(d, e);
        assert_eq!(doc, serde_json::json!({"a": 1, "o": {"b": 2, "c": 3}, "d": 4}));
    }

    #[test]
    fn unknown_finds_nested_extras_only() {
        let original = serde_json::json!({"a": 1, "o": {"b": 2, "c": 3}, "p": {"b": 2}, "d": 4});
        let known = serde_json::json!({"a": 1, "o": {"b": 2}, "p": {"b": 2}});
        let (Value::Object(o), Value::Object(k)) = (&original, &known) else { unreachable!() };
        assert_eq!(Value::Object(unknown(o, k)), serde_json::json!({"o": {"c": 3}, "d": 4}));
    }
}
