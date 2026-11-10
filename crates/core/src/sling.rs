//! The sling message: what one sling sends to an agent (SPEC section 6, D8,
//! D10). A packet, the user's prompt and a short note on how to reply in;
//! one message of text and image blocks out.
//!
//! The message reads top to bottom:
//!
//! ```text
//! slinger-sling 1 canvas=idea selected=c51,t12
//! slinger-packet 1 canvas=idea page=fable-5.2 from=41 to=58 scale=0.35
//! # new
//! c42 ink ... img=c42.png
//! <image c42.png>
//! ...
//! # reply
//! <the note>
//! # prompt
//! <the prompt, as typed>
//! ```
//!
//! Each image block sits right after the packet line that names it
//! (docs/design/packet.md, Delivery). The prompt is last and runs to the end,
//! so it needs no quoting. A packet reader skips `# reply` and `# prompt` as
//! unknown sections.
//!
//! The note names only the [`Verb`]s the caller says it serves, so it never
//! tells an agent to run a command that does not exist. The first sling of a
//! session gets the whole note; later slings, whose session remembers it
//! (D10), get one line.
//!
//! [`landed`] is the other end: the record's `reply_landed` when the agent
//! answers on the canvas.

use std::fmt::Write as _;

use crate::packet::Packet;
use crate::record::{Body, Event, ReplyLanded};
use crate::render::Image;

/// The message format's version: the second word of line 1. Within it,
/// changes only add.
pub const VERSION: u64 = 1;

/// The first word of every message.
const MAGIC: &str = "slinger-sling";

/// What the whole note says before the verbs: what the agent got, and where
/// its answer goes.
const INTRO: &str = "\
This is a sling from Slinger: the user's canvas page as a packet, then the user's prompt.
Each packet line is one item: id, kind, x,y, wxh, then fields. `# new` is ink written since the last sling, each image under its line; `# made` is what agents made; `# page` is the rest.
`selected=` on line 1 names the items the user selected. An empty prompt asks about the selection.
";

/// A command the note can tell the agent to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// `canvas put`: add text, a box or an arrow beside an item.
    Put,
    /// `canvas crop`: an image of one item.
    Crop,
}

impl Verb {
    /// The note's line for this verb on a first sling.
    fn full(self, canvas: &str) -> String {
        match self {
            Self::Put => format!(
                "Answer on the canvas, not in chat: the user reads the canvas.\n\
                 - `slinger canvas put {canvas} --beside <id> --text <text>` puts text beside \
                 item <id>; `--box w,h` or `--arrow <from-id> <to-id>` in place of `--text` \
                 puts a box or an arrow.\n"
            ),
            Self::Crop => format!(
                "- `slinger canvas crop {canvas} --item <id>` gives an image of an item the \
                 packet shows only as a line.\n"
            ),
        }
    }

    /// The note's fragment for this verb on a later sling.
    fn short(self, canvas: &str) -> String {
        match self {
            Self::Put => {
                format!("answer with `slinger canvas put {canvas} --beside <id>`, not in chat")
            }
            Self::Crop => format!("`slinger canvas crop {canvas} --item <id>` for an image"),
        }
    }
}

/// One sling, as the message needs it.
#[derive(Debug, Clone, Copy)]
pub struct Sling<'a> {
    /// The packet built for this sling. Its header names the canvas.
    pub packet: &'a Packet,
    /// What the user typed. Empty is valid: the selection is the question.
    pub prompt: &'a str,
    /// The packet ids (`c42`, `t12`) of the selected items. Ids that are not
    /// in the packet are left out, so the agent is never pointed at an item
    /// it cannot see.
    pub selection: &'a [String],
    /// True for the first sling of a session: it gets the whole note.
    pub first: bool,
    /// The verbs the agent can run. The note names these and no others.
    pub verbs: &'a [Verb],
}

/// One piece of a message, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    Image(Image),
}

/// What is sent to the agent for one sling: text and image blocks in reading
/// order. Two text blocks are never next to each other.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub blocks: Vec<Block>,
}

/// The message for `sling`. Every sling has one: there is nothing to reject.
#[must_use]
pub fn message(sling: &Sling<'_>) -> Message {
    let packet = sling.packet;
    let canvas = packet.header.canvas.as_str();
    let mut out = Out { blocks: Vec::new(), text: header(sling) };
    // Packet::text writes line 1, then `# new`, then one line per `new` entry.
    let mut lines = packet.text().split_inclusive('\n').map(str::to_string).collect::<Vec<_>>();
    let rest = lines.split_off(2.min(lines.len()));
    out.text.extend(lines);
    let mut rest = rest.into_iter();
    for entry in &packet.new {
        out.text.extend(rest.next());
        for name in entry.field("img").into_iter().flat_map(|names| names.split(',')) {
            if let Some(image) = packet.images.iter().find(|i| i.name == name) {
                out.image(image.clone());
            }
        }
    }
    out.text.extend(rest);
    out.text.push_str(&note(canvas, sling.first, sling.verbs));
    out.text.push_str("# prompt\n");
    out.text.push_str(sling.prompt);
    out.finish()
}

/// What an agent's reply on a canvas adds to the record: `reply_landed`,
/// naming `items`, for the last of `slings` (the `sling_sent` events sent to
/// that agent on that canvas, oldest first). `None` before the first sling:
/// the reply answers none.
#[must_use]
pub fn landed(slings: &[Event], items: Vec<String>) -> Option<Body> {
    let sling = slings.iter().rev().find(|e| matches!(e.body, Body::SlingSent(_)))?.number?;
    Some(Body::ReplyLanded(ReplyLanded { sling, items }))
}

/// Line 1: the message's version, the canvas and the selection.
fn header(sling: &Sling<'_>) -> String {
    let packet = sling.packet;
    let mut out = format!("{MAGIC} {VERSION} canvas={}", packet.header.canvas);
    let all = || packet.new.iter().chain(&packet.made).chain(&packet.page);
    let mut selected = sling.selection.iter().filter(|id| all().any(|e| &e.id == *id));
    if let Some(first) = selected.next() {
        let _ = write!(out, " selected={first}");
        for id in selected {
            let _ = write!(out, ",{id}");
        }
    }
    out.push('\n');
    out
}

/// The `# reply` section: the whole note on a first sling, one line after.
/// Nothing on a later sling when there are no verbs to name.
fn note(canvas: &str, first: bool, verbs: &[Verb]) -> String {
    if first {
        let mut out = format!("# reply\n{INTRO}");
        for verb in verbs {
            out.push_str(&verb.full(canvas));
        }
        return out;
    }
    if verbs.is_empty() {
        return String::new();
    }
    let line: Vec<String> = verbs.iter().map(|v| v.short(canvas)).collect();
    format!("# reply\n{}.\n", capitalize(&line.join("; ")))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The blocks so far and the text not yet in a block.
struct Out {
    blocks: Vec<Block>,
    text: String,
}

impl Out {
    fn image(&mut self, image: Image) {
        self.flush();
        self.blocks.push(Block::Image(image));
    }

    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.blocks.push(Block::Text(std::mem::take(&mut self.text)));
        }
    }

    fn finish(mut self) -> Message {
        self.flush();
        Message { blocks: self.blocks }
    }
}
