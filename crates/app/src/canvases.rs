//! The canvases the panes show. Each canvas a pane shows is followed once,
//! however many panes show it: a [`Follower`] on its own thread, and the
//! [`Mirror`] its news keeps up to date. Each canvas pane has its own
//! viewport, and keeps the last scene it drew, so the next one repaints only
//! what changed. Runs on the main thread and never waits: the followers do.

use std::rc::Rc;

use slinger_control::Client;

use crate::backend::{Follower, News, Problem};
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect, Size, Viewport};
use crate::mirror::{Applied, Mirror};
use crate::panes::{Kind, Panes};
use crate::scene::{CanvasPane, canvas};

/// The canvases of a window's panes.
pub struct Canvases {
    daemon: Client,
    open: Vec<Open>,
    views: Vec<(PaneId, View)>,
}

/// One canvas followed: its page as last heard, and what is wrong, if anything.
struct Open {
    canvas: String,
    follower: Follower,
    mirror: Option<Mirror>,
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
        Self { daemon, open: Vec::new(), views: Vec::new() }
    }

    /// Follows every canvas through `daemon` instead, from the start.
    pub fn reconnect(&mut self, daemon: Client) {
        self.daemon = daemon;
        self.open.clear();
    }

    /// Takes in what the followers heard. Returns whether anything a pane
    /// shows changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut lost = Vec::new();
        for open in &mut self.open {
            for news in open.follower.news() {
                changed = true;
                match news {
                    News::Shown(page) => {
                        (open.mirror, open.problem) = (Some(Mirror::new(&page)), None);
                    }
                    News::Problem(problem) => open.problem = Some(problem),
                    News::Changed(feed) => {
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
        changed
    }

    /// What is wrong with any canvas followed.
    pub fn problems(&self) -> impl Iterator<Item = &Problem> {
        self.open.iter().filter_map(|o| o.problem.as_ref())
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
        self.open.retain(|o| shown.contains(&o.canvas.as_str()));
        for canvas in &shown {
            if !self.open.iter().any(|o| o.canvas == *canvas) {
                let follower = Follower::start(self.daemon.clone(), (*canvas).to_string(), None);
                self.open.push(Open {
                    canvas: (*canvas).to_string(),
                    follower,
                    mirror: None,
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
            let scene = Rc::new(canvas::build(mirror.items(), view.viewport, screen, theme, last));
            view.last = Some(Rc::clone(&scene));
            scenes.push(CanvasPane { pane: id, frame, scene });
        }
        scenes
    }
}

/// The canvas `pane` shows, if it shows one.
fn canvas_of(panes: &Panes, pane: PaneId) -> Option<&str> {
    let shown = panes.shows(pane);
    (shown.kind == Kind::Canvas).then_some(shown.name.as_str())
}
