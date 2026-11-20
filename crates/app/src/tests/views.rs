//! Views that come back (slinger-691123): where a pane looks goes to the
//! daemon, and a window that has no view of a pane in memory (a relaunch)
//! opens it there, with no fit; focus comes back to the pane that had it.

use super::step::{page, settle};
use super::*;
use crate::layout::{PaneId, Viewport};

const ZOOM_IN: Chord = Chord::new(CMD, '=');
const PAN_LEFT: Chord = Chord::new(crate::commands::ALT, crate::commands::LEFT);

/// A window onto `socket`'s workspace `notes`, following its layout.
fn following(daemon: &control::Client, socket: &Path, config: &Path) -> World {
    let mut w = world(daemon, config);
    w.socket = socket.to_path_buf();
    w.shared.workspace = Some("notes".into());
    w.shared.follow(Ok(daemon.pane_list("notes").unwrap()));
    w
}

/// Pane `pane`'s viewport and the page it shows, once it draws.
fn looking(w: &mut World, pane: PaneId) -> (Viewport, usize) {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let shown = w.canvases.show(&w.shared.panes, area, &theme);
    let viewport = shown.iter().find(|p| p.pane == pane).expect("the pane draws").scene.viewport;
    (viewport, w.canvases.page_names(&w.shared.panes, pane).map_or(0, |(_, at)| at))
}

#[test]
fn a_pane_s_page_pan_and_zoom_go_to_the_daemon_and_a_new_window_opens_there() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _home = open_notes(&daemon);
        page(&daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = following(&daemon, socket, config.path());
        settle(&mut w, |d| d.contains(&3));
        assert!(w.send(Event::Command(Command::NewPage)));
        let turned = |w: &World| {
            let names = w.canvases.page_names(&w.shared.panes, w.shared.panes.focus());
            names.is_some_and(|(_, at)| at == 1)
        };
        w.until(turned);
        w.send(Event::Key(ZOOM_IN));
        w.send(Event::Key(PAN_LEFT));
        let pane = w.shared.panes.focus();
        let (left, at) = looking(&mut w, pane);
        assert_eq!((left.percent(), at), (125, 1));
        w.send(Event::Quitting);
        let kept = daemon.pane_list("notes").unwrap();
        let view = kept.layout.panes()[0].view.clone().expect("kept at quit");
        assert_eq!((view.canvas.as_str(), view.zoom), ("notes", left.zoom()));
        assert!(view.page.as_deref().is_some_and(|p| p != "p1"), "the new page: {view:?}");
        assert_eq!(kept.focus, Some(pane.number()));

        // A relaunch: a window with no view in memory.
        let mut again = following(&daemon, socket, config.path());
        again
            .until(|w| w.canvases.page_names(&w.shared.panes, pane).is_some_and(|(_, at)| at == 1));
        assert_eq!(looking(&mut again, pane), (left, 1), "where it was left, not fitted");
    });
}

#[test]
fn a_view_in_memory_wins_over_a_later_layout_s_kept_view() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _home = open_notes(&daemon);
        page(&daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = following(&daemon, socket, config.path());
        settle(&mut w, |d| d.contains(&3));
        w.send(Event::Key(ZOOM_IN));
        let pane = w.shared.panes.focus();
        let mine = looking(&mut w, pane).0;
        let elsewhere =
            control::PaneView { canvas: "notes".into(), page: None, x: 900.0, y: 900.0, zoom: 3.0 };
        let view = control::PaneViewed { id: pane.number(), view: elsewhere };
        let layout = daemon.pane_view("notes", &[view], None).unwrap();
        w.shared.follow(Ok(layout));
        assert_eq!(looking(&mut w, pane).0, mine);
    });
}

#[test]
fn focus_comes_back_to_the_pane_the_daemon_kept_it_on() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _home = open_notes(&daemon);
        daemon.pane_open("notes", control::PaneKind::Canvas, "notes", None, false).unwrap();
        let ids: Vec<u32> =
            daemon.pane_list("notes").unwrap().layout.panes().iter().map(|p| p.id).collect();
        daemon.pane_view("notes", &[], Some(ids[1])).unwrap();
        let config = tempfile::tempdir().unwrap();
        let w = following(&daemon, socket, config.path());
        assert_eq!(w.shared.panes.focus(), PaneId::numbered(ids[1]));
    });
}
