//! `canvas_crop` and `canvas_strokes`: what an agent asks a canvas for when
//! the packet is not enough (docs/service.md, "Crops and strokes").
//!
//! A client writes these verbs with [`Ask`], not [`crate::Request`]:
//! [`crate::Request`] reads them but does not serialize them, so a binary that
//! never sends one (the app, whose size budget is nearly full) carries no
//! code for them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::absolute;
use crate::{Client, RemoteError};

/// The requests as a client writes them: the same lines as
/// [`crate::Request::CanvasCrop`] and [`crate::Request::CanvasStrokes`] (a
/// test holds the two together).
#[derive(Debug, Serialize)]
#[serde(tag = "op")]
enum Ask<'a> {
    #[serde(rename = "canvas_crop")]
    Crop { canvas: &'a str, of: &'a str, path: PathBuf },
    #[serde(rename = "canvas_strokes")]
    Strokes { canvas: &'a str, item: &'a str },
}

/// One image a crop wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasImage {
    pub path: PathBuf,
    /// Where its top-left pixel sits in the whole picture: zero unless a tile.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The reply to [`crate::Request::CanvasCrop`]: the images, tiles in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasImages {
    pub images: Vec<CanvasImage>,
}

/// The reply to [`crate::Request::CanvasStrokes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasStrokes {
    /// Each stroke as `canvas_show` gives an item, with absolute `points`.
    pub strokes: Vec<Value>,
}

impl Client {
    /// Writes images of what `of` names on the canvas, at the packet's scale,
    /// and returns them. `of` is an item id (`i12`, `c12`) or a box
    /// (`x,y,w,h` in tenths, after `<page>:` for another page than the
    /// first). One image goes to `file`; tiles go beside it as
    /// `<stem>-<n>.png`. A relative `file` is taken from this process's
    /// working directory.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas, page or item that is not there, `invalid`
    /// for an `of` that is not one, a crop too large, or a file the daemon
    /// cannot write, and `unavailable` when the daemon does not answer.
    pub fn canvas_crop(
        &self,
        canvas: &str,
        of: &str,
        file: &Path,
    ) -> Result<Vec<CanvasImage>, RemoteError> {
        let reply: CanvasImages = self.call(&Ask::Crop { canvas, of, path: absolute(file)? })?;
        Ok(reply.images)
    }

    /// The strokes `item` names: one stroke, or every stroke of a cluster.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for an
    /// id that is not one or an item that is not ink, and `unavailable` when
    /// the daemon does not answer.
    pub fn canvas_strokes(&self, canvas: &str, item: &str) -> Result<Vec<Value>, RemoteError> {
        let reply: CanvasStrokes = self.call(&Ask::Strokes { canvas, item })?;
        Ok(reply.strokes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_crop_and_a_strokes_read_back_as_the_requests_they_ask() {
        let cases = [
            (
                Ask::Crop { canvas: "main", of: "p1:0,0,40,20", path: "/tmp/c.png".into() },
                Request::CanvasCrop {
                    canvas: "main".into(),
                    of: "p1:0,0,40,20".into(),
                    path: "/tmp/c.png".into(),
                },
            ),
            (
                Ask::Strokes { canvas: "main", item: "c4" },
                Request::CanvasStrokes { canvas: "main".into(), item: "c4".into() },
            ),
        ];
        for (ask, request) in cases {
            let line = request_line(&ask).unwrap();
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(request), "{line}");
        }
        let line = request_line(&Ask::Strokes { canvas: "x", item: "i1" }).unwrap();
        assert_eq!(line, "{\"v\":1,\"op\":\"canvas_strokes\",\"canvas\":\"x\",\"item\":\"i1\"}\n");
    }
}
