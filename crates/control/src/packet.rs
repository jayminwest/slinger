//! `canvas_read`: what an agent reads from a canvas page (docs/service.md,
//! docs/design/packet.md), and its client method.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::client::absolute;
use crate::{Client, RemoteError, Request};

/// The arguments of [`Request::CanvasRead`](crate::Request::CanvasRead).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketAsk {
    pub canvas: String,
    /// The page's id or name; the first page without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// A box on the page (`b4` or `i4`): read what it holds, open or folded,
    /// as if it were the page. Its page is the page.
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub inside: Option<String>,
    /// The canvas operations numbered above this are new. Without it, the
    /// ones after the canvas's last sling are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
    /// Item ids that narrow the `page` section, when not empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub select: Vec<String>,
    /// The absolute path of the directory to write the packet into, instead
    /// of the daemon's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<PathBuf>,
}

/// The reply to [`Request::CanvasRead`](crate::Request::CanvasRead).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasPacket {
    /// The blob id of the packet's text as stored: without `at`.
    pub packet: String,
    /// The directory holding `packet.txt` and the images.
    pub dir: PathBuf,
    /// The packet's text, with `at=<dir>` in its header.
    pub text: String,
    /// Each image the text names, with its blob id, in the order named.
    pub images: Vec<PacketImage>,
}

/// One image of a packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketImage {
    pub name: String,
    pub id: String,
}

impl Client {
    /// The packet of a canvas page, stored as a blob and written into a
    /// directory: `ask.out` (made absolute here), or the daemon's own.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, `invalid` for a
    /// selected id that is not an item id or a directory the daemon cannot
    /// write, and `unavailable` when the daemon does not answer.
    pub fn canvas_read(&self, mut ask: PacketAsk) -> Result<CanvasPacket, RemoteError> {
        ask.out = ask.out.as_deref().map(absolute).transpose()?;
        self.call(&Request::CanvasRead(ask))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_canvas_read_reads_back_as_itself() {
        let full = PacketAsk {
            canvas: "idea".into(),
            page: Some("p2".into()),
            inside: Some("b4".into()),
            after: Some(3),
            select: vec!["c1".into()],
            out: Some("/tmp/p".into()),
        };
        let bare = PacketAsk { canvas: "idea".into(), ..PacketAsk::default() };
        for ask in [full, bare] {
            let request = Request::CanvasRead(ask);
            let line = request_line(&request).unwrap();
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(request), "{line}");
        }
        let bare = Request::CanvasRead(PacketAsk { canvas: "x".into(), ..PacketAsk::default() });
        let line = request_line(&bare);
        assert_eq!(line.unwrap(), "{\"v\":1,\"op\":\"canvas_read\",\"canvas\":\"x\"}\n");
    }
}
