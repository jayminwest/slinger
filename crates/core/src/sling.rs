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
//! session gets the whole note, which ends by pointing at `slinger guide` for
//! the rest of the CLI when the agent can run it (it has a verb or listens);
//! later slings, whose session remembers it (D10), get one line. The CLI's
//! tests hold every command the stored snapshots show to its command tree.
//!
//! A sling a listener takes (a plain Claude Code session, SPEC section 5)
//! names the agent it answers as on line 1 (`agent=scout`) and adds what a
//! listener must do itself to the note ([`Listener`]). It reads the message
//! as text ([`Message::text`]): the images are files in the packet's `at=`
//! directory.
//!
//! [`first`] says which slings get the whole note. [`landed`] is the other
//! end: the record's `reply_landed` when the agent answers on the canvas.

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
A `pdf` line is a page of a PDF (`blob get` fetches the file its `blob=` names); a quoted text at the end of an ink line is the PDF's words under that ink.
";

/// The whole note's last line for an agent that can run the CLI: where the
/// rest of it is.
const GUIDE: &str = "`slinger guide` shows every other command and flag.\n";

/// A command the note can tell the agent to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// `canvas put`: add text, a box, ink or an arrow beside an item.
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
                 item <id>; `--box w,h`, `--ink x,y,dx,dy,...` or `--arrow <from-id> <to-id>` \
                 in place of `--text` puts a box, a pen stroke or an arrow. `--color #rrggbb` \
                 colours it; `--at x,y` puts its top-left corner there instead; `--size n` \
                 sizes text (default 240). Put lengths are tenths: the packet's 120 is 1200.\n"
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
    /// The listener that takes it. `None`: it goes to the agent's process.
    pub listener: Option<Listener<'a>>,
}

/// Who takes a sling by listening: a session nobody started as the agent,
/// so its commands do not know the agent's name and no hook sets its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listener<'a> {
    /// The agent it answers as.
    pub agent: &'a str,
    /// The workspace it listens on.
    pub workspace: &'a str,
}

impl Listener<'_> {
    /// The note's lines for a listener.
    fn note(self) -> String {
        let Self { agent, workspace } = self;
        format!(
            "You took this sling by listening, as agent `{agent}`. Each `img=` names a PNG in \
             the packet's `at=` directory: read the ones you need.\n\
             - Add `--agent {agent}` to each `canvas put` and `status set`.\n\
             - When you have answered, run `slinger status set done --agent {agent}` (or \
             `blocked --note <what you need>`), then `slinger session listen {workspace} --pid \
             $PPID` for the next sling.\n"
        )
    }
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

impl Message {
    /// About how many tokens a model spends reading the whole message: the
    /// packet, its images, and the sling's own lines, note and prompt
    /// around it. The packet's rule ([`Packet::tokens`]), so the two agree.
    #[must_use]
    pub fn tokens(&self) -> u64 {
        let chars = self.blocks.iter().map(|b| match b {
            Block::Text(t) => t.chars().count() as u64,
            Block::Image(_) => 0,
        });
        let images = self.blocks.iter().filter_map(|b| match b {
            Block::Image(i) => Some(i),
            Block::Text(_) => None,
        });
        Packet::estimate(chars.sum(), images)
    }

    /// The message as a listener reads it: its text, top to bottom. Each
    /// image is a file the packet's lines name, so none is lost.
    #[must_use]
    pub fn text(&self) -> String {
        let text = self.blocks.iter().filter_map(|b| match b {
            Block::Text(text) => Some(text.as_str()),
            Block::Image(_) => None,
        });
        text.collect()
    }
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
    out.text.push_str(&note(canvas, sling.first, sling.verbs, sling.listener));
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

/// Whether a sling into the conversation `session` is its first, and so gets
/// the whole note: none of `slings` (the agent's `sling_sent` events) went
/// into it. A conversation remembers its slings (D10), even across a resume,
/// so a new process on an old conversation is not a first; a new
/// conversation is, whatever the agent was sent before.
#[must_use]
pub fn first(slings: &[Event], session: &str) -> bool {
    !slings.iter().any(|e| match &e.body {
        Body::SlingSent(sent) => sent.session.as_deref() == Some(session),
        _ => false,
    })
}

/// Line 1: the message's version, the canvas and the selection.
fn header(sling: &Sling<'_>) -> String {
    let packet = sling.packet;
    let mut out = format!("{MAGIC} {VERSION} canvas={}", packet.header.canvas);
    if let Some(listener) = sling.listener {
        let _ = write!(out, " agent={}", listener.agent);
    }
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

/// The `# reply` section: the whole note on a first sling (ending with the
/// guide's line when there is a CLI to run), one line after, then a
/// listener's lines. Nothing on a later sling when there are no verbs
/// to name and no listener.
fn note(canvas: &str, first: bool, verbs: &[Verb], listener: Option<Listener<'_>>) -> String {
    let theirs = listener.map(Listener::note).unwrap_or_default();
    if first {
        let mut out = format!("# reply\n{INTRO}");
        for verb in verbs {
            out.push_str(&verb.full(canvas));
        }
        if !verbs.is_empty() || listener.is_some() {
            out.push_str(GUIDE);
        }
        return out + &theirs;
    }
    if verbs.is_empty() && theirs.is_empty() {
        return String::new();
    }
    let line: Vec<String> = verbs.iter().map(|v| v.short(canvas)).collect();
    let line = if line.is_empty() { String::new() } else { capitalize(&line.join("; ")) + ".\n" };
    format!("# reply\n{line}{theirs}")
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
