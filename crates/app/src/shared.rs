//! The window's panes as the server keeps them for its workspace, and what
//! goes with them: the window's size, the agent UIs, the notes, the
//! divider under the pen and the key overlay over them.

use std::path::Path;
use std::sync::Arc;

use slinger_control::{self as control, PaneLayout};

use crate::backend::{self, Problem};
use crate::commands::Command;
use crate::layout::{self, Rect, Size};
use crate::panes::{self, Pane, Panes};
use crate::scene::overlay::KeyOverlay;
use crate::scene::resize::Dividers;
use crate::scene::{Notes, Uis};
use crate::ui;

/// The window's panes as the server keeps them for its workspace: one pane
/// until the server's layout comes, or without a daemon.
#[derive(Debug)]
pub struct Shared {
    /// The window's content size: the panes share what the sidebar leaves.
    pub size: Size,
    pub panes: Panes,
    /// The agent UIs the panes show, as last fetched.
    pub uis: Uis,
    /// The workspace the panes are the server's for; `None` while the
    /// server cannot say, when the window changes its own panes.
    pub workspace: Option<String>,
    /// The problem asking the server, if any.
    pub trouble: Option<Problem>,
    /// Why an agent UI's last reply did not reach its agent, by showing.
    pub notes: Notes,
    /// The divider between panes under the pen, if any.
    pub dividers: Dividers,
    /// The key overlay (`show-keys`), when it is open.
    pub overlay: KeyOverlay,
}

impl Default for Shared {
    fn default() -> Self {
        let main = Pane { kind: panes::Kind::Canvas, name: "main".to_string() };
        let (uis, notes) = (Uis::new(), Notes::new());
        let (size, workspace, trouble, dividers) =
            (layout::WINDOW, None, None, Dividers::default());
        let overlay = KeyOverlay::default();
        Self { size, panes: Panes::new(main), uis, workspace, trouble, notes, dividers, overlay }
    }
}

impl Shared {
    /// Takes the server's newest layout, or the problem asking for it: the
    /// panes follow it, focus kept on the same number.
    pub fn follow(&mut self, news: Result<PaneLayout, Problem>) {
        match news {
            Ok(layout) => {
                self.panes = Panes::follow(&layout.layout, self.panes.number());
                (self.workspace, self.trouble) = (Some(layout.workspace), None);
            }
            Err(problem) => (self.workspace, self.trouble) = (None, Some(problem)),
        }
    }

    /// Takes the agent UIs' code the watch fetched, each as a page for a
    /// pane or why it cannot show; whether there was any.
    pub fn fetched(&mut self, codes: Vec<(String, Result<ui::Shown, Problem>)>) -> bool {
        let any = !codes.is_empty();
        for (name, shown) in codes {
            let page = shown.and_then(|shown| {
                let page = ui::Page::new(shown, "pane");
                page.map(Arc::new).map_err(|why| Problem { message: why, hint: None })
            });
            self.uis.insert(name, page);
        }
        any
    }

    /// Sends an agent UI's reply `line` to the daemon on `socket`, which
    /// records it and hands it to the agent that opened the showing. The
    /// showing's pane notes an agent that did not take it, until one does;
    /// whether that changed what the window shows, or why the reply cannot
    /// go on, for the page's promise.
    pub fn reply(&mut self, line: &str, socket: &Path) -> Result<bool, String> {
        Ok(match backend::ui::reply(&control::Client::new(socket), line)? {
            (open, Some(unreachable)) => {
                self.notes.insert(open, unreachable);
                true
            }
            (open, None) => self.notes.remove(&open).is_some(),
        })
    }

    /// Runs a pane command on the panes in `area`. Opening, closing and
    /// resizing go to the daemon on `socket`, which keeps the layout (to the
    /// window's own panes without a workspace). `show-keys` opens or closes
    /// the key overlay. Other commands that are not about panes do nothing.
    pub fn command(&mut self, command: Command, area: Rect, socket: &Path) {
        if command == Command::ShowKeys {
            return self.overlay.toggle();
        }
        if matches!(command, Command::GrowPane | Command::ShrinkPane) && self.workspace.is_some() {
            let steps = if command == Command::GrowPane { 1.0 } else { -1.0 };
            return self.resized(self.panes.grown(steps), socket);
        }
        let panes = &mut self.panes;
        let server = self.workspace.as_deref().map(|w| (control::Client::new(socket), w));
        let asked = match (command, server) {
            (Command::SplitRight | Command::SplitDown, Some((daemon, workspace))) => {
                let below = command == Command::SplitDown;
                backend::panes::open(&daemon, workspace, panes.focused(), panes.number(), below)
                    .map(|(layout, number)| Panes::follow(&layout, number))
            }
            (Command::ClosePane, Some((daemon, workspace))) => {
                // Focus goes to the pane before it; the first's to the next.
                let number = panes.number();
                backend::panes::close(&daemon, workspace, number)
                    .map(|layout| Panes::follow(&layout, number.saturating_sub(1)))
            }
            _ => return panes.command(command, area),
        };
        match asked {
            Ok(now) => *panes = now,
            Err(problem) => self.trouble = Some(problem),
        }
    }

    /// Sends a moved divider (the number of the pane before it, and its
    /// split's share) to the daemon on `socket`, which keeps the layout.
    /// Without a workspace the window's own panes are the layout.
    pub fn resized(&mut self, moved: Option<(u32, f64)>, socket: &Path) {
        let (Some((number, ratio)), Some(workspace)) = (moved, self.workspace.as_deref()) else {
            return;
        };
        let daemon = control::Client::new(socket);
        match backend::panes::resize(&daemon, workspace, number, ratio) {
            Ok(layout) => self.panes = Panes::follow(&layout, self.panes.number()),
            Err(problem) => self.trouble = Some(problem),
        }
    }
}
