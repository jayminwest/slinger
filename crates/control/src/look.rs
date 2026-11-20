//! `canvas_crop` and `canvas_strokes`: what an agent asks a canvas for when
//! the packet is not enough (docs/service.md, "Crops and strokes").

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::absolute;
use crate::{Client, RemoteError};

/// `canvas_crop`: images of what `of` names, an item id or a box, written
/// as the PNG file at `path` or, for tiles, beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasCropAsk {
    pub canvas: String,
    pub of: String,
    pub path: PathBuf,
}

/// `canvas_strokes`: the raw points of a stroke, or of every stroke of a
/// cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasStrokesAsk {
    pub canvas: String,
    pub item: String,
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

/// The reply to `canvas_crop`: the images, tiles in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasImages {
    pub images: Vec<CanvasImage>,
}

/// The reply to `canvas_strokes`.
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
        let ask = CanvasCropAsk { canvas: canvas.into(), of: of.into(), path: absolute(file)? };
        Ok(self.call(ask)?.images)
    }

    /// The strokes `item` names: one stroke, or every stroke of a cluster.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for an
    /// id that is not one or an item that is not ink, and `unavailable` when
    /// the daemon does not answer.
    pub fn canvas_strokes(&self, canvas: &str, item: &str) -> Result<Vec<Value>, RemoteError> {
        Ok(self.call(CanvasStrokesAsk { canvas: canvas.into(), item: item.into() })?.strokes)
    }
}

#[cfg(test)]
mod tests {
    use crate::wire::request_line;
    use crate::{CanvasCropAsk, CanvasStrokesAsk, Request};

    #[test]
    fn a_crop_and_a_strokes_read_back_as_themselves() {
        let cases = [
            Request::CanvasCrop(CanvasCropAsk {
                canvas: "main".into(),
                of: "p1:0,0,40,20".into(),
                path: "/tmp/c.png".into(),
            }),
            Request::CanvasStrokes(CanvasStrokesAsk { canvas: "main".into(), item: "c4".into() }),
        ];
        for request in cases {
            let line = request_line(&request).unwrap();
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(request), "{line}");
        }
        let ask = CanvasStrokesAsk { canvas: "x".into(), item: "i1".into() };
        let line = request_line(&Request::CanvasStrokes(ask));
        assert_eq!(
            line.unwrap(),
            "{\"v\":1,\"op\":\"canvas_strokes\",\"canvas\":\"x\",\"item\":\"i1\"}\n"
        );
    }
}
