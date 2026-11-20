//! Closing the shown workspace (slinger-c0f0fa): its views and its focus
//! stay while the window still shows it, go once it has left it, and the
//! workspace opened again comes back as the daemon kept it, not as this run
//! left it.

use super::*;
use crate::layout::{PaneId, Viewport};

/// Pane `pane`'s viewport, once it draws.
fn viewport(w: &mut World, pane: PaneId) -> Option<Viewport> {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let shown = w.canvases.show(&w.shared.panes, area, &theme);
    shown.iter().find(|p| p.pane == pane).map(|p| p.scene.viewport)
}

/// Shows `workspace`'s layout as the watch would bring it, and draws.
fn follow(w: &mut World, daemon: &control::Client, workspace: &str) {
    w.shared.follow(Ok(daemon.pane_list(workspace).unwrap()));
    view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
}

#[test]
fn the_shown_workspace_closed_and_opened_again_comes_back_as_the_daemon_kept_it() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        daemon.pane_open("notes", control::PaneKind::Canvas, "notes", None, false).unwrap();
        let ideas = tempfile::tempdir().unwrap();
        let file = "version = 1\nname = \"ideas\"\ncanvas = \"ideas\"\n";
        std::fs::write(ideas.path().join("slinger-workspace.toml"), file).unwrap();
        daemon.workspace_open(Some(ideas.path()), None).unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut w = world(&daemon, config.path());
        w.socket = socket.to_path_buf();
        w.sidebar.update(backend::workspaces(&daemon));
        w.sidebar.show(Some("notes".into()));
        follow(&mut w, &daemon, "notes");
        let ids: Vec<u32> =
            daemon.pane_list("notes").unwrap().layout.panes().iter().map(|p| p.id).collect();
        let (first, second) = (PaneId::numbered(ids[0]), PaneId::numbered(ids[1]));
        assert!(w.shared.panes.focus_on(second));
        w.until(|w| w.canvases.page_names(&w.shared.panes, second).is_some());
        w.send(Event::Key(Chord::new(CMD, '=')));
        let zoomed = viewport(&mut w, second).expect("it draws");
        let notes_dir = w.sidebar.shown_dir().unwrap();

        let closed = w.send(Event::Command(Command::CloseWorkspace));
        let open: Vec<String> =
            daemon.workspace_list().unwrap().into_iter().map(|w| w.name).collect();
        assert!(closed && open == ["ideas"], "notes closed in the daemon");
        assert_eq!(viewport(&mut w, second), Some(zoomed), "still shown: its view stays");
        follow(&mut w, &daemon, "ideas");
        assert!(w.shared.focused.iter().all(|(n, _)| n != "notes"), "no focus kept for it");

        daemon.workspace_open(Some(&notes_dir), None).unwrap();
        w.sidebar.update(backend::workspaces(&daemon));
        let left = daemon.pane_list("notes").unwrap();
        let view = left.layout.panes()[1].view.clone().expect("its last looks were kept");
        assert_eq!((view.zoom, left.focus), (zoomed.zoom(), Some(ids[1])), "sent before the close");
        let start = Viewport::start();
        let kept =
            control::PaneView { canvas: "notes".into(), page: None, x: 0.0, y: 0.0, zoom: 1.0 };
        let views = [control::PaneViewed { id: ids[1], view: kept }];
        daemon.pane_view("notes", &views, Some(ids[0])).unwrap();
        follow(&mut w, &daemon, "notes");
        assert_eq!(w.shared.panes.focus(), first, "focus where the daemon kept it");
        w.until(|w| w.canvases.page_names(&w.shared.panes, second).is_some());
        let again = viewport(&mut w, second).expect("it draws");
        assert_eq!(again.zoom(), start.zoom(), "the daemon's view, not the one left");
    });
}

#[test]
fn closing_the_last_workspace_goes_back_to_the_window_s_own_panes() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(&daemon, config.path());
        w.socket = socket.to_path_buf();
        w.sidebar.update(backend::workspaces(&daemon));
        follow(&mut w, &daemon, "notes");
        assert!(w.send(Event::Command(Command::CloseWorkspace)));
        assert_eq!(w.shared.panes.workspace(), Some("notes"), "until the watch says");
        w.shared.follow(backend::panes::layout(&daemon, None, None));
        assert_eq!(w.shared.panes.workspace(), None, "none left: the window's own");
        assert!(w.shared.trouble.is_some(), "it says why");
        w.shared.follow(backend::panes::layout(&daemon, None, None));
        assert_eq!(w.shared.panes.workspace(), None);
    });
}
