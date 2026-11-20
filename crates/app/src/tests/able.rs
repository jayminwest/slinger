//! The one enabled predicate against the window (slinger-8f04db): each
//! command that `commands::able::can` refuses for what the window shows,
//! run as its menu item runs it, changes nothing; one it allows acts; and a
//! refused command's key answers as it did before the menu greyed it.

use super::step::{page, select_all};
use super::*;
use crate::commands::DELETE;
use crate::commands::able::{Facts, can};
use crate::layout::PaneId;
use crate::scene::canvas::ItemId;

/// What the window's facts are now ([`crate::window::facts`]).
fn facts(w: &World) -> Facts {
    let area = w.sidebar.panes(w.shared.size.rect());
    crate::window::facts(&w.shared, &w.canvases, &w.tools, &w.sidebar, area)
}

/// What a command could change: the items on the daemon, the panes and
/// the focus, the focused pane's pages, the selection and the step.
type Seen = (usize, usize, PaneId, Option<(Vec<String>, usize)>, Option<Vec<ItemId>>, bool);

fn seen(w: &World, daemon: &control::Client) -> Seen {
    let (panes, focus) = (&w.shared.panes, w.shared.panes.focus());
    (
        daemon.canvas_show("notes", None).unwrap().items.len(),
        panes.place(w.sidebar.panes(w.shared.size.rect())).len(),
        focus,
        w.canvases.page_names(panes, focus),
        w.canvases.selection().map(|s| s.ids.clone()),
        w.canvases.inside(panes, focus),
    )
}

/// Runs each command `can` refuses now as a menu item does (all but
/// `export-ui`, which opens a file panel first), and checks that none
/// changed anything. The commands it ran.
fn refused_change_nothing(w: &mut World, daemon: &control::Client) -> Vec<Command> {
    let now = facts(w);
    let refused = Command::ALL.iter().map(|&(c, _)| c);
    let refused: Vec<Command> =
        refused.filter(|&c| !can(c, &now) && c != Command::ExportUi).collect();
    for &command in &refused {
        let before = seen(w, daemon);
        w.send(Event::Command(command));
        w.send(Event::Tick);
        assert_eq!(seen(w, daemon), before, "{command:?} changed something");
    }
    refused
}

#[test]
fn on_an_empty_page_in_one_pane_what_is_refused_does_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let refused = refused_change_nothing(&mut w, daemon);
        for command in [
            Command::ClosePane,
            Command::FocusNext,
            Command::NextPage,
            Command::RemovePage,
            Command::DeleteSelection,
            Command::Duplicate,
            Command::Fold,
            Command::StepIn,
            Command::StepOut,
            Command::ClearSelection,
            Command::Sling,
            Command::CloseWorkspace,
            Command::StartAgent,
            Command::DismissNotice,
        ] {
            assert!(refused.contains(&command), "{command:?} is grey");
        }
        assert!(!refused.contains(&Command::SplitRight), "a split can act");
        assert!(!refused.contains(&Command::NewPage), "a new page can act");
    });
}

#[test]
fn with_items_selected_delete_acts_and_what_is_refused_does_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        select_all(&mut w, 3);
        let refused = refused_change_nothing(&mut w, daemon);
        assert!(refused.contains(&Command::StepOut) && refused.contains(&Command::ClosePane));
        let now = facts(&w);
        for command in [Command::DeleteSelection, Command::Fold, Command::StepIn, Command::Sling] {
            assert!(can(command, &now), "{command:?} can act on the selection");
        }
        assert!(w.send(Event::Command(Command::DeleteSelection)));
        assert!(w.canvases.selection().is_none());
        w.until(|_| count(daemon, "box") == 0);
        assert!(!can(Command::DeleteSelection, &facts(&w)), "nothing left to delete");
    });
}

#[test]
fn a_refused_command_s_key_answers_as_before() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let now = facts(&w);
        assert!(!can(Command::DeleteSelection, &now) && !can(Command::ClosePane, &now));
        let delete = Event::Key(Chord::new(0, DELETE));
        assert!(!w.send(delete), "nothing to delete: the key goes on, as before");
        assert!(w.send(Event::Key(Chord::new(CMD, 'w'))), "close-pane's key is still the app's");
        assert_eq!(w.shared.panes.place(w.sidebar.panes(w.shared.size.rect())).len(), 1);
    });
}
