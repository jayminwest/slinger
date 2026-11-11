//! The showing verbs: `ui_open`, `ui_shown` and `ui_reply`, their requests,
//! replies and client methods (docs/service.md, "Showings"). An agent opens
//! a UI; the app asks what a pane shows and sends each reply.
//!
//! [`crate::Request`] reads them as one variant, [`UiAsk`], and does not
//! write them: a client writes [`UiAsk`] itself.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, RemoteError, Ui};

/// A showing request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UiAsk {
    /// An agent shows a UI, with data, in a new pane of a workspace or as a
    /// live item on a canvas.
    UiOpen(UiOpen),
    /// What a pane's UI target shows: a showing's id or a UI reference.
    UiShown { ui: String },
    /// A showing replies to the agent that opened it.
    UiReply { open: String, value: Value },
}

/// What [`UiAsk::UiOpen`] asks. A pane needs `workspace`; a live item
/// (`host` `canvas`) needs `canvas` and `item`, and takes `size`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiOpen {
    /// The UI: `<name>`, `<name>@<version>` or a blob id.
    pub ui: String,
    /// The workspace whose window shows it as a pane: a name, or the
    /// directory of its file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workspace: String,
    /// The agent that opens it, which its replies go to.
    pub agent: String,
    /// Where it shows: `pane`, or `canvas` for a live item.
    #[serde(default = "pane")]
    pub host: String,
    /// The JSON the UI's context gives it; `null` when absent.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    /// Open it beside this pane, by number; else beside the whole window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beside: Option<u32>,
    /// Below instead of to the right.
    #[serde(default, skip_serializing_if = "is_false")]
    pub below: bool,
    /// The canvas a live item goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    /// The item a live item goes beside, as a packet or `canvas show`
    /// names it (`i12`, `c12`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// A live item's `[w, h]`, in tenths of a unit; [`LIVE_SIZE`] when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[i64; 2]>,
}

/// The size of a live item when `ui_open` gives none: 320 by 240 units, in tenths.
pub const LIVE_SIZE: [i64; 2] = [3200, 2400];

/// The reply to [`UiAsk::UiOpen`]: the showing's id, the version it shows,
/// and the pane that shows it or the live item that does; the `ui_opened`
/// event, and the workspace the showing is tagged with (for a live item,
/// the canvas's; empty when no open workspace holds it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiOpened {
    pub open: String,
    pub ui: Ui,
    pub workspace: String,
    /// The pane's number, for a pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<u32>,
    /// The live item, for a canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<LiveItem>,
    pub event: u64,
}

/// A live item `ui_open` put on a canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveItem {
    pub canvas: String,
    /// Its id (`i12`).
    pub item: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// Its box, in tenths of a unit.
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// The number of the canvas operation that added it.
    pub seq: u64,
}

/// The reply to [`UiAsk::UiShown`]: the version shown, and for a showing its
/// id, its data and its agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiShown {
    pub ui: Ui,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// The reply to [`UiAsk::UiReply`]: the `ui_reply` event, the agent, and
/// whether its session took the reply. A reply its session did not take
/// stays in the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiReplied {
    pub event: u64,
    pub agent: String,
    pub delivered: bool,
}

impl Client {
    /// Shows a UI, with data, for an agent: in a new pane of a workspace,
    /// or as a live item beside an item of a canvas.
    ///
    /// # Errors
    ///
    /// `not_found` for a UI, a workspace, a pane `beside`, a canvas or an
    /// `item` that is not there; `invalid` for a blank agent, a host that
    /// is not `pane` or `canvas`, a missing workspace, canvas or item, a
    /// size not above zero, data over 16384 bytes or a window with the
    /// most panes it holds; and `unavailable` when the daemon does not answer.
    pub fn ui_open(&self, open: UiOpen) -> Result<UiOpened, RemoteError> {
        self.call(&UiAsk::UiOpen(open))
    }

    /// What the pane target `ui` shows: a showing's id (`pick-slot#12`) or a
    /// UI reference.
    ///
    /// # Errors
    ///
    /// `not_found` when nothing matches, `invalid` for a target of no known
    /// form, and `unavailable` when the daemon does not answer.
    pub fn ui_shown(&self, ui: &str) -> Result<UiShown, RemoteError> {
        self.call(&UiAsk::UiShown { ui: ui.into() })
    }

    /// Sends `value` from the showing `open` to the agent that opened it.
    /// It is recorded first; an agent whose session is not running does not
    /// get it (`delivered` is false).
    ///
    /// # Errors
    ///
    /// `not_found` for a showing that is not there, `invalid` for a value
    /// over 16384 bytes or a showing that sent its 100 replies, and
    /// `unavailable` when the daemon does not answer.
    pub fn ui_reply(&self, open: &str, value: Value) -> Result<UiReplied, RemoteError> {
        self.call(&UiAsk::UiReply { open: open.into(), value })
    }
}

fn pane() -> String {
    "pane".into()
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_showing_request_reads_back_as_itself_by_its_own_op() {
        let open = |data, beside, below| {
            UiAsk::UiOpen(UiOpen {
                ui: "pick".into(),
                workspace: "w".into(),
                agent: "a".into(),
                host: "pane".into(),
                data,
                beside,
                below,
                ..UiOpen::default()
            })
        };
        let item = UiAsk::UiOpen(UiOpen {
            ui: "pick".into(),
            agent: "a".into(),
            host: "canvas".into(),
            canvas: Some("notes".into()),
            item: Some("c3".into()),
            size: Some([100, 50]),
            ..UiOpen::default()
        });
        for (ask, line) in [
            (
                open(Value::Null, None, false),
                r#"{"v":1,"op":"ui_open","ui":"pick","workspace":"w","agent":"a","host":"pane"}"#,
            ),
            (
                open(json!({"n": 1}), Some(2), true),
                r#"{"v":1,"op":"ui_open","ui":"pick","workspace":"w","agent":"a","host":"pane","data":{"n":1},"beside":2,"below":true}"#,
            ),
            (
                item,
                r#"{"v":1,"op":"ui_open","ui":"pick","agent":"a","host":"canvas","canvas":"notes","item":"c3","size":[100,50]}"#,
            ),
            (UiAsk::UiShown { ui: "pick#3".into() }, r#"{"v":1,"op":"ui_shown","ui":"pick#3"}"#),
            (
                UiAsk::UiReply { open: "pick#3".into(), value: json!([1]) },
                r#"{"v":1,"op":"ui_reply","open":"pick#3","value":[1]}"#,
            ),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Showing(ask)), "{line}");
        }
        let bare = r#"{"v":1,"op":"ui_open","ui":"pick","workspace":"w","agent":"a"}"#;
        let read = crate::read_request(&mut bare.as_bytes()).unwrap();
        assert_eq!(read, Some(Request::Showing(open(Value::Null, None, false))));
        let err = crate::read_request(&mut r#"{"v":1,"op":"ui_reply","value":1}"#.as_bytes());
        assert_eq!(err.unwrap_err().code(), "bad_request");
    }
}
