//! The window's panes as the server keeps them for its workspace, and what
//! goes with them: the window's size, the agent UIs, the notes, the
//! divider under the pen, the footers' and the sidebar's tap targets, the
//! key overlay, the command palette and the page menu over them, and the
//! hand that space holds for a pan, and the right or middle press under
//! way.

use std::path::Path;
use std::sync::Arc;

use slinger_control::{self as control, PaneLayout};

use crate::backend::{self, Problem};
use crate::commands::Command;
use crate::layout::{self, Point, Rect, Size};
use crate::panes::{self, Pane, Panes};
use crate::scene::hand::Hand;
use crate::scene::overlay::KeyOverlay;
use crate::scene::pages::menu::PageMenu;
use crate::scene::palette::Palette;
use crate::scene::paste::Keyboard;
use crate::scene::resize::Dividers;
use crate::scene::secondary::Secondary;
use crate::scene::taps::{Taps, Took};
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
    /// The command palette (`show-commands`), when it is open.
    pub palette: Palette,
    /// The page menu a tap on a footer's page name opens.
    pub menu: PageMenu,
    /// The footers' and the sidebar's buttons, as last drawn.
    pub taps: Taps,
    /// Where the last press left the keyboard ([`crate::route`]).
    pub keyboard: Keyboard,
    /// Space held for a pan, and the pan under way ([`crate::route`]).
    pub hand: Hand,
    /// The right or middle press under way: a pan or a context click
    /// ([`crate::route`]).
    pub secondary: Secondary,
    /// The pane each workspace left had focus on, in this run.
    pub focused: Vec<(String, layout::PaneId)>,
}

impl Default for Shared {
    fn default() -> Self {
        let main = Pane { kind: panes::Kind::Canvas, name: "main".to_string() };
        let (uis, notes) = (Uis::new(), Notes::new());
        let (size, workspace, trouble, dividers) =
            (layout::WINDOW, None, None, Dividers::default());
        let (overlay, palette) = (KeyOverlay::default(), Palette::default());
        let panes = Panes::new(main);
        let (menu, taps, keyboard) = (PageMenu::default(), Taps::default(), Keyboard::Window);
        Self {
            size,
            panes,
            uis,
            workspace,
            trouble,
            notes,
            dividers,
            overlay,
            palette,
            menu,
            taps,
            keyboard,
            hand: Hand::default(),
            secondary: Secondary::default(),
            focused: Vec::new(),
        }
    }
}

impl Shared {
    /// Takes the server's newest layout, or the problem asking for it: the
    /// panes follow it. In the same workspace focus stays on the same pane,
    /// or, when it closed, goes to the pane now at its number; in another,
    /// it goes to the pane that had it there last in this run, else the one
    /// the daemon kept it on (`focus`), else the first, and the page menu
    /// closes.
    pub fn follow(&mut self, news: Result<PaneLayout, Problem>) {
        let layout = match news {
            Ok(layout) => layout,
            Err(problem) => return (self.workspace, self.trouble) = (None, Some(problem)),
        };
        let name = layout.workspace.as_str();
        let left = self.panes.workspace().filter(|w| *w != name).map(str::to_string);
        let (number, focus) = (self.panes.number(), self.panes.focus());
        let first =
            self.panes.workspace().is_none() && !self.focused.iter().any(|(w, _)| w == name);
        let mut panes = if let Some(left) = left {
            self.focused.retain(|(w, _)| *w != left);
            self.focused.push((left, focus));
            self.menu = PageMenu::default();
            let mut panes = Panes::follow(&layout.layout, 1);
            let was = self.focused.iter().find(|(w, _)| w == name).map(|&(_, was)| was);
            if let Some(was) = was.or(layout.focus.map(layout::PaneId::numbered)) {
                panes.focus_on(was);
            }
            panes
        } else if let Some(kept) = layout.focus.filter(|_| first) {
            // The first layout of this run: focus where the daemon kept it.
            let mut panes = Panes::follow(&layout.layout, number);
            panes.focus_on(layout::PaneId::numbered(kept));
            panes
        } else {
            Panes::follow(&layout.layout, number)
        };
        if self.panes.workspace() == Some(name) {
            panes.focus_on(focus);
        }
        self.panes = panes.of(name);
        (self.workspace, self.trouble) = (Some(layout.workspace), None);
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

    /// A press at `at`, a point in the window: the open palette's, else
    /// the open key overlay's, else the open page menu's (each takes every
    /// press, and a press on a row holds it until the lift); with none open,
    /// a press on a button holds it until the lift ([`Shared::lift`]), and
    /// goes no further.
    pub fn press(&mut self, at: Point) -> Took {
        let size = self.size;
        let taken = Took::from(self.palette.press(at, size));
        let taken = if taken == Took::No { self.overlay.press(at, size).into() } else { taken };
        let taken = if taken == Took::No { self.menu.press(at, size) } else { taken };
        match taken {
            Took::No if self.taps.press(at) => Took::Yes,
            taken => taken,
        }
    }

    /// A command chosen from the menu bar with the pointer: the open
    /// palette, else the open key overlay, else the open page menu closes
    /// first, and then it runs ([`Took::Tap`]), except the `show-commands`
    /// or `show-keys` that only closed its own ([`Took::Yes`]). With none
    /// open, [`Took::No`]: it runs as its key would.
    pub fn chosen(&mut self, command: Command) -> Took {
        let taken = Took::from(self.palette.chosen(command));
        let taken = if taken == Took::No { self.overlay.chosen(command).into() } else { taken };
        if taken == Took::No { self.menu.chosen(command) } else { taken }
    }

    /// A pen or mouse press at `at`, a point in the window, with the panes
    /// in `area`: the pane under it (what it shows or its footer) takes
    /// focus before the press does its work, so every focused-pane command
    /// acts there next. Under the key overlay or the palette (they take
    /// the press), on a divider, or for a touch, focus stays; a press that
    /// `grabs` the selection there focuses its pane even on a divider.
    /// Returns whether it moved.
    pub fn press_focus(&mut self, area: Rect, at: Point, touch: bool, grabs: bool) -> bool {
        if touch || self.modal() {
            return false;
        }
        if grabs { self.panes.focus_in(area, at) } else { self.panes.focus_at(area, at) }
    }

    /// Whether the key overlay or the palette is open: either takes every
    /// pen, mouse and trackpad event.
    pub fn modal(&self) -> bool {
        self.overlay.is_open() || self.palette.is_open() || self.menu.is_open()
    }

    /// The pen or the pointer, up, moved to `at`: over the open key
    /// overlay or page menu, the row under it lights up; else the button
    /// under it. True when that changed.
    pub fn hover(&mut self, at: Point) -> bool {
        if self.overlay.is_open() {
            return self.overlay.hover(at, self.size);
        }
        self.menu.pointer(at, self.size, false) || (!self.modal() && self.taps.hover(at))
    }

    /// The pen, down, moved to `at`: the open key overlay, or the open page
    /// menu's pages, scroll with it.
    /// True while the overlay, the palette or the page menu is open, or the
    /// pen holds a button: they take it.
    pub fn drag(&mut self, at: Point) -> bool {
        self.overlay.drag(at, self.size)
            || self.palette.is_open()
            || self.menu.pointer(at, self.size, true)
            || self.taps.drag(at)
    }

    /// A scroll by `dy` points: the open key overlay's rows, or the open
    /// page menu's pages, follow it. True while the overlay, the palette or
    /// the page menu is open: they take it.
    pub fn scroll(&mut self, dy: f64) -> bool {
        self.overlay.scroll(dy, self.size)
            || self.palette.is_open()
            || self.menu.scroll(dy, self.size)
    }

    /// The pen lifted at `at`: a tap on the palette's, the key overlay's or
    /// the page menu's row runs it, and so does one on the button the pen
    /// went down on.
    pub fn lift(&mut self, at: Point) -> Took {
        let taken = Took::from(self.palette.lift(at, self.size));
        let taken = if taken == Took::No { self.overlay.lift(at, self.size).into() } else { taken };
        let taken = if taken == Took::No { self.menu.lift(at, self.size) } else { taken };
        if taken == Took::No { self.taps.lift(at) } else { taken }
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
    /// the key overlay, `show-commands` the command palette. Other commands
    /// that are not about panes do nothing.
    pub fn command(&mut self, command: Command, area: Rect, socket: &Path) {
        if command == Command::ShowKeys {
            return self.overlay.toggle();
        }
        if command == Command::ShowCommands {
            return self.palette.toggle();
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
                    .map(|(layout, number)| Panes::follow(&layout, number).of(workspace))
            }
            (Command::ClosePane, Some((daemon, workspace))) => {
                // Focus goes to the pane before it; the first's to the next.
                let number = panes.number();
                backend::panes::close(&daemon, workspace, number)
                    .map(|layout| Panes::follow(&layout, number.saturating_sub(1)).of(workspace))
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
            Ok(layout) => {
                let mut panes = Panes::follow(&layout, self.panes.number());
                panes.focus_on(self.panes.focus());
                self.panes = panes.of(workspace);
            }
            Err(problem) => self.trouble = Some(problem),
        }
    }
}

#[cfg(test)]
mod tests;
