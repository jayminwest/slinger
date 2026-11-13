//! The pen's current tool, colour and width, and the pop-up at the pen that
//! changes them (SPEC section 11). Pure: `main` hands in where the pen is,
//! presses and keys, and reads back the [`Tool`], the colour, the width and
//! the labels to draw. A canvas pane reads [`Tools::tool`], [`Tools::colour`]
//! and [`Tools::width`] to know what the pen does. The width is the pen's
//! line alone: boxes and arrows keep their one line width.
//!
//! A tablet pen's eraser end erases whatever tool is chosen: while it is
//! the end in use ([`Tools::set_eraser_end`]), [`Tools::tool`] is the eraser
//! (so the footer and the cursor say so), and the chosen tool
//! ([`Tools::chosen`]) comes back with the tip.
//!
//! A pop-up opens from a key command where the pen or pointer last was, or
//! at the focused pane's centre before either has been in the window. The
//! pen picks a cell; a digit picks the cell with that number; the arrow keys
//! move the highlight and return (or space) picks it. A pick, escape, a press
//! outside the pop-up, or the same command again closes it.

use std::time::Instant;

use crate::commands::{Chord, Command, Keymap};
use crate::config::{Palette, Rgb, Theme, Width, Widths};
use crate::layout::{self, Point, Rect};
use crate::scene::Label;
use crate::scene::flash::Flash;

/// What the pen does on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
    Lasso,
    Text,
    /// Draws a box.
    Box,
    /// Draws an arrow.
    Arrow,
    /// Picks items with a press or a drawn box, and moves the selection.
    Select,
    /// Drags a box on a `pdf` item: the part of its page in the box goes
    /// on the clipboard as a snippet ([`crate::scene::crop`]).
    CropPdf,
}

impl Tool {
    /// The word a pop-up shows for the tool.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Eraser => "eraser",
            Tool::Lasso => "lasso",
            Tool::Text => "text",
            Tool::Box => "box",
            Tool::Arrow => "arrow",
            Tool::Select => "select",
            Tool::CropPdf => "crop",
        }
    }
}

/// The four pop-ups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The palette's colours.
    Colour,
    /// The shapes: box and arrow.
    Shape,
    /// The tools that are not shapes: pen, eraser, lasso, text and select.
    Tool,
    /// The pen's widths.
    Width,
}

impl Kind {
    /// The pop-up a command opens: `pick-colour` to `pick-width`.
    pub fn opened_by(command: Command) -> Option<Kind> {
        match command {
            Command::PickColour => Some(Kind::Colour),
            Command::PickShape => Some(Kind::Shape),
            Command::PickTool => Some(Kind::Tool),
            Command::PickWidth => Some(Kind::Width),
            _ => None,
        }
    }
}

const SHAPES: [Tool; 2] = [Tool::Box, Tool::Arrow];
const TOOLS: [Tool; 5] = [Tool::Pen, Tool::Eraser, Tool::Lasso, Tool::Text, Tool::Select];

/// An open pop-up: what it offers, where its cells are, and which is lit.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    kind: Kind,
    place: layout::Popup,
    lit: usize,
}

/// The pen's state: its tool, its colour and width, the lists it picks
/// them from, where it last was (if it has been in the window), the open pop-up, if any, the flash that
/// names a new tool or width, and whether the pen's eraser end is the one in use.
#[derive(Debug, Clone, PartialEq)]
pub struct Tools {
    tool: Tool,
    eraser_end: bool,
    colour: Rgb,
    /// A colour picked from the pop-up and not yet taken ([`Tools::picked`]).
    picked: Option<Rgb>,
    palette: Palette,
    width: Width,
    widths: Widths,
    pen: Option<Point>,
    popup: Option<Popup>,
    flash: Flash,
}

impl Tools {
    /// The pen with the palette's first colour and the default widths'
    /// first width, and no pop-up. Until the pen or pointer moves, a pop-up
    /// opens at the centre of the pane it opens in.
    pub fn new(palette: Palette) -> Self {
        let colour = palette.colours()[0];
        let flash = Flash::new(Tool::Pen);
        let widths = Widths::default();
        let width = widths.widths()[0];
        Self {
            tool: Tool::Pen,
            eraser_end: false,
            colour,
            picked: None,
            palette,
            width,
            widths,
            pen: None,
            popup: None,
            flash,
        }
    }

    /// The pen with `widths` to pick from and their first width: the start.
    #[must_use]
    pub fn with_widths(mut self, widths: Widths) -> Self {
        self.width = widths.widths()[0];
        self.widths = widths;
        self
    }

    /// The flash that names a new tool ([`Tools::chosen`]) or width.
    pub fn flash(&self) -> &Flash {
        &self.flash
    }

    /// The window is drawn at `now`: a new tool or width since the last
    /// draw flashes, with its key in `keys`.
    pub fn follow(&self, keys: &Keymap, now: Instant) {
        self.flash.follow(self.tool, keys, now);
        self.flash.follow_width(self.width, keys, now);
    }

    /// The pen's line width.
    pub fn width(&self) -> Width {
        self.width
    }

    /// What the pen does: the eraser while the pen's eraser end is in use,
    /// else the chosen tool.
    pub fn tool(&self) -> Tool {
        if self.eraser_end { Tool::Eraser } else { self.tool }
    }

    /// The tool chosen with a key, a menu or a pop-up: what the pen's tip does.
    pub fn chosen(&self) -> Tool {
        self.tool
    }

    /// The pen's eraser end is the end in use (`true`), or its tip, a mouse
    /// or a trackpad is (`false`). Returns whether that changed.
    pub fn set_eraser_end(&mut self, eraser: bool) -> bool {
        std::mem::replace(&mut self.eraser_end, eraser) != eraser
    }

    /// Runs a tool command (`tool-pen` to `tool-select`, `crop-pdf`): the tool it names
    /// is what the pen does now. An open tool or shape pop-up closes, since
    /// its lit cell no longer matches. `width-up` and `width-down` step the
    /// width along the list, and close an open width pop-up. False for any
    /// other command.
    pub fn command(&mut self, command: Command) -> bool {
        if matches!(command, Command::WidthUp | Command::WidthDown) {
            let widths = self.widths.widths();
            let at = widths.iter().position(|&w| w == self.width).unwrap_or(0);
            let to = match command {
                Command::WidthUp => (at + 1).min(widths.len() - 1),
                _ => at.saturating_sub(1),
            };
            self.width = widths[to];
            if self.open_kind() == Some(Kind::Width) {
                self.popup = None;
            }
            return true;
        }
        self.tool = match command {
            Command::ToolPen => Tool::Pen,
            Command::ToolEraser => Tool::Eraser,
            Command::ToolLasso => Tool::Lasso,
            Command::ToolText => Tool::Text,
            Command::ToolBox => Tool::Box,
            Command::ToolArrow => Tool::Arrow,
            Command::ToolSelect => Tool::Select,
            Command::CropPdf => Tool::CropPdf,
            _ => return false,
        };
        if matches!(self.open_kind(), Some(Kind::Tool | Kind::Shape)) {
            self.popup = None;
        }
        true
    }

    /// The pen's colour.
    pub fn colour(&self) -> Rgb {
        self.colour
    }

    /// The colour picked from the pop-up since the last call, if any (the
    /// pen's colour, picked again too): it recolours the selection.
    pub fn picked(&mut self) -> Option<Rgb> {
        self.picked.take()
    }

    /// The open pop-up's kind, if one is open.
    pub fn open_kind(&self) -> Option<Kind> {
        self.popup.as_ref().map(|p| p.kind)
    }

    /// A new palette, from a reloaded config. The pen keeps its colour; an
    /// open colour pop-up closes, since its cells no longer match.
    pub fn set_palette(&mut self, palette: Palette) {
        if palette != self.palette && self.open_kind() == Some(Kind::Colour) {
            self.popup = None;
        }
        self.palette = palette;
    }

    /// New widths, from a reloaded config. The pen keeps its width; an open
    /// width pop-up closes, since its cells no longer match.
    pub fn set_widths(&mut self, widths: Widths) {
        if widths != self.widths && self.open_kind() == Some(Kind::Width) {
            self.popup = None;
        }
        self.widths = widths;
    }

    /// The pen or pointer is at `at`, in the window's points.
    pub fn moved(&mut self, at: Point) {
        self.pen = Some(at);
    }

    /// Opens the `kind` pop-up where the pen last was, or at the centre of
    /// `within` (the focused pane, in the window's points) before it has
    /// been in the window, inside `within`, with the current choice lit.
    /// The same kind open already closes instead; another kind is replaced.
    pub fn open(&mut self, kind: Kind, within: Rect) {
        if self.open_kind() == Some(kind) {
            self.popup = None;
            return;
        }
        let (count, cell) = match kind {
            Kind::Colour => (self.palette.colours().len(), layout::SWATCH),
            Kind::Shape => (SHAPES.len(), layout::WORD),
            Kind::Tool => (TOOLS.len(), layout::WORD),
            Kind::Width => (self.widths.widths().len(), layout::WORD),
        };
        let lit = match kind {
            Kind::Colour => self.palette.colours().iter().position(|&c| c == self.colour),
            Kind::Shape => SHAPES.iter().position(|&t| t == self.tool),
            Kind::Tool => TOOLS.iter().position(|&t| t == self.tool),
            Kind::Width => self.widths.widths().iter().position(|&w| w == self.width),
        };
        let centre = Point { x: within.x + within.width / 2.0, y: within.y + within.height / 2.0 };
        let place = layout::popup(self.pen.unwrap_or(centre), count, cell, within);
        self.popup = Some(Popup { kind, place, lit: lit.unwrap_or(0) });
    }

    /// Closes the open pop-up, if any: a button tapped elsewhere ran.
    pub fn close(&mut self) {
        self.popup = None;
    }

    /// A press of the pen or pointer at `at`. With a pop-up open, a press on
    /// a cell picks it, a press elsewhere on the pop-up does nothing, and a
    /// press outside closes it; each is used up (true). With none open, the
    /// press is not the pop-up's (false).
    pub fn press(&mut self, at: Point) -> bool {
        self.pen = Some(at);
        let Some(popup) = &self.popup else { return false };
        if let Some(i) = popup.place.cells.iter().position(|c| c.contains(at)) {
            self.pick(i);
        } else if !popup.place.panel.contains(at) {
            self.popup = None;
        }
        true
    }

    /// A key press. With a pop-up open, its keys (escape, return, space, the
    /// arrows, 1 to 9) are used up (true); any other key, or any key with no
    /// pop-up open, is not the pop-up's (false).
    pub fn key(&mut self, chord: Chord) -> bool {
        let Some(popup) = &mut self.popup else { return false };
        let count = popup.place.cells.len();
        let pick = match chord {
            ESCAPE => {
                self.popup = None;
                None
            }
            RETURN | SPACE => Some(popup.lit),
            LEFT | UP => {
                popup.lit = (popup.lit + count - 1) % count;
                None
            }
            RIGHT | DOWN => {
                popup.lit = (popup.lit + 1) % count;
                None
            }
            _ => match DIGITS.iter().position(|&d| d == chord) {
                Some(i) => (i < count).then_some(i),
                None => return false,
            },
        };
        if let Some(i) = pick {
            self.pick(i);
        }
        true
    }

    /// Makes cell `i` of the open pop-up the choice, and closes it.
    fn pick(&mut self, i: usize) {
        let Some(popup) = self.popup.take() else { return };
        match popup.kind {
            Kind::Colour => {
                self.colour = self.palette.colours()[i];
                self.picked = Some(self.colour);
            }
            Kind::Shape => self.tool = SHAPES[i],
            Kind::Tool => self.tool = TOOLS[i],
            Kind::Width => self.width = self.widths.widths()[i],
        }
    }

    /// The open pop-up as labels in `theme`'s colours, to draw on top of
    /// everything else: a panel with a border, then each cell. A colour is
    /// a swatch, with a ring when lit; a word is lit by swapping its colours;
    /// a width is a line that wide in the pen's colour, framed when lit.
    pub fn labels(&self, theme: &Theme) -> Vec<Label> {
        let Some(popup) = &self.popup else { return Vec::new() };
        let fill = |frame: Rect, colour: Rgb| Label {
            text: String::new(),
            colour,
            fill: Some(colour),
            frame,
        };
        let panel = popup.place.panel;
        let mut labels = vec![fill(panel, theme.muted), fill(panel.outset(-1.0), theme.background)];
        for (i, &cell) in popup.place.cells.iter().enumerate() {
            let lit = i == popup.lit;
            let word = |tool: Tool| Label {
                text: tool.name().to_string(),
                colour: if lit { theme.background } else { theme.text },
                fill: lit.then_some(theme.text),
                frame: cell,
            };
            match popup.kind {
                Kind::Colour => {
                    if lit {
                        labels.push(fill(cell.outset(2.0), theme.text));
                    }
                    labels.push(fill(cell, self.palette.colours()[i]));
                }
                Kind::Shape => labels.push(word(SHAPES[i])),
                Kind::Tool => labels.push(word(TOOLS[i])),
                Kind::Width => {
                    if lit {
                        labels.push(fill(cell, theme.text));
                        labels.push(fill(cell.outset(-1.0), theme.background));
                    }
                    labels.push(fill(line(cell, self.widths.widths()[i]), self.colour));
                }
            }
        }
        labels
    }
}

/// A line `width` wide across the middle of `cell`, inside its edges: as
/// wide as the cell leaves room for, at least one point.
fn line(cell: Rect, width: Width) -> Rect {
    let high = width.points().clamp(1.0, cell.height - 2.0 * LINE_INSET);
    Rect {
        x: cell.x + LINE_INSET,
        y: cell.y + (cell.height - high) / 2.0,
        width: cell.width - 2.0 * LINE_INSET,
        height: high,
    }
}

/// The room between a width's line and the edges of its cell, in points.
const LINE_INSET: f64 = 4.0;

const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const RETURN: Chord = Chord::new(0, '\r');
const SPACE: Chord = Chord::new(0, ' ');
const UP: Chord = Chord::new(0, '\u{f700}');
const DOWN: Chord = Chord::new(0, '\u{f701}');
const LEFT: Chord = Chord::new(0, '\u{f702}');
const RIGHT: Chord = Chord::new(0, '\u{f703}');
const DIGITS: [Chord; Palette::MAX] = [
    Chord::new(0, '1'),
    Chord::new(0, '2'),
    Chord::new(0, '3'),
    Chord::new(0, '4'),
    Chord::new(0, '5'),
    Chord::new(0, '6'),
    Chord::new(0, '7'),
    Chord::new(0, '8'),
    Chord::new(0, '9'),
];

#[cfg(test)]
mod tests;
