//! `canvas_find`: the text items of a canvas whose words hold a phrase, on
//! every page (docs/service.md, "Canvases").
//!
//! A client writes the verb with [`Ask`], not [`crate::Request`], which
//! reads it but does not serialize it: the app sends it, and each variant
//! [`crate::Request`] serializes would grow the app (its size budget).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, RemoteError};

/// The request as a client writes it: the same line as
/// [`crate::Request::CanvasFind`] (a test holds the two together).
#[derive(Debug, Serialize)]
#[serde(tag = "op", rename = "canvas_find")]
struct Ask<'a> {
    canvas: &'a str,
    text: &'a str,
}

/// The reply to [`crate::Request::CanvasFind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFound {
    /// Each text item that holds the phrase, as `canvas_show` gives an
    /// item (its `page` included): page by page, in reading order.
    pub found: Vec<Value>,
}

impl Client {
    /// The text items of `canvas` whose words hold `text`, ignoring case
    /// and spacing: page by page in the canvas's order, and on a page top
    /// to bottom, then left to right. Text of no words finds nothing.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, and `unavailable` when
    /// the daemon does not answer.
    pub fn canvas_find(&self, canvas: &str, text: &str) -> Result<Vec<Value>, RemoteError> {
        let reply: CanvasFound = self.call(&Ask { canvas, text })?;
        Ok(reply.found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_find_reads_back_as_the_request_it_asks() {
        let line = request_line(&Ask { canvas: "main", text: "plan B" }).unwrap();
        assert_eq!(
            line,
            "{\"v\":1,\"op\":\"canvas_find\",\"canvas\":\"main\",\"text\":\"plan B\"}\n"
        );
        let read = crate::read_request(&mut line.as_bytes()).unwrap();
        let want = Request::CanvasFind { canvas: "main".into(), text: "plan B".into() };
        assert_eq!(read, Some(want));
    }
}
