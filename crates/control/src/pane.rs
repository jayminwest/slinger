//! The pane verbs: their requests, replies and client methods
//! (docs/service.md, "Panes"). The server keeps each workspace's panes;
//! the app, the CLI and agents change them with these verbs alone.
//!
//! [`crate::Request`] reads them as one variant, [`PaneAsk`], and does not
//! write them: a client writes [`PaneAsk`] itself.

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// A pane request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PaneAsk {
    /// The panes of a workspace.
    PaneList { workspace: String },
    /// Open a pane that shows `target`, beside pane number `beside` (to
    /// its right, or below it with `below`), or beside the whole window.
    PaneOpen {
        workspace: String,
        kind: PaneKind,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        beside: Option<u32>,
        #[serde(default, skip_serializing_if = "is_false")]
        below: bool,
    },
    /// Close pane number `pane`. One that is already gone succeeds.
    PaneClose { workspace: String, pane: u32 },
}

/// What a pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneKind {
    /// A canvas, by its id.
    Canvas,
    /// An agent's UI, by its name.
    Ui,
}

/// How a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneSplit {
    /// Side by side: the first side on the left.
    Row,
    /// Stacked: the first side on top.
    Column,
}

/// One pane: its number (1 for the first, left to right and top to bottom)
/// and what it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    pub pane: u32,
    pub kind: PaneKind,
    pub target: String,
}

/// How the panes tile the window: a pane, or a split of the space in two,
/// `ratio` (0.1 to 0.9) of it to the first side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaneTree {
    Pane(Pane),
    Split { split: PaneSplit, ratio: f64, first: Box<PaneTree>, second: Box<PaneTree> },
}

impl PaneTree {
    /// Every pane, in order.
    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        match self {
            Self::Pane(pane) => vec![pane],
            Self::Split { first, second, .. } => {
                let mut panes = first.panes();
                panes.extend(second.panes());
                panes
            }
        }
    }
}

/// The reply to every pane verb: the workspace's panes after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneLayout {
    pub workspace: String,
    pub layout: PaneTree,
    /// The number of the pane a `pane_open` opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<u32>,
    /// The `pane_moved` event the change appended; absent when nothing
    /// changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<u64>,
}

impl Client {
    /// The panes of `workspace` (its name, or the absolute directory of its
    /// file).
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open, and `unavailable` when
    /// the daemon does not answer.
    pub fn pane_list(&self, workspace: &str) -> Result<PaneLayout, RemoteError> {
        self.call(&PaneAsk::PaneList { workspace: workspace.into() })
    }

    /// Opens a pane on `workspace` that shows the canvas or UI `target`,
    /// beside pane number `beside` (to its right, or below it with `below`),
    /// or beside the whole window.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open, a canvas or UI that is
    /// not there, or no pane `beside`; `invalid` when the window holds the
    /// most panes it can; and `unavailable` when the daemon does not answer.
    pub fn pane_open(
        &self,
        workspace: &str,
        kind: PaneKind,
        target: &str,
        beside: Option<u32>,
        below: bool,
    ) -> Result<PaneLayout, RemoteError> {
        let (workspace, target) = (workspace.into(), target.into());
        self.call(&PaneAsk::PaneOpen { workspace, kind, target, beside, below })
    }

    /// Closes pane number `pane` of `workspace`; the pane beside it takes
    /// its space. A pane that is already gone, or the last pane, which
    /// stays, changes nothing and succeeds.
    ///
    /// # Errors
    ///
    /// As [`Client::pane_list`].
    pub fn pane_close(&self, workspace: &str, pane: u32) -> Result<PaneLayout, RemoteError> {
        self.call(&PaneAsk::PaneClose { workspace: workspace.into(), pane })
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_pane_request_reads_back_as_itself_by_its_own_op() {
        let open = |beside, below| PaneAsk::PaneOpen {
            workspace: "w".into(),
            kind: PaneKind::Ui,
            target: "board".into(),
            beside,
            below,
        };
        for (ask, line) in [
            (
                PaneAsk::PaneList { workspace: "w".into() },
                r#"{"v":1,"op":"pane_list","workspace":"w"}"#,
            ),
            (
                open(None, false),
                r#"{"v":1,"op":"pane_open","workspace":"w","kind":"ui","target":"board"}"#,
            ),
            (
                open(Some(2), true),
                r#"{"v":1,"op":"pane_open","workspace":"w","kind":"ui","target":"board","beside":2,"below":true}"#,
            ),
            (
                PaneAsk::PaneClose { workspace: "w".into(), pane: 3 },
                r#"{"v":1,"op":"pane_close","workspace":"w","pane":3}"#,
            ),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Pane(ask)), "{line}");
        }
        for line in [
            r#"{"v":1,"op":"pane_open","workspace":"w","kind":"browser","target":"x"}"#,
            r#"{"v":1,"op":"pane_close","workspace":"w"}"#,
            r#"{"v":1,"op":"pane_list"}"#,
        ] {
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }

    #[test]
    fn a_layout_reads_back_as_a_tree_of_numbered_panes() {
        let text = r#"{"workspace":"w","layout":{"split":"row","ratio":0.6,
            "first":{"pane":1,"kind":"canvas","target":"main"},
            "second":{"pane":2,"kind":"ui","target":"board"}},"pane":2}"#;
        let reply: PaneLayout = serde_json::from_str(text).unwrap();
        let PaneTree::Split { split, ratio, .. } = &reply.layout else { panic!("{reply:?}") };
        assert_eq!((*split, *ratio, reply.pane, reply.event), (PaneSplit::Row, 0.6, Some(2), None));
        let panes: Vec<_> =
            reply.layout.panes().iter().map(|p| (p.pane, p.kind, p.target.as_str())).collect();
        assert_eq!(panes, [(1, PaneKind::Canvas, "main"), (2, PaneKind::Ui, "board")]);
        let again: PaneLayout =
            serde_json::from_value(serde_json::to_value(&reply).unwrap()).unwrap();
        assert_eq!(again, reply);
    }
}
