//! The window's tap targets: the pen-first buttons in the footers and the
//! sidebar (SPEC P6: the pen reaches every common action with no
//! keyboard). Each is a frame and what a tap there does ([`Tap`]), most
//! often a registry command, run as its key would run it. Pure: the scene
//! lays the targets out each time it is drawn and hands them in
//! ([`Taps::set`]); `main` hands in the pen.
//!
//! A tap runs when the pen lifts on the target it went down on, not on the
//! press: a drag off it cancels. A press on a target is the target's
//! alone (it never draws ink or starts a tool's gesture), and so is the
//! drag and the lift after it. The target under the pen lights up
//! ([`style::hover`]), and more while pressed ([`style::selected`]), as a
//! key overlay row does.

use crate::commands::Command;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::palette::Taken;
use crate::scene::style;

/// What a tap on a target does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    /// Runs a command on the focused pane, as its key does.
    Run(Command),
    /// Opens the focused pane's page menu, over the target.
    Pages,
    /// Turns the focused pane to its page with this index.
    Page(usize),
    /// Opens the focused pane's profile menu, over the target: its
    /// agent's profiles to sling under.
    Profiles,
    /// Slings under the profile menu's row with this index.
    Profile(usize),
    /// Shows the panes of the sidebar's workspace with this index.
    Workspace(usize),
    /// The pill that says what became of the last sling, or the answer
    /// under it ([`crate::scene::slung`]).
    Slung,
    /// The open entry's "✓" button: as return in it.
    Send,
    /// The open entry's "✕" button: as escape in it.
    Cancel,
}

/// A target: where it takes a tap, and what the tap does.
pub type Target = (Rect, Tap);

/// Where a command chosen in a pop-up was tapped: nowhere in particular.
const NOWHERE: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

/// What a press or a lift was to the targets (or to what is over them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Took {
    /// Not theirs: it goes on.
    No,
    /// Used up.
    Yes,
    /// A tap on this target.
    Tap(Target),
}

impl From<Taken> for Took {
    /// A pop-up's answer as a target's: a command chosen there runs as a
    /// tap on no particular place.
    fn from(taken: Taken) -> Self {
        match taken {
            Taken::No => Took::No,
            Taken::Yes => Took::Yes,
            Taken::Run(command) => Took::Tap((NOWHERE, Tap::Run(command))),
        }
    }
}

/// The targets as last drawn, where the pen is, and the target it went
/// down on, if any.
#[derive(Debug, Default)]
pub struct Taps {
    targets: Vec<Target>,
    pointer: Option<Point>,
    held: Option<Target>,
}

impl Taps {
    /// The targets as now drawn, in the window's points.
    pub fn set(&mut self, targets: Vec<Target>) {
        self.targets = targets;
    }

    /// The target under `at`, if any.
    fn at(&self, at: Point) -> Option<Target> {
        self.targets.iter().copied().find(|(frame, _)| frame.contains(at))
    }

    /// The pen or the pointer, up, moved to `at`. True when that changed
    /// which target lights up.
    pub fn hover(&mut self, at: Point) -> bool {
        let before = self.pointer.and_then(|p| self.at(p));
        self.pointer = Some(at);
        before != self.at(at)
    }

    /// The pen went down at `at`: on a target, it holds the target until
    /// it lifts (true); elsewhere, the press is not theirs (false).
    pub fn press(&mut self, at: Point) -> bool {
        self.pointer = Some(at);
        self.held = self.at(at);
        self.held.is_some()
    }

    /// The pen moved to `at` while down. True while it holds a target:
    /// the drag is the target's.
    pub fn drag(&mut self, at: Point) -> bool {
        if self.held.is_some() {
            self.pointer = Some(at);
        }
        self.held.is_some()
    }

    /// The pen lifted at `at`: on the target it went down on, that is a
    /// tap ([`Took::Tap`]); anywhere else after a press on one, it is
    /// used up and nothing runs ([`Took::Yes`]). With no target held,
    /// [`Took::No`].
    pub fn lift(&mut self, at: Point) -> Took {
        let Some(held) = self.held.take() else { return Took::No };
        self.pointer = Some(at);
        if self.at(at) == Some(held) { Took::Tap(held) } else { Took::Yes }
    }

    /// The target lit, and its fill in `theme`: the one held while the pen
    /// is over it, else the one under the pen. None while the pen holds a
    /// target and is off it.
    pub fn lit(&self, theme: &Theme) -> Option<(Rect, Rgb)> {
        let under = self.pointer.and_then(|p| self.at(p));
        match self.held {
            Some(held) => (under == Some(held)).then(|| (held.0, style::selected(theme))),
            None => under.map(|(frame, _)| (frame, style::hover(theme))),
        }
    }
}

#[cfg(test)]
mod tests;
