//! `pane_list`, `pane_open`, `pane_close`, `pane_resize` and `pane_view`: a
//! workspace's panes as they cross the socket. The core changes the layout;
//! each change appends its `pane_moved` to the record, tagged with the
//! workspace. Every reply carries each pane's kept view and the focus
//! ([`crate::views`]); a change drops the views of the panes it closed.

use slinger_control::{self as control, PaneAsk, PaneKind, RemoteError, Reply};
use slinger_core::record::{Body, Event, PaneMoved, Tags};
use slinger_core::workspace::panes::{Axis, Kind, Layout, Node, Pane};
use slinger_core::workspace::view::{self, Views};

use super::{catalog, drawn, kept, remote, stored};
use crate::store::Store;
use crate::workspaces::Opened;

pub fn answer(store: &Store, ask: PaneAsk) -> Result<Reply, RemoteError> {
    let workspaces = store.workspaces();
    match ask {
        PaneAsk::PaneList { workspace } => {
            let (opened, layout, ()) =
                workspaces.panes(&workspace, |_| Ok(())).map_err(|e| kept(&e))?;
            Ok(reply(store, &opened, &layout, None, None))
        }
        PaneAsk::PaneOpen { workspace, kind, target, beside, below } => {
            let pane = shown(store, kind, &target)?;
            let (opened, layout, number, event) = open(store, &workspace, pane, beside, below)?;
            Ok(reply(store, &opened, &layout, Some(number), Some(event)))
        }
        PaneAsk::PaneClose { workspace, pane } => {
            let (opened, layout, moved) = workspaces
                .panes(&workspace, |layout| Ok(layout.close(pane as usize)))
                .map_err(|e| kept(&e))?;
            let event = moved.map(|moved| record(store, &opened, moved)).transpose()?;
            settle(store, &opened, &layout, None)?;
            Ok(reply(store, &opened, &layout, None, event))
        }
        PaneAsk::PaneResize { workspace, pane, ratio } => {
            let (opened, layout, moved) = workspaces
                .panes(&workspace, |layout| Ok(layout.resize(pane as usize, ratio)))
                .map_err(|e| kept(&e))?;
            let event = moved.map(|moved| record(store, &opened, moved)).transpose()?;
            settle(store, &opened, &layout, None)?;
            Ok(reply(store, &opened, &layout, None, event))
        }
        PaneAsk::PaneView { workspace, views, focus } => {
            let (opened, layout, ()) =
                workspaces.panes(&workspace, |_| Ok(())).map_err(|e| kept(&e))?;
            let canvases = layout.panes().into_iter().filter(|p| p.kind == Kind::Canvas);
            let shown: Vec<_> = canvases.map(|p| (p.id, p.target.clone())).collect();
            let ids = ids(&layout);
            let (_, ()) = workspaces
                .views()
                .change(&opened.dir, |kept| {
                    kept.keep(&ids);
                    for control::PaneViewed { id, view } in views {
                        let view = view::PaneView {
                            canvas: view.canvas,
                            page: view.page,
                            x: view.x,
                            y: view.y,
                            zoom: view.zoom,
                        };
                        view.check()?;
                        if shown.iter().any(|(at, canvas)| *at == id && *canvas == view.canvas) {
                            kept.set(id, view)?;
                        }
                    }
                    kept.focus = focus.filter(|f| ids.contains(f)).or(kept.focus);
                    Ok(())
                })
                .map_err(|e| kept(&e))?;
            Ok(reply(store, &opened, &layout, None, None))
        }
    }
}

/// Drops the kept views of the panes `layout` no longer holds and, with
/// `copy`, gives pane id `to` the view of pane id `from`.
fn settle(
    store: &Store,
    opened: &Opened,
    layout: &Layout,
    copy: Option<(u32, u32)>,
) -> Result<(), RemoteError> {
    let ids = ids(layout);
    let change = |views: &mut Views| {
        views.keep(&ids);
        if let Some((from, to)) = copy {
            views.copy(from, to);
        }
        Ok(())
    };
    store.workspaces().views().change(&opened.dir, change).map_err(|e| kept(&e))?;
    Ok(())
}

fn ids(layout: &Layout) -> Vec<u32> {
    layout.panes().iter().map(|p| p.id).collect()
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
    // A pane opened beside one on the same canvas starts where that one looks.
    let panes = layout.panes();
    let (source, new) = (beside.and_then(|n| panes.get(n - 1)), panes.get(number - 1));
    let copy = match (source, new) {
        (Some(s), Some(n))
            if s.kind == Kind::Canvas && n.kind == s.kind && n.target == s.target =>
        {
            Some((s.id, n.id))
        }
        _ => None,
    };
    settle(store, &opened, &layout, copy)?;
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

fn reply(
    store: &Store,
    opened: &Opened,
    layout: &Layout,
    pane: Option<u32>,
    event: Option<u64>,
) -> Reply {
    let views = store.workspaces().views().of(&opened.dir);
    let focus = views.focus.filter(|f| ids(layout).contains(f));
    let mut number = 0;
    let layout = wire(layout.root(), &mut number, &views);
    Reply::Pane(control::PaneLayout {
        workspace: opened.workspace.name.clone(),
        layout,
        pane,
        event,
        focus,
    })
}

/// The tree as it crosses the socket, each pane with its number, its id and
/// its kept view.
fn wire(node: &Node, number: &mut u32, views: &Views) -> control::PaneTree {
    match node {
        Node::Pane(pane) => {
            *number += 1;
            let kind = match pane.kind {
                Kind::Canvas => PaneKind::Canvas,
                Kind::Ui => PaneKind::Ui,
            };
            let view = (kind == PaneKind::Canvas).then(|| views.view_of(pane.id, &pane.target));
            let view = view.flatten().map(|v| control::PaneView {
                canvas: v.canvas.clone(),
                page: v.page.clone(),
                x: v.x,
                y: v.y,
                zoom: v.zoom,
            });
            control::PaneTree::Pane(control::Pane {
                pane: *number,
                id: pane.id,
                kind,
                target: pane.target.clone(),
                view,
            })
        }
        Node::Split { axis, ratio, first, second } => control::PaneTree::Split {
            split: match axis {
                Axis::Row => control::PaneSplit::Row,
                Axis::Column => control::PaneSplit::Column,
            },
            ratio: *ratio,
            first: Box::new(wire(first, number, views)),
            second: Box::new(wire(second, number, views)),
        },
    }
}
