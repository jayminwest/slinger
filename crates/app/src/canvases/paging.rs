//! The canvas commands on a pane (SPEC D6): pan and zoom; the next and
//! previous page, a new page, a rename and a removal, which turn only that
//! pane; and undo and redo, the user's own steps on its canvas. Changes go
//! through the ink's queue, in turn with the pen's, so an undo pressed
//! right after a stroke undoes that stroke. The decisions are
//! `scene::pages`'s; this module finds the pane's canvas and page.

use super::{Canvases, canvas_of, find, pane_shown, wanted};
use crate::backend::Follower;
use crate::commands::Command;
use crate::config::Theme;
use crate::layout::{self, PaneId, Point, Rect};
use crate::panes::{Kind, Panes};
use crate::scene::Label;
use crate::scene::ink::Change;
use crate::scene::pages::{self, Page, PageOp};
use crate::scene::status;

/// How far a pan command moves the canvas, in screen points.
const PAN_STEP: f64 = 80.0;
/// How much a zoom command zooms in, or out.
const ZOOM_STEP: f64 = 1.25;

impl Canvases {
    /// Runs a canvas command on `pane` of `panes`, laid at `frame`; false
    /// for a command that is not about canvases. A page command on a pane
    /// whose page has not arrived does nothing.
    pub fn command(
        &mut self,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        command: Command,
    ) -> bool {
        let centre = Point { x: frame.width / 2.0, y: frame.height / 2.0 };
        match command {
            Command::PanLeft => self.pan(pane, PAN_STEP, 0.0),
            Command::PanRight => self.pan(pane, -PAN_STEP, 0.0),
            Command::PanUp => self.pan(pane, 0.0, PAN_STEP),
            Command::PanDown => self.pan(pane, 0.0, -PAN_STEP),
            Command::ZoomIn => self.zoom(pane, centre, ZOOM_STEP),
            Command::ZoomOut => self.zoom(pane, centre, 1.0 / ZOOM_STEP),
            Command::NextPage | Command::PreviousPage => {
                self.turn(panes, pane, command == Command::NextPage);
            }
            Command::NewPage => self.new_page(panes, pane),
            Command::RenamePage => self.rename_page(panes, (pane, frame)),
            Command::RemovePage => self.remove_page(panes, pane),
            Command::Undo | Command::Redo => self.step(panes, pane, command == Command::Redo),
            Command::PlaceButton => self.place_button(panes, pane, frame),
            _ => return false,
        }
        true
    }

    /// The canvas `pane` of `panes` shows, its pages and the one shown,
    /// once that page has arrived.
    pub(super) fn pages_of(
        &self,
        panes: &Panes,
        pane: PaneId,
    ) -> Option<(String, Vec<Page>, String)> {
        let canvas = canvas_of(panes, pane)?;
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas))?;
        let mirror = open.mirror.as_ref()?;
        Some((canvas.to_string(), mirror.pages().to_vec(), mirror.page().to_string()))
    }

    /// What the last page command did, once, for the flash.
    pub fn said(&mut self) -> Option<String> {
        self.asks.said()
    }

    /// Turns `pane` to the next page, or the previous one.
    fn turn(&mut self, panes: &Panes, pane: PaneId, forward: bool) {
        let Some((canvas, list, at)) = self.pages_of(panes, pane) else { return };
        if let Some(to) = pages::step(&list, &at, forward) {
            self.asks.turned(&list, to);
            self.view(pane).page = Some((canvas, to.to_string()));
        }
    }

    /// Adds a page after the last; `pane` turns to it once it is written.
    fn new_page(&mut self, panes: &Panes, pane: PaneId) {
        let Some((canvas, list, _)) = self.pages_of(panes, pane) else { return };
        let op = PageOp::Add { name: pages::new_name(&list) };
        self.asks.changed(&list, &op);
        let key = self.ink.push_keyed(Change::Page { canvas: canvas.clone(), op });
        self.turning.push((key, pane, canvas));
        self.send();
    }

    /// Asks for a new name for the page `pane` shows, in an entry in its
    /// `frame`; an open entry closes.
    fn rename_page(&mut self, panes: &Panes, (pane, frame): (PaneId, Rect)) {
        let Some((canvas, list, at)) = self.pages_of(panes, pane) else { return };
        let Some(shown) = list.iter().find(|p| p.id == at) else { return };
        self.shapes.end(&mut self.ink);
        self.close_entries();
        self.asks.rename((pane, frame), &canvas, shown);
        self.send();
    }

    /// Removes the page `pane` shows, asking first when it has items. The
    /// last page stays, and says so.
    fn remove_page(&mut self, panes: &Panes, pane: PaneId) {
        let Some((canvas, list, at)) = self.pages_of(panes, pane) else { return };
        let Some(shown) = list.iter().find(|p| p.id == at) else { return };
        if list.len() == 1 {
            self.asks.kept_last();
            return;
        }
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane).len();
        if let Some((canvas, op)) = self.asks.remove(pane, &canvas, shown, items) {
            self.change_pages(canvas, op);
        }
    }

    /// Sends `op` on `canvas` in its turn, and flashes what it does. The
    /// panes on a page it removes go to the page beside it at once.
    pub(super) fn change_pages(&mut self, canvas: String, op: PageOp) {
        let mut open = self.open.iter().filter(|o| o.canvas == canvas);
        let list = open.find_map(|o| o.mirror.as_ref()).map(|m| m.pages().to_vec());
        let list = list.unwrap_or_default();
        self.asks.changed(&list, &op);
        if let PageOp::Remove { page } = &op {
            let next = pages::after_remove(&list, page);
            self.leave(&canvas, page, next.map(str::to_string).as_deref());
        }
        self.ink.push(Change::Page { canvas, op });
        self.send();
    }

    /// Page `gone` of `canvas` was removed (the feed says so): the panes
    /// on it go to `next`, and its follower follows that page instead (the
    /// first page, for one that followed the first).
    pub(super) fn gone(&mut self, canvas: &str, gone: &str, next: Option<&str>) {
        self.leave(canvas, gone, next);
        let on = |o: &&mut super::Open| {
            o.canvas == canvas && o.mirror.as_ref().is_some_and(|m| m.page() == gone)
        };
        for open in self.open.iter_mut().filter(on) {
            open.asked = open.asked.as_ref().and(next.map(str::to_string));
            let (daemon, asked) = (self.daemon.clone(), open.asked.clone());
            open.follower = Follower::start(daemon, canvas.to_string(), asked);
        }
    }

    /// The panes that asked for page `gone` of `canvas` show `next` (the
    /// first page without one) instead.
    fn leave(&mut self, canvas: &str, gone: &str, next: Option<&str>) {
        for (_, view) in &mut self.views {
            if view.page.as_ref().is_some_and(|(c, p)| c == canvas && p == gone) {
                view.page = next.map(|n| (canvas.to_string(), n.to_string()));
            }
        }
    }

    /// Undoes the user's last step on the canvas `pane` shows, or redoes
    /// the last undone, after every change made before it. A text being
    /// typed ends first, so it is a step too.
    fn step(&mut self, panes: &Panes, pane: PaneId, redo: bool) {
        let Some(canvas) = canvas_of(panes, pane) else { return };
        let canvas = canvas.to_string();
        self.shapes.end(&mut self.ink);
        self.ink.push(Change::Step { canvas, redo });
        self.send();
    }

    /// The question a page command asks, if any, at the foot of the canvas
    /// pane of `panes` laid in `area` it was asked in, above its footer.
    pub fn labels(&self, panes: &Panes, area: Rect, theme: &Theme) -> Vec<Label> {
        let canvases = panes.place(area).into_iter().filter(|(_, p, _)| p.kind == Kind::Canvas);
        let line = |(id, _, frame): (PaneId, &crate::panes::Pane, Rect)| {
            let text = self.asks.prompt(id)?;
            let frame = layout::foot(status::content(frame));
            Some(Label { text, colour: theme.text, fill: None, frame })
        };
        canvases.filter_map(line).collect()
    }
}
