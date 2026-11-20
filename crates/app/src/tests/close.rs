//! A workspace row's ✕ (slinger-9cd9d6): a tap closes that workspace in
//! the daemon, by its folder; the window keeps showing what it showed
//! unless that was the one closed; a failed close leaves the row.

use super::*;
use crate::layout::PaneId;
use crate::scene::taps::Tap;

/// Hands `event` to [`route`] and gives back what it routed to.
fn routed(w: &mut World, event: Event) -> Routed {
    route(
        event,
        &mut w.shared,
        &mut w.canvases,
        &mut w.tools,
        &w.settings,
        &w.socket,
        &mut w.sidebar,
    )
}

/// A press and a lift on the middle of the ✕ of the sidebar's workspace
/// `index`, as last drawn: what the lift routed to.
fn tap_close(w: &mut World, index: usize) -> Routed {
    view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
    let targets = w.sidebar.targets(&w.settings.config.theme, w.shared.size.rect());
    let frame = targets.iter().find(|t| t.1 == Tap::Close(index)).expect("a ✕").0;
    let (x, y) = (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0);
    routed(w, Event::Press(at(x, y, 1.0)));
    routed(w, Event::Lift(at(x, y, 1.1)))
}

/// The names of the workspaces `daemon` holds open.
fn open(daemon: &control::Client) -> Vec<String> {
    daemon.workspace_list().unwrap().into_iter().map(|w| w.name).collect()
}

#[test]
fn a_row_s_close_mark_closes_that_workspace_and_only_the_shown_one_changes_the_window() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        let ideas = tempfile::tempdir().unwrap();
        let file = "version = 1\nname = \"ideas\"\ncanvas = \"ideas\"\n";
        std::fs::write(ideas.path().join("slinger-workspace.toml"), file).unwrap();
        daemon.workspace_open(Some(ideas.path()), None).unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut w = world(&daemon, config.path());
        w.socket = socket.to_path_buf();
        w.sidebar = Sidebar::new(true);
        w.sidebar.update(backend::workspaces(&daemon));
        w.sidebar.show(Some("notes".into()));
        let notes_dir = w.sidebar.shown_dir().unwrap();
        w.shared.focused.push(("ideas".into(), PaneId::numbered(2)));

        let ideas_at = usize::from(w.sidebar.dir(0) == Some(notes_dir.clone()));
        let other = tap_close(&mut w, ideas_at);
        assert!(matches!(other, Routed::Answer(Step::Draw(_))), "no switch");
        assert_eq!(open(&daemon), ["notes"], "ideas closed in the daemon");
        assert_eq!(w.sidebar.shown_dir(), Some(notes_dir), "notes still shows");
        assert_eq!(w.sidebar.dir(1), None, "its row is gone");
        assert!(w.shared.focused.is_empty(), "nor is its focus kept");
        let said = flashed(&w.tools, &w.settings);
        assert!(said.is_some_and(|s| s.starts_with("Closed ideas")), "the flash says so");

        let shown = tap_close(&mut w, 0);
        assert_eq!(w.sidebar.shown_dir(), None, "none left to show");
        assert!(matches!(shown, Routed::Watch(None, Step::Draw(_))), "the watch follows the first");
        assert_eq!(open(&daemon), Vec::<String>::new());
    });
}

#[test]
fn a_close_with_no_daemon_leaves_the_row_and_the_flash_says_why() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(&daemon, config.path());
        w.sidebar = Sidebar::new(true);
        w.sidebar.update(backend::workspaces(&daemon));
        let dir = w.sidebar.dir(0);
        assert!(matches!(tap_close(&mut w, 0), Routed::Answer(_)), "nothing to follow");
        assert_eq!(open(&daemon), ["notes"], "the socket names no daemon: still open");
        assert_eq!(w.sidebar.dir(0), dir, "the row stays");
        let said = flashed(&w.tools, &w.settings).unwrap_or_default();
        assert!(!said.is_empty() && !said.starts_with("Closed"), "why: {said}");
    });
}
