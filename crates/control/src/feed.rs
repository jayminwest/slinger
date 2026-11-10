//! The canvas change feed: the operations after a point, with a wait so a
//! client need not poll in a tight loop. One request and one reply, as every
//! verb; the wait only holds the reply back.
//!
//! A client writes the request with [`Changes`], not [`crate::Request`], so a
//! binary that never sends it (the app, whose size budget is nearly full)
//! carries no code for it.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::TIMEOUT;
use crate::{Client, RemoteError};

/// The longest wait the daemon holds a reply for, in milliseconds. A longer
/// `wait` waits this long.
pub const MAX_WAIT_MS: u64 = 60_000;

/// `canvas_changes` as a client writes it: the same line as
/// [`crate::Request::CanvasChanges`] (a test holds the two together).
#[derive(Debug, Serialize)]
#[serde(tag = "op", rename = "canvas_changes")]
struct Changes<'a> {
    canvas: &'a str,
    after: u64,
    wait: u64,
}

/// The reply to [`crate::Request::CanvasChanges`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFeed {
    /// The number of the first operation not in `ops`: ask again with
    /// `after` one below it.
    pub next: u64,
    /// The canvas file's operation lines, lowest number first, each with
    /// every field the file holds (docs/design/canvas-file.md).
    pub ops: Vec<Value>,
}

impl Client {
    /// The operations of `canvas` numbered above `after`. When there are
    /// none, waits up to `wait` (at most [`MAX_WAIT_MS`]) for one; a reply
    /// may then still be empty. A long backlog comes in parts: ask again
    /// from `next`.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, `invalid` for an `after`
    /// the canvas has not reached, and `unavailable` when the daemon does
    /// not answer.
    pub fn canvas_changes(
        &self,
        canvas: &str,
        after: u64,
        wait: Duration,
    ) -> Result<CanvasFeed, RemoteError> {
        let wait = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX).min(MAX_WAIT_MS);
        self.call_within(&Changes { canvas, after, wait }, TIMEOUT + Duration::from_millis(wait))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_client_writes_what_the_daemon_reads() {
        let line = request_line(&Changes { canvas: "main", after: 6, wait: 100 }).unwrap();
        assert_eq!(
            line,
            "{\"v\":1,\"op\":\"canvas_changes\",\"canvas\":\"main\",\"after\":6,\"wait\":100}\n"
        );
        let read = crate::read_request(&mut line.as_bytes()).unwrap();
        assert_eq!(
            read,
            Some(Request::CanvasChanges { canvas: "main".into(), after: 6, wait: 100 })
        );
        let bare = br#"{"v":1,"op":"canvas_changes","canvas":"main"}"#;
        let read = crate::read_request(&mut &bare[..]).unwrap();
        assert_eq!(read, Some(Request::CanvasChanges { canvas: "main".into(), after: 0, wait: 0 }));
    }
}
