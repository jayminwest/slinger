//! The drive protocol: what `slinger drive` asks a driven window
//! (`slinger-app --drive SOCKET`) and what the window answers, one line of
//! JSON each way. This is the one protocol that does not go to slingerd;
//! it lives here so both ends write and read the same types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request to a driven window, tagged by `op` on the wire. A field that
/// is a flag defaults to false and a button to the left one, so a line
/// written by hand may leave them out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DriveAsk {
    /// Press `chord` ("cmd+j"); `text` is what it types when passed on.
    Key { chord: String, text: Option<String> },
    /// Type `text`, a key press per character.
    Type { text: String },
    /// Press the pen or a mouse button.
    Press(PenAt),
    /// Move the pen or a mouse button while it is down.
    Drag(PenAt),
    /// Lift the pen or let a mouse button up.
    Lift(PenAt),
    /// Press at the first point, drag through the others, lift at the last.
    Stroke {
        points: Vec<[f64; 2]>,
        #[serde(flatten)]
        pen: PenFlags,
    },
    /// Move the pointer with no button down.
    Move { x: f64, y: f64 },
    /// Scroll by `dx`, `dy` over a point.
    Scroll { x: f64, y: f64, dx: f64, dy: f64 },
    /// Pinch over a point: zoom by `factor` about it.
    Zoom { x: f64, y: f64, factor: f64 },
    /// The space bar down, or up.
    Space {
        #[serde(default)]
        down: bool,
    },
    /// The pen comes near the tablet, or leaves it.
    Proximity {
        #[serde(default)]
        near: bool,
        #[serde(default)]
        eraser: bool,
    },
    /// Choose a command as its menu item does, by its name.
    Command { name: String },
    /// Copy the selection to the driven window's own clipboard.
    Copy,
    /// Cut the selection to the driven window's own clipboard.
    Cut,
    /// Paste `text`, else what the driven window last cut or copied.
    Paste {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// The text the driven window last cut or copied.
    Clip,
    /// Choose the item titled `title` in the context menu that is open.
    Pick { title: String },
    /// Drop the file at `path` on a point.
    Drop { x: f64, y: f64, path: PathBuf },
    /// Take the daemon's news now.
    Tick,
    /// The scene as text: its outline, or all of it (`debug`).
    Scene {
        #[serde(default)]
        debug: bool,
    },
    /// Write the window's content as a PNG at `path`.
    Shot { path: PathBuf },
    /// Make the window's content `w` by `h` points.
    Resize { w: f64, h: f64 },
}

/// A point of the window, from its top left, and what made the event
/// there: the pen, or a mouse button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenAt {
    pub x: f64,
    pub y: f64,
    #[serde(flatten)]
    pub pen: PenFlags,
    #[serde(default)]
    pub button: DriveButton,
    /// Control was held: a left press is the right button's.
    #[serde(default)]
    pub control: bool,
}

/// What made a pen event; a mouse with none of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(clippy::struct_excessive_bools, reason = "one flag for each fact of a pen event")]
pub struct PenFlags {
    #[serde(default)]
    pub tablet: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub touch: bool,
    #[serde(default)]
    pub eraser: bool,
}

/// A mouse button; the left one is the pen's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Every `op` a driven window takes.
pub const DRIVE_OPS: [&str; 22] = [
    "key",
    "type",
    "press",
    "drag",
    "lift",
    "stroke",
    "move",
    "scroll",
    "zoom",
    "space",
    "proximity",
    "command",
    "copy",
    "cut",
    "paste",
    "clip",
    "pick",
    "drop",
    "tick",
    "scene",
    "shot",
    "resize",
];

impl DriveAsk {
    /// The request's line, without its newline; keys sorted.
    #[must_use]
    pub fn line(&self) -> String {
        serde_json::to_value(self).map(|v| v.to_string()).unwrap_or_default()
    }

    /// The request `line` holds; else what is wrong with it, said for the
    /// one who wrote it.
    ///
    /// # Errors
    ///
    /// The line is not JSON, has no `op` or an unknown one, or lacks a
    /// field its op needs.
    pub fn read(line: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(line).map_err(|e| format!("not JSON: {e}"))?;
        let op = value["op"].as_str().ok_or("no \"op\"")?.to_string();
        if !DRIVE_OPS.contains(&op.as_str()) {
            return Err(format!("no op {op:?}"));
        }
        let button = value["button"].clone();
        Self::deserialize(value).map_err(|e| wrong(&op, &button, &e.to_string()))
    }
}

/// Why a line of a known `op` is not its request, from serde's `error`.
fn wrong(op: &str, button: &Value, error: &str) -> String {
    if let Some(name) = button.as_str()
        && !matches!(name, "left" | "right" | "middle")
    {
        return format!("no button {name:?}: left, right or middle");
    }
    if op == "stroke" {
        return "stroke needs \"points\": [[x, y], ...]".into();
    }
    let field = error.split('`').nth(1).unwrap_or_default();
    match field {
        "x" | "y" | "dx" | "dy" | "factor" | "w" | "h" => format!("it needs a number \"{field}\""),
        "chord" | "text" | "name" | "title" | "path" => format!("it needs a string \"{field}\""),
        _ => format!("{op}: {error}"),
    }
}

/// A driven window's answer: what it said, or why it refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveReply {
    Said(String),
    Refused(String),
}

/// A reply line's fields.
#[derive(Serialize, Deserialize)]
struct Line {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    said: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl DriveReply {
    /// The reply's line, without its newline: `{"ok":true,"said":...}` or
    /// `{"error":...,"ok":false}`.
    #[must_use]
    pub fn line(&self) -> String {
        let line = match self {
            Self::Said(said) => Line { ok: true, said: Some(said.clone()), error: None },
            Self::Refused(error) => Line { ok: false, said: None, error: Some(error.clone()) },
        };
        serde_json::to_value(line).map(|v| v.to_string()).unwrap_or_default()
    }

    /// The reply `line` holds. A line that is not one is a refusal that
    /// says the app gave no reply.
    #[must_use]
    pub fn read(line: &str) -> Self {
        match serde_json::from_str::<Line>(line) {
            Ok(Line { ok: true, said, .. }) => Self::Said(said.unwrap_or_default()),
            Ok(Line { error: Some(error), .. }) => Self::Refused(error),
            _ => Self::Refused("the app gave no reply".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = include_str!("../tests/fixtures/drive.jsonl");

    #[test]
    fn each_golden_line_reads_and_writes_back_byte_for_byte() {
        for line in GOLDEN.lines() {
            let ask = DriveAsk::read(line).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(ask.line(), line);
        }
        let op = |l: &str| serde_json::from_str::<Value>(l).unwrap()["op"].to_string();
        let ops: std::collections::BTreeSet<_> = GOLDEN.lines().map(op).collect();
        assert_eq!(ops.len(), DRIVE_OPS.len() - 1, "a line for each op but proximity");
    }

    #[test]
    fn a_line_written_by_hand_may_leave_out_flags_and_the_button() {
        let press = DriveAsk::read(r#"{"op":"press","x":1,"y":2}"#).unwrap();
        let at = PenAt {
            x: 1.0,
            y: 2.0,
            pen: PenFlags::default(),
            button: DriveButton::Left,
            control: false,
        };
        assert_eq!(press, DriveAsk::Press(at));
        let paste = DriveAsk::read(r#"{"op":"paste","text":null}"#).unwrap();
        assert_eq!(paste, DriveAsk::Paste { text: None });
        assert_eq!(DriveAsk::read(r#"{"op":"copy","x":1}"#), Ok(DriveAsk::Copy));
        let near = DriveAsk::read(r#"{"op":"proximity","near":true}"#).unwrap();
        assert_eq!(near, DriveAsk::Proximity { near: true, eraser: false });
    }

    #[test]
    fn a_wrong_line_says_how() {
        for (line, said) in [
            ("x", "not JSON"),
            ("{}", "no \"op\""),
            (r#"{"op":"fly"}"#, "no op \"fly\""),
            (r#"{"op":"press","x":1}"#, "it needs a number \"y\""),
            (r#"{"op":"key"}"#, "it needs a string \"chord\""),
            (r#"{"op":"press","x":1,"y":2,"button":"fourth"}"#, "no button \"fourth\""),
            (r#"{"op":"stroke"}"#, "stroke needs \"points\""),
        ] {
            let error = DriveAsk::read(line).unwrap_err();
            assert!(error.starts_with(said), "{line}: {error}");
        }
    }

    #[test]
    fn a_reply_is_what_the_window_said_or_why_it_refused() {
        let said = DriveReply::Said("taken".into());
        assert_eq!(said.line(), r#"{"ok":true,"said":"taken"}"#);
        let refused = DriveReply::Refused("no chord".into());
        assert_eq!(refused.line(), r#"{"error":"no chord","ok":false}"#);
        assert_eq!(DriveReply::read(&said.line()), said);
        assert_eq!(DriveReply::read(&refused.line()), refused);
        assert_eq!(DriveReply::read(r#"{"ok":true}"#), DriveReply::Said(String::new()));
        let none = DriveReply::Refused("the app gave no reply".into());
        assert_eq!(DriveReply::read(""), none);
        assert_eq!(DriveReply::read(r#"{"ok":false}"#), none);
    }
}
