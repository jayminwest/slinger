//! The agent's canvas verbs: their requests, replies and client methods
//! (docs/service.md, "Agent canvas verbs").
//!
//! [`crate::Request`] reads them as one variant, [`AgentAsk`], and does not
//! write them: a binary that never sends one (the app) carries no code for
//! them. A new agent canvas verb is a new variant here.

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// An agent canvas request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentAsk {
    /// Put text, a box, ink or an arrow beside an item, where it covers
    /// nothing or where the agent says.
    CanvasPut(CanvasPut),
    /// Change an item the agent made: its text, its size or its place.
    CanvasEdit(CanvasEdit),
    /// Take an item the agent made off the canvas. One already gone is fine.
    CanvasRemove(CanvasRemove),
}

/// What [`AgentAsk::CanvasPut`] asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PutLine")]
pub struct CanvasPut {
    pub canvas: String,
    /// The agent that puts it, and so makes the item.
    pub agent: String,
    /// The item it goes beside: `i12`, or a packet's `c12`, `t12`.
    pub beside: String,
    /// What goes there: one of `text`, `box`, `ink` or `arrow` on the wire.
    #[serde(flatten)]
    pub what: Put,
    /// Its colour, `#rrggbb` or `#rrggbbaa`; black when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Where the top-left corner of its box goes, `[x, y]` in tenths of a
    /// unit, in place of the free space beside `beside`. Not for an arrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[i64; 2]>,
}

/// A put as the line holds it: exactly one of `text`, `box`, `ink` and `arrow`.
#[derive(Deserialize)]
struct PutLine {
    canvas: String,
    agent: String,
    beside: String,
    text: Option<String>,
    #[serde(rename = "box")]
    frame: Option<[i64; 2]>,
    ink: Option<Vec<i64>>,
    arrow: Option<[String; 2]>,
    color: Option<String>,
    at: Option<[i64; 2]>,
}

impl TryFrom<PutLine> for CanvasPut {
    type Error = &'static str;

    fn try_from(line: PutLine) -> Result<Self, Self::Error> {
        let what = match (line.text, line.frame, line.ink, line.arrow) {
            (Some(text), None, None, None) => Put::Text(text),
            (None, Some(size), None, None) => Put::Box(size),
            (None, None, Some(pts), None) => Put::Ink(pts),
            (None, None, None, Some(ends)) => Put::Arrow(ends),
            _ => return Err("a put takes exactly one of `text`, `box`, `ink` and `arrow`"),
        };
        let (color, at) = (line.color, line.at);
        Ok(Self { canvas: line.canvas, agent: line.agent, beside: line.beside, what, color, at })
    }
}

/// What a put adds. Lengths are in tenths of a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Put {
    Text(String),
    /// `[w, h]`.
    Box([i64; 2]),
    /// A pen stroke: x,y pairs, the first where it starts, each next the
    /// step from the one before. Only its shape counts.
    Ink(Vec<i64>),
    /// `[from, to]`: two item ids, as `beside` takes them.
    Arrow([String; 2]),
}

/// The reply to [`AgentAsk::CanvasPut`]: the new item and the box it covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasPlaced {
    /// The new item's id (`i12`).
    pub item: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// Its box, in tenths of a unit.
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// The number of the operation that added it.
    pub seq: u64,
    /// The `reply_landed` event appended, when the put answers a sling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<u64>,
}

/// What [`AgentAsk::CanvasEdit`] asks. What it leaves out stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasEdit {
    pub canvas: String,
    /// The agent that asks; it must have made the item.
    pub agent: String,
    /// The item: `i12`, or a packet's `t12`.
    pub item: String,
    /// New words for a text item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A new `[w, h]` for a text or a box, in tenths of a unit.
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub size: Option<[i64; 2]>,
    /// Where the item's top-left corner goes, `[x, y]` in tenths of a unit.
    #[serde(default, rename = "move", skip_serializing_if = "Option::is_none")]
    pub to: Option<[i64; 2]>,
}

/// What [`AgentAsk::CanvasRemove`] asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasRemove {
    pub canvas: String,
    /// The agent that asks; it must have made the item.
    pub agent: String,
    /// The item: `i12`, or a packet's `t12`.
    pub item: String,
}

/// The reply to [`AgentAsk::CanvasEdit`]: the item and the box it covers now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasEdited {
    /// The item's id (`i12`).
    pub item: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// Its box, in tenths of a unit.
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// The numbers of the operations written: none when nothing changed.
    pub seq: Vec<u64>,
}

/// The reply to [`AgentAsk::CanvasRemove`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasRemoved {
    /// The numbers of the operations written: none when it was already gone.
    pub seq: Vec<u64>,
}

impl Client {
    /// Puts `what` on `canvas` beside the item `beside`, as `agent`, in
    /// black where it covers nothing.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_put_as`]'s.
    pub fn canvas_put(
        &self,
        canvas: &str,
        agent: &str,
        beside: &str,
        what: Put,
    ) -> Result<CanvasPlaced, RemoteError> {
        let (canvas, agent, beside) = (canvas.into(), agent.into(), beside.into());
        self.canvas_put_as(CanvasPut { canvas, agent, beside, what, color: None, at: None })
    }

    /// Puts what `put` asks for.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for a
    /// blank agent, an id that is not one, a colour that is not one, a place
    /// for an arrow or a put that cannot be drawn, and `unavailable` when the
    /// daemon does not answer.
    pub fn canvas_put_as(&self, put: CanvasPut) -> Result<CanvasPlaced, RemoteError> {
        self.call(&AgentAsk::CanvasPut(put))
    }

    /// Changes an item the agent of `edit` made.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for a
    /// blank agent, an id that is not one, an item the agent did not make or
    /// a change that does not fit the item, and `unavailable` when the
    /// daemon does not answer.
    pub fn canvas_edit(&self, edit: CanvasEdit) -> Result<CanvasEdited, RemoteError> {
        self.call(&AgentAsk::CanvasEdit(edit))
    }

    /// Removes the item `item` from `canvas`, as `agent`. An item already
    /// gone is fine.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, `invalid` for a blank
    /// agent, an id that is not one or an item the agent did not make, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_remove(
        &self,
        canvas: &str,
        agent: &str,
        item: &str,
    ) -> Result<CanvasRemoved, RemoteError> {
        let (canvas, agent, item) = (canvas.into(), agent.into(), item.into());
        self.call(&AgentAsk::CanvasRemove(CanvasRemove { canvas, agent, item }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    fn put(what: Put) -> AgentAsk {
        AgentAsk::CanvasPut(CanvasPut {
            canvas: "idea".into(),
            agent: "scout".into(),
            beside: "c12".into(),
            what,
            color: None,
            at: None,
        })
    }

    #[test]
    fn a_put_reads_back_as_itself_by_its_own_op() {
        let head = r#"{"v":1,"op":"canvas_put","canvas":"idea","agent":"scout","beside":"c12""#;
        for (ask, tail) in [
            (put(Put::Text("hi".into())), r#""text":"hi"}"#),
            (put(Put::Box([200, 100])), r#""box":[200,100]}"#),
            (put(Put::Arrow(["c1".into(), "t2".into()])), r#""arrow":["c1","t2"]}"#),
            (put(Put::Ink(vec![0, 0, 50, 10])), r#""ink":[0,0,50,10]}"#),
            (
                {
                    let AgentAsk::CanvasPut(mut ask) = put(Put::Box([1, 2])) else {
                        unreachable!()
                    };
                    (ask.color, ask.at) = (Some("#ffd400".into()), Some([10, 20]));
                    AgentAsk::CanvasPut(ask)
                },
                r##""box":[1,2],"color":"#ffd400","at":[10,20]}"##,
            ),
        ] {
            let line = format!("{head},{tail}");
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Agent(ask)), "{line}");
        }
    }

    #[test]
    fn an_edit_and_a_remove_read_back_as_themselves_by_their_own_ops() {
        let edit = CanvasEdit {
            canvas: "idea".into(),
            agent: "scout".into(),
            item: "i2".into(),
            text: Some("hi".into()),
            to: Some([10, 20]),
            ..CanvasEdit::default()
        };
        let remove =
            CanvasRemove { canvas: "idea".into(), agent: "scout".into(), item: "i2".into() };
        let head = r#"{"v":1,"op":"#;
        for (ask, line) in [
            (
                AgentAsk::CanvasEdit(edit),
                r#""canvas_edit","canvas":"idea","agent":"scout","item":"i2","text":"hi","move":[10,20]}"#,
            ),
            (
                AgentAsk::CanvasRemove(remove),
                r#""canvas_remove","canvas":"idea","agent":"scout","item":"i2"}"#,
            ),
        ] {
            let line = format!("{head}{line}");
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Agent(ask)), "{line}");
        }
    }

    #[test]
    fn a_put_with_none_or_two_things_or_no_beside_is_a_bad_request() {
        let head = r#"{"v":1,"op":"canvas_put","canvas":"idea","agent":"scout""#;
        for tail in [
            r#","beside":"c12"}"#,
            r#","beside":"c12","text":"hi","box":[1,2]}"#,
            r#","text":"hi"}"#,
            r#","beside":"c12","box":[1]}"#,
            r#","beside":"c12","ink":[1,2],"arrow":["a","b"]}"#,
            r#","beside":"c12","text":"hi","at":[1]}"#,
        ] {
            let line = format!("{head}{tail}");
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }
}
