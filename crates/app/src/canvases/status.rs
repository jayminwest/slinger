//! What each pane's footer says (`scene::status`): on a canvas pane, the
//! pen, whether a text is open or items are selected on its page, and the
//! page; on an agent UI pane, its UI's name. The decisions are
//! `scene::status`'s; this module finds the pane's page and state.

use super::Canvases;
use crate::commands::Keymap;
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::panes::{Kind, Panes};
use crate::scene::pages;
use crate::scene::status::{self, Footer, Pen, State};
use crate::scene::tools::Tool;

impl Canvases {
    /// The footer of each pane of `panes` laid in `area`, in `theme`'s
    /// colours, with the pen's `tool` and `colour` and the live keys of
    /// `keys`.
    pub fn footers(
        &self,
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
            status::footer(frame, theme, keys, Some(Pen { tool, colour, state }), &centre)
        };
        panes.place(area).into_iter().map(footer).collect()
    }
}
