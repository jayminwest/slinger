//! The control protocol of `slingerd` (docs/service.md): the requests, the
//! replies, the wire format and the socket client.
//!
//! This crate owns the wire format. The daemon and its clients see typed
//! values only, so a change to the format is a change here and nowhere else.
//! It does not link the core: a client stays small and cannot call around the
//! daemon.

mod client;
mod wire;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use client::Client;
pub use wire::{MAX_LINE, VERSION, read_request, write_reply};

/// A request to the daemon. One variant per verb; `op` on the wire names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Ask for a greeting.
    Hello { name: String },
}

/// A successful answer. One variant per verb, holding that verb's reply type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Reply {
    Hello(Greeting),
}

/// The reply to [`Request::Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Greeting {
    pub greeting: String,
}

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

const SOCKET_FILE: &str = "slinger.sock";

/// Where the daemon listens when no path is given: `$XDG_RUNTIME_DIR`, else
/// the temporary directory (per user on macOS).
#[must_use]
pub fn default_socket() -> PathBuf {
    socket_in(std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from), std::env::temp_dir())
}

fn socket_in(runtime: Option<PathBuf>, temp: PathBuf) -> PathBuf {
    runtime.filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(temp).join(SOCKET_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_socket_prefers_the_runtime_dir() {
        let (run, temp) = (PathBuf::from("/run/user/1"), PathBuf::from("/tmp"));
        assert_eq!(socket_in(Some(run.clone()), temp.clone()), run.join(SOCKET_FILE));
        assert_eq!(socket_in(Some("".into()), temp.clone()), temp.join(SOCKET_FILE));
        assert_eq!(socket_in(None, temp.clone()), temp.join(SOCKET_FILE));
        assert!(default_socket().ends_with(SOCKET_FILE));
    }

    #[test]
    fn a_remote_error_shows_its_message_and_keeps_its_code() {
        let err = RemoteError::new("invalid", "name is empty", Some("pass a name".into()));
        assert_eq!(err.to_string(), "name is empty");
        assert_eq!((err.code(), err.hint()), ("invalid", Some("pass a name")));
    }
}
