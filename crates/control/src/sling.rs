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
    /// The conversation it went into. Empty when it waits for a listener.
    pub session_id: String,
    /// How it goes: `process`, or `listen` when it waits for a listener to
    /// take it. Absent from a daemon that only knows `process`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<String>,
    /// About how many tokens the agent reads for it: the packet, its images
    /// and the sling's wrapper (the note and the prompt). Absent from a
    /// daemon that does not count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
}

impl Client {
    /// Sends the new ink of a canvas page and a prompt to the agent of the
    /// canvas's workspace: to its listener when it has one or no process of
    /// its runs (the sling then waits to be taken), else to its process.
    /// A sling that names a profile goes only to a process under it. The
    /// answer comes as the agent's canvas puts and its status.
    ///
    /// # Errors
    ///
    /// `unavailable` when a profile is named and the agent's process is not
    /// running, or the daemon does not answer, `conflict` when it runs under
    /// another profile than the one asked for, `not_found` for a canvas,
    /// page or workspace that is not there, and `invalid` for a selected id
    /// that is not one or a workspace with no agent.
    pub fn sling_send(&self, ask: SlingAsk) -> Result<Slung, RemoteError> {
        self.call(&SessionAsk::SlingSend(ask))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_without_tokens_from_an_older_daemon_still_reads() {
        let old =
            r#"{"event":1,"packet":"p","from":1,"to":2,"strokes":1,"agent":"a","session_id":"s"}"#;
        let slung: Slung = serde_json::from_str(old).unwrap();
        assert_eq!(slung.tokens, None);
        let text = serde_json::to_string(&Slung { tokens: Some(9), ..slung }).unwrap();
        assert!(text.ends_with(r#""tokens":9}"#), "{text}");
    }
}
