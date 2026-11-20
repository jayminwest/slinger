//! A context click on a sidebar workspace row or a divider
//! (slinger-7e44c2): the menu is for that row's workspace or that divider,
//! and its pick carries it (`Event::Picked`), so it acts there without a
//! switch; a menu closed with no pick leaves nothing behind.

use std::path::{Path, PathBuf};

use super::*;
use crate::commands::context::{Popup, Subject};
use crate::scene::secondary::{Button, Phase};
use crate::scene::taps::Tap;

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

/// A right press and lift at `at`: the menu the window would pop.
fn right(w: &mut World, at: Point) -> Popup {
    let button = Button::Secondary;
    routed(w, Event::Button { button, phase: Phase::Down, at });
    match routed(w, Event::Button { button, phase: Phase::Up, at }) {
        Routed::Answer(Step::Menu(popup, _)) => popup,
        _ => panic!("no menu at {at:?}"),
    }
}

/// The middle of the name of the sidebar's workspace in `dir`.
fn row(w: &World, dir: &PathBuf) -> Point {
    let index = (0..9).find(|&i| w.sidebar.dir(i).as_ref() == Some(dir)).unwrap();
    let targets = w.sidebar.targets(&w.settings.config.theme, w.shared.size.rect());
    let frame = targets.iter().find(|t| t.1 == Tap::Workspace(index)).unwrap().0;
    Point { x: frame.x + 40.0, y: frame.y + 11.0 }
}

/// A context click on the row of the workspace in `dir`.
fn on_row(w: &mut World, dir: &PathBuf) -> Popup {
    let at = row(w, dir);
    right(w, at)
}

fn enabled(popup: &Popup, title: &str) -> bool {
    popup.items.iter().find(|i| i.title == title).unwrap().enabled
}

fn open(daemon: &control::Client) -> Vec<String> {
    let mut names: Vec<_> = daemon.workspace_list().unwrap().into_iter().map(|w| w.name).collect();
    names.sort();
    names
}

/// Opens a workspace named `name` with no agent, in a fresh directory.
fn open_bare(daemon: &control::Client, name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let file = format!("version = 1\nname = \"{name}\"\ncanvas = \"{name}\"\n");
    std::fs::write(dir.path().join("slinger-workspace.toml"), file).unwrap();
    daemon.workspace_open(Some(dir.path()), None).unwrap();
    dir
}

/// A window onto notes (agent scout), ideas and plans (no agent), the
/// sidebar open and ideas shown, its socket `socket`.
fn three(daemon: &control::Client, socket: &Path, config: &Path) -> World {
    let mut w = world(daemon, config);
    w.socket = socket.to_path_buf();
    w.sidebar = Sidebar::new(true);
    w.sidebar.update(backend::workspaces(daemon));
    w.sidebar.show(Some("ideas".into()));
    w
}

/// The folder of the open workspace `name`.
fn dir(daemon: &control::Client, name: &str) -> PathBuf {
    let list = backend::workspaces(daemon).unwrap();
    list.into_iter().find(|ws| ws.name == name).unwrap().dir
}

#[test]
fn a_row_s_agent_items_act_on_its_agent_and_grey_by_it_with_no_switch() {
    with_agents(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        let (_ideas, _plans) = (open_bare(&daemon, "ideas"), open_bare(&daemon, "plans"));
        let config = tempfile::tempdir().unwrap();
        let mut w = three(&daemon, socket, config.path());
        let (shown, notes) = (dir(&daemon, "ideas"), dir(&daemon, "notes"));

        let popup = on_row(&mut w, &notes);
        assert_eq!(popup.subject, Subject::Workspace { dir: notes.clone() });
        assert_eq!(w.sidebar.shown_dir(), Some(shown.clone()), "no switch");
        assert!(enabled(&popup, "Start agent") && !enabled(&popup, "Stop agent"), "scout stopped");
        let ideas = on_row(&mut w, &shown);
        let restart = "Restart agent in a new conversation";
        assert!(!enabled(&ideas, "Start agent") && !enabled(&ideas, restart), "no agent");

        let picked = |c| Event::Picked(c, popup.subject.clone());
        assert!(w.send(picked(Command::StartAgent)));
        assert_eq!(flashed(&w.tools, &w.settings).as_deref(), Some("Start agent · scout"));
        w.sidebar.update(backend::workspaces(&daemon));
        let running = on_row(&mut w, &notes);
        assert!(!enabled(&running, "Start agent") && enabled(&running, "Stop agent"), "it runs");
        assert!(w.send(picked(Command::StopAgent)));
        assert_eq!(flashed(&w.tools, &w.settings).as_deref(), Some("Stop agent · scout"));
        assert_eq!(w.sidebar.shown_dir(), Some(shown), "still no switch");
        let folder = Command::ShowWorkspaceFolder;
        let found = window::finder(folder, &w.shared, &w.canvases, Some(notes.clone()));
        assert_eq!(found.map(PathBuf::from), Ok(notes), "the row's folder");
    });
}

#[test]
fn a_row_s_close_closes_its_workspace_and_a_menu_closed_unpicked_leaves_nothing() {
    with_socket(|socket| {
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        let (_ideas, _plans) = (open_bare(&daemon, "ideas"), open_bare(&daemon, "plans"));
        let config = tempfile::tempdir().unwrap();
        let mut w = three(&daemon, socket, config.path());
        let (shown, notes, plans) =
            (dir(&daemon, "ideas"), dir(&daemon, "notes"), dir(&daemon, "plans"));

        let popup = on_row(&mut w, &notes);
        let closed = routed(&mut w, Event::Picked(Command::CloseWorkspace, popup.subject));
        assert!(matches!(closed, Routed::Answer(Step::Draw(_))), "no watch: the shown one stays");
        assert_eq!(open(&daemon), ["ideas", "plans"], "notes closed in the daemon");
        assert_eq!(w.sidebar.shown_dir(), Some(shown), "ideas still shows");

        let index = (0..3).find(|&i| w.sidebar.dir(i).as_ref() == Some(&plans)).unwrap();
        let targets = w.sidebar.targets(&w.settings.config.theme, w.shared.size.rect());
        let close = targets.iter().find(|t| t.1 == Tap::Close(index)).unwrap().0;
        let mark = right(&mut w, Point { x: close.x + close.width / 2.0, y: close.y + 11.0 });
        assert_eq!(mark.subject, Subject::Workspace { dir: plans }, "its ✕ opens its menu too");
        let shut = routed(&mut w, Event::Command(Command::CloseWorkspace));
        assert!(matches!(shut, Routed::Watch(None, _)), "the menu bar's close: the shown one");
        assert_eq!(open(&daemon), ["plans"], "not the row clicked before");
    });
}

#[test]
fn the_rest_of_the_sidebar_offers_open_and_hide_and_a_divider_its_grow_and_shrink() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.sidebar = Sidebar::new(true);
        let empty = right(&mut w, Point { x: 100.0, y: 400.0 });
        assert_eq!(empty.subject, Subject::Sidebar);
        assert_eq!(super::context::titles(&empty), ["Open workspace…", "Show or hide sidebar"]);

        assert!(w.send(Event::Command(Command::SplitRight)));
        assert!(w.send(Event::Command(Command::SplitDown)));
        let area = w.sidebar.panes(w.shared.size.rect());
        let placed = w.shared.panes.place(area);
        let (first, lower) = (placed[0].2, placed[2].2);
        let outer = Point { x: first.x + first.width, y: first.y + 100.0 };
        let inner = Point { x: lower.x + 100.0, y: lower.y };
        let divider = |w: &mut World, at| match right(w, at).subject {
            Subject::Divider { before, ratio } => (before, ratio),
            other => panic!("{other:?}"),
        };
        let ((before, ratio), (_, inside)) = (divider(&mut w, outer), divider(&mut w, inner));
        let popup = right(&mut w, outer);
        assert_eq!(super::context::titles(&popup), ["Grow pane", "Shrink pane"]);

        assert!(w.send(Event::Picked(Command::GrowPane, popup.subject.clone())));
        let moved = Point { x: outer.x + area.width * 0.05, ..outer };
        let (now, grown) = divider(&mut w, moved);
        assert_eq!(now, before, "the same divider");
        assert!((grown - ratio - 0.05).abs() < 1e-9, "one step: {ratio} to {grown}");
        let inner = Point { x: inner.x + area.width * 0.05, ..inner };
        assert!((divider(&mut w, inner).1 - inside).abs() < 1e-9, "no other divider moved");
        let shrink = right(&mut w, moved).subject;
        assert!(w.send(Event::Picked(Command::ShrinkPane, shrink)));
        assert!((divider(&mut w, outer).1 - ratio).abs() < 1e-9, "and back");
    });
}
