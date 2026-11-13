//! Each pane's footer (SPEC P6: every state is visible), and the pen's
//! buttons in it: one line along the pane's bottom edge, [`HEIGHT`] points
//! high, on a fill a little lighter than the background. Every button is a
//! tap target at least [`style::TAP`] wide and the footer's height, in
//! muted text, that runs a registry command (or opens the page menu) when
//! tapped ([`crate::scene::taps`]); a word that is not a button says a
//! state. A canvas pane's footer, from the left:
//!
//! ```text
//! ◧  ▬  ≡  Pen P · 2 selected  ↶ ↷    ‹  ideas · 2/3  ›    ● scout · working  ➤ Sling   −  100%  +   ◫ ⊟ ✕   ⌘/ keys   ⌘K commands
//! ```
//!
//! - left, the pen: the sidebar's button while it is collapsed (in the
//!   pane at the window's bottom left, where the open sidebar's own
//!   "◧ Hide sidebar" sits: [`strip`]); the swatch (the pen's colour and
//!   width) picks a colour, the bars pick a width, the tool's name (bold,
//!   with its key) picks a tool; what the pane is doing (`· typing`,
//!   `· N selected`); undo and redo.
//! - middle, the page: the previous page, its name and where it is among
//!   the pages (a tap opens the page menu), the next page. Centred in the
//!   pane when the sides leave room, else in the space between them.
//! - right: the agent of the canvas's workspace with its status, and
//!   "➤ Sling", which slings the selection, or the page when nothing is
//!   selected, in one tap (in the focus colour while there is an agent to
//!   take it, muted when there is none); zoom out, the zoom (a tap goes
//!   back to 100%), zoom in; split
//!   right, split down, close (only with two panes or more); the keys of
//!   the key overlay and the command palette, each left out while unbound.
//!
//! A button that is only a glyph or a mark (the sidebar's, the swatch, the
//! width, undo, redo, the page arrows, zoom out and in, split and close)
//! names its command and its key in a tip just above the footer while it
//! is lit: under the pointer, or held by the pen ([`tip`]). The tip is a
//! small panel with rounded corners and a hairline edge, as the menus are.
//!
//! An agent UI pane's footer has its UI's name in the middle and the
//! right's pane buttons and keys. Groups of buttons sit [`cells::GAP`]
//! apart, the parts at least [`cells::SPACE`]. A narrow pane drops whole
//! groups, the least needed first (the rank in brackets; higher stays
//! longer): zoom (30), the pane buttons (35), the width (40), the overlay's
//! key (50), undo and redo (55), the page (60), the palette's key (70),
//! the agent (80); the tool, the state, "➤ Sling" and the sidebar's button
//! stay. Every command a
//! dropped button runs is still in the palette. What the pane shows is
//! above the footer ([`content`]). Pure: `platform` draws a [`Footer`].

mod cells;

use self::cells::{Cell, Face, Side, Unit};
use crate::commands::{Command, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::Label;
use crate::scene::overlay::{Align, Line};
use crate::scene::sidebar::{self, Status};
use crate::scene::style::{self, Rule};
use crate::scene::taps::{Tap, Target};
use crate::scene::tools::Tool;

/// How high a footer is, in points: a button is as tall as the pen needs.
pub const HEIGHT: f64 = style::TAP;
/// How high a footer's text is set, in points.
pub const TEXT_SIZE: f64 = 12.0;
/// The height of a line of footer text.
const LINE: f64 = 16.0;
/// The width of the pen's swatch, and the most it is high.
const SWATCH: f64 = 10.0;
/// The space inside the footer's ends.
const PAD: f64 = 10.0;
/// The width button's three bars: how wide, how thick each, and the gap.
const BARS: (f64, [f64; 3], f64) = (12.0, [1.0, 2.0, 3.0], 2.0);
/// A tip: how high, the room either side of its words, and its gap above
/// the footer and from the window's edge.
const TIP: (f64, f64, f64, f64) = (22.0, 8.0, 4.0, 8.0);
/// A tip's corners.
const TIP_RADIUS: f64 = 6.0;
/// How far a lit button's fill sits in from its target, and its corners.
const LIT_INSET: f64 = 2.0;
pub const LIT_RADIUS: f64 = 6.0;

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

/// What a pane's footer shows. A canvas pane's has the `pen`, its page
/// line as `centre` (with `pages`: the page buttons) and its `zoom` in
/// percent; an agent UI pane's has its UI's name. `sidebar` puts the
/// sidebar's button first (the sidebar is collapsed); `close` adds the
/// close button (there is another pane). `sling` adds "➤ Sling" (a canvas
/// pane), and `agent` names the agent its sling goes to, with its status;
/// with `profiles` (it has some) a tap there opens them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[expect(clippy::struct_excessive_bools, reason = "each says whether one part of the footer shows")]
pub struct Shows<'a> {
    pub pen: Option<Pen>,
    pub centre: &'a str,
    pub pages: bool,
    pub zoom: Option<u32>,
    pub sidebar: bool,
    pub close: bool,
    pub sling: bool,
    pub agent: Option<(&'a str, Option<Status>)>,
    pub profiles: bool,
}

/// What the footer's sling button says.
pub const SLING: &str = "➤ Sling";

/// Text in one colour, bold or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub colour: Rgb,
    pub bold: bool,
}

/// One text of a footer: its runs, one after the other, centred in `frame`.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub frame: Rect,
    pub runs: Vec<Run>,
}

/// One pane's footer, fully decided: its `frame` filled with `fill`, a
/// hairline along its top (`rule`), the filled `marks` (the pen's swatch,
/// the width's bars), the texts, and the buttons' `targets`. `lit` is the
/// button under the pen and its fill, drawn under its text ([`light`]).
/// `tips` are the buttons with no words, each with the command a tip names,
/// and `tip` is the lit one's, drawn over the pane above the footer ([`tip`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub frame: Rect,
    pub fill: Rgb,
    pub rule: Rule,
    pub marks: Vec<(Rect, Rgb)>,
    pub parts: Vec<Part>,
    pub targets: Vec<Target>,
    pub lit: Option<(Rect, Rgb)>,
    pub tips: Vec<(Rect, Command)>,
    pub tip: Option<Line>,
}

/// What a pane laid at `frame` shows above its footer: the hit-testing,
/// the paint and the centre of a canvas pane's pan and zoom are in it.
pub fn content(frame: Rect) -> Rect {
    Rect { height: (frame.height - HEIGHT).max(0.0), ..frame }
}

/// The footer of a pane laid at `frame`, in `theme`'s colours, with the
/// live keys of `keys`, showing what `shows` says.
pub fn footer(frame: Rect, theme: &Theme, keys: &Keymap, shows: &Shows) -> Footer {
    let muted = |text: &str| vec![Run { text: text.to_string(), colour: theme.muted, bold: false }];
    let glyph = |text: &str, command| Cell::glyph(muted(text), Tap::Run(command));
    let mut units = Vec::new();
    if shows.sidebar {
        units.push(Unit::new(Side::Left, 100, vec![glyph("◧", Command::ToggleSidebar)]));
    }
    if let Some(pen) = shows.pen {
        let swatch = Face::Swatch(pen.colour, pen.width.clamp(1.0, SWATCH));
        let tool = command(pen.tool);
        let mut name = vec![Run { text: tool.title().to_string(), colour: theme.text, bold: true }];
        if let Some(chord) = keys.chord(tool) {
            name.extend(muted(&format!(" {}", chord.glyphs())));
        }
        units.extend([
            Unit::new(Side::Left, 90, vec![Cell::mark(swatch, Command::PickColour)]),
            Unit::new(Side::Left, 40, vec![Cell::mark(Face::Widths, Command::PickWidth)]),
            Unit::new(Side::Left, 90, vec![Cell::words(name, Some(Tap::Run(Command::PickTool)))]),
        ]);
        let state = match pen.state {
            State::Idle => None,
            State::Typing => Some("· typing".to_string()),
            State::Selected(n) => Some(format!("· {n} selected")),
        };
        if let Some(state) = state {
            units.push(Unit::new(Side::Left, 85, vec![Cell::words(muted(&state), None)]));
        }
        let steps = vec![glyph("↶", Command::Undo), glyph("↷", Command::Redo)];
        units.push(Unit::new(Side::Left, 55, steps));
    }
    if !shows.centre.is_empty() {
        let cells = if shows.pages {
            vec![
                glyph("‹", Command::PreviousPage),
                Cell::words(muted(shows.centre), Some(Tap::Pages)),
                glyph("›", Command::NextPage),
            ]
        } else {
            vec![Cell::words(muted(shows.centre), None)]
        };
        units.push(Unit::new(Side::Middle, 60, cells));
    }
    if let Some((name, status)) = shows.agent {
        let (glyph, colour) = (sidebar::glyph(status), sidebar::colour(status, theme));
        let mut runs = vec![Run { text: glyph.to_string(), colour, bold: false }];
        let word = status.map(|s| format!(" · {}", s.word())).unwrap_or_default();
        runs.extend(muted(&format!(" {name}{word}")));
        let tap = shows.profiles.then_some(Tap::Profiles);
        units.push(Unit::new(Side::Right, 80, vec![Cell::words(runs, tap)]));
    }
    if shows.sling {
        let reaches = shows.agent.is_some_and(|(_, s)| s != Some(Status::Stopped));
        let colour = if reaches { theme.focus } else { theme.muted };
        let run = Run { text: SLING.to_string(), colour, bold: reaches };
        let button = Cell::words(vec![run], Some(Tap::Run(Command::SlingNow)));
        units.push(Unit::new(Side::Right, 95, vec![button]));
    }
    if let Some(zoom) = shows.zoom {
        let level = Cell::words(muted(&format!("{zoom}%")), Some(Tap::Run(Command::ZoomReset)));
        let zoom = vec![glyph("−", Command::ZoomOut), level, glyph("+", Command::ZoomIn)];
        units.push(Unit::new(Side::Right, 30, zoom));
    }
    let mut split = vec![glyph("◫", Command::SplitRight), glyph("⊟", Command::SplitDown)];
    if shows.close {
        split.push(glyph("✕", Command::ClosePane));
    }
    units.push(Unit::new(Side::Right, 35, split));
    for (command, word, rank) in
        [(Command::ShowKeys, "keys", 50), (Command::ShowCommands, "commands", 70)]
    {
        if let Some(chord) = keys.chord(command) {
            let hint =
                Cell::words(muted(&format!("{} {word}", chord.glyphs())), Some(Tap::Run(command)));
            units.push(Unit::new(Side::Right, rank, vec![hint]));
        }
    }
    build(frame, theme, units)
}

/// The open sidebar's foot, at `frame` (its bottom, as high as a footer):
/// its one button, "◧ Hide sidebar", where a collapsed sidebar's button
/// sits in the pane footer beside it.
pub fn strip(frame: Rect, theme: &Theme) -> Footer {
    let run = Run { text: "◧  Hide sidebar".into(), colour: theme.muted, bold: false };
    let hide = Cell::words(vec![run], Some(Tap::Run(Command::ToggleSidebar)));
    let frame =
        Rect { y: frame.y + frame.height - HEIGHT, height: HEIGHT.min(frame.height), ..frame };
    build(frame, theme, vec![Unit::new(Side::Left, 100, vec![hide])])
}

/// The footer along the bottom of `frame` with `units` laid in it.
fn build(frame: Rect, theme: &Theme, units: Vec<Unit>) -> Footer {
    let bar =
        Rect { y: frame.y + content(frame).height, height: frame.height.min(HEIGHT), ..frame };
    let line = Rect {
        x: bar.x + PAD,
        y: bar.y + (bar.height - LINE) / 2.0,
        width: (bar.width - 2.0 * PAD).max(0.0),
        height: LINE,
    };
    let (mut marks, mut parts, mut targets, mut tips) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let middle = bar.y + bar.height / 2.0;
    for (cell, face, tap, tip) in cells::lay(units, line, bar) {
        let centre = cell.x + cell.width / 2.0;
        match face {
            Face::Text(runs) => {
                let frame = Rect { y: line.y, height: LINE, ..cell };
                parts.push(Part { frame, runs });
            }
            Face::Swatch(colour, high) => {
                let (x, y) = (centre - SWATCH / 2.0, middle - high / 2.0);
                marks.push((Rect { x, y, width: SWATCH, height: high }, colour));
            }
            Face::Widths => {
                let (wide, thick, gap) = BARS;
                let tall: f64 = thick.iter().sum::<f64>() + 2.0 * gap;
                let mut y = middle - tall / 2.0;
                for high in thick {
                    let x = centre - wide / 2.0;
                    marks.push((Rect { x, y, width: wide, height: high }, theme.muted));
                    y += high + gap;
                }
            }
        }
        targets.extend(tap.map(|tap| (cell, tap)));
        if let (true, Some(Tap::Run(command))) = (tip, tap) {
            tips.push((cell, command));
        }
    }
    let rule = Rule { frame: Rect { height: style::HAIRLINE, ..bar }, colour: style::edge(theme) };
    let fill = style::footer(theme);
    Footer { frame: bar, fill, rule, marks, parts, targets, lit: None, tips, tip: None }
}

/// Gives the footer that has the lit button at `lit` its tip, if the
/// button is one with no words: its command's title and live key from
/// `keys`, on a rounded panel with a hairline edge in `theme`'s colours,
/// just above the footer, centred on the button and kept inside `window`.
/// False when no footer's tip it is.
pub fn tip(footers: &mut [Footer], lit: Rect, theme: &Theme, keys: &Keymap, window: Rect) -> bool {
    let Some((footer, command)) = footers.iter_mut().find_map(|f| {
        let command = f.tips.iter().find(|(t, _)| *t == lit).map(|&(_, c)| c)?;
        Some((f, command))
    }) else {
        return false;
    };
    let (high, inset, gap, edge) = TIP;
    let text = match keys.chord(command) {
        Some(chord) => format!("{}  {}", command.title(), chord.glyphs()),
        None => command.title().to_string(),
    };
    let width = (cells::width(&text) + 2.0 * inset).min(window.width - 2.0 * edge).max(0.0);
    let centre = lit.x + lit.width / 2.0;
    let x = (centre - width / 2.0).min(window.x + window.width - edge - width).max(window.x + edge);
    let y = (footer.frame.y - gap - high).max(window.y);
    let frame = Rect { x, y, width, height: high };
    let label = Label { text, colour: theme.text, fill: Some(style::panel(theme)), frame };
    let (radius, edge) = (TIP_RADIUS, Some(style::edge(theme)));
    footer.tip =
        Some(Line { label, size: TEXT_SIZE, bold: false, align: Align::Centre, radius, edge });
    true
}

/// Lights the button at `lit` (its target, and the fill `main` gives
/// it) in whichever of `footers` has it: the fill sits just inside the
/// target, under its text and marks, which turn from muted to the text
/// colour. False when no footer has it.
pub fn light(footers: &mut [Footer], (target, fill): (Rect, Rgb), theme: &Theme) -> bool {
    let inside = |frame: Rect| {
        let centre = crate::layout::Point {
            x: frame.x + frame.width / 2.0,
            y: frame.y + frame.height / 2.0,
        };
        target.contains(centre)
    };
    let Some(footer) = footers.iter_mut().find(|f| f.targets.iter().any(|(t, _)| *t == target))
    else {
        return false;
    };
    footer.lit = Some((target.outset(-LIT_INSET), fill));
    let runs = footer.parts.iter_mut().filter(|p| inside(p.frame)).flat_map(|p| &mut p.runs);
    for run in runs.filter(|r| r.colour == theme.muted) {
        run.colour = theme.text;
    }
    for (_, colour) in footer.marks.iter_mut().filter(|(m, c)| inside(*m) && *c == theme.muted) {
        *colour = theme.text;
    }
    true
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
        Tool::Select => Command::ToolSelect,
        Tool::CropPdf => Command::CropPdf,
    }
}

#[cfg(test)]
mod tests;
