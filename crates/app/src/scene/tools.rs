//! The pen's current tool, colour and width, the size a new text starts at,
//! and the pop-up at the pen that changes them (SPEC section 11). Pure: `main` hands in where the pen is,
//! presses and keys, and reads back the [`Tool`], the colour, the width and
//! the labels to draw. A canvas pane reads [`Tools::tool`], [`Tools::colour`]
//! and [`Tools::width`] to know what the pen does. The width is the pen's
//! line alone: boxes and arrows keep their one line width. The text size
//! ([`Tools::size`], one of [`SIZES`]) is a new text's alone: a text being
//! typed or edited keeps its own.
//!
//! A tablet pen's eraser end erases whatever tool is chosen: while it is
//! the end in use ([`Tools::set_eraser_end`]), [`Tools::tool`] is the eraser
//! (so the footer and the cursor say so), and the chosen tool
//! ([`Tools::chosen`]) comes back with the tip.
//!
//! The pen tool is the tablet's (docs/design/pen-less.md, pick 1B): while
//! the device last seen is a mouse or a trackpad ([`Tools::set_device`]),
//! the pen tool chosen acts as the select tool, so a mouse never draws a
//! stray line, unless `[pointer] press = "tool"` ([`Press`]). Every other
//! tool does with the mouse what it does with the pen, and a tablet press
//! always gets the chosen tool.
//!
//! A pop-up opens from a key command where the pen or pointer last was, or
//! at the focused pane's centre before either has been in the window. The
//! pen picks a cell; a digit picks the cell with that number; the arrow keys
//! move the highlight and return (or space) picks it. A pick, escape, a press
//! outside the pop-up, or the same command again closes it.

use std::time::Instant;

use crate::commands::{Chord, Command, Keymap};
use crate::config::{Config, Palette, Press, Rgb, Width, Widths};
use crate::layout::{self, Point, Rect};
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

/// The five pop-ups.
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
    /// The text sizes ([`SIZES`]).
    Size,
}

impl Kind {
    /// The pop-up a command opens: `pick-colour` to `pick-size`.
    pub fn opened_by(command: Command) -> Option<Kind> {
        match command {
            Command::PickColour => Some(Kind::Colour),
            Command::PickShape => Some(Kind::Shape),
            Command::PickTool => Some(Kind::Tool),
            Command::PickWidth => Some(Kind::Width),
            Command::PickSize => Some(Kind::Size),
            _ => None,
        }
    }
}

const SHAPES: [Tool; 2] = [Tool::Box, Tool::Arrow];
const TOOLS: [Tool; 5] = [Tool::Pen, Tool::Eraser, Tool::Lasso, Tool::Text, Tool::Select];

/// The sizes a new text can start at, in canvas points, smallest first:
/// small, the one every text started at before sizes could be picked
/// (the app's start), then larger headings.
pub const SIZES: [u16; 6] = [12, 16, 24, 36, 48, 72];

/// The size a new text starts at when the app starts: the core's default.
const START_SIZE: usize = 2;

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
    /// Whether the device last seen is a tablet pen, not a mouse or a trackpad.
    tablet: bool,
    /// What a mouse press does with the pen tool.
    press: Press,
    colour: Rgb,
    /// A colour picked from the pop-up and not yet taken ([`Tools::picked`]).
    picked: Option<Rgb>,
    palette: Palette,
    width: Width,
    widths: Widths,
    /// The index in [`SIZES`] of a new text's size.
    size: usize,
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
            tablet: false,
            press: Press::default(),
            colour,
            picked: None,
            palette,
            width,
            widths,
            size: START_SIZE,
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

    /// The pen as `config` sets it up: its palette, its widths and what a
    /// mouse press does with the pen tool.
    pub fn configured(config: &Config) -> Self {
        let mut tools = Self::new(config.palette.clone()).with_widths(config.widths.clone());
        tools.press = config.press;
        tools
    }

    /// A reloaded `config`: its palette ([`Tools::set_palette`]), its widths
    /// ([`Tools::set_widths`]) and what a mouse press does with the pen tool.
    pub fn set_config(&mut self, config: &Config) {
        self.set_palette(config.palette.clone());
        self.set_widths(config.widths.clone());
        self.press = config.press;
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
        self.flash.follow_size(self.size(), keys, now);
    }

    /// The size a new text starts at, in canvas points.
    pub fn size(&self) -> u16 {
        SIZES[self.size]
    }

    /// The pen's line width.
    pub fn width(&self) -> Width {
        self.width
    }

    /// What a press does: the eraser while the pen's eraser end is in use;
    /// the select tool for a mouse or a trackpad with the pen tool chosen
    /// (unless [`Press::Tool`]); else the chosen tool.
    pub fn tool(&self) -> Tool {
        match self.tool {
            _ if self.eraser_end => Tool::Eraser,
            Tool::Pen if !self.tablet && self.press == Press::Select => Tool::Select,
            tool => tool,
        }
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

    /// The device last seen is a tablet pen (`true`: it came near the
    /// tablet, or pressed) or a mouse or a trackpad (`false`: the pen left,
    /// or the mouse pressed). Returns whether that changed.
    pub fn set_device(&mut self, tablet: bool) -> bool {
        std::mem::replace(&mut self.tablet, tablet) != tablet
    }

    /// Runs a tool command (`tool-pen` to `tool-select`, `crop-pdf`): the tool it names
    /// is what the pen does now. An open tool or shape pop-up closes, since
    /// its lit cell no longer matches. `width-up` and `width-down` step the
    /// width along the list, and close an open width pop-up; `size-up` and
    /// `size-down` do the same for the text size. False for any other command.
    pub fn command(&mut self, command: Command) -> bool {
        if matches!(command, Command::SizeUp | Command::SizeDown) {
            self.size = match command {
                Command::SizeUp => (self.size + 1).min(SIZES.len() - 1),
                _ => self.size.saturating_sub(1),
            };
            if self.open_kind() == Some(Kind::Size) {
                self.popup = None;
            }
            return true;
        }
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

    /// Where the pen or pointer last was in the window, if it has been.
    pub fn pointer(&self) -> Option<Point> {
        self.pen
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
            Kind::Size => (SIZES.len(), layout::WORD),
        };
        let lit = match kind {
            Kind::Colour => self.palette.colours().iter().position(|&c| c == self.colour),
            Kind::Shape => SHAPES.iter().position(|&t| t == self.tool),
            Kind::Tool => TOOLS.iter().position(|&t| t == self.tool),
            Kind::Width => self.widths.widths().iter().position(|&w| w == self.width),
            Kind::Size => Some(self.size),
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
            Kind::Size => self.size = i,
        }
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

mod labels;

#[cfg(test)]
mod tests;
