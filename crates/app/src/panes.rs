//! The window's panes (SPEC D12, P6): what each one shows, how they tile the
//! window (a [`Tree`]), which one has focus, and the text a workspace keeps
//! of them. Pure. There is always one pane at least, and always exactly one
//! with focus.
//!
//! The layout field of a workspace (docs/workspace.md) holds
//! `version = `[`VERSION`] and `panes = "<text>"`: the tree's text
//! ([`Tree::encode`]) with each pane written `canvas:<name>` or `ui:<name>`.
//! Focus is not kept; a layout opens with its first pane focused.

use crate::layout::{Axis, Direction, PaneId, Rect, Tree};

/// The version of the layout text this app writes and reads.
#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
pub const VERSION: u32 = 1;

/// How much of its split [`Panes::grow`] gives a pane with each step.
pub const STEP: f64 = 0.05;

/// What a pane shows.
#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
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

#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
impl Pane {
    /// The word for this pane in the layout text: `canvas:main`.
    fn word(&self) -> String {
        let kind = match self.kind {
            Kind::Canvas => "canvas",
            Kind::Ui => "ui",
        };
        format!("{kind}:{}", self.name)
    }

    /// The pane a word names; `None` for an unknown kind or an empty name.
    fn from_word(word: &str) -> Option<Self> {
        let (kind, name) = word.split_once(':')?;
        let kind = match kind {
            "canvas" => Kind::Canvas,
            "ui" => Kind::Ui,
            _ => return None,
        };
        (!name.is_empty()).then(|| Pane { kind, name: name.to_string() })
    }
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

    /// The pane with focus.
    pub fn focus(&self) -> PaneId {
        self.focus
    }

    /// Each pane, in order, what it shows and where it goes in `area`.
    pub fn place(&self, area: Rect) -> Vec<(PaneId, &Pane, Rect)> {
        self.tree.rects(area).into_iter().map(|(id, rect)| (id, self.shows(id), rect)).collect()
    }

    fn shows(&self, id: PaneId) -> &Pane {
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
}

/// The text a workspace keeps.
#[cfg_attr(not(test), expect(dead_code, reason = "used from slinger-2241"))]
impl Panes {
    /// The layout text (see the module's comment).
    pub fn encode(&self) -> String {
        self.tree.encode(|id| self.shows(id).word())
    }

    /// The panes a layout field holds; `None` for a version this app does
    /// not read, or text that is not a layout. The caller then picks one.
    pub fn decode(version: u32, text: &str) -> Option<Self> {
        if version != VERSION {
            return None;
        }
        let mut shows = Vec::new();
        let tree = Tree::decode(text, |id, word| {
            Pane::from_word(word).map(|pane| shows.push((id, pane))).is_some()
        })?;
        let focus = tree.panes()[0];
        Some(Self { tree, shows, focus })
    }
}

#[cfg(test)]
mod tests;
