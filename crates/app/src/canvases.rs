//! The canvases the panes show. Each canvas a pane shows is followed once,
//! however many panes show it: a [`Follower`] on its own thread, and the
//! [`Mirror`] its news keeps up to date. Each canvas pane has its own
//! viewport, and keeps the last scene it drew, so the next one repaints only
//! what changed. The pen draws on a canvas pane: its [`Ink`] shows over the
//! page at once and goes to the daemon through an [`Outbox`]. The eraser and
//! the lasso work on the page as shown ([`Select`]), and so do the text, box
//! and arrow tools ([`Shapes`]); what they change goes the same way, in turn
//! with the strokes. Runs on the main thread and
//! never waits: the followers and the outbox do.

use std::borrow::Cow;
use std::rc::Rc;

use slinger_control::Client;

use crate::backend::{Follower, News, Outbox, Problem};
use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Point, Rect, Size, Viewport};
use crate::mirror::{Applied, Mirror};
use crate::panes::{Kind, Panes};
use crate::scene::canvas::Item;
use crate::scene::ink::{Ink, Pen};
use crate::scene::select::{Select, Selection};
use crate::scene::shapes::{Press, Shapes};
use crate::scene::tools::Tool;
use crate::scene::{CanvasPane, canvas};

/// The canvases of a window's panes.
pub struct Canvases {
    daemon: Client,
    open: Vec<Open>,
    views: Vec<(PaneId, View)>,
    ink: Ink,
    select: Select,
    shapes: Shapes,
    outbox: Outbox,
    /// The pane the pen draws in, and its frame in the window, while down.
    pen: Option<(PaneId, Rect)>,
    /// When the oldest pen event no scene has shown yet happened.
    unshown: Option<f64>,
}

/// One canvas followed: its page as last heard, the number of the last
/// change heard, and what is wrong, if anything.
struct Open {
    canvas: String,
    follower: Follower,
    mirror: Option<Mirror>,
    heard: u64,
    problem: Option<Problem>,
}

/// One canvas pane: where it looks, and what it last drew.
#[derive(Default)]
struct View {
    viewport: Viewport,
    last: Option<Rc<canvas::Scene>>,
}

impl Canvases {
    /// None followed yet; [`Canvases::show`] starts following.
    pub fn new(daemon: Client) -> Self {
        let outbox = Outbox::start(daemon.clone());
        let (ink, select) = (Ink::default(), Select::default());
        let (open, views) = (Vec::new(), Vec::new());
        let shapes = Shapes::default();
        Self { daemon, open, views, ink, select, shapes, outbox, pen: None, unshown: None }
    }

    /// Follows every canvas, and sends ink not yet sent, through `daemon`
    /// instead, from the start.
    pub fn reconnect(&mut self, daemon: Client) {
        self.outbox.reconnect(daemon.clone());
        self.daemon = daemon;
        self.open.clear();
    }

    /// Takes in what the followers heard and what the daemon said to the
    /// ink, and sends the next stroke. Returns whether anything a pane
    /// shows changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut lost = Vec::new();
        for open in &mut self.open {
            for news in open.follower.news() {
                changed = true;
                match news {
                    News::Shown(page) => {
                        open.heard = page.next.saturating_sub(1);
                        (open.mirror, open.problem) = (Some(Mirror::new(&page)), None);
                    }
                    News::Problem(problem) => open.problem = Some(problem),
                    News::Changed(feed) => {
                        open.heard = feed.next.saturating_sub(1);
                        let Some(mirror) = &mut open.mirror else { continue };
                        if mirror.apply(&feed) == Applied::Gone {
                            lost.push(open.canvas.clone());
                        }
                    }
                }
            }
        }
        for canvas in lost {
            self.turn(&canvas, None);
        }
        self.send() || changed
    }

    /// Takes the daemon's answers to the ink, lets go of the changes the
    /// mirrors now hold, and hands the next change to the outbox. Returns
    /// whether there were answers.
    fn send(&mut self) -> bool {
        let answers = self.outbox.answers();
        let answered = !answers.is_empty();
        for (key, answer) in answers {
            self.ink.answered(key, answer);
        }
        for open in self.open.iter().filter(|o| o.mirror.is_some()) {
            self.ink.heard(&open.canvas, open.heard);
        }
        if let Some((key, change)) = self.ink.next() {
            self.outbox.send(key, change);
        }
        answered
    }

    /// What is wrong with any canvas followed, or with sending the ink.
    pub fn problems(&self) -> impl Iterator<Item = &Problem> {
        self.open.iter().filter_map(|o| o.problem.as_ref()).chain(self.ink.problem())
    }

    /// The pen went down in `pane` of `panes`, laid at `frame` in the
    /// window, with `tool` and `colour`: a press outside a text being typed
    /// ends it; else the pen starts a stroke on the page the pane shows, the
    /// eraser an erase, the lasso a loop or a drag of the selection, the
    /// text tool a text, the box and arrow tools a drag. Returns false, and
    /// nothing starts, for touch (a palm) or a pane whose canvas page has
    /// not arrived.
    pub fn pen_down(
        &mut self,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        pen: Pen,
        (tool, colour): (Tool, Rgb),
    ) -> bool {
        let Some(canvas) = canvas_of(panes, pane).filter(|_| !pen.touch) else { return false };
        let open = self.open.iter().find(|o| o.canvas == canvas);
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string()) else {
            return false;
        };
        let viewport = self.view(pane).viewport;
        let at = to_canvas(viewport, frame, pen.at);
        let items = pane_shown(&self.open, &self.ink, panes, pane);
        let press = Press { at, when: pen.when, zoom: viewport.zoom() };
        let here = (canvas, shown.as_str());
        if self.shapes.down((tool, colour), here, press, &items, &mut self.ink) {
            drop(items);
            self.send();
        } else if tool == Tool::Pen {
            self.ink.down(canvas, &shown, colour, at);
        } else if !self.select.down(tool, here, at, viewport.zoom(), &items) {
            return false;
        }
        self.pen = Some((pane, frame));
        self.unshown = Some(pen.when);
        true
    }

    /// The pen moved while down: the stroke or the gesture under it goes on.
    /// Returns false when none is under it.
    pub fn pen_drag(&mut self, panes: &Panes, pen: Pen) -> bool {
        let Some((pane, frame)) = self.pen else { return false };
        let at = to_canvas(self.view(pane).viewport, frame, pen.at);
        self.unshown = Some(self.unshown.map_or(pen.when, |t| t.min(pen.when)));
        if self.ink.drag(at) || self.shapes.drag(at) {
            return true;
        }
        let items = pane_shown(&self.open, &self.ink, panes, pane);
        self.select.drag(at, &items)
    }

    /// The pen lifted: the stroke, box, arrow, erase or move is finished
    /// and goes to the daemon in its turn, or the loop selects. Returns
    /// false when nothing was under the pen.
    pub fn pen_up(&mut self, panes: &Panes, pen: Pen) -> bool {
        let Some((pane, frame)) = self.pen.take() else { return false };
        let at = to_canvas(self.view(pane).viewport, frame, pen.at);
        let ended = self.ink.up(Some(at)) || {
            let items = pane_shown(&self.open, &self.ink, panes, pane);
            self.shapes.up(Some(at), &items, &mut self.ink)
                || self.select.up(Some(at), &items, &mut self.ink)
        };
        self.unshown = None;
        self.send();
        ended
    }

    /// Whether a text is being typed: the keys that type go to it.
    pub fn editing(&self) -> bool {
        self.shapes.editing()
    }

    /// Typed `text` goes into the text being typed; escape ends it, and the
    /// text goes to the daemon. Returns false when no text is being typed.
    pub fn typed(&mut self, text: &str) -> bool {
        let used = self.shapes.typed(text, &mut self.ink);
        if used {
            self.send();
        }
        used
    }

    /// The selection, if any: one page's items, by the server's ids.
    pub fn selection(&self) -> Option<&Selection> {
        self.select.selection()
    }

    /// Removes the selection, in one step, when `pane` of `panes` shows its
    /// canvas. Returns false when it does not, or there is none.
    pub fn delete_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.selection().is_some_and(|s| canvas_of(panes, pane) == Some(&s.canvas));
        shown && self.select.delete(&mut self.ink) && {
            self.send();
            true
        }
    }

    /// Clears the selection when `pane` of `panes` shows its canvas.
    /// Returns false when it does not, or there is none.
    pub fn clear_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.selection().is_some_and(|s| canvas_of(panes, pane) == Some(&s.canvas));
        shown && self.select.clear()
    }

    /// A paint showed ink `delay` seconds after its pen event.
    pub fn painted(&mut self, delay: f64) {
        self.ink.painted(delay);
    }

    /// The last stroke's pen-to-paint delays as one line (SPEC O2), once.
    pub fn delay_log(&mut self) -> Option<String> {
        self.ink.delay_log()
    }

    /// Shows the page after the one `pane`'s canvas shows, in every pane on
    /// that canvas. Until it arrives, the panes keep the page they have.
    pub fn next_page(&mut self, panes: &Panes, pane: PaneId) {
        let Some(canvas) = canvas_of(panes, pane) else { return };
        let open = self.open.iter().find(|o| o.canvas == canvas);
        let next = open.and_then(|o| o.mirror.as_ref()).map(|m| m.next_page().to_string());
        if next.is_some() {
            self.turn(canvas, next);
        }
    }

    /// Follows `page` of `canvas` from now on.
    fn turn(&mut self, canvas: &str, page: Option<String>) {
        if let Some(open) = self.open.iter_mut().find(|o| o.canvas == canvas) {
            open.follower = Follower::start(self.daemon.clone(), canvas.to_string(), page);
        }
    }

    /// Drags `pane`'s canvas by `dx`, `dy` screen points.
    pub fn pan(&mut self, pane: PaneId, dx: f64, dy: f64) {
        let view = self.view(pane);
        view.viewport = view.viewport.pan(dx, dy);
    }

    /// Zooms `pane`'s canvas by `factor` about `about`, a point in the pane.
    pub fn zoom(&mut self, pane: PaneId, about: Point, factor: f64) {
        let view = self.view(pane);
        view.viewport = view.viewport.zoom_about(about, factor);
    }

    fn view(&mut self, pane: PaneId) -> &mut View {
        let at = self.views.iter().position(|(p, _)| *p == pane).unwrap_or_else(|| {
            self.views.push((pane, View::default()));
            self.views.len() - 1
        });
        &mut self.views[at].1
    }

    /// The scene of each canvas pane of `panes` laid in `area` whose page
    /// has arrived. Starts following each canvas a pane shows, stops
    /// following the others, and forgets the views of panes that closed.
    pub fn show(&mut self, panes: &Panes, area: Rect, theme: &Theme) -> Vec<CanvasPane> {
        let placed = panes.place(area);
        let shown: Vec<&str> = placed
            .iter()
            .filter(|(_, pane, _)| pane.kind == Kind::Canvas)
            .map(|(_, pane, _)| pane.name.as_str())
            .collect();
        for o in self.open.iter().filter(|o| !shown.contains(&o.canvas.as_str())) {
            // No feed of it comes now: its strokes already written are let go.
            self.ink.heard(&o.canvas, u64::MAX);
        }
        self.open.retain(|o| shown.contains(&o.canvas.as_str()));
        for canvas in &shown {
            if !self.open.iter().any(|o| o.canvas == *canvas) {
                let follower = Follower::start(self.daemon.clone(), (*canvas).to_string(), None);
                self.open.push(Open {
                    canvas: (*canvas).to_string(),
                    follower,
                    mirror: None,
                    heard: 0,
                    problem: None,
                });
            }
        }
        self.views.retain(|(id, _)| placed.iter().any(|(p, _, _)| p == id));
        let mut scenes = Vec::new();
        for (id, pane, frame) in placed {
            let open =
                self.open.iter().find(|o| pane.kind == Kind::Canvas && o.canvas == pane.name);
            let Some(mirror) = open.and_then(|o| o.mirror.as_ref()) else { continue };
            let screen = Size { width: frame.width, height: frame.height };
            let at = self.views.iter().position(|(p, _)| *p == id);
            let at = at.unwrap_or_else(|| {
                self.views.push((id, View::default()));
                self.views.len() - 1
            });
            let view = &mut self.views[at].1;
            let last = view.last.as_deref();
            let here = (pane.name.as_str(), mirror.page());
            let items = self.ink.shown(here.0, here.1, mirror.items());
            self.select.keep(here, &items);
            let items = self.select.shown(here, items, view.viewport.zoom(), theme.muted);
            let items = self.shapes.shown(here, items);
            let items = items.iter().map(AsRef::as_ref);
            let mut scene = canvas::build(items, view.viewport, screen, theme, last);
            if self.pen.is_some_and(|(p, _)| p == id) {
                scene.pen_event = self.unshown.take();
            }
            let scene = Rc::new(scene);
            view.last = Some(Rc::clone(&scene));
            scenes.push(CanvasPane { pane: id, frame, scene });
        }
        scenes
    }
}

/// The canvas point under `at`, a point in the window, in a pane laid at
/// `frame` that looks through `viewport`.
fn to_canvas(viewport: Viewport, frame: Rect, at: Point) -> Point {
    viewport.to_canvas(Point { x: at.x - frame.x, y: at.y - frame.y })
}

/// The page `pane` of `panes` shows, as the user sees it: the server's copy
/// with the ink laid over it; empty while it has not arrived.
fn pane_shown<'a>(open: &'a [Open], ink: &Ink, panes: &Panes, pane: PaneId) -> Vec<Cow<'a, Item>> {
    let Some(canvas) = canvas_of(panes, pane) else { return Vec::new() };
    let mirror = open.iter().find(|o| o.canvas == canvas).and_then(|o| o.mirror.as_ref());
    mirror.map_or_else(Vec::new, |m| ink.shown(canvas, m.page(), m.items()))
}

/// The canvas `pane` shows, if it shows one.
fn canvas_of(panes: &Panes, pane: PaneId) -> Option<&str> {
    let shown = panes.shows(pane);
    (shown.kind == Kind::Canvas).then_some(shown.name.as_str())
}
