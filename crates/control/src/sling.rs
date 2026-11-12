//! `sling_send`: a canvas's new ink and a prompt, to the agent of the
//! canvas's workspace (docs/service.md, "Slings"), and its client method.
//!
//! [`crate::Request`] reads it as a session verb ([`crate::SessionAsk`]) and
//! does not write it, so the app carries no code for it.

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError, SessionAsk};

/// What a sling sends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlingAsk {
    pub canvas: String,
    /// The page's id or name; the first page without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The selected items: ids as the canvas file (`i12`) or a packet
    /// (`c12`, `t12`) writes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub select: Vec<String>,
    /// What the user asks. Empty is valid: the selection is the question.
    #[serde(default)]
    pub prompt: String,
    /// The profile the agent must run under: a process under another one,
    /// or under none, refuses the sling (`conflict`). Absent: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// The reply to a sling: what went, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slung {
    /// The number of the `sling_sent` event appended.
    pub event: u64,
    /// The blob id of the packet's text.
    pub packet: String,
    /// The canvas operations the packet covers: `from..=to`; nothing new
    /// when `from` is past `to`.
    pub from: u64,
    pub to: u64,
    /// How many of the user's strokes were new.
    pub strokes: u64,
    /// The agent it went to.
    pub agent: String,
    /// The conversation it went into.
    pub session_id: String,
}

impl Client {
    /// Sends the new ink of a canvas page and a prompt to the running agent
    /// of the canvas's workspace, and returns once it is delivered. The
    /// answer comes as the agent's canvas puts and its status.
    ///
    /// # Errors
    ///
    /// `unavailable` when the agent's process is not running or the daemon
    /// does not answer, `conflict` when it runs under another profile than
    /// the one asked for, `not_found` for a canvas, page or workspace that is
    /// not there, and `invalid` for a selected id that is not one or a
    /// workspace with no agent.
    pub fn sling_send(&self, ask: SlingAsk) -> Result<Slung, RemoteError> {
        self.call(&SessionAsk::SlingSend(ask))
    }
}
