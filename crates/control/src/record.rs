//! The record verbs: `record_append` and `record_list` (docs/service.md).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, RemoteError};

/// An event to append: `record_append`. The daemon sets its number, time
/// and machine tag, and checks `payload` against `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEvent {
    pub kind: String,
    /// The program or agent that writes it.
    pub source: String,
    #[serde(default)]
    pub tags: NewTags,
    #[serde(default = "empty_object")]
    pub payload: Value,
}

/// The tags a writer sets. The daemon adds `machine`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
}

/// Which events to read: `record_list`. Each field set narrows; none set
/// reads them all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    /// Only events numbered above this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
}

/// The reply to `record_append`: the event as stored, in the record's event
/// format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appended {
    pub event: Value,
}

/// The reply to `record_list`: the events, lowest number first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    pub events: Vec<Value>,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

impl Client {
    /// Appends an event and returns it as stored.
    ///
    /// # Errors
    ///
    /// `invalid` for an unknown kind or a payload that does not fit it, and
    /// `unavailable` when the daemon does not answer.
    pub fn record_append(&self, event: NewEvent) -> Result<Value, RemoteError> {
        Ok(self.call(event)?.event)
    }

    /// The events `filter` selects, lowest number first. None is not an error.
    ///
    /// # Errors
    ///
    /// The daemon's error, or `unavailable` when it does not answer.
    pub fn record_list(&self, filter: EventFilter) -> Result<Vec<Value>, RemoteError> {
        Ok(self.call(filter)?.events)
    }
}
