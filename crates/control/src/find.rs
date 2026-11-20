//! `canvas_find`: the text items of a canvas whose words hold a phrase, on
//! every page (docs/service.md, "Canvases").

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, RemoteError};

/// `canvas_find`: the text items whose words hold `text`, on every page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFindAsk {
    pub canvas: String,
    pub text: String,
}

/// The reply to `canvas_find`.
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
        Ok(self.call(CanvasFindAsk { canvas: canvas.into(), text: text.into() })?.found)
    }
}

#[cfg(test)]
mod tests {
    use crate::wire::request_line;
    use crate::{CanvasFindAsk, Request};

    #[test]
    fn a_find_reads_back_as_itself() {
        let want =
            Request::CanvasFind(CanvasFindAsk { canvas: "main".into(), text: "plan B".into() });
        let line = request_line(&want).unwrap();
        assert_eq!(
            line,
            "{\"v\":1,\"op\":\"canvas_find\",\"canvas\":\"main\",\"text\":\"plan B\"}\n"
        );
        let read = crate::read_request(&mut line.as_bytes()).unwrap();
        assert_eq!(read, Some(want));
    }
}
