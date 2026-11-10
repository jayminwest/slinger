//! What an event says: its kind and the payload that kind carries.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::BlobId;

/// The kind of an event and its typed payload. Each v1 kind has a variant.
/// A kind this reader does not know is kept as [`Body::Other`], so a newer
/// writer never breaks an older reader (B5).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// `sling_sent`: a selection and a prompt went to an agent.
    SlingSent(SlingSent),
    /// `reply_landed`: an agent's reply was placed on a canvas.
    ReplyLanded(ReplyLanded),
    /// `file_touched`: an agent wrote a file.
    FileTouched(FileTouched),
    /// `pane_moved`: a pane got a new frame.
    PaneMoved(PaneMoved),
    /// `agent_blocked`: an agent waits for the user.
    AgentBlocked(AgentBlocked),
    /// `agent_done`: an agent finished its task.
    AgentDone(AgentDone),
    /// A kind this reader does not know, with its payload as written.
    Other { kind: String, payload: Value },
}

/// Payload of `sling_sent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlingSent {
    /// The prompt the user added to the selection.
    pub prompt: String,
    /// The packet the agent got, stored as a blob (R7).
    pub packet: BlobId,
}

/// Payload of `reply_landed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyLanded {
    /// The number of the `sling_sent` event this replies to.
    pub sling: u64,
    /// The ids of the canvas items the reply made.
    pub items: Vec<String>,
}

/// Payload of `file_touched`. It names the file and its content hash, never
/// the contents (D14: the record is not where files live).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTouched {
    /// The file's path on the event's machine.
    pub path: String,
    /// The hash of the file's contents after the write.
    pub hash: BlobId,
}

/// Payload of `pane_moved`. The frame is in points, origin at the window's
/// top left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneMoved {
    pub pane: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Payload of `agent_blocked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlocked {
    /// What the agent needs from the user.
    pub reason: String,
}

/// Payload of `agent_done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDone {
    /// What the agent says it did, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Body {
    /// The kind's name as it is written in an event.
    #[must_use]
    pub fn kind(&self) -> &str {
        match self {
            Self::SlingSent(_) => "sling_sent",
            Self::ReplyLanded(_) => "reply_landed",
            Self::FileTouched(_) => "file_touched",
            Self::PaneMoved(_) => "pane_moved",
            Self::AgentBlocked(_) => "agent_blocked",
            Self::AgentDone(_) => "agent_done",
            Self::Other { kind, .. } => kind,
        }
    }

    /// The payload as JSON.
    pub(super) fn payload(&self) -> Value {
        match self {
            Self::SlingSent(p) => to_value(p),
            Self::ReplyLanded(p) => to_value(p),
            Self::FileTouched(p) => to_value(p),
            Self::PaneMoved(p) => to_value(p),
            Self::AgentBlocked(p) => to_value(p),
            Self::AgentDone(p) => to_value(p),
            Self::Other { payload, .. } => payload.clone(),
        }
    }

    /// The body for `kind`. An unknown kind keeps its payload as it is.
    ///
    /// # Errors
    ///
    /// A known kind whose payload does not fit that kind.
    pub(super) fn from_parts(kind: String, payload: Value) -> serde_json::Result<Self> {
        Ok(match kind.as_str() {
            "sling_sent" => Self::SlingSent(serde_json::from_value(payload)?),
            "reply_landed" => Self::ReplyLanded(serde_json::from_value(payload)?),
            "file_touched" => Self::FileTouched(serde_json::from_value(payload)?),
            "pane_moved" => Self::PaneMoved(serde_json::from_value(payload)?),
            "agent_blocked" => Self::AgentBlocked(serde_json::from_value(payload)?),
            "agent_done" => Self::AgentDone(serde_json::from_value(payload)?),
            _ => Self::Other { kind, payload },
        })
    }
}

/// Every payload is a struct of strings, numbers and lists, which always
/// serializes: a non-finite float becomes `null` rather than an error.
#[expect(
    clippy::expect_used,
    reason = "payload structs have no map keys, so serializing cannot fail"
)]
fn to_value(payload: &impl Serialize) -> Value {
    serde_json::to_value(payload).expect("a payload serializes")
}
