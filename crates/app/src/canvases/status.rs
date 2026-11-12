//! What each pane's footer says (`scene::status`): on a canvas pane, the
//! pen, whether a text is open or items are selected on its page, and the
//! page; on an agent UI pane, its UI's name. The decisions are
//! `scene::status`'s; this module finds the pane's page and state.

use super::Canvases;
use crate::commands::{Command, Keymap};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::panes::{Kind, Panes};
use crate::scene::pages;
use crate::scene::status::{self, Footer, Pen, State};
use crate::scene::tools::Tool;

impl Canvases {
    /// The pen's line width, in canvas units: new strokes take it, and the
    /// footers show it.
    pub fn set_pen_width(&mut self, width: f64) {
        self.ink.set_width(width);
    }

    /// The footer of each pane of `panes` laid in `area`, in `theme`'s
    /// colours, with the pen's `tool`, `colour` and width and the live keys
    /// of `keys`. Their targets are kept for [`Canvases::hint`].
    pub fn footers(
        &mut self,
        panes: &Panes,
        area: Rect,
        theme: &Theme,
        (tool, colour): (Tool, Rgb),
        keys: &Keymap,
    ) -> Vec<Footer> {
        let footer = |(id, pane, frame): (_, &crate::panes::Pane, Rect)| {
            if pane.kind == Kind::Ui {
                return status::footer(frame, theme, keys, None, &pane.name);
            }
            let page = self.pages_of(panes, id);
            let typing = page.as_ref().is_some_and(|(canvas, _, at)| {
                self.shapes.typing_on() == Some((canvas.as_str(), at.as_str()))
            });
            let selected = self.selection().filter(|_| self.shows_selection(panes, id));
            let state = match selected {
                _ if typing => State::Typing,
                Some(selection) if !selection.ids.is_empty() => {
                    State::Selected(selection.ids.len())
                }
                _ => State::Idle,
            };
            let centre = page.map(|(_, list, at)| pages::line(&list, &at)).unwrap_or_default();
            status::footer(
                frame,
                theme,
                keys,
                Some(Pen { tool, colour, width: self.ink.width(), state }),
                &centre,
            )
        };
        let footers: Vec<Footer> = panes.place(area).into_iter().map(footer).collect();
        self.hints = footers.iter().flat_map(|f| f.targets.iter().copied()).collect();
        footers
    }

    /// The command of the footer target under `at`, a point in the window,
    /// as the footers were last laid: a tap on a key hint opens the key
    /// overlay or the palette.
    pub fn hint(&self, at: Point) -> Option<Command> {
        self.hints.iter().find(|(frame, _)| frame.contains(at)).map(|&(_, command)| command)
    }
}
