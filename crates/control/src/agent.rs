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
    /// Put text, a box or an arrow beside an item, where it covers nothing.
    CanvasPut(CanvasPut),
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
    /// What goes there: one of `text`, `box` or `arrow` on the wire.
    #[serde(flatten)]
    pub what: Put,
}

/// A put as the line holds it: exactly one of `text`, `box` and `arrow`.
#[derive(Deserialize)]
struct PutLine {
    canvas: String,
    agent: String,
    beside: String,
    text: Option<String>,
    #[serde(rename = "box")]
    frame: Option<[i64; 2]>,
    arrow: Option<[String; 2]>,
}

impl TryFrom<PutLine> for CanvasPut {
    type Error = &'static str;

    fn try_from(line: PutLine) -> Result<Self, Self::Error> {
        let what = match (line.text, line.frame, line.arrow) {
            (Some(text), None, None) => Put::Text(text),
            (None, Some(size), None) => Put::Box(size),
            (None, None, Some(ends)) => Put::Arrow(ends),
            _ => return Err("a put takes exactly one of `text`, `box` and `arrow`"),
        };
        Ok(Self { canvas: line.canvas, agent: line.agent, beside: line.beside, what })
    }
}

/// What a put adds. Lengths are in tenths of a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Put {
    Text(String),
    /// `[w, h]`.
    Box([i64; 2]),
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

impl Client {
    /// Puts `what` on `canvas` beside the item `beside`, as `agent`.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for a
    /// blank agent, an id that is not one or a put that cannot be drawn, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_put(
        &self,
        canvas: &str,
        agent: &str,
        beside: &str,
        what: Put,
    ) -> Result<CanvasPlaced, RemoteError> {
        let (canvas, agent, beside) = (canvas.into(), agent.into(), beside.into());
        self.call(&AgentAsk::CanvasPut(CanvasPut { canvas, agent, beside, what }))
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
        })
    }

    #[test]
    fn a_put_reads_back_as_itself_by_its_own_op() {
        let head = r#"{"v":1,"op":"canvas_put","canvas":"idea","agent":"scout","beside":"c12""#;
        for (ask, tail) in [
            (put(Put::Text("hi".into())), r#""text":"hi"}"#),
            (put(Put::Box([200, 100])), r#""box":[200,100]}"#),
            (put(Put::Arrow(["c1".into(), "t2".into()])), r#""arrow":["c1","t2"]}"#),
        ] {
            let line = format!("{head},{tail}");
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
        ] {
            let line = format!("{head}{tail}");
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }
}
