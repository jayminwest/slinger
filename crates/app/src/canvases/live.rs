//! The pages of the live items the canvas panes show
//! (docs/design/live-items.md). Each showing a pane draws is fetched once,
//! by its id, on a thread of its own ([`Lives`]), and shows over its item
//! once it has come ([`scene::live`]). A press on an item whose page shows
//! goes to the page, unless it grabs the selection: a live item is selected
//! with the lasso around it, then moved or deleted like any other.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slinger_control::Client;

use super::Canvases;
use crate::backend::{Problem, ui::fetch};
use crate::layout::Point;
use crate::scene::canvas::{Item, Shape};
use crate::scene::{Scene, live};
use crate::ui::{Page, Shown};

/// The live items' pages: asked for, and come.
pub(super) struct Lives {
    ask: Sender<String>,
    heard: Receiver<(String, Result<Shown, Problem>)>,
    asked: HashSet<String>,
    pages: HashMap<String, Arc<Page>>,
}

impl Lives {
    /// Fetches through `daemon`, on a thread that ends with this.
    pub(super) fn start(daemon: Client) -> Self {
        let (ask, asks) = mpsc::channel::<String>();
        let (tell, heard) = mpsc::channel();
        let spawned = thread::Builder::new().name("live items".into()).spawn(move || {
            for open in asks {
                let shown = fetch(&daemon, &open);
                if tell.send((open, shown)).is_err() {
                    break;
                }
            }
        });
        // Without the thread no page comes: each item keeps its box and name.
        drop(spawned);
        Self { ask, heard, asked: HashSet::new(), pages: HashMap::new() }
    }

    /// Takes the pages fetched. Returns whether any came. A showing that
    /// cannot show (gone, or a bridge this app does not speak) keeps its
    /// box and name.
    pub(super) fn poll(&mut self) -> bool {
        let mut came = false;
        for (open, shown) in self.heard.try_iter() {
            if let Some(page) = shown.ok().and_then(|s| Page::new(s, "canvas").ok()) {
                self.pages.insert(open, Arc::new(page));
                came = true;
            }
        }
        came
    }

    /// Whether `at`, a canvas point, is on an item of `items` whose page shows.
    pub(super) fn under(&self, items: &[Cow<'_, Item>], at: Point) -> bool {
        items.iter().any(|i| match &i.shape {
            Shape::Live { frame, open, .. } => frame.contains(at) && self.pages.contains_key(open),
            _ => false,
        })
    }
}

impl Canvases {
    /// Puts the page of each live item `scene`'s canvas panes draw over it,
    /// and asks for each page not yet asked for.
    pub fn show_live(&mut self, scene: &mut Scene) {
        let lives = &mut self.lives;
        for open in live::showings(&scene.canvases) {
            if lives.asked.insert(open.to_string()) {
                // The thread is gone only if it could not start.
                let _ = lives.ask.send(open.to_string());
            }
        }
        scene.webs.extend(live::webs(&scene.canvases, &lives.pages));
    }
}

#[cfg(test)]
mod tests;
