//! Each pane's footer (SPEC P6: every state is visible): one line along the
//! pane's bottom edge, [`HEIGHT`] points high, on a fill a little lighter
//! than the background. A canvas pane's footer says, from the left, the
//! pen's colour and width (a swatch: a line that thick, up to a square),
//! its tool (bold) and the tool's live key, and what the pane is doing:
//! `· typing` while a text is open there, `· N selected` with a selection
//! on its page. In the middle, its page and
//! where that is among the pages; on the right, the keys of the key overlay
//! and the command palette, each left out while unbound. An agent UI pane's
//! footer has no tool part, and its UI's name in the middle. A narrow pane
//! drops the right part, then the middle. What the pane shows is above the
//! footer ([`content`]). Pure: `platform` draws a [`Footer`].

use crate::commands::{Command, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::style::{self, Rule};
use crate::scene::tools::Tool;

/// How high a footer is, in points.
pub const HEIGHT: f64 = 28.0;
/// How high a footer's text is set, in points.
pub const TEXT_SIZE: f64 = 12.0;
/// The height of a line of footer text.
const LINE: f64 = 16.0;
/// The width of the pen's swatch, and the most it is high.
const SWATCH: f64 = 10.0;
/// The space inside the footer's ends, and between the swatch and the tool.
const PAD: f64 = 10.0;
const GAP: f64 = 6.0;
/// The least space kept between two parts.
const SPACE: f64 = 16.0;
/// About how wide a character of footer text is: enough to tell whether a
/// part fits. Wide enough that a part said to fit does.
const CHAR: f64 = 7.0;

/// What a canvas pane is doing, besides showing its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    /// A text is open on its page.
    Typing,
    /// This many items on its page are selected.
    Selected(usize),
}

/// The pen as a canvas pane's footer says it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    pub tool: Tool,
    pub colour: Rgb,
    /// The pen's line width, in points.
    pub width: f64,
    pub state: State,
}

/// Text in one colour, bold or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub colour: Rgb,
    pub bold: bool,
}

/// Where a part's text sits in its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// One part of a footer: its runs, one after the other, in `frame`.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub frame: Rect,
    pub align: Align,
    pub runs: Vec<Run>,
}

/// One pane's footer, fully decided: its `frame` filled with `fill`, a
/// hairline along its top (`rule`), the pen's swatch (its frame and colour)
/// on a canvas pane, and the parts. A tap on one of its `targets` runs that
/// command: the right part's key hints open the key overlay and the
/// palette, so the pen can reach them with no keyboard.
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub frame: Rect,
    pub fill: Rgb,
    pub rule: Rule,
    pub swatch: Option<(Rect, Rgb)>,
    pub parts: Vec<Part>,
    pub targets: Vec<(Rect, Command)>,
}

/// What a pane laid at `frame` shows above its footer: the hit-testing,
/// the paint and the centre of a canvas pane's pan and zoom are in it.
pub fn content(frame: Rect) -> Rect {
    Rect { height: (frame.height - HEIGHT).max(0.0), ..frame }
}

/// The footer of a pane laid at `frame`, in `theme`'s colours, with the
/// live keys of `keys`: a canvas pane's with its `pen`, an agent UI pane's
/// with none. `centre` is what the middle says (the page, or the UI's
/// name); empty says nothing.
pub fn footer(frame: Rect, theme: &Theme, keys: &Keymap, pen: Option<Pen>, centre: &str) -> Footer {
    let bar =
        Rect { y: frame.y + content(frame).height, height: frame.height.min(HEIGHT), ..frame };
    let line = Rect {
        x: bar.x + PAD,
        y: bar.y + (bar.height - LINE) / 2.0,
        width: (bar.width - 2.0 * PAD).max(0.0),
        height: LINE,
    };
    let run = |text: String, colour, bold| Run { text, colour, bold };
    let (mut left, mut swatch, mut indent) = (Vec::new(), None, 0.0);
    if let Some(pen) = pen {
        let command = command(pen.tool);
        let high = pen.width.clamp(1.0, SWATCH);
        let y = bar.y + (bar.height - high) / 2.0;
        swatch = Some((Rect { x: line.x, y, width: SWATCH, height: high }, pen.colour));
        indent = SWATCH + GAP;
        left.push(run(command.title().to_string(), theme.text, true));
        if let Some(chord) = keys.chord(command) {
            left.push(run(format!(" {}", chord.glyphs()), theme.muted, false));
        }
        match pen.state {
            State::Idle => {}
            State::Typing => left.push(run(" · typing".to_string(), theme.muted, false)),
            State::Selected(n) => left.push(run(format!(" · {n} selected"), theme.muted, false)),
        }
    }
    let hints = [(Command::ShowKeys, "keys"), (Command::ShowCommands, "commands")];
    let hints: Vec<(String, Command)> = hints
        .iter()
        .filter_map(|&(c, word)| {
            keys.chord(c).map(|chord| (format!("{} {word}", chord.glyphs()), c))
        })
        .collect();
    let right = hints.iter().map(|(text, _)| text.as_str()).collect::<Vec<_>>().join(" · ");
    let side = indent + left.iter().map(|r| width(&r.text)).sum::<f64>();
    // The middle is centred, so it fits beside the wider of the two sides.
    let fits = |side: f64| 2.0 * side + width(centre) + 2.0 * SPACE <= line.width;
    let mut parts = Vec::new();
    if !left.is_empty() {
        let frame = Rect { x: line.x + indent, width: (line.width - indent).max(0.0), ..line };
        parts.push(Part { frame, align: Align::Left, runs: left });
    }
    if !centre.is_empty() && fits(side) {
        let runs = vec![run(centre.to_string(), theme.muted, false)];
        parts.push(Part { frame: line, align: Align::Centre, runs });
    }
    let mut targets = Vec::new();
    if !right.is_empty() && fits(side.max(width(&right))) {
        targets = self::targets(bar, line.x + line.width, &hints);
        let runs = vec![run(right, theme.muted, false)];
        parts.push(Part { frame: line, align: Align::Right, runs });
    }
    let rule = Rule { frame: Rect { height: style::HAIRLINE, ..bar }, colour: style::edge(theme) };
    Footer { frame: bar, fill: style::footer(theme), rule, swatch, parts, targets }
}

/// Where each of `hints` (its text and command), set right-aligned to
/// `right` in `bar` and parted by " · ", takes a tap: the bar's full
/// height, and across its share of the text's width, with the parting
/// line's half on either side. The width is [`width`]'s, a little more
/// than the text's, so a target reaches past the text's left end.
fn targets(bar: Rect, right: f64, hints: &[(String, Command)]) -> Vec<(Rect, Command)> {
    let mut x = right + PAD;
    let mut targets = Vec::new();
    for (n, (text, command)) in hints.iter().enumerate().rev() {
        let (last, first) = (n + 1 == hints.len(), n == 0);
        let spare = |end: bool| if end { PAD } else { width(" · ") / 2.0 };
        let wide = width(text) + spare(last) + spare(first);
        x -= wide;
        targets.push((Rect { x, width: wide, ..bar }, *command));
    }
    targets.reverse();
    targets
}

/// The command that picks `tool`: its title names the tool, its chord is
/// the tool's key.
pub fn command(tool: Tool) -> Command {
    match tool {
        Tool::Pen => Command::ToolPen,
        Tool::Eraser => Command::ToolEraser,
        Tool::Lasso => Command::ToolLasso,
        Tool::Text => Command::ToolText,
        Tool::Box => Command::ToolBox,
        Tool::Arrow => Command::ToolArrow,
    }
}

/// About how wide `text` is in footer text.
fn width(text: &str) -> f64 {
    f64::from(u32::try_from(text.chars().count()).unwrap_or(u32::MAX)) * CHAR
}

#[cfg(test)]
mod tests;
