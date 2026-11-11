//! `pane_list`, `pane_open` and `pane_close`: a workspace's panes as they
//! cross the socket. The core changes the layout; each change appends its
//! `pane_moved` to the record, tagged with the workspace.

use slinger_control::{self as control, PaneAsk, PaneKind, RemoteError, Reply};
use slinger_core::record::{Body, Event, PaneMoved, Tags};
use slinger_core::workspace::panes::{Axis, Kind, Layout, Node, Pane};

use super::{catalog, drawn, kept, remote, stored};
use crate::store::Store;
use crate::workspaces::Opened;

pub fn answer(store: &Store, ask: PaneAsk) -> Result<Reply, RemoteError> {
    let workspaces = store.workspaces();
    match ask {
        PaneAsk::PaneList { workspace } => {
            let (opened, layout, ()) =
                workspaces.panes(&workspace, |_| Ok(())).map_err(|e| kept(&e))?;
            Ok(reply(&opened, &layout, None, None))
        }
        PaneAsk::PaneOpen { workspace, kind, target, beside, below } => {
            let pane = shown(store, kind, &target)?;
            let (opened, layout, number, event) = open(store, &workspace, pane, beside, below)?;
            Ok(reply(&opened, &layout, Some(number), Some(event)))
        }
        PaneAsk::PaneClose { workspace, pane } => {
            let (opened, layout, moved) = workspaces
                .panes(&workspace, |layout| Ok(layout.close(pane as usize)))
                .map_err(|e| kept(&e))?;
            let event = moved.map(|moved| record(store, &opened, moved)).transpose()?;
            Ok(reply(&opened, &layout, None, event))
        }
    }
}

/// Opens `pane` in `workspace`, beside pane number `beside` (to its right,
/// or below it with `below`) or beside the whole window, and appends its
/// `pane_moved`. Returns the workspace, its layout now, the new pane's number
/// and the event's.
pub fn open(
    store: &Store,
    workspace: &str,
    pane: Pane,
    beside: Option<u32>,
    below: bool,
) -> Result<(Opened, Layout, u32, u64), RemoteError> {
    let axis = if below { Axis::Column } else { Axis::Row };
    let beside = beside.map(|n| n as usize);
    let (opened, layout, (number, moved)) = store
        .workspaces()
        .panes(workspace, |layout| layout.open(pane, beside, axis))
        .map_err(|e| kept(&e))?;
    let event = record(store, &opened, moved)?;
    // A layout holds at most 64 panes.
    Ok((opened, layout, u32::try_from(number).unwrap_or(u32::MAX), event))
}

/// The pane for `target`, which must be there: a canvas, or a UI with a
/// version.
fn shown(store: &Store, kind: PaneKind, target: &str) -> Result<Pane, RemoteError> {
    let kind = match kind {
        PaneKind::Canvas => {
            store.canvases().show(target, None).map_err(|e| drawn(&e))?;
            Kind::Canvas
        }
        PaneKind::Ui => {
            catalog(store)?.versions(target).map_err(|e| remote(&e))?;
            Kind::Ui
        }
    };
    Pane::new(kind, target).map_err(|e| remote(&e))
}

/// Appends a change's `pane_moved`, tagged with the workspace; its number.
fn record(store: &Store, opened: &Opened, moved: PaneMoved) -> Result<u64, RemoteError> {
    let tags = Tags { workspace: Some(opened.workspace.tag.clone()), ..Tags::on("") };
    let event = Event::new(0, "slingerd", tags, Body::PaneMoved(moved));
    Ok(store.append(event).map_err(|e| stored(&e))?.number.unwrap_or_default())
}

fn reply(opened: &Opened, layout: &Layout, pane: Option<u32>, event: Option<u64>) -> Reply {
    let mut number = 0;
    let layout = wire(layout.root(), &mut number);
    Reply::Pane(control::PaneLayout {
        workspace: opened.workspace.name.clone(),
        layout,
        pane,
        event,
    })
}

/// The tree as it crosses the socket, each pane with its number.
fn wire(node: &Node, number: &mut u32) -> control::PaneTree {
    match node {
        Node::Pane(pane) => {
            *number += 1;
            let kind = match pane.kind {
                Kind::Canvas => PaneKind::Canvas,
                Kind::Ui => PaneKind::Ui,
            };
            control::PaneTree::Pane(control::Pane {
                pane: *number,
                kind,
                target: pane.target.clone(),
            })
        }
        Node::Split { axis, ratio, first, second } => control::PaneTree::Split {
            split: match axis {
                Axis::Row => control::PaneSplit::Row,
                Axis::Column => control::PaneSplit::Column,
            },
            ratio: *ratio,
            first: Box::new(wire(first, number)),
            second: Box::new(wire(second, number)),
        },
    }
}
