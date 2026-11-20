//! The pane verbs: their requests, replies and client methods
//! (docs/service.md, "Panes"). The server keeps each workspace's panes;
//! the app, the CLI and agents change them with these verbs alone.
//!
//! [`crate::Request`] carries them as one variant, [`PaneAsk`].

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError, Request};

/// A pane request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Move the border after pane number `pane` (on its right, or below
    /// it) so its split gives its first side `ratio`.
    PaneResize { workspace: String, pane: u32, ratio: f64 },
    /// Keep where the panes named by id look, and which pane has the focus.
    PaneView {
        workspace: String,
        views: Vec<PaneViewed>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focus: Option<u32>,
    },
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

/// One pane: its number (1 for the first, left to right and top to bottom),
/// its id and what it shows. The number moves as panes open and close and
/// names the pane in the verbs; the id stays while the pane is open and is
/// never given again in its workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub pane: u32,
    /// 1 or more. 0 in a reply from a daemon that has no pane ids.
    #[serde(default)]
    pub id: u32,
    pub kind: PaneKind,
    pub target: String,
    /// Where the pane was left looking, kept by `pane_view`: only on a
    /// canvas pane, and only when it is of the canvas the pane shows. None
    /// for a pane with no kept view (or from a daemon that keeps none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<PaneView>,
}

/// Where a canvas pane looks: the canvas and page it shows, the canvas
/// point at its top left, and its zoom (1 is 100%).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneView {
    pub canvas: String,
    /// The page; none for the canvas's first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

/// One view `pane_view` keeps: the pane's id, and its view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneViewed {
    pub id: u32,
    #[serde(flatten)]
    pub view: PaneView,
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
    /// The id of the pane that had the focus, as `pane_view` kept it; absent
    /// when none was kept or that pane is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<u32>,
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
        self.call(&Request::Pane(PaneAsk::PaneList { workspace: workspace.into() }))
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
        self.call(&Request::Pane(PaneAsk::PaneOpen { workspace, kind, target, beside, below }))
    }

    /// Closes pane number `pane` of `workspace`; the pane beside it takes
    /// its space. A pane that is already gone, or the last pane, which
    /// stays, changes nothing and succeeds.
    ///
    /// # Errors
    ///
    /// As [`Client::pane_list`].
    pub fn pane_close(&self, workspace: &str, pane: u32) -> Result<PaneLayout, RemoteError> {
        self.call(&Request::Pane(PaneAsk::PaneClose { workspace: workspace.into(), pane }))
    }

    /// Moves the border after pane number `pane` of `workspace` (on its
    /// right, or below it): the split whose first side ends with that pane
    /// gives its first side `ratio`, held in 0.1 to 0.9. A pane that is not
    /// there, the last pane (no border comes after it), or a share the
    /// split already has changes nothing and succeeds.
    ///
    /// # Errors
    ///
    /// As [`Client::pane_list`].
    pub fn pane_resize(
        &self,
        workspace: &str,
        pane: u32,
        ratio: f64,
    ) -> Result<PaneLayout, RemoteError> {
        self.call(&Request::Pane(PaneAsk::PaneResize { workspace: workspace.into(), pane, ratio }))
    }

    /// Keeps where the panes of `workspace` named in `views` look, each by
    /// its id, and, with `focus`, which pane has the focus; the panes'
    /// other views stay. A view for a pane that is not there, or that does
    /// not show the view's canvas, and a focus on a pane that is not there,
    /// are left out and the call succeeds. Writes neither the workspace's
    /// file nor the record. A daemon without the verb answers `bad_request`.
    ///
    /// # Errors
    ///
    /// `invalid` for a view whose numbers are not finite or whose zoom is 0
    /// or less (none is kept), and as [`Client::pane_list`].
    pub fn pane_view(
        &self,
        workspace: &str,
        views: &[PaneViewed],
        focus: Option<u32>,
    ) -> Result<PaneLayout, RemoteError> {
        let (workspace, views) = (workspace.into(), views.to_vec());
        self.call(&Request::Pane(PaneAsk::PaneView { workspace, views, focus }))
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
            (
                PaneAsk::PaneResize { workspace: "w".into(), pane: 1, ratio: 0.3 },
                r#"{"v":1,"op":"pane_resize","workspace":"w","pane":1,"ratio":0.3}"#,
            ),
            (
                PaneAsk::PaneView { workspace: "w".into(), views: vec![viewed()], focus: Some(3) },
                r#"{"v":1,"op":"pane_view","workspace":"w","views":[{"id":3,"canvas":"notes","page":"p2","x":-10.5,"y":4.0,"zoom":0.5}],"focus":3}"#,
            ),
            (
                PaneAsk::PaneView { workspace: "w".into(), views: vec![], focus: None },
                r#"{"v":1,"op":"pane_view","workspace":"w","views":[]}"#,
            ),
        ] {
            assert_eq!(request_line(&Request::Pane(ask.clone())).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Pane(ask)), "{line}");
        }
        for line in [
            r#"{"v":1,"op":"pane_open","workspace":"w","kind":"browser","target":"x"}"#,
            r#"{"v":1,"op":"pane_close","workspace":"w"}"#,
            r#"{"v":1,"op":"pane_list"}"#,
            r#"{"v":1,"op":"pane_resize","workspace":"w","pane":1}"#,
            r#"{"v":1,"op":"pane_view","workspace":"w"}"#,
            r#"{"v":1,"op":"pane_view","workspace":"w","views":[{"id":1,"canvas":"c","x":0,"y":0}]}"#,
            r#"{"v":1,"op":"pane_view","workspace":"w","views":[{"canvas":"c","x":0,"y":0,"zoom":1}]}"#,
        ] {
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }

    fn viewed() -> PaneViewed {
        let view = PaneView {
            canvas: "notes".into(),
            page: Some("p2".into()),
            x: -10.5,
            y: 4.0,
            zoom: 0.5,
        };
        PaneViewed { id: 3, view }
    }

    #[test]
    fn a_layout_carries_each_pane_s_kept_view_and_the_focus_and_reads_without_them() {
        let text = r#"{"workspace":"w","layout":{"split":"row","ratio":0.5,
            "first":{"pane":1,"id":3,"kind":"canvas","target":"notes",
                "view":{"canvas":"notes","page":"p2","x":-10.5,"y":4.0,"zoom":0.5}},
            "second":{"pane":2,"id":4,"kind":"canvas","target":"notes"}},"focus":3}"#;
        let reply: PaneLayout = serde_json::from_str(text).unwrap();
        let panes = reply.layout.panes();
        assert_eq!((panes[0].view.as_ref(), panes[1].view.as_ref()), (Some(&viewed().view), None));
        assert_eq!(reply.focus, Some(3));
        let line = serde_json::to_string(&reply).unwrap();
        assert!(line.ends_with(r#""target":"notes"}},"focus":3}"#), "{line}");
        assert!(
            line.contains(r#""target":"notes","view":{"canvas":"notes","page":"p2","#),
            "{line}"
        );

        let old = r#"{"workspace":"w","layout":{"pane":1,"id":1,"kind":"canvas","target":"c"}}"#;
        let old: PaneLayout = serde_json::from_str(old).unwrap();
        assert_eq!((old.focus, old.layout.panes()[0].view.as_ref()), (None, None));
        assert!(!serde_json::to_string(&old).unwrap().contains("focus"));
    }

    #[test]
    fn a_layout_reads_back_as_a_tree_of_numbered_panes() {
        let text = r#"{"workspace":"w","layout":{"split":"row","ratio":0.6,
            "first":{"pane":1,"id":4,"kind":"canvas","target":"main"},
            "second":{"pane":2,"id":2,"kind":"ui","target":"board"}},"pane":2}"#;
        let reply: PaneLayout = serde_json::from_str(text).unwrap();
        let PaneTree::Split { split, ratio, .. } = &reply.layout else { panic!("{reply:?}") };
        assert_eq!((*split, *ratio, reply.pane, reply.event), (PaneSplit::Row, 0.6, Some(2), None));
        let panes: Vec<_> = reply.layout.panes().iter().map(|p| (p.pane, p.id, p.kind)).collect();
        assert_eq!(panes, [(1, 4, PaneKind::Canvas), (2, 2, PaneKind::Ui)]);
        let again: PaneLayout =
            serde_json::from_value(serde_json::to_value(&reply).unwrap()).unwrap();
        assert_eq!(again, reply);
    }

    #[test]
    fn a_pane_from_a_daemon_with_no_ids_reads_as_id_0_and_writes_its_id() {
        let old: Pane =
            serde_json::from_str(r#"{"pane":1,"kind":"canvas","target":"main"}"#).unwrap();
        assert_eq!((old.pane, old.id), (1, 0));
        assert_eq!(old.view, None);
        let new = Pane { id: 7, ..old };
        assert_eq!(
            serde_json::to_string(&new).unwrap(),
            r#"{"pane":1,"id":7,"kind":"canvas","target":"main"}"#
        );
    }
}
