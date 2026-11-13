//! The window's panes (SPEC D12, P6): what each one shows, how they tile the
//! window (a [`Tree`]) and which one has focus. Pure. There is always one
//! pane at least, and always exactly one with focus.
//!
//! The server keeps a workspace's panes (docs/service.md, "Panes"); the
//! window follows them ([`Panes::follow`]) and names a pane to the server by
//! its number, 1 for the first in order. Focus is the window's own.

use slinger_control::{PaneKind, PaneSplit, PaneTree};

use crate::commands::Command;
use crate::layout::{Axis, Direction, Divider, PaneId, Point, Rect, Tree};

/// How much of its split [`Panes::grow`] gives a pane with each step.
pub const STEP: f64 = 0.05;

/// What a pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A canvas, by its id.
    Canvas,
    /// An agent's UI, by its name.
    Ui,
}

/// One pane's content: a kind and a name. A name has no white space in it,
/// as canvas ids and UI names never do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub kind: Kind,
    pub name: String,
}

/// The panes of one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Panes {
    tree: Tree,
    /// What each pane in the tree shows; one entry per pane.
    shows: Vec<(PaneId, Pane)>,
    focus: PaneId,
}

impl Panes {
    /// One pane, showing `pane`, with focus.
    pub fn new(pane: Pane) -> Self {
        let tree = Tree::default();
        let focus = tree.panes()[0];
        Self { tree, shows: vec![(focus, pane)], focus }
    }

    /// The panes the server keeps, with focus on pane number `focus` (the
    /// last pane past the last; the first for 0).
    pub fn follow(layout: &PaneTree, focus: u32) -> Self {
        let tree = Tree::default();
        let root = tree.panes()[0];
        let mut panes = Self { tree, shows: Vec::new(), focus: root };
        panes.build(root, layout);
        let order = panes.tree.panes();
        let at = (focus.max(1) as usize).min(order.len());
        panes.focus = order[at - 1];
        panes
    }

    /// Makes pane `at` show `node`, splitting it for a split. A full tree
    /// shows the first side of a split it cannot make.
    fn build(&mut self, at: PaneId, node: &PaneTree) {
        match node {
            PaneTree::Pane(pane) => {
                let kind = match pane.kind {
                    PaneKind::Canvas => Kind::Canvas,
                    PaneKind::Ui => Kind::Ui,
                };
                self.shows.push((at, Pane { kind, name: pane.target.clone() }));
            }
            PaneTree::Split { split, ratio, first, second } => {
                let axis = match split {
                    PaneSplit::Row => Axis::Row,
                    PaneSplit::Column => Axis::Column,
                };
                if let Some(new) = self.tree.split_by(at, axis, *ratio) {
                    self.build(new, second);
                }
                self.build(at, first);
            }
        }
    }

    /// The pane with focus.
    pub fn focus(&self) -> PaneId {
        self.focus
    }

    /// The focused pane's number: 1 for the first in order.
    pub fn number(&self) -> u32 {
        self.number_of(self.focus)
    }

    /// Pane `id`'s number: 1 for the first in order (the first's for one
    /// not in the tree).
    pub fn number_of(&self, id: PaneId) -> u32 {
        let at = self.tree.panes().iter().position(|&p| p == id).unwrap_or(0);
        u32::try_from(at + 1).unwrap_or(u32::MAX)
    }

    /// What the focused pane shows.
    pub fn focused(&self) -> &Pane {
        self.shows(self.focus)
    }

    /// Each pane, in order, what it shows and where it goes in `area`.
    pub fn place(&self, area: Rect) -> Vec<(PaneId, &Pane, Rect)> {
        self.tree.rects(area).into_iter().map(|(id, rect)| (id, self.shows(id), rect)).collect()
    }

    /// What pane `id` shows.
    pub fn shows(&self, id: PaneId) -> &Pane {
        let found = self.shows.iter().find(|(p, _)| *p == id);
        // The tree and `shows` change together, so every pane has an entry.
        found.map_or(&self.shows[0].1, |(_, pane)| pane)
    }

    /// Splits the focused pane along `axis`. The new pane shows the same and
    /// takes focus. A full tree does not split.
    pub fn split(&mut self, axis: Axis) {
        if let Some(new) = self.tree.split(self.focus, axis) {
            let pane = self.shows(self.focus).clone();
            self.shows.push((new, pane));
            self.focus = new;
        }
    }

    /// Closes the focused pane; focus goes to the pane that was before it in
    /// order (the first pane's to the next). The last pane stays.
    pub fn close(&mut self) {
        let panes = self.tree.panes();
        let at = panes.iter().position(|&p| p == self.focus).unwrap_or(0);
        let heir = if at == 0 { panes.get(1) } else { panes.get(at - 1) };
        if let Some(&heir) = heir
            && self.tree.close(self.focus)
        {
            self.shows.retain(|(p, _)| *p != self.focus);
            self.focus = heir;
        }
    }

    /// Moves focus to the pane under `at` (what it shows or its footer)
    /// when the panes fill `area`. Away from every pane, or on a divider
    /// between two (the pen grabs it), focus stays. Returns whether it moved.
    pub fn focus_at(&mut self, area: Rect, at: Point) -> bool {
        self.divider_at(area, at).is_none() && self.focus_in(area, at)
    }

    /// Moves focus to the pane under `at`, as [`Panes::focus_at`], even
    /// within reach of a divider: for a press that grabs something in the
    /// pane (the selection) rather than the divider.
    pub fn focus_in(&mut self, area: Rect, at: Point) -> bool {
        let Some((id, _)) = self.tree.rects(area).into_iter().find(|(_, f)| f.contains(at)) else {
            return false;
        };
        std::mem::replace(&mut self.focus, id) != id
    }

    /// Moves focus to the next pane in order, back to the first after the last.
    pub fn focus_next(&mut self) {
        self.focus = self.tree.next(self.focus);
    }

    /// Moves focus to the pane beside it in `direction` when the panes fill
    /// `area`. At the edge, focus stays.
    pub fn focus_toward(&mut self, direction: Direction, area: Rect) {
        if let Some(id) = self.tree.toward(self.focus, direction, area) {
            self.focus = id;
        }
    }

    /// Gives the focused pane `steps` [`STEP`]s more of its split (fewer
    /// for a negative count).
    pub fn grow(&mut self, steps: f64) {
        self.tree.grow(self.focus, steps * STEP);
    }

    /// The divider [`Panes::grow`] would move, as the server names it: the
    /// number of the pane before it, and its split's new share. `None`
    /// when the grow changes nothing.
    pub fn grown(&self, steps: f64) -> Option<(u32, f64)> {
        let (before, ratio) = self.tree.grown(self.focus, steps * STEP)?;
        Some((self.number_of(before), ratio))
    }

    /// The divider between two panes under `at` when the panes fill `area`.
    pub fn divider_at(&self, area: Rect, at: Point) -> Option<Divider> {
        self.tree.divider_at(area, at)
    }

    /// Gives the split of the divider after pane `before` the share `ratio`
    /// (see [`Tree::set_ratio`]); whether there is such a divider.
    pub fn set_ratio(&mut self, before: PaneId, ratio: f64) -> bool {
        self.tree.set_ratio(before, ratio)
    }

    /// Runs a pane command on the panes laid in `area`. Commands that are
    /// not about panes do nothing.
    pub fn command(&mut self, command: Command, area: Rect) {
        match command {
            Command::Quit
            | Command::ReloadConfig
            | Command::PickColour
            | Command::PickShape
            | Command::PickTool
            | Command::PickWidth
            | Command::WidthUp
            | Command::WidthDown
            | Command::ToggleSidebar
            | Command::DismissNotice
            | Command::SlingNow
            | Command::PanLeft
            | Command::PanRight
            | Command::PanUp
            | Command::PanDown
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::NextPage
            | Command::DeleteSelection
            | Command::ClearSelection
            | Command::ToolPen
            | Command::ToolEraser
            | Command::ToolLasso
            | Command::ToolText
            | Command::ToolBox
            | Command::ToolArrow
            | Command::ToolSelect
            | Command::CropPdf
            | Command::Sling
            | Command::PreviousPage
            | Command::NewPage
            | Command::RenamePage
            | Command::RemovePage
            | Command::Undo
            | Command::Redo
            | Command::Dictate
            | Command::ShowKeys
            | Command::ShowCommands
            | Command::LeaveFeedback
            | Command::ToggleGrid
            | Command::Save
            | Command::ShowCanvasFile
            | Command::OpenPdf
            | Command::OpenWorkspace
            | Command::CloseWorkspace
            | Command::ShowWorkspaceFolder
            | Command::StartAgent
            | Command::StopAgent
            | Command::RestartAgent => {}
            Command::SplitRight => self.split(Axis::Row),
            Command::SplitDown => self.split(Axis::Column),
            Command::ClosePane => self.close(),
            Command::FocusNext => self.focus_next(),
            Command::FocusLeft => self.focus_toward(Direction::Left, area),
            Command::FocusRight => self.focus_toward(Direction::Right, area),
            Command::FocusUp => self.focus_toward(Direction::Up, area),
            Command::FocusDown => self.focus_toward(Direction::Down, area),
            Command::GrowPane => self.grow(1.0),
            Command::ShrinkPane => self.grow(-1.0),
        }
    }
}

#[cfg(test)]
mod tests;
