//! `pane_view` through the real socket: the views and the focus it keeps
//! come back in every pane reply and after a restart, a close drops a gone
//! pane's view, an open beside a pane on the same canvas copies its view,
//! and nothing of it reaches the workspace's file or the record.

use slinger_control::{Client, EventFilter, PaneKind, PaneLayout, PaneView, PaneViewed};
use tempfile::TempDir;

use super::Daemon;

fn at(id: u32, canvas: &str, x: f64, zoom: f64) -> PaneViewed {
    let page = Some("p2".into());
    PaneViewed { id, view: PaneView { canvas: canvas.into(), page, x, y: -3.5, zoom } }
}

/// Each pane's id and the x of its view, if it has one.
fn seen(layout: &PaneLayout) -> Vec<(u32, Option<f64>)> {
    layout.layout.panes().iter().map(|p| (p.id, p.view.as_ref().map(|v| v.x))).collect()
}

fn events(client: &Client) -> usize {
    client.record_list(EventFilter::default()).unwrap().len()
}

#[test]
fn views_and_the_focus_come_back_in_every_reply_and_after_a_restart() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.workspace_open(None, Some("Notes")).unwrap();
    client.pane_open("Notes", PaneKind::Canvas, "notes", Some(1), false).unwrap();
    let file = client.workspace_show("Notes").unwrap().dir.join("slinger-workspace.toml");
    let (bytes, count) = (std::fs::read(&file).unwrap(), events(&client));

    let set = client.pane_view("Notes", &[at(1, "notes", 10.0, 0.5)], Some(2)).unwrap();
    assert_eq!(
        (seen(&set), set.focus, set.event),
        (vec![(1, Some(10.0)), (2, None)], Some(2), None)
    );
    assert_eq!(set.layout.panes()[0].view, Some(at(1, "notes", 10.0, 0.5).view));
    let more = client.pane_view("Notes", &[at(2, "notes", 20.0, 2.0)], None).unwrap();
    assert_eq!((seen(&more), more.focus), (vec![(1, Some(10.0)), (2, Some(20.0))], Some(2)));
    let listed = client.pane_list("Notes").unwrap();
    assert_eq!((seen(&listed), listed.focus), (seen(&more), Some(2)));
    let resized = client.pane_resize("Notes", 1, 0.3).unwrap();
    assert_eq!((seen(&resized), resized.focus), (seen(&more), Some(2)), "a resize keeps them");
    client.pane_resize("Notes", 1, 0.5).unwrap();

    let views = std::fs::read_to_string(data.path().join("workspaces/views.json")).unwrap();
    assert!(views.starts_with(r#"{"v":1,"workspaces":{"/"#), "{views}");
    assert_eq!(std::fs::read(&file).unwrap(), bytes, "the workspace file is not written");
    assert_eq!(events(&client), count + 2, "only the resizes appended events");
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    let again = daemon.client().pane_list("Notes").unwrap();
    assert_eq!((seen(&again), again.focus), (seen(&more), Some(2)));
    daemon.stop();
}

#[test]
fn a_close_drops_the_gone_pane_s_view_and_its_focus() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(None, Some("w")).unwrap();
    client.pane_open("w", PaneKind::Canvas, "w", Some(1), false).unwrap();
    let both = [at(1, "w", 1.0, 1.0), at(2, "w", 2.0, 1.0)];
    client.pane_view("w", &both, Some(2)).unwrap();
    let closed = client.pane_close("w", 2).unwrap();
    assert_eq!((seen(&closed), closed.focus), (vec![(1, Some(1.0))], None));
    let opened = client.pane_open("w", PaneKind::Canvas, "w", None, false).unwrap();
    assert_eq!(seen(&opened), [(1, Some(1.0)), (3, None)], "beside the window: no copy");
    daemon.stop();
}

#[test]
fn a_pane_opened_beside_one_on_the_same_canvas_starts_where_it_looks() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(None, Some("w")).unwrap();
    client.canvas_create("board").unwrap();
    client.pane_view("w", &[at(1, "w", 5.0, 3.0)], None).unwrap();
    let same = client.pane_open("w", PaneKind::Canvas, "w", Some(1), true).unwrap();
    assert_eq!(seen(&same), [(1, Some(5.0)), (2, Some(5.0))]);
    let other = client.pane_open("w", PaneKind::Canvas, "board", Some(1), false).unwrap();
    assert_eq!(seen(&other), [(1, Some(5.0)), (3, None), (2, Some(5.0))]);
    daemon.stop();
}

#[test]
fn a_view_for_no_such_pane_or_another_canvas_is_left_out_and_a_bad_zoom_is_invalid() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(None, Some("a")).unwrap();
    client.workspace_open(None, Some("b")).unwrap();
    let left_out = [at(9, "a", 1.0, 1.0), at(1, "board", 1.0, 1.0)];
    let reply = client.pane_view("a", &left_out, Some(9)).unwrap();
    assert_eq!((seen(&reply), reply.focus), (vec![(1, None)], None));

    client.pane_view("a", &[at(1, "a", 4.0, 1.0)], Some(1)).unwrap();
    for zoom in [0.0, -2.0] {
        let bad = [at(1, "a", 7.0, 1.0), at(1, "a", 8.0, zoom)];
        let err = client.pane_view("a", &bad, None).unwrap_err();
        assert_eq!(err.code(), "invalid", "{zoom}");
        assert!(err.hint().is_some());
    }
    let kept = client.pane_list("a").unwrap();
    assert_eq!((seen(&kept), kept.focus), (vec![(1, Some(4.0))], Some(1)), "nothing was kept");
    let other = client.pane_list("b").unwrap();
    assert_eq!((seen(&other), other.focus), (vec![(1, None)], None), "b shares no views");
    let code = client.pane_view("nope", &[], None).unwrap_err().code().to_string();
    assert_eq!(code, "not_found");

    client.workspace_close("a").unwrap();
    client.workspace_open(None, Some("a")).unwrap();
    let back = client.pane_list("a").unwrap();
    assert_eq!((seen(&back), back.focus), (vec![(1, Some(4.0))], Some(1)), "a reopen restores");
    daemon.stop();
}
