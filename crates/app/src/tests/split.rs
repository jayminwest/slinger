//! A split copies the source pane's view (slinger-64b4d4): the new pane
//! shows the same page, at the same pan and zoom, inside the same boxes;
//! after that each pane moves on its own. `step-in-beside` then escape
//! lands on the source's view.

use super::step::{ROOM, STEP_IN, page, select_all, settle};
use super::*;
use crate::commands::{ALT, SHIFT};
use crate::layout::{PaneId, Viewport};

const BESIDE: Chord = Chord::new(CMD | ALT | SHIFT, 'i');
const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const ZOOM_IN: Chord = Chord::new(CMD, '=');
const PAN_LEFT: Chord = Chord::new(ALT, crate::commands::LEFT);

/// Each canvas pane's viewport and the ids it draws, once shown.
fn shown(w: &mut World) -> Vec<(PaneId, Viewport, Vec<u64>)> {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let panes = w.canvases.show(&w.shared.panes, area, &theme);
    let ids = |p: &crate::scene::CanvasPane| p.scene.draws.iter().map(|d| d.id.0).collect();
    panes.iter().map(|p| (p.pane, p.scene.viewport, ids(p))).collect()
}

fn of(w: &mut World, pane: PaneId) -> (Viewport, Vec<u64>) {
    let found = shown(w).into_iter().find(|(p, ..)| *p == pane);
    found.map(|(_, v, ids)| (v, ids)).expect("the pane draws")
}

/// The split's checks on `w`, whose focused pane shows `page`'s items.
fn split_copies(w: &mut World) {
    settle(w, |d| d.contains(&3));
    select_all(w, 3);
    assert!(w.send(Event::Key(STEP_IN)));
    w.send(Event::Key(ZOOM_IN));
    w.send(Event::Key(ZOOM_IN));
    let source = w.shared.panes.focus();
    let (place, inside) = of(w, source);
    assert!(inside.contains(&ROOM) && !inside.contains(&3), "stepped in: {inside:?}");
    assert_eq!(place.percent(), 156);

    assert!(w.send(Event::Command(Command::SplitDown)));
    let new = w.shared.panes.focus();
    assert_ne!(new, source, "the new pane has focus");
    let (copied, ids) = of(w, new);
    assert_eq!(copied, place, "the same pan and zoom");
    assert!(ids.contains(&ROOM) && !ids.contains(&3), "inside the same box: {ids:?}");
    assert_eq!(of(w, source), (place, inside.clone()), "the source is as it was");

    w.send(Event::Key(PAN_LEFT));
    assert_ne!(of(w, new).0, place, "the new pane pans");
    assert_eq!(of(w, source).0, place, "the source does not");
    assert!(w.send(Event::Key(ESCAPE)), "the new pane steps out");
    assert!(of(w, new).1.contains(&3));
    assert!(of(w, source).1.contains(&ROOM), "the source stays in");
}

#[test]
fn a_split_of_the_window_s_own_panes_copies_the_view_which_then_moves_on_its_own() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert_eq!(w.shared.workspace, None, "no workspace: the window's own panes");
        split_copies(&mut w);
    });
}

#[test]
fn a_split_through_the_daemon_copies_the_view_into_the_pane_it_opened() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _home = open_notes(&daemon);
        page(&daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(&daemon, config.path());
        w.socket = socket.to_path_buf();
        w.shared.workspace = Some("notes".into());
        w.shared.follow(Ok(daemon.pane_list("notes").unwrap()));
        assert_eq!(w.shared.panes.workspace(), Some("notes"));
        split_copies(&mut w);
        assert_eq!(daemon.pane_list("notes").unwrap().layout.panes().len(), 2, "the daemon's");
    });
}

#[test]
fn step_in_beside_then_escape_shows_the_source_s_place() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        settle(&mut w, |d| d.contains(&3));
        w.send(Event::Key(ZOOM_IN));
        w.send(Event::Key(PAN_LEFT));
        let source = w.shared.panes.focus();
        let place = of(&mut w, source).0;
        select_all(&mut w, 3);
        assert!(w.send(Event::Key(BESIDE)));
        let new = w.shared.panes.focus();
        assert!(of(&mut w, new).1.contains(&ROOM), "the new pane is in the box");
        assert!(w.send(Event::Key(ESCAPE)));
        assert_eq!(of(&mut w, new).0, place, "out of it, at the source's place");
    });
}
