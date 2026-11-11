//! The pen's current tool and colour, and the pop-up at the pen that changes
//! them (SPEC section 11). Pure: `main` hands in where the pen is, presses and
//! keys, and reads back the [`Tool`], the colour and the labels to draw. A
//! canvas pane reads [`Tools::tool`] and [`Tools::colour`] to know what the
//! pen does.
//!
//! A pop-up opens from a key command where the pen or pointer last was. The
//! pen picks a cell; a digit picks the cell with that number; the arrow keys
//! move the highlight and return (or space) picks it. A pick, escape, a press
//! outside the pop-up, or the same command again closes it.

use crate::commands::Chord;
use crate::config::{Palette, Rgb, Theme};
use crate::layout::{self, Point, Rect};
use crate::scene::Label;

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
        }
    }
}

/// The three pop-ups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The palette's colours.
    Colour,
    /// The shapes: box and arrow.
    Shape,
    /// The tools that are not shapes: pen, eraser, lasso and text.
    Tool,
}

const SHAPES: [Tool; 2] = [Tool::Box, Tool::Arrow];
const TOOLS: [Tool; 4] = [Tool::Pen, Tool::Eraser, Tool::Lasso, Tool::Text];

/// An open pop-up: what it offers, where its cells are, and which is lit.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    kind: Kind,
    place: layout::Popup,
    lit: usize,
}

/// The pen's state: its tool, its colour, the palette it picks from, where
/// it last was, and the open pop-up, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Tools {
    tool: Tool,
    colour: Rgb,
    palette: Palette,
    pen: Point,
    popup: Option<Popup>,
}

impl Tools {
    /// The pen with the palette's first colour, and no pop-up. Until the pen
    /// or pointer moves, a pop-up opens at `pen`.
    pub fn new(palette: Palette, pen: Point) -> Self {
        let colour = palette.colours()[0];
        Self { tool: Tool::Pen, colour, palette, pen, popup: None }
    }

    /// What the pen does.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The pen's colour.
    pub fn colour(&self) -> Rgb {
        self.colour
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

    /// The pen or pointer is at `at`, in the window's points.
    pub fn moved(&mut self, at: Point) {
        self.pen = at;
    }

    /// Opens the `kind` pop-up where the pen last was, inside `within` (the
    /// focused pane, in the window's points), with the current choice lit.
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
        };
        let lit = match kind {
            Kind::Colour => self.palette.colours().iter().position(|&c| c == self.colour),
            Kind::Shape => SHAPES.iter().position(|&t| t == self.tool),
            Kind::Tool => TOOLS.iter().position(|&t| t == self.tool),
        };
        let place = layout::popup(self.pen, count, cell, within);
        self.popup = Some(Popup { kind, place, lit: lit.unwrap_or(0) });
    }

    /// A press of the pen or pointer at `at`. With a pop-up open, a press on
    /// a cell picks it, a press elsewhere on the pop-up does nothing, and a
    /// press outside closes it; each is used up (true). With none open, the
    /// press is not the pop-up's (false).
    pub fn press(&mut self, at: Point) -> bool {
        self.pen = at;
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
            Kind::Colour => self.colour = self.palette.colours()[i],
            Kind::Shape => self.tool = SHAPES[i],
            Kind::Tool => self.tool = TOOLS[i],
        }
    }

    /// The open pop-up as labels in `theme`'s colours, to draw on top of
    /// everything else: a panel with a border, then each cell. A colour is
    /// a swatch, with a ring when lit; a word is lit by swapping its colours.
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
            }
        }
        labels
    }
}

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
