//! The control protocol of `slingerd` (docs/service.md): the requests, the
//! replies, the wire format and the socket client.
//!
//! This crate owns the wire format. The daemon and its clients see typed
//! values only, so a change to the format is a change here and nowhere else.
//! It does not link the core: a client stays small and cannot call around the
//! daemon.

mod agent;
mod blob;
mod build;
mod canvas;
mod client;
mod daemon;
mod drive;
mod feed;
mod feedback;
mod figure;
mod find;
mod fold;
mod look;
mod packet;
mod pane;
mod place;
mod profile;
mod record;
mod session;
mod sling;
mod ui;
mod uis;
mod unix;
mod verb;
mod wire;
mod workspace;

use std::fmt;

use serde::{Deserialize, Serialize};

// Each verb's module holds its request and reply types and its `Client`
// methods; all of them are the crate's API.
pub use agent::*;
pub use blob::*;
pub use canvas::*;
pub use daemon::*;
pub use drive::*;
pub use feed::*;
pub use feedback::*;
pub use figure::*;
pub use find::*;
pub use fold::*;
pub use look::*;
pub use packet::*;
pub use pane::*;
pub use profile::*;
pub use record::*;
pub use session::*;
pub use sling::*;
pub use ui::*;
pub use uis::*;
pub use workspace::*;

pub use build::{PACKAGE, stamp};
pub use client::Client;
pub use place::{
    default_data_dir, default_socket, live_socket, name_socket, named_socket, running_socket,
    socket_name_file, unname_socket,
};
pub use unix::{bind_socket, connect_socket};
pub use verb::{OPS, Request, Verb};
pub use wire::{MAX_LINE, VERSION, read_request, write_reply};

/// A failure as it crosses the socket: the core's error, or one of the
/// protocol's own (`bad_request`, `too_large`, `unsupported_version`,
/// `unavailable`, `internal`). Callers branch on [`RemoteError::code`], never
/// on the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteError {
    code: String,
    error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

impl RemoteError {
    pub fn new(code: &str, error: impl Into<String>, hint: Option<String>) -> Self {
        Self { code: code.to_string(), error: error.into(), hint }
    }

    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}

impl std::error::Error for RemoteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_error_shows_its_message_and_keeps_its_code() {
        let err = RemoteError::new("invalid", "name is empty", Some("pass a name".into()));
        assert_eq!(err.to_string(), "name is empty");
        assert_eq!((err.code(), err.hint()), ("invalid", Some("pass a name")));
    }
}
