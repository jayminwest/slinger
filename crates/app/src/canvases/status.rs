//! What each pane's footer says (`scene::status`): on a canvas pane, the
//! pen, whether a text is open or items are selected on its page, the page
//! and the zoom; on an agent UI pane, its UI's name. The decisions are
//! `scene::status`'s; this module finds the pane's page, view and state.
//! Also the page menu's list, and a tap there that turns a pane to a page.

use std::path::Path;

use super::{Canvases, canvas_of};
use crate::commands::Keymap;
use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Rect};
use crate::panes::{Kind, Panes};
use crate::scene::pages;
use crate::scene::sidebar::Agent;
use crate::scene::status::{self, Footer, Pen, Shows, State};
use crate::scene::title;
use crate::scene::tools::Tool;

impl Canvases {
    /// The pen's line width, in canvas units: new strokes take it, and the
    /// footers show it.
    pub fn set_pen_width(&mut self, width: f64) {
        self.ink.set_width(width);
    }

    /// The footer of each pane of `panes` laid in `area`, in `theme`'s
    /// colours, with the pen's `tool`, `colour` and width and the live keys
    /// of `keys`. With the sidebar `collapsed`, the pane at the area's
    /// bottom left has its button.
    pub fn footers(
        &self,
        panes: &Panes,
        area: Rect,
        theme: &Theme,
        (tool, colour): (Tool, Rgb),
        keys: &Keymap,
        collapsed: bool,
    ) -> Vec<Footer> {
        let placed = panes.place(area);
        let close = placed.len() > 1;
        let corner = |frame: Rect| {
            let bottom = (frame.y + frame.height - area.y - area.height).abs() < 0.5;
            collapsed && (frame.x - area.x).abs() < 0.5 && bottom
        };
        let footer = |&(id, pane, frame): &(PaneId, &crate::panes::Pane, Rect)| {
            let sidebar = corner(frame);
            if pane.kind == Kind::Ui {
                let shows = Shows {
                    pen: None,
                    centre: &pane.name,
                    pages: false,
                    zoom: None,
                    sidebar,
                    close,
                    sling: false,
                    agent: None,
                };
                return status::footer(frame, theme, keys, &shows);
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
            let zoom = self.views.iter().find(|(p, _)| *p == id).map(|(_, v)| v.viewport);
            let agent = self.agent_of(&pane.name);
            let label = agent.map(Agent::label);
            let shows = Shows {
                pen: Some(Pen { tool, colour, width: self.ink.width(), state }),
                centre: &centre,
                pages: !centre.is_empty(),
                zoom: Some(zoom.unwrap_or_else(crate::layout::Viewport::start).percent()),
                sidebar,
                close,
                sling: true,
                agent: label.as_deref().zip(agent.map(|a| a.status)),
            };
            status::footer(frame, theme, keys, &shows)
        };
        placed.iter().map(footer).collect()
    }

    /// The names of the pages of the canvas `pane` of `panes` shows, and
    /// the index of the one it shows; `None` until its page has arrived,
    /// or for an agent UI pane.
    pub fn page_names(&self, panes: &Panes, pane: PaneId) -> Option<(Vec<String>, usize)> {
        let (_, list, at) = self.pages_of(panes, pane)?;
        let here = list.iter().position(|p| p.id == at).unwrap_or(0);
        Some((list.into_iter().map(|p| p.name).collect(), here))
    }

    /// Turns `pane` of `panes` to the page with index `index` of its
    /// canvas, and flashes it ("Page 2 · 2/3"). An index past the last does
    /// nothing.
    pub fn turn_to(&mut self, panes: &Panes, pane: PaneId, index: usize) {
        let Some((canvas, list, _)) = self.pages_of(panes, pane) else { return };
        let Some(to) = list.get(index) else { return };
        self.asks.turned(&list, &to.id);
        self.view(pane).page = Some((canvas, to.id.clone()));
    }

    /// The canvas `pane` of `panes` shows, and its file once the daemon
    /// has said where it is; `None` for an agent UI pane.
    pub fn file<'a>(
        &'a self,
        panes: &'a Panes,
        pane: PaneId,
    ) -> Option<(&'a str, Option<&'a str>)> {
        let canvas = canvas_of(panes, pane)?;
        let mut open = self.open.iter().filter(|o| o.canvas == canvas);
        Some((canvas, open.find_map(|o| o.file.as_deref())))
    }

    /// What `save` flashes for `pane` of `panes`, with `home` as `~`
    /// ([`title::saved`]): every change is written as it is made.
    pub(super) fn saved(&self, panes: &Panes, pane: PaneId, home: Option<&Path>) -> String {
        let file = self.file(panes, pane);
        let settled = file.is_none_or(|(canvas, _)| self.ink.settled(canvas));
        let problem = self.ink.problem().map(|p| p.message.as_str());
        title::saved(file, settled, problem, home)
    }
}
