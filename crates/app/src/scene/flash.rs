//! The flash: a pill at the top centre of the focused pane that names a
//! change the window shows nowhere else (SPEC P6: every state is visible).
//! A new tool, from its command or a pick in a pop-up, flashes its name and
//! its key ("Pen ⌘1"); a new width flashes it and the width pop-up's key
//! ("Width 4 ⇧⌘W"); so does a command with no other visible effect
//! (`reload-config`), a saved feedback note ("Feedback saved"), and a page
//! command says what it did ("Page 2 · 2/3", "Renamed: plan"; the words are
//! `scene::pages`'s). It shows for [`SHOWN`], then goes.
//!
//! Pure: `main` hands in the time and reads back the [`Pill`] to draw. The
//! pill carries when it goes, so the platform sets one timer for then and
//! draws once more; nothing redraws while it shows. The flash follows the
//! tool and the width as the window is drawn, from a shared reference: drawing is when a
//! change shows.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::commands::{Chord, Command, Keymap};
use crate::config::{Rgb, Theme, Width};
use crate::layout::Rect;
use crate::scene::Label;
use crate::scene::tools::Tool;

/// How long a flash shows.
pub const SHOWN: Duration = Duration::from_millis(800);
/// The pill's text size, in points.
pub const TEXT: f64 = 14.0;
/// The radius of the pill's corners, in points.
pub const CORNER: f64 = 8.0;
/// The pill's height, in points.
const HEIGHT: f64 = 28.0;
/// The room beside the text inside the pill, in points.
const PAD: f64 = 12.0;
/// The gap above the pill, from the top of the pane, in points.
const GAP: f64 = 12.0;
/// The width of one character of the text, about, in points.
const CHAR: f64 = TEXT * 0.6;
/// How much lighter than the pane the pill is, in each channel.
const STEP: u8 = 0x1a;

/// A flash on screen: its text, in a rounded rectangle ([`CORNER`]) of
/// `label.fill`, in [`TEXT`]-point type, until `until`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pill {
    pub label: Label,
    pub until: Instant,
}

/// What flashes: the tool and width last seen (none yet: the first does not
/// flash), and the flash showing, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Flash {
    tool: Cell<Tool>,
    width: Cell<Option<Width>>,
    shown: RefCell<Option<(String, Instant)>>,
}

impl Flash {
    /// Nothing showing; `tool` is the tool the pen starts with, which does
    /// not flash.
    pub fn new(tool: Tool) -> Self {
        Self { tool: Cell::new(tool), width: Cell::new(None), shown: RefCell::new(None) }
    }

    /// The pen's tool is `tool` at `now`: when it changed, it flashes with
    /// its key in `keys`.
    pub fn follow(&self, tool: Tool, keys: &Keymap, now: Instant) {
        if self.tool.replace(tool) != tool {
            self.command(command(tool), keys, now);
        }
    }

    /// The pen's width is `width` at `now`: when it changed since the last
    /// width seen, it flashes with the width pop-up's key in `keys`.
    pub fn follow_width(&self, width: Width, keys: &Keymap, now: Instant) {
        if self.width.replace(Some(width)).is_some_and(|was| was != width) {
            let text = format!("Width {}", width.text());
            self.show(text, keys.chord(Command::PickWidth), now);
        }
    }

    /// `command` ran at `now`: it flashes, with its key in `keys` if it has
    /// one, in place of any flash showing.
    pub fn command(&self, command: Command, keys: &Keymap, now: Instant) {
        self.show(command.title().to_string(), keys.chord(command), now);
    }

    /// `text` flashes from `now`, then `chord` if there is one, in place of
    /// any flash showing.
    fn show(&self, mut text: String, chord: Option<Chord>, now: Instant) {
        if let Some(chord) = chord {
            text.push(' ');
            text.push_str(&chord.glyphs());
        }
        self.say(text, now);
    }

    /// `text` flashes from `now`, in place of any flash showing: what a
    /// command did, when its title alone says too little ("Page 2 · 2/3").
    pub fn say(&self, text: String, now: Instant) {
        self.shown.replace(Some((text, now + SHOWN)));
    }

    /// Lets a flash past its time go at `now`. True when one went: the
    /// window must be drawn again without it.
    pub fn fade(&self, now: Instant) -> bool {
        let gone = self.shown.borrow().as_ref().is_some_and(|(_, until)| *until <= now);
        if gone {
            self.shown.replace(None);
        }
        gone
    }

    /// The flash showing at `now`, if any, at the top centre of `pane` (the
    /// focused pane, in the window's points), in `theme`'s colours on a
    /// step lighter than the pane.
    pub fn pill(&self, pane: Rect, theme: &Theme, now: Instant) -> Option<Pill> {
        let shown = self.shown.borrow();
        let (text, until) = shown.as_ref().filter(|(_, until)| *until > now)?;
        #[expect(clippy::cast_precision_loss, reason = "a flash is a few characters long")]
        let wide = text.chars().count() as f64 * CHAR + 2.0 * PAD;
        let width = wide.min(pane.width);
        let frame = Rect {
            x: pane.x + (pane.width - width) / 2.0,
            y: pane.y + GAP.min((pane.height - HEIGHT).max(0.0)),
            width,
            height: HEIGHT,
        };
        let fill = lighter(theme.background);
        let label = Label { text: text.clone(), colour: theme.text, fill: Some(fill), frame };
        Some(Pill { label, until: *until })
    }
}

/// The command that picks `tool`: the key and the name a flash shows.
fn command(tool: Tool) -> Command {
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

/// `colour` a [`STEP`] lighter in each channel.
fn lighter(colour: Rgb) -> Rgb {
    let up = |c: u8| c.saturating_add(STEP);
    Rgb { r: up(colour.r), g: up(colour.g), b: up(colour.b) }
}

#[cfg(test)]
mod tests;
